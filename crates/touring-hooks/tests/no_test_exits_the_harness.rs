//! S216 guard (2026-09-25): no test may CALL a hook's diverging `run`.
//!
//! Every hook handler's `run(…)` is `run_returning(…).emit()`, and `emit()` ends in
//! `process::exit(0)` — even for `Allow`. Called from a test, it ends the whole test
//! binary with status 0: libtest never prints `test result`, cargo reports success,
//! and every test not yet run disappears from the verdict. `integration_e2e` announced
//! 33 tests and ran 9 (serial) — three `post_bash::run` calls had never executed a
//! single assertion after them. Tests call `run_returning` instead.
//!
//! A source scan (not a behavioural test) because the failure hides itself — the same
//! reasoning as the S13 guard `touring-dispatch/tests/daemon_table_never_exits.rs`.
//! A CALL passes the runtime by reference (`::run(&…`); a mention in a string or a doc
//! comment does not, which is what separates the two.

use std::path::{Path, PathBuf};

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .to_path_buf()
}

/// Handler modules whose `pub fn run(` body reaches `.emit()` (diverges).
fn diverging_handlers() -> Vec<String> {
    let dir = crates_dir().join("touring-hook-handlers/src/hooks");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("hooks dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("handler source");
        if let Some(at) = src.find("pub fn run(") {
            let body = &src[at..(at + 400).min(src.len())];
            if body.contains(".emit()") {
                out.push(
                    path.file_stem()
                        .expect("stem")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    out.sort();
    out
}

/// Every CALL `<module>::run(&…` in `text` (word-bounded; mentions excluded).
fn diverging_calls(text: &str, modules: &[String]) -> Vec<(usize, String)> {
    let mut hits = Vec::new();
    for (n, line) in text.lines().enumerate() {
        for m in modules {
            let needle = format!("{m}::run(");
            let mut from = 0;
            while let Some(i) = line[from..].find(&needle) {
                let start = from + i;
                let bounded = line[..start]
                    .chars()
                    .last()
                    .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                let is_call = line[start + needle.len()..].trim_start().starts_with('&');
                if bounded && is_call {
                    hits.push((n + 1, line.trim().to_string()));
                }
                from = start + needle.len();
            }
        }
    }
    hits
}

fn test_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if name != "target" {
                test_sources(&path, out);
            }
        } else if name.ends_with(".rs")
            && (path.components().any(|c| c.as_os_str() == "tests") || name.ends_with("_tests.rs"))
        {
            out.push(path);
        }
    }
}

#[test]
fn no_test_calls_a_diverging_hook_run() {
    let modules = diverging_handlers();
    // Positive control: the detector sees the handlers known to diverge.
    for known in ["pre_bash", "post_bash", "post_write"] {
        assert!(
            modules.iter().any(|m| m == known),
            "detector blind: {known} not seen as diverging ({modules:?})"
        );
    }
    // Positive/negative control of the call matcher on literal fixtures.
    assert_eq!(
        diverging_calls(
            "let r = touring_hooks::post_bash::run(&mut rt, &input);",
            &modules
        )
        .len(),
        1
    );
    assert!(diverging_calls("must NOT call crate::post_bash::run(rt, v)", &modules).is_empty());
    assert!(diverging_calls("\"post_bash::run(\",", &modules).is_empty());

    let mut files = Vec::new();
    test_sources(&crates_dir(), &mut files);
    assert!(files.len() > 50, "scanned only {} test files", files.len());
    let mut offenders = Vec::new();
    // This guard's own positive-control fixture is a call by construction.
    for file in files.iter().filter(|f| !f.ends_with(file!())) {
        let text = std::fs::read_to_string(file).expect("test source");
        for (line, code) in diverging_calls(&text, &modules) {
            offenders.push(format!("{}:{line}: {code}", file.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "tests calling a diverging hook `run` (ends the test binary with exit 0 — use run_returning):\n{}",
        offenders.join("\n")
    );
}
