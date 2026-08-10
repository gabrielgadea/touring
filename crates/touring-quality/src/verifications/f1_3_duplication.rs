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

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F1.3 verifier — Code Duplication.
#[allow(non_camel_case_types)]
pub struct F1_3_Duplication;

impl Verification for F1_3_Duplication {
    fn id(&self) -> DimId {
        DimId::F1_3
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_duplication_dim(target)
    }
}

// ── Real engine: Type-1 block clone detection ─────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_duplication_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::analyze_duplication;

    // Machine-generated trees (openapi-generator markers) are excluded from the
    // DUPLICATION corpus only — their clones are the generator's signature, not
    // debt — and the exclusion is announced in the evidence (never silent).
    let (raw, generated_excluded, truncated) =
        crate::verifications::read_target_source_excluding_generated(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_duplication(&raw, lang);

    let value = score_duplication(r.ratio, r.combined_ratio);
    let mut evidence = format!(
        "F1.3: duplication ratio={:.1}% ({} dup / {} meaningful lines, {} clone block(s)) \
         ({lang}) — score={value:.3} (touring-analysis analyze_duplication, Type-1 block clones)",
        r.ratio * 100.0,
        r.duplicated_lines,
        r.total_meaningful_lines,
        r.clone_blocks
    );
    // A1 (2026-08-08, decisão do Gabriel): Type-2 ENTRA na nota, através de uma
    // banda própria, e a nota final é o mínimo das duas. Type-1 sozinho era um
    // limite inferior apresentado como medida — a dimensão se chama "Code
    // Duplication" e era cega a toda cópia renomeada. Combinar por `min`
    // preserva exatamente a severidade histórica do Type-1 (nada que reprovava
    // passa a aprovar) e deixa cada razão ser julgada na escala em que foi
    // calibrada.
    evidence.push_str(&format!(
        "; Type-2 (token-normalized): {} clone region(s), +{} line(s) Type-1 cannot see \
         → combined={:.1}% (scored on its OWN band: <15% healthy, 15-30% warn, 30-50% \
         pay-down; F1.3 = min(type1_band, combined_band))",
        r.type2_clone_regions,
        r.type2_only_lines,
        r.combined_ratio * 100.0
    ));
    if let Some(reason) = r.near_pass_skipped {
        evidence.push_str(&format!("; ⚠ {reason}"));
    }
    if generated_excluded > 0 {
        evidence.push_str(&format!(
            "; {generated_excluded} machine-generated file(s) excluded (openapi-generator markers)"
        ));
    }
    // A truncagem NÃO pode ser silenciosa. Um score de prefixo apresentado como
    // score do escopo faz o gate parecer medir o que não mediu — e, pior, o
    // corte por bytes torna o número quase imune a remediação real: remover
    // duplicação dentro da janela só admite mais conteúdo na borda. Quem lê o
    // score precisa saber que este é um recorte, e reescopar por crate para
    // obter um número confiável.
    if truncated {
        evidence.push_str(
            "; ⚠ TRUNCADO no teto de varredura (16 MiB) — este score cobre um PREFIXO do escopo, \
             não o escopo inteiro, e é insensível a remediação feita depois do corte; \
             pontue por crate para um número confiável",
        );
    }
    Ok((value, evidence))
}

/// D03 Type-1 duplicated-line-ratio band, jscpd/SonarQube calibration: < 3%
/// healthy (`1.0`), 3–8% accumulating (`0.8 → 0.5` warn), > 8% pay-down-now
/// (`0.5 → 0.1` fail). **Unchanged since 2026-07-02** — every historical F1.3
/// score is still comparable through this function.
#[cfg(feature = "workspace-integration")]
fn score_type1_band(ratio: f64) -> f32 {
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

/// Band for the **combined** (Type-1 ∪ Type-2) coverage.
///
/// Deliberately NOT the Type-1 band. Combined coverage measures a broader
/// thing: a run of identically-shaped statements is a Type-2 clone and not a
/// Type-1 one, so the two ratios live on different scales. Feeding 17–43%
/// (this workspace, measured) into a band whose fail floor is 20% would pin
/// every crate at 0.1 — a constant, and a constant carries no information.
///
/// Calibrated against the empirical distribution (touring-foundation 16.8%,
/// touring-simd 25.1%, touring-cli 26.3%, touring-analysis 35.5%,
/// touring-quality 42.7%) and the clone-detection literature, where Type-2/3
/// coverage of 15–30% is the ordinary range for real systems: < 15% healthy,
/// 15–30% accumulating, 30–50% pay-down, > 50% saturated.
#[cfg(feature = "workspace-integration")]
fn score_combined_band(ratio: f64) -> f32 {
    let r = ratio as f32;
    if r <= 0.15 {
        1.0
    } else if r <= 0.30 {
        (0.8 - (r - 0.15) / 0.15 * 0.3).clamp(0.5, 0.8)
    } else if r <= 0.50 {
        (0.5 - (r - 0.30) / 0.20 * 0.4).clamp(0.1, 0.5)
    } else {
        0.1
    }
}

/// F1.3 score: the **stricter** of the two bands.
///
/// `min` rather than a blend or a replacement, for three reasons. It can only
/// ever *lower* a score relative to the Type-1-only history, so nothing that
/// used to fail now passes — the dimension never becomes more permissive by
/// accident. It lets each ratio be judged on its own calibrated scale instead
/// of forcing one number through the other's band. And it keeps the dimension
/// honest to its own name: "Code Duplication" that is blind to renamed
/// copy-paste was always a lower bound presented as a measurement.
#[cfg(feature = "workspace-integration")]
fn score_duplication(type1_ratio: f64, combined_ratio: f64) -> f32 {
    score_type1_band(type1_ratio).min(score_combined_band(combined_ratio))
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

    /// A machine-generated SDK tree (openapi-generator marker) is excluded from
    /// the duplication corpus, and the exclusion is ANNOUNCED in the evidence.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_generated_sdk_tree_excluded_and_announced() {
        let dir = tempfile::tempdir().expect("tempdir");
        let block = "    let a = step_one(input);\n    let b = step_two(a);\n    \
                     let c = step_three(b);\n    let d = step_four(c);\n    \
                     let e = step_five(d);\n    let g = step_six(e);\n    let h = step_seven(g);\n";
        let sdk = dir.path().join("sdks").join("go");
        std::fs::create_dir_all(&sdk).expect("mkdir");
        std::fs::write(sdk.join(".openapi-generator-ignore"), "").expect("marker");
        std::fs::write(
            sdk.join("a.go"),
            format!("func first() {{\n{block}}}\nfunc second() {{\n{block}}}\n"),
        )
        .expect("write");
        std::fs::write(dir.path().join("clean.rs"), "fn f() -> i32 { 1 + 2 + 3 }\n")
            .expect("write");
        let s = F1_3_Duplication.check(dir.path()).expect("check");
        assert!(
            s.value > 0.95,
            "generated-tree clones must not count as debt, got {}",
            s.value
        );
        assert!(
            s.evidence.contains("machine-generated file(s) excluded"),
            "exclusion must be announced, got: {}",
            s.evidence
        );
    }

    /// Without a generator marker the same tree DOES count (the filter is
    /// structural, never a blanket sdk-name exemption).
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_unmarked_sdk_tree_still_counts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let block = "    let a = step_one(input);\n    let b = step_two(a);\n    \
                     let c = step_three(b);\n    let d = step_four(c);\n    \
                     let e = step_five(d);\n    let g = step_six(e);\n    let h = step_seven(g);\n";
        let sdk = dir.path().join("sdks").join("go");
        std::fs::create_dir_all(&sdk).expect("mkdir");
        std::fs::write(
            sdk.join("a.go"),
            format!("func first() {{\n{block}}}\nfunc second() {{\n{block}}}\n"),
        )
        .expect("write");
        let s = F1_3_Duplication.check(dir.path()).expect("check");
        assert!(
            s.value < 0.95,
            "unmarked clones must still count as debt, got {}",
            s.value
        );
    }

    /// D03 band mapping is monotone non-increasing in the duplication ratio.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_score_duplication_bands() {
        // O Type-1 mantém EXATAMENTE a calibração histórica (combined=0 isola).
        assert!((score_duplication(0.0, 0.0) - 1.0).abs() < 1e-6);
        assert!((score_duplication(0.02, 0.0) - 1.0).abs() < 1e-6, "< 3% é saudável");
        assert!(score_duplication(0.05, 0.0) < 1.0 && score_duplication(0.05, 0.0) >= 0.5);
        assert!(score_duplication(0.15, 0.0) < 0.5, "> 8% tem de reprovar");
        // Monotone non-increasing em cada eixo.
        let mut prev = 2.0_f32;
        for pct in [0.0, 0.03, 0.05, 0.08, 0.12, 0.25] {
            let s = score_duplication(pct, 0.0);
            assert!(s <= prev, "type1 {pct} deu {s} > prev {prev}");
            prev = s;
        }
        let mut prev = 2.0_f32;
        for pct in [0.0, 0.15, 0.25, 0.35, 0.55] {
            let s = score_duplication(0.0, pct);
            assert!(s <= prev, "combined {pct} deu {s} > prev {prev}");
            prev = s;
        }
    }

    /// `min` só pode BAIXAR: nada que reprovava por Type-1 passa a aprovar.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn combining_never_relaxes_the_type1_verdict() {
        for t1 in [0.0, 0.02, 0.05, 0.09, 0.15, 0.30] {
            let type1_only = score_type1_band(t1);
            for combined in [t1, 0.10, 0.20, 0.40, 0.80] {
                let combined = combined.max(t1); // combined ⊇ type1, sempre
                assert!(
                    score_duplication(t1, combined) <= type1_only + 1e-6,
                    "t1={t1} combined={combined} afrouxou {type1_only}"
                );
            }
        }
    }

    /// A banda do combinado tem de DISCRIMINAR na faixa observada (17-43%),
    /// não colapsar num valor único — um score constante não carrega informação.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn the_combined_band_discriminates_across_the_measured_range() {
        let observed = [0.168, 0.251, 0.263, 0.355, 0.427]; // foundation..quality
        let scores: Vec<f32> = observed.iter().map(|r| score_combined_band(*r)).collect();
        let (lo, hi) = (
            scores.iter().cloned().fold(f32::MAX, f32::min),
            scores.iter().cloned().fold(f32::MIN, f32::max),
        );
        assert!(
            hi - lo > 0.3,
            "a banda tem de separar os crates observados, deu spread {}: {scores:?}",
            hi - lo
        );
        assert!(lo > 0.1, "nenhum crate real deveria saturar no piso: {scores:?}");
    }
}
