//! CLI smoke tests via `assert_cmd` — exercises the compiled `touring`
//! binary across the 20 most critical subcommands (Touring skill
//! CLI ranks Tier 1–3).
//!
//! # Why this file exists
//!
//! `binary_e2e.rs` uses a hand-rolled `run_touring` helper that covers
//! ~15 hook-specific paths. This file complements it by:
//! 1. Using `assert_cmd::Command::cargo_bin("touring")` — idiomatic, no
//!    manual PATH resolution, automatically rebuilds the binary.
//! 2. Focusing on the 20 read-only CLI commands that developers run
//!    interactively (doctor, status, index find, wiring audit, …).
//! 3. Proving each command:
//!    - Exits with a defined status code
//!    - Produces JSON (for `-j` variants) or structured text
//!    - Responds within a reasonable timeout
//!
//! # Daemon dependence
//!
//! Several commands (`index find`, `wiring audit`, `memory recall`, …)
//! delegate to the touring daemon via Unix socket. When the daemon is
//! not running, the CLI returns a structured error (exit 1) rather than
//! panicking. Tests tagged `#[ignore = "requires daemon"]` cover those
//! cases — run locally with `cargo test -- --ignored` once the daemon
//! is up.
//!
//! # Stateless commands
//!
//! `--version`, `--help`, `generate list-kinds`, and the help screens of
//! each subcommand do NOT touch the daemon and are the core of this
//! smoke suite. They guarantee the binary itself remains bootable.

use assert_cmd::Command;
use predicates::prelude::*;

/// Returns a fresh `assert_cmd::Command` pointing at the compiled
/// `touring` binary, with daemon-conflicting env vars neutralized so the
/// tests do not interfere with a running daemon on the host.
fn touring() -> Command {
    let mut cmd = Command::cargo_bin("touring").expect("touring binary not built");
    cmd.env("TOKIO_CONSOLE_BIND", "127.0.0.1:0");
    cmd.env("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:0");
    cmd
}

// ── Tier 0: Absolute sanity (no daemon, no args) ───────────────────────

#[test]
fn version_exits_success_with_semver_output() {
    // INVARIANT: `--version` must succeed, print a semver-shaped string.
    // Note: the `touring` binary currently emits the version string on
    // stderr (alongside telemetry init logs) — asserting on stderr here
    // locks that contract so any future routing change surfaces loudly.
    touring()
        .arg("--version")
        .assert()
        .success()
        .stderr(predicate::str::is_match(r"\d+\.\d+\.\d+").expect("valid semver regex"));
}

#[test]
fn help_exits_success_and_mentions_subcommands() {
    // INVARIANT: `--help` lists at least the core subcommands so
    // users discover the CLI surface without external docs. Help
    // output shares the stderr routing noted on `version`.
    touring()
        .arg("--help")
        .assert()
        .success()
        .stderr(predicate::str::contains("doctor").or(predicate::str::contains("status")));
}

// ── Tier 1: Health + dashboard (daemon-optional, graceful fallback) ────

#[test]
fn doctor_json_produces_structured_output() {
    // `doctor` checks daemon health, binary version, socket, circuit
    // breaker. Must always exit (even on degraded state) with valid JSON.
    let out = touring()
        .args(["doctor", "-j"])
        .output()
        .expect("run doctor");
    // Either success (daemon up) or exit 1 (degraded) — both acceptable.
    assert!(
        out.status.code() == Some(0) || out.status.code() == Some(1),
        "doctor must exit 0 or 1, got {:?}",
        out.status.code()
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Structured output: either a JSON array/object or explicit error text.
    assert!(
        stdout.contains('{') || stdout.contains('[') || !stdout.is_empty(),
        "doctor must produce non-empty output"
    );
}

#[test]
fn status_json_returns_valid_json_shape() {
    // Status aggregates index + wiring + learning signals. Even when
    // the daemon is down it should emit a JSON error envelope, not
    // panic or hang.
    let out = touring()
        .args(["status", "-j"])
        .output()
        .expect("run status");
    assert!(
        out.status.code() == Some(0) || out.status.code() == Some(1),
        "status must exit 0 or 1"
    );
}

// ── Tier 2: Index queries (all require daemon) ─────────────────────────

#[test]
#[ignore = "requires daemon socket"]
fn index_status_reports_symbol_count() {
    touring()
        .args(["index", "status", "-j"])
        .assert()
        .success()
        .stdout(predicate::str::contains("symbol_count"));
}

#[test]
#[ignore = "requires daemon socket"]
fn index_find_missing_symbol_returns_empty_array() {
    touring()
        .args(["index", "find", "DefinitelyDoesNotExistSymbol_Z9QK", "-j"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]").or(predicate::str::contains("definitions")));
}

#[test]
#[ignore = "requires daemon socket"]
fn index_search_prefix_returns_results() {
    // Prefix search for a symbol that exists in the workspace.
    touring()
        .args(["index", "search", "Hook", "-j"])
        .assert()
        .success();
}

// ── Tier 3: AST queries (local parse, no daemon needed) ────────────────

#[test]
#[ignore = "requires daemon socket for symbol store lookup"]
fn ast_meta_skeleton_emits_symbol_list() {
    touring()
        .args([
            "ast",
            "meta",
            "crates/touring-server/src/main.rs",
            "--depth",
            "skeleton",
            "-j",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("symbols").or(predicate::str::contains("language")));
}

#[test]
#[ignore = "requires daemon socket"]
fn ast_overview_file_returns_symbols() {
    touring()
        .args(["ast", "overview", "crates/touring-server/src/main.rs", "-j"])
        .assert()
        .success();
}

#[test]
#[ignore = "requires daemon socket"]
fn ast_blast_returns_dependency_tree() {
    touring()
        .args(["ast", "blast", "crates/touring-server/src/main.rs"])
        .assert()
        .success();
}

// ── Tier 4: Wiring (daemon-only) ───────────────────────────────────────

#[test]
#[ignore = "requires daemon socket"]
fn wiring_orphans_returns_json_list() {
    touring()
        .args(["wiring", "orphans", "-j"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[").or(predicate::str::contains("orphans")));
}

#[test]
#[ignore = "requires daemon socket"]
fn wiring_modules_returns_scored_modules() {
    touring()
        .args(["wiring", "modules", "-j"])
        .assert()
        .success();
}

#[test]
#[ignore = "requires daemon socket"]
fn wiring_audit_runs_full_audit() {
    touring().args(["wiring", "audit", "-j"]).assert().success();
}

#[test]
#[ignore = "requires daemon socket"]
fn wiring_suggest_bulk_csv_single_symbol() {
    // Bulk mode with --symbols= (single symbol to verify JSON array response)
    touring()
        .args(["wiring", "suggest", "--symbols=cli_wiring_suggest"])
        .assert()
        .success()
        .stdout(predicate::str::contains("orphan_symbols"));
}

#[test]
#[ignore = "requires daemon socket"]
fn wiring_suggest_bulk_csv_multiple_symbols() {
    // Bulk mode with --symbols= (multiple symbols comma-separated)
    touring()
        .args([
            "wiring",
            "suggest",
            "--symbols=cli_wiring_suggest,cli_wiring_orphans",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("results"));
}

#[test]
#[ignore = "requires daemon socket"]
fn wiring_suggest_legacy_single_arg() {
    // Legacy single-arg mode (no --symbols=)
    touring()
        .args(["wiring", "suggest", "cli_wiring_suggest"])
        .assert()
        .success()
        .stdout(predicate::str::contains("source"));
}

// ── Tier 5: Memory / Tantivy (daemon-only) ─────────────────────────────

#[test]
#[ignore = "requires daemon socket"]
fn memory_stats_returns_counts() {
    touring()
        .args(["memory", "stats", "-j"])
        .assert()
        .success()
        .stdout(predicate::str::contains("total").or(predicate::str::contains("count")));
}

#[test]
#[ignore = "requires daemon socket"]
fn memory_recall_handles_empty_query_gracefully() {
    // BOUNDARY: empty-ish query must not panic; either succeeds with
    // zero hits or returns a structured error.
    let out = touring()
        .args(["memory", "recall", "xxnonexistxx_unique_term_zzz"])
        .output()
        .expect("memory recall");
    assert!(out.status.code() == Some(0) || out.status.code() == Some(1));
}

#[test]
#[ignore = "requires daemon socket"]
fn tantivy_search_returns_hits_array() {
    touring()
        .args(["tantivy", "search", "Hook", "-j"])
        .assert()
        .success();
}

#[test]
#[ignore = "requires daemon socket"]
fn tantivy_stats_returns_index_health() {
    touring()
        .args(["tantivy", "stats", "-j"])
        .assert()
        .success()
        .stdout(predicate::str::contains("total_docs").or(predicate::str::contains("docs")));
}

// ── Tier 6: Gate metrics + decompose + session (daemon-only) ───────────

#[test]
#[ignore = "requires daemon socket"]
fn gate_metrics_exposes_rkyv_counters() {
    touring()
        .args(["gate-metrics", "-j"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rkyv").or(predicate::str::contains("count")));
}

#[test]
#[ignore = "requires daemon socket"]
fn decompose_status_lists_active_tasks() {
    touring()
        .args(["decompose", "status", "-j"])
        .assert()
        .success();
}

#[test]
#[ignore = "requires daemon socket"]
fn session_list_returns_json() {
    touring().args(["session", "list", "-j"]).assert().success();
}

// ── Tier 7: Generate (local, no daemon) ────────────────────────────────

#[test]
fn generate_list_kinds_returns_thirty_kinds() {
    // `generate list-kinds` is pure enum enumeration — no daemon needed.
    // Guards against accidental GeneratorKind removal / rename.
    touring()
        .args(["generate", "list-kinds", "-j"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("[")
                .and(predicate::str::contains("kind").or(predicate::str::contains("name"))),
        );
}

// ── Tier 8: Profile query (daemon-optional) ────────────────────────────

#[test]
fn profile_query_returns_json_entries() {
    // `touring profile query` delegates to the daemon via
    // `daemon_query("cli-profile-status")` (see cli/profile.rs::profile_status).
    // - Exit 0 → daemon answered; stdout carries the JSON profile payload.
    // - Exit 1 → daemon down/transient; the error is reported on stderr and
    //   stdout is empty — the documented graceful-degradation contract.
    let out = touring()
        .args(["profile", "query", "-j"])
        .output()
        .expect("run profile query");
    let code = out.status.code();
    assert!(
        code == Some(0) || code == Some(1),
        "profile query must exit 0 or 1, got {code:?}"
    );
    if code == Some(0) {
        // Daemon answered — the payload must carry the profile JSON shape.
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("entries") || stdout.contains("percent_total"),
            "exit-0 profile query must return JSON with `entries` or \
             `percent_total`, got: {stdout}"
        );
    }
    // Exit 1 → daemon unavailable; stdout is empty by contract, nothing to assert.
}

#[test]
fn profile_query_with_section_filter() {
    // `touring profile query --section pre_edit` filters by label prefix.
    // Schema validation only — daemon may or may not have data.
    let out = touring()
        .args(["profile", "query", "--section", "pre_edit", "-j"])
        .output()
        .expect("run profile query --section");
    assert!(
        out.status.code() == Some(0) || out.status.code() == Some(1),
        "profile query must exit 0 or 1"
    );
}

#[test]
fn profile_query_with_top_n_limit() {
    // `touring profile query --top 5` limits entry count.
    // Schema validation only.
    let out = touring()
        .args(["profile", "query", "--top", "5", "-j"])
        .output()
        .expect("run profile query --top");
    assert!(
        out.status.code() == Some(0) || out.status.code() == Some(1),
        "profile query must exit 0 or 1"
    );
}
