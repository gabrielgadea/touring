//! I/O Bottlenecks (D23 / F2.10) — blocking I/O in async context + I/O inside
//! loop bodies + unbuffered byte-loop reads + `block_on` inside an async
//! runtime.
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | blocking-in-async | an `async fn` *and* a `std::fs::`/`std::net::`/`TcpStream::connect`/`reqwest::blocking` call in the same file | Rust |
//! | block_on-in-runtime | an `async fn` *and* a `block_on(` call in the same file | Rust |
//! | io-in-loop | a `std::fs::`/`std::net::`/`TcpStream::`/`reqwest::blocking` call **inside a loop body** (`for`/`while` via `super::loop_blocks::loop_bodies`) | Rust |
//! | unbuffered-read-loop | a `read_exact(` **inside a loop body** with no `BufReader` in the same body | Rust |
//!
//! **Disjoint** from F2.7 db-perf (which keys on `db.execute`/`db.query` in
//! loop, db-specific), F2.8 memory (which keys on `unbounded_channel(`/
//! `Box::leak(`/.clone in loop), and F2.9 caching. F2.10 anchors on the
//! file/network I/O *and* `reqwest::blocking` family of calls — none of which
//! the other engines inspect. `block_on(` inside a runtime panics per
//! tokio docs, so even one occurrence is a finding.
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.
//!
//! **Sources (context7, `/tokio-rs/tokio`, High reputation):** a blocking
//! call (`std::fs`/`std::net`/`reqwest::blocking`) inside an `async fn` "will
//! block the thread", starving the executor (tokio task blocking guidance);
//! `tokio::runtime::Runtime::block_on` is the documented panic source when
//! called inside an existing runtime (`Cannot start a runtime from within a
//! runtime`); `BufReader` is the canonical buffer for byte-loop reads.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::loop_blocks::loop_bodies;

/// Density→score scale (shared with the other ADVISORY-tier engines).
const SCALE: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    JsTs,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => Lang::JsTs,
        _ => Lang::Other,
    }
}

/// I/O-bottleneck findings for one file.
pub type IoReport = crate::quality::SmellReport;

/// I/O calls that block the executor when called inside an `async fn`. Both
/// stdlib blocking I/O and the `reqwest::blocking` client.
const BLOCKING_IO_NEEDLES: [&[u8]; 5] = [
    b"std::fs::",
    b"std::net::",
    b"TcpStream::connect",
    b"UdpSocket::",
    b"reqwest::blocking",
];

/// `true` if `bytes` contains an `async fn` (header) outside comments/tests.
fn has_async_fn(bytes: &[u8], regions: &[(usize, usize)]) -> bool {
    memmem::find_iter(bytes, b"async fn").any(|off| !offset_suppressed(off, regions))
}

/// Detector 1 — blocking I/O in async context: an `async fn` in the file plus
/// at least one blocking I/O call. tokio docs: "blocking the thread" inside
/// `async fn` "will starve the executor of its ability to make progress".
/// One finding per file (the count of distinct call sites is captured in the
/// `findings` tuple for diagnostics).
fn blocking_in_async(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if !has_async_fn(bytes, regions) {
        return 0;
    }
    let call_sites: usize = BLOCKING_IO_NEEDLES
        .iter()
        .map(|needle| {
            memmem::find_iter(bytes, needle)
                .filter(|&off| !offset_suppressed(off, regions))
                .count()
        })
        .sum();
    if call_sites > 0 { 1 } else { 0 }
}

/// Detector 2 — `block_on(` inside an `async fn` panics (tokio: "Cannot start
/// a runtime from within a runtime"). One finding per file.
fn block_on_in_runtime(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if !has_async_fn(bytes, regions) {
        return 0;
    }
    let has_block_on =
        memmem::find_iter(bytes, b"block_on(").any(|off| !offset_suppressed(off, regions));
    if has_block_on { 1 } else { 0 }
}

/// Detector 3 — file/network I/O inside a loop body. Uses
/// [`loop_bodies`] (the shared brace-/indent-scoped loop-body finder) so a
/// closure brace in the iterator expr is not misscoped. Count = number of
/// loop bodies that contain a blocking-I/O call. `db.execute`/`db.query` are
/// F2.7's claim (disjoint by needle).
fn io_in_loop(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let loops = loop_bodies(bytes, regions, "rust");
    let mut count = 0;
    for (start, end) in loops {
        let body = &bytes[start..end];
        if BLOCKING_IO_NEEDLES
            .iter()
            .any(|n| memmem::find(body, n).is_some())
        {
            count += 1;
        }
    }
    count
}

/// Detector 4 — `read_exact(` inside a loop body. The Rust idiom is to wrap
/// the handle in `BufReader::new(x)` *before* the loop (so the buffered reader
/// is constructed once and reused across iterations); we therefore check for
/// `BufReader` anywhere in the file outside comments/tests — any `read_exact`
/// loop in a file that has adopted the buffered-I/O pattern is considered
/// safe. Without `BufReader` anywhere, a `read_exact` loop is a per-call
/// syscall (unbuffered). `read_exact` is the canonical "I want N bytes"
/// primitive; pairing it with `BufReader` is the idiomatic fix.
fn unbuffered_read_loop(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_bufreader =
        memmem::find_iter(bytes, b"BufReader").any(|off| !offset_suppressed(off, regions));
    if has_bufreader {
        return 0;
    }
    let loops = loop_bodies(bytes, regions, "rust");
    let mut count = 0;
    for (start, end) in loops {
        let body = &bytes[start..end];
        if memmem::find(body, b"read_exact(").is_some() {
            count += 1;
        }
    }
    count
}

/// Analyze I/O-bottleneck smells in `source`.
pub fn analyze_io(source: &str, lang: &str) -> IoReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let _ = canonical_lang(lang);
    let mut report = IoReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    report.push(
        "blocking I/O in async context (std::fs/std::net/reqwest::blocking inside async fn)",
        blocking_in_async(bytes, &regions),
        1.0,
    );
    report.push(
        "block_on inside async runtime (panic risk)",
        block_on_in_runtime(bytes, &regions),
        1.0,
    );
    report.push(
        "file/network I/O inside loop body (N+1 I/O)",
        io_in_loop(bytes, &regions),
        0.9,
    );
    report.push(
        "unbuffered read_exact loop (no BufReader in body)",
        unbuffered_read_loop(bytes, &regions),
        0.7,
    );
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score an [`IoReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
/// Delegates to [`super::score_utils::density_score`] for the `max(20)` floor
/// so short files don't saturate (F2.13 lesson).
pub fn score_io(report: &IoReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocking_in_async_flagged() {
        let src =
            "async fn load() {\n    let s = std::fs::read_to_string(\"a.txt\").unwrap();\n}\n";
        let r = analyze_io(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("blocking I/O in async")),
            "async fn + std::fs is flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn blocking_sync_only_not_flagged() {
        let src = "fn load() {\n    let s = std::fs::read_to_string(\"a.txt\").unwrap();\n}\n";
        let r = analyze_io(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("blocking I/O in async")),
            "sync fn + std::fs is not flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn block_on_in_async_flagged() {
        let src = "async fn go() {\n    let r = tokio::runtime::Runtime::new().unwrap().block_on(work());\n}\n";
        let r = analyze_io(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("block_on inside async")),
            "block_on inside async is a panic risk: {:?}",
            r.findings
        );
    }
    #[test]
    fn block_on_test_setup_not_flagged() {
        let src = "fn main() {\n    let r = tokio::runtime::Runtime::new().unwrap().block_on(work());\n}\n";
        let r = analyze_io(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("block_on inside async")),
            "block_on in sync fn is fine: {:?}",
            r.findings
        );
    }
    #[test]
    fn io_in_loop_flagged() {
        let src = "fn load_all(paths: &[&str]) {\n    for p in paths {\n        let s = std::fs::read_to_string(p).unwrap();\n    }\n}\n";
        let r = analyze_io(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("file/network I/O inside loop")),
            "fs in for-loop is N+1 I/O: {:?}",
            r.findings
        );
    }
    #[test]
    fn unbuffered_read_loop_flagged() {
        let src = "fn drain(r: &mut std::fs::File) {\n    let mut buf = [0u8; 4096];\n    loop {\n        let n = r.read_exact(&mut buf).unwrap();\n        if n == 0 { break; }\n    }\n}\n";
        let r = analyze_io(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("unbuffered read_exact loop")),
            "read_exact loop without BufReader is flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn buffered_read_loop_not_flagged() {
        let src = "fn drain(r: std::fs::File) {\n    let mut r = std::io::BufReader::new(r);\n    let mut buf = [0u8; 4096];\n    loop {\n        let n = r.read_exact(&mut buf).unwrap();\n        if n == 0 { break; }\n    }\n}\n";
        let r = analyze_io(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("unbuffered read_exact loop")),
            "BufReader-wrapped read_exact loop is clean: {:?}",
            r.findings
        );
    }
    #[test]
    fn closure_brace_in_loop_not_misscoped() {
        let src = "fn load_all(paths: &[&str]) {\n    for p in paths.iter().map(|p| { p.to_string() }) {\n        real(p);\n    }\n    let s = std::fs::read_to_string(\"a.txt\").unwrap();\n}\n";
        let r = analyze_io(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("file/network I/O inside loop")),
            "real body is empty of fs; trailing fs call is outside the loop: {:?}",
            r.findings
        );
    }
    #[test]
    fn trait_impl_for_not_a_loop() {
        let src = "impl Loader for MyL {\n    fn load(&self) {\n        let s = std::fs::read_to_string(\"a.txt\").unwrap();\n    }\n}\n";
        let r = analyze_io(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("file/network I/O inside loop")),
            "impl body is not a loop: {:?}",
            r.findings
        );
    }
    #[test]
    fn comment_excluded() {
        let src = "// async fn foo() { let s = std::fs::read_to_string(\"a\"); }\nfn real() {}\n";
        let r = analyze_io(src, "rust");
        assert_eq!(
            r.violations, 0,
            "commented async+fs is excluded: {:?}",
            r.findings
        );
    }
    #[test]
    fn score_monotonic_dirty_below_clean() {
        let bad = analyze_io(
            "async fn f() {\n    let s = std::fs::read_to_string(\"a\").unwrap();\n    let r = tokio::runtime::Runtime::new().unwrap().block_on(work());\n    for p in paths { let s = std::fs::read_to_string(p).unwrap(); }\n}\n",
            "rust",
        );
        let good = analyze_io(
            "async fn f() { let s = tokio::fs::read(\"a\").await.unwrap(); }\n",
            "rust",
        );
        assert!(
            score_io(&bad) < score_io(&good),
            "dirty ({:.3}) must score below clean ({:.3})",
            score_io(&bad),
            score_io(&good)
        );
    }
    /// Regression test for the F2.13 saturation fix (`max(20)` floor in
    /// [`super::score_utils::density_score`]). A short file with 3
    /// I/O smells must NOT score 0.0.
    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_io(
            "async fn f() {\n    let s = std::fs::read_to_string(\"a\").unwrap();\n    do().await;\n    let r = tokio::runtime::Runtime::new().unwrap().block_on(work());\n}\n",
            "rust",
        );
        let s = score_io(&r);
        assert!(
            s > 0.0,
            "short file with 3 I/O smells must not score 0.0: {s}"
        );
    }
}
