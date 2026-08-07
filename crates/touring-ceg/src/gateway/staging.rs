//! P5.1 — managed staging area for transient code bodies.
//!
//! CEG Pln2 Phase **P5** (`docs/2026-05-17-ceg-pln2-plan.md`). Replaces the
//! ad-hoc use of `/tmp` for code bodies the gateway must materialise on disk
//! before X5 SANDBOX or X8 SUPERVISED-EXEC can run them.
//!
//! # Why a managed area
//!
//! A code body routed through the `X0..X9` pipeline has to live *somewhere*
//! on disk for the sandbox or supervised runner to invoke it. Writing it to
//! `/tmp` severs the thread: the gateway computes a verdict for "this body",
//! but the file on disk carries no record of which session produced it or of
//! what the gateway already decided. A later invocation of the same path
//! cannot recover that verdict and is forced to re-analyse from scratch
//! (CEG Pln2 risk R9 — heredoc temporal-split).
//!
//! The staging area fixes this at the storage layer. Every transient body
//! gets a stable home under:
//!
//! ```text
//! ~/.claude/touring/staging/<session>/<file>
//! ```
//!
//! partitioned by session so concurrent sessions never collide, and made
//! resolvable by the P5.2 staging registry so a staged path can map back to
//! its origin and prior X2/X3 verdict.
//!
//! # Transient by construction
//!
//! The area is *not* permanent storage. [`gc_staging`] sweeps session
//! directories whose most-recent activity is older than a retention window,
//! so the tree never grows unbounded. The GC is the directory-level analogue
//! of [`cleanup_tee`] — and reuses it,
//! so a single `gc_staging` call bounds the whole `~/.claude/touring/`
//! transient family (staging tree + tee logs) at once.
//!
//! # Path safety
//!
//! Session ids and file names are sanitised to a single safe path component
//! before they touch the filesystem: a hostile id like `../../etc` cannot
//! escape the staging root.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::gateway::sandbox_executor::{cleanup_tee, tee_dir};

/// Default retention window for staged scripts — 24 hours. Shorter than the
/// 7-day tee-log window: a staged body is consumed within a session, whereas
/// a tee log is kept for cross-session failure retrospection.
pub const DEFAULT_STAGING_RETENTION_SECS: u64 = 24 * 60 * 60;

/// Resolves the staging root directory.
///
/// Honors the `TOURING_STAGING_DIR` env var (test determinism — escapes the
/// process-global HOME race in parallel tests). Otherwise the staging tree is
/// a sibling of the tee directory under `~/.claude/touring/`: this **reuses**
/// [`tee_dir`] so the HOME-or-`/tmp`
/// fallback logic lives in exactly one place.
pub fn staging_root() -> PathBuf {
    if let Ok(custom) = std::env::var("TOURING_STAGING_DIR")
        && !custom.is_empty()
    {
        return PathBuf::from(custom);
    }
    tee_dir()
        .parent()
        .map(|base| base.join("staging"))
        .unwrap_or_else(|| PathBuf::from("/tmp/touring-staging"))
}

/// The retention window (seconds) applied by [`gc_staging`], read from
/// `TOURING_STAGING_RETENTION_SECS` and defaulting to
/// [`DEFAULT_STAGING_RETENTION_SECS`].
pub fn staging_retention_secs() -> u64 {
    std::env::var("TOURING_STAGING_RETENTION_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_STAGING_RETENTION_SECS)
}

/// Reduces `raw` to a single, safe filesystem path component.
///
/// Every character outside `[A-Za-z0-9._-]` becomes `_`, so the result can
/// never contain a `/` or `\` separator. A component made only of dots
/// (`.`, `..`) — the path-traversal escape — is neutralised to `default`.
/// The result is length-capped to keep on-disk paths sane.
fn sanitize_component(raw: &str) -> String {
    let mapped: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = mapped.trim_matches('.');
    if trimmed.is_empty() {
        return "default".to_string();
    }
    trimmed.chars().take(120).collect()
}

/// The current session id, from `TOURING_SESSION_ID` / `CLAUDE_SESSION_ID`,
/// falling back to `default` when neither is set.
fn current_session_id() -> String {
    std::env::var("TOURING_SESSION_ID")
        .or_else(|_| std::env::var("CLAUDE_SESSION_ID"))
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string())
}

/// Resolves (without creating) the on-disk path a transient `file_name` would
/// occupy for `session`: `<staging_root>/<session>/<file_name>`.
///
/// Both `session` and `file_name` are sanitised — the returned path is always
/// strictly within the staging root.
pub fn stage_path(session: &str, file_name: &str) -> PathBuf {
    StagingArea::for_session(session).path_for(file_name)
}

/// A managed, session-partitioned home for transient code bodies.
///
/// Construct with [`StagingArea::for_session`] (process-global root),
/// [`StagingArea::current`] (the current session), or
/// [`StagingArea::with_root`] (an explicit root — embedding callers and
/// parallel-safe tests).
#[derive(Debug, Clone)]
pub struct StagingArea {
    root: PathBuf,
    session: String,
}

impl StagingArea {
    /// Binds a staging area to `session` under the process-global
    /// [`staging_root`]. The session id is sanitised on the way in.
    pub fn for_session(session: impl AsRef<str>) -> Self {
        Self {
            root: staging_root(),
            session: sanitize_component(session.as_ref()),
        }
    }

    /// Binds a staging area to the current session (`current_session_id`).
    pub fn current() -> Self {
        Self::for_session(current_session_id())
    }

    /// Binds a staging area to an explicit `root` instead of the
    /// process-global staging root. Used by embedding callers that manage
    /// their own staging tree and by tests that need parallel-safe isolation
    /// without mutating a process-global env var.
    pub fn with_root(root: impl Into<PathBuf>, session: impl AsRef<str>) -> Self {
        Self {
            root: root.into(),
            session: sanitize_component(session.as_ref()),
        }
    }

    /// The staging root this area is bound to.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The sanitised session id this area is bound to.
    pub fn session(&self) -> &str {
        &self.session
    }

    /// This area's session directory: `<root>/<session>/`.
    pub fn session_dir(&self) -> PathBuf {
        self.root.join(&self.session)
    }

    /// Resolves (without creating) the path `file_name` would occupy in this
    /// session. The name is sanitised — the result stays within the session
    /// directory.
    pub fn path_for(&self, file_name: &str) -> PathBuf {
        self.session_dir().join(sanitize_component(file_name))
    }

    /// Writes `contents` to `<session_dir>/<file_name>`, creating the session
    /// directory if it does not yet exist. Returns the staged path.
    pub fn stage(&self, file_name: &str, contents: &[u8]) -> io::Result<PathBuf> {
        let dir = self.session_dir();
        fs::create_dir_all(&dir)?;
        let path = dir.join(sanitize_component(file_name));
        fs::write(&path, contents)?;
        Ok(path)
    }

    /// `true` when this session directory currently exists on disk.
    pub fn exists(&self) -> bool {
        self.session_dir().is_dir()
    }

    /// Removes this session directory and everything staged in it. A no-op
    /// (not an error) when the session directory does not exist.
    pub fn clear(&self) -> io::Result<()> {
        let dir = self.session_dir();
        if dir.is_dir() {
            fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }
}

/// What a [`gc_staging`] / [`gc_staging_in`] sweep removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct GcReport {
    /// Stale session directories removed.
    pub sessions_removed: u64,
    /// Files removed — staged scripts inside swept session dirs, plus any
    /// loose files found directly in the staging root.
    pub files_removed: u64,
    /// Tee logs removed by the reused [`cleanup_tee`].
    /// Always `0` for [`gc_staging_in`] (which sweeps a single explicit root).
    pub tee_logs_removed: u64,
}

impl GcReport {
    /// Total entries removed across all three categories.
    pub fn total(&self) -> u64 {
        self.sessions_removed + self.files_removed + self.tee_logs_removed
    }

    /// `true` when the sweep removed nothing.
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

impl fmt::Display for GcReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "gc_staging: {} session(s), {} file(s), {} tee log(s) removed",
            self.sessions_removed, self.files_removed, self.tee_logs_removed
        )
    }
}

/// The most-recent activity time of a session directory: the later of its own
/// mtime and the newest mtime among its direct children. Using the contained
/// files' mtimes catches modifications that do not bump the parent dir mtime;
/// using the dir's own mtime as a floor keeps a freshly-created empty session
/// alive through a long retention window.
fn last_activity(dir: &Path) -> SystemTime {
    let mut newest = fs::metadata(dir)
        .and_then(|m| m.modified())
        .unwrap_or(UNIX_EPOCH);
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            if let Ok(mtime) = entry.metadata().and_then(|m| m.modified())
                && mtime > newest
            {
                newest = mtime;
            }
        }
    }
    newest
}

/// Shallow count of non-directory entries directly inside `dir`.
fn count_files(dir: &Path) -> u64 {
    let n = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().map(|t| !t.is_dir()).unwrap_or(false))
                .count()
        })
        .unwrap_or(0);
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// Sweeps a single staging `root`: removes session directories (and any loose
/// files in the root) whose most-recent activity is older than
/// `retention_secs`. Does **not** touch the tee tree — that is the extra step
/// [`gc_staging`] adds. Returns without error when `root` does not exist.
pub fn gc_staging_in(root: &Path, retention_secs: u64) -> io::Result<GcReport> {
    let mut report = GcReport::default();
    if !root.is_dir() {
        return Ok(report);
    }
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(retention_secs))
        .unwrap_or(UNIX_EPOCH);
    for entry in fs::read_dir(root)? {
        let Ok(entry) = entry else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        let path = entry.path();
        if meta.is_dir() {
            // A session directory is stale when its most-recent activity is
            // past the cutoff.
            if last_activity(&path) <= cutoff {
                let inside = count_files(&path);
                if fs::remove_dir_all(&path).is_ok() {
                    report.sessions_removed += 1;
                    report.files_removed += inside;
                }
            }
        } else {
            // A loose file directly in the staging root (outside any session
            // partition) — swept by its own mtime, mirroring `cleanup_tee`.
            let stale = meta.modified().map(|m| m <= cutoff).unwrap_or(false);
            if stale && fs::remove_file(&path).is_ok() {
                report.files_removed += 1;
            }
        }
    }
    Ok(report)
}

/// Garbage-collects the managed staging tree, removing every staged entry
/// older than `retention_secs`.
///
/// Sweeps [`staging_root`], then **reuses**
/// [`cleanup_tee`] on the same window so
/// a single call bounds the whole `~/.claude/touring/` transient family
/// (staging tree + tee logs). A failure reaching the tee tree never aborts
/// the staging sweep — `tee_logs_removed` falls back to `0`.
pub fn gc_staging(retention_secs: u64) -> io::Result<GcReport> {
    let mut report = gc_staging_in(&staging_root(), retention_secs)?;
    report.tee_logs_removed = cleanup_tee(retention_secs).unwrap_or(0);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    // ── stage_path — pure path composition (no env, no disk) ─────────────

    #[test]
    fn stage_path_composes_session_and_file() {
        let p = stage_path("sess-a", "snippet.py");
        assert!(
            p.ends_with("sess-a/snippet.py"),
            "stage_path must end with <session>/<file>, got {p:?}"
        );
    }

    #[test]
    fn stage_path_sanitizes_traversal() {
        let p = stage_path("../../etc", "../evil.sh");
        assert!(
            !p.components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
            "sanitised path must not contain `..`, got {p:?}"
        );
    }

    #[test]
    fn sanitize_component_neutralizes_dot_components() {
        assert_eq!(sanitize_component(".."), "default");
        assert_eq!(sanitize_component("."), "default");
        assert_eq!(sanitize_component(""), "default");
        assert_eq!(sanitize_component("a/b"), "a_b");
        assert_eq!(sanitize_component("ok-name_1.rs"), "ok-name_1.rs");
        // A `..` buried in an otherwise-valid name stays a literal single
        // component — it has no separator, so it cannot traverse.
        assert!(!sanitize_component("x..y").contains('/'));
    }

    // ── StagingArea — env-free via `with_root` + tempdir ─────────────────

    #[test]
    fn staging_area_stage_writes_transient_script() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let area = StagingArea::with_root(tmp.path(), "sess-write");
        let staged = area.stage("run.sh", b"echo staged-script").expect("stage");
        assert_eq!(fs::read(&staged).expect("read back"), b"echo staged-script");
        assert!(staged.starts_with(tmp.path()));
    }

    #[test]
    fn staging_area_stage_creates_session_dir() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let area = StagingArea::with_root(tmp.path(), "sess-mkdir");
        assert!(!area.exists(), "session dir must not pre-exist");
        area.stage("a.py", b"print('p5.1')").expect("stage");
        assert!(area.exists(), "session dir must exist after stage");
        assert!(area.session_dir().is_dir());
    }

    #[test]
    fn staging_area_path_for_under_session() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let area = StagingArea::with_root(tmp.path(), "sess-pf");
        let p = area.path_for("x.ts");
        assert!(p.starts_with(area.session_dir()));
        assert!(p.ends_with("x.ts"));
    }

    #[test]
    fn staging_area_exists_reflects_disk() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let area = StagingArea::with_root(tmp.path(), "sess-ex");
        assert!(!area.exists());
        area.stage("f", b"x").expect("stage");
        assert!(area.exists());
    }

    #[test]
    fn staging_area_clear_removes_session() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let area = StagingArea::with_root(tmp.path(), "sess-clear");
        area.stage("f1", b"a").expect("stage");
        area.stage("f2", b"b").expect("stage");
        assert!(area.exists());
        area.clear().expect("clear");
        assert!(!area.exists(), "clear must remove the session dir");
        // clear on an absent session is a no-op, not an error.
        area.clear().expect("clear is idempotent");
    }

    #[test]
    fn staging_area_for_session_sanitizes() {
        let area = StagingArea::with_root("/tmp/x", "../../escape");
        assert!(
            !area.session().contains('/'),
            "session id must be a single safe component, got {:?}",
            area.session()
        );
    }

    // ── gc_staging_in — env-free via explicit root ───────────────────────

    #[test]
    fn gc_staging_in_removes_stale_session() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let area = StagingArea::with_root(tmp.path(), "stale");
        area.stage("old.sh", b"echo old").expect("stage");
        // retention 0 → cutoff is `now`; the staged file's mtime is <= now.
        let report = gc_staging_in(tmp.path(), 0).expect("gc");
        assert_eq!(report.sessions_removed, 1);
        assert_eq!(report.files_removed, 1);
        assert!(!area.exists(), "stale session must be swept");
    }

    #[test]
    fn gc_staging_in_keeps_fresh_session() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let area = StagingArea::with_root(tmp.path(), "fresh");
        area.stage("new.sh", b"echo new").expect("stage");
        // huge retention → cutoff is the epoch; nothing recent is stale.
        let report = gc_staging_in(tmp.path(), u64::MAX).expect("gc");
        assert_eq!(report.sessions_removed, 0);
        assert!(area.exists(), "fresh session must survive GC");
    }

    #[test]
    fn gc_staging_in_removes_empty_session() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let empty = tmp.path().join("empty-sess");
        fs::create_dir_all(&empty).expect("mkdir");
        let report = gc_staging_in(tmp.path(), 0).expect("gc");
        assert_eq!(report.sessions_removed, 1);
        assert!(!empty.exists(), "empty stale session must be swept");
    }

    #[test]
    fn gc_staging_in_sweeps_loose_files() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        // A file written directly into the staging root, outside any session.
        fs::write(tmp.path().join("loose.txt"), b"orphan").expect("write");
        let report = gc_staging_in(tmp.path(), 0).expect("gc");
        assert_eq!(report.files_removed, 1);
        assert!(!tmp.path().join("loose.txt").exists());
    }

    #[test]
    fn gc_staging_in_absent_root_is_ok() {
        let report = gc_staging_in(Path::new("/nonexistent/touring/staging/xyz"), 0)
            .expect("gc on absent root is not an error");
        assert!(report.is_empty());
    }

    #[test]
    fn gc_report_total_and_display() {
        let r = GcReport {
            sessions_removed: 2,
            files_removed: 5,
            tee_logs_removed: 3,
        };
        assert_eq!(r.total(), 10);
        assert!(!r.is_empty());
        assert!(GcReport::default().is_empty());
        let shown = r.to_string();
        assert!(shown.contains("2 session"));
        assert!(shown.contains("5 file"));
        assert!(shown.contains("3 tee log"));
    }

    // ── env-resolved paths — `#[serial]` (env vars are process-global) ───

    #[test]
    #[serial]
    fn staging_root_honors_env_override() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_STAGING_DIR", tmp.path()) };
        let root = staging_root();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_STAGING_DIR") };
        assert_eq!(root, tmp.path());
    }

    #[test]
    #[serial]
    fn staging_retention_secs_reads_env() {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_STAGING_RETENTION_SECS") };
        assert_eq!(staging_retention_secs(), DEFAULT_STAGING_RETENTION_SECS);
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_STAGING_RETENTION_SECS", "3600") };
        let parsed = staging_retention_secs();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_STAGING_RETENTION_SECS") };
        assert_eq!(parsed, 3600);
    }

    #[test]
    #[serial]
    fn staging_area_current_uses_session_env() {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_SESSION_ID", "live-session-7") };
        let area = StagingArea::current();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_SESSION_ID") };
        assert_eq!(area.session(), "live-session-7");
    }

    #[test]
    #[serial]
    fn gc_staging_resolves_env_root() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_STAGING_DIR", tmp.path()) };
        let area = StagingArea::for_session("env-sess");
        area.stage("s.sh", b"echo env").expect("stage");
        assert!(area.exists());
        // `u64::MAX` retention → cutoff is the epoch → nothing is swept. This
        // exercises the `cleanup_tee` reuse non-destructively: at this
        // retention `cleanup_tee` removes nothing from the real tee tree.
        let report = gc_staging(u64::MAX).expect("gc");
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_STAGING_DIR") };
        assert_eq!(report.sessions_removed, 0);
        assert!(area.exists(), "env-resolved fresh session survives GC");
    }
}
