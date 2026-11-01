//! F4.3 — Deprecated APIs verifier.
//!
//! W0 (2026-06-21): corrected measure. The MVP stub counted `#[deprecated`
//! *declarations* and penalised them — inverted. Declaring a deprecation on
//! your own item is good hygiene (D42); the real defect is *consuming* a
//! deprecated API and silencing the compiler. We now score that instead.

use crate::DimId;
use crate::verifications::{Verification, lang_of, strip_rust_comments_and_strings};
use anyhow::Result;
use std::path::Path;

/// F4.3 verifier — Deprecated APIs.
#[allow(non_camel_case_types)]
pub struct F4_3_Deprecated;

impl Verification for F4_3_Deprecated {
    fn id(&self) -> DimId {
        DimId::F4_3
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        let raw = crate::verifications::read_target_source(target)?;

        // Dispatch to language-specific detector.
        if lang_of(target) == "python" {
            return detect_python_deprecated(&raw);
        }

        // F4.3 measures CONSUMPTION of deprecated APIs, not their declaration.
        // Declaring `#[deprecated(..)]` on your own item is *good* hygiene (D42):
        // it hands consumers a migration path, so it is NOT penalised. The real
        // defect is *using* a deprecated API and silencing the compiler — visible
        // at file scope as the lint-suppression attribute. Needles are split via
        // `concat!` so this verifier never matches its own source text.
        //
        // 2026-06-26: strip comments and string literals first so docs that
        // *mention* `#[allow(deprecated)]` in a doc comment are not flagged as
        // suppressions. Same pattern as f1_8_dep_cycles.
        let code_only = strip_rust_comments_and_strings(&raw);
        let supp_needle = concat!("allow(", "deprecated");
        let decl_needle = concat!("#[", "deprecated");
        let suppressions = code_only.matches(supp_needle).count();
        let declarations = code_only.matches(decl_needle).count();

        let value = if suppressions == 0 {
            1.0
        } else {
            (1.0 - suppressions as f32 * 0.25).max(0.0)
        };

        let evidence = format!(
            "Deprecated-API suppressions={suppressions} (penalised), own deprecation declarations={declarations} (good hygiene, not penalised); score={value:.3}"
        );
        Ok((value, evidence))
    }
}

/// Python deprecated-stdlib catalog (PEP 594 / Python 3.10+).
///
/// Needles are built at runtime via concatenation so the source of this
/// verifier never embeds a contiguous deprecated-module name that would
/// trigger a self-match.
///
/// Catalog:
/// - `imp` — removed in Python 3.12 (use `importlib`)
/// - `distutils` — removed in Python 3.12 (use `setuptools`)
/// - `from collections import <ABC>` — ABCs moved to `collections.abc` in 3.10
/// - `.utcnow()` — deprecated since Python 3.12
/// - `asyncio.coroutine` — removed in Python 3.11
fn detect_python_deprecated(source: &str) -> anyhow::Result<(f32, String)> {
    let mut hits: Vec<&str> = Vec::new();

    // Build needles at runtime.
    let imp = "im".to_owned() + "p";
    let distutils = "dist".to_owned() + "utils";
    let utcnow = ".utcnow()";
    let asyncio_coro = "asyncio".to_owned() + ".coroutine";
    let abc_names = [
        "Mapping",
        "MutableMapping",
        "Sequence",
        "MutableSequence",
        "Set",
        "MutableSet",
        "Callable",
        "Iterator",
        "Generator",
        "Coroutine",
        "AsyncGenerator",
        "AsyncIterator",
        "AsyncIterable",
        "Iterable",
    ];

    let imports = python_imports(source);
    if imports.iter().any(|i| names_module(&i.module, &imp)) {
        hits.push("import imp (removed in 3.12, use importlib)");
    }
    if imports.iter().any(|i| names_module(&i.module, &distutils)) {
        hits.push("from distutils (removed in 3.12, use setuptools)");
    }
    // `from collections import Mapping` etc — only the names of THAT statement count.
    if imports.iter().any(|i| {
        i.module == "collections" && i.names.iter().any(|n| abc_names.contains(&n.as_str()))
    }) {
        hits.push("from collections import <ABC> (moved to collections.abc in 3.10)");
    }
    if source.contains(utcnow) {
        hits.push(".utcnow() (deprecated in 3.12, use datetime.now(timezone.utc))");
    }
    if source.contains(asyncio_coro.as_str()) {
        hits.push("asyncio.coroutine (removed in 3.11, use async def)");
    }

    let n = hits.len();
    let value = if n == 0 {
        1.0_f32
    } else {
        (1.0 - n as f32 * 0.3).max(0.0)
    };
    let evidence = if n == 0 {
        "F4.3 (Python): no deprecated-stdlib imports detected — score=1.000".to_string()
    } else {
        format!(
            "F4.3 (Python): {} deprecated-stdlib hit(s): {} — score={value:.3}",
            n,
            hits.join("; ")
        )
    };
    Ok((value, evidence))
}

/// One Python import statement: the module it names and, for `from … import`, the names it pulls.
struct PyImport {
    module: String,
    names: Vec<String>,
}

/// `module` is `root` or one of its submodules — a whole dotted name, never a prefix
/// (`importlib` is not `imp`; `distutils.core` is `distutils`).
fn names_module(module: &str, root: &str) -> bool {
    module == root
        || module
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// The code of one line: the part before a `#` comment, trimmed.
fn code_of(line: &str) -> &str {
    line.split('#').next().unwrap_or("").trim()
}

/// Every import statement of `source`, parsed at statement level. Parenthesised and backslash
/// continuations are joined, so each name belongs to its own statement. 2026-09-26: both needles
/// were substring matches over the whole rest of the file — `import importlib` read as the
/// removed module, and a correct `collections.abc` import two lines below tainted a plain
/// `collections` import (a P0 BLOCK at 0.4 on a file with zero deprecated imports), while
/// `import os, imp` went unseen.
fn python_imports(source: &str) -> Vec<PyImport> {
    let mut out = Vec::new();
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        let mut stmt = code_of(line).to_string();
        if !(stmt.starts_with("import ") || stmt.starts_with("from ")) {
            continue;
        }
        while stmt.ends_with('\\') || stmt.matches('(').count() > stmt.matches(')').count() {
            let Some(next) = lines.next() else { break };
            stmt = format!("{} {}", stmt.trim_end_matches('\\'), code_of(next));
        }
        let clean = stmt.replace(['(', ')', '\\'], " ");
        let first_word = |part: &str| part.split_whitespace().next().map(str::to_string);
        if let Some(rest) = clean.strip_prefix("import ") {
            out.extend(
                rest.split(',')
                    .filter_map(first_word)
                    .map(|module| PyImport {
                        module,
                        names: Vec::new(),
                    }),
            );
        } else if let Some((module, names)) = clean
            .strip_prefix("from ")
            .and_then(|rest| rest.split_once(" import "))
        {
            out.push(PyImport {
                module: module.trim().to_string(),
                names: names.split(',').filter_map(first_word).collect(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_deprecated_returns_valid_score() {
        let f = write_temp("fn example() {}\n");
        let s = F4_3_Deprecated.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_deprecated_empty_file() {
        let f = write_temp("");
        let s = F4_3_Deprecated.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// W0 fixture: DECLARING a deprecation is good hygiene → must score 1.0.
    /// (The old stub inverted this and returned 0.3.) Trigger built via
    /// `concat!` so this test's own source never carries the contiguous needle.
    #[test]
    fn test_declaring_deprecation_is_not_penalised() {
        let content = concat!(
            "#[",
            "deprecated(note = \"use bar\")]\n",
            "pub fn foo() {}\n"
        );
        let f = write_temp(content);
        let s = F4_3_Deprecated.check(f.path()).expect("check");
        assert_eq!(
            s.value, 1.0,
            "declaring #[deprecated] must NOT be penalised"
        );
    }

    /// W0 fixture: CONSUMING a deprecated API (suppression attr) is the real
    /// defect → must score below 1.0.
    #[test]
    fn test_suppressing_deprecation_warning_is_penalised() {
        let content = concat!("#[", "allow(", "deprecated)]\n", "fn uses_old() {}\n");
        let f = write_temp(content);
        let s = F4_3_Deprecated.check(f.path()).expect("check");
        assert!(
            s.value < 1.0,
            "suppressing the deprecation lint must be penalised, got {}",
            s.value
        );
    }

    fn python_score(source: &str) -> f32 {
        let mut f = tempfile::Builder::new()
            .suffix(".py")
            .tempfile()
            .expect("create temp .py");
        f.write_all(source.as_bytes()).expect("write");
        F4_3_Deprecated.check(f.path()).expect("check").value
    }

    /// 2026-09-26: a substring needle condemned `importlib` as the removed module — the
    /// replacement the evidence itself recommends. A module is a whole name, never a prefix.
    #[test]
    fn importlib_is_not_the_removed_module() {
        let src = concat!(
            "import ",
            "importlib\n",
            "from ",
            "importlib import import_module\n"
        );
        assert_eq!(python_score(src), 1.0);
    }

    /// 2026-09-26: the ABC names were searched in the WHOLE rest of the file, so a correct
    /// `collections.abc` import two lines below tainted a plain `collections` import (BLOCK 0.4
    /// on a file with zero deprecated imports). The names belong to their own statement.
    #[test]
    fn collections_abc_below_does_not_taint_a_plain_collections_import() {
        let src = concat!(
            "from collections import Counter, defaultdict\n",
            "from collections.abc import Callable, Iterable\n",
            "def f(x: Iterable[int]) -> Callable: ...\n"
        );
        assert_eq!(python_score(src), 1.0);
    }

    /// Positive controls: the fix must keep catching every real form.
    #[test]
    fn the_removed_module_is_still_caught_in_every_import_form() {
        for src in [
            concat!("import ", "imp\n"),
            concat!("import os, ", "imp as legacy\n"),
            concat!("from ", "imp import reload\n"),
            concat!("from ", "distutils.core import setup\n"),
        ] {
            assert!(python_score(src) < 1.0, "not caught: {src:?}");
        }
    }

    #[test]
    fn an_abc_from_plain_collections_is_caught_even_parenthesised_or_later() {
        for src in [
            "from collections import Mapping\n",
            "from collections import (\n    OrderedDict,\n    Mapping,\n)\n",
            "from collections import Counter\nx = 1\nfrom collections import Iterable\n",
        ] {
            assert!(python_score(src) < 1.0, "not caught: {src:?}");
        }
    }
}
