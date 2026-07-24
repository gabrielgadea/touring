//! Modernization (D43 / F4.4) — polyglot adoption of newer language/edition
//! features replacing older equivalents.
//!
//! D43 asks whether code uses a **newer language version/edition** construct in
//! place of a superseded one — distinct from [`super::idioms`] (F4.1), which
//! scores the idiomatic *style* of a construct that exists in every version.
//! Modernization is **version-anchored**: `try!`(2015)→`?`(2018),
//! `lazy_static!`→`std::sync::LazyLock`(1.80), `super(Cls, self)`→`super()`(Py3),
//! `require`→ESM `import`, `ioutil`(Go<1.16)→`io`/`os`, anonymous functional
//! class→lambda(Java 8), `<stdio.h>`→`<cstdio>`. Signals already covered by F4.1
//! (`var`, `interface{}`, `typedef`, `NULL`, `using namespace std`) are *not*
//! repeated here, to keep the two dimensions disjoint.
//!
//! | Lang | Era | Modernizations detected |
//! |------|-----|-------------------------|
//! | Rust | edition 2018 / 1.80 | `try!(`→`?`, `extern crate X`→paths (sysroot crates allowlisted), `#[macro_use]`→`use`, `lazy_static!`→`LazyLock`/`OnceLock` |
//! | Python | Py2→Py3 | `super(Cls, self)`→`super()`, `(object):` redundant base |
//! | TypeScript / JavaScript | ESM / ES6+ | `require(`→`import`, `module.exports`→`export`, `Object.assign({...}`→spread, `indexOf(..) !== -1`→`includes()` |
//! | Go | 1.16 / 1.20 | `ioutil.`→`io`/`os` (deprecated), `rand.Seed(` (auto-seeded) |
//! | Java | 8 / 16 | anonymous functional-interface class→lambda, `Collectors.toList()`→`.toList()` |
//! | C++ | C++11+ | `std::bind(`→lambda, C headers `<stdio.h>`→`<cstdio>` |
//!
//! Comments and `#[cfg(test)]`/test regions are excluded via
//! [`super::code_regions`]. A per-file scanner cannot replace `cargo fix
//! --edition` / a codemod, so it catches a high-confidence subset; F4.4 is WARN
//! (advisory). Replaces a stub that counted `try!` + `extern crate` only. Zero
//! non-std deps beyond `memchr`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

/// A pure-substring modernization opportunity: literal `needle`, the
/// suggested modern form (`message`), and a `weight`.
struct ModRule {
    needle: &'static [u8],
    message: &'static str,
    weight: f32,
}

/// Canonical language bucket (collapses extension aliases).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Python,
    TsJs,
    Go,
    Java,
    Cpp,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "python" | "py" => Lang::Python,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" => Lang::TsJs,
        "go" => Lang::Go,
        "java" => Lang::Java,
        "cpp" | "c++" | "cc" | "cxx" | "c" | "h" | "hpp" => Lang::Cpp,
        _ => Lang::Other,
    }
}

// ── Pure-substring needles (structural smells live in analyze_<lang>) ──────────
const RUST_MOD_NEEDLES: &[ModRule] = &[
    ModRule {
        needle: b"try!(",
        message: "`try!(e)` -> the `?` operator (Rust 2018)",
        weight: 0.8,
    },
    ModRule {
        needle: b"lazy_static!",
        message: "`lazy_static!` -> `std::sync::LazyLock` / `OnceLock` (Rust 1.80)",
        weight: 0.7,
    },
    ModRule {
        needle: b"#[macro_use]",
        message: "`#[macro_use]` -> import macros by path with `use` (Rust 2018)",
        weight: 0.4,
    },
];

const PYTHON_MOD_NEEDLES: &[ModRule] = &[ModRule {
    needle: b"(object):",
    message: "`class X(object):` -> drop the redundant `object` base (Python 3)",
    weight: 0.5,
}];

const JSTS_MOD_NEEDLES: &[ModRule] = &[
    ModRule {
        needle: b"require(",
        message: "`require(..)` -> ES module `import` (ESM)",
        weight: 0.4,
    },
    ModRule {
        needle: b"module.exports",
        message: "`module.exports` -> ESM `export` / `export default`",
        weight: 0.4,
    },
    ModRule {
        needle: b"Object.assign({",
        message: "`Object.assign({}, x)` -> object spread `{ ...x }`",
        weight: 0.4,
    },
];

const GO_MOD_NEEDLES: &[ModRule] = &[
    ModRule {
        needle: b"ioutil.",
        message: "`ioutil.*` -> `io`/`os` (deprecated since Go 1.16)",
        weight: 0.7,
    },
    ModRule {
        needle: b"rand.Seed(",
        message: "`rand.Seed(..)` -> auto-seeded top-level rand (deprecated Go 1.20)",
        weight: 0.5,
    },
];

const JAVA_MOD_NEEDLES: &[ModRule] = &[
    ModRule {
        needle: b"new Runnable()",
        message: "anonymous `Runnable` -> a lambda (Java 8)",
        weight: 0.4,
    },
    ModRule {
        needle: b"new Callable<",
        message: "anonymous `Callable` -> a lambda (Java 8)",
        weight: 0.4,
    },
    ModRule {
        needle: b"new Comparator<",
        message: "anonymous `Comparator` -> a lambda / `Comparator.comparing` (Java 8)",
        weight: 0.4,
    },
    ModRule {
        needle: b"new Function<",
        message: "anonymous `Function` -> a lambda (Java 8)",
        weight: 0.4,
    },
    ModRule {
        needle: b"new Supplier<",
        message: "anonymous `Supplier` -> a lambda (Java 8)",
        weight: 0.4,
    },
    ModRule {
        needle: b"new Consumer<",
        message: "anonymous `Consumer` -> a lambda (Java 8)",
        weight: 0.4,
    },
    ModRule {
        needle: b"new Predicate<",
        message: "anonymous `Predicate` -> a lambda (Java 8)",
        weight: 0.4,
    },
    ModRule {
        needle: b"Collectors.toList()",
        message: "`.collect(Collectors.toList())` -> `.toList()` (Java 16)",
        weight: 0.3,
    },
];

const CPP_MOD_NEEDLES: &[ModRule] = &[
    ModRule {
        needle: b"std::bind(",
        message: "`std::bind(..)` -> a lambda (modernize-avoid-bind)",
        weight: 0.4,
    },
    ModRule {
        needle: b"<stdio.h>",
        message: "`<stdio.h>` -> `<cstdio>` (C++ header)",
        weight: 0.4,
    },
    ModRule {
        needle: b"<stdlib.h>",
        message: "`<stdlib.h>` -> `<cstdlib>` (C++ header)",
        weight: 0.4,
    },
    ModRule {
        needle: b"<string.h>",
        message: "`<string.h>` -> `<cstring>` (C++ header)",
        weight: 0.4,
    },
    ModRule {
        needle: b"<math.h>",
        message: "`<math.h>` -> `<cmath>` (C++ header)",
        weight: 0.4,
    },
    ModRule {
        needle: b"<time.h>",
        message: "`<time.h>` -> `<ctime>` (C++ header)",
        weight: 0.4,
    },
];

fn mod_needles_for(lang: Lang) -> &'static [ModRule] {
    match lang {
        Lang::Rust => RUST_MOD_NEEDLES,
        Lang::Python => PYTHON_MOD_NEEDLES,
        Lang::TsJs => JSTS_MOD_NEEDLES,
        Lang::Go => GO_MOD_NEEDLES,
        Lang::Java => JAVA_MOD_NEEDLES,
        Lang::Cpp => CPP_MOD_NEEDLES,
        Lang::Other => &[],
    }
}

/// Per-file modernization analysis (parallel shape to [`super::idioms::IdiomReport`]).
#[derive(Debug, Clone, Default)]
pub struct ModernizationReport {
    /// Total modernization opportunities found in production code.
    pub violations: usize,
    /// Weighted sum (each opportunity scaled by its rule weight).
    pub weighted_total: f32,
    /// Production lines considered (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired rule, for evidence (highest count first).
    pub findings: Vec<(String, usize)>,
}

impl ModernizationReport {
    /// Record `count` occurrences of one modernization with the given `weight`.
    /// A zero count is a no-op.
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count == 0 {
            return;
        }
        self.violations += count;
        self.weighted_total += weight * count as f32;
        self.findings.push((message.to_string(), count));
    }
}

// ── Low-level byte helpers (UTF-8-safe: operate on &[u8]) ──────────────────────

/// Read an ASCII identifier (`[A-Za-z0-9_]`) starting at `start`.
fn read_ident(bytes: &[u8], start: usize) -> &[u8] {
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    &bytes[start..end]
}

/// First index at/after `i` that is not a space or tab.
fn first_nonspace_after(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    i
}

// ── Structural detectors ──────────────────────────────────────────────────────

/// `extern crate X;` for a non-sysroot crate — unnecessary since Rust 2018
/// (use the crate by path). The sysroot crates that *still* legitimately need
/// `extern crate` in `no_std` / proc-macro contexts are allowlisted.
fn rust_extern_crate(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    const SYSROOT: &[&[u8]] = &[b"alloc", b"core", b"test", b"proc_macro", b"std"];
    let mut n = 0;
    for off in memmem::find_iter(bytes, b"extern crate ") {
        if offset_suppressed(off, regions) {
            continue;
        }
        let name = read_ident(bytes, off + b"extern crate ".len());
        if !name.is_empty() && !SYSROOT.contains(&name) {
            n += 1;
        }
    }
    n
}

/// `super(Cls, self)` (Python 2 explicit form) — Python 3 has zero-argument
/// `super()`. Only the *argument-bearing* form is flagged; `super()` is fine.
fn py_super_with_args(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    for off in memmem::find_iter(bytes, b"super(") {
        if offset_suppressed(off, regions) {
            continue;
        }
        // `super` must start an identifier (not `xsuper(`).
        if off > 0 && (bytes[off - 1].is_ascii_alphanumeric() || bytes[off - 1] == b'_') {
            continue;
        }
        let f = first_nonspace_after(bytes, off + b"super(".len());
        if bytes.get(f) != Some(&b')') {
            n += 1;
        }
    }
    n
}

/// `x.indexOf(y) !== -1` / `=== -1` / `< 0` / `>= 0` (membership test) ->
/// `x.includes(y)`. Line-scoped: requires `.indexOf(` and a find-style
/// comparison on the same line.
fn jsts_indexof_includes(source: &str, regions: &[(usize, usize)]) -> usize {
    const CMP: &[&str] = &["!==-1", "===-1", "!=-1", "==-1", ">-1", "<0", ">=0"];
    let mut n = 0;
    let mut offset = 0usize;
    for chunk in source.split_inclusive('\n') {
        let line_off = offset;
        offset += chunk.len();
        if offset_suppressed(line_off, regions) {
            continue;
        }
        if !chunk.contains(".indexOf(") {
            continue;
        }
        let compact: String = chunk.chars().filter(|c| !c.is_whitespace()).collect();
        if CMP.iter().any(|c| compact.contains(c)) {
            n += 1;
        }
    }
    n
}

// ── Per-language assembly ────────────────────────────────────────────────────

fn analyze_rust(bytes: &[u8], regions: &[(usize, usize)], report: &mut ModernizationReport) {
    report.push(
        "`extern crate X;` -> use the crate by path (unnecessary since Rust 2018)",
        rust_extern_crate(bytes, regions),
        0.5,
    );
}

fn analyze_python(bytes: &[u8], regions: &[(usize, usize)], report: &mut ModernizationReport) {
    report.push(
        "`super(Cls, self)` -> zero-argument `super()` (Python 3)",
        py_super_with_args(bytes, regions),
        0.7,
    );
}

fn analyze_jsts(source: &str, regions: &[(usize, usize)], report: &mut ModernizationReport) {
    report.push(
        "`indexOf(..) !== -1` -> `.includes(..)`",
        jsts_indexof_includes(source, regions),
        0.4,
    );
}

/// Analyze modernization opportunities for `lang`. Unknown languages yield an
/// empty report (no model → no findings → score 1.0).
#[must_use]
pub fn analyze_modernization(source: &str, lang: &str) -> ModernizationReport {
    let regions = non_executable_regions(source, lang);
    let bytes = source.as_bytes();
    let mut report = ModernizationReport {
        total_lines: source.lines().count(),
        ..ModernizationReport::default()
    };
    let canon = canonical_lang(lang);

    for rule in mod_needles_for(canon) {
        let count = memmem::find_iter(bytes, rule.needle)
            .filter(|&off| !offset_suppressed(off, &regions))
            .count();
        report.push(rule.message, count, rule.weight);
    }

    match canon {
        Lang::Rust => analyze_rust(bytes, &regions, &mut report),
        Lang::Python => analyze_python(bytes, &regions, &mut report),
        Lang::TsJs => analyze_jsts(source, &regions, &mut report),
        // Go / Java / C++ are needle-only; Other has no model.
        _ => {}
    }

    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// D43 modernization score: `1 - density * SCALE`, where density is the
/// weighted opportunity count per production line. A fully-modern file is `1.0`;
/// legacy-construct debt accumulates linearly. WARN-tier (advisory).
#[must_use]
pub fn score_modernization(report: &ModernizationReport) -> f32 {
    const SCALE: f32 = 6.0;
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Rust ──────────────────────────────────────────────────────────────────
    #[test]
    fn rust_modern_is_clean() {
        let src = "use std::sync::LazyLock;\nfn f() -> Result<(), E> {\n    let x = g()?;\n    Ok(())\n}\n";
        let r = analyze_modernization(src, "rust");
        assert_eq!(r.violations, 0, "modern rust is clean: {:?}", r.findings);
        assert!((score_modernization(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rust_legacy_flagged() {
        let src = "#[macro_use]\nextern crate serde;\nlazy_static! { static ref X: u8 = 1; }\nfn f() -> R { let x = try!(g()); Ok(x) }\n";
        let r = analyze_modernization(src, "rust");
        // try! + lazy_static! + macro_use + extern crate serde
        assert!(r.violations >= 4, "legacy rust must flag: {:?}", r.findings);
    }

    #[test]
    fn rust_sysroot_extern_crate_allowed() {
        // `extern crate alloc;` is still required in no_std and must not flag.
        let r = analyze_modernization("#![no_std]\nextern crate alloc;\n", "rust");
        assert_eq!(
            r.violations, 0,
            "sysroot extern crate is allowed: {:?}",
            r.findings
        );
    }

    // ── Python ────────────────────────────────────────────────────────────────
    #[test]
    fn python_super_with_args_flagged() {
        let bad = analyze_modernization(
            "class C(B):\n    def __init__(self):\n        super(C, self).__init__()\n",
            "python",
        );
        assert!(
            bad.violations >= 1,
            "super(C, self) must flag: {:?}",
            bad.findings
        );
        let ok = analyze_modernization(
            "class C(B):\n    def __init__(self):\n        super().__init__()\n",
            "python",
        );
        assert_eq!(
            ok.violations, 0,
            "zero-arg super() is modern: {:?}",
            ok.findings
        );
    }

    #[test]
    fn python_redundant_object_base_flagged() {
        let bad = analyze_modernization("class C(object):\n    pass\n", "python");
        assert!(
            bad.violations >= 1,
            "(object) base must flag: {:?}",
            bad.findings
        );
        let ok = analyze_modernization("class C:\n    pass\n", "python");
        assert_eq!(
            ok.violations, 0,
            "no explicit base is modern: {:?}",
            ok.findings
        );
    }

    // ── TypeScript / JavaScript ─────────────────────────────────────────────────
    #[test]
    fn jsts_require_and_exports_flagged() {
        let bad = analyze_modernization(
            "const fs = require('fs');\nmodule.exports = fs;\n",
            "javascript",
        );
        assert!(
            bad.violations >= 2,
            "require + module.exports must flag: {:?}",
            bad.findings
        );
        let ok = analyze_modernization("import fs from 'fs';\nexport default fs;\n", "javascript");
        assert_eq!(
            ok.violations, 0,
            "ESM import/export is modern: {:?}",
            ok.findings
        );
    }

    #[test]
    fn jsts_indexof_includes_flagged() {
        let bad = analyze_modernization("if (arr.indexOf(x) !== -1) { use(x); }\n", "typescript");
        assert!(
            bad.violations >= 1,
            "indexOf !== -1 must flag: {:?}",
            bad.findings
        );
        let ok = analyze_modernization("if (arr.includes(x)) { use(x); }\n", "typescript");
        assert_eq!(ok.violations, 0, ".includes() is modern: {:?}", ok.findings);
    }

    // ── Go ──────────────────────────────────────────────────────────────────────
    #[test]
    fn go_ioutil_and_rand_seed_flagged() {
        let bad = analyze_modernization(
            "data, _ := ioutil.ReadFile(p)\nrand.Seed(time.Now().UnixNano())\n",
            "go",
        );
        assert!(
            bad.violations >= 2,
            "ioutil + rand.Seed must flag: {:?}",
            bad.findings
        );
        let ok = analyze_modernization("data, _ := os.ReadFile(p)\n", "go");
        assert_eq!(ok.violations, 0, "os.ReadFile is modern: {:?}", ok.findings);
    }

    // ── Java ──────────────────────────────────────────────────────────────────
    #[test]
    fn java_anon_class_and_collectors_flagged() {
        let bad = analyze_modernization(
            "Runnable r = new Runnable() { public void run() {} };\nList<X> l = s.collect(Collectors.toList());\n",
            "java",
        );
        assert!(
            bad.violations >= 2,
            "anon class + Collectors.toList must flag: {:?}",
            bad.findings
        );
        let ok = analyze_modernization("Runnable r = () -> {};\nList<X> l = s.toList();\n", "java");
        assert_eq!(
            ok.violations, 0,
            "lambda + .toList() is modern: {:?}",
            ok.findings
        );
    }

    // ── C++ ─────────────────────────────────────────────────────────────────────
    #[test]
    fn cpp_bind_and_c_headers_flagged() {
        let bad = analyze_modernization("#include <stdio.h>\nauto f = std::bind(g, _1);\n", "cpp");
        assert!(
            bad.violations >= 2,
            "std::bind + <stdio.h> must flag: {:?}",
            bad.findings
        );
        let ok = analyze_modernization(
            "#include <cstdio>\nauto f = [](int x) { return g(x); };\n",
            "cpp",
        );
        assert_eq!(
            ok.violations, 0,
            "<cstdio> + lambda is modern: {:?}",
            ok.findings
        );
    }

    // ── Cross-cutting ──────────────────────────────────────────────────────────
    #[test]
    fn comments_and_tests_excluded() {
        // The legacy forms live only in a comment and a #[cfg(test)] module.
        let src = "// avoid try!( and extern crate foo here\nfn prod() -> bool { true }\n#[cfg(test)]\nmod tests {\n    extern crate bar;\n}\n";
        let r = analyze_modernization(src, "rust");
        assert_eq!(
            r.violations, 0,
            "comment/test legacy excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn cpp_define_not_suppressed_after_code_regions_fix() {
        // Regression guard for the code_regions CPP fix: `#include` is a
        // preprocessor line, NOT a comment, so the header needle still fires.
        let r = analyze_modernization("#include <math.h>\nint main() { return 0; }\n", "cpp");
        assert!(
            r.violations >= 1,
            "<math.h> in an #include must be seen: {:?}",
            r.findings
        );
    }

    #[test]
    fn unknown_language_is_empty() {
        let r = analyze_modernization("extern crate x; try!( super(A,b) require(", "haskell");
        assert_eq!(r.violations, 0);
        assert!((score_modernization(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn score_is_monotone_in_density() {
        let mk = |w: f32| ModernizationReport {
            weighted_total: w,
            total_lines: 100,
            ..Default::default()
        };
        let mut prev = 2.0f32;
        for w in [0.0, 1.0, 3.0, 6.0, 12.0] {
            let s = score_modernization(&mk(w));
            assert!(s <= prev, "more legacy debt must not raise the score");
            prev = s;
        }
    }
}
