//! Tests of the `context_cost` ruler (B5, 01/11/2026). Every number is arithmetic a reader
//! can redo by hand: `TOKENS_PER_BYTE` is 0.25, so a window byte count divides by 4 exactly.

use super::*;
use crate::cli::transcript_scan::fixtures::*;
use serde_json::json;

const OPUS: &str = "claude-opus-5-5";
/// A marker planted in every transcript text the fixtures carry.
const CANARY: &str = "CANARIO-NAO-VAZAR";

fn summary(lines: &[String]) -> CostSummary {
    let mut t = CostTracker::default();
    t.observe_lines(lines.iter().map(String::as_str));
    t.finish()
}

fn payload(lines: &[String]) -> Value {
    cost_payload(&summary(lines))
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Claude Code writes one assistant record PER content block, each repeating the message's
/// usage. Summing records would bill one API call once per block.
#[test]
fn tokens_are_deduped_by_message_id_and_split_by_kind() {
    let p = payload(&[
        assistant("m1", OPUS, [10, 100, 1000, 50], text("hi")),
        assistant("m1", OPUS, [10, 100, 1000, 50], tool_use("Bash", json!({}))),
        assistant("m2", OPUS, [5, 0, 2000, 20], text("again")),
    ]);
    assert_eq!(p["calls"], 2);
    assert_eq!(p["tokens"]["input"], 15);
    assert_eq!(p["tokens"]["cache_creation"], 100);
    assert_eq!(p["tokens"]["cache_read"], 3000);
    assert_eq!(p["tokens"]["output"], 70);
    assert_eq!(p["tokens"]["input_total"], 3115, "input + cache write + cache read");
}

/// The window is cumulative: a block that entered at call 1 is re-sent at call 2. Each
/// category is its bytes × 0.25, summed over the calls that carried it.
#[test]
fn window_bytes_are_attributed_to_hooks_announcements_tool_results_and_the_rest() {
    let listing = "- a: x\n- b: y";
    let hook_text = "[TOURING SUGGEST]\n  MUST touring doctor -j\n";
    let p = payload(&[
        skill_listing(&["a", "b"], listing),
        hook(hook_text),
        user_text("hello"),
        assistant("m1", OPUS, [1000, 0, 0, 10], text("abcd")),
        user_result(&"r".repeat(40)),
        assistant("m2", OPUS, [2000, 0, 0, 10], text("done")),
    ]);
    let a = &p["attribution"];
    // announce + hook + user_text are in the window at BOTH calls
    let two = |bytes: usize| 2.0 * bytes as f64 * 0.25;
    assert!(close(num(&a["announcement"]["tokens"]), two(listing.len())));
    assert!(close(
        num(&a["announcement"]["by_kind"]["skill_listing"]["tokens"]),
        two(listing.len())
    ));
    assert!(close(num(&a["injected_hooks"]["tokens"]), two(hook_text.len())));
    assert!(close(
        num(&a["injected_hooks"]["by_family"]["touring-suggest"]["tokens"]),
        two(hook_text.len())
    ));
    assert!(close(num(&a["rest"]["parts"]["user_text"]), two(5)));
    // the tool result (40 B) and the first answer (4 B) enter only before call 2
    assert!(close(num(&a["tool_results"]["tokens"]), 40.0 * 0.25));
    assert!(close(num(&a["rest"]["parts"]["assistant_output"]), 4.0 * 0.25));
    assert_eq!(p["reconciliation"]["measured_input_tokens"], 3000);
}

/// Compaction empties the window: what came before is no longer re-sent.
#[test]
fn compaction_resets_the_window() {
    let p = payload(&[
        user_text(&"x".repeat(8)),
        assistant("m1", OPUS, [100, 0, 0, 1], text("")),
        assistant("m2", OPUS, [100, 0, 0, 1], text("")),
        compact_boundary(),
        user_text(&"s".repeat(4)),
        assistant("m3", OPUS, [100, 0, 0, 1], text("")),
    ]);
    // 2 calls × 8 B × 0.25  +  1 call × 4 B × 0.25
    assert!(close(num(&p["attribution"]["rest"]["parts"]["user_text"]), 5.0));
    assert_eq!(p["compactions"], 1);
}

/// The residual is the unseen prefix (system prompt, tool schemas), never negative. The one
/// way the identity fails is OVER-attribution — more tokens attributed than the call billed —
/// and that, and only that, is the reconciliation error.
#[test]
fn unseen_prefix_is_the_residual_and_overshoot_is_the_reconciliation_error() {
    // 400 B of user text = 100 estimated tokens against a call that billed 500
    let under = payload(&[
        user_text(&"x".repeat(400)),
        assistant("m1", OPUS, [500, 0, 0, 1], text("")),
    ]);
    assert_eq!(under["reconciliation"]["unseen_tokens"], 400.0);
    assert!(close(num(&under["reconciliation"]["unseen_share"]), 0.8));
    assert_eq!(under["reconciliation_error"], 0.0);
    assert_eq!(under["reconciliation"]["within_tolerance"], true);

    // the same 100 estimated tokens against a call that billed only 60: 40 over
    let over = payload(&[
        user_text(&"x".repeat(400)),
        assistant("m1", OPUS, [60, 0, 0, 1], text("")),
    ]);
    assert_eq!(over["reconciliation"]["unseen_tokens"], 0.0);
    assert!(close(num(&over["reconciliation_error"]), 40.0 / 60.0));
    assert_eq!(over["reconciliation"]["within_tolerance"], false);
    assert!(close(num(&over["reconciliation"]["attributed_tokens"]), 100.0));
}

/// Dollars come from a price table keyed by model id. The cache write is billed at the 5 min
/// or the 1 h rate by the `cache_creation` split the transcript carries.
#[test]
fn priced_model_yields_exact_dollars_with_the_cache_write_split() {
    let p = payload(&[
        assistant("m1", OPUS, [1_000_000, 0, 0, 100_000], text("")),
        assistant_cc("m2", OPUS, [0, 1_000_000, 0, 0], Some(400_000), text("")),
        assistant("m3", OPUS, [0, 0, 1_000_000, 0], text("")),
    ]);
    // 4.00 in + 2.00 out | 600k × 5.00 + 400k × 8.00 | 1M × 0.20
    let want = 6.0 + (3.0 + 3.2) + 0.2;
    assert!(close(num(&p["cost"]["usd"]), want), "{}", p["cost"]);
    assert_eq!(p["cost"]["unpriced_calls"], 0);
    assert_eq!(p["cost"]["unpriced_models"], json!([]));
    assert_eq!(p["cost"]["by_model"][OPUS]["priced"], true);
}

/// An unknown model never gets a guessed price: the dollar figure is null and the model is
/// named, so nobody reads a partial sum as the bill.
#[test]
fn unknown_model_makes_the_dollar_figure_null_and_is_listed() {
    let p = payload(&[
        assistant("m1", OPUS, [1_000_000, 0, 0, 0], text("")),
        assistant("m2", "k3", [1_000_000, 0, 0, 0], text("")),
    ]);
    assert!(p["cost"]["usd"].is_null(), "{}", p["cost"]);
    assert_eq!(p["cost"]["unpriced_models"], json!(["k3"]));
    assert_eq!(p["cost"]["unpriced_calls"], 1);
    assert_eq!(p["cost"]["priced_calls"], 1);
    assert!(close(num(&p["cost"]["usd_priced_calls"]), 4.0));
    assert_eq!(p["cost"]["by_model"]["k3"]["priced"], false);
    assert!(p["cost"]["by_model"]["k3"]["usd"].is_null());
    // tokens are measured whatever the price
    assert_eq!(p["tokens"]["input_total"], 2_000_000);
}

/// A non-standard speed is billed at a rate this table does not carry: unpriced, not guessed.
#[test]
fn fast_mode_is_unpriced_not_guessed() {
    let mut rec: Value =
        serde_json::from_str(&assistant("m1", OPUS, [1000, 0, 0, 0], text(""))).expect("json");
    rec["message"]["usage"]["speed"] = json!("fast");
    let p = payload(&[rec.to_string()]);
    assert!(p["cost"]["usd"].is_null());
    assert_eq!(p["cost"]["unpriced_models"], json!([format!("{OPUS}@fast")]));
}

/// Subagent records and the synthetic zero-usage record are not main-window calls.
#[test]
fn sidechain_and_synthetic_records_are_not_counted() {
    let mut side: Value =
        serde_json::from_str(&assistant("s1", OPUS, [999, 0, 0, 0], text(""))).expect("json");
    side["isSidechain"] = json!(true);
    let p = payload(&[
        side.to_string(),
        assistant("syn", "<synthetic>", [0, 0, 0, 0], text("")),
        assistant("m1", OPUS, [7, 0, 0, 0], text("")),
    ]);
    assert_eq!(p["calls"], 1);
    assert_eq!(p["tokens"]["input"], 7);
    assert_eq!(p["sidechain_records_skipped"], 1);
}

/// `cost-state` is Claude Code's own running total, an INDEPENDENT measure: it also holds
/// the subagents and side calls this transcript does not, so the transcript's share of it is
/// the honest coverage of this ruler (and it validates the price table where it reaches 1).
#[test]
fn cost_state_is_read_and_compared_by_model() {
    let p = payload(&[
        assistant("m1", OPUS, [1_000_000, 0, 0, 0], text("")),
        cost_state(
            10.0,
            json!({
                "claude-opus-5-5[1m]": {
                    "inputTokens": 2_000_000, "outputTokens": 0,
                    "cacheReadInputTokens": 0, "cacheCreationInputTokens": 0, "costUSD": 8.0
                },
                "haiku": {
                    "inputTokens": 5, "outputTokens": 5,
                    "cacheReadInputTokens": 0, "cacheCreationInputTokens": 0, "costUSD": 2.0
                }
            }),
        ),
    ]);
    let cs = &p["cost_state"];
    assert_eq!(cs["total_cost_usd"], 10.0);
    // the all-model ratio is the coverage that survives a model-label mismatch between the two
    assert_eq!(cs["input_total_tokens"], 2_000_005);
    assert!(close(num(&cs["input_total_ratio"]), 1_000_000.0 / 2_000_005.0), "{cs}");
    let opus = &cs["by_model"][OPUS];
    assert!(close(num(&opus["input_total_ratio"]), 0.5), "{opus}");
    assert!(close(num(&opus["usd_ratio"]), 0.5), "{opus}");
    assert!(cs["by_model"]["haiku"]["input_total_ratio"].is_null(), "not in this transcript");
}

#[test]
fn a_transcript_without_cost_state_says_so() {
    let p = payload(&[assistant("m1", OPUS, [1, 0, 0, 0], text(""))]);
    assert!(p["cost_state"].is_null());
}

/// The payload carries counts, family names and numbers — never a byte of the transcript.
#[test]
fn payload_never_contains_transcript_text() {
    let p = payload(&[
        skill_listing(&["a"], &format!("- a: {CANARY}")),
        hook(&format!("[TOURING SUGGEST]\n  MUST x {CANARY}\n")),
        user_text(&format!("{CANARY} user")),
        assistant("m1", OPUS, [100, 0, 0, 1], text(CANARY)),
        user_result(&format!("{CANARY} result")),
        assistant("m2", OPUS, [200, 0, 0, 1], tool_use("Bash", json!({"command": CANARY}))),
    ]);
    assert!(!p.to_string().contains(CANARY), "{p}");
}

/// The IO shell: a project aggregate plus one row per session, newest first, with the scan's
/// own limits shown.
#[test]
fn project_payload_has_one_row_per_session_and_names_the_scan() {
    let root = Path::new("/home/x/proj");
    let home = tempfile::tempdir().expect("tempdir");
    let dir = crate::cli::context_budget::transcript_dir_for(home.path(), root);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for id in ["s1", "s2"] {
        let body = [
            user_text("hello"),
            assistant(&format!("m-{id}"), OPUS, [100, 0, 0, 1], text("x")),
        ]
        .join("\n");
        std::fs::write(dir.join(format!("{id}.jsonl")), body).expect("write");
    }
    let (cost, surface) = context_cost_and_surface_in(home.path(), root, &KPI_CAPS);
    assert_eq!(cost["available"], true);
    assert_eq!(cost["sessions_scanned"], 2);
    assert_eq!(cost["by_session"].as_array().map(Vec::len), Some(2));
    assert_eq!(cost["calls"], 2);
    assert_eq!(cost["constants"]["bytes_per_token"], 4);
    assert_eq!(surface["available"], true);
    assert_eq!(surface["sessions_scanned"], 2);
}

#[test]
fn absent_transcript_directory_is_displayed_not_zero() {
    let home = tempfile::tempdir().expect("tempdir");
    let (cost, surface) =
        context_cost_and_surface_in(home.path(), Path::new("/home/x/none"), &KPI_CAPS);
    assert_eq!(cost["available"], false);
    assert_eq!(surface["available"], false);
    assert!(cost["reason"].as_str().is_some_and(|r| r.contains("no transcript directory")));
}

/// A price entry never exists half-filled: the table is the single place a model becomes
/// priced, and the five rates must be positive.
#[test]
fn every_price_row_is_complete_and_keyed_by_a_unique_model() {
    let mut seen = BTreeSet::new();
    for p in PRICES {
        assert!(seen.insert(p.model), "duplicate {}", p.model);
        for rate in [p.input, p.output, p.cache_read, p.cache_write_5m, p.cache_write_1h] {
            assert!(rate > 0.0, "{} has a non-positive rate", p.model);
        }
    }
    assert!(price_for("claude-opus-5-5").is_some());
    assert!(price_for("k3").is_none());
    assert!(price_for("claude-opus-5-5[1m]").is_none(), "exact id only");
}
