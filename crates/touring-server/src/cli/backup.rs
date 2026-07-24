//! `touring backup` / `touring restore` — SQLite database backup and restore.
//!
//! # Usage
//!
//! ```text
//! touring backup knowledge [--output <path>]
//! touring backup memory   [--output <path>]
//! touring backup graph    [--output <path>]
//! touring backup all      [--output <dir>]
//! touring restore knowledge --input <path>
//! touring restore memory   --input <path>
//! touring restore graph    --input <path>
//! ```
//!
//! Backup uses SQLite's `VACUUM INTO` for a clean, WAL-checkpointed snapshot.
//! Restore copies the backup file back after validating the SQLite magic header.
//!
//! # Project root resolution
//!
//! 1. `TOURING_PROJECT_ROOT` environment variable (if set)
//! 2. Walk up from `CWD` until `.claude/touring/` is found

use std::path::{Path, PathBuf};

use super::common::{flag_value, parse_global_flags};

// ── Constants ────────────────────────────────────────────────────────────────

const DOMAINS: &[&str] = &["knowledge", "memory", "graph"];

/// First 16 bytes of every valid SQLite 3 database file.
const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\x00";

// ── Project root helpers ─────────────────────────────────────────────────────

/// Locate the `.claude/touring/` directory for the active project.
///
/// Resolution order:
/// 1. `TOURING_PROJECT_ROOT` env var
/// 2. Walk up from CWD
fn find_db_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TOURING_PROJECT_ROOT") {
        return Some(PathBuf::from(p).join(".claude").join("touring"));
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join(".claude").join("touring");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn db_path(db_dir: &Path, domain: &str) -> PathBuf {
    db_dir.join(format!("{domain}.db"))
}

// ── SQLite helpers ───────────────────────────────────────────────────────────

/// Return `true` if `path` begins with the SQLite format-3 magic header.
fn is_valid_sqlite(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 16];
    f.read_exact(&mut buf).is_ok() && &buf == SQLITE_MAGIC
}

/// Escape a path for embedding in a `VACUUM INTO '<path>'` SQL literal.
/// Single quotes are doubled per SQL standard escaping rules.
fn sqlite_escape_path(p: &Path) -> String {
    p.to_string_lossy().replace('\'', "''")
}

// ── Core backup / restore ────────────────────────────────────────────────────

/// Backup a single database file using `VACUUM INTO`.
///
/// - Skips (with a warning) if `src` does not exist.
/// - Returns an error if `dst` already exists (prevents silent overwrites).
fn backup_one(src: &Path, dst: &Path, json: bool) -> anyhow::Result<()> {
    if !src.exists() {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "status": "skipped",
                    "src": src.to_string_lossy(),
                    "reason": "source not found"
                })
            );
        } else {
            eprintln!("skip: {} not found", src.display());
        }
        return Ok(());
    }

    if dst.exists() {
        anyhow::bail!(
            "destination {} already exists — remove it or choose a different path",
            dst.display()
        );
    }

    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = rusqlite::Connection::open(src)
        .map_err(|e| anyhow::anyhow!("cannot open {}: {e}", src.display()))?;
    let escaped = sqlite_escape_path(dst);
    conn.execute_batch(&format!("VACUUM INTO '{escaped}'"))
        .map_err(|e| anyhow::anyhow!("VACUUM INTO failed for {}: {e}", src.display()))?;

    let size = dst.metadata().map(|m| m.len()).unwrap_or(0);
    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "src": src.to_string_lossy(),
                "dst": dst.to_string_lossy(),
                "size_bytes": size,
            })
        );
    } else {
        println!(
            "backed up {} → {} ({size} bytes)",
            src.display(),
            dst.display()
        );
    }
    Ok(())
}

/// Restore a domain database from a backup file.
///
/// Validates the source is a valid SQLite database before overwriting.
fn restore_one(src: &Path, dst: &Path, domain: &str, json: bool) -> anyhow::Result<()> {
    if !src.exists() {
        anyhow::bail!("input file {} not found", src.display());
    }
    if !is_valid_sqlite(src) {
        anyhow::bail!(
            "{} does not appear to be a valid SQLite database",
            src.display()
        );
    }

    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if dst.exists() && !json {
        eprintln!("warning: overwriting live database {}", dst.display());
    }

    let bytes = std::fs::copy(src, dst).map_err(|e| anyhow::anyhow!("copy failed: {e}"))?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "domain": domain,
                "src": src.to_string_lossy(),
                "dst": dst.to_string_lossy(),
                "bytes_copied": bytes,
            })
        );
    } else {
        println!(
            "restored {domain}: {} → {} ({bytes} bytes)",
            src.display(),
            dst.display()
        );
    }
    Ok(())
}

// ── Subcommand handlers ──────────────────────────────────────────────────────

fn cmd_backup(args: &[String]) -> anyhow::Result<()> {
    let (flags, args) = parse_global_flags(args);
    let subcommand = args.get(2).map(|s| s.as_str()).unwrap_or("help");

    let db_dir = find_db_dir().ok_or_else(|| {
        anyhow::anyhow!(
            "could not locate .claude/touring/ — run from a Touring project root \
             or set TOURING_PROJECT_ROOT"
        )
    })?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();

    match subcommand {
        "knowledge" | "memory" | "graph" => {
            let src = db_path(&db_dir, subcommand);
            let default_name = format!("{subcommand}_{timestamp}.db");
            let dst_str = flag_value(&args, "--output").unwrap_or(default_name.as_str());
            backup_one(&src, &PathBuf::from(dst_str), flags.json)?;
        }

        "all" => {
            let default_dir = format!("touring_backup_{timestamp}");
            let out_dir_str = flag_value(&args, "--output").unwrap_or(default_dir.as_str());
            let out_dir = PathBuf::from(out_dir_str);
            std::fs::create_dir_all(&out_dir)?;

            let mut any_err = false;
            for &domain in DOMAINS {
                let src = db_path(&db_dir, domain);
                let dst = out_dir.join(format!("{domain}.db"));
                if let Err(e) = backup_one(&src, &dst, flags.json) {
                    eprintln!("error backing up {domain}: {e}");
                    any_err = true;
                }
            }
            if !flags.json {
                println!("backup complete: {}", out_dir.display());
            }
            if any_err {
                anyhow::bail!("one or more domains failed to back up");
            }
        }

        _ => {
            eprintln!("Usage:");
            eprintln!("  touring backup knowledge [--output <path>]");
            eprintln!("  touring backup memory   [--output <path>]");
            eprintln!("  touring backup graph    [--output <path>]");
            eprintln!("  touring backup all      [--output <dir>]");
        }
    }
    Ok(())
}

fn cmd_restore(args: &[String]) -> anyhow::Result<()> {
    let (flags, args) = parse_global_flags(args);
    let domain = args.get(2).map(|s| s.as_str()).unwrap_or("");

    if !DOMAINS.contains(&domain) {
        anyhow::bail!("unknown domain '{domain}' — must be one of: knowledge, memory, graph");
    }

    let input_str = flag_value(&args, "--input")
        .ok_or_else(|| anyhow::anyhow!("--input <path> is required for restore"))?;
    let src = PathBuf::from(input_str);

    let db_dir = find_db_dir().ok_or_else(|| {
        anyhow::anyhow!(
            "could not locate .claude/touring/ — run from a Touring project root \
             or set TOURING_PROJECT_ROOT"
        )
    })?;
    let dst = db_path(&db_dir, domain);

    restore_one(&src, &dst, domain, flags.json)
}

// ── Entry point ──────────────────────────────────────────────────────────────

/// Dispatch `touring backup` and `touring restore` subcommands.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    match args.get(1).map(|s| s.as_str()) {
        Some("backup") => cmd_backup(args),
        Some("restore") => cmd_restore(args),
        _ => {
            eprintln!("Usage:");
            eprintln!("  touring backup <domain|all> [--output <path>]");
            eprintln!("  touring restore <domain> --input <path>");
            Ok(())
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_test_db(path: &Path) {
        let conn = rusqlite::Connection::open(path).expect("open test db");
        conn.execute_batch(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, val TEXT); \
             INSERT INTO t VALUES (1, 'hello');",
        )
        .expect("create test table");
    }

    #[test]
    fn backup_one_creates_valid_copy() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("src.db");
        let dst = dir.path().join("dst.db");
        make_test_db(&src);

        backup_one(&src, &dst, false).expect("backup");

        assert!(dst.exists(), "destination must exist after backup");
        assert!(is_valid_sqlite(&dst), "destination must be valid SQLite");
    }

    #[test]
    fn backup_one_data_is_readable_after_copy() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("src.db");
        let dst = dir.path().join("dst.db");
        make_test_db(&src);
        backup_one(&src, &dst, false).expect("backup");

        let conn = rusqlite::Connection::open(&dst).expect("open dst");
        let val: String = conn
            .query_row("SELECT val FROM t WHERE id=1", [], |r| r.get(0))
            .expect("query");
        assert_eq!(val, "hello");
    }

    #[test]
    fn backup_one_skips_missing_src() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("nonexistent.db");
        let dst = dir.path().join("dst.db");

        // Should not error — missing source is a skip, not a failure
        backup_one(&src, &dst, false).expect("should not error on missing src");
        assert!(!dst.exists(), "dst must not be created for missing src");
    }

    #[test]
    fn backup_one_fails_if_dst_exists() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("src.db");
        let dst = dir.path().join("dst.db");
        make_test_db(&src);
        std::fs::write(&dst, b"existing content").expect("create dst");

        assert!(
            backup_one(&src, &dst, false).is_err(),
            "must error if destination exists"
        );
    }

    #[test]
    fn is_valid_sqlite_accepts_real_db() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("real.db");
        make_test_db(&p);
        assert!(is_valid_sqlite(&p));
    }

    #[test]
    fn is_valid_sqlite_rejects_garbage() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("garbage.db");
        std::fs::write(&p, b"not a database at all").expect("write");
        assert!(!is_valid_sqlite(&p));
    }

    #[test]
    fn is_valid_sqlite_rejects_missing_file() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("missing.db");
        assert!(!is_valid_sqlite(&p));
    }

    #[test]
    fn sqlite_escape_path_doubles_single_quotes() {
        let p = Path::new("/tmp/it's a path/db.db");
        assert_eq!(sqlite_escape_path(p), "/tmp/it''s a path/db.db");
    }

    #[test]
    fn sqlite_escape_path_no_change_for_clean_path() {
        let p = Path::new("/tmp/touring/knowledge.db");
        assert_eq!(sqlite_escape_path(p), "/tmp/touring/knowledge.db");
    }

    #[test]
    fn restore_one_rejects_non_sqlite() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("bad.db");
        let dst = dir.path().join("dst.db");
        std::fs::write(&src, b"definitely not sqlite").expect("write");

        let result = restore_one(&src, &dst, "knowledge", false);
        assert!(result.is_err(), "restore must reject non-SQLite input");
        assert!(result.unwrap_err().to_string().contains("valid SQLite"));
    }

    #[test]
    fn restore_one_succeeds_for_valid_db() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("backup.db");
        let dst = dir.path().join("live.db");
        make_test_db(&src);

        restore_one(&src, &dst, "knowledge", false).expect("restore");
        assert!(dst.exists());
        assert!(is_valid_sqlite(&dst));
    }

    #[test]
    fn find_db_dir_uses_env_var() {
        let dir = tempdir().expect("tempdir");
        let db_dir = dir.path().join(".claude").join("touring");
        std::fs::create_dir_all(&db_dir).expect("create dir");

        // SAFETY: test is single-threaded; no concurrent env access.
        unsafe { std::env::set_var("TOURING_PROJECT_ROOT", dir.path()) };
        let found = find_db_dir();
        unsafe { std::env::remove_var("TOURING_PROJECT_ROOT") };

        assert_eq!(found, Some(db_dir));
    }
}
