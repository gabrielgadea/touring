//! Architectural Consistency analysis (D12 / F1.12) — polyglot detector of the
//! canonical "mixed patterns / layer-violation smell" within a file. While
//! F1.8 detects cycles and F1.7 enforces `pub(crate)` boundaries, F1.12
//! surfaces **internal inconsistency** within a file — a file that mixes
//! error-handling styles (panic + Result + unwrap), logging styles
//! (println + tracing), or config sources (env + config::load) is a
//! consistency smell: the workspace has no convention, drift accumulates.
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | `mixed-error-styles` | `panic!` + `Result<T, _>` + `.unwrap()` all in the same file (≥2 distinct styles) | Rust |
//! | `mixed-logging-styles` | `println!`/`eprintln!` + `tracing::*!` or `log::*!` in the same file | Rust/JS/TS/Python |
//! | `mixed-config-sources` | `std::env::var(` + `config::` or `figment::` / `dotenv::` in the same file | Rust/Python |
//! | `hardcoded-role-string` | `if .*\.role == "admin"` / `== "user"` (string-literal role check — should be enum) | all |
//! | `mixed-async-styles` | `tokio::` + `async-std` + `smol` in the same file (impossible — flag if found) | Rust |
//!
//! **Disjoint** from F1.7 boundaries (F1.7 keys on `pub` vs `pub(crate)`
//! surface area; F1.12 keys on *internal* style consistency); F1.8 dep cycles
//! (F1.8 keys on workspace-level SCCs; F1.12 keys on intra-file pattern mix);
//! F1.11 patterns (F1.11 keys on GoF / ownership anti-patterns; F1.12 keys on
//! cross-cutting-concern consistency — error / log / config).
//!
//! **Sources (context7, `/sverweij/dependency-cruiser`, High reputation, bench
//! 87.06)**: dependency-cruiser enforces layer / path policy via the
//! `forbidden` block (`name`, `from.path`, `to.path` regex). ArchUnit enforces
//! architectural rules in Java. In a single file, the analogous "consistency"
//! check is whether the file follows the workspace's cross-cutting-concern
//! convention — when a single file mixes error/log/config patterns, drift
//! is in progress.
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::score_utils::density_score;

/// Density→score scale (ADVISORY-tier; arch consistency is a meta-signal).
const SCALE: f32 = 6.0;

/// Error-handling patterns.
const PANIC: &[u8] = b"panic!";
const UNWRAP: &[u8] = b".unwrap()";
const EXPECT: &[u8] = b".expect(";
const RESULT_T: &[u8] = b"Result<";
const ANYHOW: &[u8] = b"anyhow::";
const THISERROR: &[u8] = b"thiserror::";

/// Logging patterns.
const PRINTLN: &[u8] = b"println!";
const EPRINTLN: &[u8] = b"eprintln!";
const TRACING_INFO: &[u8] = b"tracing::info!";
const TRACING_ERROR: &[u8] = b"tracing::error!";
const TRACING_DEBUG: &[u8] = b"tracing::debug!";
const LOG_INFO: &[u8] = b"log::info!";
const LOG_ERROR: &[u8] = b"log::error!";

/// Config patterns.
const ENV_VAR: &[u8] = b"std::env::var(";
const ENV_VAR_SHORT: &[u8] = b"env::var(";
const CONFIG_LOAD: &[u8] = b"config::";
const FIGMENT: &[u8] = b"figment::";
const DOTENV: &[u8] = b"dotenv::";
const PY_OS_ENVIRON: &[u8] = b"os.environ[";
const PY_OS_ENVIRON_GET: &[u8] = b"os.environ.get";
const PY_OS_GETENV: &[u8] = b"os.getenv(";

/// Async runtime patterns.
const TOKIO: &[u8] = b"tokio::";
const ASYNC_STD: &[u8] = b"async_std::";
const SMOL: &[u8] = b"smol::";

/// Hardcoded role/permission string-literal check.
const ROLE_ADMIN_EQ: &[u8] = b".role == \"admin\"";
const ROLE_USER_EQ: &[u8] = b".role == \"user\"";
const ROLE_EQ_GENERIC: &[u8] = b".role == \"";

#[cfg(target_os = "linux")]
const OS_PATH_SEP: char = '/';
#[cfg(not(target_os = "linux"))]
const OS_PATH_SEP: char = '\\';

/// Findings of a single arch-consistency analysis pass.
#[derive(Debug, Clone, Default)]
pub struct ArchConsistencyReport {
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl ArchConsistencyReport {
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count > 0 {
            self.violations += count;
            self.weighted_total += count as f32 * weight;
            self.findings.push((message.to_string(), count));
        }
    }
}

/// Count occurrences of `needle` in `bytes` outside non-executable regions.
fn count_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

/// Detect mixed error-handling styles in Rust files.
fn detect_mixed_error_styles(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_panic = count_executable(bytes, regions, PANIC) > 0;
    let has_unwrap = count_executable(bytes, regions, UNWRAP) > 0;
    let has_expect = count_executable(bytes, regions, EXPECT) > 0;
    let has_result = count_executable(bytes, regions, RESULT_T) > 0;
    let has_anyhow = count_executable(bytes, regions, ANYHOW) > 0;
    let has_thiserror = count_executable(bytes, regions, THISERROR) > 0;
    // Styles present in the file: panic, unwrap/expect, Result<_, _>, anyhow/thiserror.
    let panic_style = has_panic as u8;
    let unwrap_style = (has_unwrap || has_expect) as u8;
    let result_style = has_result as u8;
    let typed_error_style = (has_anyhow || has_thiserror) as u8;
    let style_count = panic_style + unwrap_style + result_style + typed_error_style;
    // ≥ 3 distinct error styles in one file is drift.
    if style_count >= 3 { 1 } else { 0 }
}

/// Detect mixed logging styles (println + tracing).
fn detect_mixed_logging(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_println = count_executable(bytes, regions, PRINTLN) > 0;
    let has_eprintln = count_executable(bytes, regions, EPRINTLN) > 0;
    let has_tracing = count_executable(bytes, regions, TRACING_INFO) > 0
        || count_executable(bytes, regions, TRACING_ERROR) > 0
        || count_executable(bytes, regions, TRACING_DEBUG) > 0;
    let has_log = count_executable(bytes, regions, LOG_INFO) > 0
        || count_executable(bytes, regions, LOG_ERROR) > 0;
    let has_stdout_stderr = (has_println || has_eprintln) as u8;
    let has_structured = (has_tracing || has_log) as u8;
    if has_stdout_stderr == 1 && has_structured == 1 {
        1
    } else {
        0
    }
}

/// Detect mixed config sources (env::var + config:: or dotenv::) for Rust.
fn detect_mixed_config_rust(bytes: &[u8], regions: &[(usize, usize)]) -> bool {
    let has_env = count_executable(bytes, regions, ENV_VAR) > 0
        || count_executable(bytes, regions, ENV_VAR_SHORT) > 0;
    let has_structured = count_executable(bytes, regions, CONFIG_LOAD) > 0
        || count_executable(bytes, regions, FIGMENT) > 0
        || count_executable(bytes, regions, DOTENV) > 0;
    has_env && has_structured
}

/// Detect mixed config sources for Python.
fn detect_mixed_config_python(bytes: &[u8], regions: &[(usize, usize)]) -> bool {
    let has_env = count_executable(bytes, regions, PY_OS_ENVIRON) > 0
        || count_executable(bytes, regions, PY_OS_ENVIRON_GET) > 0
        || count_executable(bytes, regions, PY_OS_GETENV) > 0;
    let has_structured = count_executable(bytes, regions, b"dotenv") > 0
        || count_executable(bytes, regions, b"pydantic_settings") > 0
        || count_executable(bytes, regions, b"BaseSettings") > 0;
    has_env && has_structured
}

/// Detect mixed config sources (env::var + config:: or dotenv::).
fn detect_mixed_config(bytes: &[u8], regions: &[(usize, usize)], lang: &str) -> usize {
    let mixed = match lang {
        "rust" | "rs" => detect_mixed_config_rust(bytes, regions),
        "python" | "py" => detect_mixed_config_python(bytes, regions),
        _ => false,
    };
    if mixed { 1 } else { 0 }
}

/// Detect hardcoded role/permission string-literal check.
fn detect_hardcoded_role(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let admin = count_executable(bytes, regions, ROLE_ADMIN_EQ);
    let user = count_executable(bytes, regions, ROLE_USER_EQ);
    let generic = count_executable(bytes, regions, ROLE_EQ_GENERIC);
    admin + user + generic
}

/// Detect conflicting async runtimes in Rust.
fn detect_mixed_async_runtimes(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let tokyo = count_executable(bytes, regions, TOKIO);
    let async_std = count_executable(bytes, regions, ASYNC_STD);
    let smol = count_executable(bytes, regions, SMOL);
    let runtime_count = (tokyo > 0) as u8 + (async_std > 0) as u8 + (smol > 0) as u8;
    if runtime_count >= 2 { 1 } else { 0 }
}

/// Analyze architectural consistency in `source` for the given language.
pub fn analyze_arch_consistency(source: &str, lang: &str) -> ArchConsistencyReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let mut report = ArchConsistencyReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    let mixed_errors = detect_mixed_error_styles(bytes, &regions);
    if mixed_errors > 0 {
        report.push(
            "≥3 distinct error-handling styles in one file (panic + Result + anyhow / \
             unwrap) — no convention, drift accumulates; pick one style per layer",
            1,
            0.6,
        );
    }
    let mixed_logs = detect_mixed_logging(bytes, &regions);
    if mixed_logs > 0 {
        report.push(
            "mixed logging styles in one file (println!/eprintln! + tracing/log) — \
             stdout/stderr mixed with structured logging breaks log aggregation",
            1,
            0.5,
        );
    }
    let mixed_config = detect_mixed_config(bytes, &regions, lang);
    if mixed_config > 0 {
        report.push(
            "mixed config sources (env::var + config::/figment/dotenv) — pick one \
             config layer, route the other through it",
            1,
            0.6,
        );
    }
    let hardcoded_role = detect_hardcoded_role(bytes, &regions);
    if hardcoded_role > 0 {
        report.push(
            "hardcoded role string comparison (`.role == \"admin\"`) — stringly-typed \
             role; use enum/RoleId so renames are caught at compile time",
            hardcoded_role,
            0.7,
        );
    }
    if lang == "rust" || lang == "rs" {
        let mixed_runtimes = detect_mixed_async_runtimes(bytes, &regions);
        if mixed_runtimes > 0 {
            report.push(
                "≥2 async runtimes in one file (tokio + async-std / smol) — \
                 runtime conflict; pick one runtime per workspace",
                1,
                0.8,
            );
        }
    }
    let _ = OS_PATH_SEP; // reserved for future path-based layer detection
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`ArchConsistencyReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
pub fn score_arch_consistency(report: &ArchConsistencyReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_single_style_clean() {
        let src = r#"
use thiserror::Error;
use tracing::info;
use std::env::var;

#[derive(Error, Debug)]
pub enum AppError { NotFound }

fn handler() -> Result<(), AppError> { Ok(()) }
fn load() { var("X").ok(); }
fn log() { info!("hello"); }
"#;
        let r = analyze_arch_consistency(src, "rust");
        assert_eq!(
            r.violations, 0,
            "single-style file is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn mixed_error_styles_flagged() {
        let src = r#"
fn a() -> Result<(), String> { panic!("oops"); }
fn b() { let x = foo().unwrap(); bar().expect("bad"); baz()?; }
fn c() { anyhow::bail!("err"); }
"#;
        let r = analyze_arch_consistency(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("error-handling styles")),
            "≥3 error styles flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn mixed_logging_styles_flagged() {
        let src = r#"
fn a() { println!("debug"); }
fn b() { tracing::info!("info"); }
"#;
        let r = analyze_arch_consistency(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("logging styles")),
            "println + tracing flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn mixed_config_sources_flagged() {
        let src = r#"
use config::Config;
fn a() { std::env::var("X").ok(); }
fn b() { let c = Config::default(); }
"#;
        let r = analyze_arch_consistency(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("config sources")),
            "env::var + config:: flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn hardcoded_role_string_flagged() {
        let src = r#"
fn check(u: &User) -> bool {
    if u.role == "admin" { true } else { false }
}
"#;
        let r = analyze_arch_consistency(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("hardcoded role")),
            "hardcoded role string flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn mixed_async_runtimes_flagged() {
        let src = r#"
use tokio::runtime::Runtime;
use async_std::task;
fn go() {
    let rt = Runtime::new().unwrap();
    task::block_on(async { 1 });
}
"#;
        let r = analyze_arch_consistency(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("async runtimes")),
            "tokio + async-std flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn only_one_runtime_clean() {
        let src = r#"
use tokio::runtime::Runtime;
fn go() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async { 1 });
}
"#;
        let r = analyze_arch_consistency(src, "rust");
        // No mixed-runtimes finding (only tokio).
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("async runtimes")),
            "only tokio → no mixed-runtime finding: {:?}",
            r.findings
        );
    }

    #[test]
    fn empty_file_clean() {
        let r = analyze_arch_consistency("", "rust");
        assert_eq!(r.violations, 0, "empty file: {:?}", r.findings);
    }

    #[test]
    fn comment_excluded() {
        let src = r#"
// panic!("oops")  // would be flagged if executable
// tracing::info!("x")
// env::var("X")
fn handler() -> Result<(), String> { Ok(()) }
"#;
        let r = analyze_arch_consistency(src, "rust");
        assert_eq!(
            r.violations, 0,
            "all smells in comments are excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn python_mixed_config_flagged() {
        let src = r#"
import os
from pydantic_settings import BaseSettings

def a():
    x = os.environ.get("X")
    return x

class Settings(BaseSettings):
    debug: bool = False
"#;
        let r = analyze_arch_consistency(src, "python");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("config sources")),
            "Python os.environ + pydantic-settings flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_arch_consistency(
            r#"
fn a() -> Result<(), String> { panic!("oops"); }
fn b() { let x = foo().unwrap(); }
fn c() { println!("debug"); tracing::info!("info"); }
fn d() { std::env::var("X").ok(); let c = config::Config::default(); }
fn e(u: &User) -> bool { u.role == "admin" }
"#,
            "rust",
        );
        let good = analyze_arch_consistency(
            r#"
use thiserror::Error;
use tracing::info;
fn handler() -> Result<(), AppError> { Ok(()) }
"#,
            "rust",
        );
        assert!(
            score_arch_consistency(&bad) < score_arch_consistency(&good),
            "drift-laden file ({:.3}) must score below consistent ({:.3})",
            score_arch_consistency(&bad),
            score_arch_consistency(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_arch_consistency(
            r#"
fn a() -> Result<(), String> { panic!("oops"); }
fn b() { let x = foo().unwrap(); }
fn c() { println!("debug"); tracing::info!("info"); }
fn d() { std::env::var("X").ok(); }
fn e(u: &User) -> bool { u.role == "admin" }
"#,
            "rust",
        );
        let s = score_arch_consistency(&r);
        assert!(
            s > 0.0,
            "drift file with many smells must not score 0.0: {s}"
        );
    }
}
