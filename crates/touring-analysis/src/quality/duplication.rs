//! Code duplication (D03) — Type-1 (exact, modulo whitespace) block clone detection.
//!
//! jscpd / SonarQube-CPD style: finds runs of `MIN_BLOCK_LINES`+ consecutive
//! *meaningful production* lines that recur verbatim elsewhere in the file, and
//! reports the duplicated-line ratio (target < 3%, the jscpd "healthy" threshold).
//!
//! Three precision levers the prior single-line substring stub lacked (the stub
//! counted every *isolated* line that recurred — so common idioms like `Ok(())`
//! or `let x = Vec::new();` were scored as "duplication" while real copy-paste
//! *blocks* were missed):
//!
//! 1. **Block, not line** — a clone is `MIN_BLOCK_LINES`+ consecutive meaningful
//!    lines recurring at a non-overlapping position; one repeated line is not debt.
//! 2. **Production-only, comment/blank/structural-aware** — comments and
//!    `#[cfg(test)]` regions (via `super::code_regions`, jscpd's test-exclusion
//!    convention) are dropped, as are blank and pure-bracket (`}`, `);`) lines
//!    that recur everywhere and are not duplication.
//! 3. **Content-keyed** — windows are keyed by their normalized text (whitespace
//!    collapsed), so there is no hash-collision false clone and indentation
//!    differences never hide a Type-1 clone.
//!
//! Zero non-std dependencies.

use std::collections::HashMap;

use super::code_regions::{non_executable_regions, offset_suppressed};

/// Minimum consecutive meaningful lines that constitute a clone block. jscpd's
/// default `min-lines` is 5; 6 is slightly more conservative to avoid trivial
/// false positives (precision over recall for an advisory dimension).
const MIN_BLOCK_LINES: usize = 6;

/// Duplication analysis for a source buffer.
#[derive(Debug, Clone, Default)]
pub struct DuplicationReport {
    /// Meaningful production lines considered (non-blank, non-structural,
    /// non-comment, non-`#[cfg(test)]`).
    pub total_meaningful_lines: usize,
    /// Lines covered by at least one clone block.
    pub duplicated_lines: usize,
    /// Number of distinct clone blocks (recurring windows) found.
    pub clone_blocks: usize,
    /// `duplicated_lines / total_meaningful_lines` in `[0, 1]`.
    pub ratio: f64,
}

/// A meaningful production line: its normalized text and original 0-based index.
struct Line {
    norm: String,
    #[allow(dead_code)]
    index: usize,
}

/// Collect meaningful production lines, tracking byte offsets correctly (so
/// `\r\n` files do not drift the comment/test region check).
fn collect_meaningful_lines(source: &str, regions: &[(usize, usize)]) -> Vec<Line> {
    let mut out = Vec::new();
    let mut line_start = 0usize;
    for (index, raw_line) in source.split_inclusive('\n').enumerate() {
        let content = raw_line.trim_end_matches('\n').trim_end_matches('\r');
        let trimmed = content.trim();
        let advance = raw_line.len();

        let keep = !trimmed.is_empty()
            // Skip pure structural lines (only brackets / punctuation).
            && !trimmed
                .chars()
                .all(|c| "{}()[];,".contains(c) || c.is_whitespace())
            // Skip comment / `#[cfg(test)]` regions: test the first non-ws byte.
            && {
                let lead_ws = content.len() - content.trim_start().len();
                !offset_suppressed(line_start + lead_ws, regions)
            };

        if keep {
            // Normalize: collapse internal whitespace runs to a single space.
            let norm = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
            out.push(Line { norm, index });
        }
        line_start += advance;
    }
    out
}

/// Analyze intra-file code duplication. `lang` selects the comment/string lexer
/// for `code_regions` (`"rust"`, `"python"`, `"typescript"`, `"go"`, …).
#[must_use]
pub fn analyze_duplication(source: &str, lang: &str) -> DuplicationReport {
    let regions = non_executable_regions(source, lang);
    let lines = collect_meaningful_lines(source, &regions);
    let n = lines.len();
    if n < MIN_BLOCK_LINES {
        return DuplicationReport {
            total_meaningful_lines: n,
            ..DuplicationReport::default()
        };
    }

    // Content-key each MIN_BLOCK_LINES window → its ascending start positions.
    let mut windows: HashMap<String, Vec<usize>> = HashMap::new();
    for i in 0..=(n - MIN_BLOCK_LINES) {
        let key = lines[i..i + MIN_BLOCK_LINES]
            .iter()
            .map(|l| l.norm.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        windows.entry(key).or_default().push(i);
    }

    let mut duplicated = vec![false; n];
    let mut clone_blocks = 0usize;
    for positions in windows.values() {
        if positions.len() < 2 {
            continue;
        }
        // A window matching itself shifted by < K (a single long identical run)
        // is not a clone. Require 2+ *non-overlapping* occurrences (greedy over
        // the ascending positions: take a position only once the previous taken
        // window has ended).
        let mut occ = 0usize;
        let mut next_free = 0usize;
        for &p in positions {
            if p >= next_free {
                occ += 1;
                next_free = p + MIN_BLOCK_LINES;
            }
        }
        if occ >= 2 {
            clone_blocks += 1;
            for &p in positions {
                for j in p..p + MIN_BLOCK_LINES {
                    if let Some(slot) = duplicated.get_mut(j) {
                        *slot = true;
                    }
                }
            }
        }
    }

    let duplicated_lines = duplicated.iter().filter(|&&d| d).count();
    let ratio = duplicated_lines as f64 / n as f64;
    DuplicationReport {
        total_meaningful_lines: n,
        duplicated_lines,
        clone_blocks,
        ratio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build N distinct non-trivial lines.
    fn distinct(n: usize) -> String {
        (0..n)
            .map(|i| format!("    let var_{i} = compute_value({i}) + offset_{i};\n"))
            .collect()
    }

    #[test]
    fn clean_code_has_no_duplication() {
        let src = format!("fn f() {{\n{}}}\n", distinct(20));
        let r = analyze_duplication(&src, "rust");
        assert_eq!(r.duplicated_lines, 0);
        assert_eq!(r.ratio, 0.0);
    }

    #[test]
    fn copy_pasted_block_is_detected() {
        // A 7-line meaningful block appears twice, separated by other code.
        let block = "    let a = step_one(input);\n    let b = step_two(a);\n    \
                     let c = step_three(b);\n    let d = step_four(c);\n    \
                     let e = step_five(d);\n    let g = step_six(e);\n    let h = step_seven(g);\n";
        let src = format!(
            "fn first() {{\n{block}}}\nfn middle() {{\n{}}}\nfn second() {{\n{block}}}\n",
            distinct(10)
        );
        let r = analyze_duplication(&src, "rust");
        assert!(
            r.clone_blocks >= 1,
            "the repeated 7-line block must be a clone"
        );
        assert!(r.duplicated_lines >= 7, "got {}", r.duplicated_lines);
        assert!(r.ratio > 0.0);
    }

    #[test]
    fn isolated_repeated_idiom_is_not_duplication() {
        // `Ok(())` and a couple common lines recur but never as a 6-line block.
        let src = "fn a() -> Result<(), E> {\n    do_a()?;\n    Ok(())\n}\n\
                   fn b() -> Result<(), E> {\n    do_b()?;\n    Ok(())\n}\n\
                   fn c() -> Result<(), E> {\n    do_c()?;\n    Ok(())\n}\n";
        let r = analyze_duplication(src, "rust");
        assert_eq!(
            r.duplicated_lines, 0,
            "isolated idioms are not block clones"
        );
    }

    #[test]
    fn structural_and_blank_lines_ignored() {
        // Many `}` and blank lines repeat but must never form a clone.
        let src = "fn a() {\n}\n\nfn b() {\n}\n\nfn c() {\n}\n\nfn d() {\n}\n\nfn e() {\n}\n";
        let r = analyze_duplication(src, "rust");
        assert_eq!(r.duplicated_lines, 0);
    }

    #[test]
    fn comments_excluded_from_duplication() {
        // An identical 6-line comment block repeated is documentation, not a clone.
        let comment_block = "// line one of the note\n// line two of the note\n\
                             // line three of the note\n// line four of the note\n\
                             // line five of the note\n// line six of the note\n";
        let src = format!("fn a() {{}}\n{comment_block}fn b() {{}}\n{comment_block}fn c() {{}}\n");
        let r = analyze_duplication(&src, "rust");
        assert_eq!(
            r.duplicated_lines, 0,
            "repeated comment blocks are not code clones"
        );
    }

    #[test]
    fn cfg_test_duplication_excluded() {
        // Repetitive test setup inside #[cfg(test)] is not production duplication.
        let block = "    let a = step_one(input);\n    let b = step_two(a);\n    \
                     let c = step_three(b);\n    let d = step_four(c);\n    \
                     let e = step_five(d);\n    let g = step_six(e);\n";
        let src = format!(
            "fn prod() {{ ok() }}\n#[cfg(test)]\nmod tests {{\nfn t1() {{\n{block}}}\nfn t2() {{\n{block}}}\n}}\n"
        );
        let r = analyze_duplication(&src, "rust");
        assert_eq!(r.duplicated_lines, 0, "test-region duplication is excluded");
    }

    #[test]
    fn empty_and_tiny_source() {
        assert_eq!(analyze_duplication("", "rust").ratio, 0.0);
        assert_eq!(
            analyze_duplication("fn a() {}\n", "rust").duplicated_lines,
            0
        );
    }

    #[test]
    fn ratio_is_bounded() {
        let block = distinct(6);
        let src = format!("fn a() {{\n{block}}}\nfn b() {{\n{block}}}\n");
        let r = analyze_duplication(&src, "rust");
        assert!((0.0..=1.0).contains(&r.ratio));
        assert!(r.ratio > 0.0, "two identical 6-line blocks must register");
    }
}
