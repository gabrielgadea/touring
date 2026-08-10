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
//! **Type-2 (2026-08-08, A1)** — Type-1 only sees clones that survived
//! copy-paste *verbatim*. Rename one variable and the block becomes invisible to
//! it, which means the reported ratio has always been a LOWER BOUND presented as
//! a measurement. [`analyze_duplication`] now also runs a token-normalized pass
//! (identifiers/literals abstracted away) in two stages: exact recurrence of the
//! normalized form, then MinHash+LSH candidate generation verified by exact
//! Jaccard for the gapped cases. Both are reported separately from Type-1 and
//! the near pass declares itself when a size budget makes it skip.
//!
//! Zero non-std dependencies for Type-1; the near pass reuses
//! `touring_simd::similarity` (MinHash for candidates, Jaccard for the verdict).

use std::collections::HashMap;

use touring_simd::similarity::{JaccardComputer, JaccardSimilarity, MinHasher, Signature, band_keys};

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
    /// Lines covered by at least one **Type-1** (verbatim) clone block.
    pub duplicated_lines: usize,
    /// Number of distinct Type-1 clone blocks (recurring windows) found.
    pub clone_blocks: usize,
    /// `duplicated_lines / total_meaningful_lines` in `[0, 1]` — Type-1 only.
    pub ratio: f64,
    /// Type-2 clone **regions**: maximal contiguous spans of lines covered by a
    /// clone that is identical once identifiers and literals are abstracted
    /// away (renamed copy-paste), or near-identical at Jaccard ≥
    /// [`TYPE2_JACCARD_THRESHOLD`].
    ///
    /// Regions, not window forms — unlike [`Self::clone_blocks`]. A single
    /// copy-pasted function yields one region but a dozen distinct normalized
    /// windows (the same span seen at each shift), and reporting the windows
    /// made a handful of real clones look like hundreds. The two fields are
    /// deliberately not symmetric; the count that misleads is not worth the
    /// symmetry.
    pub type2_clone_regions: usize,
    /// Lines a Type-2 clone covers that Type-1 did **not** already cover — the
    /// duplication that was previously invisible. Kept separate so the two
    /// numbers are never silently summed into one another.
    pub type2_only_lines: usize,
    /// `(duplicated_lines + type2_only_lines) / total_meaningful_lines`.
    pub combined_ratio: f64,
    /// `Some(reason)` when the MinHash/LSH near-duplicate stage did not run.
    /// The exact Type-2 stage always runs; only the gapped one has a budget.
    /// A skipped stage must be visible, or `type2_*` reads as "none found"
    /// when it means "not looked for".
    pub near_pass_skipped: Option<&'static str>,
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

// ── Type-2: token-normalized clone detection (A1, 2026-08-08) ───────────────

/// Exact Jaccard a candidate pair must reach to be called a gapped clone.
///
/// Chosen from the arithmetic, not from taste. With `SHINGLE_K = 4`, each
/// changed token destroys up to 4 shingles on each side, so a window of `m`
/// shingles with `d` destroyed has `J = (m - d) / (m + d)`. Over the
/// [`NEAR_BLOCK_LINES`]-line window (~117 shingles) that puts the tolerated
/// edit at ~4 tokens: J = 0.934 at 1 token, 0.872 at 2, 0.708 at 5.
///
/// The same 0.75 over a 6-line window would tolerate barely ONE token
/// (J = 0.869 at 1, 0.754 at 2) — which is why the gapped stage uses a longer
/// window than the exact one. A stricter 0.85 there would have made this whole
/// stage redundant with the exact-recurrence stage, i.e. dead machinery.
pub const TYPE2_JACCARD_THRESHOLD: f64 = 0.75;

/// Window length for the gapped (MinHash/LSH) stage.
///
/// Longer than [`MIN_BLOCK_LINES`] on purpose: a gapped clone is a claim about
/// two blocks being *mostly* the same, and "mostly" is only meaningful with
/// enough content for one edit to be a small fraction of it. See
/// [`TYPE2_JACCARD_THRESHOLD`] for the arithmetic.
const NEAR_BLOCK_LINES: usize = 12;

/// Tokens per shingle. 4 is short enough that a one-token edit destroys only a
/// few shingles, long enough that common punctuation runs are not universal.
const SHINGLE_K: usize = 4;

/// Distinct windows above which the MinHash/LSH stage is skipped and says so.
/// The exact Type-2 stage is O(n) and always runs.
const MAX_NEAR_WINDOWS: usize = 50_000;

/// Distinct tokens a window must contain to be eligible as a clone.
///
/// Calibrated against the workspace, not chosen a priori. Without it the
/// combined ratio came out at 31–56% per crate — a claim that half the code is
/// duplicated, which is not actionable and therefore not a measurement worth
/// publishing. The cause is structural: once identifiers collapse to `$I`, six
/// consecutive `let x = f(y);` lines become ONE token stream
/// (`let $I = $I ( $I ) ;` ×6, six distinct tokens in total) and match every
/// other such run in the corpus.
///
/// Such a window carries too little self-information to be evidence of
/// copy-paste — the shapes coincide because the language has few shapes, not
/// because anyone copied anything. Windows below the floor are excluded from
/// BOTH Type-2 stages, and the exclusion is a real loss of recall: a genuine
/// renamed clone made only of trivial assignments is indistinguishable from
/// idiom and is not reported.
const MIN_DISTINCT_TOKENS: usize = 16;

/// Members of one LSH bucket that are paired up. A degenerate bucket would
/// otherwise be quadratic; the truncation is surfaced via `near_pass_skipped`.
const MAX_BUCKET_MEMBERS: usize = 64;

/// Keywords kept verbatim during normalization — the union across Rust,
/// Python, TypeScript/JS and Go.
///
/// Abstracting these away would make `if a { x }` and `while b { y }`
/// indistinguishable, which manufactures clones out of ordinary control flow.
/// The union (rather than a per-language set) is deliberately conservative:
/// keeping a word verbatim can only ever *reduce* the number of reported
/// clones, so a Go keyword surviving in Rust costs recall, never precision.
const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "case", "catch", "chan", "class", "const", "continue",
    "def", "default", "defer", "del", "do", "elif", "else", "enum", "except", "extends", "false",
    "final", "finally", "fn", "for", "from", "func", "function", "go", "if", "impl", "import",
    "in", "interface", "is", "lambda", "let", "loop", "map", "match", "mod", "move", "mut", "new",
    "nil", "none", "not", "or", "package", "pass", "pub", "range", "raise", "ref", "return",
    "select", "self", "static", "struct", "super", "switch", "trait", "true", "try", "type",
    "unsafe", "use", "var", "where", "while", "with", "yield",
];

/// Rewrites one line into its Type-2 canonical token stream.
///
/// Identifiers become `$I`, numbers `$N`, string/char literals `$S`; keywords,
/// operators and punctuation survive verbatim. Two blocks that differ only by
/// renaming produce byte-identical output.
fn normalize_line_tokens(line: &str, out: &mut Vec<String>) {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'"' || c == b'\'' {
            // String/char literal: consume to the matching quote, honouring
            // backslash escapes so `"a\"b"` is one token, not two.
            let quote = c;
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                i += if bytes[i] == b'\\' { 2 } else { 1 };
            }
            i = (i + 1).min(bytes.len());
            out.push("$S".to_string());
        } else if c.is_ascii_digit() {
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'_')
            {
                i += 1;
            }
            out.push("$N".to_string());
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = line.get(start..i).unwrap_or_default();
            if KEYWORDS.contains(&word) {
                out.push(word.to_string());
            } else {
                out.push("$I".to_string());
            }
        } else {
            out.push((c as char).to_string());
            i += 1;
        }
    }
}

/// FNV-1a over a token — the shingle alphabet.
fn token_hash(token: &str) -> u32 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in token.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    (h ^ (h >> 32)) as u32
}

/// 128-bit content key for a window's normalized token stream.
///
/// Type-1 keys windows by their full text specifically to rule out
/// hash-collision false clones. Here the same guarantee is bought with two
/// independent 64-bit hashes: a collision needs ~2⁶⁴ windows, versus the ~10⁵
/// a large corpus produces, and the key costs 16 bytes instead of ~300 — the
/// difference between adding a bounded cost to this analysis and doubling its
/// peak memory.
fn window_key(tokens: &[u32]) -> (u64, u64) {
    let (mut a, mut b) = (0xcbf2_9ce4_8422_2325u64, 0x9E37_79B9_7F4A_7C15u64);
    for &t in tokens {
        a ^= u64::from(t);
        a = a.wrapping_mul(0x0000_0100_0000_01B3);
        b = b.rotate_left(7) ^ u64::from(t).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    }
    (a, b)
}

/// Sorted, deduped `SHINGLE_K`-gram set for one window's token stream.
fn shingles(tokens: &[u32]) -> Vec<u32> {
    if tokens.len() < SHINGLE_K {
        // Too short to shingle: fall back to the tokens themselves so a tiny
        // window still has a comparable (if coarse) set rather than an empty
        // one, which would bucket with every other empty set.
        let mut v = tokens.to_vec();
        v.sort_unstable();
        v.dedup();
        return v;
    }
    let mut v: Vec<u32> = tokens
        .windows(SHINGLE_K)
        .map(|w| {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for &t in w {
                h ^= u64::from(t);
                h = h.wrapping_mul(0x0000_0100_0000_01B3);
            }
            (h ^ (h >> 32)) as u32
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Result of the Type-2 passes over one corpus.
struct Type2Outcome {
    /// Whether ANY clone was confirmed — the region count is derived from
    /// `covered` by the caller, which owns the Type-1 vector it is diffed with.
    any: bool,
    /// Per meaningful line: covered by some Type-2 clone.
    covered: Vec<bool>,
    skipped: Option<&'static str>,
}

/// Marks `[start, start+len)` as covered.
fn mark_span(covered: &mut [bool], start: usize, len: usize) {
    for slot in covered.iter_mut().skip(start).take(len) {
        *slot = true;
    }
}

/// Two stages over the token-normalized windows: exact recurrence of the
/// normalized form (renamed copy-paste), then MinHash+LSH candidates verified
/// by exact Jaccard (gapped copy-paste).
fn analyze_type2(lines: &[Line]) -> Type2Outcome {
    let n = lines.len();
    let mut covered = vec![false; n];
    if n < MIN_BLOCK_LINES {
        return Type2Outcome { any: false, covered, skipped: None };
    }

    // Token-normalize once per line; windows reuse the slices.
    let per_line: Vec<Vec<u32>> = lines
        .iter()
        .map(|l| {
            let mut toks = Vec::new();
            normalize_line_tokens(&l.norm, &mut toks);
            toks.iter().map(|t| token_hash(t)).collect()
        })
        .collect();

    let window_tokens = |start: usize| -> Vec<u32> {
        per_line
            .get(start..start + MIN_BLOCK_LINES)
            .map(|ls| ls.iter().flatten().copied().collect())
            .unwrap_or_default()
    };

    // ── Stage 1: exact recurrence of the normalized form ────────────────────
    // Distinct-token count of a window — the entropy gate (see
    // MIN_DISTINCT_TOKENS). Cheap: the token hashes are already computed.
    let distinct_tokens = |toks: &[u32]| -> usize {
        let mut v = toks.to_vec();
        v.sort_unstable();
        v.dedup();
        v.len()
    };

    let mut by_key: HashMap<(u64, u64), Vec<usize>> = HashMap::new();
    for i in 0..=(n - MIN_BLOCK_LINES) {
        let toks = window_tokens(i);
        if distinct_tokens(&toks) < MIN_DISTINCT_TOKENS {
            continue;
        }
        by_key.entry(window_key(&toks)).or_default().push(i);
    }

    let mut found = false;
    for positions in by_key.values() {
        if positions.len() < 2 {
            continue;
        }
        // Same non-overlap rule as Type-1: a long uniform run is one block, not
        // a clone of itself shifted by one line.
        let mut occ = 0usize;
        let mut next_free = 0usize;
        for &p in positions {
            if p >= next_free {
                occ += 1;
                next_free = p + MIN_BLOCK_LINES;
            }
        }
        if occ >= 2 {
            found = true;
            for &p in positions {
                mark_span(&mut covered, p, MIN_BLOCK_LINES);
            }
        }
    }

    // ── Stage 2: gapped clones via MinHash + LSH, verified exactly ──────────
    // Its own, longer window (see NEAR_BLOCK_LINES) and its own dedupe: one
    // representative per distinct normalized form, since identical forms were
    // already settled by stage 1 and deduping is what keeps buckets small.
    if n < NEAR_BLOCK_LINES {
        return Type2Outcome { any: found, covered, skipped: None };
    }
    let near_tokens = |start: usize| -> Vec<u32> {
        per_line
            .get(start..start + NEAR_BLOCK_LINES)
            .map(|ls| ls.iter().flatten().copied().collect())
            .unwrap_or_default()
    };
    let mut near_by_key: HashMap<(u64, u64), usize> = HashMap::new();
    let mut near_cache: HashMap<usize, Vec<u32>> = HashMap::new();
    for i in 0..=(n - NEAR_BLOCK_LINES) {
        let toks = near_tokens(i);
        if distinct_tokens(&toks) < MIN_DISTINCT_TOKENS {
            continue;
        }
        if let std::collections::hash_map::Entry::Vacant(slot) =
            near_by_key.entry(window_key(&toks))
        {
            slot.insert(i);
            near_cache.insert(i, toks);
        }
    }
    let representatives: Vec<usize> = near_by_key.values().copied().collect();
    if representatives.len() > MAX_NEAR_WINDOWS {
        return Type2Outcome {
            any: found,
            covered,
            skipped: Some("near-duplicate pass skipped: distinct windows exceed the 50k budget — score by crate for gapped-clone coverage"),
        };
    }

    let hasher = MinHasher::new();
    let jaccard = JaccardComputer::new();
    let mut buckets: HashMap<(usize, u64), Vec<usize>> = HashMap::new();
    let mut sets: HashMap<usize, Vec<u32>> = HashMap::new();
    let mut sigs: HashMap<usize, Signature> = HashMap::new();
    for &rep in &representatives {
        let sh = shingles(near_cache.get(&rep).map(Vec::as_slice).unwrap_or_default());
        if sh.is_empty() {
            continue;
        }
        let sig = hasher.signature(&sh);
        for (band, key) in band_keys(&sig).into_iter().enumerate() {
            buckets.entry((band, key)).or_default().push(rep);
        }
        sigs.insert(rep, sig);
        sets.insert(rep, sh);
    }

    /// Slack below the threshold that the MinHash estimate must clear before a
    /// pair is worth an exact comparison. The estimate has standard error
    /// `sqrt(t(1-t)/32)` ≈ 0.075 at t = 0.75, so three sigma is ≈ 0.22 — a pair
    /// whose estimate falls below `threshold - 0.22` is virtually never
    /// confirmed, and skipping it is a cost saving, not a second verdict.
    const ESTIMATE_SLACK: f64 = 0.22;

    let mut truncated_bucket = false;
    let mut confirmed: Vec<(usize, usize)> = Vec::new();
    for members in buckets.values() {
        let members = if members.len() > MAX_BUCKET_MEMBERS {
            truncated_bucket = true;
            members.get(..MAX_BUCKET_MEMBERS).unwrap_or(members)
        } else {
            members.as_slice()
        };
        for (idx, &a) in members.iter().enumerate() {
            for &b in members.iter().skip(idx + 1) {
                let (Some(sa), Some(sb)) = (sets.get(&a), sets.get(&b)) else {
                    continue;
                };
                // Cheap 32-comparison reject first (see ESTIMATE_SLACK); the
                // verdict is still exact, never the estimate.
                if let (Some(ga), Some(gb)) = (sigs.get(&a), sigs.get(&b))
                    && MinHasher::estimate(ga, gb) < TYPE2_JACCARD_THRESHOLD - ESTIMATE_SLACK
                {
                    continue;
                }
                if jaccard.jaccard(sa, sb) >= TYPE2_JACCARD_THRESHOLD {
                    confirmed.push((a.min(b), a.max(b)));
                }
            }
        }
    }
    confirmed.sort_unstable();
    confirmed.dedup();
    for &(a, b) in &confirmed {
        // Overlapping windows of one long block are not two clones.
        if b.saturating_sub(a) < NEAR_BLOCK_LINES {
            continue;
        }
        found = true;
        mark_span(&mut covered, a, NEAR_BLOCK_LINES);
        mark_span(&mut covered, b, NEAR_BLOCK_LINES);
    }

    Type2Outcome {
        any: found,
        covered,
        skipped: truncated_bucket.then_some(
            "near-duplicate pass capped one or more LSH buckets at 64 members — some gapped pairs were not scored",
        ),
    }
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

    // Type-2 runs over the SAME meaningful lines, so the two coverage vectors
    // are index-aligned and `type2_only` is a true set difference — never an
    // approximation of one.
    let t2 = analyze_type2(&lines);
    let type2_only_lines = t2
        .covered
        .iter()
        .zip(duplicated.iter())
        .filter(|(t2c, t1c)| **t2c && !**t1c)
        .count();
    // One region per maximal contiguous run of Type-2-covered lines.
    let mut type2_clone_regions = 0usize;
    let mut inside = false;
    for &c in &t2.covered {
        if c && !inside {
            type2_clone_regions += 1;
        }
        inside = c;
    }
    debug_assert!(
        t2.any || type2_clone_regions == 0,
        "coverage without a confirmed clone means a window was marked by accident"
    );

    DuplicationReport {
        total_meaningful_lines: n,
        duplicated_lines,
        clone_blocks,
        ratio,
        type2_clone_regions,
        type2_only_lines,
        combined_ratio: (duplicated_lines + type2_only_lines) as f64 / n as f64,
        near_pass_skipped: t2.skipped,
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

    // ── Type-2 (A1, 2026-08-08) ──────────────────────────────────────────────

    /// A structurally varied 8-line block, parameterised by an identifier
    /// prefix so a "renamed" copy is a true rename.
    ///
    /// Variety is not decoration: [`MIN_DISTINCT_TOKENS`] deliberately excludes
    /// windows made of one repeated shape, so a fixture built from eight
    /// identical `let x = f(y);` lines would exercise the floor rather than the
    /// detector. Real copy-paste has branches, loops and literals — this
    /// mirrors that.
    fn varied_block(p: &str) -> String {
        format!(
            "    let {p}_total = {p}_input.len();\n\
             if {p}_total == 0 {{ return {p}_empty(); }}\n\
             let mut {p}_acc = Vec::with_capacity({p}_total);\n\
             for {p}_item in {p}_input {{ {p}_acc.push({p}_item * 3); }}\n\
             let {p}_label = match {p}_total {{ 1 => \"one\", _ => \"many\" }};\n\
             {p}_report({p}_label, {p}_acc.len());\n\
             while {p}_acc.len() > 4 {{ {p}_acc.pop(); }}\n\
             {p}_finish({p}_acc, {p}_total)\n"
        )
    }


    #[test]
    fn renamed_copy_paste_is_invisible_to_type1_and_caught_by_type2() {
        // The exact gap A1 exists to close: same block, every identifier
        // renamed. Type-1 sees two unrelated blocks.
        let src = format!(
            "fn f() {{\n{}}}\nfn g() {{\n{}}}\n",
            varied_block("alpha"),
            varied_block("omega")
        );
        let r = analyze_duplication(&src, "rust");
        assert_eq!(r.duplicated_lines, 0, "Type-1 must NOT see a renamed clone");
        assert!(r.type2_clone_regions >= 1, "Type-2 must see it");
        assert!(r.type2_only_lines >= 12, "got {}", r.type2_only_lines);
        assert!(r.combined_ratio > r.ratio);
    }

    #[test]
    fn a_type1_clone_is_never_counted_twice() {
        // A verbatim clone is also token-identical, so both passes cover it.
        // `type2_only_lines` is a set DIFFERENCE — those lines belong to Type-1.
        let block = varied_block("same");
        let src = format!("fn first() {{\n{block}}}\nfn second() {{\n{block}}}\n");
        let r = analyze_duplication(&src, "rust");
        assert!(r.duplicated_lines >= 16, "got {}", r.duplicated_lines);
        // The two `fn first()` / `fn second()` headers are the only lines
        // Type-2 can reach and Type-1 cannot: they differ verbatim and coincide
        // once names are abstracted. Anything more would mean the two passes
        // are measuring the same lines twice.
        assert!(r.type2_only_lines <= 2, "got {}", r.type2_only_lines);
        assert!(r.combined_ratio >= r.ratio);
    }

    #[test]
    fn control_flow_keywords_are_not_abstracted_away() {
        // If `if`/`while`/`match` collapsed into $I, every branch would clone
        // every loop. Six lines of each, differing ONLY in the keyword.
        let ifs: String = (0..6)
            .map(|i| format!("    if cond_{i} {{ act_{i}(); }}\n"))
            .collect();
        let whiles: String = (0..6)
            .map(|i| format!("    while cond_{i} {{ act_{i}(); }}\n"))
            .collect();
        let src = format!("fn f() {{\n{ifs}}}\nfn g() {{\n{whiles}}}\n");
        let r = analyze_duplication(&src, "rust");
        assert_eq!(
            r.type2_clone_regions, 0,
            "an if-chain and a while-chain are not the same block"
        );
    }

    #[test]
    fn literals_of_different_shape_still_normalize_together() {
        // Type-2 abstracts literals but keeps their KIND: two different numbers
        // are both `$N` and must match…
        let a = varied_block("first").replace("* 3", "* 7").replace("> 4", "> 9");
        let b = varied_block("second").replace("* 3", "* 11").replace("> 4", "> 2");
        let r = analyze_duplication(&format!("fn f() {{\n{a}}}\nfn g() {{\n{b}}}\n"), "rust");
        assert!(r.type2_clone_regions >= 1, "different numbers are both $N");

        // …while a number and a string are different tokens and must not.
        let c = varied_block("third").replace("* 3", "* \"x\"");
        let r2 = analyze_duplication(&format!("fn f() {{\n{a}}}\nfn h() {{\n{c}}}\n"), "rust");
        assert!(
            r2.type2_only_lines < r.type2_only_lines,
            "$N vs $S must reduce the match ({} vs {})",
            r2.type2_only_lines,
            r.type2_only_lines
        );
    }

    #[test]
    fn a_gapped_clone_survives_a_small_edit() {
        // Two structurally varied 16-line blocks whose copy drops an argument
        // in two places — no run of 6 verbatim-identical lines survives, so
        // Type-1 is blind and only the MinHash/LSH stage can see it.
        let base = format!("{}{}", varied_block("aa"), varied_block("bb"));
        let copy = format!("{}{}", varied_block("cc"), varied_block("dd"))
            .replace("cc_report(cc_label, cc_acc.len())", "cc_report(cc_label)")
            .replace("dd_finish(dd_acc, dd_total)", "dd_finish(dd_acc)");
        let src = format!("fn f() {{\n{base}}}\nfn g() {{\n{copy}}}\n");
        let r = analyze_duplication(&src, "rust");
        assert!(
            r.type2_clone_regions >= 1,
            "a 16-line block with two dropped arguments is a gapped clone"
        );
    }

    #[test]
    fn structurally_varied_code_has_no_type2_duplication() {
        // NOTE: `distinct(n)` is Type-1-clean but token-IDENTICAL line to line
        // (`let $I = $I ( $N ) + $I ;` ×20), so it is Type-2 duplication by
        // construction — using it here would have asserted the opposite of the
        // truth. Real structural variety is what must score zero.
        let src = "fn f(input: &[u32]) -> u32 {\n\
                   let total: u32 = input.iter().sum();\n\
                   if total == 0 { return 0; }\n\
                   let mut seen = std::collections::HashSet::new();\n\
                   for value in input { seen.insert(value % 7); }\n\
                   let spread = seen.len() as u32;\n\
                   match spread { 0 => total, 1..=3 => total / spread, _ => total.saturating_sub(spread) }\n\
                   }\n";
        let r = analyze_duplication(src, "rust");
        assert_eq!(r.type2_only_lines, 0, "varied code is not a clone of itself");
        assert_eq!(r.combined_ratio, 0.0);
        assert!(r.near_pass_skipped.is_none(), "a small file must be fully measured");
    }

    #[test]
    fn a_run_of_identically_shaped_lines_is_below_the_entropy_floor() {
        // The recall this design KNOWINGLY trades away. Twenty assignments
        // differing only in their index normalize to one token stream of six
        // distinct tokens — indistinguishable from ordinary idiom, and matching
        // every other such run in a corpus. Reporting it drove the workspace
        // combined ratio to 31–56%, a number no one can act on. The floor
        // suppresses it; this test exists so the trade is deliberate and
        // visible rather than rediscovered as a "bug".
        let r = analyze_duplication(&format!("fn f() {{\n{}}}\n", distinct(20)), "rust");
        assert_eq!(r.duplicated_lines, 0, "Type-1 sees nothing here");
        assert_eq!(
            r.type2_only_lines, 0,
            "below MIN_DISTINCT_TOKENS — idiom, not evidence of copy-paste"
        );
    }

    #[test]
    fn comments_and_test_regions_stay_excluded_from_type2() {
        // Type-2 runs over the SAME filtered lines, so every Type-1 exclusion
        // must hold — a renamed clone inside #[cfg(test)] is still not debt.
        let a = "    let alpha = step_one(input);\n    let beta = step_two(alpha);\n    \
                 let gamma = step_three(beta);\n    let delta = step_four(gamma);\n    \
                 let epsilon = step_five(delta);\n    let zeta = step_six(epsilon);\n";
        let b = "    let one = first(src);\n    let two = second(one);\n    \
                 let three = third(two);\n    let four = fourth(three);\n    \
                 let five = fifth(four);\n    let six = sixth(five);\n";
        let src = format!(
            "fn prod() {{ ok() }}\n#[cfg(test)]\nmod tests {{\nfn t1() {{\n{a}}}\nfn t2() {{\n{b}}}\n}}\n"
        );
        let r = analyze_duplication(&src, "rust");
        assert_eq!(r.type2_only_lines, 0, "test-region clones are not production debt");
    }

    #[test]
    fn combined_ratio_never_falls_below_the_type1_ratio() {
        // Structural invariant: Type-2 can only ADD coverage. If this ever
        // inverts, the two coverage vectors have drifted out of alignment.
        for src in [
            String::new(),
            "fn a() {}\n".to_string(),
            format!("fn f() {{\n{}}}\n", distinct(30)),
            format!("fn f() {{\n{b}}}\nfn g() {{\n{b}}}\n", b = distinct(8)),
        ] {
            let r = analyze_duplication(&src, "rust");
            assert!(
                r.combined_ratio >= r.ratio - 1e-12,
                "combined {} < type1 {}",
                r.combined_ratio,
                r.ratio
            );
            assert!((0.0..=1.0).contains(&r.combined_ratio));
        }
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
