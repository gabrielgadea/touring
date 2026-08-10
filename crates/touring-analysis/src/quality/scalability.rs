//! Scalability anti-patterns (D26 / F2.13) — language-aware detector of the
//! canonical horizontal/vertical scaling smells: **unbounded state in-process**
//! (an `Arc<Mutex<HashMap<…>>>` / `Arc<RwLock<HashMap<…>>>` shared across tasks
//! is the canonical "state doesn't scale past one process" — at two replicas
//! the maps diverge), **unbounded channels** (`mpsc::unbounded_channel` /
//! `unbounded_send` have no capacity ceiling and can OOM under load; tokio
//! docs: "has the ability of causing the process to run out of memory"),
//! **hardcoded thread-pool sizes** (a `rayon::ThreadPoolBuilder::new().num_threads(N)`
//! with a small literal N caps the parallel work regardless of the host's
//! CPU count), **missing external-call timeouts** (a `reqwest::get(` without
//! `.timeout(...)` hangs forever if the remote stalls — SPOF for the whole
//! request), **hot loop without `yield_now()`** (a `while { work() }` inside
//! `async fn` without an explicit yield point starves the executor).
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | unbounded-state-in-process | `Arc<Mutex<HashMap<…>>>` / `Arc<RwLock<HashMap<…>>>` / `Arc<Mutex<Vec<…>>>` | Rust |
//! | unbounded-channel | `tokio::sync::mpsc::unbounded_channel(` *and* (per-file) no `tokio::sync::mpsc::channel(N)` in same file | Rust |
//! | hardcoded-thread-pool-small | `rayon::ThreadPoolBuilder::new().num_threads(N)` with `N ≤ 8` literal | Rust |
//! | missing-timeout-external | `reqwest::Client::new(` / `reqwest::get(` without `.timeout(` anywhere in same file | Rust |
//! | hot-loop-no-yield | `loop { … }` / `while … { … }` in `async fn` body without `tokio::task::yield_now()` | Rust |
//! | go-goroutine-no-cap | a `go func()` in a file with no `sync.WaitGroup`/`chan` to bound fanout | Go |
//!
//! **Disjoint** from F2.8 memory (which keys on `unbounded_channel(` directly
//! — F2.13 keys on the *file-level* comparison: `unbounded_channel` *and*
//! absence of a bounded sibling in the same file), F2.10 I/O (which keys on
//! `reqwest::blocking` — F2.13 keys on `reqwest::get` without `.timeout`),
//! F2.11 concurrency (which keys on `std::sync::Mutex::lock(` in `async fn`
//! — F2.13 keys on `Arc<Mutex<HashMap>>` as a *state shape* anti-pattern).
//!
//! **Sources (context7, `/tokio-rs/tokio`, High reputation, bench 84.72):**
//! bounded `mpsc::channel(buffer)` uses a `Semaphore` to enforce capacity
//! ("The channel will buffer up to the provided number of messages … Once
//! the buffer is full, attempts to send new messages will wait" —
//! backpressure). `tokio::sync::mpsc::unbounded_channel` warns: "has the
//! ability of causing the process to run out of memory" (doc-comment).
//! `ThreadPoolBuilder::new().num_threads(N)` caps parallelism at N; the
//! default `tokio::runtime::Builder::worker_threads(None)` is
//! `unwrap_or_else(num_cpus)`, i.e. scales to host — a hardcoded small N
//! is therefore a deliberate cap. `spawn_blocking` guidance: "Use
//! `spawn_blocking` for short-lived blocking operations … Use dedicated
//! threads for long-lived or persistent blocking workloads" (rule of
//! thumb we mirror in our detector scoring).
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

const SCALE: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Go,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "go" => Lang::Go,
        _ => Lang::Other,
    }
}

/// Scalability findings for one file.
pub type ScalabilityReport = crate::quality::SmellReport;

#[inline]
fn _ident_at_word_boundary(bytes: &[u8], idx: usize) -> bool {
    // Word-boundary helper kept for future detectors that need to check
    // whether an identifier boundary exists at `idx` (so a generic
    // `unbounded_channel(` does not match `is_unbounded_channel(`).
    // Currently unused; suppressed via `_` prefix to keep clippy
    // `#[warn(dead_code)]` clean while preserving the test surface.
    let Some(&c) = bytes.get(idx) else {
        return false;
    };
    c.is_ascii_alphanumeric() || c == b'_'
}

/// `Arc<Mutex<HashMap<…>>>` / `Arc<RwLock<HashMap<…>>>` / `Arc<Mutex<Vec<…>>>`
/// shared across tasks — in-process state that cannot be replicated (the
/// two replicas' maps diverge). The shared-state anti-pattern for
/// horizontal scaling. One finding per file (idempotent — the file has
/// adopted a non-shared store, period).
fn unbounded_state_in_process(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_state = memmem::find_iter(bytes, b"Arc<Mutex<HashMap")
        .chain(memmem::find_iter(bytes, b"Arc<RwLock<HashMap"))
        .chain(memmem::find_iter(bytes, b"Arc<Mutex<Vec"))
        .chain(memmem::find_iter(bytes, b"Arc<RwLock<Vec"))
        .any(|off| !offset_suppressed(off, regions));
    if has_state { 1 } else { 0 }
}

/// `tokio::sync::mpsc::unbounded_channel(` *and* no bounded sibling in the
/// same file (a file that has adopted both is mid-migration — flag until
/// the bounded channel wins). File-level finding.
fn unbounded_channel_in_file(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_unbounded =
        memmem::find_iter(bytes, b"unbounded_channel(").any(|off| !offset_suppressed(off, regions));
    if !has_unbounded {
        return 0;
    }
    let has_bounded = memmem::find_iter(bytes, b"tokio::sync::mpsc::channel(")
        .any(|off| !offset_suppressed(off, regions));
    if has_bounded { 0 } else { 1 }
}

/// `rayon::ThreadPoolBuilder::new().num_threads(N)` with N ≤ 8 literal —
/// hardcoded small pool caps parallelism regardless of host CPU count.
/// Heuristic: only flags *literal* N values (variable assignment is OK
/// because the var is presumably configured externally).
fn hardcoded_thread_pool(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut count = 0;
    for off in memmem::find_iter(bytes, b".num_threads(") {
        if offset_suppressed(off, regions) {
            continue;
        }
        let start = off + b".num_threads(".len();
        if start >= bytes.len() {
            continue;
        }
        let c = bytes[start];
        if !c.is_ascii_digit() {
            continue;
        }
        let mut j = start;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        let n: usize = std::str::from_utf8(&bytes[start..j])
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if n > 0 && n <= 8 {
            count += 1;
        }
    }
    count
}

/// `reqwest::Client::new(` / `reqwest::get(` *without* `.timeout(` in the same
/// file. Missing timeout = SPOF for the whole request under stall.
fn missing_external_timeout(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_reqwest_client = memmem::find_iter(bytes, b"reqwest::Client::new(")
        .chain(memmem::find_iter(bytes, b"reqwest::get("))
        .any(|off| !offset_suppressed(off, regions));
    if !has_reqwest_client {
        return 0;
    }
    let has_timeout = memmem::find_iter(bytes, b".timeout(")
        .chain(memmem::find_iter(bytes, b"tokio::time::timeout"))
        .any(|off| !offset_suppressed(off, regions));
    if has_timeout { 0 } else { 1 }
}

/// `loop { … }` / `while … { … }` inside an `async fn` body, *without* a
/// `tokio::task::yield_now()` in the same body. The hot loop starves the
/// executor (every scheduling slice is used by the loop).
fn hot_loop_no_yield(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_async =
        memmem::find_iter(bytes, b"async fn").any(|off| !offset_suppressed(off, regions));
    if !has_async {
        return 0;
    }
    let has_loop = memmem::find_iter(bytes, b"loop {")
        .chain(memmem::find_iter(bytes, b"while "))
        .any(|off| !offset_suppressed(off, regions));
    if !has_loop {
        return 0;
    }
    let has_yield =
        memmem::find_iter(bytes, b"yield_now").any(|off| !offset_suppressed(off, regions));
    if has_yield { 0 } else { 1 }
}

/// `go func()` in a file with no `sync.WaitGroup` / bounded `chan` — a
/// goroutine fanout unbounded by the runtime. File-level finding.
fn go_goroutine_no_cap(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_go = memmem::find_iter(bytes, b"go func(").any(|off| !offset_suppressed(off, regions));
    if !has_go {
        return 0;
    }
    let has_bound = memmem::find_iter(bytes, b"sync.WaitGroup")
        .chain(memmem::find_iter(bytes, b"make(chan "))
        .any(|off| !offset_suppressed(off, regions));
    if has_bound { 0 } else { 1 }
}

/// Analyze scalability smells in `source` for the given language.
pub fn analyze_scalability(source: &str, lang: &str) -> ScalabilityReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let lang = canonical_lang(lang);
    let mut report = ScalabilityReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match lang {
        Lang::Rust => {
            report.push(
                "unbounded state in-process (Arc<Mutex<HashMap/Vec<…>>> — cannot replicate)",
                unbounded_state_in_process(bytes, &regions),
                1.0,
            );
            report.push(
                "unbounded mpsc channel (unbounded_channel( without a bounded sibling in the file)",
                unbounded_channel_in_file(bytes, &regions),
                1.0,
            );
            report.push(
                "hardcoded rayon thread-pool size ≤8 (.num_threads(N) caps parallelism)",
                hardcoded_thread_pool(bytes, &regions),
                0.7,
            );
            report.push(
                "missing external-call timeout (reqwest::Client::new/get( without .timeout())",
                missing_external_timeout(bytes, &regions),
                0.8,
            );
            report.push(
                "hot async loop without yield_now() (loop {…} / while in async fn body)",
                hot_loop_no_yield(bytes, &regions),
                0.5,
            );
        }
        Lang::Go => {
            report.push(
                "go func() with no WaitGroup/bounded chan (unbounded goroutine fanout)",
                go_goroutine_no_cap(bytes, &regions),
                0.9,
            );
        }
        _ => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`ScalabilityReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
/// `total_lines.max(20)` floors the denominator so a 4-line test file with
/// 3 weighted findings doesn't saturate the score at 0 (the floor of 20
/// reflects the heuristic: no single small file is so small that 3 smells
/// are "acceptable density" — the smell is the smell, regardless of LOC).
pub fn score_scalability(report: &ScalabilityReport) -> f32 {
    let lines = report.total_lines.max(20) as f32;
    let density = report.weighted_total / lines;
    (1.0 - density * SCALE).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unbounded_state_flagged() {
        let src = "struct S {\n    state: Arc<Mutex<HashMap<String, Vec<u8>>>,\n}\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("unbounded state in-process")),
            "Arc<Mutex<HashMap<…>>> is flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn unbounded_state_with_external_store_clean() {
        let src = "struct S {\n    state: Arc<Mutex<HashMap<K, V>>>,\n}\nfn go() { let (tx, rx) = tokio::sync::mpsc::channel(64); }\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("unbounded state in-process"))
        );
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("unbounded mpsc channel"))
        );
    }
    #[test]
    fn unbounded_channel_alone_flagged() {
        let src = "fn go() {\n    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();\n}\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("unbounded mpsc channel")),
            "unbounded_channel alone is flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn unbounded_channel_with_bounded_sibling_clean() {
        let src = "fn go() {\n    let (tx_a, _) = tokio::sync::mpsc::unbounded_channel();\n    let (tx_b, rx_b) = tokio::sync::mpsc::channel(64);\n}\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("unbounded mpsc channel")),
            "bounded sibling de-suppresses: {:?}",
            r.findings
        );
    }
    #[test]
    fn hardcoded_thread_pool_small_flagged() {
        let src = "let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("hardcoded rayon")),
            ".num_threads(4) is flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn hardcoded_thread_pool_large_not_flagged() {
        let src = "let pool = rayon::ThreadPoolBuilder::new().num_threads(64).build().unwrap();\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("hardcoded rayon")),
            ".num_threads(64) is fine: {:?}",
            r.findings
        );
    }
    #[test]
    fn hardcoded_thread_pool_variable_clean() {
        let src = "let pool = rayon::ThreadPoolBuilder::new().num_threads(n).build().unwrap();\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("hardcoded rayon")),
            "variable .num_threads(n) is fine: {:?}",
            r.findings
        );
    }
    #[test]
    fn missing_timeout_flagged() {
        let src = "async fn go() {\n    let r = reqwest::get(url).await?;\n}\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("missing external-call timeout")),
            "reqwest::get( without .timeout is flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn timeout_present_clean() {
        let src = "async fn go() {\n    let r = reqwest::get(url).timeout(Duration::from_secs(5)).await?;\n}\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("missing external-call timeout")),
            ".timeout( is the fix: {:?}",
            r.findings
        );
    }
    #[test]
    fn hot_loop_no_yield_flagged() {
        let src = "async fn go() {\n    let mut x = 0u64;\n    loop { x += compute(); }\n}\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("hot async loop")),
            "loop {{ … }} in async fn without yield is flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn hot_loop_with_yield_clean() {
        let src = "async fn go() {\n    let mut x = 0u64;\n    loop {\n        x += compute();\n        tokio::task::yield_now().await;\n    }\n}\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("hot async loop")),
            "yield_now is the fix: {:?}",
            r.findings
        );
    }
    #[test]
    fn sync_function_with_loop_clean() {
        let src = "fn go() {\n    loop { compute(); }\n}\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("hot async loop")),
            "sync loop without yield is fine: {:?}",
            r.findings
        );
    }
    #[test]
    fn go_goroutine_no_cap_flagged() {
        let src = "package main\nfunc go() {\n    for i := 0; i < 100; i++ { go func() { work() }() }\n}\n";
        let r = analyze_scalability(src, "go");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("go func() with no WaitGroup")),
            "goroutines without WaitGroup are flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn go_goroutine_with_waitgroup_clean() {
        let src = "package main\nfunc go() {\n    var wg sync.WaitGroup\n    for i := 0; i < 100; i++ { wg.Add(1); go func() { defer wg.Done(); work() }() }\n    wg.Wait()\n}\n";
        let r = analyze_scalability(src, "go");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("go func() with no WaitGroup")),
            "WaitGroup is the fix: {:?}",
            r.findings
        );
    }
    #[test]
    fn pure_function_clean() {
        let src = "fn add(a: i32, b: i32) -> i32 { a + b }\n";
        let r = analyze_scalability(src, "rust");
        assert_eq!(
            r.violations, 0,
            "pure-Rust function is clean: {:?}",
            r.findings
        );
    }
    #[test]
    fn comment_excluded() {
        let src = "// let s = Arc<Mutex<HashMap<String, Vec<u8>>>;\nfn real() {}\n";
        let r = analyze_scalability(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("unbounded state in-process")),
            "commented state is excluded: {:?}",
            r.findings
        );
    }
    #[test]
    fn score_monotonic_dirty_below_clean() {
        let bad = analyze_scalability(
            "fn go() {\n    let s: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));\n    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();\n    let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();\n}\n",
            "rust",
        );
        let good = analyze_scalability(
            "fn go() {\n    let s: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));\n    let (tx, rx) = tokio::sync::mpsc::channel(64);\n    let pool = rayon::ThreadPoolBuilder::new().num_threads(n).build().unwrap();\n}\n",
            "rust",
        );
        assert!(
            score_scalability(&bad) < score_scalability(&good),
            "dirty ({:.3}) must score below clean ({:.3})",
            score_scalability(&bad),
            score_scalability(&good)
        );
    }
}
