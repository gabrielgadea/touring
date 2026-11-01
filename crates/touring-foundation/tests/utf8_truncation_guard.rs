//! Structural guard: no source file truncates a string by raw byte index.
//!
//! Origin — 2026-08-19. The `touring-project-actor` of the `analise` daemon
//! died five times with:
//!
//! ```text
//! thread 'touring-project-actor' panicked at hook_registry.rs:947:59:
//! end byte index 60 is not a char boundary; it is inside 'ê' (bytes 59..61)
//! ```
//!
//! `&s[..s.len().min(60)]` is safe only while `s` is ASCII. Every project in
//! this workspace's fleet is written in Portuguese, so "while `s` is ASCII" is
//! a coin flip on the content of a task subject. A sweep found **236** sites of
//! the identical shape across 31 files — and, separately, six correct
//! implementations of safe truncation already living in the tree, none of them
//! reached by those 236 sites. That is the recurring failure mode: one concept,
//! N implementations, and the lesson learned in only one of them.
//!
//! [`touring_foundation::truncate_str`] is the canonical one. This guard makes
//! it the ONLY one on the byte-index path, so the class cannot come back
//! through the next file nobody swept.

use std::fs;
use std::path::{Path, PathBuf};

fn crates_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .to_path_buf()
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // `target/` is build output, not source under contract.
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// A byte-index truncation of the shape `&x[..x.len().min(N)]`, which panics
/// whenever byte `N` lands inside a multi-byte character.
fn byte_truncations(source: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for (lineno, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        // Prose about the pattern is not the pattern.
        if trimmed.starts_with("//") {
            continue;
        }
        let Some(open) = line.find("[..") else {
            continue;
        };
        // Inside a string literal it is PROSE about the pattern, not the
        // pattern. `pre_edit.rs` carries the antipattern message verbatim —
        // the hook that warns everyone else about this exact defect.
        if line[..open].matches('"').count() % 2 == 1 {
            continue;
        }
        let rest = &line[open..];
        // `.join(` after the slice means a collection of strings, not a string:
        // slicing a `Vec` by its own length cannot land inside a character.
        if rest
            .find(']')
            .is_some_and(|close| rest[close..].trim_start_matches(']').starts_with(".join("))
        {
            continue;
        }
        if !rest.contains(".len().min(") {
            continue;
        }
        // The variable named before `[..` must be the one measured inside it —
        // that is what makes it a self-truncation rather than a slice of some
        // other collection (`&all[..other.len().min(3)]` is a different shape,
        // and slicing a `Vec` cannot panic on a char boundary at all).
        let head = &line[..open];
        let Some(var) = head
            .rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
            .next()
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        if rest.contains(&format!("{var}.len().min(")) {
            hits.push(format!("{}: {}", lineno + 1, line.trim()));
        }
    }
    hits
}

#[test]
fn no_source_truncates_a_string_by_raw_byte_index() {
    let mut files = Vec::new();
    rust_sources(&crates_dir(), &mut files);
    assert!(
        files.len() > 500,
        "the sweep found only {} files — it is not reaching the tree",
        files.len()
    );

    let mut offenders: Vec<String> = Vec::new();
    for file in &files {
        // This guard describes the pattern in its own prose and helper.
        if file.ends_with("utf8_truncation_guard.rs") {
            continue;
        }
        let Ok(source) = fs::read_to_string(file) else {
            continue;
        };
        for hit in byte_truncations(&source) {
            offenders.push(format!("{}:{hit}", file.display()));
        }
    }

    assert!(
        offenders.is_empty(),
        "{} raw byte-index truncation(s) — each panics on the first accented \
         character that lands on the cut. Use `touring_foundation::truncate_str(&s, N)`, \
         which backs up to a char boundary and is a drop-in `&str -> &str`:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

/// The guard is only worth its runtime if it can still SEE the pattern. A
/// detector that silently stopped matching would report a clean tree forever —
/// the failure mode that makes a guard worse than no guard, because it also
/// removes the suspicion.
#[test]
fn the_detector_still_recognizes_the_pattern_it_guards_against() {
    let offending = r#"
        fn f(subject: &str) -> &str {
            let short = &subject[..subject.len().min(60)];
            short
        }
    "#;
    assert_eq!(
        byte_truncations(offending).len(),
        1,
        "the detector must still match the exact shape that panicked in production"
    );

    let fixed = r#"
        fn f(subject: &str) -> &str {
            let short = truncate_str(&subject, 60);
            short
        }
    "#;
    assert!(
        byte_truncations(fixed).is_empty(),
        "the canonical fix must read as clean"
    );

    // Two shapes that LOOK like the pattern and are not it. Both were live in
    // the tree when this guard was written, and a guard that cries wolf on them
    // gets suppressed — which is how a guard stops guarding.
    let collection_slice = r#"
        fn f(names: &[String]) -> String {
            names[..names.len().min(4)].join(", ")
        }
    "#;
    assert!(
        byte_truncations(collection_slice).is_empty(),
        "slicing a collection cannot land inside a character — `.join(` is the tell"
    );

    let prose_about_the_pattern = r#"
        fn advice() -> String {
            "RUST ANTIPATTERN: &s[..s.len().min(N)] can panic".to_string()
        }
    "#;
    assert!(
        byte_truncations(prose_about_the_pattern).is_empty(),
        "the hook that warns about this defect must not be reported as committing it"
    );

    // The shape WITHOUT a leading `&`, which the first sweep's regex missed and
    // this guard caught — six real defects, in files nobody had swept.
    let no_leading_ampersand = r#"
        fn f(subject: &str) -> String {
            subject[..subject.len().min(120)].to_string()
        }
    "#;
    assert_eq!(
        byte_truncations(no_leading_ampersand).len(),
        1,
        "the borrow is incidental; the truncation is the defect"
    );
}
