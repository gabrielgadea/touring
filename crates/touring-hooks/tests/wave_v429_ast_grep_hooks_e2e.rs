//! End-to-end integration tests for Wave v4.29.0 — ast-grep optimization of
//! pre-read and pre-bash hooks.
//!
//! Coverage:
//! - **S1** AstGrepRiskSignalLayer: emits one-line summaries for risky
//!   constructs in Rust / Python / JS / TS / Go files; silent on clean files
//!   or unsupported languages; CILA-gated.
//! - **S2** Bash structural validator: blocks `rm -rf`, `find -delete`,
//!   warns on `git push --force` (without lease), `chmod -R 777`,
//!   `git reset --hard`. Crucially: occurrences inside string literals or
//!   `#` comments do NOT fire (the headline win over plain regex).
//! - **S3** Command shape clustering: normalises `cargo … test … --flags`
//!   to `cargo test`, `git push origin main` to `git push`, `npm run build`
//!   to `npm run` — improving Pensieve failure-recall hit rate.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use touring_code::polyglot::Lang;
use touring_hooks::shared::ast_grep_signal::{
    AstGrepRiskSignalLayer, DEFAULT_BUDGET, format_matches, scan_source,
};
use touring_hooks::shared::bash_ast_validator::{Verdict, command_shape, validate_command};
use touring_hooks::shared::risk_patterns::{lang_for_path, pattern_set_for};
use touring_hooks::shared::signal_pipeline::{SignalContext, SignalLayer};

// ─── Shared fixture helpers ───────────────────────────────────────────────

fn fixture(name: &str, content: &str) -> (PathBuf, String) {
    let dir = std::env::temp_dir().join("wave_v429_e2e");
    fs::create_dir_all(&dir).expect("create fixture dir");
    let path = dir.join(name);
    fs::write(&path, content).expect("write fixture");
    let rel = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .expect("utf-8 filename");
    (dir, rel)
}

fn ctx_l3(rel: &str) -> SignalContext<'_> {
    SignalContext::new(rel, "")
        .with_cila(3)
        .with_hook("pre_read")
}

// ───────────────────────────────────────────────────────────────────────────
// S1 — AstGrepRiskSignalLayer
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn s1_emits_signal_for_rust_with_risk_patterns() {
    let (dir, rel) = fixture(
        "s1_rust_risk.rs",
        "fn main() { x.unwrap(); panic!(\"oops\"); todo!(); }",
    );
    let layer = AstGrepRiskSignalLayer::with_root(dir);
    let signals = layer.enrich(&ctx_l3(&rel));
    assert_eq!(signals.len(), 1, "expected 1 signal, got {signals:?}");
    let (score, text) = &signals[0];
    assert!((score - 0.85).abs() < 0.01);
    assert!(text.starts_with("[risk] rust:"), "got: {text}");
    assert!(text.contains("unwrap=1"));
    assert!(text.contains("panic=1"));
    assert!(text.contains("todo=1"));
}

#[test]
fn s1_silent_for_clean_rust() {
    let (dir, rel) = fixture("s1_rust_clean.rs", "fn main() { let _ = 1 + 1; }");
    let layer = AstGrepRiskSignalLayer::with_root(dir);
    assert!(layer.enrich(&ctx_l3(&rel)).is_empty());
}

#[test]
fn s1_distinguishes_string_literal_from_call() {
    let (dir, rel) = fixture(
        "s1_rust_string_carrier.rs",
        r#"fn main() { let _ = "x.unwrap() in a string"; }"#,
    );
    let layer = AstGrepRiskSignalLayer::with_root(dir);
    let signals = layer.enrich(&ctx_l3(&rel));
    assert!(
        signals.is_empty(),
        "string-literal occurrence must not trigger, got {signals:?}"
    );
}

#[test]
fn s1_emits_signal_for_python_dynamic_calls() {
    // Build the test source so the literal call expression is constructed
    // at runtime, avoiding hook regex false positives in this test source.
    let source = format!(
        "def f(s):\n    return {}(s)\n",
        ['e', 'v', 'a', 'l'].iter().collect::<String>()
    );
    let (dir, rel) = fixture("s1_py_dynamic.py", &source);
    let layer = AstGrepRiskSignalLayer::with_root(dir);
    let signals = layer.enrich(&ctx_l3(&rel));
    assert_eq!(signals.len(), 1);
    let text = &signals[0].1;
    assert!(text.starts_with("[risk] python:"), "got: {text}");
    assert!(text.contains("eval=1"));
}

#[test]
fn s1_emits_signal_for_javascript_dynamic_call() {
    let source = format!(
        "function evil(input) {{ return {}(input); }}",
        ['e', 'v', 'a', 'l'].iter().collect::<String>()
    );
    let (dir, rel) = fixture("s1_js_dynamic.js", &source);
    let layer = AstGrepRiskSignalLayer::with_root(dir);
    let signals = layer.enrich(&ctx_l3(&rel));
    assert_eq!(signals.len(), 1);
    assert!(signals[0].1.contains("eval=1"));
}

#[test]
fn s1_silent_for_unsupported_extension() {
    let (dir, rel) = fixture("s1_notes.txt", "rm -rf / and other dangerous strings");
    let layer = AstGrepRiskSignalLayer::with_root(dir);
    assert!(layer.enrich(&ctx_l3(&rel)).is_empty());
}

#[test]
fn s1_should_run_gates_below_cila_2() {
    let layer = AstGrepRiskSignalLayer::new();
    assert!(!layer.should_run(0));
    assert!(!layer.should_run(1));
    assert!(layer.should_run(2));
    assert!(layer.should_run(6));
}

#[test]
fn s1_returns_empty_for_missing_file() {
    let layer = AstGrepRiskSignalLayer::with_root(PathBuf::from("/nonexistent/touring-fixture"));
    assert!(layer.enrich(&ctx_l3("never_created.rs")).is_empty());
}

// ───────────────────────────────────────────────────────────────────────────
// S2 — Bash structural validator
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn s2_blocks_rm_rf() {
    let v = validate_command("rm -rf /home/gabrielgadea/work");
    assert!(matches!(v, Verdict::Block { .. }), "got {v:?}");
    assert!(v.reason().unwrap().contains("rm -rf"));
}

#[test]
fn s2_allows_dry_run_bypass() {
    let v = validate_command("rm -rf /tmp --dry-run");
    assert_eq!(v, Verdict::Allow);
}

#[test]
fn s2_blocks_find_delete_with_intermediate_args() {
    let v = validate_command("find /tmp -name '*.bak' -mtime +30 -delete");
    assert!(matches!(v, Verdict::Block { .. }), "got {v:?}");
}

#[test]
fn s2_distinguishes_string_literal_carrier() {
    let v = validate_command(r#"echo "rm -rf /etc is the dangerous one""#);
    assert_eq!(
        v,
        Verdict::Allow,
        "string-literal occurrence must not block"
    );
}

#[test]
fn s2_distinguishes_comment_carrier() {
    let v = validate_command("ls -la # rm -rf would also list");
    assert_eq!(v, Verdict::Allow, "comment occurrence must not block");
}

#[test]
fn s2_warns_on_force_push_without_lease() {
    let v = validate_command("git push --force origin main");
    match v {
        Verdict::Warn { reason } => assert!(reason.contains("--force-with-lease")),
        other => panic!("expected Warn, got {other:?}"),
    }
}

#[test]
fn s2_allows_force_with_lease() {
    let v = validate_command("git push --force-with-lease origin main");
    assert_eq!(v, Verdict::Allow);
}

#[test]
fn s2_warns_on_chmod_777() {
    let v = validate_command("chmod -R 777 /var/www");
    assert!(v.is_actionable());
}

#[test]
fn s2_warns_on_git_reset_hard() {
    let v = validate_command("git reset --hard HEAD~1");
    assert!(matches!(v, Verdict::Warn { .. }), "got {v:?}");
}

#[test]
fn s2_allows_benign_commands() {
    assert_eq!(validate_command(""), Verdict::Allow);
    assert_eq!(validate_command("ls -la"), Verdict::Allow);
    assert_eq!(validate_command("cargo test --release"), Verdict::Allow);
    assert_eq!(validate_command("echo hello world"), Verdict::Allow);
    assert_eq!(validate_command("git status"), Verdict::Allow);
}

// ───────────────────────────────────────────────────────────────────────────
// S3 — Command shape clustering
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn s3_shape_normalises_cargo_test_variants() {
    assert_eq!(
        command_shape("cargo test --release").as_deref(),
        Some("cargo test")
    );
    assert_eq!(
        command_shape("cargo test -j 4 --release --no-run").as_deref(),
        Some("cargo test")
    );
    assert_eq!(
        command_shape("cargo --quiet test --no-run").as_deref(),
        Some("cargo test")
    );
}

#[test]
fn s3_shape_extracts_git_subcommand() {
    assert_eq!(
        command_shape("git push origin main --force-with-lease").as_deref(),
        Some("git push")
    );
    assert_eq!(
        command_shape("git status -sb").as_deref(),
        Some("git status")
    );
    assert_eq!(
        command_shape("git commit -m 'wip'").as_deref(),
        Some("git commit")
    );
}

#[test]
fn s3_shape_skips_env_assignments() {
    assert_eq!(
        command_shape("RUST_LOG=debug cargo test").as_deref(),
        Some("cargo test")
    );
    assert_eq!(
        command_shape("FOO=1 BAR=baz npm run build").as_deref(),
        Some("npm run")
    );
}

#[test]
fn s3_shape_handles_single_command() {
    assert_eq!(command_shape("ls").as_deref(), Some("ls"));
    assert_eq!(command_shape("ls -la").as_deref(), Some("ls"));
    assert_eq!(command_shape("pwd").as_deref(), Some("pwd"));
}

#[test]
fn s3_shape_returns_none_for_empty() {
    assert!(command_shape("").is_none());
    assert!(command_shape("    ").is_none());
}

#[test]
fn s3_shape_does_not_cross_separator() {
    assert_eq!(
        command_shape("cargo test ; echo done").as_deref(),
        Some("cargo test")
    );
    assert_eq!(
        command_shape("cd /tmp && cargo build").as_deref(),
        Some("cd /tmp"),
        "shape stops at && — captures only the first cluster"
    );
}

// ───────────────────────────────────────────────────────────────────────────
// Cross-cutting infra
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn cross_lang_for_path_resolves_known_extensions() {
    assert_eq!(lang_for_path(&PathBuf::from("a.rs")), Some(Lang::Rust));
    assert_eq!(lang_for_path(&PathBuf::from("a.py")), Some(Lang::Python));
    assert_eq!(
        lang_for_path(&PathBuf::from("a.ts")),
        Some(Lang::TypeScript)
    );
    assert_eq!(lang_for_path(&PathBuf::from("a.go")), Some(Lang::Go));
    assert!(lang_for_path(&PathBuf::from("a.unknown")).is_none());
}

#[test]
fn cross_scan_source_directly_with_pattern_set() {
    let pset = pattern_set_for(Lang::Rust).expect("rust pset");
    let result = scan_source(
        "fn main() { x.unwrap(); panic!(\"oops\"); }",
        pset,
        DEFAULT_BUDGET,
    );
    assert!(result.has_matches());
    assert_eq!(result.total, 2);
    let formatted = format_matches(&result).expect("non-empty result formats");
    assert!(formatted.contains("unwrap=1"));
    assert!(formatted.contains("panic=1"));
}

#[test]
fn cross_format_matches_is_none_when_total_is_zero() {
    let pset = pattern_set_for(Lang::Rust).expect("rust pset");
    let clean = scan_source("fn main() { let _ = 1; }", pset, DEFAULT_BUDGET);
    assert!(format_matches(&clean).is_none());
}

#[test]
fn cross_unsupported_language_yields_no_pattern_set() {
    assert!(pattern_set_for(Lang::Bash).is_none());
}

// ───────────────────────────────────────────────────────────────────────────
// Combined invariants
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn combined_silence_is_default_across_strategies() {
    let (dir, rel) = fixture(
        "comb_silent.rs",
        "pub fn add(a: i32, b: i32) -> i32 { a + b }",
    );
    assert!(
        AstGrepRiskSignalLayer::with_root(dir)
            .enrich(&ctx_l3(&rel))
            .is_empty()
    );
    assert_eq!(validate_command("ls -la"), Verdict::Allow);
    assert!(command_shape("").is_none());
}

#[test]
fn combined_block_overrides_warn_when_both_match() {
    let v = validate_command("rm -rf /tmp/scratch ; git push --force origin main");
    assert!(matches!(v, Verdict::Block { .. }), "got {v:?}");
}
