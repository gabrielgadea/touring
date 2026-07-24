//! P5.2 — the full staging registry.
//!
//! CEG Pln2 Phase **P5** (`docs/2026-05-17-ceg-pln2-plan.md`). P1.6 shipped a
//! `StagingRegistry` *stub* — a process-local, path-keyed map. P5.2 promotes
//! it to the full registry this module owns: **content-indexed**,
//! **cross-session** (persisted to disk), and storing the **rich X2/X3
//! verdict** ([`StaticReport`] + [`VgpReport`]) so a later execution recovers
//! the full prior analysis instead of a coarse pass/fail bool.
//!
//! # Why content-indexed
//!
//! The threat (CEG Pln2 risk **R9**) is the heredoc temporal-split: a body is
//! written in one turn and run in a later one. The stub keyed entries by
//! *path* — but a script can be written to `/tmp/a.sh` and run from a copy at
//! `/tmp/b.sh`, or the path can be reconstructed differently between turns.
//! This registry keys on the **blake3 hash of the body** ([`content_hash`]):
//! resolution is by *what the code is*, not *where it sits*. A secondary
//! path index ([`StagingRegistry::resolve_path`]) still serves the
//! `bash <path>` execution form, which carries only a path.
//!
//! # No re-analysis needed
//!
//! A [`RegistryEntry`] records the X2 [`StaticReport`] and X3 [`VgpReport`]
//! verbatim. Persisted to `<staging_root>/registry.json` and reloaded, a later
//! execution — even in a fresh process — resolves the entry and reuses the
//! stage-time verdict. The gateway never re-runs X2-X5 on a body it already
//! analysed and that has not changed.
//!
//! # Reuse
//!
//! Builds on [`StagingArea`] (P5.1) for the on-disk write and on the P1.6
//! vocabulary — [`classify_command`], [`TemporalSplitSignal`],
//! [`StagedScript`], [`StagingGateDecision`], [`ReanalysisReason`] — for the
//! classifier seam and the gate decision surface.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::gateway::staging_classify::{
    ReanalysisReason, StagedScript, StagingGateDecision, TemporalSplitSignal, classify_command,
};

use super::staging::{StagingArea, staging_root};
use super::static_stage::{StaticReport, StaticSeverity};
use super::vgp_stage::VgpReport;

/// The blake3 content hash of a code body, hex-encoded.
///
/// The registry's primary key. Two bodies with identical bytes hash to the
/// same key — that is what makes the registry temporal-split-resistant.
#[must_use]
pub fn content_hash(body: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(body);
    hasher.finalize().to_hex().to_string()
}

/// Unix seconds now, saturating to `0` on a clock before the epoch.
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One staged code body, with the X2/X3 analysis the gateway computed for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// The staged-script identity — path, content hash, origin, the coarse
    /// pass/fail verdict. Reuses the P1.6 [`StagedScript`] vocabulary.
    pub script: StagedScript,
    /// The X2 STATIC verdict captured when the body was staged.
    pub static_report: StaticReport,
    /// The X3 VGP verdict captured when the body was staged.
    pub vgp_report: VgpReport,
    /// Unix seconds at which the entry was recorded.
    pub registered_at_unix: u64,
}

impl RegistryEntry {
    /// `true` when both stage-time verdicts cleared: the X2 severity is not
    /// [`StaticSeverity::Block`] and every X3 reference resolved. This is the
    /// composite "passed" signal — `Warn` is surfaced but not blocking.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.static_report.severity != StaticSeverity::Block && self.vgp_report.all_resolved()
    }
}

/// The full, content-indexed, persistable staging registry.
///
/// `entries` is the primary index (content hash → entry); `by_path` is the
/// secondary index (staged path → content hash) so a path-only execution
/// still resolves. Both are kept consistent by every mutator.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagingRegistry {
    /// content hash → entry — the primary, temporal-split-resistant index.
    entries: BTreeMap<String, RegistryEntry>,
    /// staged path → content hash — the secondary index for `bash <path>`.
    by_path: BTreeMap<PathBuf, String>,
}

impl StagingRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The canonical on-disk location: `<staging_root>/registry.json`.
    /// Reuses [`staging_root`] (P5.1) so the registry lives inside the managed
    /// staging tree it indexes.
    #[must_use]
    pub fn default_path() -> PathBuf {
        staging_root().join("registry.json")
    }

    /// Loads a registry from `path`. A missing file yields an empty registry
    /// (first run) — callers may `load` unconditionally.
    pub fn load(path: &Path) -> io::Result<Self> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::new()),
            Err(e) => Err(e),
        }
    }

    /// Persists the registry to `path` atomically — written to a sibling
    /// `.tmp` file, then renamed, so a concurrent reader never observes a
    /// half-written registry.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, &json)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Records a pre-built entry, indexing it by content hash and by path.
    /// Re-registering the same hash supersedes the prior entry.
    pub fn register(&mut self, entry: RegistryEntry) {
        let hash = entry.script.content_hash.clone();
        self.by_path.insert(entry.script.path.clone(), hash.clone());
        self.entries.insert(hash, entry);
    }

    /// Stages `body` into `area` under `file_name`, then records it with the
    /// X2/X3 verdicts the gateway computed. This is the P5.2 acceptance path
    /// — "a staged script is indexed".
    ///
    /// Reuses [`StagingArea::stage`] (P5.1) for the on-disk write.
    pub fn register_staged(
        &mut self,
        area: &StagingArea,
        file_name: &str,
        body: &[u8],
        origin: impl Into<String>,
        static_report: StaticReport,
        vgp_report: VgpReport,
    ) -> io::Result<RegistryEntry> {
        let staged_path = area.stage(file_name, body)?;
        let verdict = static_report.severity != StaticSeverity::Block && vgp_report.all_resolved();
        let entry = RegistryEntry {
            script: StagedScript {
                path: staged_path,
                content_hash: content_hash(body),
                origin: origin.into(),
                prior_verdict: Some(verdict),
            },
            static_report,
            vgp_report,
            registered_at_unix: now_unix(),
        };
        self.register(entry.clone());
        Ok(entry)
    }

    /// Resolves an entry by the **content** of a body — the temporal-split
    /// resolver. A body staged in an earlier turn resolves here regardless of
    /// the path a later execution names.
    #[must_use]
    pub fn resolve_body(&self, body: &[u8]) -> Option<&RegistryEntry> {
        self.entries.get(&content_hash(body))
    }

    /// Resolves an entry by content hash directly.
    #[must_use]
    pub fn resolve_hash(&self, hash: &str) -> Option<&RegistryEntry> {
        self.entries.get(hash)
    }

    /// Resolves an entry by its staged path (the secondary index).
    #[must_use]
    pub fn resolve_path(&self, path: &Path) -> Option<&RegistryEntry> {
        self.by_path.get(path).and_then(|h| self.entries.get(h))
    }

    /// The R9 gate decision for an execution that names a `path`, given the
    /// content hash observed **now**. Reuses the P1.6 [`StagingGateDecision`]:
    ///
    /// - path absent from the registry → [`ReanalysisReason::Unregistered`]
    /// - registered but the hash differs → [`ReanalysisReason::ContentChanged`]
    /// - registered, unchanged, no stage verdict → [`ReanalysisReason::NeverAnalysed`]
    /// - registered, unchanged, verdict present → [`StagingGateDecision::ReuseVerdict`]
    #[must_use]
    pub fn gate_decision_for_execution(
        &self,
        path: &Path,
        current_hash: &str,
    ) -> StagingGateDecision {
        match self.resolve_path(path) {
            None => StagingGateDecision::RequiresReanalysis {
                reason: ReanalysisReason::Unregistered,
            },
            Some(entry) if entry.script.content_hash != current_hash => {
                StagingGateDecision::RequiresReanalysis {
                    reason: ReanalysisReason::ContentChanged,
                }
            }
            Some(entry) => verdict_decision(entry),
        }
    }

    /// The R9 gate decision for an execution whose `body` is in hand — the
    /// content-keyed variant. Temporal-split-resistant: it resolves by hash,
    /// so a path mismatch between the write turn and the run turn is moot.
    #[must_use]
    pub fn gate_decision_for_body(&self, body: &[u8]) -> StagingGateDecision {
        match self.resolve_body(body) {
            None => StagingGateDecision::RequiresReanalysis {
                reason: ReanalysisReason::Unregistered,
            },
            Some(entry) => verdict_decision(entry),
        }
    }

    /// The end-to-end temporal-split check for a bash command: classify it,
    /// and when it runs a script, return the gate decision for that path.
    /// Returns `None` when the command runs no script (nothing to decide).
    ///
    /// Reuses [`classify_command`] (P1.6) — this is the seam joining the
    /// classifier to the registry.
    #[must_use]
    pub fn gate_for_command(
        &self,
        command: &str,
        current_hash: &str,
    ) -> Option<StagingGateDecision> {
        match classify_command(command) {
            TemporalSplitSignal::ExecutesScript { path } => {
                Some(self.gate_decision_for_execution(&path, current_hash))
            }
            TemporalSplitSignal::WritesScript { .. } | TemporalSplitSignal::Neither => None,
        }
    }

    /// Prunes entries registered more than `retention_secs` ago. Returns the
    /// count removed; keeps the path index consistent.
    pub fn gc(&mut self, retention_secs: u64) -> u64 {
        let cutoff = now_unix().saturating_sub(retention_secs);
        let before = self.entries.len();
        let mut dropped_paths: Vec<PathBuf> = Vec::new();
        self.entries.retain(|_, entry| {
            let keep = entry.registered_at_unix > cutoff;
            if !keep {
                dropped_paths.push(entry.script.path.clone());
            }
            keep
        });
        for path in &dropped_paths {
            self.by_path.remove(path);
        }
        u64::try_from(before - self.entries.len()).unwrap_or(u64::MAX)
    }

    /// The number of entries recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when the registry has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `true` when an entry with `hash` is recorded.
    #[must_use]
    pub fn contains_hash(&self, hash: &str) -> bool {
        self.entries.contains_key(hash)
    }
}

/// Maps a resolved, unchanged entry to its gate decision: reuse the verdict
/// when one was recorded, else force re-analysis.
fn verdict_decision(entry: &RegistryEntry) -> StagingGateDecision {
    match entry.script.prior_verdict {
        Some(verdict) => StagingGateDecision::ReuseVerdict { verdict },
        None => StagingGateDecision::RequiresReanalysis {
            reason: ReanalysisReason::NeverAnalysed,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_report() -> StaticReport {
        StaticReport {
            severity: StaticSeverity::Clear,
            findings: Vec::new(),
            risk_summary: None,
        }
    }

    fn block_report() -> StaticReport {
        StaticReport {
            severity: StaticSeverity::Block,
            findings: vec!["structural: destructive command".to_string()],
            risk_summary: None,
        }
    }

    fn resolved_vgp() -> VgpReport {
        VgpReport {
            verified: vec!["touring_index".to_string()],
            unresolved: Vec::new(),
        }
    }

    fn unresolved_vgp() -> VgpReport {
        VgpReport {
            verified: Vec::new(),
            unresolved: vec!["ghost_symbol".to_string()],
        }
    }

    fn entry_at(path: &str, hash: &str, at: u64, verdict: Option<bool>) -> RegistryEntry {
        RegistryEntry {
            script: StagedScript {
                path: PathBuf::from(path),
                content_hash: hash.to_string(),
                origin: "test".to_string(),
                prior_verdict: verdict,
            },
            static_report: clear_report(),
            vgp_report: resolved_vgp(),
            registered_at_unix: at,
        }
    }

    // ── content hash ──

    #[test]
    fn content_hash_deterministic_and_distinct() {
        assert_eq!(content_hash(b"echo hello"), content_hash(b"echo hello"));
        assert_ne!(content_hash(b"echo hello"), content_hash(b"echo world"));
    }

    // ── register / resolve ──

    #[test]
    fn register_and_resolve_by_hash() {
        let mut reg = StagingRegistry::new();
        assert!(reg.is_empty());
        reg.register(entry_at("/tmp/a.sh", "h1", 100, Some(true)));
        assert_eq!(reg.len(), 1);
        assert!(reg.contains_hash("h1"));
        assert_eq!(reg.resolve_hash("h1").unwrap().script.origin, "test");
        assert!(reg.resolve_hash("missing").is_none());
    }

    #[test]
    fn resolve_body_is_temporal_split_resistant() {
        let mut reg = StagingRegistry::new();
        let body = b"deploy --prod";
        // Register under one path, content-keyed.
        reg.register(entry_at(
            "/tmp/written-here.sh",
            &content_hash(body),
            100,
            Some(true),
        ));
        // The body resolves regardless of the path it later runs from.
        assert!(reg.resolve_body(body).is_some());
        assert!(reg.resolve_body(b"a different body").is_none());
    }

    #[test]
    fn resolve_path_secondary_index() {
        let mut reg = StagingRegistry::new();
        reg.register(entry_at("/tmp/p.sh", "ph", 100, Some(true)));
        assert!(reg.resolve_path(Path::new("/tmp/p.sh")).is_some());
        assert!(reg.resolve_path(Path::new("/tmp/other.sh")).is_none());
    }

    #[test]
    fn re_register_same_hash_supersedes() {
        let mut reg = StagingRegistry::new();
        reg.register(entry_at("/tmp/a.sh", "h", 100, Some(false)));
        reg.register(entry_at("/tmp/a.sh", "h", 200, Some(true)));
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.resolve_hash("h").unwrap().registered_at_unix, 200);
    }

    // ── register_staged — reuses StagingArea ──

    #[test]
    fn register_staged_writes_through_staging_area() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let area = StagingArea::with_root(tmp.path(), "p52-sess");
        let mut reg = StagingRegistry::new();
        let body = b"echo staged-and-indexed";
        let entry = reg
            .register_staged(
                &area,
                "job.sh",
                body,
                "turn-1",
                clear_report(),
                resolved_vgp(),
            )
            .expect("register_staged");
        // The body was written to disk by StagingArea.
        assert!(entry.script.path.exists());
        assert_eq!(fs::read(&entry.script.path).expect("read"), body);
        // ...and indexed: resolvable by content.
        assert_eq!(reg.len(), 1);
        assert!(reg.resolve_body(body).is_some());
        assert_eq!(reg.resolve_body(body).unwrap().script.origin, "turn-1");
    }

    // ── RegistryEntry::passed — composite verdict ──

    #[test]
    fn registry_entry_passed_composite() {
        let pass = RegistryEntry {
            static_report: clear_report(),
            vgp_report: resolved_vgp(),
            ..entry_at("/x", "h", 0, Some(true))
        };
        assert!(pass.passed());

        let blocked = RegistryEntry {
            static_report: block_report(),
            ..pass.clone()
        };
        assert!(
            !blocked.passed(),
            "X2 Block must fail the composite verdict"
        );

        let unverified = RegistryEntry {
            vgp_report: unresolved_vgp(),
            ..pass.clone()
        };
        assert!(
            !unverified.passed(),
            "an unresolved X3 reference must fail the composite verdict"
        );
    }

    // ── gate decisions — risk R9 ──

    #[test]
    fn gate_unregistered_forces_reanalysis() {
        let reg = StagingRegistry::new();
        assert_eq!(
            reg.gate_decision_for_execution(Path::new("/tmp/unseen.sh"), "any"),
            StagingGateDecision::RequiresReanalysis {
                reason: ReanalysisReason::Unregistered
            }
        );
    }

    #[test]
    fn gate_content_changed_forces_reanalysis() {
        let mut reg = StagingRegistry::new();
        reg.register(entry_at("/tmp/x.sh", "stage-hash", 100, Some(true)));
        assert_eq!(
            reg.gate_decision_for_execution(Path::new("/tmp/x.sh"), "mutated-hash"),
            StagingGateDecision::RequiresReanalysis {
                reason: ReanalysisReason::ContentChanged
            }
        );
    }

    #[test]
    fn gate_known_unchanged_reuses_verdict() {
        let mut reg = StagingRegistry::new();
        reg.register(entry_at("/tmp/x.sh", "h", 100, Some(true)));
        assert_eq!(
            reg.gate_decision_for_execution(Path::new("/tmp/x.sh"), "h"),
            StagingGateDecision::ReuseVerdict { verdict: true }
        );
    }

    #[test]
    fn gate_never_analysed_forces_reanalysis() {
        let mut reg = StagingRegistry::new();
        reg.register(entry_at("/tmp/x.sh", "h", 100, None));
        assert_eq!(
            reg.gate_decision_for_execution(Path::new("/tmp/x.sh"), "h"),
            StagingGateDecision::RequiresReanalysis {
                reason: ReanalysisReason::NeverAnalysed
            }
        );
    }

    #[test]
    fn gate_decision_for_body_resolves_by_content() {
        let mut reg = StagingRegistry::new();
        let body = b"run the thing";
        reg.register(entry_at("/tmp/x.sh", &content_hash(body), 100, Some(true)));
        assert_eq!(
            reg.gate_decision_for_body(body),
            StagingGateDecision::ReuseVerdict { verdict: true }
        );
        assert_eq!(
            reg.gate_decision_for_body(b"never staged"),
            StagingGateDecision::RequiresReanalysis {
                reason: ReanalysisReason::Unregistered
            }
        );
    }

    // ── gate_for_command — reuses classify_command ──

    #[test]
    fn gate_for_command_classifies_then_decides() {
        let reg = StagingRegistry::new();
        // A script run that the registry never saw → forced re-analysis.
        assert_eq!(
            reg.gate_for_command("bash /tmp/x.sh", "h"),
            Some(StagingGateDecision::RequiresReanalysis {
                reason: ReanalysisReason::Unregistered
            })
        );
    }

    #[test]
    fn gate_for_command_none_for_nonscript() {
        let reg = StagingRegistry::new();
        assert!(reg.gate_for_command("ls -la /tmp", "h").is_none());
        // A write is not a run — nothing for the registry to decide here.
        assert!(reg.gate_for_command("cat > /tmp/x.sh", "h").is_none());
    }

    // ── gc ──

    #[test]
    fn gc_prunes_stale_entries() {
        let mut reg = StagingRegistry::new();
        reg.register(entry_at("/tmp/old.sh", "old", 0, Some(true)));
        reg.register(entry_at("/tmp/new.sh", "new", now_unix(), Some(true)));
        let removed = reg.gc(3600);
        assert_eq!(removed, 1);
        assert_eq!(reg.len(), 1);
        assert!(reg.resolve_hash("new").is_some());
        // The path index of the pruned entry is gone too.
        assert!(reg.resolve_path(Path::new("/tmp/old.sh")).is_none());
    }

    #[test]
    fn default_path_under_staging_root() {
        assert!(StagingRegistry::default_path().ends_with("registry.json"));
    }

    // ── E2E 1 — the full temporal-split flow ──

    #[test]
    fn e2e_temporal_split_write_then_later_run() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let area = StagingArea::with_root(tmp.path(), "e2e-sess");
        let mut reg = StagingRegistry::new();
        let body = b"echo deploy";

        // Turn N — a heredoc writes a script; the gate stages + indexes it.
        let written = classify_command("cat > /tmp/deploy.sh <<EOF");
        assert!(matches!(written, TemporalSplitSignal::WritesScript { .. }));
        let entry = reg
            .register_staged(
                &area,
                "deploy.sh",
                body,
                "turn-N",
                clear_report(),
                resolved_vgp(),
            )
            .expect("register_staged");
        let hash = entry.script.content_hash.clone();

        // Turn N+M — the script is run; classify + gate by the staged path.
        let run = classify_command("bash /tmp/deploy.sh");
        assert!(matches!(run, TemporalSplitSignal::ExecutesScript { .. }));

        // Unchanged content → the stage-time verdict is reused, no re-analysis.
        assert_eq!(
            reg.gate_decision_for_execution(&entry.script.path, &hash),
            StagingGateDecision::ReuseVerdict { verdict: true }
        );
        // Mutated between turns → forced re-analysis, no stale verdict trusted.
        assert_eq!(
            reg.gate_decision_for_execution(&entry.script.path, "mutated"),
            StagingGateDecision::RequiresReanalysis {
                reason: ReanalysisReason::ContentChanged
            }
        );
        // A script that evaded registration → still forced through analysis.
        assert_eq!(
            reg.gate_for_command("bash /tmp/evaded.sh", "x"),
            Some(StagingGateDecision::RequiresReanalysis {
                reason: ReanalysisReason::Unregistered
            })
        );
    }

    // ── E2E 2 — persistence recovers the X2/X3 verdict ──

    #[test]
    fn e2e_persistence_round_trip_recovers_verdict() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let area = StagingArea::with_root(tmp.path(), "persist-sess");
        let body = b"python3 build step";
        let static_report = clear_report();
        let vgp_report = resolved_vgp();

        // Stage + index, then persist to disk.
        let mut reg = StagingRegistry::new();
        let original = reg
            .register_staged(
                &area,
                "step.sh",
                body,
                "turn-N",
                static_report.clone(),
                vgp_report.clone(),
            )
            .expect("register_staged");
        let registry_path = tmp.path().join("registry.json");
        reg.save(&registry_path).expect("save");

        // A later process (a fresh `StagingRegistry`) loads and resolves it —
        // the full X2/X3 verdict is recovered, so X2-X5 need not re-run.
        let reloaded = StagingRegistry::load(&registry_path).expect("load");
        let recovered = reloaded.resolve_body(body).expect("entry survives reload");
        assert_eq!(recovered, &original);
        assert_eq!(recovered.static_report, static_report);
        assert_eq!(recovered.vgp_report, vgp_report);
        assert!(recovered.passed());
    }

    #[test]
    fn load_missing_file_yields_empty_registry() {
        let reg = StagingRegistry::load(Path::new("/nonexistent/touring/registry.json"))
            .expect("missing file is not an error");
        assert!(reg.is_empty());
    }
}
