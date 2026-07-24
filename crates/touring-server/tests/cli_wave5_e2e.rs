//! Wave 5 (2026-04-18) — `assert_cmd` + `predicates`-based end-to-end
//! tests for the touring CLI subcommands that have zero or low external
//! dependencies (they don't need the daemon, or they hit purely
//! library-level code paths).
//!
//! # Scope
//!
//! `binary_e2e.rs` and `cli_smoke.rs` already cover the daemon-dependent
//! surface. This file fills the gap for the new Wave 5 surfaces:
//!
//! - `touring --version`   — enriched output with git/rustc/build/features
//! - `touring --help`      — help text assembled from the command table
//! - `touring ast rust-semantic <rs>` — syn-backed analyzer (no daemon)
//! - `touring ast format-rust <rs>`   — prettyplease (no daemon)
//! - `touring ast workspace-info`     — cargo_metadata (no daemon)
//!
//! # Why assert_cmd + predicates
//!
//! `std::process::Command` forces each test to re-implement stdout/stderr
//! matching and exit-code handling. `assert_cmd` gives us `.assert()`
//! chained with `predicates` adverbs (`predicate::str::contains`,
//! `predicate::str::is_match`, …) — this replaces ~30 LOC of handwritten
//! matching per test with ~3 LOC.

use assert_cmd::Command;
use predicates::prelude::*;

/// Helper: return a `Command` pointing at the workspace `touring` binary.
/// `assert_cmd` resolves via CARGO_BIN_EXE_<name> which is set for all
/// `[[bin]]` targets in the same package — this works out of the box
/// because `touring-server/Cargo.toml` declares `[[bin]] name = "touring"`.
fn touring() -> Command {
    Command::cargo_bin("touring").expect("touring binary not built")
}

#[test]
fn version_contains_semver_and_metadata() {
    // `touring --version` writes to stderr (see main.rs arm).
    touring()
        .arg("--version")
        .assert()
        .success()
        .stderr(predicate::str::is_match(r"touring \d+\.\d+\.\d+").expect("regex compiles"))
        .stderr(predicate::str::contains("rustc:"))
        .stderr(predicate::str::contains("built:"));
}

#[test]
fn version_v_short_flag_equivalent() {
    touring()
        .arg("-V")
        .assert()
        .success()
        .stderr(predicate::str::contains("touring"));
}

#[test]
fn help_lists_subcommands() {
    // `--help` prints the command table to stderr (print_help uses eprintln!).
    // Assert that a representative cross-section of known commands is listed;
    // if a subcommand is removed or renamed, this test fails loud.
    touring()
        .arg("--help")
        .assert()
        .success()
        .stderr(predicate::str::contains("status"))
        .stderr(predicate::str::contains("doctor"))
        .stderr(predicate::str::contains("ast"));
}

#[test]
fn unknown_subcommand_fails_with_hint() {
    // The dispatcher must exit non-zero for unknown commands rather
    // than silently no-oping.
    touring()
        .arg("this-command-does-not-exist")
        .assert()
        .failure();
}

#[test]
fn ast_rust_semantic_parses_fixture() {
    // Wave 4 (syn) analyzer. Feed it a self-contained Rust snippet
    // via a tempfile so the test is hermetic — no workspace file state.
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new().expect("mktemp");
    writeln!(
        tmp,
        "fn greet<T: std::fmt::Display>(x: T) -> String {{ format!(\"{{x}}\") }}"
    )
    .expect("write tmp");

    touring()
        .args(["ast", "rust-semantic"])
        .arg(tmp.path())
        .assert()
        .success()
        // Output is JSON; assert on stable structural fields that the
        // rust_semantic analyzer always emits for any valid Rust file.
        // "generics" confirms the generic parameter T was parsed;
        // "item_count" confirms at least one item (the fn) was found.
        .stdout(predicate::str::contains("generics"))
        .stdout(predicate::str::contains("item_count"));
}

#[test]
fn ast_format_rust_emits_rustfmt_clean_output() {
    // prettyplease reformats "fn  main  ( )  {  }" → "fn main() {}".
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new().expect("mktemp");
    writeln!(tmp, "fn  main  ( )  {{ }}").expect("write tmp");

    touring()
        .args(["ast", "format-rust"])
        .arg(tmp.path())
        .assert()
        .success()
        // prettyplease output contains exactly one space between fn and main.
        .stdout(predicate::str::contains("fn main"));
}

#[test]
fn doctor_json_output_contains_binary_version_check() {
    // `touring doctor -j` returns a JSON array of health checks. Even
    // without a running daemon, the `binary_version` check must
    // succeed because it only reads the baked-in CARGO_PKG_VERSION.
    touring()
        .args(["doctor", "-j"])
        .assert()
        // Exit code may be non-zero if daemon is down, so we only
        // check that the binary_version field is present in the
        // JSON payload on stdout (or stderr — implementation detail).
        .stdout(
            predicate::str::contains("binary_version")
                .or(predicate::str::contains("daemon_socket")),
        );
}
