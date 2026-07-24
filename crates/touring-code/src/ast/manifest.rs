//! Multi-ecosystem manifest inventory — the toolchain-parity analog of
//! `cargo_metadata` ([`crate::ast::wiring::WorkspaceInfo`]) for the non-Rust
//! ecosystems Touring indexes.
//!
//! `WorkspaceInfo` reads a Cargo workspace via `cargo metadata`. This module
//! gives the equivalent lightweight package / dependency view for:
//!
//! - **Node / npm** — `package.json`
//! - **Python / PyPI** — `pyproject.toml` (PEP 621 `[project]` or Poetry `[tool.poetry]`)
//! - **Go** — `go.mod`
//!
//! It powers `touring ast workspace-info` on non-Cargo projects and supplies the
//! ecosystem + dependency inventory the F2.5 CVE dimension needs to reach beyond
//! RustSec toward OSV.dev (whose ecosystem identifiers this module's
//! [`Ecosystem::as_osv`] deliberately matches).
//!
//! Parsing is best-effort and total: an unreadable / malformed manifest yields
//! `None` rather than an error, so a partial project never aborts a scan.

use std::path::Path;

use serde::Serialize;

/// A package ecosystem. The string forms match OSV.dev's canonical ecosystem
/// identifiers so a dependency inventory can feed an OSV batch query directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Ecosystem {
    /// Node / npm (`package.json`).
    Npm,
    /// Python / PyPI (`pyproject.toml`).
    PyPI,
    /// Go modules (`go.mod`).
    Go,
}

impl Ecosystem {
    /// The OSV.dev canonical ecosystem identifier (`"npm"`, `"PyPI"`, `"Go"`).
    #[must_use]
    pub fn as_osv(&self) -> &'static str {
        match self {
            Ecosystem::Npm => "npm",
            Ecosystem::PyPI => "PyPI",
            Ecosystem::Go => "Go",
        }
    }
}

/// One declared dependency: its name and the version requirement as written.
#[derive(Debug, Clone, Serialize)]
pub struct Dependency {
    /// Package name.
    pub name: String,
    /// The version requirement string as declared (`"^1.2.0"`, `">=2,<3"`,
    /// `"v1.4.0"`, …), or `None` when the manifest omits it.
    pub version_req: Option<String>,
}

/// A single package parsed from one manifest.
#[derive(Debug, Clone, Serialize)]
pub struct ManifestPackage {
    /// Package / module name.
    pub name: String,
    /// Declared version, when present (application manifests often omit it).
    pub version: Option<String>,
    /// Direct dependencies (across all dependency groups).
    pub dependencies: Vec<Dependency>,
}

/// One ecosystem manifest located on disk.
#[derive(Debug, Clone, Serialize)]
pub struct EcosystemManifest {
    /// Which ecosystem this manifest belongs to.
    pub ecosystem: Ecosystem,
    /// Absolute or given path to the manifest file.
    pub manifest_path: String,
    /// The package it declares.
    pub package: ManifestPackage,
}

/// The non-Cargo manifests discovered directly under a directory.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ManifestInventory {
    /// The directory scanned.
    pub root: String,
    /// Manifests found (npm / PyPI / Go), in a stable order.
    pub manifests: Vec<EcosystemManifest>,
}

impl ManifestInventory {
    /// Detect and parse the non-Cargo ecosystem manifests directly under `dir`
    /// (`package.json`, `pyproject.toml`, `go.mod`). Best-effort: malformed or
    /// absent manifests are skipped, never fatal.
    #[must_use]
    pub fn scan(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        let mut manifests = Vec::new();
        if let Some(m) = parse_package_json(&dir.join("package.json")) {
            manifests.push(m);
        }
        if let Some(m) = parse_pyproject(&dir.join("pyproject.toml")) {
            manifests.push(m);
        }
        if let Some(m) = parse_go_mod(&dir.join("go.mod")) {
            manifests.push(m);
        }
        Self {
            root: dir.display().to_string(),
            manifests,
        }
    }

    /// True when no non-Cargo manifest was found.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }

    /// Total dependency count across all discovered manifests.
    #[must_use]
    pub fn total_dependencies(&self) -> usize {
        self.manifests
            .iter()
            .map(|m| m.package.dependencies.len())
            .sum()
    }
}

// ── npm (package.json) ─────────────────────────────────────────────────────

fn parse_package_json(path: &Path) -> Option<EcosystemManifest> {
    let raw = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let name = json
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let version = json
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let mut dependencies = Vec::new();
    for group in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(obj) = json.get(group).and_then(|v| v.as_object()) {
            for (dep_name, req) in obj {
                dependencies.push(Dependency {
                    name: dep_name.clone(),
                    version_req: req.as_str().map(str::to_string),
                });
            }
        }
    }
    Some(EcosystemManifest {
        ecosystem: Ecosystem::Npm,
        manifest_path: path.display().to_string(),
        package: ManifestPackage {
            name,
            version,
            dependencies,
        },
    })
}

// ── PyPI (pyproject.toml — PEP 621 or Poetry) ──────────────────────────────

fn parse_pyproject(path: &Path) -> Option<EcosystemManifest> {
    let raw = std::fs::read_to_string(path).ok()?;
    let doc: toml::Value = toml::from_str(&raw).ok()?;

    // Prefer PEP 621 `[project]`; fall back to Poetry `[tool.poetry]`.
    let (name, version, dependencies) = doc
        .get("project")
        .map(parse_pep621)
        .filter(|(n, _, _)| !n.is_empty())
        .or_else(|| doc.get("tool").and_then(|t| t.get("poetry")).map(parse_poetry))
        .unwrap_or_default();

    if name.is_empty() && dependencies.is_empty() {
        return None; // not a recognizable pyproject (e.g. a build-only shim)
    }
    Some(EcosystemManifest {
        ecosystem: Ecosystem::PyPI,
        manifest_path: path.display().to_string(),
        package: ManifestPackage {
            name,
            version,
            dependencies,
        },
    })
}

/// PEP 621 `[project]`: `dependencies = ["requests>=2", ...]`.
fn parse_pep621(project: &toml::Value) -> (String, Option<String>, Vec<Dependency>) {
    let name = project
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let version = project
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let dependencies = project
        .get("dependencies")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|e| e.as_str()).map(parse_pep508).collect())
        .unwrap_or_default();
    (name, version, dependencies)
}

/// Poetry `[tool.poetry]`: name/version + a `[tool.poetry.dependencies]` table
/// (the `python` interpreter constraint is skipped — it is not a package).
fn parse_poetry(poetry: &toml::Value) -> (String, Option<String>, Vec<Dependency>) {
    let name = poetry
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let version = poetry
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let dependencies = poetry
        .get("dependencies")
        .and_then(|v| v.as_table())
        .map(|table| {
            table
                .iter()
                .filter(|(dep_name, _)| dep_name.as_str() != "python")
                .map(|(dep_name, req)| Dependency {
                    name: dep_name.clone(),
                    version_req: req.as_str().map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default();
    (name, version, dependencies)
}

/// Extract the package name (and requirement remainder) from a PEP 508 spec such
/// as `"requests>=2.0"`, `"flask[async]>=1"`, or `"pkg; python_version<'3.9'"`.
fn parse_pep508(spec: &str) -> Dependency {
    let spec = spec.trim();
    let name_end = spec
        .find(|c: char| " [<>=!~;(".contains(c))
        .unwrap_or(spec.len());
    let name = spec[..name_end].trim().to_string();
    let req = spec[name_end..].trim();
    Dependency {
        name,
        version_req: if req.is_empty() {
            None
        } else {
            Some(req.to_string())
        },
    }
}

// ── Go (go.mod) ─────────────────────────────────────────────────────────────

/// Extract the `module` import-path declared in a `go.mod` file's content.
///
/// Shared single source of truth for the two Go consumers of this line: the
/// dependency inventory ([`parse_go_mod`]) and the wiring producer-key
/// derivation ([`crate::ast::go_wiring::go_package_key_for_file`]). A Go
/// package's import-path is `<module>/<dir-relative-to-go.mod>`, so the wiring
/// feeder needs exactly this value to key producers as `go:<import-path>`.
pub(crate) fn go_module_path(content: &str) -> Option<String> {
    for raw_line in content.lines() {
        // Strip `//` line comments (mirrors `parse_go_mod`).
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if let Some(rest) = line.strip_prefix("module ") {
            let module = rest.trim();
            if !module.is_empty() {
                return Some(module.to_string());
            }
        }
    }
    None
}

fn parse_go_mod(path: &Path) -> Option<EcosystemManifest> {
    let raw = std::fs::read_to_string(path).ok()?;
    let module = go_module_path(&raw).unwrap_or_default();
    let mut dependencies = Vec::new();
    let mut in_require_block = false;

    for raw_line in raw.lines() {
        // Strip `//` line comments (covers `// indirect`).
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line == "require (" {
            in_require_block = true;
            continue;
        }
        if line == ")" {
            in_require_block = false;
            continue;
        }
        // The `module` line is parsed upfront via `go_module_path`.
        if let Some(rest) = line.strip_prefix("require ") {
            dependencies.extend(parse_go_require(rest.trim()));
            continue;
        }
        if in_require_block {
            dependencies.extend(parse_go_require(line));
        }
    }

    if module.is_empty() {
        return None;
    }
    Some(EcosystemManifest {
        ecosystem: Ecosystem::Go,
        manifest_path: path.display().to_string(),
        package: ManifestPackage {
            name: module,
            version: None,
            dependencies,
        },
    })
}

/// Parse one `go.mod` require line: `example.com/foo/bar v1.2.3`.
fn parse_go_require(line: &str) -> Option<Dependency> {
    let mut parts = line.split_whitespace();
    let name = parts.next()?.to_string();
    let version_req = parts.next().map(str::to_string);
    Some(Dependency { name, version_req })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &Path, name: &str, body: &str) {
        let mut f = std::fs::File::create(dir.join(name)).expect("create");
        f.write_all(body.as_bytes()).expect("write");
    }

    #[test]
    fn npm_package_json_extracts_name_version_and_all_dep_groups() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{
                "name": "web-app",
                "version": "1.4.0",
                "dependencies": { "react": "^18.2.0", "lodash": "4.17.21" },
                "devDependencies": { "typescript": "^5.0.0" }
            }"#,
        );
        let inv = ManifestInventory::scan(tmp.path());
        assert_eq!(inv.manifests.len(), 1);
        let m = &inv.manifests[0];
        assert_eq!(m.ecosystem, Ecosystem::Npm);
        assert_eq!(m.ecosystem.as_osv(), "npm");
        assert_eq!(m.package.name, "web-app");
        assert_eq!(m.package.version.as_deref(), Some("1.4.0"));
        assert_eq!(m.package.dependencies.len(), 3, "prod + dev deps");
        let react = m.package.dependencies.iter().find(|d| d.name == "react").unwrap();
        assert_eq!(react.version_req.as_deref(), Some("^18.2.0"));
    }

    #[test]
    fn pyproject_pep621_extracts_deps_and_names() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "pyproject.toml",
            r#"
[project]
name = "svc"
version = "0.2.0"
dependencies = ["requests>=2.28", "flask[async]>=2", "typing-extensions"]
"#,
        );
        let inv = ManifestInventory::scan(tmp.path());
        let m = &inv.manifests[0];
        assert_eq!(m.ecosystem, Ecosystem::PyPI);
        assert_eq!(m.ecosystem.as_osv(), "PyPI");
        assert_eq!(m.package.name, "svc");
        assert_eq!(m.package.dependencies.len(), 3);
        // PEP 508 name extraction: strip version + extras + markers.
        let names: Vec<&str> = m.package.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"requests"));
        assert!(names.contains(&"flask"), "extras `[async]` stripped from name");
        assert!(names.contains(&"typing-extensions"));
        let req = m.package.dependencies.iter().find(|d| d.name == "requests").unwrap();
        assert_eq!(req.version_req.as_deref(), Some(">=2.28"));
    }

    #[test]
    fn pyproject_poetry_extracts_deps_and_skips_python_constraint() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "pyproject.toml",
            r#"
[tool.poetry]
name = "poetry-svc"
version = "3.1.0"

[tool.poetry.dependencies]
python = "^3.11"
httpx = "^0.27"
pydantic = "2.5.0"
"#,
        );
        let inv = ManifestInventory::scan(tmp.path());
        let m = &inv.manifests[0];
        assert_eq!(m.package.name, "poetry-svc");
        let names: Vec<&str> = m.package.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"httpx"));
        assert!(names.contains(&"pydantic"));
        assert!(!names.contains(&"python"), "python interpreter constraint is not a package");
    }

    #[test]
    fn go_mod_extracts_module_and_require_block() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "go.mod",
            "module example.com/myapp\n\ngo 1.21\n\nrequire (\n\tgithub.com/gin-gonic/gin v1.9.1\n\tgolang.org/x/text v0.14.0 // indirect\n)\n\nrequire github.com/stretchr/testify v1.8.4\n",
        );
        let inv = ManifestInventory::scan(tmp.path());
        let m = &inv.manifests[0];
        assert_eq!(m.ecosystem, Ecosystem::Go);
        assert_eq!(m.ecosystem.as_osv(), "Go");
        assert_eq!(m.package.name, "example.com/myapp");
        let names: Vec<&str> = m.package.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"github.com/gin-gonic/gin"), "block require");
        assert!(names.contains(&"golang.org/x/text"), "// indirect stripped, dep kept");
        assert!(names.contains(&"github.com/stretchr/testify"), "single-line require");
        let gin = m.package.dependencies.iter().find(|d| d.name.contains("gin")).unwrap();
        assert_eq!(gin.version_req.as_deref(), Some("v1.9.1"));
    }

    #[test]
    fn scan_of_empty_dir_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let inv = ManifestInventory::scan(tmp.path());
        assert!(inv.is_empty());
        assert_eq!(inv.total_dependencies(), 0);
    }

    #[test]
    fn polyglot_dir_reports_all_three_ecosystems() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "package.json", r#"{"name":"a","dependencies":{"x":"1"}}"#);
        write(tmp.path(), "pyproject.toml", "[project]\nname=\"b\"\ndependencies=[\"y\"]\n");
        write(tmp.path(), "go.mod", "module c\nrequire z v1.0.0\n");
        let inv = ManifestInventory::scan(tmp.path());
        assert_eq!(inv.manifests.len(), 3);
        assert_eq!(inv.total_dependencies(), 3);
    }
}
