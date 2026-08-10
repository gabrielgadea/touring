//! F1.4 — Clean Code / SOLID verifier (D04).
//!
//! **Real engine (default `workspace-integration`)**: for Rust, delegates to
//! [`touring_analysis::quality::RustQualitySignals`] — a **syn-AST** report —
//! and scores a SOLID *heuristic* from real semantic signals: `semantic_complexity`
//! as a Single-Responsibility proxy (a type/module doing too much), and
//! `unsafe_count` as an encapsulation/abstraction smell. SOLID is not fully
//! mechanically decidable, so this is explicitly a *signal-grounded heuristic*
//! (not ground truth) — but it reads real AST structure rather than the prior
//! meaningless `pub`/`private` ratio. For non-Rust (or a snippet syn cannot
//! parse), it falls back to average cyclomatic complexity per function from
//! [`touring_analysis::quality::estimate_complexity`] as a crude SRP proxy.
//!
//! **Standalone fallback (`--no-default-features`)**: the prior public/private
//! function coupling ratio, clearly labelled.
//!
//! **Scope**: per-file (`AggKind::WeightedLoc`).

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F1.4 verifier — Clean Code / SOLID.
#[allow(non_camel_case_types)]
pub struct F1_4_Solid;

impl Verification for F1_4_Solid {
    fn id(&self) -> DimId {
        DimId::F1_4
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_solid_dim(target)
    }
}

// ── Real engine: SOLID heuristic from syn-AST semantic signals ────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_solid_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{RustQualitySignals, estimate_complexity};

    let raw = crate::verifications::read_target_source(target)?;

    if let Some(sig) = RustQualitySignals::from_source(&raw) {
        let value = score_solid_rust(
            sig.semantic_complexity,
            sig.unsafe_count,
            sig.max_type_methods,
        );
        let evidence = format!(
            "F1.4: SOLID — sem_cx={:.2} (SRP proxy) god_object=max {} inherent methods/type \
             unsafe={} impl_blocks={} trait_bounds={} — score={value:.3} \
             (touring-analysis RustQualitySignals, syn-AST)",
            sig.semantic_complexity,
            sig.max_type_methods,
            sig.unsafe_count,
            sig.impl_block_count,
            sig.trait_bound_count
        );
        return Ok((value, evidence));
    }

    // Non-Rust / unparseable: average cyclomatic complexity per function.
    let lang = crate::verifications::lang_from_ext(target);
    let m = estimate_complexity(&raw, lang);
    let value = score_solid_fallback(m.avg_complexity);
    let evidence = format!(
        "F1.4: SOLID heuristic (non-Rust) — avg_cc/fn={:.1} fns={} ({lang}) — \
         score={value:.3} (touring-analysis estimate_complexity avg complexity)",
        m.avg_complexity, m.function_count
    );
    Ok((value, evidence))
}

/// SOLID heuristic for Rust from syn-AST signals. `semantic_complexity` (∈[0,1])
/// is a Single-Responsibility proxy: a type/module concentrating much generic /
/// trait / nesting complexity is likely doing too much. `unsafe` blocks are an
/// encapsulation smell (a leaked low-level abstraction). Both are *signals*, not
/// proof — a genuinely cohesive but intricate parser can be complex yet SOLID.
#[cfg(feature = "workspace-integration")]
fn score_solid_rust(semantic_complexity: f32, unsafe_count: usize, max_type_methods: usize) -> f32 {
    let srp_penalty = (semantic_complexity * 0.5).clamp(0.0, 0.5);
    let unsafe_penalty = (unsafe_count as f32 * 0.05).min(0.2);
    // God-object / SRP penalty (W4 2026-07-02): a single type with many inherent
    // methods has more than one reason to change. Generous threshold (15 free)
    // so cohesive types are unpenalised; ramps to a 0.4 cap at ~55 methods.
    let god_penalty = ((max_type_methods.saturating_sub(15)) as f32 / 100.0).min(0.4);
    (1.0 - srp_penalty - unsafe_penalty - god_penalty).clamp(0.0, 1.0)
}

/// Non-Rust SRP proxy: average cyclomatic complexity per function. `≤3` is
/// clean; functions averaging high branching are doing too much. Maps
/// avg≈3→1.0 down to avg≈20→0.4.
#[cfg(feature = "workspace-integration")]
fn score_solid_fallback(avg_complexity: f64) -> f32 {
    let avg = avg_complexity as f32;
    (1.0 - ((avg - 3.0).max(0.0) / 17.0) * 0.6).clamp(0.0, 1.0)
}

// ── Standalone fallback: public/private coupling ratio ────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_solid_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;

    let public_fns = raw.matches("pub fn ").count() as f32;
    let private_fns = raw.matches("fn ").count() as f32 - public_fns;
    let coupling = if private_fns > 0.0 {
        public_fns / (public_fns + private_fns * 2.0)
    } else {
        0.5
    };
    let value = (1.0 - coupling.min(1.0)).clamp(0.0, 1.0);
    let evidence = format!(
        "Clean Code / SOLID: pub/private coupling={coupling:.2} (heuristic; build \
         --features workspace-integration for syn-AST SOLID signals) — score={value:.3}"
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

    #[test]
    fn test_solid_returns_valid_score() {
        let f = write_temp_rs("fn example() {}\n");
        let s = F1_4_Solid.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_solid_empty_file() {
        let f = write_temp_rs("");
        let s = F1_4_Solid.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Clean, simple, cohesive Rust scores high (low semantic complexity, no unsafe).
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_simple_cohesive_scores_high() {
        let code = "struct Point { x: i32, y: i32 }\n\
                    impl Point {\n    fn new(x: i32, y: i32) -> Self { Self { x, y } }\n    \
                    fn sum(&self) -> i32 { self.x + self.y }\n}\n";
        let f = write_temp_rs(code);
        let s = F1_4_Solid.check(f.path()).expect("check");
        assert!(
            s.value > 0.8,
            "simple cohesive type should score > 0.8, got {}",
            s.value
        );
    }

    /// `unsafe` is an encapsulation smell → lowers the SOLID score below a
    /// clean equivalent.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_unsafe_lowers_solid() {
        let clean = write_temp_rs("fn f(x: i32) -> i32 { x + 1 }\n");
        let unsafe_code = write_temp_rs(
            "fn f(x: i32) -> i32 {\n    unsafe { std::ptr::null::<u8>(); }\n    \
             unsafe { std::ptr::null::<u8>(); }\n    x + 1\n}\n",
        );
        let sc = F1_4_Solid.check(clean.path()).expect("check");
        let su = F1_4_Solid.check(unsafe_code.path()).expect("check");
        assert!(
            su.value < sc.value,
            "unsafe ({}) must score below clean ({})",
            su.value,
            sc.value
        );
    }

    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_score_solid_rust_bounds() {
        assert!((score_solid_rust(0.0, 0, 0) - 1.0).abs() < 1e-6);
        assert!((0.0..=1.0).contains(&score_solid_rust(1.0, 100, 200)));
        assert!(score_solid_rust(0.9, 4, 0) < score_solid_rust(0.1, 0, 0));
    }

    /// W4 (2026-07-02): a God-object (many inherent methods on one type) scores
    /// below a cohesive type, even at equal semantic complexity / unsafe.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_god_object_penalised() {
        let cohesive = score_solid_rust(0.2, 0, 8); // under the 15-method free band
        let god = score_solid_rust(0.2, 0, 60); // 60 methods on one type
        assert!(
            god < cohesive,
            "God-object ({god}) must score below a cohesive type ({cohesive})"
        );
    }
}
