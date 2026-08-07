//! Estimate complexity metrics from source code.
//!
//! `max_complexity`/`avg_complexity` are the TRUE per-function cyclomatic
//! complexity from the tree-sitter CC walker (`touring-code`) for the five
//! supported languages (Rust/Python/TS/JS/Bash); a keyword-counting heuristic
//! is the fallback for other languages or on parse failure. Halstead, MI, and
//! cognitive complexity remain heuristic estimates (see W0.4 roadmap).
//!
//! ## StringZilla SIMD integration (T1.1)
//!
//! `count_lines` uses [`stringzilla::stringzilla::RangeUtf8NewlineSplits`]
//! for the line-splitting hot path.  The iterator delegates to
//! `sz_utf8_find_newline` which selects SWAR → SIMD kernels at runtime,
//! yielding measurably lower branch density on large files while keeping
//! byte-identical semantics to `str::lines()` for ASCII source.

use std::collections::HashSet;

use memchr::memmem;
use stringzilla::stringzilla::RangeUtf8NewlineSplits;
use touring_code::ast::{Lang, compute_complexity_for_source};

use super::{ComplexityMetrics, HalsteadMetrics};

/// Estimate complexity metrics from source code.
///
/// Uses keyword-based heuristic (counting branching keywords)
/// as a fast proxy for cyclomatic complexity.
pub fn estimate_complexity(source: &str, language: &str) -> ComplexityMetrics {
    let bytes = source.as_bytes();

    let branch_keywords: Vec<&[u8]> = match language {
        "rust" => vec![
            b"if ", b"else ", b"match ", b"for ", b"while ", b"loop ", b"=> ",
        ],
        "python" => vec![b"if ", b"elif ", b"else:", b"for ", b"while ", b"except:"],
        "typescript" | "ts" | "javascript" | "js" => {
            vec![b"if ", b"else ", b"for ", b"while ", b"switch ", b"case "]
        }
        "go" => vec![b"if ", b"else ", b"for ", b"switch ", b"case "],
        "c" | "cpp" | "c++" => vec![b"if ", b"else ", b"for ", b"while ", b"switch ", b"case "],
        "java" => vec![
            b"if ", b"else ", b"for ", b"while ", b"switch ", b"case ", b"catch ",
        ],
        _ => vec![b"if ", b"else ", b"for ", b"while "],
    };

    let fn_keywords: Vec<&[u8]> = match language {
        "rust" => vec![b"fn "],
        "python" => vec![b"def "],
        "go" => vec![b"func "],
        // K&R brace style assumed (standard in Java). Every method/constructor
        // signature ends with `) {`. Field declarations like `private String name;`
        // do NOT match, preventing overcounting.
        "java" => vec![b") {"],
        _ => vec![b"fn ", b"function "],
    };

    // Count functions
    let function_count: usize = fn_keywords
        .iter()
        .map(|kw| memmem::find_iter(bytes, kw).count())
        .sum();

    // Count branch points as complexity proxy
    let total_branches: usize = branch_keywords
        .iter()
        .map(|kw| memmem::find_iter(bytes, kw).count())
        .sum();

    // Keyword-counting fallback (mean-of-branches). Historically this value was
    // stored in `max_complexity`, but it is an AVERAGE — it diluted a single
    // monster function across many trivial ones (a CC-40 fn among 39 CC-1 fns
    // scored ~2). Kept ONLY as the fallback for languages the tree-sitter engine
    // does not cover or when parsing fails.
    let fallback_max = match total_branches.checked_div(function_count) {
        Some(quotient) => quotient + 1,
        None if total_branches > 0 => total_branches + 1,
        None => 1,
    };
    let fallback_avg = if function_count > 0 {
        (total_branches as f64 / function_count as f64) + 1.0
    } else {
        1.0
    };

    // Real per-function cyclomatic complexity via tree-sitter (touring-code).
    // `max_complexity` becomes the TRUE maximum over all callable symbols
    // (McCabe: 1 + Σ decision points), `avg_complexity` the true mean. This is
    // the single source of CC truth shared with the TDG engine (touring ast tdg)
    // — it eliminates the historical ~8× divergence between the two estimators.
    let (max_complexity, avg_complexity) =
        real_cc_or_fallback(source, language, fallback_max, fallback_avg);

    // Count symbols (rough: type + fn + const keywords)
    let type_keywords: Vec<&[u8]> = match language {
        "rust" => vec![b"struct ", b"enum ", b"trait ", b"type ", b"const "],
        "python" => vec![b"class "],
        "go" => vec![b"type ", b"const "],
        _ => vec![b"class ", b"interface ", b"const "],
    };

    let symbol_count: usize = function_count
        + type_keywords
            .iter()
            .map(|kw| memmem::find_iter(bytes, kw).count())
            .sum::<usize>();

    let cognitive_complexity = estimate_cognitive_complexity(source);
    let (sloc, cloc, blank) = count_lines(source, language);
    let lloc = sloc.saturating_sub(cloc);
    let nexits = count_exits(source, language);
    let halstead = estimate_halstead(source, language);
    // MI (SEI/Mozilla) is defined over AVERAGE per-module complexity, not the
    // per-function maximum — feed it `avg_complexity` so the max-CC fix above
    // does not double-penalise MI for files that contain one complex function.
    let maintainability_index = estimate_maintainability_index(
        halstead.volume,
        avg_complexity.round().max(1.0) as usize,
        lloc,
    );

    ComplexityMetrics {
        function_count,
        max_complexity,
        avg_complexity,
        symbol_count,
        cognitive_complexity,
        sloc,
        cloc,
        lloc,
        nexits,
        blank,
        maintainability_index,
        halstead,
    }
}

/// Map the free-text `language` label used across the quality engine to the
/// tree-sitter [`Lang`] whose CC walker (`touring-code`) is actually accurate.
///
/// Only the five languages with a real branch-node grammar in
/// [`touring_code::ast::complexity`] are mapped (Python/Rust/TS/JS/Bash);
/// anything else returns `None` and the caller falls back to the keyword
/// heuristic. Go/Java/C/C++ are intentionally excluded — their tree-sitter
/// CC walker returns 1 for every function (no branch nodes wired), which would
/// be a silent regression versus the keyword estimate.
fn lang_for_cc(language: &str) -> Option<Lang> {
    match language {
        "rust" | "rs" => Some(Lang::Rust),
        "python" | "py" => Some(Lang::Python),
        "typescript" | "ts" | "tsx" => Some(Lang::TypeScript),
        "javascript" | "js" | "jsx" | "mjs" | "cjs" => Some(Lang::JavaScript),
        "bash" | "sh" | "shell" | "zsh" => Some(Lang::Bash),
        _ => None,
    }
}

/// Compute `(max_complexity, avg_complexity)` from the real per-function
/// tree-sitter CC walker, falling back to the keyword estimate when the
/// language is unsupported, parsing fails, or the file has no callable symbols.
///
/// `max_complexity` is the TRUE maximum McCabe CC over all callable symbols;
/// `avg_complexity` the true mean. Both are clamped to ≥1 / ≥1.0.
fn real_cc_or_fallback(
    source: &str,
    language: &str,
    fallback_max: usize,
    fallback_avg: f64,
) -> (usize, f64) {
    let Some(lang) = lang_for_cc(language) else {
        return (fallback_max, fallback_avg);
    };
    let Ok(ccs) = compute_complexity_for_source(source, lang) else {
        return (fallback_max, fallback_avg);
    };
    if ccs.is_empty() {
        return (fallback_max, fallback_avg);
    }
    let max = ccs.iter().map(|(_, cc)| *cc as usize).max().unwrap_or(1);
    let sum: usize = ccs.iter().map(|(_, cc)| *cc as usize).sum();
    let avg = sum as f64 / ccs.len() as f64;
    (max.max(1), avg.max(1.0))
}

/// Count source / comment / blank lines for a source buffer.
///
/// Returns `(sloc, cloc, blank)` where:
/// - `sloc` — lines with any non-whitespace content (includes comments)
/// - `cloc` — lines whose first non-whitespace token opens a comment
///   (`//`, `/*`, `*/`, `*` continuation, or `#` for Python-family)
/// - `blank` — fully empty lines
///
/// Conservative by design: a mixed line like `foo(); // note` counts as
/// `sloc=1 cloc=0`. Matches the Mozilla rust-code-analysis convention of
/// LLOC = SLOC - CLOC without double-counting mixed lines.
///
/// ## StringZilla SIMD (T1.1)
///
/// Line splitting is performed by [`RangeUtf8NewlineSplits`], which calls
/// `sz_utf8_find_newline` — a SWAR/SIMD routine that recognises `\n`,
/// `\r\n`, and all 7 Unicode newline codepoints.  Semantics are identical
/// to `str::lines()` for ASCII/UTF-8 source files; the iterator strips the
/// newline bytes and yields the bare line content as `&[u8]`.
fn count_lines(source: &str, language: &str) -> (usize, usize, usize) {
    let python_family = matches!(language, "python" | "py" | "bash" | "sh" | "ruby");
    let mut sloc = 0usize;
    let mut cloc = 0usize;
    let mut blank = 0usize;

    // `RangeUtf8NewlineSplits` emits one extra zero-length slice at the end
    // when the input ends with a newline.  `str::lines()` does NOT yield that
    // trailing entry, so we compensate by collecting into a peekable iterator
    // and skipping the last item when it is empty and the source ends with a
    // newline character.
    let bytes = source.as_bytes();
    let ends_with_newline = matches!(bytes.last(), Some(b'\n') | Some(b'\r'));
    let all_lines: Vec<&[u8]> = RangeUtf8NewlineSplits::new(bytes).collect();
    let line_count = all_lines.len();
    let effective_lines = if ends_with_newline && all_lines.last().map_or(false, |l| l.is_empty()) {
        // Drop the artifact trailing empty slice.
        line_count.saturating_sub(1)
    } else {
        line_count
    };

    for line_bytes in all_lines.iter().take(effective_lines) {
        // Skip any leading ASCII whitespace to find the first token.
        let trimmed = match line_bytes.iter().position(|b| !b.is_ascii_whitespace()) {
            Some(pos) => &line_bytes[pos..],
            None => {
                // Line is entirely whitespace or empty (genuine blank line).
                blank += 1;
                continue;
            }
        };

        sloc += 1;

        // Classify comment lines by their first non-whitespace token.
        let is_comment = if python_family {
            trimmed.first() == Some(&b'#')
        } else {
            // `//`, `/*`, `*/`, or `*` (block-comment continuation).
            matches!(
                (trimmed.first(), trimmed.get(1)),
                (Some(&b'/'), Some(&b'/'))
                    | (Some(&b'/'), Some(&b'*'))
                    | (Some(&b'*'), Some(&b'/'))
                    | (Some(&b'*'), _)
            )
        };

        if is_comment {
            cloc += 1;
        }
    }

    (sloc, cloc, blank)
}

/// Count explicit `return` statements (NEXITS proxy).
///
/// Byte-level scan — matches the same trade-off as the existing
/// `estimate_complexity` keyword scanner (no string/comment skipping).
/// Language-aware tokens:
/// - Rust / Go / C / C++ / Java / TS / JS: `return ` and `return;`
/// - Python: `return ` and standalone `return\n`
fn count_exits(source: &str, language: &str) -> usize {
    let bytes = source.as_bytes();
    let tokens: &[&[u8]] = match language {
        "python" | "py" => &[b"return ", b"return\n"],
        _ => &[b"return ", b"return;", b"return\n"],
    };
    tokens
        .iter()
        .map(|tok| memmem::find_iter(bytes, tok).count())
        .sum()
}

/// Estimate cognitive complexity by penalising nesting depth.
///
/// Single linear pass: tracks `{` / `}` to maintain `nesting_depth`,
/// adds `1 + (nesting_depth / 3)` per branching keyword encountered.
/// Language-agnostic keywords: `if`, `for`, `while`, `match`, `loop`, `else`.
///
/// Note: keyword scanning is byte-level and does not skip string literals
/// or comments — same trade-off as `estimate_complexity`.
pub fn estimate_cognitive_complexity(source: &str) -> usize {
    let mut score: usize = 0;
    let mut nesting_depth: usize = 0;
    let bytes = source.as_bytes();
    let mut i = 0;

    while let Some(&byte) = bytes.get(i) {
        match byte {
            b'{' => {
                nesting_depth = nesting_depth.saturating_add(1);
                i += 1;
            }
            b'}' => {
                nesting_depth = nesting_depth.saturating_sub(1);
                i += 1;
            }
            b'i' if matches!(bytes.get(i..i + 3), Some(b"if ") | Some(b"if(")) => {
                score += 1 + (nesting_depth / 3);
                i += 2;
            }
            b'f' if matches!(bytes.get(i..i + 4), Some(b"for ") | Some(b"for(")) => {
                score += 1 + (nesting_depth / 3);
                i += 3;
            }
            b'w' if matches!(bytes.get(i..i + 6), Some(b"while ") | Some(b"while(")) => {
                score += 1 + (nesting_depth / 3);
                i += 5;
            }
            b'm' if matches!(bytes.get(i..i + 6), Some(b"match ")) => {
                score += 1 + (nesting_depth / 3);
                i += 5;
            }
            b'l' if matches!(bytes.get(i..i + 5), Some(b"loop ") | Some(b"loop{")) => {
                score += 1 + (nesting_depth / 3);
                i += 4;
            }
            b'e' if matches!(bytes.get(i..i + 5), Some(b"else ") | Some(b"else{")) => {
                score += 1 + (nesting_depth / 3);
                i += 4;
            }
            _ => {
                i += 1;
            }
        }
    }

    score
}

/// Operator token inventory per language. Each entry is a distinct
/// operator in Halstead's sense; both keyword-operators (e.g. `if`, `for`)
/// and punctuation-operators (e.g. `+`, `==`, `=>`) are included.
///
/// Zero-alloc: returns `&'static [&'static [u8]]`.
fn operator_inventory(language: &str) -> &'static [&'static [u8]] {
    match language {
        "rust" => &[
            b"if ",
            b"else",
            b"match",
            b"for ",
            b"while",
            b"loop",
            b"return",
            b"break",
            b"continue",
            b"let ",
            b"fn ",
            b"struct",
            b"enum",
            b"trait",
            b"impl",
            b"mut ",
            b"pub ",
            b"use ",
            b"mod ",
            b"as ",
            b"?",
            b"=>",
            b"->",
            b"::",
            b"==",
            b"!=",
            b"<=",
            b">=",
            b"&&",
            b"||",
            b"+=",
            b"-=",
            b"*=",
            b"/=",
            b"=",
            b"+",
            b"-",
            b"*",
            b"/",
            b"%",
            b"!",
            b"&",
            b"|",
            b"^",
            b";",
            b",",
            b"(",
            b")",
            b"{",
            b"}",
            b"[",
            b"]",
            b"<",
            b">",
            b".",
        ],
        "python" | "py" => &[
            b"if ",
            b"elif",
            b"else",
            b"for ",
            b"while",
            b"return",
            b"break",
            b"continue",
            b"def ",
            b"class",
            b"import",
            b"from ",
            b"as ",
            b"lambda",
            b"yield",
            b"try",
            b"except",
            b"finally",
            b"with ",
            b"in ",
            b"is ",
            b"not ",
            b"and ",
            b"or ",
            b"==",
            b"!=",
            b"<=",
            b">=",
            b"//",
            b"**",
            b"+=",
            b"-=",
            b"*=",
            b"/=",
            b"=",
            b"+",
            b"-",
            b"*",
            b"/",
            b"%",
            b";",
            b",",
            b":",
            b"(",
            b")",
            b"[",
            b"]",
            b"<",
            b">",
            b".",
        ],
        "typescript" | "ts" | "javascript" | "js" => &[
            b"if ",
            b"else",
            b"for ",
            b"while",
            b"switch",
            b"case",
            b"return",
            b"break",
            b"continue",
            b"function",
            b"const",
            b"let ",
            b"var ",
            b"class",
            b"new ",
            b"typeof",
            b"instanceof",
            b"=>",
            b"===",
            b"!==",
            b"==",
            b"!=",
            b"<=",
            b">=",
            b"&&",
            b"||",
            b"??",
            b"+=",
            b"-=",
            b"*=",
            b"/=",
            b"=",
            b"+",
            b"-",
            b"*",
            b"/",
            b"%",
            b"!",
            b";",
            b",",
            b"(",
            b")",
            b"{",
            b"}",
            b"[",
            b"]",
            b"<",
            b">",
            b".",
        ],
        "go" => &[
            b"if ",
            b"else",
            b"for ",
            b"switch",
            b"case",
            b"return",
            b"break",
            b"continue",
            b"func ",
            b"type ",
            b"struct",
            b"interface",
            b"var ",
            b"const",
            b"go ",
            b"chan",
            b":=",
            b"==",
            b"!=",
            b"<=",
            b">=",
            b"&&",
            b"||",
            b"<-",
            b"+=",
            b"-=",
            b"*=",
            b"/=",
            b"=",
            b"+",
            b"-",
            b"*",
            b"/",
            b"%",
            b";",
            b",",
            b"(",
            b")",
            b"{",
            b"}",
            b"[",
            b"]",
            b"<",
            b">",
            b".",
        ],
        _ => &[
            b"if ", b"else", b"for ", b"while", b"return", b"==", b"!=", b"<=", b">=", b"&&",
            b"||", b"=", b"+", b"-", b"*", b"/", b"%", b";", b",", b"(", b")", b"{", b"}", b"[",
            b"]", b"<", b">", b".",
        ],
    }
}

/// Estimate Halstead software science metrics via byte-level scanning.
///
/// Approach (zero-dep, matches the same trade-off class as
/// `estimate_complexity`):
/// 1. **Operators** — fixed per-language inventory (`operator_inventory`).
///    `n1` = count of distinct tokens with ≥1 occurrence; `N1` = total
///    occurrences across all inventoried tokens.
/// 2. **Operands** — identifiers + numeric literals harvested by a
///    single-pass byte walk. Identifier tokens matching the operator
///    inventory's word entries are excluded to prevent double-counting.
///    `n2` = size of the distinct operand set; `N2` = total occurrences.
/// 3. Derived: `n = n1 + n2`, `N = N1 + N2`, `V = N * log2(n)`,
///    `D = (n1/2) * (N2/n2)`, `E = D * V`, `B = V / 3000`, `T = E / 18`.
///
/// Returns zeroed metrics for empty input or degenerate vocabulary
/// (avoids `log2(0)` / division-by-zero).
#[must_use]
pub fn estimate_halstead(source: &str, language: &str) -> HalsteadMetrics {
    if source.is_empty() {
        return HalsteadMetrics::default();
    }

    let bytes = source.as_bytes();
    let operators = operator_inventory(language);

    // Build a set of keyword-operators (alphabetic-only entries) to
    // suppress double-counting when we walk identifiers.
    let keyword_ops: HashSet<&[u8]> = operators
        .iter()
        .filter_map(|op| {
            let trimmed = trim_trailing_space(op);
            if trimmed
                .iter()
                .all(|b| b.is_ascii_alphabetic() || *b == b'_')
            {
                Some(trimmed)
            } else {
                None
            }
        })
        .collect();

    // Pass 1 — operators.
    let mut n1 = 0usize;
    let mut big_n1 = 0usize;
    for op in operators {
        let count = memmem::find_iter(bytes, op).count();
        if count > 0 {
            n1 += 1;
            big_n1 += count;
        }
    }

    // Pass 2 — operands (identifiers + integer literals).
    // Uses `.get()` throughout to satisfy `clippy::indexing_slicing`.
    let mut distinct_operands: HashSet<&[u8]> = HashSet::new();
    let mut big_n2 = 0usize;
    let mut i = 0usize;
    while let Some(&b) = bytes.get(i) {
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while let Some(&c) = bytes.get(i) {
                if c.is_ascii_alphanumeric() || c == b'_' {
                    i += 1;
                } else {
                    break;
                }
            }
            if let Some(token) = bytes.get(start..i)
                && !keyword_ops.contains(token)
            {
                distinct_operands.insert(token);
                big_n2 += 1;
            }
        } else if b.is_ascii_digit() {
            let start = i;
            while let Some(&c) = bytes.get(i) {
                if c.is_ascii_digit() || c == b'.' || c == b'_' {
                    i += 1;
                } else {
                    break;
                }
            }
            if let Some(token) = bytes.get(start..i) {
                distinct_operands.insert(token);
                big_n2 += 1;
            }
        } else {
            i += 1;
        }
    }
    let n2 = distinct_operands.len();

    let vocabulary = n1 + n2;
    let length = big_n1 + big_n2;

    if vocabulary == 0 || n2 == 0 {
        return HalsteadMetrics {
            n1,
            n2,
            big_n1,
            big_n2,
            vocabulary,
            length,
            ..HalsteadMetrics::default()
        };
    }

    let volume = (length as f64) * (vocabulary as f64).log2();
    let difficulty = ((n1 as f64) / 2.0) * ((big_n2 as f64) / (n2 as f64));
    let effort = difficulty * volume;
    let bugs = volume / 3000.0;
    let time_seconds = effort / 18.0;

    HalsteadMetrics {
        n1,
        n2,
        big_n1,
        big_n2,
        vocabulary,
        length,
        volume,
        difficulty,
        effort,
        bugs,
        time_seconds,
    }
}

/// Estimate the Maintainability Index (MI) from Halstead volume,
/// cyclomatic complexity, and logical lines of code.
///
/// Uses the SEI / Mozilla variant, normalized to `[0, 100]`:
///
/// ```text
/// MI = max(0, (171 − 5.2·ln(V) − 0.23·CC − 16.2·ln(LLOC)) · 100 / 171)
/// ```
///
/// Higher values indicate more maintainable code:
/// - **≥ 85**: highly maintainable (green)
/// - **65–85**: moderate (yellow)
/// - **< 65**: difficult to maintain (red)
///
/// Returns `0.0` for degenerate inputs (`volume ≤ 0.0` or `lloc == 0`)
/// to avoid `ln(0)` producing `-inf` contamination.
#[must_use]
pub fn estimate_maintainability_index(volume: f64, cyclomatic: usize, lloc: usize) -> f64 {
    if volume <= 0.0 || lloc == 0 {
        return 0.0;
    }
    let raw = 171.0 - 5.2 * volume.ln() - 0.23 * (cyclomatic as f64) - 16.2 * (lloc as f64).ln();
    (raw * 100.0 / 171.0).clamp(0.0, 100.0)
}

/// Strip a single trailing ASCII space from a byte slice literal. Used
/// to normalize operator-keyword inventory entries like `b"if "` → `b"if"`
/// for identifier-suppression lookups.
fn trim_trailing_space(op: &'static [u8]) -> &'static [u8] {
    match op.last() {
        Some(&b' ') => op.get(..op.len().saturating_sub(1)).unwrap_or(op),
        _ => op,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_rust_function() {
        let source = "fn foo() -> i32 { 42 }";
        let m = estimate_complexity(source, "rust");
        assert_eq!(m.function_count, 1);
        assert_eq!(m.max_complexity, 1);
    }

    #[test]
    fn test_branching_rust() {
        let source =
            "fn foo(x: i32) -> i32 { if x > 0 { if x > 10 { 100 } else { 50 } } else { 0 } }";
        let m = estimate_complexity(source, "rust");
        assert_eq!(m.function_count, 1);
        assert!(
            m.max_complexity >= 3,
            "should detect branches: {}",
            m.max_complexity
        );
    }

    #[test]
    fn test_max_cc_is_true_max_not_mean() {
        // Regression guard (W0 harness reform, 2026-07-02): one monster function
        // among many trivial ones MUST surface the monster's real CC as
        // `max_complexity` — the historical bug computed branches/functions (a
        // MEAN), diluting a CC-N function to ~1 and letting F1.1 score 1.000.
        let mut src = String::from(
            "fn monster(x: i32) -> i32 {\n\
                 if x > 1 { if x > 2 { if x > 3 { if x > 4 { if x > 5 { return 5; } } } } }\n\
                 for i in 0..x { if i % 2 == 0 {} else {} }\n\
                 match x { 1 => 1, 2 => 2, 3 => 3, _ => 0 }\n\
             }\n",
        );
        for i in 0..20 {
            src.push_str(&format!("fn trivial_{i}() {{}}\n"));
        }
        let m = estimate_complexity(&src, "rust");
        assert!(
            m.max_complexity >= 8,
            "true max CC of the monster must not be diluted by 20 trivial fns: got {}",
            m.max_complexity
        );
        assert!(
            m.avg_complexity < m.max_complexity as f64,
            "avg ({}) must be strictly below max ({}) when one fn dominates",
            m.avg_complexity,
            m.max_complexity
        );
    }

    #[test]
    fn test_python_complexity() {
        let source = "def foo(x):\n    if x:\n        for i in range(10):\n            pass\n    elif y:\n        pass";
        let m = estimate_complexity(source, "python");
        assert_eq!(m.function_count, 1);
        assert!(m.max_complexity >= 3);
    }

    #[test]
    fn test_empty_source() {
        let m = estimate_complexity("", "rust");
        assert_eq!(m.function_count, 0);
        assert_eq!(m.max_complexity, 1);
    }

    #[test]
    fn test_multiple_functions() {
        let source = "fn a() {}\nfn b() { if true {} }\nfn c() { for x in y {} }";
        let m = estimate_complexity(source, "rust");
        assert_eq!(m.function_count, 3);
    }

    #[test]
    fn test_symbol_count_includes_types() {
        let source = "struct Foo {}\nenum Bar {}\nfn baz() {}";
        let m = estimate_complexity(source, "rust");
        assert_eq!(m.symbol_count, 3); // 1 fn + 1 struct + 1 enum
    }

    #[test]
    fn test_java_no_overcount_field_declarations() {
        // Field declarations must NOT be counted as functions.
        // Only actual method signatures (ending with `) {`) should count.
        let source = r#"
public class Foo {
    private String name;
    public int x;
    protected boolean active;

    public Foo(String name) {
        this.name = name;
    }

    public String getName() {
        return name;
    }
}
"#;
        let m = estimate_complexity(source, "java");
        assert_eq!(
            m.function_count, 2,
            "should count constructor + getName, not field declarations: got {}",
            m.function_count
        );
    }

    #[test]
    fn test_java_constructor_counted() {
        let source = r#"
public class Bar {
    public Bar() {
        System.out.println("init");
    }
}
"#;
        let m = estimate_complexity(source, "java");
        assert_eq!(
            m.function_count, 1,
            "constructor `Bar() {{` should be counted: got {}",
            m.function_count
        );
    }

    #[test]
    fn test_cognitive_complexity_empty() {
        assert_eq!(estimate_cognitive_complexity(""), 0);
    }

    #[test]
    fn test_cognitive_complexity_no_keywords() {
        let source = "let x = 1; let y = 2; x + y";
        assert_eq!(estimate_cognitive_complexity(source), 0);
    }

    #[test]
    fn test_cognitive_flat_code_base_penalty() {
        // 3 branching keywords at nesting_depth=0: each scores 1+0/3=1 → total=3
        let source = "if a { } for x in y { } while z { }";
        let score = estimate_cognitive_complexity(source);
        assert_eq!(
            score, 3,
            "flat code: 3 keywords at depth 0 → 3, got {score}"
        );
    }

    #[test]
    fn test_cognitive_nested_scores_higher_than_flat() {
        let flat = "if a { } if b { } if c { }";
        let nested = "if a { for x in y { if b { if c { } } } }";
        let flat_score = estimate_cognitive_complexity(flat);
        let nested_score = estimate_cognitive_complexity(nested);
        assert!(
            nested_score > flat_score,
            "nested ({nested_score}) should score higher than flat ({flat_score})"
        );
    }

    #[test]
    fn test_cognitive_populated_by_estimate_complexity() {
        let source = "fn foo(x: i32) { if x > 0 { for i in 0..10 { if i > 5 { } } } }";
        let m = estimate_complexity(source, "rust");
        assert!(
            m.cognitive_complexity > 0,
            "nested code should have cognitive_complexity > 0, got {}",
            m.cognitive_complexity
        );
    }

    // ── Line metrics (SLOC / CLOC / LLOC) ─────────────────────────────────

    #[test]
    fn sloc_counts_only_nonblank_lines() {
        let source = "fn foo() {}\n\n\nfn bar() {}\n";
        let m = estimate_complexity(source, "rust");
        assert_eq!(m.sloc, 2, "two fn lines; 3 blanks are excluded");
    }

    #[test]
    fn cloc_counts_rust_line_and_block_comments() {
        let source = "// header\nfn foo() {}\n/* block */\n * continuation\n*/ tail\n";
        let m = estimate_complexity(source, "rust");
        // 5 non-blank lines: `// header`, `fn foo`, `/* block */`, ` * continuation`, `*/ tail`
        // comments: `// header`, `/* block */`, `* continuation`, `*/ tail` → 4
        assert_eq!(m.sloc, 5);
        assert_eq!(m.cloc, 4);
    }

    #[test]
    fn cloc_counts_python_hash_comments_not_rust() {
        let py = "# hdr\ndef f():\n    return 1\n";
        let m_py = estimate_complexity(py, "python");
        assert_eq!(m_py.cloc, 1, "# is a comment in python");

        let rs = "# hdr\nfn f() { 1 }\n";
        let m_rs = estimate_complexity(rs, "rust");
        assert_eq!(m_rs.cloc, 0, "# is NOT a comment in rust");
    }

    #[test]
    fn lloc_equals_sloc_minus_cloc() {
        let source = "// doc\nfn a() { 1 }\nfn b() { 2 }\n";
        let m = estimate_complexity(source, "rust");
        assert_eq!(m.lloc, m.sloc.saturating_sub(m.cloc));
        assert_eq!(
            m.lloc, 2,
            "2 real statements after excluding the doc comment"
        );
    }

    // ── NEXITS ────────────────────────────────────────────────────────────

    #[test]
    fn nexits_counts_explicit_returns() {
        let source = "fn early(x: i32) -> i32 {\n    if x < 0 { return -1; }\n    if x > 10 { return 10; }\n    return x;\n}\n";
        let m = estimate_complexity(source, "rust");
        assert_eq!(m.nexits, 3, "three explicit return statements");
    }

    #[test]
    fn nexits_is_zero_for_implicit_return() {
        let source = "fn implicit(x: i32) -> i32 { x + 1 }\n";
        let m = estimate_complexity(source, "rust");
        assert_eq!(m.nexits, 0, "no explicit return keyword present");
    }

    #[test]
    fn nexits_handles_python() {
        let source = "def f(x):\n    if x < 0:\n        return -1\n    return x\n";
        let m = estimate_complexity(source, "python");
        assert_eq!(m.nexits, 2);
    }

    // ── Regression / integration ──────────────────────────────────────────

    #[test]
    fn empty_source_yields_zero_line_metrics() {
        let m = estimate_complexity("", "rust");
        assert_eq!(m.sloc, 0);
        assert_eq!(m.cloc, 0);
        assert_eq!(m.lloc, 0);
        assert_eq!(m.nexits, 0);
    }

    // ── Maintainability Index (MI) ────────────────────────────────────────

    #[test]
    fn mi_degenerate_input_returns_zero() {
        assert_eq!(estimate_maintainability_index(0.0, 1, 10), 0.0);
        assert_eq!(estimate_maintainability_index(-1.0, 1, 10), 0.0);
        assert_eq!(estimate_maintainability_index(100.0, 1, 0), 0.0);
    }

    #[test]
    fn mi_clamps_to_zero_hundred_range() {
        // Extreme inputs must not escape [0, 100].
        let very_bad = estimate_maintainability_index(1e12, 10_000, 1_000_000);
        assert!(very_bad >= 0.0 && very_bad <= 100.0, "got {very_bad}");
        let tiny_good = estimate_maintainability_index(10.0, 1, 5);
        assert!(tiny_good >= 0.0 && tiny_good <= 100.0, "got {tiny_good}");
    }

    #[test]
    fn mi_monotonic_decreasing_in_cyclomatic() {
        // Fixed V and LLOC → higher CC should yield lower MI.
        let low = estimate_maintainability_index(500.0, 1, 50);
        let high = estimate_maintainability_index(500.0, 50, 50);
        assert!(
            low > high,
            "MI should decrease with CC: low_cc_mi={low:.2}, high_cc_mi={high:.2}"
        );
    }

    #[test]
    fn mi_populated_by_estimate_complexity() {
        let source = "fn foo(x: i32) -> i32 { if x > 0 { x + 1 } else { 0 } }";
        let m = estimate_complexity(source, "rust");
        assert!(
            m.maintainability_index > 0.0 && m.maintainability_index <= 100.0,
            "MI should be in [0, 100]: got {}",
            m.maintainability_index
        );
        // Tautology elided — `usize` is always `>= 0`; test below
        // (`blank_counts_empty_lines`) exercises the field's semantics.
        let _ = m.blank;
    }

    #[test]
    fn blank_counts_empty_lines() {
        let source = "fn a() {}\n\n\nfn b() {}\n";
        let m = estimate_complexity(source, "rust");
        assert_eq!(m.blank, 2, "two blank lines between the two fns");
    }

    // ── Halstead metrics ──────────────────────────────────────────────────

    #[test]
    fn halstead_empty_source_yields_default() {
        let h = estimate_halstead("", "rust");
        assert_eq!(h, HalsteadMetrics::default());
    }

    #[test]
    fn halstead_pure_literal_has_no_operators() {
        // No operators in the inventory occur → n1 == 0, N1 == 0.
        // Single operand `x` → n2 = 1, N2 = 1.
        let h = estimate_halstead("x", "rust");
        assert_eq!(h.n1, 0);
        assert_eq!(h.big_n1, 0);
        assert_eq!(h.n2, 1);
        assert_eq!(h.big_n2, 1);
        // Degenerate case: difficulty uses n1/2, so D=0, E=0, V well-defined.
        assert!(h.difficulty == 0.0);
    }

    #[test]
    fn halstead_rust_simple_fn_populates_all_fields() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let h = estimate_halstead(source, "rust");
        assert!(h.n1 > 0, "operators detected: n1={}", h.n1);
        assert!(h.n2 > 0, "operands detected: n2={}", h.n2);
        assert!(h.vocabulary == h.n1 + h.n2);
        assert!(h.length == h.big_n1 + h.big_n2);
        assert!(h.volume > 0.0, "V = N*log2(n) should be > 0");
        assert!(h.bugs > 0.0 && h.bugs < 0.1, "bugs = V/3000 for small fn");
        assert!(h.time_seconds > 0.0);
    }

    #[test]
    fn halstead_keyword_operators_not_double_counted_as_operands() {
        // `if`, `else`, `for` are operator-keywords — they MUST NOT appear
        // in the operand set. Distinct operands should only contain `x`,
        // `y`, `1`, `2`, `3`.
        let source = "if x { 1 } else { for y in 0..10 { 2 } 3 }";
        let h = estimate_halstead(source, "rust");
        assert!(h.n1 >= 3, "if/else/for should count as operators");
        // Operands distinct: {x, y, 0, 1, 2, 3, 10} — all non-keyword.
        assert!(
            h.n2 <= 10,
            "operand set should exclude keywords: n2={}",
            h.n2
        );
    }

    #[test]
    fn halstead_difficulty_scales_with_repeated_operands() {
        // More operand repetition → higher difficulty (D = n1/2 * N2/n2).
        let sparse = "let a = 1;";
        let dense = "let a = 1; let a = 1; let a = 1; let a = 1;";
        let hs = estimate_halstead(sparse, "rust");
        let hd = estimate_halstead(dense, "rust");
        assert!(
            hd.difficulty > hs.difficulty,
            "dense ({:.2}) should have D > sparse ({:.2})",
            hd.difficulty,
            hs.difficulty
        );
    }

    #[test]
    fn halstead_python_inventory_differs_from_rust() {
        // `def` is a Python keyword-operator, not a Rust one. Same source
        // analyzed as python should detect the operator; as rust it should
        // fall through to identifier.
        let source = "def foo(x): return x + 1";
        let hp = estimate_halstead(source, "python");
        let hr = estimate_halstead(source, "rust");
        assert!(hp.n1 >= hr.n1, "python inventory catches `def`/`return`");
    }

    #[test]
    fn halstead_populated_by_estimate_complexity() {
        let source = "fn foo(x: i32) -> i32 { if x > 0 { x + 1 } else { 0 } }";
        let m = estimate_complexity(source, "rust");
        assert!(
            m.halstead.volume > 0.0,
            "estimate_complexity must populate halstead.volume: got {}",
            m.halstead.volume
        );
        assert!(m.halstead.n1 > 0 && m.halstead.n2 > 0);
    }

    #[test]
    fn halstead_volume_monotonic_in_length() {
        let short = "fn a() { 1 }";
        let long = "fn a() { 1 } fn b() { 2 } fn c() { 3 } fn d() { 4 }";
        let hs = estimate_halstead(short, "rust");
        let hl = estimate_halstead(long, "rust");
        assert!(hl.length > hs.length);
        assert!(
            hl.volume > hs.volume,
            "longer program should have larger volume: short={:.2} long={:.2}",
            hs.volume,
            hl.volume
        );
    }

    #[test]
    fn mozilla_style_metrics_are_populated_alongside_legacy_fields() {
        // Single call must populate BOTH legacy (function_count,
        // max_complexity, cognitive_complexity) AND new Mozilla-style
        // (sloc/cloc/lloc/nexits) fields.
        let source = "// intro\nfn early(x: i32) -> i32 {\n    if x < 0 { return -1; }\n    x\n}\n";
        let m = estimate_complexity(source, "rust");
        assert!(m.function_count == 1, "legacy fn count still works");
        assert!(m.sloc >= 4, "sloc populated");
        assert!(m.cloc >= 1, "cloc populated");
        assert!(m.nexits == 1, "nexits populated");
        assert!(m.cognitive_complexity >= 1, "cognitive still works");
    }
}
