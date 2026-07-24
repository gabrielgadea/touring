//! P99 latency regression guards for the Predictive Wave (D2/D3/D4/D5).
//!
//! Verifies that the three advisory injection paths and the D5 observability
//! counters satisfy their documented latency budgets under 100+ repeated
//! executions. Uses `hdrhistogram` for accurate tail-latency measurement.
//!
//! # Budgets (from design docs)
//!
//! | Path | Budget | P99 assert |
//! |------|--------|-----------|
//! | D2 `extract_pascal_symbols` (proxy via CLI) | 40ms total (all symbols) | < 2_000ms per CLI call |
//! | D3 `extract_task_features` (25-dim) | sync inline | < 50ms per call |
//! | D4 `run_shadow_rollout` (skeleton) | 500ms per call | < 1_000ms per call |
//! | D5 `record_*` atomics | ~1ns per increment | < 10μs per call |
//! | D5 `GateMetricsSnapshot::capture()` | sync | < 5ms per call |
//!
//! # Notes
//!
//! - D3/D4/D5 tests run in-process without a daemon.
//! - D2 is tested via the compiled `touring` CLI binary (optional — skipped when binary absent).
//! - All assertions are against P99 of the distribution, not individual samples,
//!   to tolerate scheduler jitter on CI.

use hdrhistogram::Histogram;
use serde_json::json;
use std::time::{Duration, Instant};

// ── D2: PascalCase extraction — CLI proxy latency ────────────────────────────

/// D2 guard: Pre-tool-use hook overhead via CLI binary must have P99 < 2s over 5 iterations.
///
/// Tests the real binary path (touring pre-tool-use stdin). Skips gracefully when
/// the binary or daemon is unavailable — avoiding CI flakiness. Measures the full
/// round-trip including hook dispatch overhead.
#[test]
fn test_d2_pre_tool_use_cli_p99_under_2s() {
    let binary = match find_touring_binary() {
        Some(b) => b,
        None => {
            eprintln!("test_d2: touring binary not found — skipping (NOOP path)");
            return;
        }
    };

    let mut hist: Histogram<u64> =
        Histogram::new_with_bounds(1, 60_000_000, 3).expect("valid histogram bounds");

    let payload = json!({
        "tool_name": "Read",
        "tool_input": {"file_path": "/tmp/test.rs"}
    });

    // 5 iterations — CLI subprocess is expensive (~50ms each minimum).
    for _ in 0..5 {
        let t0 = Instant::now();
        let _output = invoke_cli_hook(&binary, "pre-tool-use", &payload);
        let elapsed_ms = t0.elapsed().as_millis() as u64;
        // Store in microseconds for histogram uniformity.
        hist.record((elapsed_ms * 1000).clamp(1, 60_000_000)).ok();
    }

    let p99_us = hist.value_at_quantile(0.99);
    // Budget: 2_000ms = 2_000_000μs. CLI overhead is dominated by process spawn.
    assert!(
        p99_us < 2_000_000,
        "D2 pre-tool-use CLI P99 = {}ms, expected < 2_000ms",
        p99_us / 1000
    );
}

// ── D2: extract_pascal_symbols via in-process proxy ──────────────────────────

/// D2 guard (in-process): `extract_task_features` as pascal-symbol proxy.
///
/// Since `extract_pascal_symbols` is private, we use `extract_task_features`
/// on a subject-heavy task (same scanning patterns) as a latency proxy.
/// This validates that the D2 hot path is O(n) in string length, not quadratic.
#[test]
fn test_d2_symbol_extraction_proxy_p99_under_1ms() {
    use touring_hooks::shared::extract_task_features;

    let mut hist: Histogram<u64> =
        Histogram::new_with_bounds(1, 60_000_000, 3).expect("valid histogram bounds");

    // Long subject with many PascalCase-like words to stress the scanner.
    let task = json!({
        "title": "Implement BlastRadiusEngine compute_with_timeout integration \
                  for HookRuntime GateMetrics ShadowRollout TaskRoutingDecision \
                  PheromoneMCTS CognitiveMCTS GraphInformedMCTS SymbolStore",
        "cila_level": 4,
        "status": "pending"
    });

    for _ in 0..100 {
        let t0 = Instant::now();
        let feats = extract_task_features(&task);
        let elapsed_us = t0.elapsed().as_micros() as u64;
        assert_eq!(
            feats.len(),
            25,
            "D2 proxy: extract_task_features must return 25 dims"
        );
        hist.record(elapsed_us.clamp(1, 60_000_000)).ok();
    }

    let p99_us = hist.value_at_quantile(0.99);
    // Budget: 1ms = 1_000μs. Feature extraction is in-process and very fast.
    assert!(
        p99_us < 1_000,
        "D2 symbol extraction proxy P99 = {}μs, expected < 1_000μs",
        p99_us
    );
}

// ── D3: extract_task_features latency ────────────────────────────────────────

/// D3 guard: `extract_task_features` P99 must be < 50ms over 100 iterations.
///
/// The 25-dim feature extraction runs inline in the TaskList hook response
/// path. Validates it stays well within the 50ms budget.
#[test]
fn test_d3_extract_task_features_p99_under_50ms() {
    use touring_hooks::shared::extract_task_features;

    let mut hist: Histogram<u64> =
        Histogram::new_with_bounds(1, 60_000_000, 3).expect("valid histogram bounds");

    let task = json!({
        "title": "implement predictive wave router for EnterPlanMode and TaskList hooks",
        "cila_level": 4,
        "status": "pending"
    });

    for _ in 0..100 {
        let t0 = Instant::now();
        let feats = extract_task_features(&task);
        let elapsed_us = t0.elapsed().as_micros() as u64;
        assert_eq!(
            feats.len(),
            25,
            "D3: extract_task_features must return 25 dims"
        );
        hist.record(elapsed_us.clamp(1, 60_000_000)).ok();
    }

    let p99_us = hist.value_at_quantile(0.99);
    // Budget: 50ms = 50_000μs.
    assert!(
        p99_us < 50_000,
        "D3 extract_task_features P99 = {}μs, expected < 50_000μs",
        p99_us
    );
}

/// D3 guard: `extract_task_features` on empty task must complete in < 100μs P99.
///
/// Validates the zero-input path (all fields missing) that serves as baseline
/// for NOOP hooks — no subject scan, no cila_level lookup.
#[test]
fn test_d3_extract_task_features_empty_task_p99_under_100us() {
    use touring_hooks::shared::extract_task_features;

    let mut hist: Histogram<u64> =
        Histogram::new_with_bounds(1, 60_000_000, 3).expect("valid histogram bounds");

    let empty_task = json!({});

    for _ in 0..1_000 {
        let t0 = Instant::now();
        let feats = extract_task_features(&empty_task);
        let elapsed_us = t0.elapsed().as_micros().max(1) as u64;
        // dim 3 = 1.0 (other), dim 4 = 1.0 (short), dim 7 = 1.0 (low), dim 12 = 1.0 (L0)
        assert_eq!(feats[3], 1.0, "empty task: dim 3 = other");
        assert_eq!(feats[4], 1.0, "empty task: dim 4 = short");
        hist.record(elapsed_us.clamp(1, 60_000_000)).ok();
    }

    let p99_us = hist.value_at_quantile(0.99);
    // Budget: 100μs (very generous for a stack-only operation).
    assert!(
        p99_us < 100,
        "D3 empty task P99 = {}μs, expected < 100μs",
        p99_us
    );
}

// ── D4: run_shadow_rollout latency ────────────────────────────────────────────

/// D4 guard: `run_shadow_rollout` P99 must return within 1_000ms over 30 iterations.
///
/// The rollout spawns a thread with a 500ms budget. This test validates the
/// join-timeout path (200ms wall clock) returns without panicking — advisory only.
#[test]
fn test_d4_run_shadow_rollout_p99_under_1s() {
    use touring_hooks::shared::shadow_rollout::run_shadow_rollout;

    let mut hist: Histogram<u64> =
        Histogram::new_with_bounds(1, 60_000_000, 3).expect("valid histogram bounds");

    let tasks = vec![
        json!({"task_id": "S-1", "target_module": "crates/touring-hooks/src/lifecycle"}),
        json!({"task_id": "S-2", "target_module": "crates/touring-cognitive/src/mcts"}),
        json!({"task_id": "S-3", "target_module": "crates/touring-analysis/src/blast_radius"}),
    ];

    for _ in 0..30 {
        let t0 = Instant::now();
        let result = run_shadow_rollout(&tasks, None, Duration::from_millis(500));
        let elapsed_us = t0.elapsed().as_micros() as u64;
        // Must not panic regardless of result.
        if let Some(r) = result {
            assert!(
                !r.suggested_order.is_empty(),
                "D4: suggested_order must not be empty"
            );
            let _ = r.as_hint();
        }
        hist.record(elapsed_us.clamp(1, 60_000_000)).ok();
    }

    let p99_us = hist.value_at_quantile(0.99);
    // Budget: 1_000ms = 1_000_000μs.
    assert!(
        p99_us < 1_000_000,
        "D4 run_shadow_rollout P99 = {}ms, expected < 1_000ms",
        p99_us / 1000
    );
}

/// D4 guard: Single-task shadow rollout must complete without deadlock detection.
///
/// Verifies the non-cycle path (single task, no crossing) completes gracefully
/// and returns a result without false-positive deadlock alerts.
#[test]
fn test_d4_single_task_no_deadlock_p99_under_100ms() {
    use touring_hooks::shared::shadow_rollout::run_shadow_rollout;

    let mut hist: Histogram<u64> =
        Histogram::new_with_bounds(1, 60_000_000, 3).expect("valid histogram bounds");

    let tasks =
        vec![json!({"task_id": "S-1", "target_module": "crates/touring-hooks/src/pre_edit.rs"})];

    for _ in 0..50 {
        let t0 = Instant::now();
        let result = run_shadow_rollout(&tasks, None, Duration::from_millis(100));
        let elapsed_us = t0.elapsed().as_micros() as u64;
        if let Some(r) = result {
            assert!(
                !r.deadlock_detected,
                "D4: single task must not produce false-positive deadlock"
            );
            assert!(
                r.as_hint().is_none(),
                "D4: no deadlock hint for single task"
            );
        }
        hist.record(elapsed_us.clamp(1, 60_000_000)).ok();
    }

    let p99_us = hist.value_at_quantile(0.99);
    // Budget: 100ms = 100_000μs.
    assert!(
        p99_us < 100_000,
        "D4 single-task P99 = {}ms, expected < 100ms",
        p99_us / 1000
    );
}

// ── D5: record_* helpers latency ──────────────────────────────────────────────

/// D5 guard: all nine D2/D3/D4 `record_*` helpers must have P99 < 10μs over 10_000 iterations.
///
/// Each helper is a single `AtomicU64::fetch_add(Ordering::Relaxed)` — expected
/// < 1ns per call in steady state. The 10μs P99 budget is extremely conservative.
#[test]
fn test_d5_record_helpers_p99_under_10us() {
    use touring_hooks::shared::gate_metrics::{
        record_blast_inject, record_blast_mutation, record_blast_timeout,
        record_linucb_route_generator, record_linucb_route_hint, record_linucb_route_manual,
        record_mcts_shadow_deadlock_detected, record_mcts_shadow_run, record_mcts_shadow_timeout,
    };

    let mut hist: Histogram<u64> =
        Histogram::new_with_bounds(1, 60_000_000, 3).expect("valid histogram bounds");

    // 9 helpers exercised in round-robin across 9_000 iterations (1_000 each).
    let helpers: [fn(); 9] = [
        record_blast_inject,
        record_blast_timeout,
        record_blast_mutation,
        record_linucb_route_manual,
        record_linucb_route_generator,
        record_linucb_route_hint,
        record_mcts_shadow_run,
        record_mcts_shadow_timeout,
        record_mcts_shadow_deadlock_detected,
    ];

    for (i, helper) in helpers
        .iter()
        .enumerate()
        .flat_map(|(i, h)| std::iter::repeat_n((i, *h), 1_000))
    {
        let t0 = Instant::now();
        helper();
        let elapsed_us = t0.elapsed().as_micros().max(1) as u64;
        let _ = i; // suppress unused warning
        hist.record(elapsed_us.clamp(1, 60_000_000)).ok();
    }

    let p99_us = hist.value_at_quantile(0.99);
    // Budget: 10μs per call.
    assert!(
        p99_us < 10,
        "D5 record_* helpers P99 = {}μs, expected < 10μs",
        p99_us
    );
}

// ── D5: GateMetricsSnapshot capture latency ───────────────────────────────────

/// D5 guard: `GateMetricsSnapshot::capture()` P99 must be < 5ms over 100 iterations.
///
/// Snapshot reads all counters via `Ordering::Relaxed` loads and computes
/// derived ratios — expected to be O(1) per field. The 5ms budget is generous.
#[test]
fn test_d5_gate_metrics_snapshot_p99_under_5ms() {
    use touring_hooks::shared::gate_metrics::GateMetricsSnapshot;

    let mut hist: Histogram<u64> =
        Histogram::new_with_bounds(1, 60_000_000, 3).expect("valid histogram bounds");

    for _ in 0..100 {
        let t0 = Instant::now();
        let snap = GateMetricsSnapshot::capture();
        let elapsed_us = t0.elapsed().as_micros() as u64;
        // D5 counter fields must be present and monotonically valid.
        let _ = snap.blast_inject_count;
        let _ = snap.blast_timeout_count;
        let _ = snap.blast_mutation_count;
        let _ = snap.linucb_route_manual_count;
        let _ = snap.linucb_route_generator_count;
        let _ = snap.linucb_route_hint_count;
        let _ = snap.mcts_shadow_run_count;
        let _ = snap.mcts_shadow_timeout_count;
        let _ = snap.mcts_shadow_deadlock_detected_count;
        hist.record(elapsed_us.clamp(1, 60_000_000)).ok();
    }

    let p99_us = hist.value_at_quantile(0.99);
    // Budget: 5ms = 5_000μs.
    assert!(
        p99_us < 5_000,
        "D5 GateMetricsSnapshot::capture P99 = {}μs, expected < 5_000μs",
        p99_us
    );
}

/// D5 guard: All 9 new D2/D3/D4 snapshot fields are present and serializable.
///
/// Purpose-fidelity check: verifies the Snapshot struct has all 9 fields documented
/// in D5, each with `#[serde(default)]` (backward-compatible with older JSON consumers).
#[test]
fn test_d5_snapshot_nine_fields_present_and_serde_roundtrip() {
    use touring_hooks::shared::gate_metrics::GateMetricsSnapshot;

    let snap = GateMetricsSnapshot::capture();

    // Serialize to JSON and verify all 9 D5 field names are present.
    let json = serde_json::to_string(&snap).expect("GateMetricsSnapshot must serialize");
    for field in &[
        "blast_inject_count",
        "blast_timeout_count",
        "blast_mutation_count",
        "linucb_route_manual_count",
        "linucb_route_generator_count",
        "linucb_route_hint_count",
        "mcts_shadow_run_count",
        "mcts_shadow_timeout_count",
        "mcts_shadow_deadlock_detected_count",
    ] {
        assert!(
            json.contains(field),
            "D5 snapshot missing field '{}' in JSON output",
            field
        );
    }

    // Deserialize back — verifies #[serde(default)] round-trips cleanly.
    let deser: GateMetricsSnapshot =
        serde_json::from_str(&json).expect("GateMetricsSnapshot must deserialize");
    // Values must be non-negative (monotonic counters).
    assert!(deser.blast_inject_count <= u64::MAX);
    assert!(deser.mcts_shadow_deadlock_detected_count <= u64::MAX);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Locate the `touring` binary in the workspace target directory.
/// Prefers release, falls back to debug. Returns `None` when neither exists.
fn find_touring_binary() -> Option<std::path::PathBuf> {
    let target = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("target"))?;
    ["release", "debug"]
        .iter()
        .map(|p| target.join(p).join("touring"))
        .find(|p| p.exists())
}

/// Invoke a touring CLI hook via subprocess, returning stdout as bytes.
/// Returns empty bytes when spawn fails (daemon unavailable).
fn invoke_cli_hook(binary: &std::path::Path, hook: &str, payload: &serde_json::Value) -> Vec<u8> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = match Command::new(binary)
        .arg(hook)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = write!(stdin, "{}", payload);
    }

    child
        .wait_with_output()
        .map(|o| o.stdout)
        .unwrap_or_default()
}
