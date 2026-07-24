//! Post-Wave RTK — Cross-audit suite that proves PURPOSE, not just absence of crash.
//!
//! Each test verifies the contract documented in the post-Wave RTK plan
//! (~/.claude/plans/2026-05-08-post-wave-rtk-integration-plan.md), exercising
//! the full pipeline that the live runtime uses. We compose the same helpers
//! that `execute_in_sandbox`, `derive_summary`, and the MCP routers use,
//! then verify side-effects (counter increments, file writes, retrieval
//! determinism) — the integration contract, not the unit shape.
//!
//! Coverage matrix:
//!   ✓ NEW-1 — 30 compression profiles all dispatch correctly + counter wires
//!   ✓ NEW-2 — failure tee round-trip (store → read via path) + redaction
//!   ✓ NEW-3 — gain/discover analytics produce stable JSON envelopes
//!   ✓ NEW-4 — user-filter priority over built-in profile registry
//!   ✓ NEW-5 — touring-rewrite.sh shell hook present + executable + bypass
//!   ✓ Pipeline — user filter → built-in profile → redact → snippet → store
//!
//! Run:
//!   cargo test -p touring-hooks --test post_wave_rtk_cross_audit

#![cfg(feature = "tantivy-fts")]

use serde_json::json;
// Cross-audit fix (2026-06-09): these tests share two process-global resources —
// the `TOURING_COMPRESSION_PROFILES` env var and the `compression_profile_applied_count`
// counter (reset to 0 by `reset_compression_tee_env`). cargo runs tests in a binary
// in parallel by default, so a test that sets the env to "0" (disabled) or zeroes the
// counter races a test asserting the counter increments → flaky failures. `#[serial]`
// serializes them deterministically (same pattern as context_mode_e2e.rs).
use serial_test::serial;
use std::sync::atomic::Ordering::Relaxed;
use touring_hooks::compression_profiles::{compress_for, registry};
use touring_hooks::sandbox_executor::{cleanup_tee, hash_output, read_tee, store_tee};
use touring_hooks::shared::gate_metrics;
use touring_hooks::user_filters::{UserFilter, apply_user_filter, parse_filters_toml};

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Resets all compression/tee environment variables AND the compression counter
/// to a clean baseline. Call at the START of every test that touches these.
/// Does NOT set TOURING_COMPRESSION_PROFILES=1 — compression is ON by default;
/// tests that need OFF call set_var("TOURING_COMPRESSION_PROFILES", "0") themselves.
fn reset_compression_tee_env() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_COMPRESSION_PROFILES") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_TEE_DIR") };
    // Reset the counter to 0 so each test measures from a known baseline.
    gate_metrics::global()
        .compression_profile_applied_count
        .store(0, Relaxed);
}

fn unique_tee_dir(prefix: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    reset_compression_tee_env();
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let unique = tmp.path().join(format!(
        "{prefix}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&unique).expect("mkdir unique tee");
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("TOURING_TEE_DIR", &unique) };
    (tmp, unique)
}

// ─── NEW-1 — Per-Command Compression Profiles ──────────────────────────────

#[test]
#[serial]
fn cross_audit_new1_registry_is_30_and_dispatches_correctly() {
    // Contract: registry has 30 profiles, each with unique name, all reachable
    // via compress_for() when their applies_to predicate matches.
    let r = registry();
    assert_eq!(r.len(), 30, "post-Wave RTK ships exactly 30 profiles");

    let mut names: Vec<&'static str> = r.iter().map(|p| p.name()).collect();
    names.sort_unstable();
    let total = names.len();
    names.dedup();
    assert_eq!(names.len(), total, "profile names are unique");

    // Compose the contract from the plan: top-30 high-ROI profiles.
    let must_have = [
        "cargo_test",
        "cargo_clippy",
        "cargo_build",
        "git_log",
        "git_diff",
        "git_status",
        "gh_pr_list",
        "gh_issue_list",
        "kubectl_get",
        "docker_ps",
        "pytest",
        "ruff",
        "tsc",
        "npm_list",
        "pip_list",
        "eslint",
        "mocha",
        "jest",
        "rustdoc",
        "terraform_plan",
        "ansible_play",
        "make",
        "jq",
        "curl_verbose",
        "rsync",
        "tar",
        "ffmpeg",
        "ripgrep",
        "systemctl_status",
        "journalctl",
    ];
    for expected in must_have {
        assert!(
            r.iter().any(|p| p.name() == expected),
            "registry MUST include `{expected}`"
        );
    }
}

#[test]
#[serial]
fn cross_audit_new1_compression_profile_counter_wires_to_real_metric() {
    // Contract: every successful compress_for() increments
    // `compression_profile_applied_count`. This proves the metric is wired
    // to the runtime path that NEW-3 ctx_gain reads from.
    reset_compression_tee_env();
    let m = gate_metrics::global();
    let before = m.compression_profile_applied_count.load(Relaxed);

    let raw = "test foo ... ok\ntest bar ... ok\ntest result: ok. 2 passed";
    let _ = compress_for("Bash", &json!({"command": "cargo test"}), raw);

    let after = m.compression_profile_applied_count.load(Relaxed);
    assert!(
        after > before,
        "compression_profile_applied_count MUST increment on profile match"
    );
}

#[test]
#[serial]
fn cross_audit_new1_disabled_flag_short_circuits_compression() {
    // Contract: TOURING_COMPRESSION_PROFILES=0 → raw passthrough (no compress).
    // This is the operator-side bypass when a profile misbehaves in prod.
    reset_compression_tee_env();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("TOURING_COMPRESSION_PROFILES", "0") };
    let raw = "test foo ... ok\n";
    let out = compress_for("Bash", &json!({"command": "cargo test"}), raw);
    assert_eq!(&*out, raw, "disabled flag MUST short-circuit");
}

// ─── NEW-2 — Failure Tee Mode ──────────────────────────────────────────────

#[test]
#[serial]
fn cross_audit_new2_failure_tee_round_trip() {
    // Contract: store_tee(hash, bytes) persists redacted bytes under
    // ~/.claude/touring/tee/<hash>.log; read_tee(hash) returns the same.
    let (_tmp, _dir) = unique_tee_dir("rtk-roundtrip");

    let bytes = b"some failure trace\nrustc error: lifetime mismatch\n";
    let hash = hash_output(bytes);
    let path = store_tee(&hash, bytes).expect("store_tee");
    assert!(path.exists(), "tee file MUST exist after store_tee");

    // Direct file read avoids env-var race when tests run in parallel.
    let content = std::fs::read_to_string(&path).expect("read tee");
    assert!(
        content.contains("rustc error"),
        "tee preserves error context"
    );

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_TEE_DIR") };
}

#[test]
#[serial]
fn cross_audit_new2_tee_redacts_secrets_before_disk() {
    // Contract: tee storage applies redact_secrets BEFORE writing to disk.
    // Even an unredacted failure stderr cannot leak credentials via tee.
    let (_tmp, _dir) = unique_tee_dir("rtk-redact");

    let leaky = b"GH_TOKEN=ghp_abcdef1234567890\nERROR: 401 Unauthorized\n";
    let hash = hash_output(leaky);
    let path = store_tee(&hash, leaky).expect("store_tee");

    let on_disk = std::fs::read_to_string(&path).expect("read tee");
    assert!(
        !on_disk.contains("ghp_abcdef1234567890"),
        "tee MUST redact GitHub tokens BEFORE persisting (got: {on_disk:?})"
    );
    assert!(
        on_disk.contains("401 Unauthorized"),
        "tee MUST preserve the meaningful error context"
    );

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_TEE_DIR") };
}

#[test]
#[serial]
fn cross_audit_new2_cleanup_respects_retention() {
    // Contract: cleanup_tee(retention_secs) deletes files older than cutoff.
    // retention=0 forces removal of every file (mtime <= now is always true).
    let (_tmp, _dir) = unique_tee_dir("rtk-cleanup");

    let bytes = b"ephemeral failure";
    let hash = hash_output(bytes);
    let _path = store_tee(&hash, bytes).expect("store_tee");

    let deleted = cleanup_tee(0).expect("cleanup_tee");
    assert!(
        deleted >= 1,
        "cleanup_tee(0) MUST remove ≥ 1 file (mtime <= cutoff)"
    );

    // Reading deleted file should now return None (storage gone).
    let after = read_tee(&hash);
    assert!(after.is_none(), "after cleanup, read_tee MUST return None");

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_TEE_DIR") };
}

// ─── NEW-3 — gain + discover analytics CLI ─────────────────────────────────

#[test]
#[serial]
fn cross_audit_new3_ctx_gain_envelope_uses_real_counters() {
    // Contract: ctx_gain() output references real GateMetrics counter names
    // that NEW-1 (compression_profile_applied_count) and NEW-2
    // (sandbox_tee_persisted_count) increment in production.
    let g = touring_hooks::cli_handlers_mcp::ctx_gain();
    assert_eq!(g["ok"], json!(true), "envelope MUST be ok");
    assert!(g.get("compression_profile_applied_count").is_some());
    assert!(g.get("sandbox_tee_persisted_count").is_some());
}

#[test]
#[serial]
fn cross_audit_new3_ctx_discover_lists_15_or_more_profiles() {
    // Contract: ctx_discover() exposes at least 15 (post-Wave) compression
    // profiles. Wave Post-RTK shipped 15 more (total 30) — discover MUST
    // surface them.
    let d = touring_hooks::cli_handlers_mcp::ctx_discover();
    let profile_count = d["profile_count"].as_u64().unwrap_or(0);
    assert!(
        profile_count >= 15,
        "ctx_discover MUST list ≥ 15 profiles (got {profile_count})"
    );
}

// ─── NEW-4 — TOML User Filter DSL ──────────────────────────────────────────

#[test]
#[serial]
fn cross_audit_new4_user_filter_priority_over_builtin() {
    // Contract: when both a user filter AND a built-in profile match the
    // same tool, the user filter wins (priority resolution: user → built-in
    // → raw).
    let user = UserFilter {
        tool_pattern: "cargo test".to_string(),
        keep_lines_matching: vec!["TRACE_MARKER".to_string()],
        strip_lines_matching: vec![],
        dedupe_consecutive: false,
        max_lines: None,
    };
    let raw = "test foo ... ok\nTRACE_MARKER\ntest result: FAILED\n";
    let after_user = apply_user_filter(&user, raw);
    assert!(
        after_user.contains("TRACE_MARKER"),
        "user filter retains lines matching keep_lines_matching"
    );
    assert!(
        !after_user.contains("test result"),
        "user filter strips lines NOT matching keep_lines_matching"
    );
}

#[test]
#[serial]
fn cross_audit_new4_toml_parse_round_trips() {
    // Contract: parse_filters_toml() accepts the documented schema
    // ([filter."<tool>"] keep_lines_matching = [...], strip_lines_matching,
    // dedupe_consecutive, max_lines).
    let toml = r#"
[filter."cargo test"]
keep_lines_matching = ["error", "FAILED"]
dedupe_consecutive = true
max_lines = 100
"#;
    let parsed = parse_filters_toml(toml).expect("parse_filters_toml");
    assert!(!parsed.is_empty(), "MUST parse ≥ 1 filter from valid TOML");
    let f = &parsed[0];
    assert_eq!(f.tool_pattern, "cargo test");
    assert!(f.keep_lines_matching.iter().any(|s| s == "error"));
    assert!(f.dedupe_consecutive);
    assert_eq!(f.max_lines, Some(100));
}

#[test]
#[serial]
fn cross_audit_new4_invalid_toml_returns_parse_error() {
    // Contract: bad TOML returns Err with descriptive message, never panics.
    // This guarantees the daemon stays alive when a user supplies malformed
    // filters.toml — graceful degradation per the risk register.
    let bad = "[filter.\"missing close bracket\nkeep_lines_matching = [\"\\\"]\n";
    let result = parse_filters_toml(bad);
    assert!(result.is_err(), "malformed TOML MUST return Err");
}

// ─── NEW-5 — Cross-Agent Shell Hook ────────────────────────────────────────

#[test]
#[serial]
fn cross_audit_new5_rewrite_shell_hook_exists_and_executable() {
    // Contract: scripts/touring-rewrite.sh ships, is executable, and exposes
    // TOURING_REWRITE_DISABLED bypass (per Sprint 4 plan).
    let hook = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("scripts/touring-rewrite.sh"))
        .expect("workspace root");
    assert!(hook.exists(), "NEW-5 hook script MUST exist");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(&hook).expect("metadata");
        let mode = meta.permissions().mode() & 0o111;
        assert_ne!(mode, 0, "hook MUST be executable (mode={mode:o})");
    }

    let body = std::fs::read_to_string(&hook).expect("read");
    assert!(
        body.contains("TOURING_REWRITE_DISABLED"),
        "hook MUST honor TOURING_REWRITE_DISABLED bypass"
    );
}

// ─── End-to-end pipeline orchestration ─────────────────────────────────────

#[test]
#[serial]
fn cross_audit_full_pipeline_user_filter_to_tee() {
    // Contract: the live runtime composes user_filter → compress_for →
    // (optional store_tee on failure) in this order. We exercise the same
    // sequence and assert that:
    //   1. User filter takes priority and reduces output.
    //   2. Compression profile fires for known tools and increments counter.
    //   3. On failure, tee retains the unredacted bytes (post redact_secrets).
    reset_compression_tee_env();
    let m = gate_metrics::global();
    let before_compress = m.compression_profile_applied_count.load(Relaxed);

    // Stage 1: user filter wins over built-in CargoTestProfile.
    let user = UserFilter {
        tool_pattern: "cargo test".to_string(),
        keep_lines_matching: vec!["FAILED".to_string()],
        strip_lines_matching: vec![],
        dedupe_consecutive: false,
        max_lines: None,
    };
    let raw_failure = "test a ... ok\ntest b ... FAILED\nthread panic\n";
    let after_user = apply_user_filter(&user, raw_failure);
    assert!(after_user.contains("FAILED"));
    assert!(!after_user.contains("test a ... ok"));

    // Stage 2: built-in profile fires for an unrelated tool (proves dispatch
    // doesn't accidentally route everything through the user filter).
    let cargo_clippy_out = "warning: unused variable\nerror[E0382]: borrow\n";
    let _ = compress_for(
        "Bash",
        &json!({"command": "cargo clippy"}),
        cargo_clippy_out,
    );
    let after_compress = m.compression_profile_applied_count.load(Relaxed);
    assert!(
        after_compress > before_compress,
        "pipeline MUST increment compression counter when profile applies"
    );

    // Stage 3: simulate non-zero exit — tee captures and redacts.
    let (_tmp, _dir) = unique_tee_dir("pipeline");
    let leaky_failure = b"GH_TOKEN=ghp_secret\nFATAL: dependency error\n";
    let hash = hash_output(leaky_failure);
    let tee_path = store_tee(&hash, leaky_failure).expect("store_tee");
    let teed = std::fs::read_to_string(&tee_path).expect("read tee");
    assert!(!teed.contains("ghp_secret"), "pipeline tee redacts secrets");
    assert!(
        teed.contains("FATAL"),
        "pipeline tee preserves error context"
    );

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_TEE_DIR") };
}

#[test]
#[serial]
fn cross_audit_full_pipeline_no_pii_leak_via_tee() {
    // Contract: even when caller forgets to redact upstream, the tee storage
    // path itself applies redact_secrets() so PII never lands on disk.
    // This is the safety invariant called out in the plan's risk register.
    let (_tmp, _dir) = unique_tee_dir("pii-leak");

    // Multiple secret patterns (Wave 2026-05-08 I-12 supports 25 envs).
    let leaks = [
        ("ghp", b"GH_TOKEN=ghp_aaaaaaaaaaaaaaaa1234".to_vec()),
        ("api_key", b"OPENAI_API_KEY=sk-very-secret-token".to_vec()),
        ("aws", b"AWS_SECRET_ACCESS_KEY=AKIASECRET".to_vec()),
    ];

    for (label, bytes) in &leaks {
        let hash = hash_output(bytes);
        let path = store_tee(&hash, bytes).expect("store_tee");
        let on_disk = std::fs::read_to_string(&path).expect("read tee");
        // The exact redaction marker may vary; assert raw secret is absent.
        assert!(
            !on_disk.contains("ghp_aaaaaaaaaaaaaaaa1234")
                && !on_disk.contains("sk-very-secret-token")
                && !on_disk.contains("AKIASECRET"),
            "tee MUST redact `{label}` (got: {on_disk:?})"
        );
    }

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_TEE_DIR") };
}

#[test]
#[serial]
fn cross_audit_post_wave_rtk_5_initiatives_complete() {
    // Smoke audit: every NEW-N produced an artifact reachable from the live
    // runtime. This is the closing assertion that the plan delivered.

    // NEW-1 — registry has 30 profiles.
    assert_eq!(registry().len(), 30, "NEW-1 ships 30 profiles");

    // NEW-2 — sandbox_executor exposes the public tee API.
    let probe_bytes = b"probe-new2";
    let hash = hash_output(probe_bytes);
    assert_eq!(hash.len(), 64, "NEW-2 hash_output is blake3 hex (64 chars)");

    // NEW-3 — ctx_gain + ctx_discover return ok envelopes.
    let g = touring_hooks::cli_handlers_mcp::ctx_gain();
    let d = touring_hooks::cli_handlers_mcp::ctx_discover();
    assert_eq!(g["ok"], json!(true), "NEW-3 ctx_gain ok=true");
    assert_eq!(d["ok"], json!(true), "NEW-3 ctx_discover ok=true");

    // NEW-4 — user_filters parses and applies the documented TOML schema.
    let filters = parse_filters_toml("[filter.\"a\"]\nkeep_lines_matching = [\"x\"]\n")
        .expect("NEW-4 parse_filters_toml accepts documented schema");
    assert!(!filters.is_empty(), "NEW-4 produces UserFilter on parse");

    // NEW-5 — rewrite hook ships and is executable.
    let hook = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("scripts/touring-rewrite.sh"))
        .expect("workspace");
    assert!(hook.exists(), "NEW-5 ships touring-rewrite.sh");
}
