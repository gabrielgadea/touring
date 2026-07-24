//! F4.5 — Package Management verifier (D44).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::DepHealthAnalyzer`] — hermetic cargo-deny
//! `[bans]` (wildcards, multiple-versions) + RustSec informational
//! (unmaintained/unsound, deferred here from F2.5) + cargo-machete (unused deps)
//! analysis of the manifest + resolved `Cargo.lock`. Only an author-controlled
//! registry wildcard in `[dependencies]`/`[build-dependencies]` drives the BLOCK
//! (`bans.wildcards = "deny"`); every other signal is hygiene-capped so it can
//! WARN but never fail-close (the anti-theater invariant — F4.5 must not
//! re-introduce the transitive-debt false positive F2.5's CVE-vs-informational
//! partition was built to avoid).
//!
//! **Standalone fallback (`--no-default-features`)**: the prior dependency-count
//! heuristic, clearly labelled, so the crate stays standalone-buildable.
//!
//! **Scope**: a *manifest-level* dimension (`AggKind::ScopeNative`, like F2.5).
//! A non-manifest target (e.g. a `.rs` file) has no dependency tree of its own
//! and passes (1.0).

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F4.5 verifier — package-management hygiene.
#[allow(non_camel_case_types)]
pub struct F4_5_PkgMgmt;

impl Verification for F4_5_PkgMgmt {
    fn id(&self) -> DimId {
        DimId::F4_5
    }

    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_pkg_mgmt(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: cargo-deny bans + RustSec informational + cargo machete ──────
#[cfg(feature = "workspace-integration")]
fn analyze_pkg_mgmt(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::DepHealthAnalyzer;
    let report = DepHealthAnalyzer::new().analyze(target);

    if !report.manifest_scoped {
        return Ok((
            1.0,
            "[N/A] F4.5: not a Cargo project — dependency-health scan does not apply \
             (excluded from composite; npm/pip/go manifest backends are future work)"
                .to_string(),
        ));
    }

    // Table-driven evidence assembly: (label, findings, show_list).
    let sections: [(&str, &[String], bool); 5] = [
        ("prod wildcard(s) [BLOCK]", &report.wildcards, true),
        ("dev wildcard(s)", &report.dev_wildcards, false),
        ("unmaintained/unsound", &report.unmaintained, true),
        (
            "duplicate-version crate(s)",
            &report.duplicate_versions,
            true,
        ),
        ("unused dep(s)", &report.unused, true),
    ];
    let parts: Vec<String> = sections
        .iter()
        .filter(|(_, items, _)| !items.is_empty())
        .map(|(label, items, show_list)| {
            if *show_list {
                format!("{} {label}: {}", items.len(), cap_list(items))
            } else {
                format!("{} {label}", items.len())
            }
        })
        .collect();

    let evidence = if parts.is_empty() {
        let note = if report.note.is_empty() {
            String::new()
        } else {
            format!(" ({})", report.note)
        };
        format!("F4.5: clean package management (cargo-deny bans + machete){note}")
    } else {
        let note = if report.note.is_empty() {
            String::new()
        } else {
            format!(" | {}", report.note)
        };
        format!(
            "F4.5: {} — score={:.3}{note}",
            parts.join("; "),
            report.score
        )
    };
    Ok((report.score, evidence))
}

/// Join at most the first 4 findings for evidence (full count is in the prefix).
#[cfg(feature = "workspace-integration")]
fn cap_list(items: &[String]) -> String {
    let shown = items[..items.len().min(4)].join(", ");
    if items.len() > 4 {
        format!("{shown} (+{} more)", items.len() - 4)
    } else {
        shown
    }
}

// ── Standalone fallback: dependency-count heuristic ───────────────────────────
#[cfg(not(feature = "workspace-integration"))]
const SOFT_CAPS: &[(&str, u32)] = &[
    ("Cargo.toml", 50),
    ("package.json", 100),
    ("pyproject.toml", 30),
    ("requirements.txt", 30),
];

#[cfg(not(feature = "workspace-integration"))]
fn analyze_pkg_mgmt(target: &Path) -> Result<(f32, String)> {
    let content = if target.is_dir() {
        try_read_manifest_for(target, SOFT_CAPS.iter().map(|(n, _)| *n).collect())
            .unwrap_or_default()
    } else {
        std::fs::read_to_string(target)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", target.display(), e))?
    };

    let cargo_deps = content.matches("= \"").count();
    let npm_deps = content.matches("\": \"^").count() + content.matches("\": \"~").count();
    let pip_deps = content
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && l.contains("=="))
        .count();
    let total = cargo_deps + npm_deps + pip_deps;

    let filename = target.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let cap: usize = SOFT_CAPS
        .iter()
        .find(|(n, _)| *n == filename)
        .map(|(_, c)| *c as usize)
        .unwrap_or(50);

    let value = if total == 0 {
        1.0
    } else if total >= 2 * cap {
        0.0
    } else {
        (1.0 - (total as f32) / (2.0 * cap as f32)).max(0.0)
    };
    let evidence = format!(
        "{total} total deps (cargo={cargo_deps}, npm={npm_deps}, pip={pip_deps}, cap={cap}) \
         — count heuristic (build --features workspace-integration for cargo-deny bans + machete)"
    );
    Ok((value, evidence))
}

#[cfg(not(feature = "workspace-integration"))]
fn try_read_manifest_for(dir: &Path, names: Vec<&str>) -> Result<String> {
    for n in names {
        let p = dir.join(n);
        if p.exists()
            && let Ok(s) = std::fs::read_to_string(&p)
        {
            return Ok(s);
        }
    }
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    // Real-engine tests (default `workspace-integration`).
    #[cfg(feature = "workspace-integration")]
    mod real_engine_tests {
        use super::super::*;
        use std::io::Write;

        #[test]
        fn non_manifest_target_passes() {
            let mut f = tempfile::Builder::new()
                .suffix(".rs")
                .tempfile()
                .expect("temp");
            writeln!(f, "fn main() {{}}").expect("write");
            let s = F4_5_PkgMgmt.check(f.path()).expect("check");
            // W3 (2026-07-02): a non-Cargo target is now NotApplicable (excluded
            // from the composite), not a silent Pass.
            assert_eq!(
                s.status,
                crate::DimStatus::NotApplicable,
                "non-Cargo target must be NotApplicable, got {:?}",
                s.status
            );
            assert!(s.evidence.contains("not a Cargo project"));
        }

        #[test]
        fn prod_wildcard_manifest_blocks() {
            let dir = tempfile::tempdir().expect("tmpdir");
            std::fs::write(
                dir.path().join("Cargo.toml"),
                "[package]\nname=\"x\"\nversion=\"0.1.0\"\n[dependencies]\nfoo = \"*\"\n",
            )
            .expect("write");
            let s = F4_5_PkgMgmt.check(dir.path()).expect("check");
            assert_eq!(
                s.status,
                crate::DimStatus::Fail,
                "prod wildcard must BLOCK: {}",
                s.value
            );
            assert!(s.evidence.contains("BLOCK"));
        }

        #[test]
        fn clean_manifest_passes() {
            let dir = tempfile::tempdir().expect("tmpdir");
            std::fs::write(
                dir.path().join("Cargo.toml"),
                "[package]\nname=\"x\"\nversion=\"0.1.0\"\n[dependencies]\nserde = \"1.0\"\n",
            )
            .expect("write");
            let s = F4_5_PkgMgmt.check(dir.path()).expect("check");
            assert!((0.0..=1.0).contains(&s.value));
            assert!(!s.suggestions.is_empty());
        }

        #[test]
        fn returns_valid_score_and_no_panic() {
            let s = F4_5_PkgMgmt
                .check(std::path::Path::new("Cargo.toml"))
                .expect("check");
            assert!((0.0..=1.0).contains(&s.value));
        }
    }

    // Fallback tests (standalone `--no-default-features`).
    #[cfg(not(feature = "workspace-integration"))]
    mod fallback_tests {
        use super::super::*;
        use std::io::Write;
        use tempfile::NamedTempFile;

        fn write_temp(content: &str) -> NamedTempFile {
            let mut f = NamedTempFile::new().expect("create temp");
            f.write_all(content.as_bytes()).expect("write");
            f
        }

        #[test]
        fn test_few_cargo_deps_scores_high() {
            let f = write_temp("[dependencies]\nserde = \"1.0\"\ntokio = \"1.0\"\n");
            let s = F4_5_PkgMgmt.check(f.path()).expect("check");
            assert!(s.value > 0.9, "2 deps should be perfect, got {}", s.value);
        }

        #[test]
        fn test_many_cargo_deps_lower_score() {
            let deps: String = (0..80)
                .map(|i| format!("crate_{i} = \"1.0\""))
                .collect::<Vec<_>>()
                .join("\n");
            let content = format!("[dependencies]\n{deps}");
            let f = write_temp(&content);
            let s = F4_5_PkgMgmt.check(f.path()).expect("check");
            assert!(
                s.value < 0.5,
                "80 deps should be warn/fail, got {}",
                s.value
            );
        }
    }
}
