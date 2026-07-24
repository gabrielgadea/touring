//! Criterion latency baseline for the Code Execution Gateway (CEG) — P0.4 + P7.5.
//!
//! Pln2 plan: `docs/2026-05-17-ceg-pln2-plan.md`, deliverables P0.4 and P7.5.
//!
//! Records the P50/P99 "regression floor" for the synchronous operations the
//! CEG X0..X9 pipeline reuses on its hot path. P7.5 (criterion regression
//! gate) adds a `ceg_regression_gate` group that measures the `run_gateway`
//! end-to-end hot path with `iter_custom` and **panics if the P99 budget is
//! exceeded** — this is the hard regression gate.
//!
//! Four groups:
//!   1. `ceg_bash_validation` — `validate_command` + `command_shape`, the
//!      synchronous structural-analysis path reused by X1 CLASSIFY / X2 STATIC.
//!   2. `ceg_ast_grep_scan` — `scan_source`, the polyglot risk scan reused by
//!      X2 STATIC.
//!   3. `ceg_process_spawn` — a minimal `/bin/true` spawn: the OS process-spawn
//!      primitive underlying X5 SANDBOX / X8 SUPERVISED-EXEC.
//!   4. `ceg_regression_gate` — P7.5 hard gate: `run_gateway` + X1/X2 ops
//!      measured via `iter_custom`; asserts P99 ≤ budget.
//!
//! # Budget constants (P0.4 measured baselines)
//!
//! | Operation | P99 budget |
//! |-----------|-----------|
//! | `validate_command` (per-call) | 50 µs |
//! | `scan_source` Rust/Python | 500 µs |
//! | `run_gateway` (clean echo) | 5 000 µs |
//!
//! # Regression gate design (P7.5)
//!
//! `ceg_regression_gate` uses `Bencher::iter_custom` to record raw wall-clock
//! durations, compute the P99, and **assert it fits within the P0.4 budget**.
//! The gate fails loudly (panic) so CI catches regressions immediately.
//!
//! In smoke-run mode (`-- --test`) criterion runs each body once; the
//! single-sample assertion is skipped (`if iters > 1`).
//!
//! Note on sandbox spawn: in production the sandbox dry-run latency is
//! dominated by interpreter cold-start (python3/node — tens to hundreds of ms),
//! which is runtime-bound, not Touring-code regression surface. The regression
//! gate therefore uses `deferred_dry_run` — no subprocess is ever spawned.
//!
//! Run:   `cargo bench -p touring-hooks --bench ceg_baseline`
//! Smoke: `cargo bench -p touring-hooks --bench ceg_baseline -- --test`

use std::process::Command;
use std::time::{Duration, Instant};

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

use touring_hooks::capability::builtins;
use touring_hooks::gateway::predict::ExecutionOutcomePredictor;
use touring_hooks::gateway::{
    GatewayDeps, deferred_dry_run, neutral_outcome_history, run_gateway, soft_pass_symbol,
};
use touring_hooks::shared::ast_grep_signal::{DEFAULT_BUDGET, scan_source};
use touring_hooks::shared::bash_ast_validator::{command_shape, validate_command};
use touring_hooks::shared::risk_patterns::{PYTHON_RISK, RUST_RISK};

// ── P0.4 budget constants (hard regression ceilings) ─────────────────────────

/// P99 budget for a single `validate_command` or `command_shape` call (µs).
const BASH_VALIDATION_P99_BUDGET_US: u64 = 50;

/// P99 budget for a single `scan_source` call (µs).
const AST_GREP_SCAN_P99_BUDGET_US: u64 = 500;

/// P99 budget for the `run_gateway` end-to-end hot path (µs).
///
/// Deliberately generous (5 ms) to avoid flaky CI — the gate catches
/// *catastrophic* regressions (e.g. an accidental sync DB query on the hot
/// path), not micro-variances. Tighten after P4.2 lands landlock isolation.
const RUN_GATEWAY_P99_BUDGET_US: u64 = 5_000;

/// Clean Bash snippet used for the `run_gateway` hot-path regression gate.
///
/// `echo hello` exercises the full X0..X7 pipeline without triggering any
/// risky-pattern detectors, giving a clean baseline latency signal.
const CLEAN_BASH: &str = "echo hello";

// ── Naïve percentile helper (bench-only) ─────────────────────────────────────

/// Compute a percentile over a sorted slice of durations.
///
/// `pct` is in `[0.0, 1.0]`.  Returns `Duration::ZERO` for an empty slice.
fn percentile(sorted: &[Duration], pct: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let idx = ((sorted.len() as f64 * pct).ceil() as usize).saturating_sub(1);
    sorted[idx.min(sorted.len() - 1)]
}

/// Representative bash commands spanning the validator's verdict range.
const COMMANDS: &[(&str, &str)] = &[
    ("clean_simple", "ls -la /tmp"),
    ("clean_pipe", "cat foo.txt | grep bar | wc -l"),
    ("risky_rm", "rm -rf /tmp/scratch"),
    ("risky_find_delete", "find . -name '*.tmp' -delete"),
    ("touring_cmd", "touring doctor -j"),
];

/// Rust source snippet exercising the risk pattern set (unwrap + indexing).
const RUST_SRC: &str = concat!(
    "fn main() {\n",
    "    let x: Option<i32> = Some(1);\n",
    "    let _ = x.unwrap();\n",
    "    let v: Vec<i32> = Vec::new();\n",
    "    let _first = v[0];\n",
    "}\n",
);

/// Python source snippet exercising the risk pattern set (eval + shell exec).
const PYTHON_SRC: &str = concat!(
    "import subprocess\n",
    "\n",
    "def run(value):\n",
    "    parsed = eval(value)\n",
    "    return subprocess.check_output(parsed, shell=True)\n",
);

fn bench_bash_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("ceg_bash_validation");
    for &(name, cmd) in COMMANDS {
        group.bench_function(format!("validate/{name}"), |b| {
            b.iter(|| black_box(validate_command(black_box(cmd))));
        });
        group.bench_function(format!("shape/{name}"), |b| {
            b.iter(|| black_box(command_shape(black_box(cmd))));
        });
    }
    group.finish();
}

fn bench_ast_grep_scan(c: &mut Criterion) {
    let mut group = c.benchmark_group("ceg_ast_grep_scan");
    group.bench_function("rust", |b| {
        b.iter(|| black_box(scan_source(black_box(RUST_SRC), &RUST_RISK, DEFAULT_BUDGET)));
    });
    group.bench_function("python", |b| {
        b.iter(|| {
            black_box(scan_source(
                black_box(PYTHON_SRC),
                &PYTHON_RISK,
                DEFAULT_BUDGET,
            ))
        });
    });
    group.finish();
}

fn bench_process_spawn(c: &mut Criterion) {
    let mut group = c.benchmark_group("ceg_process_spawn");
    // A process spawn is ms-scale — orders above the ns/us sync benches.
    // Trim the sample count so this group does not dominate the run.
    group.sample_size(30);
    group.bench_function("bin_true", |b| {
        b.iter(|| {
            let status = Command::new("/bin/true").status().expect("spawn /bin/true");
            black_box(status.success())
        });
    });
    group.finish();
}

// ── P7.5 regression gate ──────────────────────────────────────────────────────

/// P7.5 criterion regression gate for the `run_gateway` hot path and the
/// synchronous X1/X2 operations that underpin it.
///
/// Uses `Bencher::iter_custom` to collect raw wall-clock durations, compute
/// the P99, and **assert it stays within the P0.4 budget**.  The assertion
/// panics with a human-readable message so CI surfaces the regression
/// immediately.
///
/// # Gate behaviour
///
/// - `iters > 1` (full bench): sort durations, assert P99 ≤ budget.
/// - `iters == 1` (--test smoke): skip assertion — a single-sample P99 is
///   not representative.
///
/// # No subprocess
///
/// All three gates use `deferred_dry_run` (the production-safe X5 runner) so
/// **no subprocess is ever spawned** in bench or smoke runs.
fn bench_regression_gate(c: &mut Criterion) {
    let mut group = c.benchmark_group("ceg_regression_gate");
    // 50 samples gives a statistically meaningful P99 for µs-scale ops.
    group.sample_size(50);

    // ── Gate 1: run_gateway end-to-end (X0..X7, no subprocess) ─────────────
    //
    // Throughput annotation (Context7 best-practice 2026-05-22): exposes the
    // bench result as bytes/sec (e.g. `thrpt: [xxx MiB/s]`), making cross-
    // language and pre/post-regression comparisons quantitatively meaningful.
    group.throughput(Throughput::Bytes(CLEAN_BASH.len() as u64));
    group.bench_function(BenchmarkId::new("run_gateway", "clean_echo"), |b| {
        b.iter_custom(|iters| {
            let predictor = ExecutionOutcomePredictor::new();
            let profile = builtins::trusted();
            let deps = GatewayDeps {
                symbol_exists: &soft_pass_symbol,
                outcome_history: &neutral_outcome_history,
                sandbox_runner: &deferred_dry_run,
                predictor: &predictor,
                profile: &profile,
                // ES1 P3 (2026-06-01) — X3.5 PROVE: bench is a no-claim
                // baseline (zero Z3 cost). Documented in P1 honest scope.
                claim: None,
                claim_context: touring_hooks::offensive_integration::ClaimContext::default(),
                solver_backend: touring_hooks::offensive_integration::SolverBackendKind::Stub,
            };

            let mut durations: Vec<Duration> = Vec::with_capacity(iters as usize);
            for _ in 0..iters {
                let t0 = Instant::now();
                let result = run_gateway(black_box("Bash"), black_box(CLEAN_BASH), None, &deps);
                durations.push(t0.elapsed());
                // Consume so the call is never optimised away.
                black_box(result.is_ok());
            }

            let total: Duration = durations.iter().sum();

            if iters > 1 {
                durations.sort_unstable();
                let p99 = percentile(&durations, 0.99);
                assert!(
                    p99.as_micros() <= RUN_GATEWAY_P99_BUDGET_US as u128,
                    "CEG P7.5 REGRESSION: run_gateway P99 = {p99:?} \
                     exceeds budget {budget}µs. \
                     Investigate recent changes to the X0..X7 pipeline.",
                    budget = RUN_GATEWAY_P99_BUDGET_US,
                );
            }

            total
        });
    });

    // ── Gate 2: validate_command (X1/X2 synchronous path) ───────────────────
    group.bench_function(BenchmarkId::new("validate_command", "clean"), |b| {
        b.iter_custom(|iters| {
            let cmd = "ls -la /tmp";
            let mut durations: Vec<Duration> = Vec::with_capacity(iters as usize);
            for _ in 0..iters {
                let t0 = Instant::now();
                black_box(validate_command(black_box(cmd)));
                durations.push(t0.elapsed());
            }
            let total: Duration = durations.iter().sum();
            if iters > 1 {
                durations.sort_unstable();
                let p99 = percentile(&durations, 0.99);
                assert!(
                    p99.as_micros() <= BASH_VALIDATION_P99_BUDGET_US as u128,
                    "CEG P7.5 REGRESSION: validate_command P99 = {p99:?} \
                     exceeds budget {budget}µs.",
                    budget = BASH_VALIDATION_P99_BUDGET_US,
                );
            }
            total
        });
    });

    // ── Gate 3: scan_source Rust (X2 static-analysis path) ──────────────────
    group.bench_function(BenchmarkId::new("scan_source", "rust"), |b| {
        b.iter_custom(|iters| {
            let src = RUST_SRC;
            let mut durations: Vec<Duration> = Vec::with_capacity(iters as usize);
            for _ in 0..iters {
                let t0 = Instant::now();
                black_box(scan_source(black_box(src), &RUST_RISK, DEFAULT_BUDGET));
                durations.push(t0.elapsed());
            }
            let total: Duration = durations.iter().sum();
            if iters > 1 {
                durations.sort_unstable();
                let p99 = percentile(&durations, 0.99);
                assert!(
                    p99.as_micros() <= AST_GREP_SCAN_P99_BUDGET_US as u128,
                    "CEG P7.5 REGRESSION: scan_source(rust) P99 = {p99:?} \
                     exceeds budget {budget}µs.",
                    budget = AST_GREP_SCAN_P99_BUDGET_US,
                );
            }
            total
        });
    });

    group.finish();
}

criterion_group!(
    ceg_baseline,
    bench_bash_validation,
    bench_ast_grep_scan,
    bench_process_spawn,
    bench_regression_gate,
);
criterion_main!(ceg_baseline);
