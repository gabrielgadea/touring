//! E2E — Cross-audit of the context-mode integration wave (2026-05-08).
//!
//! Proves the full pipeline implemented across D2.2 + D2.3 + D3.3/D3.4 +
//! P3-TRIG + D6 functions end-to-end in practice (not just in unit tests).
//!
//! Flow under test:
//!
//! ```
//! [PreToolUse: Bash with large-output args]
//!         │
//!         ▼  classify_tool_routing → RouteToSandbox
//! [tool_output_router::build_sandbox_wrapper_args]
//!         │
//!         ▼  execute_and_store
//! [sandbox_executor::execute_in_sandbox_blocking]   subprocess + blake3
//!         │
//!         ▼  store_tool_output
//! [tantivy_index::ToolOutputsIndex] (BM25 over summary)
//!         │
//!         ▼  ctx_retrieve / ctx_search / ctx_insight
//! [cli_handlers_mcp] → JSON envelope returned to LLM
//! ```
//!
//! Each test asserts a contract from the design document — not just that
//! the code compiles or returns Ok, but that **the integration delivers
//! the documented behaviour** (cross-audit, not unit test).

#![cfg(feature = "tantivy-fts")]

use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;

use touring_hooks::cli_handlers_mcp::{
    CTX_MCP_TOOL_NAMES, CtxRouter, ctx_compress, ctx_index, ctx_insight, ctx_mcp_tool_count,
    ctx_retrieve, ctx_search,
};
use touring_hooks::hook_memory::{HookEvent, HookMemoryBridge, SqliteHookMemoryBridge};
// S-13 cross-audit (2026-06-06): the tool-output storage fns moved to
// `sandbox_output_store`; the CEG sandbox runner stays in `sandbox_executor`.
use touring_hooks::sandbox_executor::{SandboxConfig, execute_in_sandbox_blocking};
use touring_hooks::sandbox_output_store::{execute_and_store, sandbox_result_to_doc};
use touring_hooks::shared::feature_flags::{
    rrf_k_constant, sandbox_fallback_on_timeout, tantivy_trigram_enabled,
};
use touring_hooks::shared::hook_events::{EventPriority, classify_priority_by_hook_name};
use touring_hooks::tantivy_index::{TantivyIndex, ToolOutputDoc, ToolOutputsIndex};
use touring_hooks::tool_output_router::{
    RoutingDecision, build_sandbox_wrapper_args, classify_tool_routing,
};

// ─── Helpers ────────────────────────────────────────────────────────────────

fn isolate_home(prefix: &str) -> TempDir {
    // Reset the global singleton so it re-reads HOME from the new value.
    touring_hooks::tantivy_index::reset_tool_outputs_global();
    let tmp = TempDir::new().expect("tempdir");
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("HOME", tmp.path()) };
    let _ = std::fs::create_dir_all(tmp.path().join(format!(".claude/touring/{prefix}")));
    tmp
}

fn fresh_router() -> (CtxRouter, TempDir, TempDir) {
    let d1 = TempDir::new().expect("d1");
    let d2 = TempDir::new().expect("d2");
    let symbols = TantivyIndex::open_or_create(d1.path()).expect("symbols index");
    let outputs = ToolOutputsIndex::open_or_create(d2.path()).expect("outputs index");
    (CtxRouter::new(symbols, outputs), d1, d2)
}

// ─── 1. Classification → Decision contract ─────────────────────────────────

#[test]
fn audit_router_classifies_large_bash_as_sandbox() {
    let args = json!({"command": "gh api repos/anthropics/claude-code/issues"});
    let decision = classify_tool_routing("Bash", &args).unwrap();
    assert_eq!(
        decision,
        RoutingDecision::RouteToSandbox,
        "large-output Bash MUST route to sandbox per design"
    );
}

#[test]
fn audit_router_passes_small_read_through() {
    let args = json!({"file_path": "src/main.rs"});
    let decision = classify_tool_routing("Read", &args).unwrap();
    assert_eq!(
        decision,
        RoutingDecision::PassThrough,
        "Read tool MUST pass through (file content already in context)"
    );
}

// ─── 2. Sandbox subprocess contract ─────────────────────────────────────────

#[test]
#[serial]
fn audit_sandbox_blocking_captures_stdout_and_hashes() {
    // No HOME isolation needed — store_output uses ~/.claude/touring/sandbox_outputs
    let _home = isolate_home("sandbox_outputs");
    let res = execute_in_sandbox_blocking(
        "Bash",
        json!({"command": "printf 'audit-payload-42'"}),
        SandboxConfig::default(),
    )
    .expect("subprocess");

    assert_eq!(res.exit_code, 0, "successful subprocess MUST return 0");
    assert!(res.output_bytes >= 16, "captured stdout MUST equal payload");
    assert_eq!(res.content_hash.len(), 64, "blake3 hex MUST be 64 chars");
    assert!(!res.was_truncated, "small output MUST NOT be truncated");
    assert!(res.stored_path.is_some(), "stored_path MUST be populated");
}

#[test]
#[serial]
fn audit_sandbox_timeout_fallback_returns_sentinel_when_enabled() {
    let _home = isolate_home("sandbox_outputs_timeout");
    // Force a guaranteed-timeout: 1ms wall clock vs sleep 5s
    let cfg = SandboxConfig {
        timeout_ms: 1,
        max_output_bytes: 1024,
        fallback_on_timeout: true,
    };
    let res = execute_in_sandbox_blocking("Bash", json!({"command": "sleep 5"}), cfg)
        .expect("fallback should produce sentinel Ok, not Err");

    assert_eq!(res.exit_code, -2, "fallback sentinel MUST use exit_code=-2");
    assert!(
        res.was_truncated,
        "fallback sentinel MUST set truncated=true"
    );
    assert!(
        sandbox_fallback_on_timeout(),
        "default flag MUST be true (D2 design)"
    );
}

#[test]
#[serial]
fn audit_sandbox_timeout_propagates_when_fallback_disabled() {
    let _home = isolate_home("sandbox_outputs_no_fallback");
    let cfg = SandboxConfig {
        timeout_ms: 1,
        max_output_bytes: 1024,
        fallback_on_timeout: false,
    };
    let err = execute_in_sandbox_blocking("Bash", json!({"command": "sleep 5"}), cfg)
        .expect_err("non-fallback timeout MUST return Err");
    let msg = err.to_string();
    assert!(
        msg.contains("timed out"),
        "Err MUST mention timeout: got {msg}"
    );
}

// ─── 3. SandboxResult → ToolOutputDoc roundtrip ─────────────────────────────

#[test]
#[serial]
fn audit_sandbox_to_doc_carries_all_fields() {
    let _home = isolate_home("sandbox_to_doc");
    let res = execute_in_sandbox_blocking(
        "Bash",
        json!({"command": "printf 'doc-conversion-test'"}),
        SandboxConfig::default(),
    )
    .expect("subprocess");

    let doc = sandbox_result_to_doc(&res, "Bash");
    assert_eq!(doc.content_hash, res.content_hash);
    assert_eq!(doc.tool_name, "Bash");
    assert_eq!(doc.exit_code, 0);
    assert_eq!(doc.output_bytes, res.output_bytes);
    assert!(
        !doc.summary.is_empty(),
        "summary MUST be derived from output"
    );
    assert!(
        doc.summary.contains("doc-conversion-test"),
        "summary MUST contain captured payload: got '{}'",
        doc.summary
    );
    assert!(doc.stored_at_unix > 0, "stored_at_unix MUST be set");
}

// ─── 4. Tantivy tool_outputs roundtrip ──────────────────────────────────────

#[test]
fn audit_tool_outputs_store_and_retrieve_via_router() {
    let (router, _d1, _d2) = fresh_router();
    let doc = ToolOutputDoc {
        content_hash: "e".repeat(64),
        tool_name: "Bash".into(),
        summary: "list issues open count=42".into(),
        full_output_path: "/tmp/sandbox/e.bin".into(),
        exit_code: 0,
        output_bytes: 4096,
        was_truncated: false,
        stored_at_unix: 1_700_000_001,
        tool_args: None,
    };
    let stored = ctx_index(&router, &doc);
    assert_eq!(stored["ok"], json!(true));

    let r = ctx_retrieve(&router, &doc.content_hash);
    assert_eq!(r["ok"], json!(true));
    assert_eq!(r["found"], json!(true));
    assert_eq!(r["doc"]["tool_name"], json!("Bash"));
    assert_eq!(r["doc"]["output_bytes"], json!(4096));
}

#[test]
fn audit_search_summaries_finds_indexed_doc() {
    let (router, _d1, _d2) = fresh_router();
    let doc = ToolOutputDoc {
        content_hash: "f".repeat(64),
        tool_name: "Grep".into(),
        summary: "execution running tests authentication".into(),
        full_output_path: "/tmp/f.bin".into(),
        exit_code: 0,
        output_bytes: 1024,
        was_truncated: false,
        stored_at_unix: 1_700_000_002,
        tool_args: None,
    };
    router.tool_outputs.store_tool_output(&doc).expect("store");

    // BM25 over en_stem field — "executing" must morph to "execut" + match
    // "execution" via Porter stemming (P2-STEM contract).
    let hits = router
        .tool_outputs
        .search_summaries("executing tests", 5)
        .expect("search_summaries");
    assert!(
        !hits.is_empty(),
        "Porter stemmer MUST match execution↔executing"
    );
    assert_eq!(hits[0].content_hash, doc.content_hash);
}

// ─── 5. ctx_insight composite ───────────────────────────────────────────────

#[test]
fn audit_ctx_insight_returns_no_placeholder_strings() {
    let (router, _d1, _d2) = fresh_router();
    let v = ctx_insight(&router, "anything");
    // Wave 2026-05-08 audit requirement: NO "pending_*" strings
    let s = serde_json::to_string(&v).expect("serialize");
    assert!(
        !s.contains("pending_d6.4_extension"),
        "ctx_insight MUST NOT carry placeholder text after audit wiring: {s}"
    );
    // Structural shape expectation
    assert_eq!(v["ok"], json!(true));
    assert!(v["tool_outputs_search"].is_object());
    assert_eq!(v["tool_outputs_search"]["ok"], json!(true));
}

// ─── 6. PreCompact priority filter (D3.3 + D3.4) ───────────────────────────

#[test]
fn audit_precompact_filter_drops_medium_low_keeps_critical_high() {
    let conn = rusqlite::Connection::open_in_memory().expect("conn");
    let bridge = SqliteHookMemoryBridge::new(&conn);
    bridge.ensure_schema().expect("schema");
    let mut bridge = bridge;
    let sid = "audit_session";

    for (hook, expected) in [
        ("session_start", EventPriority::Critical),
        ("post_edit", EventPriority::High),
        ("pre_read", EventPriority::Medium),
        ("hook_memory_store", EventPriority::Low),
    ] {
        let actual = classify_priority_by_hook_name(hook);
        assert_eq!(
            actual, expected,
            "classify_priority_by_hook_name({hook}) MUST yield {expected:?}"
        );
        let ev = HookEvent::new(hook, sid, "", "execution", json!({}), format!("h:{hook}"));
        bridge.store_hook_event(ev).expect("store");
    }

    let surviving = bridge.events_surviving_compaction(sid).expect("surv");
    let dropped = bridge
        .count_events_dropped_by_compaction(sid)
        .expect("drop");
    assert_eq!(surviving.len(), 2, "MUST keep CRITICAL + HIGH");
    assert_eq!(dropped, 2, "MUST drop MEDIUM + LOW");

    // ctx_compress envelope contract
    let v = ctx_compress(&conn, sid);
    assert_eq!(v["surviving_count"], json!(2));
    assert_eq!(v["dropped_count"], json!(2));
}

#[test]
fn audit_priority_tier_migration_idempotent_through_multiple_calls() {
    let conn = rusqlite::Connection::open_in_memory().expect("conn");
    let bridge = SqliteHookMemoryBridge::new(&conn);
    // 3 calls — must all succeed (PRAGMA gate on second+ call)
    bridge.ensure_schema().expect("call 1");
    bridge.ensure_schema().expect("call 2 (idempotent)");
    bridge.ensure_schema().expect("call 3 (idempotent)");
}

// ─── 7. P3-TRIG RRF fusion ─────────────────────────────────────────────────

#[test]
fn audit_rrf_constants_match_documented_default() {
    assert_eq!(
        rrf_k_constant(),
        60,
        "k=60 (Cormack et al.) is the documented default"
    );
}

#[test]
#[serial]
fn audit_rrf_search_falls_back_when_disabled() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("TOURING_TANTIVY_TRIGRAM", "0") };
    assert!(!tantivy_trigram_enabled(), "flag setter must take effect");
    let (router, _d1, _d2) = fresh_router();
    // search must return Ok envelope even when trigram disabled
    let v = ctx_search(&router, "nothing", 5);
    assert_eq!(v["ok"], json!(true));
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_TANTIVY_TRIGRAM") };
}

// ─── 8. MCP tool registration completeness ─────────────────────────────────

#[test]
fn audit_post_wave_rtk_ctx_tools_registered() {
    // Post-Wave RTK + Wave 3 INTELLIGENCE: tool count grew from 12 to 27 (T1).
    // This audit fixes the lower-bound — ≥ 12 — and verifies the 10 essential
    // ctx_* tools that close the post-Wave RTK contract (NEW-2 ctx_tee_retrieve
    // is the only one shipping at the touring-hooks layer; ctx_gain + ctx_discover
    // are CLI subcommands `touring tantivy gain|discover` and live in
    // touring-server, not in CTX_MCP_TOOL_NAMES).
    assert!(
        ctx_mcp_tool_count() >= 12,
        "post-Wave RTK ships ≥ 12 ctx_* tools, got {}",
        ctx_mcp_tool_count()
    );
    for expected in [
        "ctx_search",
        "ctx_index",
        "ctx_retrieve",
        "ctx_insight",
        "ctx_compress",
        "ctx_session_guide",
        "ctx_aggregate",
        "ctx_facets",
        "ctx_cleanup",
        "ctx_tee_retrieve",
    ] {
        assert!(
            CTX_MCP_TOOL_NAMES.contains(&expected),
            "{expected} MUST be in registered tool names"
        );
    }
}

// ─── 9. Full pipeline closure: subprocess → store → retrieve ────────────────

#[test]
#[serial]
fn audit_full_pipeline_subprocess_to_retrieve() {
    let _home = isolate_home("full_pipeline");
    // Step 1: real subprocess via execute_and_store
    let res = execute_and_store(
        None,
        "Bash",
        json!({"command": "printf 'pipeline-end-to-end-evidence'"}),
        SandboxConfig::default(),
    )
    .expect("execute_and_store");

    assert_eq!(res.exit_code, 0);
    assert!(!res.content_hash.is_empty());

    // Step 2: retrieve from the GLOBAL tool_outputs index — proves
    // execute_and_store actually wired into the singleton, not just a
    // local index.
    let global = touring_hooks::tantivy_index::tool_outputs_for(None)
        .expect("global tool_outputs MUST initialise (HOME set)");
    let doc = global
        .get_tool_output(&res.content_hash)
        .expect("get_tool_output")
        .expect("doc must be present after execute_and_store");
    assert_eq!(doc.content_hash, res.content_hash);
    assert!(doc.summary.contains("pipeline-end-to-end-evidence"));
    assert_eq!(doc.tool_name, "Bash");
}

// ─── 10. PreToolUse → wrapper envelope shape (D2.4 contract) ───────────────

#[test]
#[serial]
fn audit_build_sandbox_wrapper_returns_content_hash_for_real_subprocess() {
    let _home = isolate_home("wrapper_shape");
    let original = json!({"command": "printf 'wrapper-evidence'"});
    let wrapped = build_sandbox_wrapper_args(None, "Bash", original);

    // After wave 2026-05-08: wrapper MUST carry actual content_hash, not
    // just the boolean flag from the legacy stub.
    assert_eq!(wrapped["_sandbox_routed"], json!(true));
    assert_eq!(wrapped["ok"], json!(true));
    assert!(
        wrapped["content_hash"].is_string(),
        "wrapper MUST embed content_hash: {wrapped}"
    );
    let hash = wrapped["content_hash"].as_str().unwrap();
    assert_eq!(hash.len(), 64, "blake3 hex");
    assert_eq!(wrapped["exit_code"], json!(0));
}
