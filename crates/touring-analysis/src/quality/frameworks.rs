//! Framework Patterns (D41 / F4.2) -- language-aware detector of the canonical
//! "fighting the framework" smells. The framework encodes decisions; fighting
//! them reintroduces exactly the problems the framework solved.
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | `block-on-in-runtime` | an `async fn` *and* a `block_on(` / `Handle::current().block_on(` call in the same file (panic per tokio docs) | Rust |
//! | `nested-tokio-main` | two `#[tokio::main]` in same file (or a manual `Runtime::new(`/`Builder::new(` alongside `#[tokio::main]`) | Rust |
//! | `reqwest-blocking-in-async` | an `async fn` *and* a `reqwest::blocking` call in the same file (blocks executor) | Rust |
//! | `sync-mutex-in-async` | an `async fn` *and* a `std::sync::Mutex::lock(` call (use `tokio::sync::Mutex` or `parking_lot`) | Rust |
//! | `runtime-build-without-main` | a manual `Runtime::new(` / `Builder::new(` *without* `#[tokio::main]` macro (likely dual-runtime; one process per `main`) | Rust |
//! | `not-tokio-test` | an `async fn test_…(` body but the surrounding `#[test]` is plain (not `#[tokio::test]`) | Rust |
//! | `axum-parsing-handler` | an axum `async fn handler(` body that calls `serde_json::from_str(`/`from_slice(` (use the `Json<T>` extractor) | Rust |
//! | `py-asyncio-no-await` | an `async def` in a Python file with **no** `await` / `async for` / `async with` anywhere -- broken coroutine | Python |
//! | `py-threading-in-async` | a Python `async def` in a file that also uses `threading.Lock(` / `threading.RLock(` (blocks the event loop) | Python |
//!
//! **Disjoint** from F2.10 I/O (which keys on `std::fs::` / `reqwest::blocking` in
//! `async fn` -- same signal class but F2.10 is file-level I/O, F4.2 is
//! framework-API misuse) and F2.11 concurrency (which keys on `std::sync::Mutex`
//! guard **held across** `.await` -- F4.2 keys on the *use* of a sync mutex inside
//! an `async fn` regardless of guard lifetime).
//!
//! **Sources (context7, `/tokio-rs/tokio`, High reputation, bench 85.72):**
//! `Handle::current().block_on(...)` inside an existing runtime panics with
//! "Cannot start a runtime from within a runtime" (tokio runtime tests). The
//! correct pattern is `tokio::task::block_in_place` first. `Builder::new_current_thread`
//! is the smallest runtime configuration for a desktop GUI app needing
//! `spawn_blocking` support. `worker_threads(N)` defaults to `num_cpus`.
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

const SCALE: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Python,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "python" | "py" => Lang::Python,
        _ => Lang::Other,
    }
}

#[derive(Debug, Clone, Default)]
/// Framework-pattern findings for one file.
pub struct FrameworksReport {
    /// Total raw violation count across all detectors.
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl FrameworksReport {
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count > 0 {
            self.violations += count;
            self.weighted_total += count as f32 * weight;
            self.findings.push((message.to_string(), count));
        }
    }
}

const ASYNC_FN: &[u8] = b"async fn";
const BLOCK_ON: &[u8] = b"block_on(";
const TOKIO_MAIN_ATTR: &[u8] = b"#[tokio::main]";
const REQWEST_BLOCKING: &[u8] = b"reqwest::blocking";
const SERDE_FROM_STR: &[u8] = b"serde_json::from_str(";
const SERDE_FROM_SLICE: &[u8] = b"serde_json::from_slice(";
const PY_ASYNC_DEF: &[u8] = b"async def ";
const PY_AWAIT: &[u8] = b"await ";
const PY_THREADING_LOCK: &[u8] = b"threading.Lock(";
const PY_THREADING_RLOCK: &[u8] = b"threading.RLock(";
const PY_ASYNC_FOR: &[u8] = b"async for ";
const PY_ASYNC_WITH: &[u8] = b"async with ";

fn has_async_fn(bytes: &[u8], regions: &[(usize, usize)]) -> bool {
    memmem::find_iter(bytes, ASYNC_FN).any(|off| !offset_suppressed(off, regions))
}

fn count_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

/// `async fn` *and* `block_on(` in the same file. Tokio panics with
/// "Cannot start a runtime from within a runtime" -- this is a hard fail.
fn detect_block_on_in_runtime(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if !has_async_fn(bytes, regions) {
        return 0;
    }
    if count_in_executable(bytes, regions, BLOCK_ON) == 0 {
        return 0;
    }
    1
}

/// More than one `#[tokio::main]` in the same file (nested runtimes), or a
/// manual `Runtime::new(` / `Builder::new(` alongside `#[tokio::main]`.
fn detect_nested_tokio_main(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let main_count = count_in_executable(bytes, regions, TOKIO_MAIN_ATTR);
    if main_count > 1 {
        return 1;
    }
    if main_count == 1 {
        // Plain memmem: `Builder::new_multi_thread()`, `Builder::new_current_thread()`,
        // `Builder::new_hybrid()` -- all have the same `Builder::new` prefix. Catching
        // the prefix (no `(`) avoids false-negative on real-world code.
        let has_manual = memmem::find(bytes, b"Builder::new").is_some()
            || memmem::find(bytes, b"Runtime::new").is_some();
        if has_manual {
            return 1;
        }
    }
    0
}

/// `async fn` *and* `reqwest::blocking` in the same file. The blocking
/// client is sync I/O -- it freezes the executor.
fn detect_reqwest_blocking_in_async(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if !has_async_fn(bytes, regions) {
        return 0;
    }
    if count_in_executable(bytes, regions, REQWEST_BLOCKING) == 0 {
        return 0;
    }
    1
}

/// `async fn` *and* `std::sync::Mutex` (any usage -- import, `.lock()`,
/// `.new()`) in the same file. Sync mutex inside an `async fn` is a
/// deadlock / `!Send` risk. The simple `std::sync::Mutex` needle catches
/// all variants (import, type alias, direct construction) without
/// false-negative on real-world code that uses `MUTEX.lock()`.
fn detect_sync_mutex_in_async(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if !has_async_fn(bytes, regions) {
        return 0;
    }
    // Plain memmem: `use std::sync::Mutex;` at the top of the file IS in
    // executable code, and a sync Mutex in scope at the async fn is the
    // smell. Region filter would drop the `use` if it sits above a `mod
    // tests { ... }` block.
    if memmem::find(bytes, b"std::sync::Mutex").is_some() {
        return 1;
    }
    0
}

/// `Runtime::new(` / `Builder::new(` *without* a `#[tokio::main]` attribute --
/// suggests a manual runtime construction (likely dual-runtime in the
/// same process: `#[tokio::main]` *and* `Runtime::new(` somewhere else).
/// File-level finding (1 if both signals present).
fn detect_runtime_build_without_main(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_main = count_in_executable(bytes, regions, TOKIO_MAIN_ATTR) > 0;
    if has_main {
        return 0;
    }
    let has_manual = memmem::find(bytes, b"Builder::new").is_some()
        || memmem::find(bytes, b"Runtime::new").is_some();
    if has_manual { 1 } else { 0 }
}

/// An `#[test]` (plain, not `#[tokio::test]`) for a function whose body uses
/// `async fn` -- the test will not actually run async code. Heuristic: a
/// function declaration line containing `async fn test_` followed by a
/// `#[test]` attribute (or vice versa).
fn detect_not_tokio_test(bytes: &[u8], _regions: &[(usize, usize)]) -> usize {
    // Plain memmem: `#[test]` / `#[tokio::test]` attribute lines can be
    // covered by code_regions (the *body* is excluded), so we read raw.
    let has_plain = memmem::find(bytes, b"#[test]").is_some();
    if !has_plain {
        return 0;
    }
    let has_tokio = memmem::find(bytes, b"#[tokio::test]").is_some();
    if has_tokio {
        return 0;
    }
    let has_async_test = memmem::find(bytes, b"async fn test_").is_some();
    if has_async_test { 1 } else { 0 }
}

/// An axum/actix handler that does manual `serde_json::from_*` (should use
/// the `Json<T>` extractor). Heuristic: presence of `from_str(` /
/// `from_slice(` in a file that defines an `async fn handler` (handler
/// function). File-level finding.
fn detect_manual_handler_parsing(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_handler =
        memmem::find_iter(bytes, b"async fn handler").any(|off| !offset_suppressed(off, regions));
    if !has_handler {
        return 0;
    }
    let has_manual = count_in_executable(bytes, regions, SERDE_FROM_STR) > 0
        || count_in_executable(bytes, regions, SERDE_FROM_SLICE) > 0;
    if has_manual { 1 } else { 0 }
}

/// `async def` in a Python file but **no** `await` / `async for` /
/// `async with` anywhere. A bare `async def` that never awaits is a
/// broken coroutine (returns a coroutine object that's never awaited).
fn detect_py_asyncio_no_await(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_async_def = count_in_executable(bytes, regions, PY_ASYNC_DEF) > 0;
    if !has_async_def {
        return 0;
    }
    let has_await = count_in_executable(bytes, regions, PY_AWAIT) > 0
        || count_in_executable(bytes, regions, PY_ASYNC_FOR) > 0
        || count_in_executable(bytes, regions, PY_ASYNC_WITH) > 0;
    if has_await { 0 } else { 1 }
}

/// `async def` in a Python file that also uses `threading.Lock` /
/// `threading.RLock` (sync primitives that block the event loop in async
/// context).
fn detect_py_threading_in_async(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_async_def = count_in_executable(bytes, regions, PY_ASYNC_DEF) > 0;
    if !has_async_def {
        return 0;
    }
    let has_threading = count_in_executable(bytes, regions, PY_THREADING_LOCK) > 0
        || count_in_executable(bytes, regions, PY_THREADING_RLOCK) > 0;
    if has_threading { 1 } else { 0 }
}

/// Analyze framework-pattern smells in `source` for the given language (Rust or Python).
pub fn analyze_frameworks(source: &str, lang: &str) -> FrameworksReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let mut report = FrameworksReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    let l = canonical_lang(lang);
    match l {
        Lang::Rust => {
            report.push(
                "block_on( inside async fn (panics: Cannot start a runtime from within a runtime)",
                detect_block_on_in_runtime(bytes, &regions),
                1.0,
            );
            report.push(
                "nested #[tokio::main] or manual Runtime alongside #[tokio::main]",
                detect_nested_tokio_main(bytes, &regions),
                1.0,
            );
            report.push(
                "reqwest::blocking inside async fn (blocks executor -- use reqwest non-blocking)",
                detect_reqwest_blocking_in_async(bytes, &regions),
                0.9,
            );
            report.push(
                "std::sync::Mutex inside async fn (use tokio::sync::Mutex or parking_lot)",
                detect_sync_mutex_in_async(bytes, &regions),
                0.7,
            );
            report.push(
                "Runtime::new( / Builder::new( without #[tokio::main] (manual runtime build)",
                detect_runtime_build_without_main(bytes, &regions),
                0.5,
            );
            report.push(
                "async fn test_ with plain #[test] (use #[tokio::test] so the body actually runs)",
                detect_not_tokio_test(bytes, &regions),
                0.7,
            );
            report.push(
                "axum handler with manual serde_json::from_* (use the Json<T> extractor)",
                detect_manual_handler_parsing(bytes, &regions),
                0.6,
            );
        }
        Lang::Python => {
            report.push(
                "async def with no await / async for / async with (broken coroutine)",
                detect_py_asyncio_no_await(bytes, &regions),
                0.9,
            );
            report.push(
                "threading.Lock / RLock in a file with async def (blocks event loop)",
                detect_py_threading_in_async(bytes, &regions),
                0.8,
            );
        }
        Lang::Other => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`FrameworksReport`] as `1 - density·SCALE`, clamped to `[0, 1]`.
pub fn score_frameworks(report: &FrameworksReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(src: &str, lang: &str) -> FrameworksReport {
        analyze_frameworks(src, lang)
    }

    #[test]
    fn empty_file_clean() {
        let r = rep("", "rust");
        assert_eq!(r.violations, 0);
        assert!(
            score_frameworks(&r) > 0.95,
            "empty file scores high: {:.3}",
            score_frameworks(&r)
        );
    }

    #[test]
    fn block_on_in_async_flagged() {
        let src = r#"async fn bad() {
    tokio::runtime::Handle::current().block_on(async { 1 });
}
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("block_on") && m.contains("runtime")),
            "block_on in async flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn nested_tokio_main_flagged() {
        let src = r#"#[tokio::main]
async fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread().build().unwrap();
}
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("nested") || m.contains("alongside")),
            "manual runtime alongside #[tokio::main] flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn reqwest_blocking_in_async_flagged() {
        let src = r#"async fn fetch() {
    let body = reqwest::blocking::get("https://example.com").unwrap();
}
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("reqwest::blocking")),
            "reqwest::blocking in async flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn sync_mutex_in_async_flagged() {
        let src = r#"use std::sync::Mutex;
async fn bad() {
    let g = MUTEX.lock().unwrap();
}
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("std::sync::Mutex")),
            "sync mutex in async flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn not_tokio_test_flagged() {
        let src = r#"#[test]
async fn test_async_thing() {
    assert_eq!(1, 1);
}
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("tokio::test")),
            "plain #[test] with async fn body flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn tokio_test_clean() {
        let src = r#"#[tokio::test]
async fn test_async_thing() {
    assert_eq!(1, 1);
}
"#;
        let r = rep(src, "rust");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("tokio::test]")),
            "#[tokio::test] is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn py_async_no_await_flagged() {
        let src = r#"async def fetch():
    return 1
"#;
        let r = rep(src, "python");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("no await")),
            "async def with no await flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn py_async_with_await_clean() {
        let src = r#"async def fetch():
    x = await some_call()
    return x
"#;
        let r = rep(src, "python");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("no await")),
            "async def with await is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn py_threading_in_async_flagged() {
        let src = r#"import threading
import asyncio

lock = threading.Lock()

async def bad():
    with lock:
        await asyncio.sleep(0)
"#;
        let r = rep(src, "python");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("threading")),
            "threading.Lock in async file flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = rep(
            r#"async fn bad() {
    tokio::runtime::Handle::current().block_on(async {});
    let _ = reqwest::blocking::get("http://x");
    let _ = std::sync::Mutex::new(0).lock();
}
"#,
            "rust",
        );
        let good = rep("pub fn add(a: i32, b: i32) -> i32 { a + b }", "rust");
        assert!(
            score_frameworks(&bad) < score_frameworks(&good),
            "framework-violating file ({:.3}) must score below clean ({:.3})",
            score_frameworks(&bad),
            score_frameworks(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = rep(
            "async fn bad() { tokio::runtime::Handle::current().block_on(async {}); }",
            "rust",
        );
        let s = score_frameworks(&r);
        assert!(
            s > 0.0,
            "short framework-violating file must not score 0.0: {s}"
        );
    }
}
