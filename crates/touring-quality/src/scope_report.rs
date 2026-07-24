//! Scope-level scoring — the multi-scope counterpart of [`crate::score_target`].
//!
//! [`score_scope`] resolves nothing itself: given a [`Scope`] (a file-set + a
//! kind), it scores **every file** on each requested dimension in parallel,
//! rolls each dimension up by its [`crate::aggregate::AggKind`], computes the
//! `ScopeNative` dimensions **once** on the scope root, and assembles a
//! [`ScopeReport`].
//!
//! Invariant (constitutional): **all 50 dimensions appear in every report at
//! every granularity.** A `ScopeNative` dimension at a sub-repository scope is
//! still evaluated (against the scope root / nearest enclosing artifact) and
//! labelled — never dropped. This is the faithful replacement for the legacy
//! concatenate-then-score-once model (which summed complexity, truncated at
//! 2 MiB, and destroyed coverage ratios).

use crate::aggregate::{AggKind, FileScore, aggregate};
use crate::scope::{Scope, file_loc};
use crate::verifications::run_verification;
use crate::{
    DimId, DimScore, DimStatus, Tier, compute_composite, default_weights, tier_from_composite,
};
use anyhow::Result;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A scope-level quality report — all 50 dimensions, at any granularity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeReport {
    /// The scope granularity (`"file"`..`"workspace"`).
    pub scope_kind: String,
    /// The scope root.
    pub root: PathBuf,
    /// Number of source files scored.
    pub file_count: usize,
    /// Total physical LOC across the scope.
    pub total_loc: usize,
    /// Per-dimension aggregated scores (always 50 when no filter is given).
    pub dimensions: BTreeMap<DimId, DimScore>,
    /// Weighted composite (0.0..1.0).
    pub composite: f32,
    /// Tier (Diamond..Unranked).
    pub tier: Tier,
    /// Dimensions that FAIL (<0.5) — BLOCK dims here are hard gate failures.
    pub blockers: Vec<DimId>,
    /// Dimensions that WARN (0.5..0.8).
    pub warnings: Vec<DimId>,
    /// Schema version for forward-compat.
    pub schema_version: u32,
}

impl ScopeReport {
    /// Schema version constant.
    pub const SCHEMA_VERSION: u32 = 1;
}

/// Score a resolved [`Scope`] on a subset of dims (empty = all 50).
pub fn score_scope(scope: &Scope, dims: &[DimId]) -> Result<ScopeReport> {
    let dims_to_score: Vec<DimId> = if dims.is_empty() {
        DimId::ALL.to_vec()
    } else {
        dims.to_vec()
    };

    // Per-file LOC weights, computed once in parallel.
    let file_loc_pairs: Vec<(PathBuf, usize)> = scope
        .files
        .par_iter()
        .map(|f| (f.clone(), file_loc(f)))
        .collect();

    // Each dim: ScopeNative → once on the root; otherwise score-per-file → roll up.
    let dimensions: BTreeMap<DimId, DimScore> = dims_to_score
        .par_iter()
        .map(|&dim| {
            let score = match dim.agg_kind() {
                AggKind::ScopeNative => score_scope_native(dim, scope),
                kind => score_rolled_up(dim, kind, &file_loc_pairs),
            };
            (dim, score)
        })
        .collect();

    Ok(build_report(scope, dimensions))
}

/// ScopeNative dim: run the verifier **once** on the scope root. At sub-repo
/// granularity the dim is still present (inherited from the root artifact) and
/// labelled — honouring "all 50 dims at every scope".
fn score_scope_native(dim: DimId, scope: &Scope) -> DimScore {
    match run_verification(dim, &scope.root) {
        Ok(mut s) => {
            if !scope.kind.is_repo_or_larger() {
                s.evidence = format!("[scope-native @ {} root] {}", scope.kind, s.evidence);
            }
            s
        }
        Err(e) => DimScore::from_value(0.0, format!("scope-native verifier error: {e}")),
    }
}

/// Non-ScopeNative dim: score each file (parallel) then aggregate by `kind`.
fn score_rolled_up(dim: DimId, kind: AggKind, files: &[(PathBuf, usize)]) -> DimScore {
    let per_file: Vec<(PathBuf, f32, usize)> = files
        .iter()
        .map(|(path, loc)| {
            let v = run_verification(dim, path).map(|s| s.value).unwrap_or(0.0);
            (path.clone(), v, *loc)
        })
        .collect();
    let refs: Vec<FileScore<'_>> = per_file
        .iter()
        .map(|(p, v, l)| (p.as_path(), *v, *l))
        .collect();
    aggregate(kind, &refs)
}

fn build_report(scope: &Scope, dimensions: BTreeMap<DimId, DimScore>) -> ScopeReport {
    let composite = compute_composite(&dimensions, default_weights());
    // W5 (2026-07-02): quality gate — a BLOCK dim failure disqualifies the tier.
    let tier = crate::composite::apply_quality_gate(tier_from_composite(composite), &dimensions);
    let mut blockers = vec![];
    let mut warnings = vec![];
    for (id, s) in &dimensions {
        match s.status {
            DimStatus::Fail => blockers.push(*id),
            DimStatus::Warn => warnings.push(*id),
            // NotApplicable never blocks or warns (W3).
            DimStatus::Pass | DimStatus::NotApplicable => {}
        }
    }
    ScopeReport {
        scope_kind: scope.kind.to_string(),
        root: scope.root.clone(),
        file_count: scope.file_count(),
        total_loc: scope.total_loc(),
        dimensions,
        composite,
        tier,
        blockers,
        warnings,
        schema_version: ScopeReport::SCHEMA_VERSION,
    }
}

/// Render a [`ScopeReport`] as a compact human-readable summary.
pub fn render_compact(report: &ScopeReport) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "{} [{}] — {:.3} ({})\n  {} files, {} LOC, {} dims, {} blockers, {} warnings\n",
        report.root.display(),
        report.scope_kind,
        report.composite,
        report.tier,
        report.file_count,
        report.total_loc,
        report.dimensions.len(),
        report.blockers.len(),
        report.warnings.len(),
    ));
    for (id, dim) in &report.dimensions {
        let marker = match dim.status {
            DimStatus::Pass => "✓",
            DimStatus::Warn => "⚠",
            DimStatus::Fail => "✗",
            DimStatus::NotApplicable => "○",
        };
        s.push_str(&format!(
            "  {} {} [{}] {:.3} — {}\n",
            marker,
            id,
            id.agg_kind().as_str(),
            dim.value,
            dim.evidence
        ));
    }
    s
}

/// Render a [`ScopeReport`] as pretty JSON.
pub fn render_json(report: &ScopeReport) -> Result<String> {
    serde_json::to_string_pretty(report).map_err(|e| anyhow::anyhow!("json render failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ScopeKind;

    /// Build a small fixture tree and return its root.
    fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (rel, body) in files {
            let p = dir.join(rel);
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(&p, body).unwrap();
        }
        dir
    }

    #[test]
    fn all_50_dims_present_at_path_scope() {
        let dir = fixture(
            "tq_sr_all50",
            &[
                ("a.rs", "pub fn a() -> i32 { 1 }\n"),
                ("b.rs", "pub fn b() -> i32 { 2 }\n"),
            ],
        );
        let scope = Scope::resolve(&dir, Some(ScopeKind::Path), &[], &[]).unwrap();
        let report = score_scope(&scope, &[]).unwrap();
        assert_eq!(
            report.dimensions.len(),
            50,
            "every report carries all 50 dims"
        );
        assert_eq!(report.file_count, 2);
        assert!(report.total_loc >= 2);
        assert!((0.0..=1.0).contains(&report.composite));
        // scope-native dims present (e.g. F1.8 dep-cycles)
        assert!(report.dimensions.contains_key(&DimId::F1_8));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn secret_in_one_file_drags_scope_worst_of() {
        // F2.4 is WorstOf: a secret in ONE of three files must pull the scope
        // F2.4 score at or below the clean baseline (it cannot be washed out).
        let clean = fixture(
            "tq_sr_clean",
            &[
                ("a.rs", "pub fn a() {}\n"),
                ("b.rs", "pub fn b() {}\n"),
                ("c.rs", "pub fn c() {}\n"),
            ],
        );
        let dirty = fixture(
            "tq_sr_dirty",
            &[
                ("a.rs", "pub fn a() {}\n"),
                (
                    "b.rs",
                    "pub const TOKEN: &str = \"ghp_0123456789abcdef0123456789abcdef0123\";\n",
                ),
                ("c.rs", "pub fn c() {}\n"),
            ],
        );
        let cs = Scope::resolve(&clean, Some(ScopeKind::Path), &[], &[]).unwrap();
        let ds = Scope::resolve(&dirty, Some(ScopeKind::Path), &[], &[]).unwrap();
        let cr = score_scope(&cs, &[DimId::F2_4]).unwrap();
        let dr = score_scope(&ds, &[DimId::F2_4]).unwrap();
        let clean_f24 = cr.dimensions[&DimId::F2_4].value;
        let dirty_f24 = dr.dimensions[&DimId::F2_4].value;
        assert!(
            dirty_f24 <= clean_f24,
            "secret tree F2.4 ({dirty_f24}) must not exceed clean ({clean_f24})"
        );
        let _ = std::fs::remove_dir_all(&clean);
        let _ = std::fs::remove_dir_all(&dirty);
    }

    #[test]
    fn render_compact_and_json_roundtrip() {
        let dir = fixture("tq_sr_render", &[("x.rs", "pub fn x() {}\n")]);
        let scope = Scope::resolve(&dir, Some(ScopeKind::Path), &[], &[]).unwrap();
        let report = score_scope(&scope, &[DimId::F1_1, DimId::F2_4]).unwrap();
        let compact = render_compact(&report);
        assert!(compact.contains("path"));
        let json = render_json(&report).unwrap();
        let back: ScopeReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dimensions.len(), report.dimensions.len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
