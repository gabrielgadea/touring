//! D38 — 1%-update benchmarks for touring-analysis.
//!
//! Measures wall-clock time for re-computing quality metrics when a single
//! function body changes inside an otherwise-stable file.  The result feeds the
//! CI regression gate: if an update run is > 10 % slower than the baseline
//! stored in `docs/perf-baseline.json`, the build fails.
//!
//! ## Baseline file
//!
//! `docs/perf-baseline.json` must be committed alongside this bench.  It is read
//! at startup; missing fields are initialised with the current measurement so
//! the first run always passes.

use std::fs;
use std::hint::black_box;
use std::time::Instant;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use touring_analysis::quality::estimate_complexity;

// ── Fixtures ─────────────────────────────────────────────────────────────────

/// ~1 KB Python source that is stable across update iterations.
const PYTHON_STABLE: &str = include_str!("fixtures/incremental_py.py.txt");

/// Single function body injected into the stable source to simulate a 1 % edit.
const PYTHON_PATCH: &str = r#"

def compute_digest(data: str) -> str:
    """Compute a SHA-256 digest of the input string."""
    import hashlib
    return hashlib.sha256(data.encode()).hexdigest()
"#;

// ── Baseline I/O ───────────────────────────────────────────────────────────────

fn baseline_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("docs")
        .join("perf-baseline.json")
}

#[derive(Debug, Default, Clone, serde::Deserialize, serde::Serialize)]
struct Baseline {
    incremental_py_ms: Option<f64>,
}

fn load_baseline() -> Baseline {
    let path = baseline_path();
    if path.exists() {
        let raw = fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str(&raw).unwrap_or_default()
    } else {
        Baseline::default()
    }
}

fn save_baseline(baseline: &Baseline) {
    let path = baseline_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let raw = serde_json::to_string_pretty(baseline).unwrap_or_default();
    let _ = fs::write(&path, raw);
}

// ── Benchmark ────────────────────────────────────────────────────────────────

fn bench_incremental_python_update(c: &mut Criterion) {
    let baseline = load_baseline();
    let baseline_bytes = PYTHON_STABLE.len() as u64;

    let mut group = c.benchmark_group("incremental_update/python");
    group.throughput(Throughput::Bytes(baseline_bytes));

    // Build the patched source: inject PYTHON_PATCH before the last blank line.
    let patched: String = PYTHON_STABLE
        .lines()
        .enumerate()
        .map(|(i, l)| {
            if i == PYTHON_STABLE.lines().count().saturating_sub(2) {
                format!("{}\n{}", l, PYTHON_PATCH.trim())
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Warm up outside the measured closure.
    let _ = estimate_complexity(black_box(&patched), black_box("python"));

    group.bench_function("incremental_update/python", |b| {
        let timer = Instant::now();
        b.iter(|| {
            let metrics = estimate_complexity(black_box(&patched), black_box("python"));
            black_box(metrics)
        });
        let elapsed_ms = timer.elapsed().as_millis() as f64;

        // Move a clone into the closure so we can still use `baseline` after.
        let baseline = baseline.clone();
        if let Some(stored) = baseline.incremental_py_ms {
            let regression = (elapsed_ms - stored) / stored;
            if regression < -0.10 || regression > 0.10 {
                eprintln!(
                    "REGRESSION: incremental_py update {:.1}% vs baseline {:.3} ms (current {:.3} ms)",
                    regression * 100.0,
                    stored,
                    elapsed_ms
                );
                std::process::exit(1);
            }
        } else {
            let mut new_baseline = baseline;
            new_baseline.incremental_py_ms = Some(elapsed_ms);
            save_baseline(&new_baseline);
        }
    });

    group.finish();
}

// ── Registration ─────────────────────────────────────────────────────────────

criterion_group!(
    name = incremental_benches;
    config = Criterion::default().sample_size(50);
    targets = bench_incremental_python_update
);

criterion_main!(incremental_benches);
