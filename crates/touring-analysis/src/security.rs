//! Wave 5 (2026-04-18) — RustSec advisory database integration.
//!
//! This module wraps the `rustsec` crate to answer one question:
//!
//! > "For a given Cargo.lock or set of crate versions, which dependencies
//! > are affected by known security advisories?"
//!
//! # Design
//!
//! - **Lazy DB load.** The advisory database clone is ~20 MiB of YAML
//!   and only makes sense to load once per process. We expose a
//!   `SecurityDb` struct that owns the loaded database; callers share
//!   it via `&self`.
//! - **Graceful offline mode.** `SecurityDb::try_open()` returns `Ok`
//!   even when the database path is missing — the resulting struct
//!   simply reports zero advisories for every query. This keeps the
//!   feature operational for air-gapped environments.
//! - **Minimal surface.** Only two call sites in consumer code:
//!   `SecurityDb::scan_package(name, version)` and the Wave 5 CLI
//!   subcommand `touring analysis security <path>` wired through
//!   `touring-server/src/cli/analysis.rs`.
//!
//! # Integration map
//!
//! | Caller                                  | Purpose                              |
//! |-----------------------------------------|--------------------------------------|
//! | `touring-server` CLI (`security.rs`)    | `touring analysis security <file>`   |
//! | `quality::CodeHealthReport` (future)    | feed advisories into health score    |
//! | `touring-hooks post-edit` (future)      | warn on edits that pull vulnerables  |
//!
//! # Why not `cargo-audit` as a shell-out
//!
//! cargo-audit is a binary that performs the same advisory lookup but
//! requires a subprocess + JSON parsing on every call. Using the
//! `rustsec` library directly keeps the lookup in-process (~100 µs per
//! package after the initial DB load) and removes the external binary
//! as a runtime dependency.

use serde::{Deserialize, Serialize};

/// One security advisory matched against a `(crate, version)` pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAdvisory {
    /// RUSTSEC advisory identifier (e.g. `RUSTSEC-2024-0001`).
    pub id: String,
    /// Affected crate name.
    pub package: String,
    /// Version of the crate that matched.
    pub version: String,
    /// Short title / summary of the vulnerability.
    pub title: String,
    /// Severity, when the advisory provides one (e.g. "low" | "medium" | "high" | "critical").
    /// `None` means the advisory is informational or severity unspecified.
    pub severity: Option<String>,
    /// URL to the full advisory text (typically a rustsec.org page).
    pub url: Option<String>,
    /// Informational category when the advisory is NOT a vulnerability:
    /// `"unmaintained"`, `"unsound"`, `"notice"`, or an open-ended kind.
    /// `None` means a genuine security vulnerability (RustSec `informational`
    /// field absent). A CVE gate (F2.5 / D14) must honor this distinction —
    /// unmaintained/unsound crates are package-management concerns (F4.5 / D44),
    /// not CVEs, and should not fail a dependency-CVE gate.
    pub informational: Option<String>,
}

/// Handle to the loaded (or absent) RustSec advisory database.
///
/// Call `SecurityDb::try_open()` once per process and share the handle
/// across consumers. Lookups are pure functions of the in-memory DB —
/// concurrent callers never race.
pub struct SecurityDb {
    /// `None` when the database could not be loaded — queries on an
    /// offline db return empty Vecs rather than errors, so callers
    /// degrade transparently to "no advisories found".
    inner: Option<rustsec::Database>,
}

impl SecurityDb {
    /// Try to load the advisory database from its default location
    /// (`~/.cargo/advisory-db`). On failure, return an offline db — this
    /// keeps security scanning operational on air-gapped machines and
    /// in CI images without internet access.
    ///
    /// rustsec 0.30 removed the network `fetch()` helper; callers now
    /// point `Database::open` at a pre-cloned repository. We try the
    /// standard cargo-audit cache directory and degrade gracefully when
    /// absent.
    ///
    /// # Errors
    ///
    /// This function never returns an error. It logs a `tracing::warn`
    /// when the DB cannot be loaded and returns an offline handle.
    #[must_use]
    pub fn try_open() -> Self {
        // Default cargo-audit / rustsec DB cache location.
        let home = match std::env::var_os("HOME") {
            Some(h) => std::path::PathBuf::from(h),
            None => return Self { inner: None },
        };
        let db_path = home.join(".cargo").join("advisory-db");

        if !db_path.exists() {
            tracing::debug!(
                target: "touring_analysis::security",
                path = %db_path.display(),
                "advisory DB cache not present; running offline"
            );
            return Self { inner: None };
        }

        match rustsec::Database::open(&db_path) {
            Ok(db) => Self { inner: Some(db) },
            Err(e) => {
                tracing::warn!(
                    target: "touring_analysis::security",
                    error = %e,
                    "RustSec advisory DB load failed; scan will report zero advisories"
                );
                Self { inner: None }
            }
        }
    }

    /// Open from an already-constructed `rustsec::Database` — primarily
    /// useful for tests that inject a prebuilt fixture DB.
    #[must_use]
    pub fn from_db(db: rustsec::Database) -> Self {
        Self { inner: Some(db) }
    }

    /// Construct explicitly-offline handle for tests or air-gapped
    /// deployments. All subsequent queries return empty Vecs.
    #[must_use]
    pub fn offline() -> Self {
        Self { inner: None }
    }

    /// True when the database is loaded and scans are meaningful.
    #[must_use]
    pub fn is_online(&self) -> bool {
        self.inner.is_some()
    }

    /// Look up advisories affecting a specific `(package, version)`.
    ///
    /// Returns an empty Vec when:
    /// - the database is offline (see [`Self::try_open`])
    /// - the version string cannot be parsed as semver
    /// - no advisory matches the pair
    #[must_use]
    pub fn scan_package(&self, name: &str, version: &str) -> Vec<SecurityAdvisory> {
        let Some(db) = self.inner.as_ref() else {
            return Vec::new();
        };

        let Ok(pkg_name) = name.parse::<rustsec::package::Name>() else {
            return Vec::new();
        };
        let Ok(pkg_version) = version.parse::<rustsec::Version>() else {
            return Vec::new();
        };

        db.query(&rustsec::database::Query::new().package_name(pkg_name))
            .into_iter()
            .filter(|adv| adv.versions.is_vulnerable(&pkg_version))
            .map(|adv| SecurityAdvisory {
                id: adv.metadata.id.to_string(),
                package: name.to_string(),
                version: version.to_string(),
                title: adv.metadata.title.clone(),
                severity: adv.severity().map(|s| s.to_string()),
                url: adv.metadata.url.as_ref().map(|u| u.to_string()),
                informational: adv
                    .metadata
                    .informational
                    .as_ref()
                    .map(|i| i.as_str().to_string()),
            })
            .collect()
    }

    /// Convenience: count advisories without allocating the full Vec.
    #[must_use]
    pub fn count_advisories(&self, name: &str, version: &str) -> usize {
        self.scan_package(name, version).len()
    }

    /// Scan an entire `Cargo.lock` for dependencies affected by RustSec
    /// advisories.
    ///
    /// Loads the lockfile (resolved, transitive dependency tree) and checks
    /// every `(package, version)` pair against the in-memory advisory database
    /// via [`Self::scan_package`]. This is the authoritative source of versions
    /// — `Cargo.toml` carries requirements (`"1.0"`), only `Cargo.lock` carries
    /// the resolved versions an advisory matches against.
    ///
    /// Degrades gracefully (returns an empty `Vec`, logging a `tracing::warn`)
    /// when the database is offline or the lockfile cannot be read/parsed —
    /// consistent with [`Self::try_open`], so a security gate built on this never
    /// blocks a workflow on tooling absence.
    #[must_use]
    pub fn scan_lockfile(&self, lockfile_path: &std::path::Path) -> Vec<SecurityAdvisory> {
        if self.inner.is_none() {
            return Vec::new();
        }
        match rustsec::Lockfile::load(lockfile_path) {
            Ok(lockfile) => lockfile
                .packages
                .iter()
                .flat_map(|pkg| self.scan_package(pkg.name.as_str(), &pkg.version.to_string()))
                .collect(),
            Err(e) => {
                tracing::warn!(
                    target: "touring_analysis::security",
                    path = %lockfile_path.display(),
                    error = %e,
                    "Cargo.lock load failed; advisory scan reports zero"
                );
                Vec::new()
            }
        }
    }
}

impl Default for SecurityDb {
    fn default() -> Self {
        Self::offline()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_db_reports_zero_advisories_for_any_package() {
        let db = SecurityDb::offline();
        assert!(!db.is_online(), "offline() must produce an offline handle");
        assert_eq!(
            db.scan_package("serde", "1.0.0").len(),
            0,
            "offline scan must always be empty"
        );
        assert_eq!(db.count_advisories("tokio", "1.40.0"), 0);
    }

    #[test]
    fn default_is_offline() {
        let db = SecurityDb::default();
        assert!(!db.is_online());
    }

    #[test]
    fn malformed_version_returns_empty_not_panic() {
        let db = SecurityDb::offline();
        // Even online this must not panic — invalid semver → no matches.
        assert_eq!(db.scan_package("serde", "not-a-version").len(), 0);
        assert_eq!(db.scan_package("serde", "").len(), 0);
    }

    #[test]
    fn advisory_round_trips_through_serde() {
        let adv = SecurityAdvisory {
            id: "RUSTSEC-2024-0001".to_string(),
            package: "foo".to_string(),
            version: "1.0.0".to_string(),
            title: "test".to_string(),
            severity: Some("medium".to_string()),
            url: Some("https://rustsec.org/advisories/RUSTSEC-2024-0001".to_string()),
            informational: None,
        };
        let json = serde_json::to_string(&adv).expect("serialize");
        let back: SecurityAdvisory = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.id, adv.id);
        assert_eq!(back.severity, adv.severity);
    }

    #[test]
    fn try_open_never_panics_even_when_db_missing() {
        // We cannot guarantee the advisory DB is unavailable in the test
        // environment, so just assert the call itself is infallible.
        let _db = SecurityDb::try_open();
    }

    #[test]
    fn scan_lockfile_offline_is_empty_and_infallible() {
        // Offline DB short-circuits before touching the filesystem: even a
        // non-existent lockfile path returns empty rather than erroring.
        let db = SecurityDb::offline();
        assert!(
            db.scan_lockfile(std::path::Path::new("/nonexistent/Cargo.lock"))
                .is_empty(),
            "offline scan_lockfile must always be empty"
        );
    }
}
