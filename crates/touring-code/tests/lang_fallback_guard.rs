//! Class guard: no handler may answer for a path whose language the parser does not read by
//! pretending it is one — `Lang::from_path(p).unwrap_or(Lang::Rust)` (cross-audit 12/11/2026,
//! F2, F10, F11).
//!
//! `Lang::from_path` returns `None` for an extension the parser does not read. The fallback to
//! `Lang::Rust` turned that `None` into a plausible answer: a `.xyz` file "analysed" as Rust, a
//! `.mts` module judged on a syntax it does not have, a rename previewed over text that was never
//! parsed as what it is. It was fixed site by site (shadow-validate, four AST handlers, three
//! semantics handlers, `touring ast importance`); this test keeps the next one out: it scans
//! `crates/*/src` and fails on any `unwrap_or(Lang::X)`, `unwrap_or_else(|| Lang::X)` or
//! `map_or(Lang::X, ..)` that is not in [`ALLOWED`] with a written reason.

use std::path::{Path, PathBuf};

/// `(path suffix, why the fallback is right THERE)`. A fallback is justified only where the
/// language is already known to be one the parser reads (the `None` is unreachable) — and then
/// the code should not need a fallback at all, so prefer fixing the site. Empty on purpose.
const ALLOWED: &[(&str, &str)] = &[];

/// The call shapes that turn a missing language into a chosen one.
const OPENERS: &[&str] = &["unwrap_or(", "unwrap_or_else(", "map_or(", "map_or_else("];

/// 1-based numbers of the lines of `text` that hand a `Lang::<Variant>` to a fallback call.
/// Comment lines are skipped; `Lang::from_str(..)` and friends (lowercase) are not a variant.
fn lang_fallbacks(text: &str) -> Vec<usize> {
    let mut found = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let code = line.trim_start();
        if code.starts_with("//") {
            continue;
        }
        let hit = OPENERS.iter().any(|opener| {
            code.match_indices(opener).any(|(at, _)| {
                let tail = code[at + opener.len()..].trim_start();
                let tail = tail.strip_prefix("||").unwrap_or(tail).trim_start();
                // the leading path: `Lang::Rust`, `touring_code::ast::languages::Lang::Rust`
                let path: String = tail
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                    .collect();
                path.rsplit_once("Lang::").is_some_and(|(before, variant)| {
                    (before.is_empty() || before.ends_with("::"))
                        && variant.chars().next().is_some_and(char::is_uppercase)
                })
            })
        });
        if hit {
            found.push(idx + 1);
        }
    }
    found
}

fn rust_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if path.file_name().is_some_and(|n| n != "target") {
                rust_files_under(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every `crates/<crate>/src/**/*.rs` of the workspace.
fn workspace_sources() -> Vec<PathBuf> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(&crates) else {
        return files;
    };
    for entry in entries.flatten() {
        rust_files_under(&entry.path().join("src"), &mut files);
    }
    files.sort();
    files
}

/// The detector detects: every shape it promises to catch, and the ones it must not.
#[test]
fn the_detector_flags_the_fallback_shapes_and_nothing_else() {
    for flagged in [
        "let l = Lang::from_path(p).unwrap_or(Lang::Rust);",
        "    .unwrap_or(touring_code::ast::languages::Lang::Rust);",
        "let l = Lang::from_path(p).unwrap_or_else(|| Lang::Python);",
        "let l = Lang::from_path(p).map_or(Lang::Rust, |l| l);",
        "let l = x.unwrap_or( Lang::TypeScript );",
    ] {
        assert_eq!(lang_fallbacks(flagged), vec![1], "must flag: {flagged}");
    }
    for clean in [
        "// let l = Lang::from_path(p).unwrap_or(Lang::Rust);",
        "let l = Lang::from_path(p).unwrap_or(Lang::from_str(\"rust\"));",
        "let n = x.unwrap_or(0);",
        "Lang::Rust => handle(),",
        "let l = x.unwrap_or(MyLang::Rust);",
        "let l = x.ok_or_else(|| anyhow::anyhow!(\"unsupported\"))?;",
    ] {
        assert!(lang_fallbacks(clean).is_empty(), "must not flag: {clean}");
    }
}

#[test]
fn no_handler_falls_back_to_a_language_the_path_does_not_name() {
    let sources = workspace_sources();
    assert!(
        sources.len() > 500,
        "the scan must see the workspace (found {} files): {:?}",
        sources.len(),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
    );
    let mut offenders = Vec::new();
    for path in &sources {
        let rel = path.to_string_lossy().replace('\\', "/");
        if ALLOWED.iter().any(|(suffix, _)| rel.ends_with(suffix)) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in lang_fallbacks(&text) {
            offenders.push(format!(
                "{}:{line}",
                rel.rsplit("crates/").next().unwrap_or(&rel)
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "a `None` from `Lang::from_path` must become an explicit error (or a skip), never a chosen \
         language — see `lang_or_error` in touring-cli's shared module; fallbacks at: {offenders:#?}"
    );
}

/// Every allowed entry still points at a fallback that exists and says why.
#[test]
fn the_allow_list_is_not_stale() {
    for (suffix, why) in ALLOWED {
        assert!(
            why.len() > 40,
            "`{suffix}` needs a real reason, not `{why}`"
        );
        let hit = workspace_sources().into_iter().any(|p| {
            p.to_string_lossy().ends_with(suffix)
                && std::fs::read_to_string(&p).is_ok_and(|t| !lang_fallbacks(&t).is_empty())
        });
        assert!(hit, "`{suffix}` no longer has a fallback: delete its row");
    }
}
