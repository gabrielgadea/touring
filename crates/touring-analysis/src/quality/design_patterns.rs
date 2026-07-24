//! Design patterns (D11 / F1.11) — polyglot design-pattern **anti-pattern**
//! detection: GoF transplants, ownership smells, type-erasure escape hatches,
//! and over-engineering.
//!
//! D11 asks whether code uses the *idiomatic* pattern for its language (Rust:
//! newtype, typestate-via-generics, RAII guard, enum dispatch, sealed traits)
//! rather than an OO/GoF transplant or an ownership smell. This engine detects
//! the **anti-patterns** that signal the wrong choice — a high-confidence
//! subset, since "missing abstraction" / "over-abstraction" cannot be judged by
//! a scanner. It is disjoint from F4.1 idioms (local style), F1.9 api-design
//! (public contract), and F4.4 modernization (version adoption): F1.11 scores
//! the *structural pattern* choice.
//!
//! | Lang | Anti-patterns detected | Idiomatic alternative |
//! |------|------------------------|-----------------------|
//! | Rust | `static mut` (Singleton via unsafe global), `Rc<RefCell<` (shared-mutable overuse), `unsafe impl Send/Sync` (manual thread-safety), `.downcast`/`dyn Any` (type erasure) | `OnceLock`/`LazyLock`, owned/`&mut`, an auto-derived marker, `enum`/generic dispatch |
//! | Python | `global` statement (mutable global state), `def __new__` (Singleton/instance-control hack) | a module-level value, `@dataclass`/factory function |
//! | TypeScript / JavaScript | `getInstance(` (Singleton), `as unknown as` (type-system escape hatch) | a module / `const`, a precise type |
//! | Go | `func init()` (hidden global init), `reflect.` (reflection) | explicit construction, interfaces / generics |
//! | Java | `getInstance(` (Singleton), `FactoryFactory` (over-engineering), `Cloneable` (broken clone), `extends Thread` | DI, a plain factory, a copy constructor, `Runnable`/`Executor` |
//! | C++ | `getInstance(` (Singleton), `dynamic_cast` (type switching), `friend class` (encapsulation break) | a namespace, virtual dispatch, a public accessor |
//!
//! Comments and `#[cfg(test)]`/test regions are excluded via
//! [`super::code_regions`]. `transmute` (owned by [`super::antipatterns`]) and
//! `impl Deref for` (high false-positive overlap with legitimate smart-pointer /
//! newtype `Deref`) are deliberately *not* detected here. F1.11 is WARN
//! (advisory). Replaces a stub that scored the `impl`/`trait` ratio — unrelated
//! to pattern quality. Zero non-std deps beyond `memchr`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

/// A pure-substring design anti-pattern: literal `needle`, the idiomatic
/// alternative (`message`), and a `weight`.
struct PatternRule {
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

// ── Pure-substring needles (structural detectors live in analyze_<lang>) ───────
const RUST_PATTERN_NEEDLES: &[PatternRule] = &[
    PatternRule {
        needle: b"static mut ",
        message: "`static mut` (Singleton via unsafe global) -> `OnceLock`/`LazyLock`",
        weight: 0.8,
    },
    PatternRule {
        needle: b"Rc<RefCell<",
        message: "`Rc<RefCell<T>>` (shared-mutable overuse) -> reconsider ownership (owned / `&mut`)",
        weight: 0.5,
    },
    PatternRule {
        needle: b".downcast",
        message: "`.downcast`/`dyn Any` (type erasure) -> `enum`/generic dispatch",
        weight: 0.5,
    },
];

const PYTHON_PATTERN_NEEDLES: &[PatternRule] = &[PatternRule {
    needle: b"def __new__",
    message: "`__new__` override (Singleton / instance-control hack) -> a module value / `@dataclass`",
    weight: 0.3,
}];

const JSTS_PATTERN_NEEDLES: &[PatternRule] = &[
    PatternRule {
        needle: b"getInstance(",
        message: "`getInstance()` (Singleton) -> a module / `const`",
        weight: 0.4,
    },
    PatternRule {
        needle: b"as unknown as ",
        message: "`as unknown as` (type-system escape hatch) -> a precise type / type guard",
        weight: 0.5,
    },
];

const GO_PATTERN_NEEDLES: &[PatternRule] = &[
    PatternRule {
        needle: b"func init()",
        message: "`func init()` (hidden global init) -> explicit construction",
        weight: 0.4,
    },
    PatternRule {
        needle: b"reflect.",
        message: "`reflect.*` (reflection) -> interfaces / generics (Go 1.18+)",
        weight: 0.4,
    },
];

const JAVA_PATTERN_NEEDLES: &[PatternRule] = &[
    PatternRule {
        needle: b"getInstance(",
        message: "`getInstance()` (Singleton) -> dependency injection",
        weight: 0.4,
    },
    PatternRule {
        needle: b"FactoryFactory",
        message: "`FactoryFactory` (over-engineering) -> a plain factory / builder",
        weight: 0.6,
    },
    PatternRule {
        needle: b"Cloneable",
        message: "`Cloneable`/`clone()` (broken clone pattern) -> a copy constructor",
        weight: 0.4,
    },
    PatternRule {
        needle: b"extends Thread",
        message: "`extends Thread` -> implement `Runnable` / use an `Executor`",
        weight: 0.4,
    },
];

const CPP_PATTERN_NEEDLES: &[PatternRule] = &[
    PatternRule {
        needle: b"getInstance(",
        message: "`getInstance()` (Singleton) -> a namespace / DI",
        weight: 0.4,
    },
    PatternRule {
        needle: b"dynamic_cast",
        message: "`dynamic_cast` (type switching) -> virtual dispatch",
        weight: 0.4,
    },
    PatternRule {
        needle: b"friend class",
        message: "`friend class` (encapsulation break) -> a public accessor",
        weight: 0.3,
    },
];

fn pattern_needles_for(lang: Lang) -> &'static [PatternRule] {
    match lang {
        Lang::Rust => RUST_PATTERN_NEEDLES,
        Lang::Python => PYTHON_PATTERN_NEEDLES,
        Lang::TsJs => JSTS_PATTERN_NEEDLES,
        Lang::Go => GO_PATTERN_NEEDLES,
        Lang::Java => JAVA_PATTERN_NEEDLES,
        Lang::Cpp => CPP_PATTERN_NEEDLES,
        Lang::Other => &[],
    }
}

/// Per-file design-pattern analysis (parallel shape to [`super::idioms::IdiomReport`]).
#[derive(Debug, Clone, Default)]
pub struct DesignPatternReport {
    /// Total design anti-patterns found in production code.
    pub violations: usize,
    /// Weighted sum (each anti-pattern scaled by its rule weight).
    pub weighted_total: f32,
    /// Production lines considered (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired rule, for evidence (highest count first).
    pub findings: Vec<(String, usize)>,
}

impl DesignPatternReport {
    /// Record `count` occurrences of one anti-pattern with the given `weight`.
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

// ── Structural detectors ──────────────────────────────────────────────────────

/// Manual `unsafe impl Send`/`unsafe impl Sync` — asserting thread-safety by
/// hand (a design smell worth review). Line-scoped so the generic form
/// `unsafe impl<T> Send for X<T>` is caught too (the `Send for`/`Sync for`
/// suffix is what auto-derived markers never write).
fn rust_unsafe_marker_impls(source: &str, regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    let mut offset = 0usize;
    for chunk in source.split_inclusive('\n') {
        let line_off = offset;
        offset += chunk.len();
        if offset_suppressed(line_off, regions) {
            continue;
        }
        if chunk.contains("unsafe impl")
            && (chunk.contains("Send for") || chunk.contains("Sync for"))
        {
            n += 1;
        }
    }
    n
}

/// A `global` statement (mutable global state) — a design smell pointing at a
/// Singleton / shared-state pattern. Line-scoped so `myglobal = ...` is not a
/// false match.
fn python_global_statement(source: &str, regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    let mut offset = 0usize;
    for chunk in source.split_inclusive('\n') {
        let line_off = offset;
        offset += chunk.len();
        if offset_suppressed(line_off, regions) {
            continue;
        }
        if chunk.trim_start().starts_with("global ") {
            n += 1;
        }
    }
    n
}

// ── Per-language assembly ────────────────────────────────────────────────────

fn analyze_rust(source: &str, regions: &[(usize, usize)], report: &mut DesignPatternReport) {
    report.push(
        "manual `unsafe impl Send`/`Sync` -> review the invariant; prefer an auto-derived marker",
        rust_unsafe_marker_impls(source, regions),
        0.5,
    );
}

fn analyze_python(source: &str, regions: &[(usize, usize)], report: &mut DesignPatternReport) {
    report.push(
        "`global` statement (mutable global state) -> pass state explicitly / a class",
        python_global_statement(source, regions),
        0.4,
    );
}

/// Analyze design-pattern anti-patterns for `lang`. Unknown languages yield an
/// empty report (no model → no findings → score 1.0).
#[must_use]
pub fn analyze_design_patterns(source: &str, lang: &str) -> DesignPatternReport {
    let regions = non_executable_regions(source, lang);
    let bytes = source.as_bytes();
    let mut report = DesignPatternReport {
        total_lines: source.lines().count(),
        ..DesignPatternReport::default()
    };
    let canon = canonical_lang(lang);

    for rule in pattern_needles_for(canon) {
        let count = memmem::find_iter(bytes, rule.needle)
            .filter(|&off| !offset_suppressed(off, &regions))
            .count();
        report.push(rule.message, count, rule.weight);
    }

    match canon {
        Lang::Rust => analyze_rust(source, &regions, &mut report),
        Lang::Python => analyze_python(source, &regions, &mut report),
        // TS/JS, Go, Java, C++ are needle-only; Other has no model.
        _ => {}
    }

    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// D11 design-pattern score: `1 - density * SCALE`, where density is the
/// weighted anti-pattern count per production line. Idiomatic code is `1.0`;
/// pattern debt accumulates linearly. WARN-tier (advisory).
#[must_use]
pub fn score_design_patterns(report: &DesignPatternReport) -> f32 {
    const SCALE: f32 = 6.0;
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Rust ──────────────────────────────────────────────────────────────────
    #[test]
    fn rust_idiomatic_is_clean() {
        let src = "use std::sync::OnceLock;\nstruct Miles(f64);\nenum State { A, B }\nfn dispatch(s: State) -> u8 { match s { State::A => 1, State::B => 2 } }\n";
        let r = analyze_design_patterns(src, "rust");
        assert_eq!(r.violations, 0, "idiomatic rust is clean: {:?}", r.findings);
        assert!((score_design_patterns(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rust_static_mut_flagged() {
        let bad = analyze_design_patterns("static mut COUNTER: u32 = 0;\n", "rust");
        assert!(
            bad.violations >= 1,
            "static mut must flag: {:?}",
            bad.findings
        );
    }

    #[test]
    fn rust_rc_refcell_and_downcast_flagged() {
        let bad = analyze_design_patterns(
            "let x: Rc<RefCell<Node>> = make();\nlet y = any.downcast_ref::<Foo>();\n",
            "rust",
        );
        assert!(
            bad.violations >= 2,
            "Rc<RefCell> + downcast must flag: {:?}",
            bad.findings
        );
    }

    #[test]
    fn rust_unsafe_marker_flagged_incl_generic() {
        let plain = analyze_design_patterns("unsafe impl Send for Foo {}\n", "rust");
        assert!(
            plain.violations >= 1,
            "unsafe impl Send must flag: {:?}",
            plain.findings
        );
        let generic = analyze_design_patterns("unsafe impl<T> Sync for Foo<T> {}\n", "rust");
        assert!(
            generic.violations >= 1,
            "generic unsafe impl Sync must flag: {:?}",
            generic.findings
        );
        // A safe auto-derived marker (no `unsafe impl`) and a type whose name
        // merely ends in "Send" must NOT flag.
        let ok = analyze_design_patterns(
            "impl Handler for MessageSender {}\n#[derive(Clone)]\nstruct S;\n",
            "rust",
        );
        assert_eq!(
            ok.violations, 0,
            "no false positive on Sender / auto markers: {:?}",
            ok.findings
        );
    }

    // ── Python ────────────────────────────────────────────────────────────────
    #[test]
    fn python_global_and_new_flagged() {
        let bad = analyze_design_patterns(
            "_inst = None\nclass S:\n    def __new__(cls):\n        global _inst\n        return _inst\n",
            "python",
        );
        assert!(
            bad.violations >= 2,
            "global + __new__ must flag: {:?}",
            bad.findings
        );
        // `myglobal = 1` must not be a false match for the `global` statement.
        let ok = analyze_design_patterns("myglobal = 1\nclass S:\n    pass\n", "python");
        assert_eq!(
            ok.violations, 0,
            "myglobal is not a global statement: {:?}",
            ok.findings
        );
    }

    // ── TypeScript / JavaScript ─────────────────────────────────────────────────
    #[test]
    fn jsts_singleton_and_escape_hatch_flagged() {
        let bad = analyze_design_patterns(
            "const db = Database.getInstance();\nconst x = (y as unknown as Foo);\n",
            "typescript",
        );
        assert!(
            bad.violations >= 2,
            "getInstance + as unknown as must flag: {:?}",
            bad.findings
        );
        let ok = analyze_design_patterns(
            "import { db } from './db';\nconst x = y as Foo;\n",
            "typescript",
        );
        assert_eq!(
            ok.violations, 0,
            "module import + single assertion is fine: {:?}",
            ok.findings
        );
    }

    // ── Go ──────────────────────────────────────────────────────────────────────
    #[test]
    fn go_init_and_reflect_flagged() {
        let bad = analyze_design_patterns(
            "func init() { register() }\nv := reflect.ValueOf(x)\n",
            "go",
        );
        assert!(
            bad.violations >= 2,
            "func init + reflect must flag: {:?}",
            bad.findings
        );
    }

    // ── Java ──────────────────────────────────────────────────────────────────
    #[test]
    fn java_gof_smells_flagged() {
        let bad = analyze_design_patterns(
            "class T extends Thread implements Cloneable {\n    static T getInstance() { return new AbstractFactoryFactory(); }\n}\n",
            "java",
        );
        // extends Thread + Cloneable + getInstance + FactoryFactory
        assert!(
            bad.violations >= 4,
            "java GoF smells must flag: {:?}",
            bad.findings
        );
    }

    // ── C++ ─────────────────────────────────────────────────────────────────────
    #[test]
    fn cpp_singleton_and_dynamic_cast_flagged() {
        let bad = analyze_design_patterns(
            "auto* d = Db::getInstance();\nauto* p = dynamic_cast<Derived*>(base);\n",
            "cpp",
        );
        assert!(
            bad.violations >= 2,
            "getInstance + dynamic_cast must flag: {:?}",
            bad.findings
        );
    }

    // ── Cross-cutting ──────────────────────────────────────────────────────────
    #[test]
    fn comments_and_tests_excluded() {
        // The anti-patterns live only in a comment and a #[cfg(test)] module.
        let src = "// avoid static mut and Rc<RefCell<T>> here\nfn prod() -> bool { true }\n#[cfg(test)]\nmod tests {\n    static mut X: u8 = 0;\n}\n";
        let r = analyze_design_patterns(src, "rust");
        assert_eq!(
            r.violations, 0,
            "comment/test anti-patterns excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn unknown_language_is_empty() {
        let r = analyze_design_patterns(
            "static mut x getInstance( dynamic_cast func init()",
            "haskell",
        );
        assert_eq!(r.violations, 0);
        assert!((score_design_patterns(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn score_is_monotone_in_density() {
        let mk = |w: f32| DesignPatternReport {
            weighted_total: w,
            total_lines: 100,
            ..Default::default()
        };
        let mut prev = 2.0f32;
        for w in [0.0, 1.0, 3.0, 6.0, 12.0] {
            let s = score_design_patterns(&mk(w));
            assert!(s <= prev, "more pattern debt must not raise the score");
            prev = s;
        }
    }
}
