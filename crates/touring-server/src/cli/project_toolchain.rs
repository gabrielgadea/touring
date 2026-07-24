//! Per-project toolchain state — the single home of the paired concepts
//! channel ↔ lockfile ↔ `.touring/bin` links (Pln2 productization F3).
//!
//! Why one module: the F1 cross-audit (F-NEW-1, 2026-07-24) showed that paired
//! resources updated on different sides of a refactor drift apart (socket vs
//! lock). Channel resolution, the toolchain lockfile, and the bin re-link all
//! describe ONE state machine, so they live together and every consumer
//! (`init-project`, `update`, `component`) goes through the same functions.
//!
//! State model (rustup-like requested-vs-resolved):
//! - `.touring/touring.toml` `[toolchain] channel` — the HUMAN's requested pin;
//!   never rewritten by machines.
//! - `.touring/toolchain.lock` — the MACHINE's resolved state: `active` channel,
//!   `previous` (deterministic `touring update --rollback`), timestamp, reason.
//! - Active-channel resolution: lock `active` > toml pin > none (dev fallback).

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

/// The per-project core binaries the rustup-style layout installs. These are
/// never removable via `touring component remove` (potentialize, never reduce).
pub(crate) const PROJECT_BINARIES: &[&str] = &["touring", "touring-hook", "touring-daemon"];

/// Lockfile name under `.touring/`.
pub(crate) const LOCK_FILE: &str = "toolchain.lock";

/// The dev channel pseudo-version: bins come from `~/.local/bin` (the
/// `update-touring` managed symlinks) instead of an installed toolchain.
pub(crate) const DEV_CHANNEL: &str = "dev";

/// Resolved machine state of a project's toolchain (`.touring/toolchain.lock`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolchainLock {
    /// The channel `.touring/bin` is currently linked against.
    pub active: String,
    /// The previously-active channel (target of `update --rollback`).
    pub previous: Option<String>,
    /// Unix seconds of the last update.
    pub updated_at: u64,
    /// Human-readable provenance of the last transition.
    pub reason: String,
}

impl ToolchainLock {
    /// Read the lock from `.touring/toolchain.lock`. `None` when absent or
    /// malformed — a broken lock must degrade to the toml pin, never panic.
    pub fn read(dot_touring: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(dot_touring.join(LOCK_FILE)).ok()?;
        let value = text.parse::<toml::Value>().ok()?;
        let active = value.get("active")?.as_str()?.to_string();
        Some(Self {
            active,
            previous: value
                .get("previous")
                .and_then(toml::Value::as_str)
                .map(String::from),
            updated_at: value
                .get("updated_at")
                .and_then(toml::Value::as_integer)
                .unwrap_or(0) as u64,
            reason: value
                .get("reason")
                .and_then(toml::Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
    }

    /// Write the lock atomically (tmp + rename) under `.touring/`.
    pub fn write(&self, dot_touring: &Path) -> Result<()> {
        let mut body = format!(
            "# Machine-managed by `touring update` — do not edit by hand.\n\
             # The requested pin lives in touring.toml [toolchain]; this file is\n\
             # the RESOLVED state (active + previous for deterministic rollback).\n\
             active = \"{}\"\n",
            self.active
        );
        if let Some(prev) = &self.previous {
            body.push_str(&format!("previous = \"{prev}\"\n"));
        }
        body.push_str(&format!("updated_at = {}\n", self.updated_at));
        body.push_str(&format!("reason = \"{}\"\n", self.reason.replace('"', "'")));
        let path = dot_touring.join(LOCK_FILE);
        let tmp = dot_touring.join(format!("{LOCK_FILE}.tmp"));
        std::fs::write(&tmp, body).map_err(|e| anyhow!("write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path)
            .map_err(|e| anyhow!("rename {} -> {}: {e}", tmp.display(), path.display()))?;
        Ok(())
    }
}

/// Read `[toolchain] channel` from a project `touring.toml` (None when the
/// file is absent/bare/malformed — malformed must not break scaffolding).
pub(crate) fn pinned_channel(toml_path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(toml_path).ok()?;
    let value = text.parse::<toml::Value>().ok()?;
    value
        .get("toolchain")
        .and_then(|t| t.get("channel"))
        .and_then(toml::Value::as_str)
        .map(String::from)
}

/// The active channel of a project: lockfile `active` (machine-resolved) wins
/// over the touring.toml pin (human-requested). `None` = unpinned (dev
/// fallback only).
pub(crate) fn resolve_active_channel(dot_touring: &Path) -> Option<String> {
    ToolchainLock::read(dot_touring)
        .map(|l| l.active)
        .or_else(|| pinned_channel(&dot_touring.join("touring.toml")))
}

/// Resolve where binary `name` for `channel` lives (first hit wins):
///   1. `$TOURING_HOME/toolchains/<channel>/bin/<name>` (skipped for the
///      `dev` pseudo-channel);
///   2. dev fallback `<dev_bin_dir>/<name>`;
///   3. `None` — nowhere.
pub(crate) fn resolve_binary_target(
    name: &str,
    channel: Option<&str>,
    touring_home: &Path,
    dev_bin_dir: &Path,
) -> Option<PathBuf> {
    channel
        .filter(|c| *c != DEV_CHANNEL)
        .map(|c| {
            touring_home
                .join("toolchains")
                .join(c)
                .join("bin")
                .join(name)
        })
        .filter(|p| p.exists())
        .or_else(|| Some(dev_bin_dir.join(name)).filter(|p| p.exists()))
}

/// Re-link `.touring/bin/` against the ACTIVE channel (lock > toml pin).
///
/// Symlinks, not copies: a toolchain upgrade swaps targets atomically and disk
/// stays honest. Fail-open by design — scaffolding must never hard-fail on a
/// half-installed toolchain; the walk-up shim simply falls through to its next
/// layer until the bin appears. Returns human-readable notes per binary.
pub(crate) fn relink_bins_inner(
    dot_touring: &Path,
    touring_home: &Path,
    dev_bin_dir: &Path,
) -> Vec<String> {
    let mut notes = Vec::new();
    let channel = resolve_active_channel(dot_touring);
    for name in PROJECT_BINARIES {
        let target = resolve_binary_target(name, channel.as_deref(), touring_home, dev_bin_dir);
        let link = dot_touring.join("bin").join(name);
        match target {
            Some(target) => {
                let _ = std::fs::remove_file(&link);
                match std::os::unix::fs::symlink(&target, &link) {
                    Ok(()) => notes.push(format!("bin/{name} -> {}", target.display())),
                    Err(e) => notes.push(format!("bin/{name}: symlink failed ({e}) — skipped")),
                }
            }
            None => notes.push(format!(
                "bin/{name}: not found in toolchain {:?} nor dev channel {} — run `touring toolchain install` (or update-touring) then re-run init-project --force",
                channel.as_deref().unwrap_or("<unpinned>"),
                dev_bin_dir.display()
            )),
        }
    }
    notes
}

/// Env-resolving wrapper over [`relink_bins_inner`] (production entry).
pub(crate) fn relink_bins(dot_touring: &Path) -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let touring_home = std::env::var("TOURING_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(&home).join(".touring"));
    let dev_bin_dir = Path::new(&home).join(".local").join("bin");
    relink_bins_inner(dot_touring, &touring_home, &dev_bin_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_bin(dir: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).expect("mkdir");
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").expect("write");
        let mut perm = std::fs::metadata(&p).expect("meta").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
        std::fs::set_permissions(&p, perm).expect("chmod");
        p
    }

    #[test]
    fn relink_bins_links_pinned_toolchain() {
        // F2 (2.1): the channel pinned in touring.toml resolves to
        // $TOURING_HOME/toolchains/<channel>/bin and gets symlinked.
        let tmp = tempfile::tempdir().expect("tmpdir");
        let dot = tmp.path().join("proj/.touring");
        std::fs::create_dir_all(dot.join("bin")).expect("mkdir");
        std::fs::write(
            dot.join("touring.toml"),
            "[toolchain]\nchannel = \"9.9.9\"\n",
        )
        .expect("write");
        let th = tmp.path().join("touring-home");
        let tc_bin = th.join("toolchains/9.9.9/bin");
        for b in PROJECT_BINARIES {
            fake_bin(&tc_bin, b);
        }
        let notes = relink_bins_inner(&dot, &th, &tmp.path().join("no-dev"));
        assert_eq!(notes.len(), 3);
        for b in PROJECT_BINARIES {
            let link = dot.join("bin").join(b);
            assert!(link.exists(), "bin/{b} must exist");
            assert_eq!(
                std::fs::read_link(&link).expect("symlink"),
                tc_bin.join(b),
                "bin/{b} must point at the pinned toolchain"
            );
        }
    }

    #[test]
    fn relink_bins_falls_back_to_dev_channel() {
        // Pinned toolchain absent → the update-touring dev symlink dir serves.
        let tmp = tempfile::tempdir().expect("tmpdir");
        let dot = tmp.path().join("proj/.touring");
        std::fs::create_dir_all(dot.join("bin")).expect("mkdir");
        std::fs::write(
            dot.join("touring.toml"),
            "[toolchain]\nchannel = \"30.3.0\"\n",
        )
        .expect("write");
        let dev = tmp.path().join("dev-bin");
        fake_bin(&dev, "touring-hook");
        let notes = relink_bins_inner(&dot, &tmp.path().join("empty-th"), &dev);
        assert!(
            std::fs::read_link(dot.join("bin/touring-hook")).expect("symlink")
                == dev.join("touring-hook"),
            "dev-channel fallback must be linked"
        );
        // The two binaries present nowhere are skipped with a note, not an error.
        assert_eq!(notes.iter().filter(|n| n.contains("not found")).count(), 2);
    }

    #[test]
    fn relink_bins_is_fail_open_when_nothing_available() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let dot = tmp.path().join("proj/.touring");
        std::fs::create_dir_all(dot.join("bin")).expect("mkdir");
        // No touring.toml at all (bare) + no toolchain + no dev bin.
        let notes = relink_bins_inner(&dot, &tmp.path().join("nope"), &tmp.path().join("nada"));
        assert_eq!(notes.len(), 3, "every binary reports a note");
        assert!(notes.iter().all(|n| n.contains("not found")));
        assert_eq!(
            std::fs::read_dir(dot.join("bin")).expect("dir").count(),
            0,
            "bin/ stays empty, init never fails"
        );
    }

    #[test]
    fn lock_roundtrip_preserves_fields() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let dot = tmp.path().join(".touring");
        std::fs::create_dir_all(&dot).expect("mkdir");
        let lock = ToolchainLock {
            active: "vB".into(),
            previous: Some("vA".into()),
            updated_at: 1_753_000_000,
            reason: "update --channel vB".into(),
        };
        lock.write(&dot).expect("write lock");
        let read = ToolchainLock::read(&dot).expect("read lock");
        assert_eq!(read, lock);
        // No stray tmp file left behind (atomic write).
        assert!(!dot.join(format!("{LOCK_FILE}.tmp")).exists());
    }

    #[test]
    fn lock_read_is_none_when_absent_or_malformed() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let dot = tmp.path().join(".touring");
        std::fs::create_dir_all(&dot).expect("mkdir");
        assert!(ToolchainLock::read(&dot).is_none(), "absent → None");
        std::fs::write(dot.join(LOCK_FILE), "not [ valid toml").expect("write");
        assert!(ToolchainLock::read(&dot).is_none(), "malformed → None");
    }

    #[test]
    fn active_channel_prefers_lock_over_toml_pin() {
        // The core F3 invariant: after `update --channel vB`, every consumer
        // (init-project re-run, component add, next update) sees vB even
        // though the human's touring.toml still requests vA.
        let tmp = tempfile::tempdir().expect("tmpdir");
        let dot = tmp.path().join(".touring");
        std::fs::create_dir_all(&dot).expect("mkdir");
        std::fs::write(dot.join("touring.toml"), "[toolchain]\nchannel = \"vA\"\n")
            .expect("write toml");
        assert_eq!(resolve_active_channel(&dot).as_deref(), Some("vA"));
        ToolchainLock {
            active: "vB".into(),
            previous: Some("vA".into()),
            updated_at: 1,
            reason: "test".into(),
        }
        .write(&dot)
        .expect("write lock");
        assert_eq!(
            resolve_active_channel(&dot).as_deref(),
            Some("vB"),
            "lock must win over the toml pin"
        );
    }

    #[test]
    fn resolve_binary_target_dev_channel_skips_toolchain_dir() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let th = tmp.path().join("touring-home");
        // Even with a toolchain literally named "dev" installed, the dev
        // pseudo-channel means "use the dev bin dir".
        fake_bin(&th.join("toolchains/dev/bin"), "touring");
        let dev = tmp.path().join("dev-bin");
        let dev_touring = fake_bin(&dev, "touring");
        let got = resolve_binary_target("touring", Some(DEV_CHANNEL), &th, &dev);
        assert_eq!(got, Some(dev_touring));
    }
}
