//! `touring migrate-from-global` — W12.7 (2026-05-23) — global → per-project migration.
//!
//! Copies the user's `~/.claude/touring/` DBs into the per-project
//! `.touring/data/` tree created by `touring init-project` (W12.1).
//!
//! This is the **migration path** from the historical global-only layout
//! into the rustup-pattern per-project layout. After running it, the project
//! daemon (W12.5) will read its own copies of `symbols.db`, `knowledge.db`,
//! `memory.db`, `graph.db` — isolated from other projects.
//!
//! Subcommand (NEW; distinct from existing `touring migrate` which is
//! DB-schema consolidation):
//!
//! ```text
//! touring migrate-from-global [--from <DIR>] [--to <DIR>] [--dry-run] [--force]
//! ```
//!
//! Flags:
//! - `--from <DIR>`   — Source dir (default `~/.claude/touring/`)
//! - `--to <DIR>`     — Destination dir (default `<CWD>/.touring/data/`)
//! - `--dry-run`      — List files that would be copied, mutate nothing
//! - `--force`        — Overwrite existing files in `--to` (default: backup `.bak.<ts>`)
//!
//! Note: This is byte-for-byte file copy; **DB content filtering** (per-project
//! subset extraction) is a future enhancement when the schemas expose a project
//! tag column. The current implementation is correct for the common case where
//! the user has been running one project under the global layout and now wants
//! to migrate it.

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// DB filenames migrated. Extend this list when new global DBs are added.
const MIGRATE_FILES: &[&str] = &[
    "symbols.db",
    "knowledge.db",
    "memory.db",
    "graph.db",
    "semantic_recall.db",
    "rlm_memory.db",
    "ann_memory.db",
    "touring_knowledge.db",
    "touring_pipeline.db",
    "got_snapshots.db",
];

/// Parsed args.
#[derive(Debug, Clone, Default)]
pub struct MigrateArgs {
    /// Source directory override (`--from <DIR>`); `None` uses `~/.claude/touring/`.
    pub from: Option<PathBuf>,
    /// Destination directory override (`--to <DIR>`); `None` uses `<CWD>/.touring/data/`.
    pub to: Option<PathBuf>,
    /// `--dry-run`/`-n`: list files that would be copied without mutating anything.
    pub dry_run: bool,
    /// `--force`/`-f`: overwrite existing destination files instead of backing them up.
    pub force: bool,
}

impl MigrateArgs {
    /// Parses `touring migrate-from-global` argv (skipping `argv[0]` and `argv[1]`),
    /// recognizing `--dry-run`/`-n`, `--force`/`-f`, and `--from`/`--to` in both
    /// space-separated and `=`-joined forms. Unknown flags are ignored (fail-open).
    pub fn parse(args: &[String]) -> Self {
        let mut out = Self::default();
        let mut i = 2; // skip argv[0]=touring, argv[1]=migrate-from-global
        while i < args.len() {
            let a = &args[i];
            match a.as_str() {
                "--dry-run" | "-n" => out.dry_run = true,
                "--force" | "-f" => out.force = true,
                "--from" => {
                    if let Some(v) = args.get(i + 1) {
                        out.from = Some(PathBuf::from(v));
                        i += 1;
                    }
                }
                "--to" => {
                    if let Some(v) = args.get(i + 1) {
                        out.to = Some(PathBuf::from(v));
                        i += 1;
                    }
                }
                other if other.starts_with("--from=") => {
                    out.from = Some(PathBuf::from(other.trim_start_matches("--from=")));
                }
                other if other.starts_with("--to=") => {
                    out.to = Some(PathBuf::from(other.trim_start_matches("--to=")));
                }
                _ => { /* ignore unknown — fail-open */ }
            }
            i += 1;
        }
        out
    }
}

/// Resolve default source: `$HOME/.claude/touring`.
fn default_source() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".claude").join("touring")
}

/// Resolve default destination: `<CWD>/.touring/data/`.
fn default_destination() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    cwd.join(".touring").join("data")
}

/// Outcome of a single migration pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    /// Resolved source directory the DBs were read from.
    pub source: PathBuf,
    /// Resolved destination directory the DBs were copied into.
    pub destination: PathBuf,
    /// Files that were (or would be, in dry-run) copied.
    pub copied: Vec<String>,
    /// Files in MIGRATE_FILES that don't exist in source — silently skipped.
    pub skipped_missing: Vec<String>,
    /// Files that already existed in dest and were backed up to `.bak.<ts>`.
    /// Empty in --force mode (which overwrites without backup).
    pub backed_up: Vec<String>,
    /// Whether this was a dry run (no files actually copied/backed up).
    pub dry_run: bool,
}

/// CLI dispatch.
pub fn run(args: &[String]) -> Result<()> {
    let parsed = MigrateArgs::parse(args);
    let src = parsed.from.clone().unwrap_or_else(default_source);
    let dst = parsed.to.clone().unwrap_or_else(default_destination);
    let report = migrate_from_global_in(&src, &dst, parsed.dry_run, parsed.force)?;
    print_report(&report);
    Ok(())
}

/// Pure-IO migration core — testable.
///
/// Returns a `MigrationReport`. In `dry_run=true` mode, no files are written
/// (but `MIGRATE_FILES` are still inspected for the report).
pub fn migrate_from_global_in(
    source: &Path,
    destination: &Path,
    dry_run: bool,
    force: bool,
) -> Result<MigrationReport> {
    if !source.exists() {
        return Err(anyhow!(
            "source {} does not exist — nothing to migrate",
            source.display()
        ));
    }
    if !source.is_dir() {
        return Err(anyhow!("source {} is not a directory", source.display()));
    }

    if !dry_run {
        std::fs::create_dir_all(destination)
            .map_err(|e| anyhow!("create_dir_all {}: {e}", destination.display()))?;
    }

    let ts_suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut copied = Vec::new();
    let mut skipped_missing = Vec::new();
    let mut backed_up = Vec::new();

    for name in MIGRATE_FILES {
        let src_file = source.join(name);
        if !src_file.is_file() {
            skipped_missing.push((*name).to_string());
            continue;
        }
        let dst_file = destination.join(name);
        if dst_file.exists() {
            if force {
                if !dry_run {
                    std::fs::remove_file(&dst_file)
                        .map_err(|e| anyhow!("remove existing {}: {e}", dst_file.display()))?;
                }
                // No backup recorded; --force overwrote.
            } else {
                let bak = destination.join(format!("{name}.bak.{ts_suffix}"));
                if !dry_run {
                    std::fs::rename(&dst_file, &bak).map_err(|e| {
                        anyhow!(
                            "backup rename {} → {}: {e}",
                            dst_file.display(),
                            bak.display()
                        )
                    })?;
                }
                backed_up.push(format!(
                    "{name} → {}",
                    bak.file_name()
                        .expect("backup path has a file name")
                        .to_string_lossy()
                ));
            }
        }
        if !dry_run {
            std::fs::copy(&src_file, &dst_file).map_err(|e| {
                anyhow!("copy {} → {}: {e}", src_file.display(), dst_file.display())
            })?;
        }
        copied.push((*name).to_string());
    }

    Ok(MigrationReport {
        source: source.to_path_buf(),
        destination: destination.to_path_buf(),
        copied,
        skipped_missing,
        backed_up,
        dry_run,
    })
}

fn print_report(r: &MigrationReport) {
    let prefix = if r.dry_run { "[DRY-RUN] " } else { "" };
    println!(
        "{}touring migrate-from-global: {} → {}",
        prefix,
        r.source.display(),
        r.destination.display()
    );
    if r.copied.is_empty() {
        println!("  (no files matched MIGRATE_FILES in source)");
    } else {
        println!("  Copied ({}):", r.copied.len());
        for f in &r.copied {
            println!(
                "    {} {}",
                if r.dry_run { "would copy" } else { "copied" },
                f
            );
        }
    }
    if !r.backed_up.is_empty() {
        println!("  Backed up ({}):", r.backed_up.len());
        for f in &r.backed_up {
            println!("    {}", f);
        }
    }
    if !r.skipped_missing.is_empty() {
        println!(
            "  Skipped (not in source): {}",
            r.skipped_missing.join(", ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(extras: &[&str]) -> Vec<String> {
        let mut v = vec!["touring".to_string(), "migrate-from-global".to_string()];
        v.extend(extras.iter().map(|s| s.to_string()));
        v
    }

    #[test]
    fn parse_defaults() {
        let p = MigrateArgs::parse(&args(&[]));
        assert!(!p.dry_run);
        assert!(!p.force);
        assert!(p.from.is_none());
        assert!(p.to.is_none());
    }

    #[test]
    fn parse_dry_run_and_force_short_long() {
        let p = MigrateArgs::parse(&args(&["--dry-run", "--force"]));
        assert!(p.dry_run);
        assert!(p.force);
        let q = MigrateArgs::parse(&args(&["-n", "-f"]));
        assert!(q.dry_run);
        assert!(q.force);
    }

    #[test]
    fn parse_from_to_separate_and_equals() {
        let p = MigrateArgs::parse(&args(&["--from", "/a", "--to", "/b"]));
        assert_eq!(p.from, Some(PathBuf::from("/a")));
        assert_eq!(p.to, Some(PathBuf::from("/b")));
        let q = MigrateArgs::parse(&args(&["--from=/x", "--to=/y"]));
        assert_eq!(q.from, Some(PathBuf::from("/x")));
        assert_eq!(q.to, Some(PathBuf::from("/y")));
    }

    fn make_source_with(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmpdir");
        for (name, content) in files {
            let p = dir.path().join(name);
            std::fs::write(&p, content).unwrap();
        }
        dir
    }

    #[test]
    fn migrate_copies_only_known_db_files() {
        let src = make_source_with(&[
            ("symbols.db", b"S"),
            ("memory.db", b"M"),
            ("not_a_db.txt", b"ignored"),
        ]);
        let dst_root = tempfile::tempdir().expect("dst");
        let dst = dst_root.path().join("data");
        let report = migrate_from_global_in(src.path(), &dst, false, false).expect("migrate");
        assert!(report.copied.contains(&"symbols.db".to_string()));
        assert!(report.copied.contains(&"memory.db".to_string()));
        // not_a_db.txt is NOT in MIGRATE_FILES — must not appear in copied
        assert!(!report.copied.iter().any(|s| s == "not_a_db.txt"));
        // Verify the file actually exists in dst
        assert!(dst.join("symbols.db").is_file());
        assert!(dst.join("memory.db").is_file());
        assert!(!dst.join("not_a_db.txt").exists());
    }

    #[test]
    fn migrate_marks_missing_files_as_skipped() {
        let src = make_source_with(&[("symbols.db", b"S")]);
        let dst_root = tempfile::tempdir().expect("dst");
        let dst = dst_root.path().join("data");
        let report = migrate_from_global_in(src.path(), &dst, false, false).expect("migrate");
        assert_eq!(report.copied, vec!["symbols.db"]);
        // The 9 other MIGRATE_FILES are missing → all skipped
        assert_eq!(report.skipped_missing.len(), MIGRATE_FILES.len() - 1);
        assert!(report.skipped_missing.contains(&"memory.db".to_string()));
    }

    #[test]
    fn migrate_dry_run_writes_nothing() {
        let src = make_source_with(&[("symbols.db", b"S"), ("memory.db", b"M")]);
        let dst_root = tempfile::tempdir().expect("dst");
        let dst = dst_root.path().join("data");
        let report = migrate_from_global_in(src.path(), &dst, true, false).expect("dry-run");
        assert!(report.dry_run);
        assert_eq!(report.copied.len(), 2);
        // dst MUST NOT have been created
        assert!(!dst.exists(), "dst should not be created in dry-run");
    }

    #[test]
    fn migrate_backs_up_existing_without_force() {
        let src = make_source_with(&[("symbols.db", b"NEW")]);
        let dst_root = tempfile::tempdir().expect("dst");
        let dst = dst_root.path().join("data");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(dst.join("symbols.db"), b"OLD").unwrap();

        let report = migrate_from_global_in(src.path(), &dst, false, false).expect("migrate");
        assert_eq!(report.copied, vec!["symbols.db"]);
        assert_eq!(report.backed_up.len(), 1);
        // dest now has NEW content
        assert_eq!(std::fs::read(dst.join("symbols.db")).unwrap(), b"NEW");
        // Backup file exists with OLD content
        let bak_entries: Vec<_> = std::fs::read_dir(&dst)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("symbols.db.bak."))
            .collect();
        assert_eq!(bak_entries.len(), 1, "expected 1 backup file");
    }

    #[test]
    fn migrate_force_overwrites_without_backup() {
        let src = make_source_with(&[("symbols.db", b"NEW")]);
        let dst_root = tempfile::tempdir().expect("dst");
        let dst = dst_root.path().join("data");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(dst.join("symbols.db"), b"OLD").unwrap();

        let report = migrate_from_global_in(src.path(), &dst, false, true).expect("migrate force");
        assert_eq!(report.copied, vec!["symbols.db"]);
        assert!(report.backed_up.is_empty(), "no backup in force mode");
        assert_eq!(std::fs::read(dst.join("symbols.db")).unwrap(), b"NEW");
        // No .bak.* entries exist
        let bak_entries: Vec<_> = std::fs::read_dir(&dst)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("symbols.db.bak."))
            .collect();
        assert!(bak_entries.is_empty());
    }

    #[test]
    fn migrate_errors_when_source_does_not_exist() {
        let dst_root = tempfile::tempdir().expect("dst");
        let dst = dst_root.path().join("data");
        let nonexistent = PathBuf::from("/tmp/does-not-exist-xyz-touring-migrate-test");
        let err = migrate_from_global_in(&nonexistent, &dst, false, false).expect_err("must fail");
        assert!(format!("{err}").contains("does not exist"));
    }

    #[test]
    fn migrate_preserves_byte_content() {
        let payload: &[u8] = b"\x00\x01\x02BINARY-PAYLOAD\xff\xfe\xfd";
        let src = make_source_with(&[("memory.db", payload)]);
        let dst_root = tempfile::tempdir().expect("dst");
        let dst = dst_root.path().join("data");
        migrate_from_global_in(src.path(), &dst, false, false).expect("migrate");
        let copied = std::fs::read(dst.join("memory.db")).unwrap();
        assert_eq!(copied.as_slice(), payload);
    }
}
