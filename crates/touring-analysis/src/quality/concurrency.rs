//! Concurrency anti-patterns (D24 / F2.11) — language-aware detector of the four
//! most common and most dangerous concurrency smells: **lock-across-await** in
//! Rust async (a `std::sync::Mutex`/`RwLock` guard held across `.await` is
//! `!Send` and deadlocks the runtime; tokio's own guidance), **sync locks in
//! async fn** (file-level: `async fn` + `std::sync::Mutex::lock(` is a tell that
//! the author reached for a sync lock inside an async context — they probably
//! meant `tokio::sync::Mutex`), **channel-preferred state sharing**
//! (`Arc<Mutex<Vec<…>>>` for cross-task state where a `tokio::sync::mpsc`
//! channel would be simpler and contention-free), and **locks where atomics
//! would do** (`Mutex<u64>` for a counter when `AtomicU64` is lock-free —
//! the Touring gate-metrics pattern).
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | lock-across-await | `let _g = x.lock().unwrap();` *and* `.await` in the same brace-scope (the guard is still live) | Rust |
//! | sync-locks-in-async | an `async fn` in the file *and* `std::sync::Mutex::lock(`/`std::sync::RwLock::read(`/`std::sync::RwLock::write(` | Rust |
//! | arc-mutex-shared-state | `Arc<Mutex<`/`Arc<RwLock<` *and* no `tokio::sync::mpsc`/`tokio::sync::oneshot` (state-with-channel opportunity) | Rust |
//! | mutex-where-atomic | `Mutex<u64>`/`Mutex<i64>` (a counter, atomics preferred) | Rust |
//! | goroutine-mutex-race | a `go func()` *and* `sync.Mutex` in the same file (the `go` keyword mid-statement is the goroutine spawn) | Go |
//! | async-with-sync-lock | `async fn` *and* `threading.Lock`/`threading.RLock` in the same file | Python |
//!
//! **Disjoint** from F2.8 memory (which keys on `unbounded_channel`/leak/clone —
//! concurrency keys on lock-across-await / channel-preferred / atomics) and
//! from F2.10 I/O (which keys on `std::fs::` in `async fn` + `block_on(` —
//! concurrency keys on `Mutex::lock(`/`RwLock::read(` in `async fn`).
//! `lock-across-await` requires the *await* to be present in the same scope as
//! the *lock guard*; neither F2.8 nor F2.10 inspects that scope relationship.
//!
//! **Sources (context7, `/tokio-rs/tokio`, High reputation, bench 96):** a
//! `std::sync::MutexGuard` is `!Send` and "will block the thread" if held
//! across `.await` (tokio task blocking guidance); `tokio::sync::Mutex` is the
//! async-aware alternative; `parking_lot::Mutex` is the sync-fast alternative.
//! "There are only two hard things in computer science: cache invalidation,
//! and naming things — and off-by-one errors" — the converse for concurrency
//! is: lock ordering, lock-across-await, and shared-mutable-state scope.
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::loop_blocks::loop_bodies;

/// Density→score scale (shared with the other ADVISORY-tier engines).
const SCALE: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Go,
    Python,
    JsTs,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "go" => Lang::Go,
        "python" | "py" => Lang::Python,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => Lang::JsTs,
        _ => Lang::Other,
    }
}

/// Concurrency findings for one file.
#[derive(Debug, Clone, Default)]
pub struct ConcurrencyReport {
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl ConcurrencyReport {
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count > 0 {
            self.violations += count;
            self.weighted_total += count as f32 * weight;
            self.findings.push((message.to_string(), count));
        }
    }
}

/// Sync lock calls in `std::sync::*` (the kind that is `!Send` and deadlocks
/// the runtime if held across `.await`). Three needle families cover the
/// canonical Rust lock patterns: explicit `std::sync::Mutex::lock(` calls, the
/// short-form `.lock().unwrap()` (which is `Mutex::new(x).lock().unwrap()` or
/// `Arc<Mutex<T>>::lock()` in practice), and `RwLock::read/write()`.
const SYNC_LOCK_NEEDLES: [&[u8]; 4] = [
    b".lock().unwrap()",
    b".lock()",
    b".read().unwrap()",
    b".write().unwrap()",
];

/// `true` if the file has any `async fn` (Rust) / `async def` (Python) /
/// `async (` (JS/TS) outside comments/tests.
fn has_async(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> bool {
    let needle: &[u8] = match lang {
        Lang::Rust | Lang::JsTs => b"async ",
        Lang::Python => b"async def ",
        _ => return false,
    };
    memmem::find_iter(bytes, needle).any(|off| !offset_suppressed(off, regions))
}

/// Detector 1 (Rust) — `lock-across-await`: a `Mutex::lock(` / `RwLock::read(`
/// / `RwLock::write(` call in a function whose body contains `.await` *after*
/// the lock (the guard is still live). tokio: holding a `std::sync::MutexGuard`
/// across `.await` is `!Send` and panics / deadlocks the runtime. Implemented
/// by finding every `async fn` body (heuristic: from `async fn` keyword to the
/// next `}` at brace depth 0), and asking "is there a sync lock *and* an
/// `.await` in the body?". This is conservative — a body where the lock guard
/// is *not* alive at the await point (e.g. `_ = lock; await;` drops the guard
/// first) is also flagged. Per-body finding (file with N async fns + locks can
/// produce up to N findings).
fn rust_lock_across_await(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut count = 0;
    for off in memmem::find_iter(bytes, b"async fn") {
        if offset_suppressed(off, regions) {
            continue;
        }
        let body_start = match find_brace_at_depth_zero(bytes, off, b'{') {
            Some(b) => b,
            None => continue,
        };
        let body = match find_matching_close(bytes, body_start) {
            Some(c) => &bytes[body_start + 1..c],
            None => continue,
        };
        let has_lock = SYNC_LOCK_NEEDLES
            .iter()
            .any(|n| memmem::find(body, n).is_some());
        let has_await = memmem::find(body, b".await").is_some();
        if has_lock && has_await {
            count += 1;
        }
    }
    count
}

/// Detector 2 (Rust) — sync-locks-in-async at file level: the author used
/// `std::sync::Mutex` in a file that has at least one `async fn`. The intent
/// is almost always wrong (`tokio::sync::Mutex` for cross-await, or
/// `parking_lot::Mutex` for sync-within-async). File-level finding (idempotent).
fn rust_sync_locks_in_async(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if !has_async(bytes, regions, Lang::Rust) {
        return 0;
    }
    let has_sync_lock = SYNC_LOCK_NEEDLES
        .iter()
        .any(|n| memmem::find_iter(bytes, n).any(|off| !offset_suppressed(off, regions)));
    if has_sync_lock { 1 } else { 0 }
}

/// Detector 3 (Rust) — `Arc<Mutex<` / `Arc<RwLock<` shared state without a
/// nearby `tokio::sync::mpsc`/`oneshot` channel. The idion is: cross-task
/// state should flow over a channel (backpressure, ownership transfer) rather
/// than a shared `Arc<Mutex<Vec<…>>>`. File-level finding.
fn rust_arc_mutex_no_channel(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_arc_mutex = memmem::find_iter(bytes, b"Arc<Mutex<")
        .chain(memmem::find_iter(bytes, b"Arc<RwLock<"))
        .any(|off| !offset_suppressed(off, regions));
    if !has_arc_mutex {
        return 0;
    }
    let has_channel = memmem::find_iter(bytes, b"tokio::sync::mpsc")
        .chain(memmem::find_iter(bytes, b"tokio::sync::oneshot"))
        .any(|off| !offset_suppressed(off, regions));
    if has_channel { 0 } else { 1 }
}

/// Detector 4 (Rust) — `Mutex<u64>` / `Mutex<i64>` / `Mutex<usize>` for a
/// counter. Atomics (`AtomicU64`, `AtomicI64`, `AtomicUsize`) are lock-free and
/// strictly cheaper for the counter case (the Touring gate-metrics pattern).
fn rust_mutex_where_atomic(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let count = memmem::find_iter(bytes, b"Mutex<u64>")
        .chain(memmem::find_iter(bytes, b"Mutex<i64>"))
        .chain(memmem::find_iter(bytes, b"Mutex<usize>"))
        .filter(|&off| !offset_suppressed(off, regions))
        .count();
    if count > 0 { 1 } else { 0 }
}

/// Detector 5 (Go) — goroutine + `sync.Mutex`: a `go func()` spawn *and* a
/// `sync.Mutex` in the same file. The combination is the canonical
/// data-race-on-mutex pattern: a goroutine acquires the mutex while the
/// spawning function still owns it.
fn go_goroutine_mutex(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_go = memmem::find_iter(bytes, b"go func(").any(|off| !offset_suppressed(off, regions));
    if !has_go {
        return 0;
    }
    let has_mutex =
        memmem::find_iter(bytes, b"sync.Mutex").any(|off| !offset_suppressed(off, regions));
    if has_mutex { 1 } else { 0 }
}

/// Detector 6 (Python) — `async def` + `threading.Lock`/`threading.RLock`:
/// async code using a sync `threading.Lock`. The right primitive is
/// `asyncio.Lock`. File-level finding.
fn py_async_with_sync_lock(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if !has_async(bytes, regions, Lang::Python) {
        return 0;
    }
    let has_sync_lock = memmem::find_iter(bytes, b"threading.Lock(")
        .chain(memmem::find_iter(bytes, b"threading.RLock("))
        .any(|off| !offset_suppressed(off, regions));
    if has_sync_lock { 1 } else { 0 }
}

// ── Tiny brace-matched scope finder (paren-aware, string-literal-aware) ─────
/// `Some(open)` if `{` at brace depth 0 is found at or after `start` in
/// `bytes`, tracking paren/bracket depth so a `for { … }` or `loop { … }`
/// header in the iterator expr doesn't get misscoped. Returns the byte index
/// of the `{` (NOT the first byte after — caller decides).
fn find_brace_at_depth_zero(bytes: &[u8], start: usize, brace: u8) -> Option<usize> {
    let mut paren: i32 = 0;
    let mut bracket: i32 = 0;
    let mut j = start;
    while j < bytes.len() {
        let after = skip_string_or_char(bytes, j);
        if after > j {
            j = after;
            continue;
        }
        match bytes[j] {
            b'(' => paren += 1,
            b')' => paren -= 1,
            b'[' => bracket += 1,
            b']' => bracket -= 1,
            c if c == brace && paren == 0 && bracket == 0 => return Some(j),
            _ => {}
        }
        j += 1;
    }
    None
}

/// Find the matching close `}` for the `{` at `open_idx`. Paren/bracket/literal
/// aware. Returns the byte index of the `}`.
fn find_matching_close(bytes: &[u8], open_idx: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut j = open_idx;
    while j < bytes.len() {
        let after = skip_string_or_char(bytes, j);
        if after > j {
            j = after;
            continue;
        }
        match bytes[j] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// `> j` after a `"/`/`b'/'` literal; `j` unchanged otherwise.
fn skip_string_or_char(bytes: &[u8], i: usize) -> usize {
    match bytes.get(i) {
        Some(b'"') | Some(b'`') => {
            let quote = bytes[i];
            let mut j = i + 1;
            while j < bytes.len() {
                match bytes[j] {
                    b'\\' => j += 2,
                    c if c == quote => return j + 1,
                    _ => j += 1,
                }
            }
            j
        }
        Some(b'\'')
            if matches!(bytes.get(i + 1), Some(b'{') | Some(b'}'))
                && bytes.get(i + 2) == Some(&b'\'') =>
        {
            i + 3
        }
        _ => i,
    }
}

#[allow(dead_code)]
fn _avoid_unused_loop_bodies_warning(
    bytes: &[u8],
    regions: &[(usize, usize)],
    lang: &str,
) -> Vec<(usize, usize)> {
    loop_bodies(bytes, regions, lang)
}

/// Analyze concurrency smells in `source` for the given language.
pub fn analyze_concurrency(source: &str, lang: &str) -> ConcurrencyReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let lang = canonical_lang(lang);
    let mut report = ConcurrencyReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match lang {
        Lang::Rust => {
            report.push(
                "lock-across-await (std::sync::Mutex/RwLock guard held across .await — deadlock)",
                rust_lock_across_await(bytes, &regions),
                1.0,
            );
            report
                .push(
                    "sync lock (std::sync::Mutex/RwLock) in async fn (use tokio::sync::Mutex or parking_lot::Mutex)",
                    rust_sync_locks_in_async(bytes, &regions),
                    0.9,
                );
            report
                .push(
                    "Arc<Mutex<…>> shared state without a tokio::sync::mpsc channel (channel is contention-free + backpressure)",
                    rust_arc_mutex_no_channel(bytes, &regions),
                    0.7,
                );
            report.push(
                "Mutex<u64/i64/usize> for a counter (use AtomicU64/I64/Usize — lock-free)",
                rust_mutex_where_atomic(bytes, &regions),
                0.7,
            );
        }
        Lang::Go => {
            report
                .push(
                    "go func() + sync.Mutex (data-race-on-mutex risk; pass ownership via channel instead)",
                    go_goroutine_mutex(bytes, &regions),
                    1.0,
                );
        }
        Lang::Python => {
            report
                .push(
                    "async def + threading.Lock (use asyncio.Lock — threading.Lock blocks the event loop)",
                    py_async_with_sync_lock(bytes, &regions),
                    1.0,
                );
        }
        _ => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`ConcurrencyReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
/// Delegates to [`super::score_utils::density_score`] for the `max(20)` floor
/// so short files don't saturate (F2.13 lesson).
pub fn score_concurrency(report: &ConcurrencyReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lock_across_await_flagged() {
        let src = "async fn go() {\n    let _g = std::sync::Mutex::new(0).lock().unwrap();\n    do_work().await;\n}\n";
        let r = analyze_concurrency(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("lock-across-await")),
            "guard live at .await is flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn lock_dropped_before_await_still_flagged_conservatively() {
        let src = "async fn go() {\n    let _g = std::sync::Mutex::new(0).lock().unwrap();\n    drop(_g);\n    do_work().await;\n}\n";
        let r = analyze_concurrency(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("lock-across-await")),
            "lock+await in same body is conservatively flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn sync_lock_in_async_function_flagged() {
        let src = "async fn go() {\n    let _g = std::sync::Mutex::new(0).lock().unwrap();\n}\n";
        let r = analyze_concurrency(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("sync lock (std::sync::Mutex/RwLock) in async fn")),
            "async fn + std::sync::Mutex is flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn sync_lock_in_sync_function_clean() {
        let src = "fn go() {\n    let _g = std::sync::Mutex::new(0).lock().unwrap();\n}\n";
        let r = analyze_concurrency(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("sync lock (std::sync::Mutex/RwLock) in async fn")),
            "sync fn + std::sync::Mutex is idiomatic: {:?}",
            r.findings
        );
    }
    #[test]
    fn arc_mutex_no_channel_flagged() {
        let src = "struct S {\n    state: Arc<Mutex<Vec<u8>>>,\n}\n";
        let r = analyze_concurrency(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("Arc<Mutex<…>> shared state")),
            "Arc<Mutex<Vec<…>>> without a channel is flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn arc_mutex_with_channel_clean() {
        let src = "struct S {\n    state: Arc<Mutex<Vec<u8>>>,\n    cmd: tokio::sync::mpsc::Sender<u8>,\n}\n";
        let r = analyze_concurrency(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("Arc<Mutex<…>> shared state")),
            "Arc<Mutex<…>> + tokio::sync::mpsc is the hybrid pattern: {:?}",
            r.findings
        );
    }
    #[test]
    fn mutex_u64_flagged() {
        let src = "let c: Mutex<u64> = Mutex::new(0);\n";
        let r = analyze_concurrency(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("Mutex<u64/i64/usize> for a counter")),
            "Mutex<u64> should be AtomicU64: {:?}",
            r.findings
        );
    }
    #[test]
    fn mutex_string_not_flagged() {
        let src = "let c: Mutex<String> = Mutex::new(String::new());\n";
        let r = analyze_concurrency(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("Mutex<u64/i64/usize> for a counter")),
            "Mutex<String> is not a counter: {:?}",
            r.findings
        );
    }
    #[test]
    fn go_goroutine_mutex_flagged() {
        let src = "package main\nimport \"sync\"\nfunc go() {\n    var m sync.Mutex\n    go func() { m.Lock(); m.Unlock() }()\n}\n";
        let r = analyze_concurrency(src, "go");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("go func() + sync.Mutex")),
            "Go: goroutine + Mutex is the race: {:?}",
            r.findings
        );
    }
    #[test]
    fn py_async_threading_lock_flagged() {
        let src = "import asyncio, threading\nasync def go():\n    lock = threading.Lock()\n    async with lock:\n        pass\n";
        let r = analyze_concurrency(src, "python");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("async def + threading.Lock")),
            "async def + threading.Lock blocks the event loop: {:?}",
            r.findings
        );
    }
    #[test]
    fn js_pure_async_clean() {
        let src =
            "async function load() {\n    const r = await fetch('/a');\n    return r.json();\n}\n";
        let r = analyze_concurrency(src, "typescript");
        assert_eq!(r.violations, 0, "pure async is clean: {:?}", r.findings);
    }
    #[test]
    fn comment_excluded() {
        let src = "// async fn go() { let _g = std::sync::Mutex::new(0).lock().unwrap(); do_work().await; }\nfn real() {}\n";
        let r = analyze_concurrency(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("lock-across-await")),
            "commented lock-across-await is excluded: {:?}",
            r.findings
        );
    }
    #[test]
    fn string_brace_in_body_not_misscoped() {
        let src = "async fn go() {\n    let s = \"open brace {\";\n    do_work().await;\n}\nfn other() {}\n";
        let r = analyze_concurrency(src, "rust");
        assert_eq!(
            r.violations, 0,
            "no lock → no finding (and no scope misscope): {:?}",
            r.findings
        );
    }
    #[test]
    fn score_monotonic_dirty_below_clean() {
        let bad = analyze_concurrency(
            "async fn go() {\n    let _g = std::sync::Mutex::new(0).lock().unwrap();\n    do_work().await;\n    let c: Mutex<u64> = Mutex::new(0);\n    let s: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(vec![]));\n}\n",
            "rust",
        );
        let good = analyze_concurrency(
            "async fn go() {\n    let s: tokio::sync::Mutex<Vec<u8>> = tokio::sync::Mutex::new(vec![]);\n    do_work().await;\n}\n",
            "rust",
        );
        assert!(
            score_concurrency(&bad) < score_concurrency(&good),
            "dirty ({:.3}) must score below clean ({:.3})",
            score_concurrency(&bad),
            score_concurrency(&good)
        );
    }
    /// Regression test for the F2.13 saturation fix (`max(20)` floor in
    /// [`super::score_utils::density_score`]). A short file with 3
    /// concurrency smells must NOT score 0.0.
    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_concurrency(
            "async fn go() {\n    let _g = std::sync::Mutex::new(0).lock().unwrap();\n    do_work().await;\n    let c: Mutex<u64> = Mutex::new(0);\n}\n",
            "rust",
        );
        let s = score_concurrency(&r);
        assert!(
            s > 0.0,
            "short file with concurrency smells must not score 0.0: {s}"
        );
    }
}
