//! Wave 4B + 4C smoke tests — validates every THSF Fase 4 component
//! can be loaded and invoked through the host runner.
//!
//! Pre-req: both the runner (host target) and all three components
//! (`--target wasm32-wasip2`) must already be built in release mode.
//! The test driver does NOT invoke cargo — run `scripts/build-wave4.sh`
//! or build each crate individually.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("runner crate has a parent dir")
        .to_path_buf()
}

fn runner_bin() -> PathBuf {
    workspace_root().join("target").join("release").join("holon-wasm-runner")
}

fn component_wasm(stem: &str) -> PathBuf {
    workspace_root()
        .join("target")
        .join("wasm32-wasip2")
        .join("release")
        .join(format!("{stem}.wasm"))
}

fn skip_if_missing(runner: &PathBuf, wasm: &PathBuf) -> bool {
    if !runner.exists() || !wasm.exists() {
        eprintln!(
            "SKIP: runner or component missing ({} / {})",
            runner.display(),
            wasm.display()
        );
        return true;
    }
    false
}

fn run_component(wasm: &PathBuf, argv: &[&str]) -> (bool, String, String) {
    let runner = runner_bin();
    let mut args: Vec<String> = vec![wasm.to_string_lossy().into_owned()];
    args.extend(argv.iter().map(|s| (*s).to_string()));
    let out = Command::new(&runner).args(&args).output().expect("spawn runner");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

// ===========================================================================
// spec-version (Wave 4B)
// ===========================================================================

#[test]
fn spec_version_list_capabilities() {
    let runner = runner_bin();
    let wasm = component_wasm("holon_spec_version");
    if skip_if_missing(&runner, &wasm) {
        return;
    }
    let (ok, stdout, stderr) = run_component(&wasm, &["list"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "[\"spec-version\"]", "got {stdout}");
}

#[test]
fn spec_version_invoke_returns_version_bytes() {
    let runner = runner_bin();
    let wasm = component_wasm("holon_spec_version");
    if skip_if_missing(&runner, &wasm) {
        return;
    }
    let (ok, stdout, stderr) = run_component(&wasm, &["invoke", "spec-version", "{}"]);
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("\"exit-code\":0"), "stdout: {stdout}");
    // ASCII bytes of "0.1.0"
    assert!(stdout.contains("48,46,49,46,48"), "version bytes missing: {stdout}");
}

#[test]
fn spec_version_invoke_unknown_capability() {
    let runner = runner_bin();
    let wasm = component_wasm("holon_spec_version");
    if skip_if_missing(&runner, &wasm) {
        return;
    }
    let (ok, stdout, _) = run_component(&wasm, &["invoke", "does-not-exist", "{}"]);
    assert!(ok);
    assert!(
        stdout.contains("unknown-capability") && stdout.contains("does-not-exist"),
        "stdout: {stdout}"
    );
}

// ===========================================================================
// blast-radius (Wave 4C)
// ===========================================================================

#[test]
fn blast_radius_transitive_dependents() {
    let runner = runner_bin();
    let wasm = component_wasm("holon_blast_radius");
    if skip_if_missing(&runner, &wasm) {
        return;
    }
    // Reverse adjacency: key = file, value = its direct dependents.
    // c.rs <- a.rs <- d.rs and c.rs <- b.rs <- d.rs
    // blast_radius(c.rs) == 3 (a, b, d)
    let args = r#"{"graph":{"c.rs":["a.rs","b.rs"],"a.rs":["d.rs"],"b.rs":["d.rs"],"d.rs":[]},"target":"c.rs"}"#;
    let (ok, stdout, stderr) = run_component(&wasm, &["invoke", "blast-radius", args]);
    assert!(ok, "stderr: {stderr}");
    // stdout is an array of byte values — look for "\"blast_radius\":3"
    // after decoding.  Easiest: look for the byte sequence of the substring.
    let needle: Vec<String> = b"\"blast_radius\":3".iter().map(|b| b.to_string()).collect();
    let encoded = needle.join(",");
    assert!(stdout.contains(&encoded), "expected 3 dependents in stdout bytes: {stdout}");
}

#[test]
fn blast_radius_leaf_target() {
    let runner = runner_bin();
    let wasm = component_wasm("holon_blast_radius");
    if skip_if_missing(&runner, &wasm) {
        return;
    }
    // d.rs has no dependents -> blast_radius == 0.
    let args = r#"{"graph":{"d.rs":[]},"target":"d.rs"}"#;
    let (ok, stdout, stderr) = run_component(&wasm, &["invoke", "blast-radius", args]);
    assert!(ok, "stderr: {stderr}");
    let needle: Vec<String> = b"\"blast_radius\":0".iter().map(|b| b.to_string()).collect();
    let encoded = needle.join(",");
    assert!(stdout.contains(&encoded), "expected 0 dependents: {stdout}");
}

// ===========================================================================
// quality-gate (Wave 4C)
// ===========================================================================

#[test]
fn quality_gate_detects_rust_antipatterns() {
    let runner = runner_bin();
    let wasm = component_wasm("holon_quality_gate");
    if skip_if_missing(&runner, &wasm) {
        return;
    }
    // Source with 3 antipatterns across 2 lines: unwrap + panic! + todo!.
    let args = r#"{"source":"fn a(){ opt.unwrap(); panic!(\"x\"); }\nfn b(){ todo!() }","lang":"rust"}"#;
    let (ok, stdout, stderr) = run_component(&wasm, &["invoke", "quality-gate", args]);
    assert!(ok, "stderr: {stderr}");
    // Look for "\"total_antipatterns\":3" in the stdout byte array.
    let needle: Vec<String> = b"\"total_antipatterns\":3".iter().map(|b| b.to_string()).collect();
    let encoded = needle.join(",");
    assert!(stdout.contains(&encoded), "expected total_antipatterns=3: {stdout}");
}

#[test]
fn quality_gate_perfect_score_on_clean_source() {
    let runner = runner_bin();
    let wasm = component_wasm("holon_quality_gate");
    if skip_if_missing(&runner, &wasm) {
        return;
    }
    let args = r#"{"source":"fn clean() -> i32 { 42 }","lang":"rust"}"#;
    let (ok, stdout, stderr) = run_component(&wasm, &["invoke", "quality-gate", args]);
    assert!(ok, "stderr: {stderr}");
    // Score 1.0 (no antipatterns) -> bytes of "\"score\":1" present.
    let needle: Vec<String> = b"\"score\":1".iter().map(|b| b.to_string()).collect();
    let encoded = needle.join(",");
    assert!(stdout.contains(&encoded), "expected score:1 on clean code: {stdout}");
}
