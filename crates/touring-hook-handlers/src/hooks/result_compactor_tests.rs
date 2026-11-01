//! Tests for the result compactor (S-14 / B1). Synthetic data only.

// The marker below is load-bearing: `hook_registry_tests::guard_set_matches_the_handlers_that_
// can_deny` derives the denying hooks from every `hooks/*.rs` source up to its first
// `#[cfg(test)]`, and this file builds `HookResponse::Deny` fixtures. Same shape as the other
// `*_tests.rs` files of this directory.
#[cfg(test)]
use super::*;
use serde_json::json;
use std::time::{Duration, Instant};

/// `n` benign build-log lines (none matches the failure vocabulary).
fn noise(n: usize) -> String {
    (1..=n)
        .map(|i| format!("   Compiling crate-{i} v0.1.{i} (/work/crate-{i})\n"))
        .collect()
}

const TRACEBACK: &str = "Traceback (most recent call last):\n  File \"/work/app.py\", line 10, in <module>\n    main()\n  File \"/work/app.py\", line 6, in main\n    run_job(spec)\nKeyError: 'spec'\n";
const RUSTC_ERROR: &str = "error[E0308]: mismatched types\n --> src/lib.rs:12:5\n  |\n12 |     let x: u32 = \"a\";\n   |            ---   ^^^ expected `u32`, found `&str`\n   |            |\n   |            expected due to this\n\n";
const CARGO_FAILURES: &str = "failures:\n\n---- tests::alpha stdout ----\nthread 'tests::alpha' panicked at src/lib.rs:5:9:\nboom at the third step\n  left: 1\n right: 2\n\nfailures:\n    tests::alpha\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored\n";
const PYTEST_SECTION: &str = "=================== FAILURES ===================\n_______________ test_alpha _______________\n\n    def test_alpha():\n>       assert load() == 3\nE       assert 2 == 3\n\ntests/test_alpha.py:4: AssertionError\n=========== short test summary info ===========\n";
const GO_FAIL: &str = "--- FAIL: TestAlpha (0.00s)\n    alpha_test.go:12: got 2, want 3\n    alpha_test.go:13: second line of the report\nFAIL\n";

fn config(dir: &Path) -> Config {
    Config {
        ceiling: DEFAULT_CEILING,
        dir: dir.to_path_buf(),
    }
}

fn bash_input(command: &str, stdout: &str) -> Value {
    json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "tool_response": {"stdout": stdout, "stderr": "warn on stderr\n", "interrupted": false},
    })
}

fn big_log() -> String {
    let mut log = noise(200);
    log.push_str(TRACEBACK);
    log.push_str(&noise(200));
    log.push_str(RUSTC_ERROR);
    log.push_str(&noise(200));
    log.push_str(CARGO_FAILURES);
    log.push_str(&noise(60));
    log
}

fn rewritten_stdout(response: &HookResponse) -> &str {
    match response {
        HookResponse::ContextWithUpdatedToolOutput {
            updated_tool_output,
            ..
        } => updated_tool_output["stdout"]
            .as_str()
            .expect("stdout stays a string"),
        other => panic!("expected a rewritten result, got {other:?}"),
    }
}

#[test]
fn test_compactor_never_cuts_failure_block() {
    for block in [
        TRACEBACK,
        RUSTC_ERROR,
        CARGO_FAILURES,
        PYTEST_SECTION,
        GO_FAIL,
    ] {
        let mut log = noise(300);
        log.push_str(block);
        log.push_str(&noise(300));
        let compacted = compact_text(&log, DEFAULT_CEILING).expect("a log this size is compacted");
        for line in block.lines() {
            assert!(
                compacted.body.contains(line),
                "failure line cut: {line:?}\n--- block ---\n{block}\n--- body ---\n{}",
                compacted.body
            );
        }
        assert!(
            compacted.body.len() * 4 < log.len(),
            "the noise around the block must go ({} of {} bytes kept)",
            compacted.body.len(),
            log.len()
        );
    }
    // all five at once, in one log
    let log = big_log();
    let body = compact_text(&log, DEFAULT_CEILING).expect("compacted").body;
    for block in [TRACEBACK, RUSTC_ERROR, CARGO_FAILURES] {
        for line in block.lines() {
            assert!(body.contains(line), "failure line cut: {line:?}");
        }
    }
}

#[test]
fn test_compactor_keeps_pointer_to_raw() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("compacted");
    let raw = big_log();
    let input = bash_input("cargo test --workspace 2>&1", &raw);
    let response = attach_via(HookResponse::Allow, &input, Some(&config(&dir)));
    let stdout = rewritten_stdout(&response);

    assert!(stdout.len() < raw.len() / 4, "the result got smaller");
    let hash = blake3::hash(raw.as_bytes()).to_hex().to_string();
    let hash = &hash[..HASH_CHARS];
    let saved = dir.join(format!("{hash}.txt"));
    assert!(stdout.contains(MARKER), "footer marker present");
    assert!(
        stdout.contains(saved.to_str().expect("utf-8 path")),
        "the footer names the saved file: {stdout}"
    );
    assert!(stdout.contains(hash), "the footer carries the hash");
    assert!(
        stdout.contains("Read ") && stdout.contains("sed -n"),
        "the footer teaches how to read it back"
    );
    assert_eq!(
        std::fs::read_to_string(&saved).expect("raw copy on disk"),
        raw,
        "the raw output is saved whole and byte-identical"
    );
    // an omitted range really points at raw line numbers
    assert!(stdout.contains("[... raw lines 13-"), "{stdout}");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file_mode = std::fs::metadata(&saved)
            .expect("meta")
            .permissions()
            .mode();
        let dir_mode = std::fs::metadata(&dir).expect("meta").permissions().mode();
        assert_eq!(file_mode & 0o777, 0o600, "raw copy is private");
        assert_eq!(dir_mode & 0o777, 0o700, "its directory is private");
    }
}

#[test]
fn test_compactor_off_by_default_is_noop() {
    let none = |_: &str| None::<String>;
    assert!(Config::from_lookup(&none).is_none(), "unset = off");
    for off in ["", "0", "off", "false", "no", "maybe"] {
        let get = |k: &str| (k == ENV_FLAG).then(|| off.to_string());
        assert!(Config::from_lookup(&get).is_none(), "{off:?} is not on");
    }
    // off => the response comes back as it went in, even for a huge failing log
    let input = bash_input("cargo test", &big_log());
    for response in [
        HookResponse::Allow,
        HookResponse::context_with_event("ctx", "PostToolUse"),
    ] {
        assert_eq!(attach_via(response.clone(), &input, None), response);
    }
}

/// An environment lookup over fixed pairs.
fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |key| {
        pairs
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| (*v).to_string())
    }
}

#[test]
fn test_compactor_flag_arms_with_the_documented_values() {
    for on in ["on", "ON", " 1 ", "true", "Yes"] {
        let pairs = [(ENV_FLAG, on), ("HOME", "/home/synthetic")];
        let cfg = Config::from_lookup(&lookup(&pairs));
        assert_eq!(
            cfg,
            Some(Config {
                ceiling: DEFAULT_CEILING,
                dir: PathBuf::from("/home/synthetic/.claude/touring/compacted"),
            }),
            "{on:?} arms it"
        );
    }
}

#[test]
fn test_compactor_env_overrides_and_missing_home() {
    let pairs = [
        (ENV_FLAG, "on"),
        (ENV_DIR, "/srv/raw"),
        (ENV_CEILING, "5000"),
    ];
    let cfg = Config::from_lookup(&lookup(&pairs));
    assert_eq!(
        cfg,
        Some(Config {
            ceiling: 5000,
            dir: PathBuf::from("/srv/raw"),
        })
    );
    // armed but nowhere to save the raw copy => off, never a half-working compactor
    let no_home = [(ENV_FLAG, "on")];
    assert_eq!(Config::from_lookup(&lookup(&no_home)), None);
}

#[test]
fn test_compactor_ceiling_parsing() {
    assert_eq!(ceiling_from(None), DEFAULT_CEILING);
    assert_eq!(ceiling_from(Some("1")), MIN_CEILING, "clamped up");
    assert_eq!(
        ceiling_from(Some("99999999999")),
        MAX_CEILING,
        "clamped down"
    );
    assert_eq!(
        ceiling_from(Some("abc")),
        DEFAULT_CEILING,
        "garbage = default"
    );
}

#[test]
fn test_compactor_below_ceiling_passes_through() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let small = noise(20);
    assert!(small.len() < DEFAULT_CEILING);
    let input = bash_input("cargo build", &small);
    let cfg = config(tmp.path());
    assert_eq!(
        attach_via(HookResponse::Allow, &input, Some(&cfg)),
        HookResponse::Allow
    );
    assert!(
        std::fs::read_dir(tmp.path()).expect("dir").next().is_none(),
        "nothing is written for a result that is left alone"
    );
}

#[test]
fn test_compactor_keeps_head_and_tail_and_marks_gaps() {
    let log = noise(500);
    let compacted = compact_text(&log, DEFAULT_CEILING).expect("compacted");
    let lines: Vec<&str> = log.lines().collect();
    for line in &lines[..HEAD_LINES] {
        assert!(compacted.body.contains(line), "head line cut: {line}");
    }
    for line in &lines[lines.len() - TAIL_LINES..] {
        assert!(compacted.body.contains(line), "tail line cut: {line}");
    }
    let omitted = 500 - HEAD_LINES - TAIL_LINES;
    assert!(
        compacted.body.contains(&format!(
            "[... raw lines {}-{} omitted ({omitted} lines) ...]",
            HEAD_LINES + 1,
            500 - TAIL_LINES
        )),
        "{}",
        compacted.body
    );
    assert_eq!(compacted.total_lines, 500);
    assert_eq!(compacted.kept_lines, HEAD_LINES + TAIL_LINES);
    assert_eq!(compacted.failure_lines, 0);
}

#[test]
fn test_compactor_clips_benign_long_lines_but_never_failure_lines() {
    let blob = format!("payload={}\n", "x".repeat(5_000));
    let failing = format!("fatal: {}\n", "y".repeat(5_000));
    let mut log = blob.clone();
    log.push_str(&noise(100));
    log.push_str(&failing);
    log.push_str(&noise(100));
    let body = compact_text(&log, DEFAULT_CEILING).expect("compacted").body;
    assert!(
        !body.contains(&"x".repeat(MAX_KEPT_LINE_CHARS + 1)),
        "a benign long line is clipped"
    );
    assert!(body.contains("bytes clipped]"));
    assert!(body.contains(failing.trim_end()), "a failure line is whole");
}

#[test]
fn test_compactor_declines_when_the_saving_is_small() {
    // every line is a failure line: nothing to cut, so the result is left alone
    let log: String = (0..300)
        .map(|i| format!("error: step {i} failed\n"))
        .collect();
    assert!(compact_text(&log, DEFAULT_CEILING).is_none());
    // just over the ceiling with few lines: head+tail would keep nearly everything
    let few = noise(40);
    assert!(few.len() > MIN_CEILING);
    assert!(compact_text(&few, MIN_CEILING).is_none());
}

#[test]
fn test_compactor_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = config(tmp.path());
    let first = attach_via(
        HookResponse::Allow,
        &bash_input("make", &big_log()),
        Some(&cfg),
    );
    let compacted = rewritten_stdout(&first).to_string();
    let again = attach_via(
        HookResponse::Allow,
        &bash_input("make", &(compacted.clone() + &noise(300))),
        Some(&cfg),
    );
    assert_eq!(again, HookResponse::Allow, "its own footer is never re-cut");
}

#[test]
fn test_compactor_only_answers_post_tool_use_of_bash() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = config(tmp.path());
    let raw = big_log();
    let failure = json!({
        "hook_event_name": "PostToolUseFailure",
        "tool_name": "Bash",
        "tool_input": {"command": "cargo test"},
        "error": raw,
    });
    let mut other_tool = bash_input("cargo test", &raw);
    other_tool["tool_name"] = json!("Read");
    let mut legacy = bash_input("cargo test", &raw);
    legacy
        .as_object_mut()
        .expect("object")
        .remove("hook_event_name");
    for input in [failure, other_tool, legacy] {
        assert_eq!(
            attach_via(HookResponse::Allow, &input, Some(&cfg)),
            HookResponse::Allow
        );
    }
}

#[test]
fn test_compactor_leaves_decision_verdicts_alone() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = config(tmp.path());
    let input = bash_input("cargo test", &big_log());
    for decision in [
        HookResponse::Block {
            reason: "tests must pass".to_string(),
            context: None,
            event_name: Some("PostToolUse".to_string()),
        },
        HookResponse::Deny {
            reason: "no".to_string(),
            context: None,
            event_name: None,
        },
        HookResponse::Halt {
            reason: "stop".to_string(),
        },
        HookResponse::ContextWithUpdatedInput {
            context: String::new(),
            event_name: None,
            updated_input: json!({"command": "ls"}),
        },
    ] {
        assert_eq!(
            attach_via(decision.clone(), &input, Some(&cfg)),
            decision,
            "a decision keeps its shape"
        );
    }
}

#[test]
fn test_compactor_keeps_existing_context_next_to_the_rewrite() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = config(tmp.path());
    let input = bash_input("pytest -q", &big_log());
    let response = attach_via(
        HookResponse::context_with_event("tests: 1 failed", "PostToolUse"),
        &input,
        Some(&cfg),
    );
    match response {
        HookResponse::ContextWithUpdatedToolOutput {
            context,
            event_name,
            ..
        } => {
            assert_eq!(context, "tests: 1 failed");
            assert_eq!(event_name.as_deref(), Some("PostToolUse"));
        }
        other => panic!("expected a rewrite carrying the context: {other:?}"),
    }
}

#[test]
fn test_compactor_skips_results_that_are_not_plain_log_text() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = config(tmp.path());
    let raw = big_log();
    for (key, value) in [
        ("interrupted", json!(true)),
        ("isImage", json!(true)),
        ("persistedOutputPath", json!("/tmp/persisted.txt")),
        ("backgroundTaskId", json!("bg-1")),
        ("returnCodeInterpretation", json!("No matches found")),
        ("structuredContent", json!([{"type": "text", "text": "x"}])),
        ("exit_code", json!(2)),
    ] {
        let mut input = bash_input("cargo test", &raw);
        input["tool_response"][key] = value;
        assert_eq!(
            attach_via(HookResponse::Allow, &input, Some(&cfg)),
            HookResponse::Allow,
            "{key} makes the result not ours to rewrite"
        );
    }
}

#[test]
fn test_compactor_keeps_the_bash_output_shape() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = config(tmp.path());
    let mut input = bash_input("npm install", &big_log());
    input["tool_response"]["noOutputExpected"] = json!(false);
    let response = attach_via(HookResponse::Allow, &input, Some(&cfg));
    let wire: Value = serde_json::from_str(&response.to_json()).expect("valid JSON");
    let hso = &wire["hookSpecificOutput"];
    assert_eq!(hso["hookEventName"], "PostToolUse");
    assert!(
        hso.get("additionalContext").is_none(),
        "no context, no blank reminder: {wire}"
    );
    let out = &hso["updatedToolOutput"];
    // Claude Code validates the value against Bash's output schema: stdout/stderr strings and
    // `interrupted` boolean are required, and every other field must survive.
    assert!(out["stdout"].is_string());
    assert_eq!(out["stderr"], "warn on stderr\n", "stderr is never touched");
    assert_eq!(out["interrupted"], false);
    assert_eq!(out["noOutputExpected"], false);
    assert_eq!(out.as_object().expect("object").len(), 4);
}

#[test]
fn test_payload_commands_are_left_alone() {
    let payload = [
        "cat Cargo.toml",
        "cat /var/log/build.log",
        "head -200 build.log",
        "sed -n '10,400p' src/lib.rs",
        "cargo test --workspace 2>&1 | tail -80",
        "cargo test 2>&1 | grep -n 'FAILED'",
        "cd crates/x && cat README.md",
        "rg -n 'TODO' crates",
        "find . -name '*.rs'",
        "git diff HEAD~3",
        "git -C /work log --stat -20",
        "git show abc123",
        "ls -la target",
        "jq '.items' data.json",
        "touring run --lang python --file /tmp/x.py",
        "FOO=1 cat file",
        "sudo cat /etc/hosts",
        "timeout 30 grep -r needle .",
        "python3 report.py | sort | uniq -c",
        "grep \"a|b\" notes.txt",
    ];
    for command in payload {
        assert!(is_payload_command(command), "payload expected: {command}");
    }
    let logs = [
        "cargo test --workspace 2>&1",
        "cargo build --release",
        "npm install",
        "python3 -c 'print(1)'",
        "python3 - <<'EOF'\nprint('a | grep b')\nEOF",
        "pip install -r requirements.txt",
        "make -j8 all 2>&1 | tee build.log",
        "git push origin main",
        "git clone https://example.invalid/r.git",
        "cd /work && pytest -q",
        "docker build .",
        "",
    ];
    for command in logs {
        assert!(!is_payload_command(command), "log expected: {command:?}");
    }
}

#[test]
fn test_last_stage_respects_quotes_redirections_and_heredocs() {
    assert_eq!(last_stage("a | b | c"), " c");
    assert_eq!(last_stage("a && b; c"), " c");
    assert_eq!(last_stage("grep 'x|y' f"), "grep 'x|y' f");
    assert_eq!(last_stage("grep \"x|y\" f"), "grep \"x|y\" f");
    assert_eq!(last_stage("echo a\\|b"), "echo a\\|b");
    assert_eq!(last_stage("cargo test 2>&1"), "cargo test 2>&1");
    assert_eq!(last_stage("cargo test &> out.log"), "cargo test &> out.log");
    assert_eq!(last_stage("cat <<EOF\nx | y\nEOF"), "cat ");
    assert_eq!(last_stage("a & b"), " b");
}

#[test]
fn test_persist_raw_is_content_addressed_and_pruned() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("raw");
    let (path, hash) = persist_raw(&dir, "same bytes").expect("saved");
    let again = persist_raw(&dir, "same bytes").expect("saved twice");
    assert_eq!(
        (path.clone(), hash.clone()),
        again,
        "same output, same file"
    );
    assert_eq!(hash.len(), HASH_CHARS);
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "same bytes");

    for i in 0..KEEP_FILES + 8 {
        persist_raw(&dir, &format!("distinct output {i}")).expect("saved");
    }
    let kept = std::fs::read_dir(&dir).expect("dir").count();
    assert!(
        kept <= KEEP_FILES,
        "pruned to the newest {KEEP_FILES}, found {kept}"
    );
}

#[test]
fn test_persist_failure_passes_the_result_through() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // a FILE where the directory should be: the raw copy cannot be saved
    let blocked = tmp.path().join("not-a-dir");
    std::fs::write(&blocked, "x").expect("file");
    let cfg = config(&blocked.join("compacted"));
    assert_eq!(
        attach_via(
            HookResponse::Allow,
            &bash_input("cargo test", &big_log()),
            Some(&cfg)
        ),
        HookResponse::Allow,
        "no raw copy, no rewrite"
    );
}

/// Budget: 5 ms at p99 per result, over a ~144 KB log. Measured 27/10/2026 on the unoptimised
/// test profile (shared, thermally loaded machine): cold 1.5 ms, p50 0.7 ms, p99 1.0 ms. The
/// debug assertion allows 3x the budget so the guard catches a complexity regression, not noise.
#[test]
fn test_compactor_timing_budget() {
    let mut log = noise(2_500);
    log.push_str(TRACEBACK);
    log.push_str(&noise(300));
    log.push_str(CARGO_FAILURES);
    assert!(
        log.len() > 100_000,
        "a realistic large log ({} B)",
        log.len()
    );
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = config(tmp.path());
    let input = bash_input("cargo test --workspace 2>&1", &log);

    // the first call also compiles the failure regex: measured separately
    let cold = Instant::now();
    assert!(compact_text(&log, DEFAULT_CEILING).is_some());
    let cold = cold.elapsed();

    let mut samples = Vec::with_capacity(300);
    for _ in 0..300 {
        let t = Instant::now();
        let response = attach_via(HookResponse::Allow, &input, Some(&cfg));
        samples.push(t.elapsed());
        assert!(matches!(
            response,
            HookResponse::ContextWithUpdatedToolOutput { .. }
        ));
    }
    samples.sort();
    let p99 = samples[samples.len() * 99 / 100];
    eprintln!(
        "result_compactor timing ({} B log, {}): cold {cold:?}, p50 {:?}, p99 {p99:?}",
        log.len(),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        samples[samples.len() / 2]
    );
    let budget = if cfg!(debug_assertions) {
        Duration::from_millis(15)
    } else {
        Duration::from_millis(5)
    };
    assert!(
        p99 <= budget,
        "p99 {p99:?} over the {budget:?} budget (cold start {cold:?}, {} B log)",
        log.len()
    );
    assert!(
        cold <= budget * 4,
        "cold start {cold:?} (regex compile) over 4x the {budget:?} budget"
    );
}
