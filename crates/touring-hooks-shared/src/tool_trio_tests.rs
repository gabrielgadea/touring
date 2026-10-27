//! Tests of [`super`] (`tool_trio`): the 9 parity fixtures of
//! `scripts/test_measure_tool_trio.py`, the shared replay fixture, the deny -> next
//! action classifier, the bounds and the (ignored) overhead measurement.

use super::*;
use serde_json::json;

// ── Transcript replay (the offline script's `measure_session`) ───────────

fn replay(records: &[Value]) -> TrioLedger {
    let mut ledger = TrioLedger::default();
    let mut pending: HashMap<String, (String, u64)> = HashMap::new();
    for rec in records {
        let Some(content) = rec.pointer("/message/content").and_then(Value::as_array) else {
            continue;
        };
        let kind = rec.get("type").and_then(Value::as_str).unwrap_or("");
        for c in content {
            match (kind, c.get("type").and_then(Value::as_str)) {
                ("assistant", Some("tool_use")) => {
                    let id = c.get("id").and_then(Value::as_str).unwrap_or("");
                    let name = c.get("name").and_then(Value::as_str).unwrap_or("");
                    let input = c.get("input").unwrap_or(&NULL);
                    pending.insert(
                        id.to_string(),
                        (tool_class(name, input), call_signature(name, input)),
                    );
                }
                ("user", Some("tool_result")) => {
                    let id = c.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
                    let Some((class, sig)) = pending.remove(id) else {
                        continue;
                    };
                    let text = match c.get("content") {
                        Some(Value::String(s)) => s.clone(),
                        Some(Value::Array(blocks)) => blocks_text(blocks),
                        _ => String::new(),
                    };
                    let is_error = c.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                    let result = outcome(&class, &text, 0, is_error);
                    ledger.on_completed("s", class, sig, None, result, text.len());
                }
                _ => {}
            }
        }
    }
    ledger
}

fn use_(i: u32, name: &str, input: Value) -> Value {
    json!({"type": "assistant", "message": {"content": [
        {"type": "tool_use", "id": format!("t{i}"), "name": name, "input": input}]}})
}

fn res(i: u32, content: Value, err: bool) -> Value {
    json!({"type": "user", "message": {"content": [
        {"type": "tool_result", "tool_use_id": format!("t{i}"), "content": content, "is_error": err}]}})
}

fn stat<'a>(l: &'a TrioLedger, class: &str) -> &'a ClassStats {
    l.classes
        .get(class)
        .unwrap_or_else(|| panic!("class {class} not recorded"))
}

// ── The 9 parity fixtures of scripts/test_measure_tool_trio.py ───────────

#[test]
fn test_cd_prefix_is_not_the_verb() {
    assert_eq!(
        classify_bash("cd /home/x/touring && cargo test -p a"),
        "cargo test"
    );
    assert_eq!(
        classify_bash("cd /x; export A=1; timeout 30 python3 s.py"),
        "python3"
    );
    assert_eq!(
        classify_bash("TOURING_GATE_OK=1 touring run --lang bash"),
        "touring run"
    );
}

#[test]
fn test_echo_header_is_not_the_verb_but_a_lone_echo_is() {
    assert_eq!(
        classify_bash("echo \"### step 1\"\ngit status --short"),
        "git status"
    );
    assert_eq!(classify_bash("echo \"$PATH\""), "echo");
}

#[test]
fn test_non_text_result_blocks_are_not_empty() {
    let blocks =
        json!([{"type": "tool_reference", "tool_name": "mcp__x__some_long_tool_name_here"}]);
    let l = replay(&[
        use_(1, "ToolSearch", json!({"query": "select:x"})),
        res(1, blocks, false),
    ]);
    assert_eq!(stat(&l, "ToolSearch").ok, 1);
}

#[test]
fn test_heredoc_cat_is_a_write_and_its_empty_output_is_success() {
    assert_eq!(classify_bash("cat > f.txt <<'EOF'\nx\nEOF"), "cat-write");
    assert_eq!(classify_bash("cat src/a.rs"), "cat-read");
    let l = replay(&[
        use_(1, "Bash", json!({"command": "cat > f <<EOF\nx\nEOF"})),
        res(1, json!(""), false),
    ]);
    assert_eq!(stat(&l, "Bash:cat-write").ok, 1);
}

#[test]
fn test_two_different_edits_of_one_file_are_not_a_retry() {
    let ok = "ok ".repeat(30);
    let l = replay(&[
        use_(
            1,
            "Edit",
            json!({"file_path": "a.rs", "old_string": "x", "new_string": "y"}),
        ),
        res(1, json!(ok), false),
        use_(
            2,
            "Edit",
            json!({"file_path": "a.rs", "old_string": "p", "new_string": "q"}),
        ),
        res(2, json!(ok), false),
    ]);
    assert_eq!(stat(&l, "Edit").retry, 0);
}

#[test]
fn test_byte_identical_call_is_a_retry() {
    let input = json!({"file_path": "a.rs", "old_string": "x", "new_string": "y"});
    let l = replay(&[
        use_(1, "Edit", input.clone()),
        res(1, json!("String not found"), true),
        use_(2, "Edit", input),
        res(2, json!("String not found"), true),
    ]);
    let edit = stat(&l, "Edit");
    assert_eq!((edit.retry, edit.erro), (1, 2));
}

#[test]
fn test_grep_no_match_is_empty_not_error() {
    let l = replay(&[
        use_(1, "Bash", json!({"command": "grep -n foo a.rs"})),
        res(1, json!("Exit code 1"), true),
    ]);
    let grep = stat(&l, "Bash:grep");
    assert_eq!((grep.vazio, grep.erro), (1, 0));
}

#[test]
fn test_hook_denial_is_not_a_tool_failure() {
    let l = replay(&[
        use_(1, "Bash", json!({"command": "ls"})),
        res(1, json!("PreToolUse:Bash hook error: [G11] ..."), true),
    ]);
    let ls = stat(&l, "Bash:ls");
    assert_eq!((ls.negado, ls.erro), (1, 0));
}

#[test]
fn test_programs_with_different_code_are_not_a_retry() {
    let tool = "mcp__touring__touring_ctx_execute";
    let l = replay(&[
        use_(1, tool, json!({"language": "python", "code": "print(1)"})),
        res(1, json!("1".repeat(60)), false),
        use_(2, tool, json!({"language": "python", "code": "print(2)"})),
        res(2, json!("2".repeat(60)), false),
    ]);
    assert_eq!(stat(&l, tool).retry, 0);
}

// ── Shared fixture: the Python script and this port agree on one expected file ──

const PARITY_FIXTURE: &str = include_str!("../../../scripts/fixtures/tool_trio_parity.jsonl");
const PARITY_EXPECTED: &str =
    include_str!("../../../scripts/fixtures/tool_trio_parity.expected.json");

#[test]
fn test_gate_metrics_trio_matches_measure_tool_trio_fixture() {
    let records: Vec<Value> = PARITY_FIXTURE
        .lines()
        .map(|line| serde_json::from_str(line).expect("fixture line is JSON"))
        .collect();
    let expected: Value = serde_json::from_str(PARITY_EXPECTED).expect("expected is JSON");
    let expected = expected.as_object().expect("expected is an object");
    let ledger = replay(&records);
    assert_eq!(
        ledger.classes.len(),
        expected.len(),
        "class sets differ: rust {:?} vs python {:?}",
        {
            let mut k: Vec<_> = ledger.classes.keys().collect();
            k.sort();
            k
        },
        expected.keys().collect::<Vec<_>>()
    );
    for (class, want) in expected {
        let got = ledger
            .classes
            .get(class)
            .unwrap_or_else(|| panic!("python measured class {class:?}, rust did not"));
        let field = |k: &str| want.get(k).and_then(Value::as_u64).unwrap_or(u64::MAX);
        assert_eq!(got.calls, field("calls"), "{class}: calls");
        assert_eq!(got.retry, field("retry"), "{class}: retry");
        assert_eq!(got.erro, field("erro"), "{class}: erro");
        assert_eq!(got.vazio, field("vazio"), "{class}: vazio");
        assert_eq!(got.negado, field("negado"), "{class}: negado");
        assert_eq!(got.ok, field("ok"), "{class}: ok");
        assert_eq!(
            got.result_bytes,
            field("result_bytes"),
            "{class}: result_bytes"
        );
    }
}

// ── Predicate edges beyond the 9 fixtures ────────────────────────────────

#[test]
fn classifier_edges_match_the_python_table() {
    assert_eq!(classify_bash("cd /tmp"), "(prefixo-só)");
    assert_eq!(classify_bash(""), "(prefixo-só)");
    assert_eq!(classify_bash("# only a comment\nls -la"), "ls");
    // Quirk kept for parity: the Python table peels `nice` but not its `-n 5` option.
    assert_eq!(classify_bash("env FOO=1 nice -n 5 cargo test"), "-n");
    assert_eq!(classify_bash("timeout -k 5 30s git status"), "git status");
    assert_eq!(classify_bash("git -C x status"), "git");
    assert_eq!(classify_bash("/usr/bin/git log"), "git log");
    assert_eq!(
        classify_bash("scripts/hw-slot --class rust -- cargo test"),
        "scripts/hw-slot"
    );
    // Quirk kept for parity: `cat` through an absolute path never matches `^cat`.
    assert_eq!(classify_bash("/bin/cat > f"), "cat-read");
    assert_eq!(classify_bash("cat a.txt | head -3"), "cat-read");
    assert_eq!(classify_bash("cat a.txt 2>&1 | head"), "cat-write");
    assert_eq!(classify_bash("a || b && c"), "a");
}

#[test]
fn outcome_edges_match_the_python_table() {
    assert_eq!(outcome("Read", "  [] ", 0, false), Outcome::Vazio);
    assert_eq!(outcome("Read", "(No content)", 0, false), Outcome::Vazio);
    assert_eq!(outcome("Read", &"x".repeat(49), 0, false), Outcome::Vazio);
    assert_eq!(outcome("Read", &"x".repeat(50), 0, false), Outcome::Ok);
    assert_eq!(outcome("Bash:rg", "", 0, true), Outcome::Vazio);
    assert_eq!(outcome("Bash:rg", "Exit code 1", 0, true), Outcome::Vazio);
    assert_eq!(
        outcome("Bash:rg", "Exit code 2\nbad regex", 0, true),
        Outcome::Erro
    );
    assert_eq!(
        outcome("Bash:git status", "Exit code 1", 0, true),
        Outcome::Erro
    );
    assert_eq!(
        outcome("Bash:ls", "blocked by the foo hook", 0, true),
        Outcome::Negado
    );
    assert_eq!(
        outcome("Bash:ls", "blocked by the foo hook", 0, false),
        Outcome::Vazio
    );
    assert_eq!(
        outcome("Read", "", 70, false),
        Outcome::Ok,
        "extra bytes count"
    );
}

#[test]
fn retry_window_forgets_after_eight_calls() {
    let mut records = vec![
        use_(0, "Read", json!({"file_path": "a"})),
        res(0, json!("x".repeat(60)), false),
    ];
    for i in 1..=8 {
        records.push(use_(i, "Read", json!({"file_path": format!("f{i}")})));
        records.push(res(i, json!("x".repeat(60)), false));
    }
    records.push(use_(9, "Read", json!({"file_path": "a"})));
    records.push(res(9, json!("x".repeat(60)), false));
    assert_eq!(
        stat(&replay(&records), "Read").retry,
        0,
        "8 other calls pushed `a` out"
    );
}

#[test]
fn signature_ignores_key_order_and_sees_values() {
    let a = json!({"x": 1, "y": {"b": 2, "a": [1, 2]}});
    let b: Value = serde_json::from_str(r#"{"y":{"a":[1,2],"b":2},"x":1}"#).expect("valid json");
    assert_eq!(call_signature("Edit", &a), call_signature("Edit", &b));
    assert_ne!(call_signature("Edit", &a), call_signature("Write", &a));
    assert_ne!(
        call_signature("Edit", &a),
        call_signature("Edit", &json!({"x": 2}))
    );
}

// ── Live payload mapping ─────────────────────────────────────────────────

#[test]
fn live_response_views_follow_what_the_model_sees() {
    let bash = json!({"stdout": "out", "stderr": "err", "interrupted": false});
    assert_eq!(response_view(Some(&bash), &NULL).text, "out\nerr");
    let quiet = json!({"stdout": "", "stderr": ""});
    assert_eq!(response_view(Some(&quiet), &NULL).bytes(), 0);

    let read =
        json!({"type": "text", "file": {"filePath": "/a", "content": "l1\nl2\n", "numLines": 2}});
    let view = response_view(Some(&read), &NULL);
    assert_eq!(view.bytes(), 6 + 2 * READ_LINE_PREFIX_BYTES);

    let edit = json!({"filePath": "/home/u/proj/a.rs", "originalFile": "x".repeat(50_000), "structuredPatch": []});
    let view = response_view(Some(&edit), &NULL);
    assert_eq!(
        view.text,
        "The file /home/u/proj/a.rs has been updated successfully."
    );
    let write =
        json!({"type": "create", "filePath": "/home/u/proj/a.rs", "content": "x".repeat(9_000)});
    assert_eq!(
        response_view(Some(&write), &NULL).text,
        "File created successfully at: /home/u/proj/a.rs"
    );

    let blocks = json!([{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]);
    assert_eq!(response_view(Some(&blocks), &NULL).text, "ab");
    let opaque = json!({"matches": ["a", "b"], "total": 2});
    assert_eq!(
        response_view(Some(&opaque), &NULL).bytes(),
        opaque.to_string().len()
    );
    assert_eq!(response_view(Some(&json!("plain")), &NULL).text, "plain");
    assert_eq!(response_view(None, &NULL).bytes(), 0);

    // A command moved to the background has empty output, yet the model is told where it
    // went: it is a result, not a `vazio`.
    let background = json!({"stdout": "", "stderr": "", "backgroundTaskId": "b2yddki45"});
    let view = response_view(Some(&background), &NULL);
    assert!(outcome("Bash:cargo test", &view.text, view.extra_bytes, false) == Outcome::Ok);

    // ListAgents repeats its listing as `sections`: the model sees the listing once.
    let agents = json!({"listing": "touring-a1 [1]\ntouring-a2 [2]", "sections": [{"rows": ["touring-a1 [1]"]}]});
    assert_eq!(
        response_view(Some(&agents), &NULL).text,
        "touring-a1 [1]\ntouring-a2 [2]"
    );

    // An Agent launch echoes its whole prompt back: those bytes are the model's own.
    let prompt = "p".repeat(4_000);
    let input = json!({"prompt": prompt, "description": "scout"});
    let launch = json!({"status": "async_launched", "agentId": "a1", "prompt": prompt, "description": "scout"});
    let with_echo = response_view(Some(&launch), &NULL).bytes();
    let without_echo = response_view(Some(&launch), &input).bytes();
    assert!(
        with_echo > 4_000 && without_echo < 100,
        "{with_echo} vs {without_echo}"
    );
}

#[test]
fn record_completed_call_reads_a_hook_payload_and_dedupes_by_tool_use_id() {
    let tool = "TrioPayloadProbe";
    let payload = json!({
        "session_id": "trio-payload-probe", "tool_name": tool, "tool_use_id": "tu-1",
        "tool_input": {"q": 1}, "tool_response": "r".repeat(80)});
    record_completed_call(&payload, false);
    record_completed_call(&payload, false); // duplicate delivery of the same call
    let failed = json!({
        "session_id": "trio-payload-probe", "tool_name": tool, "tool_use_id": "tu-2",
        "tool_input": {"q": 2}, "error": "boom"});
    record_completed_call(&failed, true);
    let snap = snapshot_json();
    let row = &snap["classes"][tool];
    assert_eq!(
        (
            row["calls"].as_u64(),
            row["ok"].as_u64(),
            row["erro"].as_u64()
        ),
        (Some(2), Some(1), Some(1))
    );
    assert_eq!(row["result_bytes"].as_u64(), Some(84));
}

// ── Deny → next action ───────────────────────────────────────────────────

fn deny(reason: &str) -> String {
    json!({"hookSpecificOutput": {"hookEventName": "PreToolUse",
        "permissionDecision": "deny", "permissionDecisionReason": reason}})
    .to_string()
}

#[test]
fn deny_gate_of_reads_denials_only() {
    assert_eq!(
        deny_gate_of(&deny("[G10 exec-burst] 3ª chamada")).as_deref(),
        Some("G10")
    );
    assert_eq!(
        deny_gate_of(&deny("[G11 orçamento de bypass] x")).as_deref(),
        Some("G11")
    );
    assert_eq!(deny_gate_of(&deny("sem prefixo")).as_deref(), Some("other"));
    assert_eq!(
        deny_gate_of(r#"{"decision":"block","reason":"[G3 edit-sem-read] x"}"#).as_deref(),
        Some("G3")
    );
    let rewrite = json!({"hookSpecificOutput": {"permissionDecision": "allow",
        "permissionDecisionReason": "[G2 exit-code-through-pipe → REESCRITO]", "updatedInput": {}}});
    assert_eq!(deny_gate_of(&rewrite.to_string()), None);
    assert_eq!(deny_gate_of("{}"), None);
    assert_eq!(deny_gate_of("not json with \"deny\""), None);
}

#[test]
fn test_deny_recovery_classifies_next_call() {
    let mut l = TrioLedger::default();
    let class = || "Bash:grep".to_string();

    // G1 denies call A; the model sends a different, accepted call B -> recovered.
    l.on_pre("s", 0xA, Some(("G1".into(), class())));
    l.on_pre("s", 0xB, None);
    // G3 denies C; the model repeats C verbatim -> repeated.
    l.on_pre("s", 0xC, Some(("G3".into(), "Edit".into())));
    l.on_pre("s", 0xC, None);
    // G6 denies D; a different call E is denied again by G7 -> G6 denied_again,
    // and G7 is now pending; its next call F is accepted -> G7 recovered.
    l.on_pre("s", 0xD, Some(("G6".into(), class())));
    l.on_pre("s", 0xE, Some(("G7".into(), "Read".into())));
    l.on_pre("s", 0xF, None);
    // A later call with nothing pending changes nothing.
    l.on_pre("s", 0x1, None);

    let g = |name: &str| l.gates.get(name).expect("gate row");
    assert_eq!((g("G1").denied, g("G1").recovered), (1, 1));
    assert_eq!((g("G3").denied, g("G3").repeated), (1, 1));
    assert_eq!((g("G6").denied, g("G6").denied_again), (1, 1));
    assert_eq!((g("G7").denied, g("G7").recovered), (1, 1));
    assert_eq!(l.classes["Bash:grep"].negado, 2);
    assert_eq!(l.classes["Edit"].negado, 1);
}

#[test]
fn deny_recovery_resolves_through_the_post_event_for_tools_cli_suggest_never_sees() {
    let mut l = TrioLedger::default();
    l.on_pre("s", 0xA, Some(("G10".into(), "Bash:python3".into())));
    // An MCP tool (no cli-suggest matcher) runs next: only PostToolUse sees it.
    l.on_completed("s", "mcp__x__y".into(), 0xB, None, Outcome::Ok, 120);
    assert_eq!(l.gates["G10"].recovered, 1);
    // A repeat of the denied signature seen only at the post event is `repeated`.
    l.on_pre("s", 0xC, Some(("G10".into(), "Bash:python3".into())));
    l.on_completed("s", "Bash:python3".into(), 0xC, None, Outcome::Ok, 120);
    assert_eq!(l.gates["G10"].repeated, 1);
    // The unresolved remainder is reported, not hidden.
    l.on_pre("s", 0xD, Some(("G10".into(), "Bash:python3".into())));
    let row = GateRow::from_stats(&l.gates["G10"]);
    assert_eq!((row.denied, row.resolved, row.unresolved), (3, 2, 1));
    assert_eq!(row.recovery_rate, Some(0.5));
}

#[test]
fn pending_denials_are_per_session() {
    let mut l = TrioLedger::default();
    l.on_pre("one", 0xA, Some(("G1".into(), "Read".into())));
    l.on_pre("two", 0xB, None); // another session's call must not resolve it
    assert!(l.has_pending("one"));
    l.on_pre("one", 0xB, None);
    assert!(!l.has_pending("one"));
    assert_eq!(l.gates["G1"].recovered, 1);
}

// ── Bounds and snapshot shape ────────────────────────────────────────────

#[test]
fn state_stays_bounded() {
    let mut l = TrioLedger::default();
    for i in 0..(MAX_CLASSES + 40) {
        l.on_completed(
            "s",
            format!("Bash:verb{i}"),
            i as u64,
            None,
            Outcome::Ok,
            10,
        );
    }
    assert_eq!(
        l.classes.len(),
        MAX_CLASSES + 1,
        "256 classes + the overflow row"
    );
    assert_eq!(l.classes[OVERFLOW_CLASS].calls, 40);
    for i in 0..(MAX_SESSIONS + 10) {
        l.on_completed(&format!("s{i}"), "Read".into(), 1, None, Outcome::Ok, 10);
    }
    assert_eq!(l.sessions.len(), MAX_SESSIONS);
    for i in 0..(MAX_GATES + 5) {
        l.on_pre("s-gates", i as u64, Some((format!("G{i}"), "Read".into())));
    }
    assert!(l.gates.len() <= MAX_GATES + 1);
    let long = "x".repeat(300);
    l.on_completed("s", format!("Bash:{long}"), 7, None, Outcome::Ok, 1);
    assert!(
        l.classes
            .keys()
            .all(|k| k.chars().count() <= MAX_LABEL_CHARS.max(OVERFLOW_CLASS.len()))
    );
    let mut window = VecDeque::new();
    for i in 0..100 {
        push_capped(&mut window, i, WINDOW);
    }
    assert_eq!(window.len(), WINDOW);
}

#[test]
fn snapshot_reports_sizes_and_rates_with_absence_as_null() {
    let mut l = TrioLedger::default();
    for (sig, bytes) in [(1u64, 100usize), (2, 100), (3, 100), (4, 20_000)] {
        l.on_completed("s", "Read".into(), sig, None, Outcome::Ok, bytes);
    }
    l.on_completed("s", "Read".into(), 5, None, Outcome::Erro, 10);
    let snap = serde_json::to_value(l.snapshot()).expect("serializes");
    let read = &snap["classes"]["Read"];
    assert_eq!(read["calls"], 5);
    assert_eq!(read["erro_pct"], 20.0);
    assert_eq!(read["eficacia_pct"], 80.0);
    assert_eq!(read["size"]["samples"], 5);
    assert_eq!(
        read["size"]["p50_le_bytes"], 256,
        "3 of 5 samples are <= 256 B"
    );
    assert_eq!(
        read["size"]["p95_le_bytes"], 20_000,
        "clamped to the largest seen"
    );
    assert_eq!(read["size"]["gt_10kb_frac"], 0.2);
    assert_eq!(read["result_bytes"], 20_310);
    assert_eq!(snap["window"], 8);
    assert!(
        snap["scope"]
            .as_array()
            .is_some_and(|a| a.len() == SCOPE_NOTES.len())
    );

    // A class with only denied calls has NO size signal: null, not zero.
    l.on_pre("s", 9, Some(("G9".into(), "Bash:sed".into())));
    let snap = serde_json::to_value(l.snapshot()).expect("serializes");
    assert!(snap["classes"]["Bash:sed"]["size"]["p50_le_bytes"].is_null());
    assert!(snap["classes"]["Bash:sed"]["size"]["gt_10kb_frac"].is_null());
    // A gate with nothing resolved yet has no rate.
    assert!(snap["deny_recovery"]["G9"]["recovery_rate"].is_null());
}

#[test]
fn global_entry_points_feed_one_ledger() {
    let session = "trio-global-probe";
    let input = json!({"p": "trio-global-probe"});
    let reason = deny("[G9 escrita-cega-inline] x");
    observe_pre_call(session, "TrioGlobalProbe", &input, Some(&reason));
    observe_pre_call(session, "TrioGlobalProbe", &json!({"p": "other"}), None);
    let snap = snapshot_json();
    assert!(snap["classes"]["TrioGlobalProbe"]["negado"].as_u64() >= Some(1));
    assert!(snap["deny_recovery"]["G9"]["recovered"].as_u64() >= Some(1));
}

// ── Overhead budget (S-11: <= 0.5 ms p99 per call) ───────────────────────
//
// Timing assertions do not belong in a debug-profile CI run, so the measurement is
// `#[ignore]`d: `scripts/hw-slot --class rust -- cargo test --release -p
// touring-hooks-shared tool_trio::tests::overhead -- --ignored --nocapture`.

#[test]
#[ignore = "timing: run in release with --nocapture (see comment)"]
fn overhead_p99_is_within_budget() {
    let big_file = "fn main() { println!(\"hi\"); }\n".repeat(2_000); // ~60 KB
    let payloads = [
        json!({"session_id": "ovh", "tool_name": "Bash", "tool_input": {"command": "cd /home/x/proj && cargo test -p touring-hooks-shared --no-fail-fast 2>&1 | tail -40"},
               "tool_response": {"stdout": "test result: ok. 120 passed\n".repeat(80), "stderr": ""}}),
        json!({"session_id": "ovh", "tool_name": "Edit", "tool_input": {"file_path": "/home/x/a.rs", "old_string": "a".repeat(2_000), "new_string": "b".repeat(4_000)},
               "tool_response": {"filePath": "/home/x/a.rs", "originalFile": big_file, "structuredPatch": [], "oldString": "a", "newString": "b"}}),
        json!({"session_id": "ovh", "tool_name": "Read", "tool_input": {"file_path": "/home/x/a.rs"},
               "tool_response": {"type": "text", "file": {"filePath": "/home/x/a.rs", "content": big_file, "numLines": 2000}}}),
        json!({"session_id": "ovh", "tool_name": "Write", "tool_input": {"file_path": "/home/x/b.rs", "content": big_file},
               "tool_response": {"type": "create", "filePath": "/home/x/b.rs", "content": big_file, "structuredPatch": []}}),
        json!({"session_id": "ovh", "tool_name": "mcp__touring__touring_ctx_execute", "tool_input": {"language": "python", "code": "print(1)\n".repeat(40)},
               "tool_response": [{"type": "text", "text": "x".repeat(6_000)}]}),
        json!({"session_id": "ovh", "tool_name": "Grep", "tool_input": {"pattern": "fn main", "path": "/home/x"},
               "tool_response": {"content": "a.rs:1:fn main\n".repeat(200)}}),
    ];
    let reason = deny("[G10 exec-burst] 5ª chamada seriada");
    let mut samples = Vec::with_capacity(30_000);
    for round in 0..5_000 {
        for (i, payload) in payloads.iter().enumerate() {
            let mut payload = payload.clone();
            payload["tool_use_id"] = json!(format!("tu-{round}-{i}"));
            let start = std::time::Instant::now();
            record_completed_call(&payload, false);
            samples.push(start.elapsed());
        }
        // A denial every round, then the call that follows it.
        let input = json!({"command": format!("python3 job{round}.py")});
        let start = std::time::Instant::now();
        observe_pre_call("ovh", "Bash", &input, Some(&reason));
        samples.push(start.elapsed());
        let start = std::time::Instant::now();
        observe_pre_call("ovh", "Bash", &json!({"command": "ls"}), None);
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    let at = |q: f64| samples[((samples.len() as f64 * q) as usize).min(samples.len() - 1)];
    let (p50, p99, max) = (at(0.50), at(0.99), samples[samples.len() - 1]);
    println!(
        "tool_trio overhead over {} events: p50={p50:?} p99={p99:?} max={max:?}",
        samples.len()
    );
    assert!(
        p99 < std::time::Duration::from_micros(500),
        "p99 {p99:?} exceeds 0.5 ms"
    );
}

// ── Calibration of the live view against real transcripts ────────────────
//
// A transcript pairs each `tool_result` (the text the MODEL saw) with `toolUseResult`
// (the structured form the `PostToolUse` hook receives as `tool_response`). Replaying
// both through `response_view` / the offline text shows how far the live byte counts and
// outcomes sit from the transcript ones, per tool.

#[test]
#[ignore = "reads local transcripts: --ignored --nocapture (TOURING_TRIO_CALIBRATION_DIR)"]
fn live_view_calibration_against_transcripts() {
    let dir = std::env::var("TOURING_TRIO_CALIBRATION_DIR").unwrap_or_else(|_| {
        format!(
            "{}/.claude/projects/-home-gabrielgadea-projects-touring",
            std::env::var("HOME").unwrap_or_default()
        )
    });
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .expect("transcript dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
    // tool -> (n, live bytes, actual bytes, outcome agreements)
    let mut rows: BTreeMap<String, (u64, u64, u64, u64)> = BTreeMap::new();
    for file in files.iter().take(12) {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let mut uses: HashMap<String, (String, Value)> = HashMap::new();
        for line in text.lines() {
            let Ok(rec) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let Some(blocks) = rec.pointer("/message/content").and_then(Value::as_array) else {
                continue;
            };
            for c in blocks {
                let id = c.get("id").and_then(Value::as_str).unwrap_or("");
                match c.get("type").and_then(Value::as_str) {
                    Some("tool_use") => {
                        let name = c.get("name").and_then(Value::as_str).unwrap_or("");
                        uses.insert(
                            id.to_string(),
                            (
                                name.to_string(),
                                c.get("input").cloned().unwrap_or(Value::Null),
                            ),
                        );
                    }
                    Some("tool_result") => {
                        // Only successes carry a structured `toolUseResult` object/array.
                        let Some(structured) = rec.get("toolUseResult").filter(|v| !v.is_string())
                        else {
                            continue;
                        };
                        let tid = c.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
                        let Some((name, input)) = uses.get(tid) else {
                            continue;
                        };
                        let actual = match c.get("content") {
                            Some(Value::String(s)) => s.clone(),
                            Some(Value::Array(b)) => blocks_text(b),
                            _ => String::new(),
                        };
                        let class = tool_class(name, input);
                        let live = response_view(Some(structured), input);
                        let agree = outcome(&class, &live.text, live.extra_bytes, false)
                            == outcome(&class, &actual, 0, false);
                        let row = rows.entry(name.clone()).or_default();
                        row.0 += 1;
                        row.1 += live.bytes() as u64;
                        row.2 += actual.len() as u64;
                        row.3 += u64::from(agree);
                    }
                    _ => {}
                }
            }
        }
    }
    println!("tool                                   n   live_B  actual_B  ratio  outcome-agree");
    for (name, (n, live, actual, agree)) in &rows {
        println!(
            "{name:<34} {n:>6} {live:>8} {actual:>9} {:>6.2} {:>6.1}%",
            *live as f64 / (*actual).max(1) as f64,
            100.0 * *agree as f64 / (*n).max(1) as f64
        );
    }
    assert!(
        !rows.is_empty(),
        "no (tool_result, toolUseResult) pairs found in {dir}"
    );
}

// ── Deny -> next action, in the offline script's vocabulary ──────────────
//
// `scripts/measure_tool_trio.py` (D1a, `deny_next`) names the same four outcomes in
// Portuguese: recuperou = `recovered`, repetiu = `repeated`, negado-de-novo =
// `denied_again`, abandonou ~ `unresolved` (a denial nothing followed). The cases below
// mirror its deny -> next fixtures call for call, so both sides keep one definition.

/// Replay a transcript the way the hooks would have seen it: a denied result is a
/// `PreToolUse` denial (`on_pre`), everything else a completed call (`on_completed`).
fn replay_denials(records: &[Value]) -> TrioLedger {
    let mut ledger = TrioLedger::default();
    let mut uses: HashMap<String, (String, u64)> = HashMap::new();
    for rec in records {
        let Some(content) = rec.pointer("/message/content").and_then(Value::as_array) else {
            continue;
        };
        for c in content {
            match c.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    let id = c.get("id").and_then(Value::as_str).unwrap_or("");
                    let name = c.get("name").and_then(Value::as_str).unwrap_or("");
                    let input = c.get("input").unwrap_or(&NULL);
                    uses.insert(
                        id.to_string(),
                        (tool_class(name, input), call_signature(name, input)),
                    );
                }
                Some("tool_result") => {
                    let id = c.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
                    let Some((class, sig)) = uses.remove(id) else {
                        continue;
                    };
                    let text = c
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let is_error = c.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                    let result = outcome(&class, &text, 0, is_error);
                    if result == Outcome::Negado {
                        let gate = text
                            .split("hook error: ")
                            .nth(1)
                            .map_or_else(|| "other".to_string(), gate_tag);
                        ledger.on_pre("s", sig, Some((gate, class)));
                    } else {
                        ledger.on_completed("s", class, sig, None, result, text.len());
                    }
                }
                _ => {}
            }
        }
    }
    ledger
}

const DENY_TEXT: &str = "PreToolUse:Bash hook error: [G10 exec-burst] 5a chamada ...";

#[test]
fn test_deny_then_different_ok_call_is_recovery() {
    let l = replay_denials(&[
        use_(1, "Bash", json!({"command": "ls a"})),
        res(1, json!(DENY_TEXT), true),
        use_(2, "Bash", json!({"command": "touring run --code x"})),
        res(2, json!("ok ".repeat(30)), false),
    ]);
    assert_eq!((l.gates["G10"].denied, l.gates["G10"].recovered), (1, 1));
}

#[test]
fn test_deny_then_same_call_is_repetition() {
    let l = replay_denials(&[
        use_(1, "Bash", json!({"command": "ls a"})),
        res(1, json!(DENY_TEXT), true),
        use_(2, "Bash", json!({"command": "ls a"})),
        res(2, json!(DENY_TEXT), true),
    ]);
    let g = GateRow::from_stats(&l.gates["G10"]);
    // repetiu = 1; the second denial is itself left without a follower (abandonou = 1).
    assert_eq!((g.repeated, g.unresolved), (1, 1));
}

#[test]
fn test_deny_then_other_denied_call_is_denied_again() {
    let l = replay_denials(&[
        use_(1, "Bash", json!({"command": "ls a"})),
        res(1, json!(DENY_TEXT), true),
        use_(2, "Bash", json!({"command": "ls b"})),
        res(2, json!(DENY_TEXT), true),
    ]);
    assert_eq!(l.gates["G10"].denied_again, 1);
}
