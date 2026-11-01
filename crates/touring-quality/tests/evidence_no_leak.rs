//! The no-leak evidence guard (25/09/2026, touring-36): a finding's evidence
//! names `arquivo:linha:regra` and NEVER the matched value — not whole, not
//! truncated, in NO output format. Without the guard, classifying a finding
//! would mean opening values, which is exactly what must never happen.
//!
//! The test plants a synthetic secret and a synthetic weak credential, runs
//! the real binary in every output format, and asserts neither the value nor
//! any slice of its body is echoed. The location and the rule MUST be there —
//! that is the other half of the contract.

use std::process::Command;

/// A value no rule would ever emit on its own.
const SECRET: &str = "ghp_Zx9Qw8ErT7YuI6OpAs5DfGh4JkL3zXcV";
const SECRET_BODY: &str = "Zx9Qw8ErT7YuI6OpAs5DfGh4JkL3zXcV";
/// A sensitive value riding the SAME line as a planted CWE-798 hit: the rule
/// fires on the `password…123` shape; the comment carries the unique value.
/// If the evidence ever echoes line content, this is what leaks.
const RIDDEN: &str = "Zx9Sup3rS3cr3tV4lu3K9Qw8";
const RIDDEN_BODY: &str = "up3rS3cr3tV4lu";

fn planted_tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("settings.rs"),
        format!("fn main() {{\n    let key = \"{SECRET}\";\n}}\n"),
    )
    .expect("secret file");
    std::fs::write(
        dir.path().join("service.yml"),
        format!("db:\n  db_password: password123  # rotate from {RIDDEN}\n"),
    )
    .expect("config file");
    dir
}

fn run_gate(gate: &str, target: &std::path::Path, format: Option<&str>) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_touring-quality"));
    cmd.args(["check", "--gate", gate, "--target"]).arg(target);
    if let Some(f) = format {
        cmd.args(["--format", f]);
    }
    let out = cmd.output().expect("run touring-quality");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn evidence_never_echoes_the_secret_in_any_output_format() {
    let dir = planted_tree();
    let secret_file = dir.path().join("settings.rs");

    for format in [None, Some("json")] {
        let out = run_gate("F2.4", &secret_file, format);
        assert!(
            !out.contains(SECRET) && !out.contains(SECRET_BODY),
            "F2.4 leaked the secret (format {format:?}): {out}"
        );
        assert!(
            out.contains(":2:provider-marker"),
            "F2.4 must still locate arquivo:linha:regra (format {format:?}): {out}"
        );
    }
}

#[test]
fn config_evidence_locates_without_echoing_the_value() {
    let dir = planted_tree();
    for format in [None, Some("json")] {
        let out = run_gate("F2.6", dir.path(), format);
        // The premise: the planted CWE-798 hit IS found and located
        // `arquivo:linha:regra` — without it, the no-leak assertion below
        // would pass vacuously.
        assert!(
            out.contains("service.yml:2:default_or_weak_credentials"),
            "F2.6 must find AND locate the planted hit (format {format:?}): {out}"
        );
        assert!(
            !out.contains(RIDDEN) && !out.contains(RIDDEN_BODY),
            "F2.6 leaked the value riding the hit line (format {format:?}): {out}"
        );
    }
}
