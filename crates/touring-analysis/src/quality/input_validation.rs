//! Input validation (D15 / F2.2) — polyglot boundary-validation security smells.
//!
//! D15 asks whether untrusted input is validated at the boundary with an
//! **allowlist** (not a blocklist), whether path traversal is prevented
//! (canonicalize + confine to a base directory), and whether deserialization /
//! input parsing is safe. The idiomatic *good* form (an `^[a-z0-9]{3,10}$`
//! allowlist regex, `realpath` + `starts_with(base)`, `yaml.safe_load`) cannot
//! be proven present by a scanner, so this engine detects the high-confidence
//! **anti-patterns**: blocklist sanitization, insecure deserialization
//! (CWE-502), unbounded input (CWE-242/120), and auto-escaping bypasses.
//!
//! It is disjoint from F2.1 OWASP (injection sinks — SQL/command, via the
//! `SecurityAnalyzer` ast-grep catalogs), F2.4 secrets, and F2.6 config: F2.2
//! scores *boundary input validation*. F2.2 is **WARN** and rolls up as
//! `WorstOf` (the worst file in scope is the score), so the catalogue is kept
//! high-precision — a false positive would drag the whole scope.
//!
//! | Lang | Anti-patterns detected (CWE) | Idiomatic alternative |
//! |------|------------------------------|-----------------------|
//! | Rust | `.replace("../"` blocklist (CWE-22), `from_utf8_unchecked` (skips validation) | `canonicalize()` + `starts_with(base)`, `from_utf8` |
//! | Python | `pickle.load(s)` / `yaml.load(` / `marshal.loads` (CWE-502), `.replace("../"` | `yaml.safe_load`, a typed schema, an allowlist |
//! | TypeScript / JavaScript | `dangerouslySetInnerHTML`, `document.write(` (DOM XSS), `.replace("../"` | sanitize (DOMPurify), `textContent` |
//! | Go | `template.HTML(`/`template.JS(`/`template.URL(` (escaping bypass) | let `html/template` auto-escape |
//! | Java | `ObjectInputStream` / `.readObject(` (CWE-502) | a `LookAheadObjectInputStream` allowlist |
//! | C / C++ | `gets(` (CWE-242), `strcpy`/`strcat`/`sprintf`, `scanf("%s"` (CWE-120) | `fgets`, `strncpy`/`snprintf`, a width-limited `%s` |
//!
//! Comments and `#[cfg(test)]`/test regions are excluded via
//! `super::code_regions`. `yaml.safe_load(` is *not* matched by the
//! `yaml.load(` needle (so the safe form is never flagged). Replaces a stub
//! that scored `validate`/`sanitize`/`.parse()` keyword density. Zero non-std
//! deps beyond `memchr`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

/// A pure-substring input-validation anti-pattern: literal `needle`, the
/// idiomatic fix (`message`), and a `weight`.
struct InputRule {
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

// ── Per-language needles ──────────────────────────────────────────────────────
const RUST_INPUT_NEEDLES: &[InputRule] = &[
    InputRule {
        needle: b".replace(\"../",
        message: "`.replace(\"../\", ..)` blocklist (CWE-22) -> `canonicalize()` + confine with `starts_with(base)`",
        weight: 0.6,
    },
    InputRule {
        needle: b"from_utf8_unchecked(",
        message: "`from_utf8_unchecked` skips UTF-8 validation -> `from_utf8` for untrusted bytes",
        weight: 0.5,
    },
];

const PYTHON_INPUT_NEEDLES: &[InputRule] = &[
    InputRule {
        needle: b"pickle.loads(",
        message: "`pickle.loads` on untrusted data = RCE (CWE-502) -> a typed/JSON schema",
        weight: 0.8,
    },
    InputRule {
        needle: b"pickle.load(",
        message: "`pickle.load` on untrusted data = RCE (CWE-502) -> a typed/JSON schema",
        weight: 0.8,
    },
    InputRule {
        needle: b"yaml.load(",
        message: "`yaml.load` (arbitrary objects) -> `yaml.safe_load`",
        weight: 0.7,
    },
    InputRule {
        needle: b"marshal.loads(",
        message: "`marshal.loads` on untrusted data (CWE-502) -> a safe format",
        weight: 0.6,
    },
    InputRule {
        needle: b".replace(\"../",
        message: "`.replace(\"../\", ..)` blocklist (CWE-22) -> `os.path.realpath` + confine to base",
        weight: 0.6,
    },
];

const JSTS_INPUT_NEEDLES: &[InputRule] = &[
    InputRule {
        needle: b"dangerouslySetInnerHTML",
        message: "`dangerouslySetInnerHTML` (XSS) -> sanitize (DOMPurify) or render text",
        weight: 0.6,
    },
    InputRule {
        needle: b"document.write(",
        message: "`document.write(` (DOM XSS sink) -> build nodes / `textContent`",
        weight: 0.5,
    },
    InputRule {
        needle: b".replace(\"../",
        message: "`.replace(\"../\", ..)` blocklist (CWE-22) -> resolve + confine to base",
        weight: 0.6,
    },
];

const GO_INPUT_NEEDLES: &[InputRule] = &[
    InputRule {
        needle: b"template.HTML(",
        message: "`template.HTML(` bypasses auto-escaping (XSS) -> let `html/template` escape",
        weight: 0.6,
    },
    InputRule {
        needle: b"template.JS(",
        message: "`template.JS(` bypasses auto-escaping (XSS) -> let `html/template` escape",
        weight: 0.5,
    },
    InputRule {
        needle: b"template.URL(",
        message: "`template.URL(` bypasses auto-escaping (XSS) -> validate the URL",
        weight: 0.5,
    },
];

const JAVA_INPUT_NEEDLES: &[InputRule] = &[
    InputRule {
        needle: b"ObjectInputStream",
        message: "`ObjectInputStream` (CWE-502) -> a `LookAheadObjectInputStream` class allowlist",
        weight: 0.6,
    },
    InputRule {
        needle: b".readObject(",
        message: "`.readObject(` (insecure deserialization, CWE-502) -> validate via `resolveClass`",
        weight: 0.7,
    },
];

const CPP_INPUT_NEEDLES: &[InputRule] = &[
    // NOTE: `gets(` is detected structurally (see `cpp_bare_gets`) because the
    // pure-substring form would also match the *safe* `fgets(`.
    InputRule {
        needle: b"strcpy(",
        message: "`strcpy(` (unbounded copy, CWE-120) -> `strncpy`/`strlcpy`",
        weight: 0.4,
    },
    InputRule {
        needle: b"strcat(",
        message: "`strcat(` (unbounded concat, CWE-120) -> `strncat`/`strlcat`",
        weight: 0.4,
    },
    InputRule {
        needle: b"sprintf(",
        message: "`sprintf(` (unbounded format) -> `snprintf`",
        weight: 0.4,
    },
    InputRule {
        needle: b"scanf(\"%s",
        message: "`scanf(\"%s\")` (unbounded input) -> a width-limited `%s` (e.g. `%31s`)",
        weight: 0.5,
    },
];

fn input_needles_for(lang: Lang) -> &'static [InputRule] {
    match lang {
        Lang::Rust => RUST_INPUT_NEEDLES,
        Lang::Python => PYTHON_INPUT_NEEDLES,
        Lang::TsJs => JSTS_INPUT_NEEDLES,
        Lang::Go => GO_INPUT_NEEDLES,
        Lang::Java => JAVA_INPUT_NEEDLES,
        Lang::Cpp => CPP_INPUT_NEEDLES,
        Lang::Other => &[],
    }
}

/// Per-file input-validation analysis (parallel shape to [`super::idioms::IdiomReport`]).
#[derive(Debug, Clone, Default)]
pub struct InputValidationReport {
    /// Total input-validation anti-patterns found in production code.
    pub violations: usize,
    /// Weighted sum (each anti-pattern scaled by its rule weight).
    pub weighted_total: f32,
    /// Production lines considered (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired rule, for evidence (highest count first).
    pub findings: Vec<(String, usize)>,
}

impl InputValidationReport {
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

/// `gets(` (unbounded read, CWE-242) with a word boundary so the *safe*
/// `fgets(` (a superset of the `gets(` substring) is never flagged.
fn cpp_bare_gets(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    for off in memmem::find_iter(bytes, b"gets(") {
        if offset_suppressed(off, regions) {
            continue;
        }
        // Preceding char must not be an identifier char (so `fgets(`/`Wgets(`
        // and any other `…gets(` are excluded — only the bare `gets(` counts).
        if off > 0 && (bytes[off - 1].is_ascii_alphanumeric() || bytes[off - 1] == b'_') {
            continue;
        }
        n += 1;
    }
    n
}

/// Analyze input-validation anti-patterns for `lang`. Unknown languages yield an
/// empty report (no model → no findings → score 1.0).
#[must_use]
pub fn analyze_input_validation(source: &str, lang: &str) -> InputValidationReport {
    let regions = non_executable_regions(source, lang);
    let bytes = source.as_bytes();
    let canon = canonical_lang(lang);
    let mut report = InputValidationReport {
        total_lines: source.lines().count(),
        ..InputValidationReport::default()
    };

    for rule in input_needles_for(canon) {
        let count = memmem::find_iter(bytes, rule.needle)
            .filter(|&off| !offset_suppressed(off, &regions))
            .count();
        report.push(rule.message, count, rule.weight);
    }

    // `gets(` needs a word boundary so the safe `fgets(` is not matched.
    if canon == Lang::Cpp {
        report.push(
            "`gets(` (unbounded read, CWE-242) -> `fgets` with a size bound",
            cpp_bare_gets(bytes, &regions),
            0.8,
        );
    }

    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// D15 input-validation score: `1 - density * SCALE`, where density is the
/// weighted anti-pattern count per production line. A clean boundary is `1.0`.
/// SCALE is steeper (8.0) than the style dimensions because a single
/// deserialization / unbounded-input smell is a real security exposure, and
/// F2.2 rolls up as `WorstOf`. WARN-tier (advisory).
#[must_use]
pub fn score_input_validation(report: &InputValidationReport) -> f32 {
    const SCALE: f32 = 8.0;
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Rust ──────────────────────────────────────────────────────────────────
    #[test]
    fn rust_validated_is_clean() {
        let src = "fn load(p: &Path, base: &Path) -> Result<Vec<u8>> {\n    let real = p.canonicalize()?;\n    if !real.starts_with(base) { bail!(\"traversal\"); }\n    std::fs::read(real)\n}\n";
        let r = analyze_input_validation(src, "rust");
        assert_eq!(
            r.violations, 0,
            "canonicalize+confine is clean: {:?}",
            r.findings
        );
        assert!((score_input_validation(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rust_blocklist_and_unchecked_flagged() {
        let bad = analyze_input_validation(
            "let safe = p.replace(\"../\", \"\");\nlet s = unsafe { std::str::from_utf8_unchecked(bytes) };\n",
            "rust",
        );
        assert!(
            bad.violations >= 2,
            "blocklist + from_utf8_unchecked must flag: {:?}",
            bad.findings
        );
    }

    // ── Python ────────────────────────────────────────────────────────────────
    #[test]
    fn python_insecure_deser_flagged_but_safe_load_clean() {
        let bad = analyze_input_validation(
            "import pickle, yaml\nobj = pickle.loads(data)\ncfg = yaml.load(text)\n",
            "python",
        );
        assert!(
            bad.violations >= 2,
            "pickle.loads + yaml.load must flag: {:?}",
            bad.findings
        );
        // The SAFE form must NOT be flagged (the `yaml.load(` needle must not
        // match `yaml.safe_load(`).
        let ok = analyze_input_validation(
            "cfg = yaml.safe_load(text)\nimport json\nobj = json.loads(data)\n",
            "python",
        );
        assert_eq!(
            ok.violations, 0,
            "yaml.safe_load + json.loads are safe: {:?}",
            ok.findings
        );
    }

    // ── TypeScript / JavaScript ─────────────────────────────────────────────────
    #[test]
    fn jsts_xss_sinks_flagged() {
        let bad = analyze_input_validation(
            "el.innerHTML = clean;\nreturn <div dangerouslySetInnerHTML={{ __html: raw }} />;\ndocument.write(input);\n",
            "typescript",
        );
        assert!(
            bad.violations >= 2,
            "dangerouslySetInnerHTML + document.write must flag: {:?}",
            bad.findings
        );
    }

    // ── Go ──────────────────────────────────────────────────────────────────────
    #[test]
    fn go_template_escape_bypass_flagged() {
        let bad = analyze_input_validation("page := template.HTML(userContent)\n", "go");
        assert!(
            bad.violations >= 1,
            "template.HTML must flag: {:?}",
            bad.findings
        );
    }

    // ── Java ──────────────────────────────────────────────────────────────────
    #[test]
    fn java_insecure_deser_flagged() {
        let bad = analyze_input_validation(
            "ObjectInputStream in = new ObjectInputStream(sock);\nObject o = in.readObject();\n",
            "java",
        );
        assert!(
            bad.violations >= 2,
            "ObjectInputStream + readObject must flag: {:?}",
            bad.findings
        );
    }

    // ── C / C++ ─────────────────────────────────────────────────────────────────
    #[test]
    fn cpp_unbounded_input_flagged() {
        let bad = analyze_input_validation(
            "char buf[8];\ngets(buf);\nstrcpy(buf, src);\nscanf(\"%s\", buf);\n",
            "cpp",
        );
        assert!(
            bad.violations >= 3,
            "gets + strcpy + scanf %s must flag: {:?}",
            bad.findings
        );
        // A width-limited %s must NOT match the unbounded `scanf("%s` needle.
        let ok = analyze_input_validation(
            "char buf[32];\nscanf(\"%31s\", buf);\nfgets(buf, sizeof(buf), stdin);\n",
            "cpp",
        );
        assert_eq!(
            ok.violations, 0,
            "bounded scanf + fgets are safe: {:?}",
            ok.findings
        );
    }

    // ── Cross-cutting ──────────────────────────────────────────────────────────
    #[test]
    fn comments_and_tests_excluded() {
        // The anti-patterns live only in a comment and a #[cfg(test)] module.
        let src = "// never call gets() or use .replace(\"../\", ..) here\nfn prod() -> bool { true }\n#[cfg(test)]\nmod tests {\n    fn t() { let _ = x.replace(\"../\", \"\"); }\n}\n";
        let r = analyze_input_validation(src, "rust");
        assert_eq!(
            r.violations, 0,
            "comment/test anti-patterns excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn unknown_language_is_empty() {
        let r = analyze_input_validation(
            "pickle.loads( gets( ObjectInputStream .replace(\"../",
            "haskell",
        );
        assert_eq!(r.violations, 0);
        assert!((score_input_validation(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn score_is_monotone_in_density() {
        let mk = |w: f32| InputValidationReport {
            weighted_total: w,
            total_lines: 100,
            ..Default::default()
        };
        let mut prev = 2.0f32;
        for w in [0.0, 1.0, 3.0, 6.0, 12.0] {
            let s = score_input_validation(&mk(w));
            assert!(
                s <= prev,
                "more input-validation debt must not raise the score"
            );
            prev = s;
        }
    }
}
