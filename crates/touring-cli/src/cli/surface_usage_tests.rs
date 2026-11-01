//! Tests of the `surface_usage` report (B3, 01/11/2026): what the harness ANNOUNCES to the
//! model against what was USED. Report only — nothing here prunes or writes.

use super::*;
use crate::cli::transcript_scan::fixtures::*;
use serde_json::json;

/// A marker planted in the prose of every announcement the fixtures carry.
const CANARY: &str = "CANARIO-NAO-VAZAR";

fn tracker(lines: &[String]) -> SurfaceTracker {
    let mut t = SurfaceTracker::default();
    t.observe_lines(lines.iter().map(String::as_str));
    t
}

fn report(lines: &[String]) -> Value {
    surface_report(&[&tracker(lines)])
}

fn typed(name: &str) -> String {
    user_text(&format!(
        "<command-message>{name}</command-message>\n<command-name>/{name}</command-name>"
    ))
}

fn announce_all() -> Vec<String> {
    vec![
        skill_listing(&["a", "b", "plug:c"], "- a: x\n- b: y\n- plug:c: z"),
        agent_listing(&["explorer", "planner"], &["- explorer: reads", "- planner: plans"]),
        deferred_tools(&["T1", "T2"]),
    ]
}

/// The F4d semantics, now with the typed `/command` the offline pass could not see: an item
/// is USED when the model called it (Skill / Agent / its own tool name) or the person typed it.
#[test]
fn announced_and_used_are_counted_per_skill_agent_and_tool() {
    let mut lines = announce_all();
    lines.extend([
        assistant("m1", "claude-opus-5-5", [1, 0, 0, 1], tool_use("Skill", json!({"skill": "a"}))),
        assistant(
            "m2",
            "claude-opus-5-5",
            [1, 0, 0, 1],
            tool_use("Agent", json!({"subagent_type": "explorer"})),
        ),
        assistant("m3", "claude-opus-5-5", [1, 0, 0, 1], tool_use("T1", json!({}))),
        assistant("m4", "claude-opus-5-5", [1, 0, 0, 1], tool_use("Bash", json!({}))),
        typed("plug:c"),
        typed("effort"),
    ]);
    let r = report(&lines);
    let s = &r["skills"];
    assert_eq!(s["announced"], 3);
    assert_eq!(s["used"], 2, "a by the model, plug:c by the person");
    assert_eq!(s["unused"], 1);
    assert_eq!(s["used_not_announced"], 1, "`effort` is a built-in command, never listed");
    assert_eq!(s["uses"]["tool"], 1);
    assert_eq!(s["uses"]["typed"], 2);
    assert_eq!(s["top_unused"][0]["name"], "b");

    let a = &r["agents"];
    assert_eq!((a["announced"].as_u64(), a["used"].as_u64(), a["unused"].as_u64()), (Some(2), Some(1), Some(1)));
    assert_eq!(a["top_unused"][0]["name"], "planner");

    let t = &r["tools"];
    assert_eq!((t["announced"].as_u64(), t["used"].as_u64(), t["unused"].as_u64()), (Some(2), Some(1), Some(1)));
    assert_eq!(t["used_not_announced"], 3, "Skill, Agent and Bash are always loaded");
}

/// An `Agent` call with no `subagent_type` runs the general-purpose agent.
#[test]
fn agent_without_subagent_type_counts_as_general_purpose() {
    let r = report(&[
        agent_listing(&["general-purpose"], &["- general-purpose: any task"]),
        assistant("m1", "claude-opus-5-5", [1, 0, 0, 1], tool_use("Agent", json!({}))),
    ]);
    assert_eq!(r["agents"]["used"], 1);
    assert_eq!(r["agents"]["unused"], 0);
}

/// The biggest announcement is the first candidate: the list is ordered by bytes, then name.
#[test]
fn unused_items_are_ranked_by_announcement_bytes() {
    let big = format!("- big: {}", "d".repeat(500));
    let content = format!("- small: s\n{big}\n- mid: {}", "m".repeat(50));
    let r = report(&[skill_listing(&["small", "big", "mid"], &content)]);
    let names: Vec<&str> = r["skills"]["top_unused"]
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|i| i["name"].as_str())
        .collect();
    assert_eq!(names, ["big", "mid", "small"]);
    let first = &r["skills"]["top_unused"][0];
    assert_eq!(first["announce_bytes"], big.len() as u64 + 1);
    assert_eq!(first["announce_tokens_est"], (big.len() as u64 + 1) as f64 * 0.25);
    assert_eq!(r["skills"]["unused_share"], 1.0);
}

/// Bytes add up over every announcement the person's sessions paid for — a listing resent in
/// the same session counts again — and `sessions_announced` counts sessions, not sends.
#[test]
fn announcement_bytes_are_summed_over_sessions_and_repeats() {
    let listing = skill_listing(&["a"], "- a: x");
    let s1 = tracker(&[listing.clone(), listing.clone()]);
    let s2 = tracker(&[listing]);
    let r = surface_report(&[&s1, &s2]);
    let a = &r["skills"]["top_unused"][0];
    assert_eq!(a["sessions_announced"], 2);
    assert_eq!(a["announce_bytes"], 3 * ("- a: x".len() as u64 + 1));
    assert_eq!(r["sessions"], 2);
    assert_eq!(
        r["skills"]["announce_tokens_est_per_session"],
        3.0 * ("- a: x".len() as f64 + 1.0) * 0.25 / 2.0
    );
}

/// An item used in ANY session is used for the project (the F4d replay's `announced ∩ used`).
#[test]
fn an_item_used_in_one_session_is_used_for_the_project() {
    let ann = skill_listing(&["a", "b"], "- a: x\n- b: y");
    let s1 = tracker(&[ann.clone()]);
    let s2 = tracker(&[
        ann,
        assistant("m1", "claude-opus-5-5", [1, 0, 0, 1], tool_use("Skill", json!({"skill": "b"}))),
    ]);
    let r = surface_report(&[&s1, &s2]);
    assert_eq!(r["skills"]["used"], 1);
    assert_eq!(r["skills"]["unused"], 1);
    assert_eq!(r["skills"]["top_used"][0]["name"], "b");
    assert_eq!(r["skills"]["top_used"][0]["sessions_used"], 1);
}

/// `invoked_skills` is a third signal (it also lists what a compaction carried over), so it
/// can mark a skill used but is never counted as another use.
#[test]
fn invoked_skills_attachment_marks_used_without_counting_a_use() {
    let att = json!({"type": "attachment", "attachment": {
        "type": "invoked_skills", "skills": [{"name": "a", "path": "/x", "content": "body"}]}})
    .to_string();
    let r = report(&[skill_listing(&["a", "b"], "- a: x\n- b: y"), att]);
    assert_eq!(r["skills"]["used"], 1);
    assert_eq!(r["skills"]["uses"]["tool"], 0);
    assert_eq!(r["skills"]["uses"]["typed"], 0);
    assert_eq!(r["skills"]["used_via_attachment"], 1);
}

/// MCP server instructions are announcement cost without a per-item use signal here; they
/// are reported as bytes only.
#[test]
fn mcp_instructions_are_reported_as_bytes() {
    let r = report(&[mcp_instructions(&["touring", "other"], &["## touring\nabc", "## other\nde"])]);
    let bytes = ("## touring\nabc".len() + "## other\nde".len()) as u64;
    assert_eq!(r["mcp_instructions"]["servers"], 2);
    assert_eq!(r["mcp_instructions"]["bytes"], bytes);
    assert_eq!(r["mcp_instructions"]["tokens_est"], bytes as f64 * 0.25);
}

/// Names and numbers only: the prose of an announcement and the text of a prompt stay out.
#[test]
fn report_never_contains_transcript_text() {
    let r = report(&[
        skill_listing(&["a"], &format!("- a: {CANARY}")),
        user_text(&format!("{CANARY} prompt")),
        assistant("m1", "claude-opus-5-5", [1, 0, 0, 1], tool_use("Bash", json!({"command": CANARY}))),
    ]);
    assert!(!r.to_string().contains(CANARY), "{r}");
}

#[test]
fn sidechain_records_are_skipped() {
    let mut side: Value = serde_json::from_str(&assistant(
        "s1",
        "claude-opus-5-5",
        [1, 0, 0, 1],
        tool_use("Skill", json!({"skill": "a"})),
    ))
    .expect("json");
    side["isSidechain"] = json!(true);
    let r = report(&[skill_listing(&["a"], "- a: x"), side.to_string()]);
    assert_eq!(r["skills"]["used"], 0, "a subagent's use is not the main session's");
}

/// A session with no announcement is shown as zero announced, never as a missing report.
#[test]
fn empty_transcript_reports_zero_not_absence() {
    let r = report(&[user_text("hello")]);
    assert_eq!(r["skills"]["announced"], 0);
    assert!(r["skills"]["unused_share"].is_null(), "0/0 is not a share");
}
