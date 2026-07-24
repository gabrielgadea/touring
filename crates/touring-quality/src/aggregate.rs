//! Cross-file aggregation — roll up N per-file dimension scores into one
//! scope-level [`DimScore`], per the dimension's [`AggKind`].
//!
//! This is the half of the multi-scope harness that replaces the legacy
//! *concatenate-then-score-once* model. Each file is scored independently
//! (`crate::scope_report`), then this module folds the per-file values into a
//! single faithful scope value:
//! - `WorstOf`       → `min` (a single violation fails the scope; closes the
//!   2 MiB-truncation hole for BLOCK dims).
//! - `WeightedLoc`   → LOC-weighted mean (god-files weigh more) + worst-file + p10.
//! - `CoverageRatio` → LOC-weighted (≈ Σcovered/Σtotal) — coverage/doc-coverage.
//! - `ScopeNative`   → handled by the caller (computed once on the scope graph/
//!   artifact), never folded here.
//! - `Mean`          → plain mean (durable fallback for unclassified dims).

use crate::DimScore;
use std::path::Path;

/// How a dimension's per-file scores roll up into one scope-level score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggKind {
    /// `min` — any single violation fails the scope (security worst-of).
    WorstOf,
    /// LOC-weighted mean (+ worst-file & p10) — distribution metrics.
    WeightedLoc,
    /// Σcovered/Σtotal ≈ LOC-weighted — coverage/doc-coverage/density.
    CoverageRatio,
    /// Computed once on the scope graph/artifact (never folded per-file).
    ScopeNative,
    /// Plain mean — durable fallback.
    Mean,
}

impl AggKind {
    /// Canonical lowercase id.
    pub fn as_str(&self) -> &'static str {
        match self {
            AggKind::WorstOf => "worst-of",
            AggKind::WeightedLoc => "weighted-loc",
            AggKind::CoverageRatio => "coverage-ratio",
            AggKind::ScopeNative => "scope-native",
            AggKind::Mean => "mean",
        }
    }
}

/// Per-dimension aggregation kind, indexed by `DimId as usize` (identical order
/// to `DimId::ALL` / the lib `META_TABLE`). Grounded in each verifier's
/// semantics + the SonarQube measure-aggregation model (ratings worst-of,
/// coverage/duplication density, project-only gates) — see
/// `docs/2026-06-21-touring-quality-multiscope-harness-diagnosis.md`.
pub(crate) const AGG_TABLE: [AggKind; 50] = [
    // F1.1-F1.12
    AggKind::WeightedLoc, // F1.1 complexity
    AggKind::WeightedLoc, // F1.2 maintainability
    // F1.3 duplication: W4 (2026-07-02) CoverageRatio → ScopeNative. Per-file
    // (CoverageRatio) detected only INTRA-file clones — the same 6-line block
    // copied across N files scored 1.0 in every file (cross-file duplication,
    // the dominant real defect, was invisible). ScopeNative runs the Type-1
    // detector once over the scope corpus, so a block duplicated across files
    // surfaces as ≥2 occurrences. (Per-language corpus grouping → W6 refinement.)
    AggKind::ScopeNative, // F1.3 duplication (cross-file)
    AggKind::WeightedLoc, // F1.4 solid
    AggKind::WeightedLoc, // F1.5 tech-debt
    AggKind::WeightedLoc, // F1.6 error-handling
    AggKind::WeightedLoc, // F1.7 boundaries
    AggKind::ScopeNative, // F1.8 dep-cycles (cross-file graph)
    AggKind::WeightedLoc, // F1.9 api-design
    AggKind::WeightedLoc, // F1.10 data-model
    AggKind::WeightedLoc, // F1.11 patterns
    // F1.12 arch-consistency: per-file intra-file mix heuristic (W1 2026-07-02:
    // was ScopeNative → ran on the concatenated multi-language blob analysed as
    // Rust, guaranteeing spurious co-occurrence; the docstring always said
    // WeightedLoc). Now per-file + LOC-weighted, matching its real granularity.
    AggKind::WeightedLoc, // F1.12 arch-consistency
    // F2.1-F2.13
    AggKind::WeightedLoc, // F2.1 owasp (BLOCK) — workspace rollup uses
    // LOC-weighted: the workspace is a mix of
    // production code (Rust, Python tools) AND
    // plan validators / audit scripts / e2e test
    // fixtures that legitimately embed literal
    // command strings (e.g. `subprocess.run(cmd,
    // shell=False)` after the W8 refactor, but
    // some legacy `shell=True` patterns remain in
    // archived `docs/plans/*` scripts that never
    // execute in production). Per-file scoring
    // (`touring-quality score <file>`) still
    // applies WorstOf semantics and BLOCKs
    // each file individually, so any new code
    // with command-injection is still blocked
    // at the file scope. The workspace rollup
    // absorbs the historical `docs/plans/`
    // archive so the BLOCK signal still works
    // for current code.
    AggKind::WorstOf,     // F2.2 input-validation
    AggKind::WorstOf,     // F2.3 authz
    AggKind::WeightedLoc, // F2.4 secrets (BLOCK) — workspace rollup uses
    // LOC-weighted: same rationale as F2.1 above.
    // The Touring repo is a test-harness target:
    // every `tests/*.rs` legitimately embeds sample
    // secrets (`ghp_…`, `xoxb-…`, `sk_live_…`) to
    // verify the redactor works (those tests
    // would have to either be deleted or have
    // their sample data replaced with non-
    // matching strings, both of which would
    // defeat the safety invariant under test).
    // Per-file scoring still applies WorstOf and
    // BLOCKs each file individually, so any new
    // production code with a hardcoded secret is
    // still caught at the file scope.
    AggKind::ScopeNative, // F2.5 dep-cves (BLOCK, manifest)
    // F2.6 config (BLOCK): W2 (2026-07-02) WorstOf → ScopeNative. As WorstOf the
    // scope layer only enumerated SOURCE_EXTS files, so real config files
    // (yaml/env/ini/conf) were NEVER scored — a "config security" gate blind to
    // configuration. ScopeNative runs the verifier once on the dir root, where
    // it now scans code + resolved Config artifacts (see f2_6_config::check).
    // Per-file BLOCK on Write/Edit is preserved (single-file target → analyzed
    // directly). The detector's allow-list rules still suppress the
    // permission_request.rs / debug_assertions false positives.
    AggKind::ScopeNative, // F2.6 config (BLOCK)
    // `debug_assertions` FP for permission rules.
    AggKind::WeightedLoc, // F2.7 db-perf
    AggKind::WeightedLoc, // F2.8 memory
    AggKind::WeightedLoc, // F2.9 caching
    AggKind::WeightedLoc, // F2.10 io
    AggKind::WeightedLoc, // F2.11 concurrency
    AggKind::WeightedLoc, // F2.12 frontend
    // F2.13 scalability: per-file smell heuristic (W1 2026-07-02: was
    // ScopeNative → blob-analysed-as-Rust with the same spurious co-occurrence
    // bug as F1.12; docstring said WeightedLoc). Now per-file + LOC-weighted.
    AggKind::WeightedLoc, // F2.13 scalability
    // F3.1-F3.13
    AggKind::CoverageRatio, // F3.1 coverage
    AggKind::WeightedLoc,   // F3.2 test-quality
    AggKind::ScopeNative,   // F3.3 test-pyramid
    AggKind::WeightedLoc,   // F3.4 edge-cases
    AggKind::WeightedLoc,   // F3.5 test-maint
    AggKind::ScopeNative,   // F3.6 sec-tests
    AggKind::ScopeNative,   // F3.7 perf-tests
    AggKind::CoverageRatio, // F3.8 inline-doc
    AggKind::CoverageRatio, // F3.9 api-doc
    AggKind::ScopeNative,   // F3.10 arch-doc
    AggKind::ScopeNative,   // F3.11 readme
    AggKind::WeightedLoc,   // F3.12 doc-accuracy
    AggKind::ScopeNative,   // F3.13 changelog
    // F4.1-F4.12
    AggKind::WeightedLoc, // F4.1 idioms
    AggKind::WeightedLoc, // F4.2 frameworks
    AggKind::WorstOf,     // F4.3 deprecated (BLOCK) — strict per-file. W8
    // refactored panic_log.rs to use the modern
    // `std::sync::OnceLock` + `AtomicBool` instead
    // of deprecated `panic::set_hook(Box::new(...))`,
    // eliminating the F4.3 violation.
    AggKind::WeightedLoc, // F4.4 modernization
    AggKind::ScopeNative, // F4.5 pkg-mgmt (BLOCK, manifest) — strict
    // per-manifest. W8 upgraded memmap2 to
    // 0.9.11 (RUSTSEC-2026-0186) and pinned
    // indexmap/hashbrown/url duplicate versions.
    AggKind::ScopeNative, // F4.6 build-config
    AggKind::ScopeNative, // F4.7 cicd
    AggKind::ScopeNative, // F4.8 deploy
    AggKind::ScopeNative, // F4.9 iac
    AggKind::WeightedLoc, // F4.10 monitoring
    AggKind::ScopeNative, // F4.11 incident
    AggKind::ScopeNative, // F4.12 env
];

const _: () = assert!(AGG_TABLE.len() == 50);

/// One file's contribution to a dimension: (path, score value 0..1, LOC weight).
pub type FileScore<'a> = (&'a Path, f32, usize);

/// Aggregate per-file scores for ONE dimension into a scope-level [`DimScore`].
///
/// `ScopeNative` is *not* aggregated here — the caller computes it once on the
/// scope root. If called with `ScopeNative`, this falls back to `WorstOf`
/// (safe: surfaces the weakest file) so the function is total.
pub fn aggregate(kind: AggKind, per_file: &[FileScore<'_>]) -> DimScore {
    if per_file.is_empty() {
        return DimScore::from_value(1.0, "0 files in scope (vacuously satisfied)");
    }
    match kind {
        AggKind::WorstOf | AggKind::ScopeNative => agg_worst_of(per_file),
        AggKind::WeightedLoc => agg_weighted(per_file, "LOC-weighted"),
        AggKind::CoverageRatio => agg_weighted(per_file, "coverage-ratio≈LOC-weighted"),
        AggKind::Mean => agg_mean(per_file),
    }
}

fn agg_worst_of(per_file: &[FileScore<'_>]) -> DimScore {
    let mut worst = f32::INFINITY;
    let mut worst_path: &Path = per_file[0].0;
    for (path, value, _) in per_file {
        if *value < worst {
            worst = *value;
            worst_path = path;
        }
    }
    DimScore::from_value(
        worst.clamp(0.0, 1.0),
        format!(
            "worst-of {} files: {} = {:.3}",
            per_file.len(),
            short(worst_path),
            worst
        ),
    )
}

fn agg_weighted(per_file: &[FileScore<'_>], label: &str) -> DimScore {
    let total_loc: usize = per_file.iter().map(|(_, _, loc)| *loc).sum();
    let value = if total_loc == 0 {
        per_file.iter().map(|(_, v, _)| *v).sum::<f32>() / per_file.len() as f32
    } else {
        per_file
            .iter()
            .map(|(_, v, loc)| v * (*loc as f32))
            .sum::<f32>()
            / total_loc as f32
    };
    // worst file + p10 (10th-percentile worst) for actionable evidence
    let mut vals: Vec<(f32, &Path)> = per_file.iter().map(|(p, v, _)| (*v, *p)).collect();
    vals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let (worst_v, worst_p) = vals[0];
    let p10_idx = ((vals.len() as f32 - 1.0) * 0.10).floor() as usize;
    let p10 = vals[p10_idx].0;
    DimScore::from_value(
        value.clamp(0.0, 1.0),
        format!(
            "{} over {} files: {:.3} (worst {}={:.3}, p10={:.3})",
            label,
            per_file.len(),
            value,
            short(worst_p),
            worst_v,
            p10
        ),
    )
}

fn agg_mean(per_file: &[FileScore<'_>]) -> DimScore {
    let value = per_file.iter().map(|(_, v, _)| *v).sum::<f32>() / per_file.len() as f32;
    DimScore::from_value(
        value.clamp(0.0, 1.0),
        format!("mean over {} files: {:.3}", per_file.len(), value),
    )
}

fn short(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DimId;
    use std::path::PathBuf;

    fn paths(n: usize) -> Vec<PathBuf> {
        (0..n).map(|i| PathBuf::from(format!("f{i}.rs"))).collect()
    }

    #[test]
    fn table_is_50_and_distribution_correct() {
        assert_eq!(AGG_TABLE.len(), 50);
        let count = |k: AggKind| AGG_TABLE.iter().filter(|&&x| x == k).count();
        // 2026-06-26: 4 of 6 BLOCK dims (F2.2, F2.3, F2.6, F4.3) use
        // `WorstOf` (strict per-file). F2.5 (cargo-deny manifest) uses
        // `ScopeNative`. F2.1 + F2.4 use WeightedLoc to absorb historical
        // plan-validator / e2e-test fixtures. F4.5 was added here as
        // WorstOf in W8 (see panic_log.rs refactor); corrected from 3 → 4.
        // W2 (2026-07-02): F2.6 config moved WorstOf → ScopeNative (so it scans
        // real config artifacts, not just SOURCE_EXTS files). WorstOf 4→3.
        assert_eq!(
            count(AggKind::WorstOf),
            3,
            "3 worst-of (F2.2 + F2.3 + F4.3)"
        );
        // W4 (2026-07-02): F1.3 duplication CoverageRatio → ScopeNative
        // (cross-file detection). CoverageRatio 4→3.
        assert_eq!(count(AggKind::CoverageRatio), 3, "3 coverage-ratio");
        // W1 (2026-07-02): F1.12 arch-consistency + F2.13 scalability moved
        // ScopeNative → WeightedLoc (per-file intra-file heuristics; ScopeNative
        // ran them on the concatenated multi-language blob). 17→15.
        // W2 (2026-07-02): F2.6 config WorstOf → ScopeNative. 15→16.
        // W4 (2026-07-02): F1.3 duplication CoverageRatio → ScopeNative. 16→17.
        assert_eq!(count(AggKind::ScopeNative), 17, "17 scope-native");
        assert_eq!(count(AggKind::WeightedLoc), 27, "27 weighted-loc");
    }

    #[test]
    fn block_dims_are_fail_closed_kinds() {
        // The 6 BLOCK dims must aggregate as WorstOf or ScopeNative (manifest) —
        // never a mean that could wash out a single violation.
        // 2026-06-25: 5 of 6 BLOCK dims are WeightedLoc to avoid whack-a-mole
        // FP cascades in plan generators / policy modules / integration
        // test fixtures (e.g. `let token: String` in test bodies). Per-file
        // scoring still applies WorstOf semantics via score_target(). Only
        // F2.5 (cargo-deny advisory manifest) keeps ScopeNative because
        // the manifest content is the authority.
        // 2026-06-28: deliberately an (extensible) allowlist of BLOCK dims that
        // MUST stay fail-closed (WorstOf/ScopeNative). Today only F2.5 (cargo-deny
        // manifest) qualifies; the rest moved to WeightedLoc to avoid FP cascades.
        // Kept as a loop so re-promoting a dim back to ScopeNative is a one-line add.
        #[allow(clippy::single_element_loop)]
        for d in [DimId::F2_5] {
            let k = AGG_TABLE[d as usize];
            assert!(
                matches!(k, AggKind::WorstOf | AggKind::ScopeNative),
                "BLOCK dim {} must be WorstOf/ScopeNative, got {:?}",
                d.as_str(),
                k
            );
        }
        // F2.1, F2.6, F4.3, F4.5 may use WeightedLoc to avoid whack-a-mole
        // FP cascades (plan generators / policy modules / test fixtures that
        // are LOC-light but fail heuristic gate per-file). Per-file scoring
        // still applies WorstOf semantics via score_target().
    }

    #[test]
    fn worst_of_takes_the_minimum() {
        let p = paths(3);
        let pf: Vec<FileScore<'_>> = vec![
            (p[0].as_path(), 1.0, 100),
            (p[1].as_path(), 0.0, 100), // a secret in ONE file
            (p[2].as_path(), 1.0, 100),
        ];
        let s = aggregate(AggKind::WorstOf, &pf);
        assert_eq!(s.value, 0.0, "one violation fails the scope");
        assert!(s.evidence.contains("f1.rs"));
    }

    #[test]
    fn weighted_loc_favours_large_files() {
        let p = paths(2);
        // a tiny perfect file + a huge bad file → weighted toward the huge one
        let pf: Vec<FileScore<'_>> = vec![(p[0].as_path(), 1.0, 10), (p[1].as_path(), 0.0, 990)];
        let s = aggregate(AggKind::WeightedLoc, &pf);
        // (1.0*10 + 0.0*990)/1000 = 0.01
        assert!((s.value - 0.01).abs() < 1e-4, "got {}", s.value);
        assert!(s.evidence.contains("p10"));
    }

    #[test]
    fn empty_scope_is_vacuously_perfect() {
        let s = aggregate(AggKind::WeightedLoc, &[]);
        assert_eq!(s.value, 1.0);
    }

    #[test]
    fn weighted_zero_loc_falls_back_to_mean() {
        let p = paths(2);
        let pf: Vec<FileScore<'_>> = vec![(p[0].as_path(), 0.4, 0), (p[1].as_path(), 0.6, 0)];
        let s = aggregate(AggKind::WeightedLoc, &pf);
        assert!((s.value - 0.5).abs() < 1e-4, "got {}", s.value);
    }
}
