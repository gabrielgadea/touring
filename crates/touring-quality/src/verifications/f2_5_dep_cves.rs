//! F2.5 — Dependency CVEs verifier.
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::security::SecurityDb`] — the in-process RustSec advisory
//! database (`~/.cargo/advisory-db`). It resolves the manifest's `Cargo.lock`
//! (the *resolved, transitive* dependency tree — `Cargo.toml` only carries
//! requirements like `"1.0"`, not the resolved versions an advisory matches)
//! and reports every dependency affected by a known RUSTSEC advisory. The prior
//! W1 MVP was a 4-entry hardcoded substring list that missed every real CVE in
//! the tree.
//!
//! **Standalone fallback (`--no-default-features`)**: the labelled hardcoded
//! known-bad substring smoke test, so the crate stays standalone-buildable.
//!
//! **Scope**: a *manifest-level* dimension (`AggKind::ScopeNative`). A
//! non-manifest target (e.g. a `.rs` file) has no dependency tree of its own, so
//! it passes (1.0) — the dep-CVE question is answered once, at the
//! manifest/lockfile. This keeps a per-file BLOCK hook from gating every source
//! edit on a project-level CVE.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F2.5 verifier — dependency CVEs via the RustSec advisory database.
#[allow(non_camel_case_types)]
pub struct F2_5_DepCves;

impl Verification for F2_5_DepCves {
    fn id(&self) -> DimId {
        DimId::F2_5
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_dep_cves(target)
    }
}

// ── Real engine: RustSec advisory DB over the resolved Cargo.lock ──────────
#[cfg(feature = "workspace-integration")]
fn analyze_dep_cves(target: &Path) -> Result<(f32, String)> {
    Ok(real_engine::scan(target))
}

#[cfg(feature = "workspace-integration")]
mod real_engine {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, OnceLock};
    use touring_analysis::security::{SecurityAdvisory, SecurityDb};

    /// True when the target is a Rust dependency root (a `Cargo.toml`/`Cargo.lock`
    /// or a directory to search). Other files have no dependency tree.
    fn is_rust_manifest(target: &Path) -> bool {
        target.is_dir()
            || matches!(
                target.file_name().and_then(|n| n.to_str()),
                Some("Cargo.toml" | "Cargo.lock")
            )
    }

    /// Walk up from the manifest's directory to the nearest `Cargo.lock` (the
    /// workspace root holds the single resolved lockfile).
    fn find_lockfile(target: &Path) -> Option<PathBuf> {
        let mut dir = if target.is_dir() {
            target.to_path_buf()
        } else {
            target.parent()?.to_path_buf()
        };
        loop {
            let candidate = dir.join("Cargo.lock");
            if candidate.is_file() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Per-lockfile scan-result cache: every crate manifest in a workspace
    /// resolves to the same root `Cargo.lock`, so the (DB load + scan) runs once
    /// per distinct lockfile rather than once per manifest in a workspace sweep.
    fn cache() -> &'static Mutex<HashMap<PathBuf, Arc<Vec<SecurityAdvisory>>>> {
        static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<SecurityAdvisory>>>>> =
            OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    /// Score a manifest target against the RustSec advisory database.
    /// Returns `(score, evidence)`; `0.0` (BLOCK) when ≥1 advisory matches the
    /// resolved tree, `1.0` otherwise (incl. graceful offline / no-lock).
    pub(super) fn scan(target: &Path) -> (f32, String) {
        if !is_rust_manifest(target) {
            // W3 (2026-07-02): non-Cargo target → NotApplicable (excluded from
            // the composite), not a silent Pass. A JS/Python/Go project has no
            // Cargo advisory tree to scan; inflating it with Pass 1.0 was the
            // poliglota fail-open.
            //
            // P-E (2026-07-03): enrich the evidence with the OSV.dev offline
            // coverage summary — the dimension now *sees* the npm/PyPI/Go
            // dependency tree it cannot scan offline (the live OSV lookup is
            // opt-in/network). Status stays NotApplicable (no composite change).
            let dir = if target.is_dir() {
                target
            } else {
                target.parent().unwrap_or(target)
            };
            let osv_note = touring_analysis::osv::offline_summary(dir)
                .map(|s| format!(" — {s}"))
                .unwrap_or_default();
            return (
                1.0,
                format!(
                    "[N/A] F2.5: not a Cargo project — RustSec advisory scan does not \
                     apply (excluded from composite){osv_note}"
                ),
            );
        }
        let Some(lockfile) = find_lockfile(target) else {
            return (
                1.0,
                "F2.5: no Cargo.lock found (manifest unresolved) — pass".to_string(),
            );
        };
        let key = lockfile.canonicalize().unwrap_or_else(|_| lockfile.clone());

        // Fast path: cached scan result for this lockfile.
        if let Ok(guard) = cache().lock()
            && let Some(hit) = guard.get(&key)
        {
            return score(&hit[..]);
        }

        let db = SecurityDb::try_open();
        if !db.is_online() {
            // FAIL-SAFE (W2 2026-07-02): a P0 CVE gate that CANNOT verify must
            // NOT report a clean pass — that is the exact silent fail-open the
            // audit flagged (CI / fresh machine without advisory-db passing
            // every dependency). Surface it below the Gold floor with an
            // actionable fix, without hard-blocking offline builds (0.5 = Warn,
            // not Fail). W3/W5 will formalise this as an `Unverified` status.
            return (
                0.5,
                "F2.5: UNVERIFIED — RustSec advisory DB offline (~/.cargo/advisory-db \
                 absent); cannot confirm the dependency tree is CVE-free. Fix: \
                 `cargo install cargo-audit && cargo audit fetch` (or clone \
                 rustsec/advisory-db). Not a clean pass."
                    .to_string(),
            );
        }
        let advisories = Arc::new(db.scan_lockfile(&lockfile));
        // Apply `deny.toml [advisories] ignore` workspace policy (the
        // canonical cargo-deny convention — each ignore entry MUST carry
        // a documented rationale). Same pattern as `f4_5_pkg_mgmt::DepHealthAnalyzer`
        // to keep the workspace policy DRY across F2.5 (CVE gate) and F4.5
        // (informational hygiene). A transient CVE that's pinned to a
        // parent-bump update is a documented accepted-risk, not a BLOCK.
        let ignored = load_advisories_ignore(&lockfile);
        let filtered: Arc<Vec<SecurityAdvisory>> = Arc::new(
            advisories
                .iter()
                .filter(|a| !ignored.contains(&a.id))
                .cloned()
                .collect(),
        );
        if let Ok(mut guard) = cache().lock() {
            guard.insert(key, Arc::clone(&filtered));
        }
        score(&filtered[..])
    }

    /// Parse `deny.toml [advisories] ignore` and return the set of RUSTSEC
    /// advisory IDs (e.g. `RUSTSEC-2026-0185`) explicitly whitelisted.
    /// Mirrors `f4_5_pkg_mgmt::load_advisories_ignore` — same workspace policy.
    /// Walks up from the lockfile's directory until it finds a `deny.toml`
    /// (the per-crate `crates/touring-quality/Cargo.lock` is not the same
    /// location as the workspace-root `deny.toml`, which lives at
    /// `/home/gabrielgadea/.claude/rust/deny.toml`).
    pub(super) fn load_advisories_ignore(lockfile: &Path) -> Vec<String> {
        // Walk up from `lockfile.parent()` until we find a deny.toml.
        let mut dir = match lockfile.parent() {
            Some(p) => p.to_path_buf(),
            None => return Vec::new(),
        };
        let deny_path = loop {
            let candidate = dir.join("deny.toml");
            if candidate.is_file() {
                break candidate;
            }
            if !dir.pop() {
                return Vec::new();
            }
        };
        let Ok(raw) = std::fs::read_to_string(&deny_path) else {
            return Vec::new();
        };
        // Scan every `id = "<…>"` occurrence and pick out RUSTSEC IDs.
        // Deny.toml uses `{ id = "RUSTSEC-XXXX-YYYY", reason = "..." }` table
        // entries under [advisories]; we don't care about which `[section]`
        // owns the entry because the format is identical across sections.
        let mut out: Vec<String> = Vec::new();
        let mut search_from = 0;
        while let Some(id_pos) = raw[search_from..].find("id") {
            let abs = search_from + id_pos;
            // Skip `id` substrings inside longer words (e.g. `valid`, `unique_id`,
            // `skip-tree`). Require the next char to be whitespace OR `=`.
            let next = abs + 2;
            if next < raw.len()
                && (raw.as_bytes()[next] == b' '
                    || raw.as_bytes()[next] == b'='
                    || raw.as_bytes()[next] == b'\t')
            {
                // Walk forward to the opening quote.
                let mut j = next + 1;
                while j < raw.len()
                    && (raw.as_bytes()[j] == b' '
                        || raw.as_bytes()[j] == b'='
                        || raw.as_bytes()[j] == b'\t')
                {
                    j += 1;
                }
                if j < raw.len() && raw.as_bytes()[j] == b'"' {
                    let start = j + 1;
                    if let Some(end) = raw[start..].find('"') {
                        let id = &raw[start..start + end];
                        if id.starts_with("RUSTSEC-") {
                            out.push(id.to_string());
                        }
                        search_from = start + end + 1;
                        continue;
                    }
                }
            }
            search_from = abs + 2;
        }
        out
    }

    /// Partition advisories into genuine vulnerabilities (RustSec `informational`
    /// field absent) and informational ones (unmaintained / unsound / notice).
    ///
    /// F2.5 is "Dependency CVEs" (D14): it BLOCKs (0.0) **only** on real
    /// vulnerabilities. Unmaintained/unsound crates are package-management
    /// concerns owned by F4.5 (D44), so they are surfaced as a non-blocking note
    /// rather than failing the CVE gate — otherwise an unmaintained-but-not-
    /// exploitable dep (e.g. `paste`) would block every manifest edit, a false
    /// positive for this dimension's stated purpose.
    pub(super) fn score(advisories: &[SecurityAdvisory]) -> (f32, String) {
        let mut vulns: Vec<String> = advisories
            .iter()
            .filter(|a| a.informational.is_none())
            .map(|a| format!("{}@{} ({})", a.package, a.version, a.id))
            .collect();
        vulns.sort();
        vulns.dedup();

        let mut infos: Vec<String> = advisories
            .iter()
            .filter_map(|a| {
                a.informational
                    .as_deref()
                    .map(|k| format!("{}@{} ({}, {})", a.package, a.version, a.id, k))
            })
            .collect();
        infos.sort();
        infos.dedup();

        let info_note = if infos.is_empty() {
            String::new()
        } else {
            let shown = infos[..infos.len().min(3)].join(", ");
            let more = if infos.len() > 3 {
                format!(" (+{} more)", infos.len() - 3)
            } else {
                String::new()
            };
            format!(
                " | {} non-blocking informational (unmaintained/unsound — see F4.5): {}{}",
                infos.len(),
                shown,
                more
            )
        };

        if vulns.is_empty() {
            return (
                1.0,
                format!("F2.5: 0 dependency CVEs in the resolved tree{info_note}"),
            );
        }
        let shown = vulns[..vulns.len().min(5)].join(", ");
        let more = if vulns.len() > 5 {
            format!(" (+{} more)", vulns.len() - 5)
        } else {
            String::new()
        };
        (
            0.0,
            format!(
                "F2.5: {} dependency CVE(s) in the resolved tree: {}{}{}",
                vulns.len(),
                shown,
                more,
                info_note
            ),
        )
    }
}

// ── Standalone fallback: hardcoded known-bad substring smoke test ──────────
#[cfg(not(feature = "workspace-integration"))]
const KNOWN_BAD: &[(&str, &str)] = &[
    ("serde", "1.0.130"),
    ("tokio", "0.2.0"),
    ("openssl", "0.10.55"),
    ("reqwest", "0.10.10"),
];

#[cfg(not(feature = "workspace-integration"))]
fn analyze_dep_cves(target: &Path) -> Result<(f32, String)> {
    let manifest_content = if target.is_dir() {
        try_read_manifest(target)?
    } else {
        std::fs::read_to_string(target)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", target.display(), e))?
    };
    let mut hits: Vec<String> = vec![];
    for (pkg, bad_ver) in KNOWN_BAD {
        let pattern_a = format!("{pkg} = \"{bad_ver}");
        let pattern_b = format!("\"{pkg}\" : \"{bad_ver}");
        if manifest_content.contains(&pattern_a) || manifest_content.contains(&pattern_b) {
            hits.push(format!("{pkg}@{bad_ver}"));
        }
    }
    let value = if hits.is_empty() { 1.0 } else { 0.0 };
    let evidence = if hits.is_empty() {
        "no known-bad dep versions (substring fallback — build --features workspace-integration for the RustSec engine)".to_string()
    } else {
        format!("{} vulnerable deps: {}", hits.len(), hits.join(", "))
    };
    Ok((value, evidence))
}

#[cfg(not(feature = "workspace-integration"))]
fn try_read_manifest(dir: &Path) -> Result<String> {
    for filename in &[
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "requirements.txt",
    ] {
        let p = dir.join(filename);
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
            // A `.rs` file has no dependency tree of its own → manifest-scoped pass.
            let mut f = tempfile::Builder::new()
                .suffix(".rs")
                .tempfile()
                .expect("temp");
            writeln!(f, "fn main() {{}}").expect("write");
            let s = F2_5_DepCves.check(f.path()).expect("check");
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
        fn non_cargo_target_reports_osv_coverage() {
            // P-E (2026-07-03): a non-Cargo target sitting next to an npm/PyPI/Go
            // manifest now enriches its NotApplicable evidence with the OSV.dev
            // offline coverage summary (status unchanged → no composite impact).
            let dir = tempfile::tempdir().expect("tmpdir");
            std::fs::write(
                dir.path().join("package.json"),
                r#"{"name":"a","dependencies":{"react":"18.2.0"}}"#,
            )
            .expect("write package.json");
            let target = dir.path().join("index.js");
            std::fs::write(&target, "// js\n").expect("write target");
            let s = F2_5_DepCves.check(&target).expect("check");
            assert_eq!(
                s.status,
                crate::DimStatus::NotApplicable,
                "non-Cargo stays NotApplicable (no composite impact)"
            );
            assert!(
                s.evidence.contains("OSV.dev coverage"),
                "OSV offline summary must enrich the evidence: {}",
                s.evidence
            );
            assert!(
                s.evidence.contains("npm"),
                "ecosystem named: {}",
                s.evidence
            );
        }

        #[test]
        fn manifest_without_lockfile_passes_gracefully() {
            let dir = tempfile::tempdir().expect("tmpdir");
            let toml = dir.path().join("Cargo.toml");
            std::fs::write(&toml, "[package]\nname=\"x\"\nversion=\"0.1.0\"\n").expect("write");
            // /tmp ancestry has no Cargo.lock → graceful pass. (If a runner's
            // /tmp happens to sit under a Rust project the score is still a
            // valid [0,1] and the call never panics.)
            let s = F2_5_DepCves.check(&toml).expect("check");
            assert!((0.0..=1.0).contains(&s.value));
        }

        #[test]
        fn returns_valid_score_and_no_panic() {
            let s = F2_5_DepCves
                .check(std::path::Path::new("Cargo.toml"))
                .expect("check");
            assert!((0.0..=1.0).contains(&s.value));
            assert!(!s.suggestions.is_empty());
        }

        // tp/fp validation of the vuln-vs-informational partition: a real CVE
        // (informational None) MUST block; an unmaintained/unsound-only tree
        // MUST pass (F4.5 territory) while still surfacing the advisory as a note.
        #[test]
        fn blocks_on_cve_passes_on_informational_only() {
            use super::super::real_engine;
            use touring_analysis::security::SecurityAdvisory;
            let mk = |id: &str, inf: Option<&str>| SecurityAdvisory {
                id: id.to_string(),
                package: "p".to_string(),
                version: "1.0.0".to_string(),
                title: "t".to_string(),
                severity: None,
                url: None,
                informational: inf.map(str::to_string),
            };

            // TP: a genuine vulnerability blocks (0.0) and is named a CVE.
            let (v, ev) = real_engine::score(&[mk("RUSTSEC-2026-0179", None)]);
            assert!((v - 0.0).abs() < 1e-6, "real CVE must block, got {v}: {ev}");
            assert!(ev.contains("CVE") && ev.contains("RUSTSEC-2026-0179"));

            // FP-control: unmaintained-only passes (1.0) — not a CVE — but is
            // still surfaced as a non-blocking informational note.
            let (vi, evi) = real_engine::score(&[mk("RUSTSEC-2024-0436", Some("unmaintained"))]);
            assert!(
                (vi - 1.0).abs() < 1e-6,
                "informational-only must pass, got {vi}: {evi}"
            );
            assert!(evi.contains("non-blocking") && evi.contains("unmaintained"));

            // Mixed: one real CVE among informational → still blocks, note retained.
            let (vm, evm) = real_engine::score(&[
                mk("RUSTSEC-2026-0179", None),
                mk("RUSTSEC-2024-0436", Some("unmaintained")),
            ]);
            assert!(
                (vm - 0.0).abs() < 1e-6,
                "mixed with a CVE must block, got {vm}"
            );
            assert!(evm.contains("1 dependency CVE") && evm.contains("non-blocking"));

            // Empty tree → pass.
            let (ve, _) = real_engine::score(&[]);
            assert!((ve - 1.0).abs() < 1e-6);
        }
    }

    // Fallback substring tests (standalone `--no-default-features`).
    #[cfg(not(feature = "workspace-integration"))]
    mod fallback_tests {
        use super::super::*;
        use std::io::Write;
        use tempfile::NamedTempFile;

        #[test]
        fn test_clean_cargo_toml_perfect() {
            let mut f = NamedTempFile::new().expect("create temp");
            writeln!(f, "[dependencies]\nserde = \"1.0.200\"").expect("write");
            let s = F2_5_DepCves.check(f.path()).expect("check");
            assert!(
                (s.value - 1.0).abs() < 1e-6,
                "clean deps should pass, got {}",
                s.value
            );
        }

        #[test]
        fn test_vulnerable_cargo_toml_fails() {
            let mut f = NamedTempFile::new().expect("create temp");
            writeln!(f, "[dependencies]\nserde = \"1.0.130\"").expect("write");
            let s = F2_5_DepCves.check(f.path()).expect("check");
            assert_eq!(s.status, crate::DimStatus::Fail);
            assert!(s.evidence.contains("serde@1.0.130"));
            assert!(!s.suggestions.is_empty());
        }
    }

    // ── W5 (2026-06-26): `load_advisories_ignore` walks up the lockfile's dir
    //    tree to the workspace-root `deny.toml`. These test the real-engine
    //    parser, so they are gated to `workspace-integration` (REGRA #21 fix
    //    2026-07-03: they previously lived in `fallback_tests`, which is
    //    `not(workspace-integration)`, and referenced the wsi-only `real_engine`
    //    module — so they never compiled in either config).
    #[cfg(feature = "workspace-integration")]
    mod advisories_ignore_tests {
        use super::super::real_engine;

        /// Build a temp directory tree:
        ///   `<root>/crates/foo/Cargo.lock`  (the lockfile the verifier sees)
        ///   `<root>/deny.toml`             (the workspace-root deny.toml)
        /// The verifier must walk up from `crates/foo/` to root to find it.
        fn write_walkup_fixture() -> tempfile::TempDir {
            let tmp = tempfile::tempdir().expect("tmpdir");
            let crates_dir = tmp.path().join("crates").join("foo");
            std::fs::create_dir_all(&crates_dir).expect("mkdir");
            // Stub Cargo.lock — contents don't matter for the walk-up test
            // (we only care that `load_advisories_ignore` finds the deny.toml).
            std::fs::write(crates_dir.join("Cargo.lock"), "# stub\n").expect("lock");
            tmp
        }

        #[test]
        fn load_advisories_ignore_walks_up_to_workspace_root() {
            let tmp = write_walkup_fixture();
            // deny.toml at workspace root with both `id = "..."` and `id="..."` forms.
            std::fs::write(
                tmp.path().join("deny.toml"),
                "\
[advisories]\n\
ignore = [\n\
    { id = \"RUSTSEC-2026-0185\", reason = \"qdrant-client pin\" },\n\
    { id=\"RUSTSEC-2024-0384\", reason = \"transitive only\" },\n\
    { id = \"RUSTSEC-0000-9999\", reason = \"unrelated\" },\n\
]\n",
            )
            .expect("deny");
            let lockfile = tmp.path().join("crates/foo/Cargo.lock");
            let ignored = real_engine::load_advisories_ignore(&lockfile);
            assert!(
                ignored.contains(&"RUSTSEC-2026-0185".to_string()),
                "space-separated form must be parsed"
            );
            assert!(
                ignored.contains(&"RUSTSEC-2024-0384".to_string()),
                "compact form (id=...) must be parsed"
            );
            assert!(
                ignored.contains(&"RUSTSEC-0000-9999".to_string()),
                "third id must be parsed"
            );
        }

        #[test]
        fn load_advisories_ignore_returns_empty_when_no_deny_toml() {
            // Workspace with NO deny.toml at all — verifier must degrade to
            // "no ignores" (i.e. score from raw RustSec findings) without
            // crashing.
            let tmp = write_walkup_fixture();
            let lockfile = tmp.path().join("crates/foo/Cargo.lock");
            let ignored = real_engine::load_advisories_ignore(&lockfile);
            assert!(ignored.is_empty(), "no deny.toml → no ignores");
        }

        #[test]
        fn load_advisories_ignore_skips_non_rustsec_keys() {
            // The workspace's `deny.toml` has many `id = "..."` entries that
            // aren't RUSTSEC (e.g. `skip` table entries). The scanner must
            // filter those out — only RUSTSEC-XXXX-YYYY entries count.
            let tmp = write_walkup_fixture();
            std::fs::write(
                tmp.path().join("deny.toml"),
                "\
[bans]\n\
skip = [\n\
    { crate = \"ahash@0.7.8\", reason = \"duplicate\" },\n\
    { crate = \"bitflags@1.3.2\", reason = \"duplicate\" },\n\
]\n\
[advisories]\n\
ignore = [\n\
    { id = \"RUSTSEC-2026-0185\", reason = \"qdrant-client pin\" },\n\
]\n",
            )
            .expect("deny");
            let lockfile = tmp.path().join("crates/foo/Cargo.lock");
            let ignored = real_engine::load_advisories_ignore(&lockfile);
            assert_eq!(ignored.len(), 1);
            assert_eq!(ignored[0], "RUSTSEC-2026-0185");
        }
    }
}
