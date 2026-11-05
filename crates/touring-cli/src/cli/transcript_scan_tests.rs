//! Tests of the transcript reader: the record-kind policy (decided once, tested once, here) and
//! the walk, the announcement parsers and the typed-command parser (B5/B3, 01/11/2026).

use super::fixtures::*;
use super::*;
use serde_json::json;
use std::collections::BTreeSet;

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|n| (*n).to_string()).collect()
}

/// A plugin skill is `plugin:skill`, and the listing line is `- plugin:skill: <prose>`.
/// Cutting at the first colon would bill the bytes to `plugin`, a name nobody announced.
#[test]
fn attribute_lines_matches_the_longest_announced_name() {
    let names = set(&["brand-voice:discover-brand", "briah", "plain"]);
    let lines = [
        "- brand-voice:discover-brand: Autonomously searches platforms",
        "- briah",
        "- plain: something: with a colon inside the prose",
        "- not-announced: ignored",
    ];
    let got = attribute_lines(lines.iter().copied(), &names);
    assert_eq!(got.len(), 3, "the unannounced line is not billed to anyone: {got:?}");
    assert_eq!(got["brand-voice:discover-brand"], lines[0].len() as u64 + 1);
    assert_eq!(got["briah"], lines[1].len() as u64 + 1);
    assert_eq!(got["plain"], lines[2].len() as u64 + 1);
}

/// A deferred-tool line is the bare name (no `- ` prefix): the same matcher serves it.
#[test]
fn attribute_lines_serves_bare_tool_names() {
    let names = set(&["ArtifactData", "mcp__touring__touring_search"]);
    let got = attribute_lines(
        ["ArtifactData", "mcp__touring__touring_search"].into_iter(),
        &names,
    );
    assert_eq!(got["ArtifactData"], "ArtifactData".len() as u64 + 1);
    assert_eq!(got.len(), 2);
}

/// The four announcement attachments parse into per-item bytes; the kind names are the
/// ones the payload publishes.
#[test]
fn parse_announcement_reads_the_four_surfaces() {
    let skill: Value = serde_json::from_str(&skill_listing(
        &["alpha", "beta"],
        "- alpha: first\n- beta: second",
    ))
    .expect("json");
    let a = parse_announcement(&skill["attachment"]).expect("skill listing");
    assert_eq!(a.kind, AnnKind::Skill);
    assert_eq!(a.total_bytes, "- alpha: first\n- beta: second".len() as u64);
    assert_eq!(a.items.len(), 2);

    let agent: Value =
        serde_json::from_str(&agent_listing(&["explorer"], &["- explorer: reads code"]))
            .expect("json");
    let a = parse_announcement(&agent["attachment"]).expect("agent listing");
    assert_eq!(a.kind, AnnKind::Agent);
    assert_eq!(a.items["explorer"], "- explorer: reads code".len() as u64 + 1);

    let tools: Value = serde_json::from_str(&deferred_tools(&["CronCreate"])).expect("json");
    let a = parse_announcement(&tools["attachment"]).expect("deferred tools");
    assert_eq!(a.kind, AnnKind::Tool);
    assert_eq!(a.items["CronCreate"], "CronCreate".len() as u64 + 1);

    let mcp: Value = serde_json::from_str(&mcp_instructions(&["touring"], &["## touring\nbody"]))
        .expect("json");
    let a = parse_announcement(&mcp["attachment"]).expect("mcp instructions");
    assert_eq!(a.kind, AnnKind::Mcp);
    assert_eq!(a.items["touring"], "## touring\nbody".len() as u64);
}

#[test]
fn parse_announcement_ignores_every_other_attachment() {
    let hook: Value = serde_json::from_str(&hook("x")).expect("json");
    assert!(parse_announcement(&hook["attachment"]).is_none());
}

/// A typed `/name` reaches the transcript as `<command-name>/name</command-name>` inside a
/// user string; the slash is not part of the name and several can share one record.
#[test]
fn typed_commands_extracts_names_without_the_slash() {
    let text = "<command-message>x</command-message>\n<command-name>/loop-engineering</command-name>\n<command-args>go</command-args>";
    assert_eq!(typed_commands(text), vec!["loop-engineering".to_string()]);
    let two = "<command-name>a</command-name> and <command-name>/b:c</command-name>";
    assert_eq!(typed_commands(two), vec!["a".to_string(), "b:c".to_string()]);
    assert!(typed_commands("no tags, just /slash text").is_empty());
    assert!(typed_commands("<command-name>unterminated").is_empty());
}

/// Newest first, capped, and the cap is REPORTED: a partial scan must never read as a
/// complete one.
#[test]
fn scan_dir_reads_newest_first_and_reports_the_cap() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (i, id) in ["old", "mid", "new"].iter().enumerate() {
        let p = dir.path().join(format!("{id}.jsonl"));
        std::fs::write(&p, format!("{0}\n{0}\nnot json\n", user_text("hi"))).expect("write");
        let t = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000 + i as u64);
        let f = std::fs::File::options().write(true).open(&p).expect("open");
        f.set_modified(t).expect("mtime");
    }
    let caps = ScanCaps { max_transcripts: 2, max_bytes: u64::MAX };
    let scan = scan_dir::<u64>(dir.path(), &caps, &mut |n, _| *n += 1).expect("scan");
    let ids: Vec<&str> = scan.sessions.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["new", "mid"]);
    assert_eq!(scan.sessions[0].state, 2, "two parseable records, one event each");
    assert_eq!(scan.sessions[0].read.malformed_lines, 1, "the bad line is counted, not hidden");
    assert_eq!(scan.skipped_by_cap, 1);
    assert!(!scan.capped, "the byte cap was not hit");
}

#[test]
fn scan_dir_stops_at_the_byte_cap_and_says_so() {
    let dir = tempfile::tempdir().expect("tempdir");
    for id in ["a", "b"] {
        std::fs::write(dir.path().join(format!("{id}.jsonl")), format!("{}\n", user_text("hi")))
            .expect("write");
    }
    let caps = ScanCaps { max_transcripts: 10, max_bytes: 1 };
    let scan = scan_dir::<u64>(dir.path(), &caps, &mut |n, _| *n += 1).expect("scan");
    assert_eq!(scan.sessions.len(), 1, "the first file is read, the second is not");
    assert!(scan.capped);
}

#[test]
fn scan_dir_reports_a_missing_directory_instead_of_zeros() {
    let err = scan_dir::<u64>(Path::new("/nonexistent/touring-xyz"), &KPI_CAPS, &mut |_, _| {})
        .err()
        .expect("absent directory is an error");
    assert!(err.contains("no transcript directory"), "{err}");
}

// ── the reader's policy: decided once, here ─────────────────────────────────────────────────

/// What the reader yields for `lines`, one comparable string per event.
fn read(lines: &[String]) -> (Vec<String>, ReadStats) {
    let mut out = Vec::new();
    let stats = read_lines(lines.iter().map(String::as_str), &mut |ev| {
        out.push(match ev {
            Event::User(u) => format!("user prompt={} {:?}", u.is_prompt, u.texts),
            Event::ToolResult(r) => {
                format!("result id={:?} err={} {:?}", r.tool_use_id, r.is_error, r.text)
            }
            Event::Assistant(a) => {
                let blocks: Vec<String> = a
                    .blocks
                    .iter()
                    .map(|b| match b {
                        Block::Text(t) => format!("text:{t}"),
                        Block::Thinking(t) => format!("thinking:{t}"),
                        Block::ToolUse(c) => format!("use:{}:{}", c.id.unwrap_or("-"), c.name),
                    })
                    .collect();
                let usage = a.usage.map_or("none".to_string(), |u| {
                    format!("{}/{}", u.model.unwrap_or("?"), u.input_tokens)
                });
                format!("assistant usage={usage} {blocks:?}")
            }
            Event::Attachment(att) => format!("attachment {}", att["type"]),
            Event::CompactBoundary => "compact".to_string(),
            Event::CostState(_) => "cost-state".to_string(),
        });
    });
    (out, stats)
}

/// How many events of the kind `prefix` the reader yields.
fn count(events: &[String], prefix: &str) -> usize {
    events.iter().filter(|e| e.starts_with(prefix)).count()
}

/// A person types a prompt; Claude Code's own expansion (a skill body, a command's caveat) is
/// filed as a user record flagged `isMeta` and is not one. Measured 01/11/2026: 862 of 1.693
/// counted "turns" were `isMeta`. The text still reaches the consumer — it filled the window.
#[test]
fn an_ismeta_user_record_is_not_a_prompt() {
    let (events, _) = read(&[user_text("faca a auditoria"), meta(&user_text("skill body"))]);
    assert_eq!(
        events,
        [
            r#"user prompt=true ["faca a auditoria"]"#,
            r#"user prompt=false ["skill body"]"#
        ]
    );
}

/// What the harness wrote — a compaction summary, a command's stdout or caveat — is not a person
/// either, however it is filed. A turn that merely carries a `<system-reminder>` stays human.
#[test]
fn text_with_a_harness_marker_is_not_a_prompt() {
    for marker in NON_HUMAN_TURN_MARKERS {
        let (events, _) = read(&[user_text(&format!("prefix {marker} suffix"))]);
        assert_eq!(count(&events, "user prompt=false"), 1, "{marker}: {events:?}");
    }
    let (events, _) = read(&[user_text("faca a auditoria\n<system-reminder>x</system-reminder>")]);
    assert_eq!(count(&events, "user prompt=true"), 1, "{events:?}");
}

/// A user record that carries a tool result is the harness answering a call, however much text
/// rides along with it; the result itself is its own event.
#[test]
fn a_user_record_that_carries_a_tool_result_is_not_a_prompt() {
    let (events, _) = read(&[user_blocks(json!([
        {"type": "tool_result", "tool_use_id": "t1", "content": "out"},
        {"type": "text", "text": "a reminder riding along"},
    ]))]);
    assert_eq!(
        events,
        [
            r#"result id=Some("t1") err=false "out""#,
            r#"user prompt=false ["a reminder riding along"]"#
        ]
    );
}

/// A prompt is a bare string or a block list of text only; both are read.
#[test]
fn a_text_only_block_array_is_a_prompt_like_a_string() {
    let (events, _) = read(&[
        user_blocks(json!([{"type": "text", "text": "one"}, {"type": "text", "text": "two"}])),
        user_text("three"),
        user_blocks(json!([{"type": "image", "source": {}}])),
    ]);
    assert_eq!(
        events,
        [r#"user prompt=true ["one", "two"]"#, r#"user prompt=true ["three"]"#],
        "a record with no text yields no user event"
    );
}

/// A subagent's records are another window: dropped before any consumer sees them, of every
/// kind, and COUNTED — a drop that is not displayed would read as a smaller session. They do not
/// date the session either.
#[test]
fn sidechain_records_are_dropped_and_counted() {
    let side_ts = |line: &str| sidechain(&stamped(line, "2026-01-01T00:00:00Z"));
    let (events, stats) = read(&[
        side_ts(&user_text("subagent prompt")),
        side_ts(&assistant("s1", "m", [9, 0, 0, 0], text("x"))),
        side_ts(&hook("subagent hook")),
        side_ts(&compact_boundary()),
        stamped(&user_text("main prompt"), "2026-11-01T10:00:00Z"),
    ]);
    assert_eq!(events, [r#"user prompt=true ["main prompt"]"#]);
    assert_eq!(stats.sidechain_dropped, 4);
    assert_eq!(stats.started_on.as_deref(), Some("2026-11-01"));
}

/// Claude Code writes one assistant record per content block, each repeating the call's usage:
/// the usage is handed out once per message id, every block is still delivered, and a record
/// with no message id is never billed.
#[test]
fn usage_is_handed_out_once_per_message_id() {
    let (events, _) = read(&[
        assistant("m1", "opus", [10, 0, 0, 5], text("a")),
        assistant("m1", "opus", [10, 0, 0, 5], text("b")),
        assistant("m2", "opus", [4, 0, 0, 1], text("c")),
        assistant_call("t1", "Bash", json!({})),
    ]);
    assert_eq!(
        events,
        [
            r#"assistant usage=opus/10 ["text:a"]"#,
            r#"assistant usage=none ["text:b"]"#,
            r#"assistant usage=opus/4 ["text:c"]"#,
            r#"assistant usage=none ["use:t1:Bash"]"#,
        ]
    );
}

/// A call repeated across the records of a message is one call. The id decides, in any record
/// of the session; a block with no id is never merged with another.
#[test]
fn a_tool_use_counts_once_per_id() {
    let call = |id: &str| json!([{"type": "tool_use", "id": id, "name": "Bash", "input": {}}]);
    let no_id = json!([{"type": "tool_use", "name": "Bash", "input": {}}]);
    let (events, _) = read(&[
        assistant("m1", "opus", [1, 0, 0, 0], call("a")),
        assistant("m1", "opus", [1, 0, 0, 0], call("a")),
        assistant("m2", "opus", [1, 0, 0, 0], call("a")),
        assistant("m2", "opus", [1, 0, 0, 0], call("b")),
        assistant("m3", "opus", [1, 0, 0, 0], no_id.clone()),
        assistant("m3", "opus", [1, 0, 0, 0], no_id),
    ]);
    let uses: Vec<&str> = events
        .iter()
        .flat_map(|e| e.split('"'))
        .filter(|p| p.starts_with("use:"))
        .collect();
    assert_eq!(uses, ["use:a:Bash", "use:b:Bash", "use:-:Bash", "use:-:Bash"]);
}

/// A result is text: a string as is, the `text` blocks joined with nothing, else the JSON.
#[test]
fn a_tool_result_is_flattened_to_one_text() {
    let result = |content: Value| {
        user_blocks(json!([{"type": "tool_result", "tool_use_id": "t", "is_error": true, "content": content}]))
    };
    let (events, _) = read(&[
        result(json!("plain")),
        result(json!([{"type": "text", "text": "a"}, {"type": "text", "text": "b"}])),
        result(json!([{"type": "image", "n": 1}])),
    ]);
    assert_eq!(
        events,
        [
            r#"result id=Some("t") err=true "plain""#,
            r#"result id=Some("t") err=true "ab""#,
            r#"result id=Some("t") err=true "[{\"n\":1,\"type\":\"image\"}]""#,
        ]
    );
}

/// Blocks keep their order; a bare-string content is one text; thinking is its own kind; and a
/// block the reader does not know is dropped.
#[test]
fn assistant_blocks_keep_their_order_and_kind() {
    let blocks = json!([
        {"type": "thinking", "thinking": "hm"},
        {"type": "text", "text": "hi"},
        {"type": "tool_use", "id": "t", "name": "Read", "input": {}},
        {"type": "server_tool_use"},
    ]);
    let bare = json!({"type": "assistant", "message": {"content": "just text"}}).to_string();
    let (events, _) = read(&[assistant("m", "opus", [1, 0, 0, 0], blocks), bare]);
    assert_eq!(
        events,
        [
            r#"assistant usage=opus/1 ["thinking:hm", "text:hi", "use:t:Read"]"#,
            r#"assistant usage=none ["text:just text"]"#,
        ]
    );
}

/// The records that are neither a turn nor a call: attachments hand over their payload, a
/// compaction and Claude Code's own cost total are events, any other system record is not.
#[test]
fn attachments_compactions_and_cost_state_are_events() {
    let other_system = json!({"type": "system", "subtype": "turn_duration"}).to_string();
    let (events, _) = read(&[
        hook("x"),
        compact_boundary(),
        other_system,
        cost_state(1.0, json!({})),
        json!({"type": "queue-operation"}).to_string(),
    ]);
    assert_eq!(events, [r#"attachment "hook_success""#, "compact", "cost-state"]);
}

/// A session is dated by its first record that carries a date; a line that is not JSON is
/// counted, never hidden.
#[test]
fn the_start_date_and_the_malformed_lines_are_reported() {
    let (_, stats) = read(&[
        user_text("undated"),
        "not json".to_string(),
        stamped(&user_text("a"), "not a date"),
        stamped(&user_text("b"), "2026-10-15T12:00:00.000Z"),
        stamped(&user_text("c"), "2026-11-03T08:00:00.000Z"),
    ]);
    assert_eq!(stats.started_on.as_deref(), Some("2026-10-15"));
    assert_eq!(stats.malformed_lines, 1);
    assert_eq!(read(&[user_text("x")]).1.started_on, None);
}

// ── provenance and the incremental door (C1, 04/11/2026) ────────────────────────────────────

/// A consumer that persists what it found (the miner's lessons) must say which session and
/// instant it came from: the record's `sessionId` and `timestamp` ride on the result event, and
/// a record that carries neither says so with `None` rather than an empty string.
#[test]
fn a_tool_result_carries_the_session_and_the_instant_of_its_record() {
    let mut stamped_rec: Value =
        serde_json::from_str(&call_result("t1", true, json!("boom"))).expect("json");
    stamped_rec["sessionId"] = json!("sess-1");
    stamped_rec["timestamp"] = json!("2026-11-04T10:00:00.000Z");
    let lines = [stamped_rec.to_string(), call_result("t2", false, json!("ok"))];
    let mut seen = Vec::new();
    read_lines(lines.iter().map(String::as_str), &mut |ev| {
        if let Event::ToolResult(r) = ev {
            seen.push((
                r.tool_use_id.map(str::to_string),
                r.session_id.map(str::to_string),
                r.timestamp.map(str::to_string),
            ));
        }
    });
    assert_eq!(
        seen,
        [
            (
                Some("t1".to_string()),
                Some("sess-1".to_string()),
                Some("2026-11-04T10:00:00.000Z".to_string())
            ),
            (Some("t2".to_string()), None, None),
        ]
    );
}

/// `LineReader` is `read_lines` taken one line at a time: the same events and the same stats,
/// and the once-per-id policy holds ACROSS pushes (a call repeated in a later record is still
/// one call), which is the whole point of keeping the reader's state between lines.
#[test]
fn a_line_reader_pushed_line_by_line_equals_read_lines() {
    let lines = vec![
        user_text("prompt"),
        assistant_call("dup", "Bash", json!({"command": "ls"})),
        "not json".to_string(),
        assistant_call("dup", "Bash", json!({"command": "ls"})),
        sidechain(&assistant_call("side", "Bash", json!({}))),
        call_result("dup", true, json!("boom")),
    ];
    let (whole_events, whole_stats) = read(&lines);

    let mut pushed_events = Vec::new();
    let mut reader = LineReader::new();
    for l in &lines {
        reader.push(l, &mut |ev| {
            pushed_events.push(match ev {
                Event::User(u) => format!("user {:?}", u.texts),
                Event::ToolResult(r) => format!("result {:?} {}", r.tool_use_id, r.is_error),
                Event::Assistant(a) => format!("assistant calls={}", a.tool_calls().count()),
                _ => "other".to_string(),
            });
        });
    }
    let pushed_stats = reader.finish();

    assert_eq!(pushed_stats, whole_stats, "stats are the walk's, however it is fed");
    assert_eq!(pushed_stats.malformed_lines, 1);
    assert_eq!(pushed_stats.sidechain_dropped, 1);
    assert_eq!(pushed_events.len(), whole_events.len(), "{pushed_events:?}");
    let calls: usize = pushed_events.iter().filter(|e| e.as_str() == "assistant calls=1").count();
    assert_eq!(calls, 1, "the repeated tool_use id is handed out once: {pushed_events:?}");
}
