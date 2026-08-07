//! E2E — Wave 3 INTELLIGENCE cross-audit (15 T1 initiatives).
//!
//! Each `audit_t1XX_*` proves a contract from the data-driven plan in
//! `~/.claude/plans/2026-05-08-wave3-intelligence-plan.md`. Tests are
//! independent, deterministic, and exercise the public surface added by
//! Wave 3 — covering: ctx_replay, ctx_purge, ctx_doctor, ctx_gain_history,
//! ctx_gain_graph, ctx_session_adoption, ctx_init_agent, ctx_smart,
//! ctx_chunk_read, ctx_explain, ctx_budget, ctx_batch_execute,
//! ctx_execute_file, ctx_upgrade, ctx_discover_session.

#![cfg(feature = "tantivy-fts")]

use serde_json::json;
use touring_hooks::cli_handlers_mcp::{
    self as mcp, CTX_MCP_TOOL_NAMES, PurgeTargets, ctx_mcp_tool_count,
};

/// Helper: resolve absolute path to this test file (file!() is relative).
fn this_test_file() -> String {
    format!(
        "{}/tests/wave3_intelligence_e2e.rs",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
fn audit_t101_replay_returns_envelope() {
    let v = mcp::ctx_replay(5);
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["n_requested"], json!(5));
    assert!(v["n_capped"].as_u64().unwrap() <= 50);
    assert!(v["feature_flag"].as_str().is_some());
}

#[test]
fn audit_t101_replay_caps_at_50() {
    let v = mcp::ctx_replay(1000);
    assert_eq!(v["n_capped"], json!(50));
}

#[test]
fn audit_t102_purge_targets_all() {
    let v = mcp::ctx_purge(None, PurgeTargets::all());
    assert_eq!(v["ok"], json!(true));
    assert!(v["removed"].is_object());
    assert!(v["preserved"]["memory_semantic"].as_str().is_some());
}

#[test]
fn audit_t102_purge_targets_partial_default() {
    let v = mcp::ctx_purge(None, PurgeTargets::default());
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["removed"]["tee_logs"], json!(0));
    assert_eq!(v["removed"]["tool_outputs_index"], json!(0));
}

#[test]
fn audit_t103_doctor_reports_components() {
    let v = mcp::ctx_doctor(None);
    assert_eq!(v["ok"], json!(true));
    let comps = v["components"].as_array().expect("components is array");
    assert!(comps.len() >= 3, "doctor should report ≥3 components");
    for c in comps {
        assert!(c["name"].as_str().is_some());
        assert!(c["status"].as_str().is_some());
    }
}

#[test]
fn audit_t104_history_returns_rows_and_caps_at_30() {
    let v = mcp::ctx_gain_history(7);
    assert_eq!(v["ok"], json!(true));
    assert!(v["rows"].is_array());
    let big = mcp::ctx_gain_history(100);
    assert_eq!(big["days_requested"], json!(30), "max 30");
}

#[test]
fn audit_t105_graph_renders_sparkline() {
    let v = mcp::ctx_gain_graph(7);
    assert_eq!(v["ok"], json!(true));
    let bar = v["sparkline"].as_str().expect("sparkline");
    assert_eq!(bar.chars().count(), 7);
}

#[test]
fn audit_t105_graph_floor_one_day() {
    let v = mcp::ctx_gain_graph(0);
    assert!(v["sparkline"].as_str().unwrap().chars().count() >= 1);
}

#[test]
fn audit_t106_session_adoption_returns_ratio() {
    let v = mcp::ctx_session_adoption();
    assert_eq!(v["ok"], json!(true));
    assert!(v["total"].as_u64().is_some());
    assert!(v["ratio"].as_f64().is_some());
}

#[test]
fn audit_t107_init_agent_known_returns_plan() {
    let v = mcp::ctx_init_agent("claude-code");
    assert_eq!(v["ok"], json!(true));
    assert!(v["plan"]["config_path"].as_str().is_some());
}

#[test]
fn audit_t107_init_agent_unknown_fails_gracefully() {
    let v = mcp::ctx_init_agent("nonexistent-agent-xyz");
    assert_eq!(v["ok"], json!(false));
    assert!(v["plan"]["error"].as_str().is_some());
}

#[test]
fn audit_t108_smart_two_lines() {
    // Use this very test file as input
    let path = this_test_file();
    let v = mcp::ctx_smart(&path);
    assert_eq!(v["ok"], json!(true));
    let summary = v["summary"].as_str().expect("summary");
    assert_eq!(summary.lines().count(), 2);
    assert!(v["line1"].as_str().unwrap().contains("LOC"));
}

#[test]
fn audit_t108_smart_missing_file_fails_gracefully() {
    let v = mcp::ctx_smart("/nonexistent/xyz.rs");
    assert_eq!(v["ok"], json!(false));
    assert!(v["error"].as_str().is_some());
}

#[test]
fn audit_t109_chunk_read_passthrough_small_file() {
    let path = this_test_file();
    let v = mcp::ctx_chunk_read(&path, Some(10000));
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["chunked"], json!(false));
}

#[test]
fn audit_t109_chunk_read_strips_bodies_when_over_threshold() {
    let path = this_test_file();
    let v = mcp::ctx_chunk_read(&path, Some(1));
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["chunked"], json!(true));
    let stripped: usize = v["stripped_lines"].as_u64().unwrap() as usize;
    let total: usize = v["line_count"].as_u64().unwrap() as usize;
    assert!(stripped < total, "stripped should be less than total");
}

#[test]
fn audit_t110_explain_known_counter() {
    let v = mcp::ctx_explain("compression_profile_applied_count");
    assert_eq!(v["ok"], json!(true));
    let exp = v["explanation"].as_str().unwrap();
    assert!(exp.contains("NEW-1") || exp.contains("compression"));
}

#[test]
fn audit_t110_explain_unknown_counter_returns_default() {
    let v = mcp::ctx_explain("nonexistent_counter_xyz");
    assert_eq!(v["ok"], json!(true));
    assert!(v["explanation"].as_str().unwrap().contains("Unknown"));
}

#[test]
fn audit_t111_budget_warning_at_75pct() {
    // Budget default 500_000; 380_000 ≈ 76%
    let v = mcp::ctx_budget(380_000);
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["alert_level"], json!("warning"));
}

#[test]
fn audit_t111_budget_alert_at_90pct() {
    let v = mcp::ctx_budget(460_000);
    assert_eq!(v["alert_level"], json!("alert"));
}

#[test]
fn audit_t111_budget_ok_below_75pct() {
    let v = mcp::ctx_budget(100_000);
    assert_eq!(v["alert_level"], json!("ok"));
}

#[test]
fn audit_t112_batch_execute_runs_multiple_kinds() {
    let items = vec![json!({"kind": "doctor"}), json!({"kind": "replay", "n": 3})];
    let v = mcp::ctx_batch_execute(None, &items);
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["count"], json!(2));
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn audit_t112_batch_execute_unknown_kind_fails_per_item() {
    let items = vec![json!({"kind": "nonexistent"})];
    let v = mcp::ctx_batch_execute(None, &items);
    let res = &v["results"][0];
    assert_eq!(res["ok"], json!(false));
}

#[test]
fn audit_t113_execute_file_returns_envelope() {
    let path = this_test_file();
    let v = mcp::ctx_execute_file(&path, "rust");
    assert_eq!(v["ok"], json!(true));
    assert!(v["content_bytes"].as_u64().unwrap() > 0);
}

#[test]
fn audit_t113_execute_file_missing_fails_gracefully() {
    let v = mcp::ctx_execute_file("/nonexistent/xyz.py", "python");
    assert_eq!(v["ok"], json!(false));
}

#[test]
fn audit_t114_upgrade_dry_run_returns_plan() {
    let v = mcp::ctx_upgrade(true);
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["dry_run"], json!(true));
    let plan = v["plan"].as_array().unwrap();
    assert!(plan.len() >= 3, "plan has multiple steps");
}

#[test]
fn audit_t115_discover_session_returns_envelope() {
    let v = mcp::ctx_discover_session();
    assert_eq!(v["ok"], json!(true));
    assert!(v["missed_opportunities"].is_array());
}

#[test]
fn audit_mcp_tool_names_count_27() {
    assert_eq!(ctx_mcp_tool_count(), 27);
    for required in [
        "ctx_replay",
        "ctx_purge",
        "ctx_doctor",
        "ctx_gain_history",
        "ctx_gain_graph",
        "ctx_session_adoption",
        "ctx_smart",
        "ctx_chunk_read",
        "ctx_explain",
        "ctx_budget",
        "ctx_batch_execute",
        "ctx_execute_file",
        "ctx_upgrade",
        "ctx_discover_session",
        "ctx_init_agent",
    ] {
        assert!(
            CTX_MCP_TOOL_NAMES.contains(&required),
            "MCP tool registry missing `{}`",
            required
        );
    }
}

#[test]
fn audit_full_pipeline_15_t1_initiatives() {
    // Drives every T1 in a single integrated flow that mirrors a real session.
    let _ = mcp::ctx_replay(5);
    let _ = mcp::ctx_purge(None, PurgeTargets::default());
    let doctor = mcp::ctx_doctor(None);
    assert_eq!(doctor["ok"], json!(true));
    let _ = mcp::ctx_gain_history(7);
    let _ = mcp::ctx_gain_graph(7);
    let _ = mcp::ctx_session_adoption();
    let _ = mcp::ctx_init_agent("claude-code");
    let smart = mcp::ctx_smart(&this_test_file());
    assert_eq!(smart["ok"], json!(true));
    let _ = mcp::ctx_chunk_read(&this_test_file(), Some(10000));
    let _ = mcp::ctx_explain("ctx_replay_count");
    let budget = mcp::ctx_budget(0);
    assert_eq!(budget["alert_level"], json!("ok"));
    let batch = mcp::ctx_batch_execute(None, &[json!({"kind": "doctor"})]);
    assert_eq!(batch["ok"], json!(true));
    let _ = mcp::ctx_execute_file(&this_test_file(), "rust");
    let upgrade = mcp::ctx_upgrade(true);
    assert_eq!(upgrade["dry_run"], json!(true));
    let discover = mcp::ctx_discover_session();
    assert_eq!(discover["ok"], json!(true));
    // Final check: 27 MCP tools registered
    assert_eq!(ctx_mcp_tool_count(), 27);
}

/// O `doctor` despachado pelo BATCH reporta a raiz que recebeu — não o cwd.
///
/// O cross-audit de 03/08 corrigiu `ctx_doctor` para receber a raiz por
/// parâmetro, mas o dispatcher de batch continuou passando `None`, caindo no
/// cwd — que DENTRO do daemon é o cwd do daemon, não o de quem perguntou. O
/// cross-audit de 04/08 encontrou a correção parcial sobrevivendo neste irmão.
#[test]
fn batch_doctor_reports_the_given_root_not_the_process_cwd() {
    let fake_root = tempfile::TempDir::new().expect("tempdir");
    std::fs::create_dir_all(fake_root.path().join(".git")).expect("marcador de projeto");

    let items = vec![serde_json::json!({"kind": "doctor"})];
    let out = mcp::ctx_batch_execute(Some(fake_root.path()), &items);

    let texto = serde_json::to_string(&out).expect("serializável");
    let esperado = fake_root.path().display().to_string();
    assert!(
        texto.contains(&esperado),
        "o doctor do batch tem de reportar a raiz RECEBIDA ({esperado}), \
         não o cwd do processo. Saída: {texto}"
    );

    // Contraprova — sem a raiz, o resultado é OUTRO. Sem esta metade o teste
    // passaria mesmo que a correção fosse revertida, desde que o tempdir
    // aparecesse na saída por qualquer outro motivo.
    let sem_raiz = serde_json::to_string(&mcp::ctx_batch_execute(None, &items))
        .expect("serializável");
    assert!(
        !sem_raiz.contains(&esperado),
        "com root=None o doctor NÃO pode reportar o tempdir — se reportar, o \
         teste não discrimina e não prova a correção. Saída: {sem_raiz}"
    );
}
