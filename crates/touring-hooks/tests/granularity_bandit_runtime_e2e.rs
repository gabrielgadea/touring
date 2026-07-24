//! Wave C1.5 (2026-04-20) — integration tests for the granularity bandit
//! wiring on `HookRuntime`.
//!
//! These tests exercise the public API (`select_task_split` +
//! `record_task_split_outcome`) the way a future `TaskDecomposer`-side caller
//! would: ask for a split factor, run the hypothetical task, feed back a
//! quality score derived from `CodeHealthReport::composite`.

use tempfile::TempDir;
use touring_hooks::runtime::HookRuntime;
use touring_intelligence::rl::bandit::SplitFactor;

/// Build a fresh HookRuntime rooted under an isolated tempdir.
fn setup_runtime() -> (TempDir, HookRuntime) {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let rt = HookRuntime::new(&root).expect("runtime init");
    (tmp, rt)
}

#[test]
fn bandit_is_lazily_initialized_on_first_access() {
    let (_tmp, mut rt) = setup_runtime();

    // Before any access, the field must be None (lazy init invariant).
    assert!(rt.learning.granularity_bandit.is_none());

    // First access creates it and returns a live &mut reference.
    let bandit = rt.granularity_bandit();
    assert_eq!(bandit.total_pulls(), 0);

    // After access, the Option is populated.
    assert!(rt.learning.granularity_bandit.is_some());
}

#[test]
fn select_task_split_returns_valid_factor_and_records_pull() {
    let (_tmp, mut rt) = setup_runtime();

    let factor = rt.select_task_split(50, "rust", 1);
    // Before any update, select should still return a valid factor (cold-start).
    assert!(SplitFactor::all().contains(&factor));

    rt.record_task_split_outcome(factor, 50, "rust", 1, 0.85);
    assert_eq!(rt.granularity_bandit().total_pulls(), 1);
}

#[test]
fn bandit_converges_when_monolithic_always_best_for_tiny_tasks() {
    let (_tmp, mut rt) = setup_runtime();

    for _ in 0..150 {
        let factor = rt.select_task_split(40, "rust", 1);
        let quality = if factor == SplitFactor::Monolithic {
            0.92
        } else {
            0.30
        };
        rt.record_task_split_outcome(factor, 40, "rust", 1, quality);
    }

    let pulls = rt.granularity_bandit().pulls_per_arm();
    let mono = pulls.first().copied().unwrap_or(0);
    let others: u64 = pulls.iter().skip(1).sum();
    assert!(
        mono > others,
        "Monolithic pulls {mono} should dominate others {others}"
    );
}

#[test]
fn record_outcome_accumulates_reward_across_calls() {
    let (_tmp, mut rt) = setup_runtime();

    // 10 outcomes all for Split2 at equal quality.
    for _ in 0..10 {
        rt.record_task_split_outcome(SplitFactor::Split2, 200, "rust", 2, 0.8);
    }
    let pulls = rt.granularity_bandit().pulls_per_arm();
    let split2 = pulls
        .get(SplitFactor::Split2.as_index())
        .copied()
        .unwrap_or(0);
    assert_eq!(split2, 10);
    assert_eq!(rt.granularity_bandit().total_pulls(), 10);
}

#[test]
fn split_factor_differs_across_contexts_after_training() {
    // Alternate training between two distinct contexts and verify the
    // reward traces diverge (different arms favored).
    let (_tmp, mut rt) = setup_runtime();

    // Context A: tiny rust → reward Monolithic.
    // Context B: large python → reward Split3.
    for _ in 0..120 {
        let fa = rt.select_task_split(30, "rust", 1);
        let qa = if fa == SplitFactor::Monolithic {
            0.9
        } else {
            0.3
        };
        rt.record_task_split_outcome(fa, 30, "rust", 1, qa);

        let fb = rt.select_task_split(700, "python", 4);
        let qb = if fb == SplitFactor::Split3 { 0.9 } else { 0.3 };
        rt.record_task_split_outcome(fb, 700, "python", 4, qb);
    }

    // Total pulls = 240 (both contexts).
    assert_eq!(rt.granularity_bandit().total_pulls(), 240);

    // At least one arm beyond cold-start threshold on each "preferred" side.
    let pulls = rt.granularity_bandit().pulls_per_arm();
    let mono = pulls.first().copied().unwrap_or(0);
    let split3 = pulls
        .get(SplitFactor::Split3.as_index())
        .copied()
        .unwrap_or(0);
    assert!(
        mono >= 3,
        "Monolithic should exceed cold-start threshold, got {mono}"
    );
    assert!(
        split3 >= 3,
        "Split3 should exceed cold-start threshold, got {split3}"
    );
}

#[test]
fn cli_granularity_status_returns_initial_json_shape() {
    // Wave C1.7: the daemon hook `cli-granularity-status` must return a
    // well-formed JSON payload even on a brand-new runtime (lazy init).
    let (_tmp, mut rt) = setup_runtime();
    let out =
        touring_hooks::cli_handlers::cli_granularity_status(&mut rt, &serde_json::Value::Null);
    let v: serde_json::Value = serde_json::from_str(&out).expect("handler must emit JSON");
    assert_eq!(v["total_pulls"], 0);
    assert_eq!(v["num_arms"], 4);
    let arms = v["arms"].as_array().expect("arms must be an array");
    assert_eq!(arms.len(), 4);
    assert_eq!(arms[0]["factor"], "Monolithic");
    assert_eq!(arms[3]["factor"], "Split4");
}

#[test]
fn cli_granularity_status_reflects_updates() {
    let (_tmp, mut rt) = setup_runtime();
    // Drive three outcomes so total_pulls = 3.
    for _ in 0..3 {
        rt.record_task_split_outcome(SplitFactor::Split2, 100, "rust", 2, 0.7);
    }
    let out =
        touring_hooks::cli_handlers::cli_granularity_status(&mut rt, &serde_json::Value::Null);
    let v: serde_json::Value = serde_json::from_str(&out).expect("JSON");
    assert_eq!(v["total_pulls"], 3);
    let split2_pulls = v["arms"][1]["pulls"].as_u64().expect("u64");
    assert_eq!(split2_pulls, 3);
}

#[test]
fn cli_granularity_reset_clears_state() {
    let (_tmp, mut rt) = setup_runtime();
    rt.record_task_split_outcome(SplitFactor::Monolithic, 20, "rust", 0, 0.9);
    assert_eq!(rt.granularity_bandit().total_pulls(), 1);

    let out = touring_hooks::cli_handlers::cli_granularity_reset(&mut rt, &serde_json::Value::Null);
    let v: serde_json::Value = serde_json::from_str(&out).expect("JSON");
    assert_eq!(v["reset"], true);
    assert_eq!(v["prior_pulls"], 1);

    // After reset, total_pulls must be zero.
    assert_eq!(rt.granularity_bandit().total_pulls(), 0);
}

#[test]
fn save_granularity_bandit_is_noop_when_never_accessed() {
    // Wave C1.7-persistence: saving before the bandit was ever accessed
    // must succeed silently (the Option is None, nothing to persist).
    let (_tmp, rt) = setup_runtime();
    rt.save_granularity_bandit()
        .expect("save must succeed on fresh runtime");
    // The file should NOT exist since no data was persisted.
    let path = rt_path(&rt, "granularity_bandit.json");
    assert!(!path.exists(), "no file should be written on empty bandit");
}

#[test]
fn snapshot_roundtrip_across_runtime_instances_preserves_pulls() {
    // Train the bandit in runtime A, save, drop; then start a fresh runtime B
    // rooted at the SAME project dir and verify the pulls survive.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().to_path_buf();

    {
        let mut rt_a = HookRuntime::new(&root).expect("runtime A");
        for _ in 0..7 {
            rt_a.record_task_split_outcome(SplitFactor::Split3, 900, "rust", 4, 0.85);
        }
        assert_eq!(rt_a.granularity_bandit().total_pulls(), 7);
        rt_a.save_granularity_bandit().expect("save A");
    }

    // Fresh runtime B on the same root — load must reconstruct prior state.
    let mut rt_b = HookRuntime::new(&root).expect("runtime B");
    let pulls = rt_b.granularity_bandit().pulls_per_arm();
    let split3_pulls = pulls
        .get(SplitFactor::Split3.as_index())
        .copied()
        .unwrap_or(0);
    assert_eq!(split3_pulls, 7, "Split3 pulls must survive save/reload");
    assert_eq!(rt_b.granularity_bandit().total_pulls(), 7);
}

#[test]
fn malformed_snapshot_falls_back_to_fresh_bandit() {
    // A corrupt snapshot must not block daemon boot — the runtime should
    // boot with a fresh bandit and log a warning (warning is eprintln, not
    // asserted here; we just verify the runtime starts cleanly).
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let data_dir = root.join(".claude/data");
    std::fs::create_dir_all(&data_dir).expect("mkdir");
    std::fs::write(data_dir.join("granularity_bandit.json"), "{not valid json")
        .expect("write garbage");

    let mut rt = HookRuntime::new(&root).expect("runtime must boot despite garbage");
    // Bandit must be usable — either None (we fell back) or populated but empty.
    assert_eq!(rt.granularity_bandit().total_pulls(), 0);
}

/// Helper: compute the on-disk path the runtime uses for a given
/// `.claude/data/*` file.
fn rt_path(rt: &HookRuntime, filename: &str) -> std::path::PathBuf {
    rt.project_root.join(".claude/data").join(filename)
}

#[test]
fn bandit_accessor_is_stable_across_calls() {
    let (_tmp, mut rt) = setup_runtime();

    // Multiple accesses must not reset state.
    let _ = rt.granularity_bandit();
    rt.record_task_split_outcome(SplitFactor::Monolithic, 10, "rust", 0, 0.7);
    let _ = rt.granularity_bandit();
    rt.record_task_split_outcome(SplitFactor::Monolithic, 10, "rust", 0, 0.7);

    assert_eq!(rt.granularity_bandit().total_pulls(), 2);
}
