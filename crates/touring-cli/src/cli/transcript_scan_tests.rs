//! Tests of the shared transcript walker and record parsers (B5/B3, 01/11/2026).

use super::fixtures::*;
use super::*;
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
        std::fs::write(&p, "{\"type\":\"user\"}\n{\"type\":\"user\"}\nnot json\n").expect("write");
        let t = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000 + i as u64);
        let f = std::fs::File::options().write(true).open(&p).expect("open");
        f.set_modified(t).expect("mtime");
    }
    let caps = ScanCaps { max_transcripts: 2, max_bytes: u64::MAX };
    let scan = scan_dir::<u64>(dir.path(), &caps, &mut |n, _| *n += 1).expect("scan");
    let ids: Vec<&str> = scan.sessions.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["new", "mid"]);
    assert_eq!(scan.sessions[0].state, 2, "two parseable records");
    assert_eq!(scan.sessions[0].malformed_lines, 1, "the bad line is counted, not hidden");
    assert_eq!(scan.skipped_by_cap, 1);
    assert!(!scan.capped, "the byte cap was not hit");
}

#[test]
fn scan_dir_stops_at_the_byte_cap_and_says_so() {
    let dir = tempfile::tempdir().expect("tempdir");
    for id in ["a", "b"] {
        std::fs::write(dir.path().join(format!("{id}.jsonl")), "{\"type\":\"user\"}\n")
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
