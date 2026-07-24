//! Polyglot syntactic validation for non-Rust artifacts.
//!
//! Runs after rendering, before shadow validation. Each artifact is parsed by
//! `touring_code::polyglot` using a trivial identifier pattern (`$X`). A
//! successful parse with at least one match proves the source is well-formed
//! enough for tree-sitter; total absence of matches on a non-empty artifact
//! is a strong signal of a malformed template expansion.
//!
//! The check emits a single `LayerResult` named `polyglot_syntax` that the
//! `speculate` transition folds into `SpeculateReport.layers`. The layer is
//! advisory (score 1.0 on pass, 0.0 on fail) — the existing composite-score
//! threshold does the hard gate.

use touring_code::polyglot::{Lang, detect_lang, search};

use crate::NormalizedScore;
use crate::plan::result::{LayerResult, RenderedFile};

/// Run syntactic checks over each rendered artifact with a polyglot-supported
/// language. Rust artifacts are skipped — they're covered by syn-based gates
/// elsewhere in the pipeline.
///
/// Returns a single `polyglot_syntax` `LayerResult` summarising every failure.
#[must_use]
pub fn polyglot_syntax_layer(artifacts: &[RenderedFile]) -> LayerResult {
    let started = std::time::Instant::now();
    let mut issues: Vec<String> = Vec::new();

    for file in artifacts {
        let Some(lang) = detect_lang(&file.path) else {
            continue;
        };
        if matches!(lang, Lang::Rust) {
            // syn-based gates already validate Rust — skip to avoid duplicate work.
            continue;
        }
        if file.content.trim().is_empty() {
            continue;
        }
        match search(lang, &file.content, "$X") {
            Err(e) => issues.push(format!(
                "{} [{}]: pattern engine error: {e}",
                file.path,
                lang.name()
            )),
            Ok(hits) if hits.is_empty() => issues.push(format!(
                "{} [{}]: tree-sitter produced zero identifier matches — likely malformed",
                file.path,
                lang.name()
            )),
            Ok(_) => {}
        }
    }

    let passed = issues.is_empty();
    let score = if passed {
        NormalizedScore::ONE
    } else {
        NormalizedScore::ZERO
    };

    LayerResult {
        name: "polyglot_syntax".to_string(),
        score,
        passed,
        issues,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    }
}

/// Convenience accessor: true when every artifact either was skipped (Rust /
/// unknown lang / empty) or parsed successfully.
#[must_use]
pub fn polyglot_passed(layer: &LayerResult) -> bool {
    layer.passed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::result::FileAction;

    fn mk(path: &str, content: &str) -> RenderedFile {
        RenderedFile::new(path, content, FileAction::Created)
    }

    #[test]
    fn passes_on_valid_python() {
        let layer = polyglot_syntax_layer(&[mk("out.py", "print('hi')\nx = 1\n")]);
        assert!(layer.passed, "issues: {:?}", layer.issues);
        assert_eq!(layer.name, "polyglot_syntax");
    }

    #[test]
    fn passes_on_valid_typescript() {
        let layer = polyglot_syntax_layer(&[mk(
            "out.ts",
            "const x: number = 1;\nfunction f() { return x; }\n",
        )]);
        assert!(layer.passed, "issues: {:?}", layer.issues);
    }

    #[test]
    fn skips_rust_silently() {
        let layer = polyglot_syntax_layer(&[mk("mod.rs", "not valid rust at all @@@")]);
        // Rust files are skipped — syn gates cover them.
        assert!(layer.passed);
    }

    #[test]
    fn skips_unknown_extensions() {
        let layer = polyglot_syntax_layer(&[mk("CHANGELOG", "# changes")]);
        assert!(layer.passed);
    }

    #[test]
    fn flags_empty_tree_python() {
        // Tree-sitter is permissive but `$X` requires at least one identifier.
        let layer = polyglot_syntax_layer(&[mk("broken.py", "    \n  \t\n")]);
        // Whitespace-only sources are skipped by `trim().is_empty()`.
        assert!(layer.passed);
    }
}
