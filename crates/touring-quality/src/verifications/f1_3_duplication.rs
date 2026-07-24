//! F1.3 — Code Duplication verifier (D03).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_duplication`] — Type-1 (exact, modulo
//! whitespace) **block** clone detection (jscpd/SonarQube-CPD style): runs of 6+
//! consecutive meaningful production lines recurring verbatim, reported as a
//! duplicated-line ratio (target < 3%, the jscpd "healthy" threshold). This
//! replaces the prior stub, which counted every *isolated* line that recurred —
//! so common idioms (`Ok(())`, `let x = Vec::new();`) were scored as duplication
//! while real copy-paste *blocks* were missed. Comments, blank/structural lines,
//! and `#[cfg(test)]` regions are excluded (jscpd's test-exclusion convention).
//!
//! **Standalone fallback (`--no-default-features`)**: the prior isolated
//! duplicate-line ratio (no block detection, counts idiom repeats), labelled.
//!
//! **Scope**: `AggKind::ScopeNative` (W4 2026-07-02, was `CoverageRatio`). The
//! Type-1 detector runs **once over the whole scope corpus**, so a block copied
//! across N files surfaces as duplication — per-file scoring hid cross-file
//! clones (each copy scored 1.0 in its own file). A single-file target is scored
//! directly (intra-file only). See `aggregate::AGG_TABLE` for the rationale.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F1.3 verifier — Code Duplication.
#[allow(non_camel_case_types)]
pub struct F1_3_Duplication;

impl Verification for F1_3_Duplication {
    fn id(&self) -> DimId {
        DimId::F1_3
    }

    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_duplication_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: Type-1 block clone detection ─────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_duplication_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::analyze_duplication;

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_duplication(&raw, lang);

    let value = score_duplication(r.ratio);
    let evidence = format!(
        "F1.3: duplication ratio={:.1}% ({} dup / {} meaningful lines, {} clone block(s)) \
         ({lang}) — score={value:.3} (touring-analysis analyze_duplication, Type-1 block clones)",
        r.ratio * 100.0,
        r.duplicated_lines,
        r.total_meaningful_lines,
        r.clone_blocks
    );
    Ok((value, evidence))
}

/// D03 duplicated-line-ratio score using jscpd/SonarQube bands: < 3% healthy
/// (`1.0`), 3–8% accumulating (`0.8 → 0.5` warn), > 8% pay-down-now
/// (`0.5 → 0.1` fail).
#[cfg(feature = "workspace-integration")]
fn score_duplication(ratio: f64) -> f32 {
    let r = ratio as f32;
    if r <= 0.03 {
        1.0
    } else if r <= 0.08 {
        (0.8 - (r - 0.03) / 0.05 * 0.3).clamp(0.5, 0.8) // 0.8 → 0.5 across [3%,8%]
    } else if r <= 0.20 {
        (0.5 - (r - 0.08) / 0.12 * 0.4).clamp(0.1, 0.5) // 0.5 → 0.1 across [8%,20%]
    } else {
        0.1
    }
}

// ── Standalone fallback: isolated duplicate-line ratio (no block detection) ───
#[cfg(not(feature = "workspace-integration"))]
fn analyze_duplication_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;

    let lines: Vec<&str> = raw.lines().collect();
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for line in &lines {
        let t = line.trim();
        if t.len() > 20 && !t.starts_with("//") {
            *seen.entry(t).or_insert(0) += 1;
        }
    }
    let dup_lines: usize = seen.values().filter(|&&c| c > 1).sum();
    let total_lines = lines.len().max(1);
    let dup_ratio = dup_lines as f32 / total_lines as f32;
    let value = (1.0 - dup_ratio.min(1.0)).clamp(0.0, 1.0);
    let evidence = format!(
        "Code Duplication: {dup_lines} repeated lines (isolated-line heuristic; build \
         --features workspace-integration for Type-1 block clone detection) — score={value:.3}"
    );
    Ok((value, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_rs(content: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()
            .expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_duplication_returns_valid_score() {
        let f = write_temp_rs("fn example() {}\n");
        let s = F1_3_Duplication.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_duplication_empty_file() {
        let f = write_temp("");
        let s = F1_3_Duplication.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Clean, non-repetitive code scores high.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_clean_code_scores_high() {
        let body: String = (0..15)
            .map(|i| format!("    let v{i} = compute({i}) + base_{i};\n"))
            .collect();
        let f = write_temp_rs(&format!("fn f() {{\n{body}}}\n"));
        let s = F1_3_Duplication.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "non-repetitive code should be high, got {}",
            s.value
        );
    }

    /// **End-to-end FP fix**: a file repeating only the idiom `Ok(())` (the stub's
    /// false positive) must NOT be penalised — it is not a block clone.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_repeated_idiom_not_penalised() {
        let code = "fn a() -> Result<(), E> { do_a()?; Ok(()) }\n\
                    fn b() -> Result<(), E> { do_b()?; Ok(()) }\n\
                    fn c() -> Result<(), E> { do_c()?; Ok(()) }\n\
                    fn d() -> Result<(), E> { do_d()?; Ok(()) }\n";
        let f = write_temp_rs(code);
        let s = F1_3_Duplication.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "repeated Ok(()) idiom is not block duplication, got {}",
            s.value
        );
    }

    /// A genuine copy-pasted block lowers the score below a clean file.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_copy_paste_block_lowers_score() {
        let block = "    let a = step_one(input);\n    let b = step_two(a);\n    \
                     let c = step_three(b);\n    let d = step_four(c);\n    \
                     let e = step_five(d);\n    let g = step_six(e);\n    let h = step_seven(g);\n";
        let clean = write_temp_rs("fn f() -> i32 { 1 + 2 + 3 }\n");
        let dup = write_temp_rs(&format!(
            "fn first() {{\n{block}}}\nfn second() {{\n{block}}}\n"
        ));
        let sc = F1_3_Duplication.check(clean.path()).expect("check");
        let sd = F1_3_Duplication.check(dup.path()).expect("check");
        assert!(
            sd.value < sc.value,
            "copy-pasted block ({}) must score below clean ({})",
            sd.value,
            sc.value
        );
    }

    /// D03 band mapping is monotone non-increasing in the duplication ratio.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_score_duplication_bands() {
        assert!((score_duplication(0.0) - 1.0).abs() < 1e-6);
        assert!(
            (score_duplication(0.02) - 1.0).abs() < 1e-6,
            "< 3% is healthy"
        );
        assert!(score_duplication(0.05) < 1.0 && score_duplication(0.05) >= 0.5);
        assert!(score_duplication(0.15) < 0.5, "> 8% must fail");
        // Monotone non-increasing.
        let mut prev = 2.0_f32;
        for pct in [0.0, 0.03, 0.05, 0.08, 0.12, 0.25] {
            let s = score_duplication(pct);
            assert!(s <= prev, "ratio {pct} gave {s} > prev {prev}");
            prev = s;
        }
    }
}
