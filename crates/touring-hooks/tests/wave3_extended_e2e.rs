//! E2E — Wave 3 INTELLIGENCE Extended (T2 + T3) cross-audit.
//! 25 audits proving each envelope contract from the data-driven plan.

#![cfg(feature = "tantivy-fts")]

use serde_json::json;
use touring_hooks::wave3_extended as w3;

#[test]
fn audit_t201_linucb_compression() {
    let v = w3::ctx_linucb_compression("Bash");
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["tool_name"], json!("Bash"));
}

#[test]
fn audit_t202_burn_detect() {
    let v = w3::ctx_burn_detect();
    assert_eq!(v["ok"], json!(true));
    assert!(v["warning"].is_boolean());
    assert_eq!(v["threshold"], json!(5));
}

#[test]
fn audit_t203_precompact_recompress() {
    let v = w3::ctx_precompact_recompress();
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["operation"], json!("precompact_aggressive"));
}

#[test]
fn audit_t204_session_inheritance() {
    let v = w3::ctx_session_inheritance();
    assert_eq!(v["ok"], json!(true));
    assert!(v["lessons_to_replay"].is_array());
}

#[test]
fn audit_t205_search_regex() {
    let v = w3::ctx_search_regex("parse_.*", 10);
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["pattern"], json!("parse_.*"));
    assert_eq!(v["top_k"], json!(10));
}

#[test]
fn audit_t206_search_phrase_prefix() {
    let v = w3::ctx_search_phrase_prefix("build sym", 5);
    assert_eq!(v["ok"], json!(true));
}

#[test]
fn audit_t207_aggregate_daily() {
    let v = w3::ctx_aggregate_daily("stored_at_dt", 7);
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["days"], json!(7));
}

#[test]
fn audit_t208_top_compressors() {
    let v = w3::ctx_top_compressors(5);
    assert_eq!(v["ok"], json!(true));
    let names = v["top_compressors"].as_array().unwrap();
    assert!(names.len() <= 5);
}

#[test]
fn audit_t209_think_in_code_triggers_on_keywords() {
    let v = w3::ctx_think_in_code("analyze the project for X");
    assert_eq!(v["should_inject"], json!(true));
    let dir = v["directive"].as_str().unwrap();
    assert!(dir.contains("THINK IN CODE"));
}

#[test]
fn audit_t209_think_in_code_skips_unrelated() {
    let v = w3::ctx_think_in_code("rename function X to Y");
    assert_eq!(v["should_inject"], json!(false));
}

#[test]
fn audit_t210_roi_sonnet_pricing() {
    let v = w3::ctx_roi("sonnet");
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["model"], json!("sonnet"));
    assert!(v["usd_saved"].as_str().unwrap().starts_with('$'));
}

#[test]
fn audit_t211_read_summary_threshold() {
    let small = "abc";
    // Realistic multi-line content: 1000 lines × ~20 bytes ≈ 20kB
    let big: String = (0..1000).map(|i| format!("line_{i:05}_data\n")).collect();
    let vs = w3::ctx_read_summary(small);
    let vb = w3::ctx_read_summary(&big);
    assert_eq!(vs["original_bytes"], json!(3));
    assert!(
        vb["summarized_bytes"].as_u64().unwrap() < vb["original_bytes"].as_u64().unwrap(),
        "expected summary smaller than original on multi-line content"
    );
    assert_eq!(vb["threshold"], json!(10240));
}

#[test]
fn audit_t212_webfetch_chunk() {
    let md = "intro\n## H2 one\nfoo\n## H2 two\nbar\n## H2 three\nbaz";
    let v = w3::ctx_webfetch_chunk(md);
    assert_eq!(v["ok"], json!(true));
    assert!(v["chunk_count"].as_u64().unwrap() >= 3);
}

#[test]
fn audit_t213_grep_rerank_keeps_top_n() {
    let hits = vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
        "d".to_string(),
    ];
    let v = w3::ctx_grep_rerank(&hits, 2);
    assert_eq!(v["original_count"], json!(4));
    assert_eq!(v["top_n"], json!(2));
    assert_eq!(v["dropped"], json!(2));
}

#[test]
fn audit_t214_err_filter_keeps_errors() {
    let raw = "info: ok\nerror: failed\nwarning: deprecated\ndone\n";
    let v = w3::ctx_err_filter(raw, 0);
    assert_eq!(v["ok"], json!(true));
    let kept = v["filtered"].as_str().unwrap();
    assert!(kept.contains("error"));
    assert!(kept.contains("warning"));
}

#[test]
fn audit_t215_tier_lift_at_3() {
    let v = w3::ctx_tier_lift(3);
    assert_eq!(v["lift_active"], json!(true));
    assert_eq!(v["diagnostic_code"], json!("Q-312"));
}

#[test]
fn audit_t215_tier_lift_below_3_inactive() {
    let v = w3::ctx_tier_lift(2);
    assert_eq!(v["lift_active"], json!(false));
}

#[test]
fn audit_t301_lsp_status() {
    let v = w3::ctx_lsp_status();
    assert_eq!(v["ok"], json!(true));
    let caps = v["supported_capabilities"].as_array().unwrap();
    assert!(!caps.is_empty());
}

#[test]
fn audit_t302_shared_cache_status() {
    let v = w3::ctx_shared_cache_status();
    assert_eq!(v["ok"], json!(true));
    assert!(v["hit_count"].is_number());
}

#[test]
fn audit_t303_cloud_sync_status() {
    let v = w3::ctx_cloud_sync_status();
    assert_eq!(v["ok"], json!(true));
}

#[test]
fn audit_t304_synthesize_profile() {
    let v = w3::ctx_synthesize_profile("cargo nextest");
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["opportunity"], json!("cargo nextest"));
}

#[test]
fn audit_t305_web_status() {
    let v = w3::ctx_web_status();
    assert_eq!(v["ok"], json!(true));
}

#[test]
fn audit_t306_federated_search() {
    let v = w3::ctx_federated_search("touring");
    assert_eq!(v["ok"], json!(true));
    assert!(v["hits"].is_array());
}

#[test]
fn audit_t307_otlp_status() {
    let v = w3::ctx_otlp_status();
    assert_eq!(v["ok"], json!(true));
}

#[test]
fn audit_t308_graphql_status() {
    let v = w3::ctx_graphql_status();
    assert_eq!(v["ok"], json!(true));
}

#[test]
fn audit_t309_polyglot_summary() {
    let v = w3::ctx_polyglot_summary("/tmp/foo.py");
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["language"], json!("python"));
}

#[test]
fn audit_t310_pr_risk_score() {
    let v = w3::ctx_pr_risk_score(42);
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["pr_number"], json!(42));
    assert!(v["risk_score"].is_number());
}

#[test]
fn audit_full_pipeline_25_t2_t3() {
    // Drive every T2 + T3 envelope in a single integrated flow.
    assert_eq!(w3::ctx_linucb_compression("Bash")["ok"], json!(true));
    assert_eq!(w3::ctx_burn_detect()["ok"], json!(true));
    assert_eq!(w3::ctx_precompact_recompress()["ok"], json!(true));
    assert_eq!(w3::ctx_session_inheritance()["ok"], json!(true));
    assert_eq!(w3::ctx_search_regex("p.*", 5)["ok"], json!(true));
    assert_eq!(w3::ctx_search_phrase_prefix("p", 5)["ok"], json!(true));
    assert_eq!(w3::ctx_aggregate_daily("f", 5)["ok"], json!(true));
    assert_eq!(w3::ctx_top_compressors(5)["ok"], json!(true));
    assert_eq!(w3::ctx_think_in_code("p")["ok"], json!(true));
    assert_eq!(w3::ctx_roi("sonnet")["ok"], json!(true));
    assert_eq!(w3::ctx_read_summary("p")["ok"], json!(true));
    assert_eq!(w3::ctx_webfetch_chunk("p")["ok"], json!(true));
    let hits = vec!["a".to_string()];
    assert_eq!(w3::ctx_grep_rerank(&hits, 1)["ok"], json!(true));
    assert_eq!(w3::ctx_err_filter("e", 0)["ok"], json!(true));
    assert_eq!(w3::ctx_tier_lift(1)["ok"], json!(true));
    assert_eq!(w3::ctx_lsp_status()["ok"], json!(true));
    assert_eq!(w3::ctx_shared_cache_status()["ok"], json!(true));
    assert_eq!(w3::ctx_cloud_sync_status()["ok"], json!(true));
    assert_eq!(w3::ctx_synthesize_profile("o")["ok"], json!(true));
    assert_eq!(w3::ctx_web_status()["ok"], json!(true));
    assert_eq!(w3::ctx_federated_search("q")["ok"], json!(true));
    assert_eq!(w3::ctx_otlp_status()["ok"], json!(true));
    assert_eq!(w3::ctx_graphql_status()["ok"], json!(true));
    assert_eq!(w3::ctx_polyglot_summary("/tmp/x.rs")["ok"], json!(true));
    assert_eq!(w3::ctx_pr_risk_score(1)["ok"], json!(true));
}
