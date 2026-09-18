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

    // Truncagem do corpus: UMA vez por escopo, não uma por dim. É o mesmo
    // diretório e a mesma resposta para as 14 dims scope-native, e a checagem
    // percorre a árvore inteira fazendo `metadata` de cada arquivo — 14 varreduras
    // concorrentes (as dims rodam em `par_iter`) de ~2000 arquivos cada, onde uma
    // basta.
    let scan_overflow = crate::verifications::dir_scan_overflow(&scope.root);

    // The security corpus: the scope's files plus its vendored code, weighed the
    // same way (cross-audit 14/09/2026, D3 — `third_party/` had left F2.1/F2.4).
    let security_loc_pairs: Vec<(PathBuf, usize)> = if scope.vendored_files.is_empty() {
        file_loc_pairs.clone()
    } else {
        let mut pairs = file_loc_pairs.clone();
        pairs.extend(
            scope
                .vendored_files
                .par_iter()
                .map(|f| (f.clone(), file_loc(f)))
                .collect::<Vec<_>>(),
        );
        pairs
    };

    // Each dim: ScopeNative → once on the root; otherwise score-per-file → roll up.
    let dimensions: BTreeMap<DimId, DimScore> = dims_to_score
        .par_iter()
        .map(|&dim| {
            let score = match dim.agg_kind() {
                AggKind::ScopeNative => score_scope_native(dim, scope, scan_overflow),
                AggKind::PerCrateNative => score_per_crate_native(dim, scope),
                kind if reads_vendored_code(dim) => score_rolled_up(dim, kind, &security_loc_pairs),
                kind => score_rolled_up(dim, kind, &file_loc_pairs),
            };
            (dim, score)
        })
        .collect();

    Ok(build_report(scope, dimensions))
}

/// Whether `dim` scores vendored code: the per-file security dims. F2.6 is
/// scope-native and reads its own security corpus (`enumerate_security_files`).
fn reads_vendored_code(dim: DimId) -> bool {
    matches!(dim, DimId::F2_1 | DimId::F2_4)
}

/// ScopeNative dim: run the verifier **once** on the scope root. At sub-repo
/// granularity the dim is still present (inherited from the root artifact) and
/// labelled — honouring "all 50 dims at every scope".
fn score_scope_native(dim: DimId, scope: &Scope, scan_overflow: Option<u64>) -> DimScore {
    match run_verification(dim, &scope.root) {
        Ok(mut s) => {
            if !scope.kind.is_repo_or_larger() {
                s.evidence = format!("[scope-native @ {} root] {}", scope.kind, s.evidence);
            }
            // Um score de PREFIXO jamais se apresenta como score do escopo. As
            // 14 dims scope-native concatenam a raiz inteira e param no teto de
            // bytes; quando isso acontece, quem lê a evidência precisa saber que
            // a cauda do corpus não foi medida. Este é o ponto único por onde as
            // 14 passam — anotar aqui cobre todas sem tocar 83 call sites.
            // `scan_overflow` vem calculado UMA vez pelo chamador.
            if let Some(total) = scan_overflow {
                s.evidence = format!(
                    "[TRUNCADO: corpus {:.1} MB > teto de varredura; medida parcial] {}",
                    total as f64 / (1024.0 * 1024.0),
                    s.evidence
                );
                // A prosa acima informa um humano; este campo informa uma MÁQUINA.
                // Sem ele o gate de convergência não tinha como distinguir um score
                // de escopo de um score de prefixo, e aceitava o segundo em silêncio.
                s.truncated = true;
            }
            s
        }
        Err(e) => DimScore::from_value(0.0, format!("scope-native verifier error: {e}")),
    }
}

/// Raízes de crate (diretórios com `Cargo.toml`) sob `root`, sem descer em
/// crates aninhados — cada uma é uma unidade de medição independente.
fn crate_roots(root: &std::path::Path) -> Vec<PathBuf> {
    if root.join("Cargo.toml").is_file() && root.join("src").is_dir() {
        return vec![root.to_path_buf()];
    }
    let mut found = Vec::new();
    let git = touring_foundation::gitignore::GitIgnoreRules::for_path(root);
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if !p.is_dir() || git.ignored_abs(&p, true).is_some() {
                continue;
            }
            // The same predicate file enumeration reads (`SKIP_DIRS` + dot-dirs):
            // a private copy here kept `third_party/` and `vendor/` crates in the
            // workspace score after enumeration had dropped them.
            let skip = p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(crate::verifications::is_skipped_dir_name);
            if skip {
                continue;
            }
            if p.join("Cargo.toml").is_file() {
                found.push(p); // não empilha: crates aninhados pertencem a este
            } else {
                stack.push(p);
            }
        }
    }
    found.sort();
    found
}

/// `PerCrateNative`: roda o verificador **uma vez por crate** e compõe por LOC.
///
/// Substitui a concatenação única do escopo, que era truncada em 16 MiB e — por
/// cortar em bytes — devolvia um número insensível a remediação real. Cada crate
/// cabe sob o teto, então cada medição parcial é honesta e a composição também.
///
/// Com 0 ou 1 crate no escopo não há o que compor: cai para o caminho
/// `ScopeNative`, que nesse caso já é exato.
fn score_per_crate_native(dim: DimId, scope: &Scope) -> DimScore {
    let roots = crate_roots(&scope.root);
    if roots.len() < 2 {
        // Caminho de fallback (raro): mesmo escopo, mesma resposta — computar aqui
        // não recria as 14 varreduras que a hoisting no chamador eliminou.
        return score_scope_native(
            dim,
            scope,
            crate::verifications::dir_scan_overflow(&scope.root),
        );
    }

    let scored: Vec<(PathBuf, f32, usize)> = roots
        .par_iter()
        .filter_map(|cr| {
            let value = run_verification(dim, cr).ok()?.value;
            let loc: usize = crate::verifications::enumerate_source_files(cr)
                .iter()
                .map(|f| file_loc(f))
                .sum();
            (loc > 0).then_some((cr.clone(), value, loc))
        })
        .collect();

    if scored.is_empty() {
        // Caminho de fallback (raro): mesmo escopo, mesma resposta — computar aqui
        // não recria as 14 varreduras que a hoisting no chamador eliminou.
        return score_scope_native(
            dim,
            scope,
            crate::verifications::dir_scan_overflow(&scope.root),
        );
    }

    let total_loc: f64 = scored.iter().map(|(_, _, l)| *l as f64).sum();
    let weighted: f64 = scored
        .iter()
        .map(|(_, v, l)| f64::from(*v) * (*l as f64))
        .sum();
    let value = (weighted / total_loc) as f32;

    let mut worst = scored.iter().collect::<Vec<_>>();
    worst.sort_by(|a, b| a.1.total_cmp(&b.1));
    let worst_names: Vec<String> = worst
        .iter()
        .take(3)
        .map(|(p, v, _)| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            format!("{name}={v:.3}")
        })
        .collect();

    DimScore::from_value(
        value,
        format!(
            "[per-crate-native] LOC-weighted over {} crate(s), {total_loc:.0} LOC \
             — cada crate medido inteiro (sem o teto de 16 MiB que truncava a \
             concatenação do escopo); piores: {}",
            scored.len(),
            worst_names.join(", ")
        ),
    )
}

/// Non-ScopeNative dim: score each file then aggregate by `kind`.
fn score_rolled_up(dim: DimId, kind: AggKind, files: &[(PathBuf, usize)]) -> DimScore {
    let scored: Vec<(PathBuf, Result<DimScore>, usize)> = files
        .iter()
        .map(|(path, loc)| (path.clone(), run_verification(dim, path), *loc))
        .collect();
    roll_up(kind, &scored)
}

/// Fold per-file verifier results into one scope-level score.
///
/// A file the verifier reports as `NotApplicable` is left out (Canvas D,
/// 15/09/2026): keeping only its value turned the `[N/A]` into a 1.0 Pass
/// inside the LOC-weighted mean, so every inapplicable file inflated the scope.
/// When no file applies, the dimension does not apply either. An engine error
/// still weighs 0.0 — fail-closed, never skipped.
fn roll_up(kind: AggKind, scored: &[(PathBuf, Result<DimScore>, usize)]) -> DimScore {
    let applicable: Vec<FileScore<'_>> = scored
        .iter()
        .filter_map(|(path, result, loc)| match result {
            Ok(s) if s.status == DimStatus::NotApplicable => None,
            Ok(s) => Some((path.as_path(), s.value, *loc)),
            Err(_) => Some((path.as_path(), 0.0, *loc)),
        })
        .collect();
    let inapplicable = scored.len() - applicable.len();
    if applicable.is_empty() && inapplicable > 0 {
        return DimScore::not_applicable(format!(
            "[N/A] none of the {inapplicable} file(s) in scope applies to this dimension \
             (excluded from composite)"
        ));
    }
    let mut score = aggregate(kind, &applicable);
    if inapplicable > 0 {
        score
            .evidence
            .push_str(&format!(" · {inapplicable} inapplicable file(s) left out"));
    }
    score
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

    /// Crate discovery skips what file enumeration skips (14/09/2026): the
    /// vendored `third_party/tree-sitter-md` (140k LOC, F1.3 0.100) stayed a
    /// crate of the workspace score because this walk kept its own skip list
    /// while `enumerate_source_files` read `SKIP_DIRS`, and it pulled the
    /// LOC-weighted F1.3 from 0.536 to 0.470.
    #[test]
    fn crate_roots_skip_the_directories_file_enumeration_skips() {
        let manifest = "[package]\nname = \"x\"\n";
        let root = fixture(
            "tq_crate_roots_skip_dirs",
            &[
                ("crates/a/Cargo.toml", manifest),
                ("crates/a/src/lib.rs", "fn a() {}\n"),
                ("crates/b/Cargo.toml", manifest),
                ("crates/b/src/lib.rs", "fn b() {}\n"),
                ("third_party/tree-sitter-md/Cargo.toml", manifest),
                ("third_party/tree-sitter-md/src/lib.rs", "fn v() {}\n"),
                ("vendor/dep/Cargo.toml", manifest),
                ("vendor/dep/src/lib.rs", "fn d() {}\n"),
            ],
        );
        assert_eq!(
            crate_roots(&root),
            vec![root.join("crates/a"), root.join("crates/b")]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

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
        let clean_f24 = &cr.dimensions[&DimId::F2_4];
        let dirty_f24 = &dr.dimensions[&DimId::F2_4];
        // This test only asserted `dirty <= clean` — which the LOC-weighted mean
        // satisfied while scoring the secret tree Warn, with no blocker
        // (cross-audit 14/09/2026, D2). A BLOCK dim must FAIL the scope.
        assert_eq!(
            clean_f24.status,
            crate::DimStatus::Pass,
            "{}",
            clean_f24.evidence
        );
        assert_eq!(
            dirty_f24.status,
            crate::DimStatus::Fail,
            "one hardcoded token fails the scope: {}",
            dirty_f24.evidence
        );
        assert!(
            dr.blockers.contains(&DimId::F2_4),
            "F2.4 blocks: {:?}",
            dr.blockers
        );
        let _ = std::fs::remove_dir_all(&clean);
        let _ = std::fs::remove_dir_all(&dirty);
    }

    /// Cross-audit 14/09/2026, D3: `third_party/` left the security corpus with the
    /// style one, so a secret in `src/third_party/` scored Pass. Vendored code is
    /// skipped by the style dims and scanned by the security dims.
    #[test]
    fn a_secret_in_vendored_code_fails_the_security_dims_but_not_the_style_corpus() {
        let dir = fixture(
            "tq_sr_vendored_secret",
            &[
                ("src/lib.rs", "pub fn a() {}\n"),
                (
                    "src/third_party/keys.rs",
                    "pub const TOKEN: &str = \"ghp_0123456789abcdef0123456789abcdef0123\";\n",
                ),
                ("vendor/dep/lib.rs", "pub fn upstream() {}\n"),
            ],
        );
        let scope = Scope::resolve(&dir, Some(ScopeKind::Path), &[], &[]).unwrap();
        assert_eq!(scope.files.len(), 1, "style corpus: {:?}", scope.files);
        assert_eq!(
            scope.vendored_files.len(),
            2,
            "vendored corpus: {:?}",
            scope.vendored_files
        );
        let report = score_scope(&scope, &[DimId::F2_4]).unwrap();
        let f24 = &report.dimensions[&DimId::F2_4];
        assert_eq!(f24.status, crate::DimStatus::Fail, "{}", f24.evidence);
        assert!(f24.evidence.contains("keys.rs"), "{}", f24.evidence);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn verdicts(
        rows: Vec<(&str, Result<DimScore>, usize)>,
    ) -> Vec<(PathBuf, Result<DimScore>, usize)> {
        rows.into_iter()
            .map(|(p, r, loc)| (PathBuf::from(p), r, loc))
            .collect()
    }

    /// Canvas D (15/09/2026): the roll-up kept only `.value`, so a large
    /// inapplicable file weighed in as a 1.0 Pass and lifted a failing scope.
    #[test]
    fn an_inapplicable_file_is_left_out_of_the_roll_up_not_counted_as_a_pass() {
        let scored = verdicts(vec![
            (
                "src/a.rs",
                Ok(DimScore::from_value(0.4, "real finding")),
                10,
            ),
            (
                "scripts/big.sh",
                Ok(DimScore::not_applicable("[N/A] shell")),
                1000,
            ),
        ]);
        let s = roll_up(AggKind::WeightedLoc, &scored);
        assert!((s.value - 0.4).abs() < 1e-6, "{}", s.evidence);
        assert_eq!(s.status, DimStatus::Fail, "{}", s.evidence);
        assert!(
            s.evidence.contains("1 inapplicable file(s) left out"),
            "{}",
            s.evidence
        );
        // The same file applicable and clean is weighed as before.
        let applicable = verdicts(vec![
            (
                "src/a.rs",
                Ok(DimScore::from_value(0.4, "real finding")),
                10,
            ),
            (
                "scripts/big.sh",
                Ok(DimScore::from_value(1.0, "clean")),
                1000,
            ),
        ]);
        assert!(roll_up(AggKind::WeightedLoc, &applicable).value > 0.99);
    }

    #[test]
    fn a_dimension_that_applies_to_no_file_is_not_applicable() {
        let scored = verdicts(vec![
            ("a.sh", Ok(DimScore::not_applicable("[N/A] shell")), 5),
            ("b.sh", Ok(DimScore::not_applicable("[N/A] shell")), 7),
        ]);
        let s = roll_up(AggKind::FailClosedLoc, &scored);
        assert_eq!(s.status, DimStatus::NotApplicable, "{}", s.evidence);
        assert!(
            s.evidence.starts_with("[N/A] none of the 2 file(s)"),
            "{}",
            s.evidence
        );
    }

    #[test]
    fn an_engine_error_still_fails_closed_beside_inapplicable_files() {
        let scored = verdicts(vec![
            ("a.sh", Ok(DimScore::not_applicable("[N/A] shell")), 5),
            ("b.rs", Err(anyhow::anyhow!("engine exploded")), 7),
        ]);
        let s = roll_up(AggKind::WorstOf, &scored);
        assert_eq!(s.value, 0.0, "{}", s.evidence);
        assert_eq!(s.status, DimStatus::Fail, "{}", s.evidence);
    }

    /// Canvas D (15/09/2026): a directory the collector reads nothing from
    /// scored `Pass 1.0 "vacuously satisfied"` on every per-file dimension, so a
    /// P0 `check` answered Diamond having read no file.
    #[test]
    fn a_scope_with_no_source_file_is_not_applicable_on_the_per_file_dims() {
        let dir = fixture("tq_sr_no_source", &[("README.md", "# nothing to score\n")]);
        let scope = Scope::resolve(&dir, Some(ScopeKind::Path), &[], &[]).unwrap();
        assert!(scope.files.is_empty(), "{:?}", scope.files);
        let report = score_scope(&scope, &[DimId::F1_1, DimId::F2_1, DimId::F2_4]).unwrap();
        for dim in [DimId::F1_1, DimId::F2_1, DimId::F2_4] {
            let s = &report.dimensions[&dim];
            assert_eq!(
                s.status,
                DimStatus::NotApplicable,
                "{dim:?}: {}",
                s.evidence
            );
            assert_eq!(s.evidence, crate::aggregate::EMPTY_SCOPE_EVIDENCE);
        }
        assert!(report.blockers.is_empty(), "{:?}", report.blockers);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Canvas D (15/09/2026): a token in a shell script never reached the P0 gate
    /// of a directory score, and the script took no part in any other dimension.
    #[test]
    fn a_token_in_a_shell_script_fails_the_directory_and_style_dims_leave_the_script_out() {
        let dir = fixture(
            "tq_sr_shell_corpus",
            &[
                ("src/lib.rs", "pub fn a() -> i32 { 1 }\n"),
                (
                    "scripts/deploy.sh",
                    "#!/bin/bash\nTOKEN=\"ghp_0123456789abcdef0123456789abcdef0123\"\ncurl -H \"Authorization: $TOKEN\" x\n",
                ),
            ],
        );
        let scope = Scope::resolve(&dir, Some(ScopeKind::Path), &[], &[]).unwrap();
        assert_eq!(scope.files.len(), 2, "{:?}", scope.files);
        let report = score_scope(&scope, &[DimId::F2_4, DimId::F1_9]).unwrap();
        let f24 = &report.dimensions[&DimId::F2_4];
        assert_eq!(f24.status, DimStatus::Fail, "{}", f24.evidence);
        assert!(f24.evidence.contains("deploy.sh"), "{}", f24.evidence);
        let f19 = &report.dimensions[&DimId::F1_9];
        assert_ne!(f19.status, DimStatus::NotApplicable, "{}", f19.evidence);
        assert!(
            f19.evidence.contains("1 inapplicable file(s) left out"),
            "{}",
            f19.evidence
        );
        let _ = std::fs::remove_dir_all(&dir);
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
