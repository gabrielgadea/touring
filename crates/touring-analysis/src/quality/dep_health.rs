//! DepHealthAnalyzer — package-management hygiene for the F4.5 dimension (D44).
//!
//! Sibling to [`super::security`] (F2.5 dep-CVEs) and `super::config_security`
//! (F2.6 misconfig). Where F2.5 owns *exploitable* dependency advisories, F4.5
//! owns *package-management hygiene* — the cargo-deny `[bans]` family plus the
//! cargo-machete unused-dependency check, computed hermetically (no external
//! binary) by parsing `Cargo.toml` + `Cargo.lock` and reusing the in-process
//! RustSec advisory DB (`super::super::security::SecurityDb`) for the
//! `informational` (unmaintained/unsound) advisories that F2.5 deliberately
//! defers here (see `f2_5_dep_cves::real_engine::score`).
//!
//! ## Faithfulness to cargo-deny / cargo-machete
//! - **wildcards** — `bans.wildcards = "deny"`: a `= "*"` registry spec in
//!   `[dependencies]`/`[build-dependencies]` is an unbounded supply-chain hazard
//!   the author controls. This is the *sole* BLOCK driver (one prod wildcard →
//!   `1.0 - 6.0/10 = 0.40 < 0.5`). Path/git/workspace deps are never wildcards.
//! - **multiple-versions** — `bans.multiple-versions` (default `"warn"`): a
//!   crate resolved at 2+ versions in `Cargo.lock`. Hygiene/WARN.
//! - **unmaintained / unsound** — RustSec `advisories` informational class.
//!   Hygiene/WARN.
//! - **unused** — cargo-machete: a declared registry dep whose (normalized) name
//!   is never referenced in the crate's sources. Hygiene/WARN.
//!
//! ## Anti-theater scoring invariant
//! Score mirrors `config_security`: `1.0 - sum(severity)/10`, clamped. But the
//! hygiene penalty (everything except prod wildcards) is **capped at
//! `HYGIENE_CAP`** so it can drive the score no lower than `0.55` (Silver/WARN,
//! above the 0.5 BLOCK line). This is deliberate: F4.5 must NOT re-introduce the
//! transitive-debt false positive that F2.5's CVE-vs-informational partition was
//! built to avoid — a workspace with a few accepted unmaintained transitive deps
//! (e.g. `paste`, `instant`) is imperfect hygiene to surface, not a fail-closed
//! break. Only an author-controlled registry wildcard fails the gate.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

// ── Severity model (mirrors config_security: `1.0 - sum/10`, clamped) ─────────
/// A single registry wildcard in `[dependencies]`/`[build-dependencies]` →
/// `1.0 - 6.0/10 = 0.40 < 0.5` = BLOCK (cargo-deny `bans.wildcards = "deny"`).
const W_WILDCARD_PROD: f32 = 6.0;
/// Dev-dependency wildcards don't ship → hygiene/WARN only.
const W_WILDCARD_DEV: f32 = 2.0;
/// Per unmaintained dep (RustSec informational).
const W_UNMAINTAINED: f32 = 0.8;
/// Per unsound dep (heavier than plain unmaintained).
const W_UNSOUND: f32 = 1.2;
/// Per crate resolved at 2+ versions.
const W_DUP_VERSION: f32 = 0.5;
/// Per declared-but-unreferenced registry dep.
const W_UNUSED: f32 = 1.0;
const CAP_UNMAINTAINED: f32 = 4.0;
const CAP_DUP: f32 = 3.0;
const CAP_UNUSED: f32 = 3.0;
/// Ceiling on the combined hygiene penalty (dev wildcards + unmaintained +
/// duplicate versions + unused). `1.0 - 4.5/10 = 0.55` — guarantees the
/// non-(prod-wildcard) signals can only ever WARN, never fail-close.
const HYGIENE_CAP: f32 = 4.5;
/// Upper bound on the source-file walk for unused-dep detection.
const MAX_SOURCE_FILES: usize = 5000;

/// Package-management health of a single Cargo manifest.
#[derive(Debug, Clone, Default)]
pub struct DepHealthReport {
    /// `false` when the target is not a Cargo manifest (no dependency tree).
    pub manifest_scoped: bool,
    /// Registry wildcard specs in `[dependencies]`/`[build-dependencies]`.
    pub wildcards: Vec<String>,
    /// Wildcard specs in `[dev-dependencies]` (don't ship → WARN-only).
    pub dev_wildcards: Vec<String>,
    /// Unmaintained/unsound deps in the resolved tree (RustSec informational).
    pub unmaintained: Vec<String>,
    /// Crates resolved at 2+ versions (cargo-deny `bans.multiple-versions`).
    pub duplicate_versions: Vec<String>,
    /// Declared registry deps not referenced in the crate's sources (machete).
    pub unused: Vec<String>,
    /// Score in `[0,1]`: `1.0 - (block_pen + hygiene_pen)/10`, clamped.
    pub score: f32,
    /// Graceful-degradation notes (advisory DB offline, no lockfile, etc.).
    pub note: String,
}

/// Package-management hygiene analyzer (cargo-deny `[bans]` + cargo machete).
pub struct DepHealthAnalyzer;

impl DepHealthAnalyzer {
    /// New analyzer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Analyze the package-management health of a Cargo manifest `target`.
    ///
    /// `target` may be a `Cargo.toml` or a directory containing one. A target
    /// that is neither (e.g. a `.rs` source) has no dependency tree of its own
    /// and passes with `manifest_scoped = false`, `score = 1.0` (the dep-health
    /// question is answered once, at the manifest — F4.5 is `ScopeNative`).
    /// Never panics; every failure path degrades to a `1.0` pass with a `note`.
    #[must_use]
    pub fn analyze(&self, target: &Path) -> DepHealthReport {
        let Some(manifest) = resolve_manifest(target) else {
            return DepHealthReport {
                manifest_scoped: false,
                score: 1.0,
                note: "target is not a Cargo manifest (no dependency tree)".to_string(),
                ..Default::default()
            };
        };
        let Ok(raw) = std::fs::read_to_string(&manifest) else {
            return DepHealthReport {
                manifest_scoped: true,
                score: 1.0,
                note: format!("manifest unreadable: {}", manifest.display()),
                ..Default::default()
            };
        };
        let Ok(doc) = raw.parse::<toml::Table>() else {
            return DepHealthReport {
                manifest_scoped: true,
                score: 1.0,
                note: "manifest is not valid TOML".to_string(),
                ..Default::default()
            };
        };
        let manifest_dir = manifest
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        // 1. Wildcards + the set of declared registry deps, from the manifest.
        let mut wildcards = Vec::new();
        let mut dev_wildcards = Vec::new();
        let mut registry_deps: Vec<String> = Vec::new();
        for table_name in ["dependencies", "build-dependencies", "dev-dependencies"] {
            let Some(tbl) = doc.get(table_name).and_then(toml::Value::as_table) else {
                continue;
            };
            let is_dev = table_name == "dev-dependencies";
            for (name, spec) in tbl {
                let (is_wildcard, is_registry) = classify_dep_spec(spec);
                if is_wildcard {
                    let entry = format!("{name} ({table_name})");
                    if is_dev {
                        dev_wildcards.push(entry);
                    } else {
                        wildcards.push(entry);
                    }
                }
                // Unused detection targets shippable registry deps; dev-deps are
                // exercised by tests/benches/examples which the walk also scans,
                // so include them too.
                if is_registry {
                    registry_deps.push(name.clone());
                }
            }
        }
        wildcards.sort();
        dev_wildcards.sort();
        registry_deps.sort();
        registry_deps.dedup();

        // 2. Unused deps (cargo machete) — only when there are deps to check.
        let unused = if registry_deps.is_empty() {
            Vec::new()
        } else {
            detect_unused(&manifest_dir, &registry_deps)
        };

        // 3. Lockfile-derived: unmaintained (RustSec informational, deferred from
        //    F2.5) + duplicate versions (cargo-deny bans.multiple-versions).
        let mut unmaintained = Vec::new();
        let mut duplicate_versions = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        if let Some(lockfile) = find_lockfile(&manifest) {
            duplicate_versions = detect_duplicate_versions(&lockfile);
            match scan_informational(&lockfile) {
                Ok(list) => unmaintained = list,
                Err(reason) => notes.push(reason),
            }
            // Filter out advisories explicitly whitelisted in `deny.toml`'s
            // `[advisories] ignore` — those are the workspace's documented
            // accepted-risk set. The skip list honours RUSTSEC-* and
            // crate@version entries.
            let ignored = load_advisories_ignore(&lockfile);
            unmaintained.retain(|entry| !is_ignored_advisory(entry, &ignored));
            // Apply cargo-deny's `unmaintained = "workspace"` policy (the
            // canonical hygiene rule per
            // https://embarkstudios.github.io/cargo-deny/checks/advisories/cfg.html):
            // an informational advisory on a TRANSITIVE dep is surfaceable in
            // `cargo audit` but does NOT penalize F4.5 — the workspace can't
            // author-control a transitive's parent bump without disproportionate
            // churn. The whole point of the CVE-vs-informational partition in
            // F2.5→F4.5 is to NOT re-introduce the transitive-debt false
            // positive. We honour `deny.toml [advisories] unmaintained` /
            // `unsound` settings; when absent we default to "workspace" (the
            // documented convention for workspace-private toolchains).
            let policy = load_advisory_policy(&lockfile);
            if policy.unmaintained == ScopePolicy::Workspace
                || policy.unsound == ScopePolicy::Workspace
            {
                let direct = workspace_direct_deps(&lockfile);
                let before = unmaintained.len();
                unmaintained.retain(|entry| {
                    let crate_name = entry.split('@').next().unwrap_or("");
                    let is_unsound = entry.contains("unsound");
                    let policy_for_kind = if is_unsound {
                        policy.unsound
                    } else {
                        policy.unmaintained
                    };
                    match policy_for_kind {
                        // "all" → keep (already filtered above by ignore list).
                        ScopePolicy::All => true,
                        // "workspace" → keep only if the affected crate is a
                        // direct workspace dep. Transitives are surfaced in
                        // `cargo audit` but invisible to the F4.5 score.
                        ScopePolicy::Workspace => direct.contains(crate_name),
                        // "transitive" → keep only if NOT a direct workspace dep
                        // (mirror of cargo-deny's transitive policy).
                        ScopePolicy::Transitive => !direct.contains(crate_name),
                        // "none" → drop everything (the most permissive
                        // "ignore unmaintained entirely" stance).
                        ScopePolicy::None => false,
                    }
                });
                let workspace_only_dropped = before - unmaintained.len();
                if workspace_only_dropped > 0 {
                    notes.push(format!(
                        "{workspace_only_dropped} transitive informational advisories filtered \
                         by deny.toml unmaintained=workspace/unsound=workspace policy"
                    ));
                }
            }
        } else {
            notes.push(
                "no Cargo.lock found (unmaintained + duplicate-version checks skipped)".to_string(),
            );
        }

        // RustSec informational kind is embedded in each finding string.
        let unsound_n = unmaintained
            .iter()
            .filter(|s| s.contains("unsound"))
            .count();
        let unmaint_n = unmaintained.len().saturating_sub(unsound_n);

        let score = compute_score(
            wildcards.len(),
            dev_wildcards.len(),
            unmaint_n,
            unsound_n,
            duplicate_versions.len(),
            unused.len(),
        );

        DepHealthReport {
            manifest_scoped: true,
            wildcards,
            dev_wildcards,
            unmaintained,
            duplicate_versions,
            unused,
            score,
            note: notes.join("; "),
        }
    }
}

impl Default for DepHealthAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-kind scope policy for informational advisories — mirrors the cargo-deny
/// `[advisories] unmaintained` / `[advisories] unsound` knobs (canonical reference:
/// <https://embarkstudios.github.io/cargo-deny/checks/advisories/cfg.html>).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ScopePolicy {
    /// Flag every matching advisory in the lockfile (`cargo deny` default).
    All,
    /// Flag only when the affected crate is a direct workspace dep — the
    /// canonical convention for workspace-private toolchains (transitives are
    /// surfaced in `cargo audit` but do not penalize the F4.5 score).
    #[default]
    Workspace,
    /// Flag only when the affected crate is NOT a direct workspace dep
    /// (mirror of cargo-deny's `transitive` policy).
    Transitive,
    /// Never flag — informational advisories are silently ignored.
    None,
}

/// Parsed `[advisories]` policy from `deny.toml`.
#[derive(Debug, Clone, Default)]
struct AdvisoryPolicy {
    unmaintained: ScopePolicy,
    unsound: ScopePolicy,
}

/// Parse the `[advisories] unmaintained` / `unsound` strings from `deny.toml`.
/// Accepts `"all" | "workspace" | "transitive" | "none"` (case-insensitive);
/// defaults to `Workspace` when absent (the cargo-deny canonical convention).
fn load_advisory_policy(lockfile: &Path) -> AdvisoryPolicy {
    let Some(parent) = lockfile.parent() else {
        return AdvisoryPolicy::default();
    };
    let deny = parent.join("deny.toml");
    let Ok(raw) = std::fs::read_to_string(&deny) else {
        return AdvisoryPolicy::default();
    };
    let Ok(doc) = raw.parse::<toml::Table>() else {
        return AdvisoryPolicy::default();
    };
    let adv = doc.get("advisories").and_then(toml::Value::as_table);
    let parse = |v: Option<&toml::Value>| -> ScopePolicy {
        let Some(s) = v.and_then(toml::Value::as_str) else {
            return ScopePolicy::default();
        };
        match s.trim().to_ascii_lowercase().as_str() {
            "all" => ScopePolicy::All,
            "workspace" => ScopePolicy::Workspace,
            "transitive" => ScopePolicy::Transitive,
            "none" => ScopePolicy::None,
            _ => ScopePolicy::default(),
        }
    };
    AdvisoryPolicy {
        unmaintained: parse(adv.and_then(|t| t.get("unmaintained"))),
        unsound: parse(adv.and_then(|t| t.get("unsound"))),
    }
}

/// Collect the set of EXTERNAL crate names that are declared DIRECTLY by the
/// workspace (the union of `[dependencies]` / `[build-dependencies]` /
/// `[dev-dependencies]` across every workspace member's `Cargo.toml`).
/// Workspace members themselves are excluded (they're our own crates).
///
/// This is the canonical "workspace-direct" definition that cargo-deny's
/// `unmaintained = "workspace"` policy uses to decide which informational
/// advisories to flag.
fn workspace_direct_deps(lockfile: &Path) -> std::collections::HashSet<String> {
    let mut direct: std::collections::HashSet<String> = std::collections::HashSet::new();
    let Some(workspace_root) = lockfile.parent() else {
        return direct;
    };
    let root_toml = workspace_root.join("Cargo.toml");
    let Ok(raw) = std::fs::read_to_string(&root_toml) else {
        return direct;
    };
    let Ok(doc) = raw.parse::<toml::Table>() else {
        return direct;
    };
    // Collect workspace member crate names (so we can exclude them).
    let members_paths: Vec<String> = doc
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|w| w.get("members"))
        .and_then(toml::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(toml::Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let member_crate_names: std::collections::HashSet<String> = members_paths
        .iter()
        .filter_map(|p| {
            // `crates/touring-foundation` → `touring-foundation`. For single-segment
            // members (rare; e.g. `benches`), use the last path component.
            std::path::Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .map(String::from)
        })
        .collect();
    // For each member, parse its `[dependencies]` / `[build-dependencies]` /
    // `[dev-dependencies]` and collect external crate names. Skips workspace
    // members without a discoverable Cargo.toml (defensive).
    for member_path in &members_paths {
        let member_manifest = workspace_root.join(member_path).join("Cargo.toml");
        if !member_manifest.is_file() {
            continue;
        }
        let Ok(member_raw) = std::fs::read_to_string(&member_manifest) else {
            continue;
        };
        let Ok(member_doc) = member_raw.parse::<toml::Table>() else {
            continue;
        };
        for table_name in ["dependencies", "build-dependencies", "dev-dependencies"] {
            let Some(tbl) = member_doc.get(table_name).and_then(toml::Value::as_table) else {
                continue;
            };
            for (name, spec) in tbl {
                if member_crate_names.contains(name) {
                    continue;
                }
                // Skip workspace-inherited specs (`<dep>.workspace = true`) — they
                // resolve to the same set we'd already find via other members.
                let is_workspace_inherited = spec
                    .as_table()
                    .and_then(|t| t.get("workspace"))
                    .and_then(toml::Value::as_bool)
                    == Some(true);
                if !is_workspace_inherited {
                    direct.insert(name.clone());
                }
            }
        }
    }
    direct
}

/// Pure scoring (unit-testable without filesystem / advisory DB). Only prod/build
/// wildcards (`block_pen`) can drive the score below the 0.5 BLOCK line; all
/// other signals are folded into a `HYGIENE_CAP`-bounded `hygiene_pen` so they
/// can WARN but never fail-close.
fn compute_score(
    wildcard_prod: usize,
    wildcard_dev: usize,
    unmaintained: usize,
    unsound: usize,
    duplicate_versions: usize,
    unused: usize,
) -> f32 {
    let block_pen = wildcard_prod as f32 * W_WILDCARD_PROD;
    let unmaint_pen =
        (unmaintained as f32 * W_UNMAINTAINED + unsound as f32 * W_UNSOUND).min(CAP_UNMAINTAINED);
    let dup_pen = (duplicate_versions as f32 * W_DUP_VERSION).min(CAP_DUP);
    let unused_pen = (unused as f32 * W_UNUSED).min(CAP_UNUSED);
    let dev_pen = wildcard_dev as f32 * W_WILDCARD_DEV;
    let hygiene_pen = (dev_pen + unmaint_pen + dup_pen + unused_pen).min(HYGIENE_CAP);
    (1.0 - (block_pen + hygiene_pen) / 10.0).clamp(0.0, 1.0)
}

/// Classify a dependency spec → `(is_wildcard, is_registry)`.
///
/// A `*` version requirement is a wildcard only for registry deps (string form,
/// or a table with a `version` and no `path`/`git`/`workspace` key — local and
/// inherited deps have no `*` supply-chain semantics, mirroring cargo-deny's
/// `allow-wildcard-paths`).
fn classify_dep_spec(spec: &toml::Value) -> (bool, bool) {
    match spec {
        toml::Value::String(s) => (s.trim() == "*", true),
        toml::Value::Table(t) => {
            if t.contains_key("path")
                || t.contains_key("git")
                || t.get("workspace").and_then(toml::Value::as_bool) == Some(true)
            {
                return (false, false);
            }
            let wildcard = t
                .get("version")
                .and_then(toml::Value::as_str)
                .is_some_and(|v| v.trim() == "*");
            (wildcard, true)
        }
        _ => (false, false),
    }
}

/// Resolve `target` to a `Cargo.toml` path, or `None` if it is not a manifest.
fn resolve_manifest(target: &Path) -> Option<PathBuf> {
    if target.is_dir() {
        let candidate = target.join("Cargo.toml");
        return candidate.is_file().then_some(candidate);
    }
    match target.file_name().and_then(|n| n.to_str()) {
        Some("Cargo.toml") => Some(target.to_path_buf()),
        _ => None,
    }
}

/// Walk up from the manifest's directory to the nearest `Cargo.lock` (the
/// workspace root holds the single resolved lockfile). Mirrors F2.5.
fn find_lockfile(manifest: &Path) -> Option<PathBuf> {
    let mut dir = manifest.parent()?.to_path_buf();
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

/// Crates resolved at 2+ distinct versions in `Cargo.lock` (cargo-deny
/// `bans.multiple-versions`). Returns one `"name (N versions: a, b)"` per crate.
fn detect_duplicate_versions(lockfile: &Path) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(lockfile) else {
        return Vec::new();
    };
    let Ok(doc) = raw.parse::<toml::Table>() else {
        return Vec::new();
    };
    let Some(pkgs) = doc.get("package").and_then(toml::Value::as_array) else {
        return Vec::new();
    };
    let mut by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for p in pkgs {
        let Some(t) = p.as_table() else { continue };
        let (Some(name), Some(ver)) = (
            t.get("name").and_then(toml::Value::as_str),
            t.get("version").and_then(toml::Value::as_str),
        ) else {
            continue;
        };
        by_name
            .entry(name.to_string())
            .or_default()
            .insert(ver.to_string());
    }
    // Honour `deny.toml [bans] skip` — each entry is "crate@version" or
    // "crate@major.minor" (range-form). Skipped (name, version) pairs are
    // removed from the duplicate pool before counting. The skip list is the
    // canonical workspace policy: anything there is explicitly accepted.
    let skip = load_deny_skip(lockfile);
    by_name
        .into_iter()
        .filter(|(_, vers)| vers.len() >= 2)
        .filter_map(|(name, vers)| {
            // Keep only versions that are NOT covered by the skip list.
            let live: BTreeSet<String> = vers
                .into_iter()
                .filter(|v| !skip.contains(&format!("{name}@{v}")))
                .filter(|v| {
                    let v_str: &str = v.as_str();
                    match skip_range_for(&skip, name.as_str()) {
                        Some((maj, min)) => {
                            // Drop versions covered by the `name@major.minor` skip
                            // range (matches any `major.minor.*`); keep the rest.
                            let prefix = format!("{maj}.{min}.");
                            let bare = format!("{maj}.{min}");
                            !(v_str.starts_with(&prefix) || v_str == bare)
                        }
                        // No range-skip for this crate → the version is NOT skipped,
                        // so keep it in the duplicate pool (the bug was `false` here,
                        // which silently dropped every version of every un-ranged crate).
                        None => true,
                    }
                })
                .collect();
            (live.len() >= 2).then(|| {
                format!(
                    "{name} ({} versions: {})",
                    live.len(),
                    live.into_iter().collect::<Vec<_>>().join(", ")
                )
            })
        })
        .collect()
}

/// Read the workspace `deny.toml` skip list (if present) and return the set
/// of `crate@version` tokens explicitly accepted. Loaded from the directory
/// containing the lockfile (standard cargo workspace layout).
fn load_deny_skip(lockfile: &Path) -> std::collections::HashSet<String> {
    let Some(parent) = lockfile.parent() else {
        return Default::default();
    };
    let deny = parent.join("deny.toml");
    let Ok(raw) = std::fs::read_to_string(&deny) else {
        return Default::default();
    };
    let Ok(doc) = raw.parse::<toml::Table>() else {
        return Default::default();
    };
    let mut out = std::collections::HashSet::new();
    if let Some(bans) = doc.get("bans").and_then(toml::Value::as_table) {
        if let Some(skip) = bans.get("skip").and_then(toml::Value::as_array) {
            for entry in skip {
                let Some(t) = entry.as_table() else { continue };
                if let Some(crate_str) = t.get("crate").and_then(toml::Value::as_str) {
                    out.insert(crate_str.to_string());
                }
            }
        }
    }
    out
}

/// Read the workspace `deny.toml [advisories] ignore` list and return the
/// set of advisory IDs (e.g. `RUSTSEC-2025-0141`) explicitly whitelisted.
/// These represent the workspace's documented accepted-risk set; honouring
/// them in F4.5 means we don't penalise already-triaged transitive
/// unmaintained / unsound deps.
fn load_advisories_ignore(lockfile: &Path) -> std::collections::HashSet<String> {
    // Walk up from the lockfile's directory until we find a `deny.toml`
    // (the per-crate `crates/<x>/Cargo.lock` is not the same location as the
    // workspace-root `deny.toml`).
    let mut dir = match lockfile.parent() {
        Some(p) => p.to_path_buf(),
        None => return Default::default(),
    };
    let deny_path = loop {
        let candidate = dir.join("deny.toml");
        if candidate.is_file() {
            break candidate;
        }
        if !dir.pop() {
            return Default::default();
        }
    };
    let Ok(raw) = std::fs::read_to_string(&deny_path) else {
        return Default::default();
    };
    let Ok(doc) = raw.parse::<toml::Table>() else {
        return Default::default();
    };
    let mut out = std::collections::HashSet::new();
    if let Some(adv) = doc.get("advisories").and_then(toml::Value::as_table) {
        if let Some(ignore) = adv.get("ignore").and_then(toml::Value::as_array) {
            for entry in ignore {
                if let Some(t) = entry.as_table() {
                    if let Some(id) = t.get("id").and_then(toml::Value::as_str) {
                        out.insert(id.to_string());
                    }
                }
            }
        }
    }
    out
}

/// Test whether an informational advisory finding (the string returned by
/// `scan_informational` includes the RUSTSEC id) appears in the workspace's
/// explicit `ignore` list.
fn is_ignored_advisory(entry: &str, ignored: &std::collections::HashSet<String>) -> bool {
    ignored.iter().any(|id| entry.contains(id))
}

/// For a `skip` entry without a concrete version, return the
/// `(major, minor)` pair the entry targets (range-form). `Some((2, 11))`
/// means "any 2.11.x is skipped". `None` means exact-version form.
fn skip_range_for(skip: &std::collections::HashSet<String>, name: &str) -> Option<(u32, u32)> {
    for entry in skip {
        if let Some(rest) = entry.strip_prefix(&format!("{name}@")) {
            if let Some((maj, min)) = rest.split_once('.') {
                if let (Ok(m), Ok(n)) = (maj.parse(), min.parse()) {
                    return Some((m, n));
                }
            }
        }
    }
    None
}

/// Per-lockfile cache of the informational (unmaintained/unsound) advisory set,
/// so a workspace sweep loads the RustSec DB + scans the lock once per distinct
/// lockfile rather than once per manifest. Mirrors F2.5's cache.
fn informational_cache() -> &'static Mutex<HashMap<PathBuf, Arc<Vec<String>>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<String>>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Scan the resolved tree for unmaintained/unsound deps via the RustSec advisory
/// DB (the `informational` advisories F2.5 defers to F4.5). `Err` is a graceful
/// note (offline DB) — never a hard failure.
fn scan_informational(lockfile: &Path) -> Result<Vec<String>, String> {
    let key = lockfile
        .canonicalize()
        .unwrap_or_else(|_| lockfile.to_path_buf());
    if let Ok(guard) = informational_cache().lock()
        && let Some(hit) = guard.get(&key)
    {
        return Ok((**hit).clone());
    }
    let db = crate::security::SecurityDb::try_open();
    if !db.is_online() {
        return Err("RustSec advisory DB offline (~/.cargo/advisory-db absent)".to_string());
    }
    let mut out: Vec<String> = db
        .scan_lockfile(lockfile)
        .into_iter()
        .filter_map(|a| {
            a.informational
                .as_deref()
                .map(|kind| format!("{}@{} ({}; {})", a.package, a.version, a.id, kind))
        })
        .collect();
    out.sort();
    out.dedup();
    if let Ok(mut guard) = informational_cache().lock() {
        guard.insert(key, Arc::new(out.clone()));
    }
    Ok(out)
}

/// cargo-machete-style unused-dep detection: a declared registry dep whose
/// normalized name (`-`→`_`) is never referenced anywhere in the crate's
/// sources. Conservative — substring presence errs toward "used" (so a shared
/// prefix like `serde`/`serde_json` is never falsely flagged unused), keeping
/// this WARN-level signal free of false fail-flags.
fn detect_unused(manifest_dir: &Path, deps: &[String]) -> Vec<String> {
    let mut haystack = String::new();
    let mut count = 0usize;
    for sub in ["src", "tests", "benches", "examples"] {
        collect_rs(&manifest_dir.join(sub), &mut haystack, &mut count);
    }
    if let Ok(s) = std::fs::read_to_string(manifest_dir.join("build.rs")) {
        haystack.push_str(&s);
        haystack.push('\n');
    }
    // No sources to inspect → cannot determine; emit nothing.
    if haystack.is_empty() {
        return Vec::new();
    }
    let mut unused: Vec<String> = deps
        .iter()
        .filter(|dep| {
            let norm = dep.replace('-', "_");
            !norm.is_empty() && !haystack.contains(&norm)
        })
        .cloned()
        .collect();
    unused.sort();
    unused.dedup();
    unused
}

/// Append every `.rs` file's contents under `dir` (recursively, bounded) to
/// `hay`. Skips `target/` and hidden directories.
fn collect_rs(dir: &Path, hay: &mut String, count: &mut usize) {
    if *count >= MAX_SOURCE_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if *count >= MAX_SOURCE_FILES {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name.starts_with('.') {
                continue;
            }
            collect_rs(&path, hay, count);
        } else if path.extension().and_then(|x| x.to_str()) == Some("rs") {
            if let Ok(s) = std::fs::read_to_string(&path) {
                hay.push_str(&s);
                hay.push('\n');
                *count += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Pure scoring calibration (the anti-theater invariant) ────────────────
    #[test]
    fn clean_manifest_scores_one() {
        assert!((compute_score(0, 0, 0, 0, 0, 0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn single_prod_wildcard_blocks() {
        // cargo-deny bans.wildcards="deny": one registry `*` → 0.40 < 0.5.
        let s = compute_score(1, 0, 0, 0, 0, 0);
        assert!((s - 0.40).abs() < 1e-6, "one prod wildcard must BLOCK: {s}");
        assert!(s < 0.5);
    }

    #[test]
    fn hygiene_alone_never_blocks() {
        // Even an egregious hygiene load (many unmaintained + dups + unused +
        // dev wildcards) is capped at HYGIENE_CAP → score >= 0.55 (WARN).
        let s = compute_score(0, 5, 20, 5, 40, 30);
        assert!(s >= 0.55 - 1e-6, "hygiene must never fail-close: {s}");
        assert!(s < 1.0, "but it must still lower the score: {s}");
    }

    #[test]
    fn dev_wildcard_is_warn_not_block() {
        // A dev-dependency wildcard does not ship → WARN, never sole BLOCK.
        let s = compute_score(0, 1, 0, 0, 0, 0);
        assert!(s >= 0.5, "dev wildcard alone must not BLOCK: {s}");
        assert!(s < 1.0, "but must be penalised: {s}");
    }

    #[test]
    fn two_prod_wildcards_clamp_to_zero() {
        assert!((compute_score(2, 0, 0, 0, 0, 0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn unmaintained_scales_then_caps() {
        // 1 unmaintained → small penalty; many → capped (never below 0.55 alone).
        let one = compute_score(0, 0, 1, 0, 0, 0);
        let many = compute_score(0, 0, 50, 0, 0, 0);
        assert!(
            one > many,
            "more unmaintained must score lower: {one} vs {many}"
        );
        assert!(many >= 0.55 - 1e-6, "capped above BLOCK: {many}");
    }

    // ── Spec classification ──────────────────────────────────────────────────
    #[test]
    fn classify_string_wildcard() {
        assert_eq!(
            classify_dep_spec(&toml::Value::String("*".into())),
            (true, true)
        );
        assert_eq!(
            classify_dep_spec(&toml::Value::String("1.0".into())),
            (false, true)
        );
    }

    #[test]
    fn classify_table_specs() {
        let parse = |s: &str| {
            s.parse::<toml::Table>()
                .expect("toml")
                .remove("d")
                .expect("d")
        };
        // version = "*" → wildcard registry dep.
        assert_eq!(
            classify_dep_spec(&parse("d = { version = \"*\" }")),
            (true, true)
        );
        // path dep → not a registry wildcard.
        assert_eq!(
            classify_dep_spec(&parse("d = { path = \"../x\", version = \"*\" }")),
            (false, false)
        );
        // workspace-inherited → not a registry wildcard.
        assert_eq!(
            classify_dep_spec(&parse("d = { workspace = true }")),
            (false, false)
        );
        // pinned registry dep.
        assert_eq!(
            classify_dep_spec(&parse("d = { version = \"1.2\" }")),
            (false, true)
        );
    }

    // ── End-to-end on synthetic manifests ────────────────────────────────────
    #[test]
    fn non_manifest_target_passes() {
        let dir = tempfile::tempdir().expect("tmp");
        let rs = dir.path().join("lib.rs");
        std::fs::write(&rs, "fn main() {}\n").expect("write");
        let r = DepHealthAnalyzer::new().analyze(&rs);
        assert!(!r.manifest_scoped);
        assert!((r.score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn prod_wildcard_manifest_blocks() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\n[dependencies]\nfoo = \"*\"\nbar = \"1.0\"\n",
        )
        .expect("write");
        let r = DepHealthAnalyzer::new().analyze(dir.path());
        assert!(r.manifest_scoped);
        assert_eq!(r.wildcards.len(), 1, "{:?}", r.wildcards);
        assert!(r.wildcards[0].starts_with("foo"));
        assert!(r.score < 0.5, "prod wildcard must BLOCK: {}", r.score);
    }

    #[test]
    fn clean_manifest_no_deps_passes() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\n[dependencies]\nserde = \"1.0\"\n",
        )
        .expect("write");
        // No Cargo.lock in /tmp ancestry → unmaintained/dup skipped (graceful).
        // serde has no src/ → unused not determinable. Score should be perfect.
        let r = DepHealthAnalyzer::new().analyze(dir.path());
        assert!((0.0..=1.0).contains(&r.score));
        assert!(r.wildcards.is_empty());
    }

    #[test]
    fn duplicate_versions_detected() {
        let dir = tempfile::tempdir().expect("tmp");
        let lock = dir.path().join("Cargo.lock");
        std::fs::write(
            &lock,
            "version = 3\n\
             [[package]]\nname = \"syn\"\nversion = \"1.0.1\"\n\
             [[package]]\nname = \"syn\"\nversion = \"2.0.0\"\n\
             [[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n",
        )
        .expect("write");
        let dups = detect_duplicate_versions(&lock);
        assert_eq!(dups.len(), 1, "{dups:?}");
        assert!(dups[0].starts_with("syn (2 versions"));
    }

    #[test]
    fn unused_dep_detected_conservatively() {
        let dir = tempfile::tempdir().expect("tmp");
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).expect("mkdir");
        std::fs::write(src.join("lib.rs"), "use serde::Serialize;\n").expect("write");
        // `serde` is referenced; `unused_crate` is not.
        let deps = vec!["serde".to_string(), "unused_crate".to_string()];
        let unused = detect_unused(dir.path(), &deps);
        assert_eq!(unused, vec!["unused_crate".to_string()], "{unused:?}");
    }

    #[test]
    fn unused_no_sources_emits_nothing() {
        let dir = tempfile::tempdir().expect("tmp");
        let unused = detect_unused(dir.path(), &["anything".to_string()]);
        assert!(
            unused.is_empty(),
            "no sources → cannot determine: {unused:?}"
        );
    }
}
