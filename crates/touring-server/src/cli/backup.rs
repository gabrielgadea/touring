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

// ── Conditional compaction (A7b, 2026-08-07) ─────────────────────────────────

/// Share of dead pages above which compaction is worth its cost.
const COMPACT_MIN_DEAD_RATIO: f64 = 0.25;

/// Bytes reclaimable below which compaction is not worth its cost.
///
/// Gortex uses 1 GiB against a 6,8 GB store; the Touring DBs are two orders of
/// magnitude smaller (`symbols.db` ≈ 186 MB), so the absolute floor is scaled
/// to 16 MiB. Both thresholds must hold — a high ratio over a tiny file buys
/// nothing, and a large absolute figure inside a healthy file is normal slack.
const COMPACT_MIN_RECLAIMABLE_BYTES: u64 = 16 * 1024 * 1024;

/// Free-list census of one SQLite file.
#[derive(Debug, Clone, Copy)]
struct DbSpace {
    page_count: u64,
    freelist_count: u64,
    page_size: u64,
}

impl DbSpace {
    fn reclaimable_bytes(self) -> u64 {
        self.freelist_count.saturating_mul(self.page_size)
    }

    fn dead_ratio(self) -> f64 {
        if self.page_count == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)] // page counts stay well inside f64
        {
            self.freelist_count as f64 / self.page_count as f64
        }
    }

    /// Both thresholds must hold — see the constants for why either alone lies.
    fn worth_compacting(self) -> bool {
        self.dead_ratio() > COMPACT_MIN_DEAD_RATIO
            && self.reclaimable_bytes() > COMPACT_MIN_RECLAIMABLE_BYTES
    }
}

fn read_db_space(path: &Path) -> anyhow::Result<DbSpace> {
    let conn = rusqlite::Connection::open(path)
        .map_err(|e| anyhow::anyhow!("cannot open {}: {e}", path.display()))?;
    let pragma = |name: &str| -> anyhow::Result<u64> {
        conn.query_row(&format!("PRAGMA {name}"), [], |r| r.get::<_, i64>(0))
            .map(|v| u64::try_from(v).unwrap_or(0))
            .map_err(|e| anyhow::anyhow!("PRAGMA {name} failed on {}: {e}", path.display()))
    };
    Ok(DbSpace {
        page_count: pragma("page_count")?,
        freelist_count: pragma("freelist_count")?,
        page_size: pragma("page_size")?,
    })
}

/// `touring compact [<domain>|all] [--force] [--dry-run]`.
///
/// A7b: `VACUUM` rewrites the whole file, so running it unconditionally on
/// every boot is expensive and running it never lets dead pages accumulate
/// silently — which is what happened to `wiring_map`, whose phantom rows had to
/// be purged by hand. Two explicit thresholds decide, and the decision plus the
/// numbers behind it are always reported, including when the answer is "no".
fn cmd_compact(args: &[String]) -> anyhow::Result<()> {
    let (flags, args) = parse_global_flags(args);
    let target = args.get(2).map(|s| s.as_str()).unwrap_or("all");
    let force = args.iter().any(|a| a == "--force");
    let dry_run = args.iter().any(|a| a == "--dry-run");

    let db_dir = find_db_dir().ok_or_else(|| {
        anyhow::anyhow!(
            "could not locate .claude/touring/ — run from a Touring project root \
             or set TOURING_PROJECT_ROOT"
        )
    })?;
    let domains: Vec<&str> = if target == "all" {
        DOMAINS.to_vec()
    } else if DOMAINS.contains(&target) {
        vec![target]
    } else {
        anyhow::bail!("unknown domain '{target}' — must be one of: knowledge, memory, graph, all");
    };

    let mut results = Vec::with_capacity(domains.len());
    for domain in domains {
        let path = db_path(&db_dir, domain);
        if !path.exists() {
            results.push(serde_json::json!({
                "domain": domain, "status": "skipped", "reason": "not found"
            }));
            continue;
        }
        let space = read_db_space(&path)?;
        let before = path.metadata().map(|m| m.len()).unwrap_or(0);
        let eligible = space.worth_compacting();
        if !eligible && !force {
            results.push(serde_json::json!({
                "domain": domain,
                "status": "below_threshold",
                "dead_ratio": space.dead_ratio(),
                "reclaimable_bytes": space.reclaimable_bytes(),
                "size_bytes": before,
            }));
            continue;
        }
        if dry_run {
            results.push(serde_json::json!({
                "domain": domain,
                "status": "would_compact",
                "dead_ratio": space.dead_ratio(),
                "reclaimable_bytes": space.reclaimable_bytes(),
                "size_bytes": before,
                "forced": force && !eligible,
            }));
            continue;
        }
        let conn = rusqlite::Connection::open(&path)
            .map_err(|e| anyhow::anyhow!("cannot open {}: {e}", path.display()))?;
        conn.execute_batch("VACUUM")
            .map_err(|e| anyhow::anyhow!("VACUUM failed for {}: {e}", path.display()))?;
        drop(conn);
        let after = path.metadata().map(|m| m.len()).unwrap_or(before);
        results.push(serde_json::json!({
            "domain": domain,
            "status": "compacted",
            "dead_ratio_before": space.dead_ratio(),
            "size_bytes_before": before,
            "size_bytes_after": after,
            "freed_bytes": before.saturating_sub(after),
            "forced": force && !eligible,
        }));
    }

    if flags.json {
        println!("{}", serde_json::json!({ "results": results }));
    } else {
        for r in &results {
            println!("{r}");
        }
    }
    Ok(())
}

// ── Entry point ──────────────────────────────────────────────────────────────

/// Dispatch `touring backup`, `touring restore` and `touring compact`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    match args.get(1).map(|s| s.as_str()) {
        Some("backup") => cmd_backup(args),
        Some("restore") => cmd_restore(args),
        Some("compact") => cmd_compact(args),
        _ => {
            eprintln!("Usage:");
            eprintln!("  touring backup <domain|all> [--output <path>]");
            eprintln!("  touring restore <domain> --input <path>");
            eprintln!("  touring compact [<domain>|all] [--force] [--dry-run]");
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

/// A7b (2026-08-07) — compaction runs on evidence, not on a schedule.
#[cfg(test)]
mod compaction_threshold_tests {
    use super::{COMPACT_MIN_RECLAIMABLE_BYTES, DbSpace, read_db_space};
    use tempfile::tempdir;

    fn space(page_count: u64, freelist_count: u64, page_size: u64) -> DbSpace {
        DbSpace {
            page_count,
            freelist_count,
            page_size,
        }
    }

    #[test]
    fn both_thresholds_must_hold() {
        let page = 4096;
        let big_enough = COMPACT_MIN_RECLAIMABLE_BYTES / page + 1;

        // Half the pages dead AND well past the byte floor → compact.
        assert!(space(big_enough * 2, big_enough, page).worth_compacting());

        // A tiny file that is 90% free list: high ratio, nothing to reclaim.
        // Vacuuming it would cost a full rewrite to save kilobytes.
        assert!(
            !space(100, 90, page).worth_compacting(),
            "ratio alone must not trigger a full rewrite"
        );

        // A large file with a large absolute free list that is still a small
        // fraction of it — ordinary slack in a healthy DB.
        assert!(
            !space(big_enough * 100, big_enough + 1, page).worth_compacting(),
            "absolute bytes alone must not trigger"
        );
    }

    #[test]
    fn an_empty_db_has_no_dead_ratio_and_never_divides_by_zero() {
        let s = space(0, 0, 4096);
        assert!((s.dead_ratio() - 0.0).abs() < f64::EPSILON);
        assert!(!s.worth_compacting());
        assert_eq!(s.reclaimable_bytes(), 0);
    }

    #[test]
    fn reclaimable_bytes_is_freelist_times_page_size() {
        assert_eq!(space(1000, 250, 4096).reclaimable_bytes(), 250 * 4096);
        assert!((space(1000, 250, 4096).dead_ratio() - 0.25).abs() < 1e-9);
    }

    #[test]
    fn a_real_sqlite_file_reports_its_page_census() {
        // Reads the same PRAGMAs the command reads, against a real file — the
        // thresholds are only meaningful if the census behind them is real.
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("census.db");
        let conn = rusqlite::Connection::open(&path).expect("open");
        conn.execute_batch(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, blob TEXT);
             INSERT INTO t (blob) SELECT hex(randomblob(400))
               FROM (WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM c LIMIT 500) SELECT i FROM c);",
        )
        .expect("seed");
        drop(conn);

        let before = read_db_space(&path).expect("census");
        assert!(before.page_count > 0, "a seeded db has pages");
        assert!(before.page_size >= 512, "page_size is a real pragma value");

        // Deleting the rows moves pages onto the free list — the exact signal
        // the thresholds read.
        let conn = rusqlite::Connection::open(&path).expect("reopen");
        conn.execute_batch("DELETE FROM t;").expect("delete");
        drop(conn);
        let after = read_db_space(&path).expect("census");
        assert!(
            after.freelist_count > before.freelist_count,
            "deleted rows must show up as dead pages ({} -> {})",
            before.freelist_count,
            after.freelist_count
        );
    }
}
