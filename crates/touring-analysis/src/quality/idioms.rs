//! Language idioms (D40 / F4.1) — polyglot non-idiomatic-construct detection.
//!
//! D40 asks whether code uses each language's **idiomatic** construct rather
//! than a transplanted or legacy form. The real oracle is each language's lint
//! tool; this engine approximates a high-confidence **subset** of those tools
//! across **7 languages** (a per-file scanner cannot replace a type-aware
//! linter, so it is honest about being a subset):
//!
//! | Lang | Oracle | Sample checks |
//! |------|--------|---------------|
//! | Rust | `clippy` | `len_zero` (`.len()==0`→`is_empty`), `bool_comparison` (`==true`), `comparison_to_empty` (`==""`), `ptr_arg` (`&Vec`/`&String`), `get_first`, `redundant_pattern_matching`, `#[allow(clippy::…)]` suppression |
//! | Python | `ruff`/`flake8` | E711 (`==None`), E712 (`==True/False`), E721 (`type()==`), E731 (`=lambda`), E722 (bare `except:`), `range(len())`→`enumerate`, `.has_key` |
//! | TypeScript / JavaScript | `ESLint` | `eqeqeq` (loose `==`/`!=`, char-aware so strict `===`/`!==` is never flagged), `no-var`, `no-array-constructor`, `no-eval`, `no-explicit-any`, `ban-ts-comment` |
//! | Go | `go vet`/staticcheck | `interface{}`→`any`, `errors.New(fmt.Sprintf())`→`fmt.Errorf` |
//! | C++ | clang-tidy | `using namespace std`, `NULL`→`nullptr`, C-style `malloc` cast |
//! | Java | — | legacy boxing ctors, `Vector`/`Hashtable`, `printStackTrace` |
//!
//! Comments and `#[cfg(test)]`/test regions are excluded via `super::code_regions`
//! (so `// use == None` documentation is never flagged). Production string
//! literals are *not* suppressed by `code_regions`, so the rare idiom-shaped
//! string literal (e.g. `"a == b"` in a message) can be a false positive; the
//! needles are chosen to make this rare, and F4.1 is WARN, not BLOCK.
//!
//! Replaces a stub that counted `let ` + `match ` occurrences and returned
//! `1.0` if there were more than five — a metric with no relationship to
//! idiomaticity. Zero non-std dependencies beyond `memchr` (already a dep).

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

/// One non-idiomatic construct to detect: a literal `needle`, a human message,
/// and a `weight` reflecting how strongly the linter would flag it.
struct IdiomRule {
    needle: &'static [u8],
    message: &'static str,
    weight: f32,
}

// ── Rust (clippy) ─────────────────────────────────────────────────────────────
const RUST_IDIOMS: &[IdiomRule] = &[
    IdiomRule {
        needle: b".len() == 0",
        message: "`.len() == 0` -> `.is_empty()` (clippy::len_zero)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b".len() != 0",
        message: "`.len() != 0` -> `!.is_empty()` (clippy::len_zero)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b".len() > 0",
        message: "`.len() > 0` -> `!.is_empty()` (clippy::len_zero)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b"== \"\"",
        message: "`== \"\"` -> `.is_empty()` (clippy::comparison_to_empty)",
        weight: 0.8,
    },
    IdiomRule {
        needle: b"== true",
        message: "`== true` -> use the bool directly (clippy::bool_comparison)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b"== false",
        message: "`== false` -> use `!bool` (clippy::bool_comparison)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b"!= true",
        message: "`!= true` -> use `!bool` (clippy::bool_comparison)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b"!= false",
        message: "`!= false` -> use the bool directly (clippy::bool_comparison)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b": &Vec<",
        message: "`&Vec<T>` param -> `&[T]` (clippy::ptr_arg)",
        weight: 0.8,
    },
    IdiomRule {
        needle: b": &String",
        message: "`&String` param -> `&str` (clippy::ptr_arg)",
        weight: 0.8,
    },
    IdiomRule {
        needle: b"#[allow(clippy::",
        message: "`#[allow(clippy::..)]` -> fix the lint, do not suppress",
        weight: 0.8,
    },
    IdiomRule {
        needle: b".get(0)",
        message: "`.get(0)` -> `.first()` (clippy::get_first)",
        weight: 0.7,
    },
    IdiomRule {
        needle: b".nth(0)",
        message: "`.nth(0)` -> `.next()` (clippy::iter_nth_zero)",
        weight: 0.7,
    },
    IdiomRule {
        needle: b".or_insert_with(Vec::new)",
        message: "-> `.or_default()` (clippy::unwrap_or_default)",
        weight: 0.6,
    },
    IdiomRule {
        needle: b".unwrap_or_else(Vec::new)",
        message: "-> `.unwrap_or_default()` (clippy::unwrap_or_default)",
        weight: 0.6,
    },
    IdiomRule {
        needle: b"if let Some(_) =",
        message: "`if let Some(_) =` -> `.is_some()` (clippy::redundant_pattern_matching)",
        weight: 0.6,
    },
    IdiomRule {
        needle: b"if let Ok(_) =",
        message: "`if let Ok(_) =` -> `.is_ok()` (clippy::redundant_pattern_matching)",
        weight: 0.6,
    },
    IdiomRule {
        needle: b".to_string().as_str()",
        message: "redundant `.to_string().as_str()` -> use the `&str`",
        weight: 0.6,
    },
];

// ── Python (ruff / flake8) ────────────────────────────────────────────────────
const PYTHON_IDIOMS: &[IdiomRule] = &[
    IdiomRule {
        needle: b"== None",
        message: "`== None` -> `is None` (E711)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b"!= None",
        message: "`!= None` -> `is not None` (E711)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b"None ==",
        message: "`None ==` -> `is None` (E711)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b"None !=",
        message: "`None !=` -> `is not None` (E711)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b"== True",
        message: "`== True` -> use the value directly (E712)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b"== False",
        message: "`== False` -> use `not value` (E712)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b"!= True",
        message: "`!= True` -> use `not value` (E712)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b"!= False",
        message: "`!= False` -> use the value directly (E712)",
        weight: 1.0,
    },
    IdiomRule {
        needle: b"== type(",
        message: "`== type(..)` -> `isinstance(..)` (E721)",
        weight: 0.8,
    },
    IdiomRule {
        needle: b"= lambda ",
        message: "lambda assigned to a name -> use `def` (E731)",
        weight: 0.7,
    },
    IdiomRule {
        needle: b"except:",
        message: "bare `except:` -> catch a specific exception (E722)",
        weight: 0.8,
    },
    IdiomRule {
        needle: b"range(len(",
        message: "`range(len(..))` -> `enumerate(..)`",
        weight: 0.7,
    },
    IdiomRule {
        needle: b".has_key(",
        message: "`.has_key()` -> `key in dict` (removed in Py3)",
        weight: 0.7,
    },
    IdiomRule {
        needle: b".iterkeys(",
        message: "`.iterkeys()` -> `.keys()` (Py2-only)",
        weight: 0.6,
    },
    IdiomRule {
        needle: b"import *",
        message: "wildcard `import *` -> import names explicitly (F403)",
        weight: 0.6,
    },
];

// ── TypeScript (ESLint + @typescript-eslint) ──────────────────────────────────
const TS_IDIOMS: &[IdiomRule] = &[
    IdiomRule {
        needle: b"var ",
        message: "`var` -> `let`/`const` (no-var)",
        weight: 0.8,
    },
    IdiomRule {
        needle: b"new Array(",
        message: "`new Array()` -> `[]` (no-array-constructor)",
        weight: 0.7,
    },
    IdiomRule {
        needle: b"new Object(",
        message: "`new Object()` -> `{}` (no-new-object)",
        weight: 0.7,
    },
    IdiomRule {
        needle: b"eval(",
        message: "`eval()` -> avoid (no-eval)",
        weight: 0.8,
    },
    IdiomRule {
        needle: b".hasOwnProperty(",
        message: "-> `Object.hasOwn()` (no-prototype-builtins)",
        weight: 0.6,
    },
    IdiomRule {
        needle: b": any",
        message: "`: any` -> a precise type (no-explicit-any)",
        weight: 0.6,
    },
    IdiomRule {
        needle: b"as any",
        message: "`as any` -> avoid (no-explicit-any)",
        weight: 0.6,
    },
    IdiomRule {
        needle: b"@ts-ignore",
        message: "`@ts-ignore` -> `@ts-expect-error` or fix the type (ban-ts-comment)",
        weight: 0.7,
    },
];

// ── JavaScript (ESLint) ───────────────────────────────────────────────────────
const JS_IDIOMS: &[IdiomRule] = &[
    IdiomRule {
        needle: b"var ",
        message: "`var` -> `let`/`const` (no-var)",
        weight: 0.8,
    },
    IdiomRule {
        needle: b"new Array(",
        message: "`new Array()` -> `[]` (no-array-constructor)",
        weight: 0.7,
    },
    IdiomRule {
        needle: b"new Object(",
        message: "`new Object()` -> `{}` (no-new-object)",
        weight: 0.7,
    },
    IdiomRule {
        needle: b"eval(",
        message: "`eval()` -> avoid (no-eval)",
        weight: 0.8,
    },
    IdiomRule {
        needle: b".hasOwnProperty(",
        message: "-> `Object.hasOwn()` (no-prototype-builtins)",
        weight: 0.6,
    },
];

// ── Go (go vet / staticcheck) ─────────────────────────────────────────────────
const GO_IDIOMS: &[IdiomRule] = &[
    IdiomRule {
        needle: b"interface{}",
        message: "`interface{}` -> `any` (Go 1.18+)",
        weight: 0.7,
    },
    IdiomRule {
        needle: b"errors.New(fmt.Sprintf(",
        message: "-> `fmt.Errorf()` (S1028)",
        weight: 0.7,
    },
    IdiomRule {
        needle: b"== nil {\n\t\treturn nil\n\t}",
        message: "redundant nil-guard (S1023)",
        weight: 0.5,
    },
];

// ── C++ (clang-tidy modernize/*) ──────────────────────────────────────────────
const CPP_IDIOMS: &[IdiomRule] = &[
    IdiomRule {
        needle: b"using namespace std",
        message: "`using namespace std` -> qualify or import specific names",
        weight: 0.7,
    },
    IdiomRule {
        needle: b"NULL",
        message: "`NULL` -> `nullptr` (modernize-use-nullptr)",
        weight: 0.6,
    },
    IdiomRule {
        needle: b")malloc(",
        message: "C-style `malloc` -> `new`/`std::make_unique`",
        weight: 0.6,
    },
    IdiomRule {
        needle: b"typedef ",
        message: "`typedef` -> `using` alias (modernize-use-using)",
        weight: 0.5,
    },
];

// ── Java ──────────────────────────────────────────────────────────────────────
const JAVA_IDIOMS: &[IdiomRule] = &[
    IdiomRule {
        needle: b"new Integer(",
        message: "`new Integer()` -> `Integer.valueOf()` (deprecated ctor)",
        weight: 0.7,
    },
    IdiomRule {
        needle: b"new Boolean(",
        message: "`new Boolean()` -> `Boolean.valueOf()`",
        weight: 0.7,
    },
    IdiomRule {
        needle: b"new Long(",
        message: "`new Long()` -> `Long.valueOf()`",
        weight: 0.7,
    },
    IdiomRule {
        needle: b"Vector<",
        message: "`Vector` -> `ArrayList` (synchronized legacy)",
        weight: 0.7,
    },
    IdiomRule {
        needle: b"Hashtable<",
        message: "`Hashtable` -> `HashMap`",
        weight: 0.7,
    },
    IdiomRule {
        needle: b".printStackTrace(",
        message: "`printStackTrace()` -> use a logger",
        weight: 0.6,
    },
    IdiomRule {
        needle: b".equals(\"\")",
        message: "`.equals(\"\")` -> `.isEmpty()`",
        weight: 0.6,
    },
];

fn rules_for(lang: &str) -> &'static [IdiomRule] {
    match lang {
        "rust" => RUST_IDIOMS,
        "python" | "py" => PYTHON_IDIOMS,
        "typescript" | "ts" | "tsx" => TS_IDIOMS,
        "javascript" | "js" | "jsx" => JS_IDIOMS,
        "go" => GO_IDIOMS,
        "cpp" | "c++" | "cc" | "cxx" => CPP_IDIOMS,
        "java" => JAVA_IDIOMS,
        _ => &[],
    }
}

/// Does this language use `===`/`!==`, requiring char-aware loose-equality
/// detection (so a strict `===` is never mistaken for a loose `==`)?
fn has_loose_equality(lang: &str) -> bool {
    matches!(
        lang,
        "javascript" | "js" | "jsx" | "typescript" | "ts" | "tsx"
    )
}

/// Per-file idiom analysis.
pub type IdiomReport = crate::quality::SmellReport;

/// Count loose-equality (`==` / `!=`) occurrences that are **not** strict
/// (`===` / `!==`), skipping comment/test regions. Char-aware: a `==` whose
/// neighbour is `=` is part of `===` and is never counted (so the correct,
/// idiomatic strict form is never flagged).
fn count_loose_equality(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut n = 0usize;
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        let (a, b) = (bytes[i], bytes[i + 1]);
        let is_eq = a == b'=' && b == b'=';
        let is_ne = a == b'!' && b == b'=';
        if is_eq || is_ne {
            let prev = if i > 0 { bytes[i - 1] } else { 0 };
            let next = bytes.get(i + 2).copied().unwrap_or(0);
            // `===`/`!==` (next `=`) or the trailing `==` of `===` (prev `=`) → strict.
            let strict = next == b'=' || (is_eq && prev == b'=');
            if !strict && !offset_suppressed(i, regions) {
                n += 1;
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    n
}

/// Analyze non-idiomatic constructs for `lang`. Unknown languages yield an empty
/// report (no idiom model → no findings).
#[must_use]
pub fn analyze_idioms(source: &str, lang: &str) -> IdiomReport {
    let regions = non_executable_regions(source, lang);
    let bytes = source.as_bytes();
    let mut report = IdiomReport {
        total_lines: source.lines().count(),
        ..IdiomReport::default()
    };

    for rule in rules_for(lang) {
        let count = memmem::find_iter(bytes, rule.needle)
            .filter(|&off| !offset_suppressed(off, &regions))
            .count();
        if count > 0 {
            report.violations += count;
            report.weighted_total += rule.weight * count as f32;
            report.findings.push((rule.message.to_string(), count));
        }
    }

    if has_loose_equality(lang) {
        let loose = count_loose_equality(bytes, &regions);
        if loose > 0 {
            report.violations += loose;
            report.weighted_total += 1.0 * loose as f32;
            report.findings.push((
                "loose `==`/`!=` -> strict `===`/`!==` (eqeqeq)".to_string(),
                loose,
            ));
        }
    }

    // Most-impactful findings first (weight*count is unavailable post-hoc, so
    // sort by raw count as a stable proxy for evidence ordering).
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// D40 idiom score: `1 - density * SCALE`, where density is the weighted
/// violation count per production line. A clean file is `1.0`; idiom debt
/// accumulates linearly. WARN-tier (advisory), so heavy non-idiomatic files may
/// land below 0.5.
#[must_use]
pub fn score_idioms(report: &IdiomReport) -> f32 {
    const SCALE: f32 = 8.0;
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idiomatic_rust_is_clean() {
        let src = "fn f(v: &[u32]) -> bool {\n    if v.is_empty() {\n        return true;\n    }\n    v.first().is_some()\n}\n";
        let r = analyze_idioms(src, "rust");
        assert_eq!(
            r.violations, 0,
            "idiomatic rust has no findings: {:?}",
            r.findings
        );
        assert!((score_idioms(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn non_idiomatic_rust_is_flagged() {
        let src = "fn f(v: &Vec<u32>) -> bool {\n    if v.len() == 0 {\n        return false;\n    }\n    v.is_empty() == true\n}\n";
        let r = analyze_idioms(src, "rust");
        // len_zero + ptr_arg + bool_comparison
        assert!(r.violations >= 3, "got {:?}", r.findings);
        assert!(score_idioms(&r) < 1.0);
    }

    #[test]
    fn python_none_and_bool_comparison_flagged() {
        let src = "def f(x):\n    if x == None:\n        return\n    if x == True:\n        pass\n";
        let r = analyze_idioms(src, "python");
        assert!(
            r.violations >= 2,
            "E711 + E712 expected, got {:?}",
            r.findings
        );
    }

    #[test]
    fn python_idiomatic_is_clean() {
        let src = "def f(x):\n    if x is None:\n        return\n    if x:\n        pass\n";
        let r = analyze_idioms(src, "python");
        assert_eq!(r.violations, 0, "{:?}", r.findings);
    }

    #[test]
    fn eqeqeq_flags_loose_but_not_strict() {
        let loose = analyze_idioms("if (a == b) { return c != d; }\n", "javascript");
        assert_eq!(
            loose.violations, 2,
            "two loose comparisons: {:?}",
            loose.findings
        );
        let strict = analyze_idioms("if (a === b) { return c !== d; }\n", "javascript");
        assert_eq!(
            strict.violations, 0,
            "strict === / !== is idiomatic: {:?}",
            strict.findings
        );
    }

    #[test]
    fn ts_var_and_any_flagged() {
        let src = "var x: any = 1;\nconst y = x as any;\n";
        let r = analyze_idioms(src, "typescript");
        // var + : any + as any (+ no eqeqeq here)
        assert!(r.violations >= 3, "got {:?}", r.findings);
    }

    #[test]
    fn go_interface_any() {
        let r = analyze_idioms("func f(x interface{}) {}\n", "go");
        assert_eq!(r.violations, 1);
    }

    #[test]
    fn comments_excluded() {
        // The non-idiomatic forms live only in a comment → not flagged.
        let src = "fn f() {\n    // avoid v.len() == 0 and x == true here\n    let _ = 1;\n}\n";
        let r = analyze_idioms(src, "rust");
        assert_eq!(
            r.violations, 0,
            "comment-only idioms not flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn test_region_excluded() {
        // A rust idiom (`.len() == 0`) living only inside a `#[cfg(test)]` module
        // is not production idiom debt.
        let src = "fn prod() -> bool { true }\n#[cfg(test)]\nmod tests {\n    fn t(v: &Vec<u8>) { let _ = v.len() == 0; }\n}\n";
        let r = analyze_idioms(src, "rust");
        assert_eq!(
            r.violations, 0,
            "test-region idioms excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn unknown_language_is_empty() {
        let r = analyze_idioms("anything == None here", "haskell");
        assert_eq!(r.violations, 0);
        assert!((score_idioms(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn score_is_monotone_in_density() {
        let mk = |w: f32| IdiomReport {
            weighted_total: w,
            total_lines: 100,
            ..Default::default()
        };
        let mut prev = 2.0f32;
        for w in [0.0, 1.0, 3.0, 6.0, 12.0] {
            let s = score_idioms(&mk(w));
            assert!(s <= prev, "more idiom debt must not raise the score");
            prev = s;
        }
    }
}
