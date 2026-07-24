//! OSV.dev multi-ecosystem vulnerability query builder.
//!
//! [`crate::security::SecurityDb`] scans a `Cargo.lock` against the RustSec
//! advisory database (Rust-only). [OSV.dev](https://osv.dev) is the
//! multi-ecosystem equivalent — npm, PyPI, Go, and more — queried via its batch
//! API. This module builds the batch-query **payload** from a
//! [`ManifestInventory`] (the offline, deterministic half of a scan) so the F2.5
//! CVE dimension can reach beyond RustSec toward the other ecosystems Touring
//! indexes.
//!
//! **Network policy**: the live HTTP lookup is intentionally *not* performed
//! here — it is opt-in / network-gated (the Code Execution Gateway denies
//! network by default). The offline half (ecosystem + dependency inventory →
//! request payload) is pure and deterministic; [`offline_summary`] reports what
//! a scan *would* cover without touching the network.

use serde::Serialize;
use touring_code::ast::manifest::ManifestInventory;

/// A package coordinate in an OSV query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OsvPackage {
    /// Package name.
    pub name: String,
    /// OSV canonical ecosystem identifier (`"npm"`, `"PyPI"`, `"Go"`).
    pub ecosystem: String,
}

/// One entry of an OSV `querybatch` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OsvQuery {
    /// The package to check.
    pub package: OsvPackage,
    /// A concrete version to check, when the manifest pins one; omitted
    /// otherwise (OSV then returns all advisories for the package).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// The OSV.dev `POST /v1/querybatch` request body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct OsvBatchQuery {
    /// One query per dependency.
    pub queries: Vec<OsvQuery>,
}

impl OsvBatchQuery {
    /// The OSV.dev batch endpoint (used by an opt-in online client).
    pub const ENDPOINT: &'static str = "https://api.osv.dev/v1/querybatch";

    /// Build a batch query from every dependency across a manifest inventory,
    /// tagging each with its ecosystem's OSV identifier.
    #[must_use]
    pub fn from_inventory(inventory: &ManifestInventory) -> Self {
        let mut queries = Vec::new();
        for manifest in &inventory.manifests {
            let ecosystem = manifest.ecosystem.as_osv().to_string();
            for dep in &manifest.package.dependencies {
                queries.push(OsvQuery {
                    package: OsvPackage {
                        name: dep.name.clone(),
                        ecosystem: ecosystem.clone(),
                    },
                    version: normalize_version(dep.version_req.as_deref()),
                });
            }
        }
        Self { queries }
    }

    /// Number of package queries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queries.len()
    }

    /// True when there are no queries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queries.is_empty()
    }

    /// Serialize to the OSV.dev querybatch JSON body.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"queries\":[]}".to_string())
    }
}

/// Reduce a version requirement to a concrete version OSV can match, when the
/// requirement is an exact pin; otherwise `None` (ranges/carets/tildes cannot be
/// mapped to a single version, so the caller queries by name and range-filters).
fn normalize_version(req: Option<&str>) -> Option<String> {
    let req = req?.trim();
    let pinned = req
        .strip_prefix("==")
        .or_else(|| req.strip_prefix('='))
        .map(str::trim)
        .unwrap_or(req);
    let core = pinned.strip_prefix('v').unwrap_or(pinned);
    let is_concrete = core.chars().next().is_some_and(|c| c.is_ascii_digit())
        && !pinned.contains(|c| "<>^~*, |".contains(c));
    is_concrete.then(|| pinned.to_string())
}

/// Offline summary of OSV-scan coverage for a directory, without any network
/// call. Detects npm/PyPI/Go manifests under `dir` and reports the ecosystems +
/// dependency count + how many OSV queries a live scan would issue. Returns
/// `None` when no non-Cargo manifest is present.
///
/// Used by F2.5 to enrich its non-Cargo evidence: the dimension now *sees* the
/// dependency tree it cannot yet scan offline, rather than reporting a bare
/// "not applicable".
#[must_use]
pub fn offline_summary(dir: &std::path::Path) -> Option<String> {
    let inventory = ManifestInventory::scan(dir);
    if inventory.is_empty() {
        return None;
    }
    let batch = OsvBatchQuery::from_inventory(&inventory);
    let mut ecosystems: Vec<&str> = inventory
        .manifests
        .iter()
        .map(|m| m.ecosystem.as_osv())
        .collect();
    ecosystems.sort_unstable();
    ecosystems.dedup();
    Some(format!(
        "OSV.dev coverage: {} manifest(s) [{}], {} dependencies → {} queries ready. \
         Live multi-ecosystem lookup is opt-in/network (set TOURING_OSV_SCAN=1; \
         POST {})",
        inventory.manifests.len(),
        ecosystems.join(", "),
        inventory.total_dependencies(),
        batch.len(),
        OsvBatchQuery::ENDPOINT,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;

    fn write(dir: &Path, name: &str, body: &str) {
        let mut f = std::fs::File::create(dir.join(name)).expect("create");
        f.write_all(body.as_bytes()).expect("write");
    }

    #[test]
    fn batch_query_tags_each_dep_with_osv_ecosystem() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"name":"a","dependencies":{"react":"18.2.0","lodash":"^4"}}"#,
        );
        let inv = ManifestInventory::scan(tmp.path());
        let batch = OsvBatchQuery::from_inventory(&inv);
        assert_eq!(batch.len(), 2);
        assert!(batch.queries.iter().all(|q| q.package.ecosystem == "npm"));
        // exact pin → concrete version; caret range → None.
        let react = batch.queries.iter().find(|q| q.package.name == "react").unwrap();
        assert_eq!(react.version.as_deref(), Some("18.2.0"));
        let lodash = batch.queries.iter().find(|q| q.package.name == "lodash").unwrap();
        assert_eq!(lodash.version, None, "caret range is not a concrete pin");
    }

    #[test]
    fn to_json_omits_absent_version_and_matches_osv_shape() {
        let batch = OsvBatchQuery {
            queries: vec![
                OsvQuery {
                    package: OsvPackage { name: "flask".into(), ecosystem: "PyPI".into() },
                    version: Some("2.3.0".into()),
                },
                OsvQuery {
                    package: OsvPackage { name: "click".into(), ecosystem: "PyPI".into() },
                    version: None,
                },
            ],
        };
        let json = batch.to_json();
        assert!(json.contains("\"ecosystem\":\"PyPI\""));
        assert!(json.contains("\"version\":\"2.3.0\""));
        // the version-less query must NOT emit a null version field.
        assert!(!json.contains("\"version\":null"));
    }

    #[test]
    fn normalize_version_accepts_pins_rejects_ranges() {
        assert_eq!(normalize_version(Some("1.2.3")), Some("1.2.3".into()));
        assert_eq!(normalize_version(Some("==2.0.0")), Some("2.0.0".into()));
        assert_eq!(normalize_version(Some("v1.9.1")), Some("v1.9.1".into())); // Go
        assert_eq!(normalize_version(Some("^1.0")), None);
        assert_eq!(normalize_version(Some(">=2,<3")), None);
        assert_eq!(normalize_version(Some("~1.4")), None);
        assert_eq!(normalize_version(None), None);
    }

    #[test]
    fn offline_summary_reports_ecosystems_and_query_count() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "package.json", r#"{"name":"a","dependencies":{"x":"1"}}"#);
        write(tmp.path(), "go.mod", "module c\nrequire z v1.0.0\n");
        let summary = offline_summary(tmp.path()).expect("has manifests");
        assert!(summary.contains("Go"));
        assert!(summary.contains("npm"));
        assert!(summary.contains("2 dependencies"));
        assert!(summary.contains("opt-in"));
    }

    #[test]
    fn offline_summary_is_none_without_manifests() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(offline_summary(tmp.path()).is_none());
    }
}
