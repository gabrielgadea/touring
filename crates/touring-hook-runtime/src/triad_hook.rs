//! WriteTriadHook — TRIAD pattern (snapshot + validate + describe) for Write operations.
//!
//! 1. **pre_write**: snapshot the target via BranchFs and ARM it keyed by
//!    `(session_id, canonical path)` — [`arm`].
//! 2. **post_write**: TAKE the pair's own baseline — [`take`] — and let the validation
//!    verdict through; on `Block` the verdict gains a `+N −M lines` summary of the write.
//!
//! **D1-A (approved 25/09/2026): the hook path never restores a file.** Claude Code's
//! PostToolUse "can't undo actions"; rolling back behind the model broke the next Edit's
//! `old_string`, and the audit reproduced a CLEAN write reverted (v1 → v2 → v1) because an
//! advisory context counted as a failure. [`TriadState::rollback`] stays for explicit,
//! human-invoked tooling only.
//!
//! **Why a keyed store**: the previous single `RefCell` slot on `HookRuntime` was shared by
//! every session served by the daemon — the post-write of file A consumed (and rolled back)
//! the snapshot of file B. The key makes a post-write see only its own pre-write.
//!
//! **The baseline crosses the process boundary (17/10/2026)**. The in-memory store lives in
//! ONE process, but pre-write and post-write do not always share one: the client falls back
//! to the standalone binary when the daemon misses its 3 s budget (and with
//! `TOURING_NO_DAEMON=1`), and under load that fallback is common — measured 7/8 lost
//! baselines with 16 concurrent loops, 0/10 unloaded. A lost baseline silently disabled the
//! S28 gate (`before = None` abstained as `_ => 0`), so antipatterns already in the file
//! became indistinguishable from ones the write introduced. So [`arm`] ALSO persists the
//! baseline to a per-user directory and [`take`] falls back to it; a genuinely absent
//! baseline is [`TriadLookup::Absent`] — a REASON the caller must declare, never a silent zero.

use crate::branch_fs::BranchFs;
use crate::errors::Result;
use crate::runtime::HookResponse;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// Snapshots older than this are dropped (their TempDir is cleaned); a post-write without
/// its pair is a plain pass-through.
pub const TRIAD_TTL: Duration = Duration::from_secs(600);
/// Upper bound on concurrently armed snapshots (all sessions of one daemon).
pub const TRIAD_MAX_ARMED: u64 = 4_096;
/// Files larger than this get no line summary (the snapshot still exists).
const DIFF_SUMMARY_MAX_BYTES: usize = 2 * 1024 * 1024;
/// A baseline bigger than this is NOT persisted across processes — an 8 MiB copy of user
/// source per write is not worth the gate; [`TriadAbsent::Oversized`] declares it instead.
pub const TRIAD_DISK_MAX_BYTES: usize = 8 * 1024 * 1024;
/// On-disk record version; a reader that does not know it declares, never guesses.
const DISK_VERSION: u32 = 1;
/// Extension of a persisted baseline record.
const DISK_SUFFIX: &str = "tbase";

/// Key of an armed snapshot: `(session_id, canonical path)`.
pub type TriadKey = (String, PathBuf);

fn triad_store() -> &'static moka::sync::Cache<TriadKey, Arc<TriadState>> {
    static STORE: OnceLock<moka::sync::Cache<TriadKey, Arc<TriadState>>> = OnceLock::new();
    STORE.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(TRIAD_MAX_ARMED)
            .time_to_live(TRIAD_TTL)
            .build()
    })
}

/// Session id from a hook payload; a payload without one gets its own `"anon"` key,
/// never another session's.
pub fn session_of(input: &serde_json::Value) -> String {
    input
        .get("session_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("anon")
        .to_string()
}

/// Canonical key path: absolute, with the PARENT canonicalized (the file itself may not
/// exist yet at pre-write), so `rel/x.py`, `/abs/x.py` and a symlinked parent agree.
pub fn canonical_path(file_path: &str) -> PathBuf {
    let raw = Path::new(file_path);
    let abs = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(raw))
            .unwrap_or_else(|_| raw.to_path_buf())
    };
    match (abs.parent(), abs.file_name()) {
        (Some(parent), Some(name)) => std::fs::canonicalize(parent)
            .map(|p| p.join(name))
            .unwrap_or(abs.clone()),
        _ => abs,
    }
}

/// Snapshot `file_path` and arm it for `session_id`. Returns false (and arms nothing)
/// when the snapshot cannot be taken — the write proceeds unprotected, never blocked.
/// A second pre-write of the same (session, path) replaces the first (last pre wins).
pub fn arm(session_id: &str, file_path: &str) -> bool {
    match run_pre_write(file_path) {
        Some(state) => {
            let key = (session_id.to_string(), canonical_path(file_path));
            // The BranchFs snapshot is process-local and dies with it; the S28 baseline is a
            // plain string, so it survives. A failure here is not fatal — the in-memory copy
            // still serves a same-process post-write.
            persist_baseline(&key, &state.baseline());
            triad_store().insert(key, Arc::new(state));
            true
        }
        None => false,
    }
}

/// What the file held before a write — the S28 gate's "before" side.
///
/// `Absent` = the file did not exist (every occurrence in it is new); `Content` = its text;
/// `Unknown` = it could not be read, and a consumer MUST abstain rather than count it as
/// absent (that conflation is what made the gate silently useless).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Baseline {
    /// The file did not exist when the baseline was taken.
    Absent,
    /// The file held this text.
    Content(String),
    /// The pre-write content could not be read — unknown, not empty.
    Unknown,
}

impl Baseline {
    /// The "before" text handed to the antipattern gate: `None` means the file did not
    /// exist, so every occurrence counted is genuinely new.
    pub fn as_before(&self) -> Option<&str> {
        match self {
            Self::Absent => None,
            Self::Content(text) => Some(text),
            Self::Unknown => None,
        }
    }

    /// Whether this baseline is usable for a before × after comparison.
    pub fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown)
    }
}

/// Why no baseline was available at post-write — declared, never swallowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriadAbsent {
    /// No pre-write armed this (session, path) in any process.
    NeverArmed,
    /// A baseline was armed but its [`TRIAD_TTL`] elapsed.
    Expired,
    /// The baseline exceeded [`TRIAD_DISK_MAX_BYTES`] and was never persisted.
    Oversized,
    /// The persisted record could not be read (foreign version, truncated, corrupt).
    Unreadable(String),
    /// The per-user state directory could not be used.
    Unavailable(String),
}

impl TriadAbsent {
    /// One line the post hook injects so the model knows the gate did not judge this write.
    pub fn note(&self) -> String {
        let why = match self {
            Self::NeverArmed => "no pre-write armed this file in any process",
            Self::Expired => "the pre-write baseline expired",
            Self::Oversized => "the baseline exceeded TRIAD_DISK_MAX_BYTES and was not persisted",
            Self::Unreadable(why) => return format!("S28 abstained: baseline unreadable ({why})"),
            Self::Unavailable(why) => {
                return format!("S28 abstained: baseline store unavailable ({why})");
            }
        };
        format!(
            "S28 abstained: {why} — NEW anti-patterns were not counted for this write \
             (pre-existing ones are indistinguishable from introduced ones)."
        )
    }
}

/// The result of taking a baseline: found (with content) or absent (with a reason).
#[derive(Debug)]
pub enum TriadLookup {
    /// A baseline is available.
    Found(Baseline),
    /// None is available — the caller MUST surface [`TriadAbsent::note`].
    Absent(TriadAbsent),
}

impl TriadLookup {
    /// The before-text for the S28 gate, or `None` when the gate must abstain.
    pub fn before(&self) -> Option<&str> {
        match self {
            Self::Found(b) if b.is_known() => b.as_before(),
            _ => None,
        }
    }

    /// Whether a usable baseline was found.
    pub fn is_found(&self) -> bool {
        matches!(self, Self::Found(b) if b.is_known())
    }
}

/// What [`take_lookup`] resolved.
#[derive(Debug)]
pub struct TriadTake {
    /// The baseline for this (session, path), if any.
    pub lookup: TriadLookup,
    /// The armed state, when the SAME process armed it (only then is the `BranchFs`
    /// snapshot available, and only then can `describe_write` print a `+N −M` summary).
    pub state: Option<Arc<TriadState>>,
}

/// Take the baseline for `(session_id, file_path)`, preferring this process's memory and
/// falling back to the per-user on-disk record. Never returns a silent "nothing" — an
/// unavailable baseline comes back as [`TriadLookup::Absent`] carrying its reason.
pub fn take_lookup(session_id: &str, file_path: &str) -> TriadTake {
    let key = (session_id.to_string(), canonical_path(file_path));
    let state = triad_store().remove(&key);
    // The same-process snapshot is AUTHORITATIVE when it exists: it is the real pre-write
    // state, while the disk holds a copy that may be missing, expired or unreadable.
    // Consulting the disk first would drop a good in-memory baseline whenever the disk merely
    // had no record — which is every NEW file (nothing to persist) and every write whose
    // persistence was skipped. That regressed the gate on exactly the case where it must run.
    if let Some(armed) = state.clone() {
        let baseline = armed.baseline();
        if baseline.is_known() {
            // Consume any record this write also left, so it cannot answer a later post-write.
            let _ = load_baseline(&key);
            return TriadTake {
                lookup: TriadLookup::Found(baseline),
                state: Some(armed),
            };
        }
    }
    let lookup = match load_baseline(&key) {
        Ok(Some(baseline)) if baseline.is_known() => TriadLookup::Found(baseline),
        // No record, or one that decodes to `Unknown` — which must not pass as a known
        // baseline, or the gate would count against a fiction.
        Ok(Some(_)) | Ok(None) => TriadLookup::Absent(TriadAbsent::NeverArmed),
        Err(reason) => TriadLookup::Absent(reason),
    };
    TriadTake { lookup, state }
}

// ── Cross-process baseline store ──────────────────────────────────────────
//
// The gate only needs the "before" TEXT, not the `BranchFs` snapshot, so the on-disk record
// is a small header plus that text — cheap enough to write on every pre-write, and it is
// what lets a standalone pre-write arm a baseline a daemon post-write can read.

/// Per-user directory for persisted baselines, with a 0700 mode.
///
/// Resolution, in order:
/// 1. `TOURING_TRIAD_DIR` — an explicit override. Used by the tests to get a private dir per
///    case (env vars are process-global, so those tests are `#[serial]`), and available as a
///    human override for a session that must not share the usual state.
/// 2. `$XDG_RUNTIME_DIR/touring/triad` when absolute (it is per-user and wiped at logout).
/// 3. `touring-<uid>/touring/triad` under the temp dir.
///
/// Returns `None` when none is usable — the caller then DECLARES the abstention rather than
/// losing it quietly.
fn triad_state_dir() -> Option<PathBuf> {
    let dir = match std::env::var_os("TOURING_TRIAD_DIR").map(PathBuf::from) {
        Some(explicit) => explicit,
        None => {
            let base = std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| std::env::temp_dir().join(format!("touring-{}", current_uid())));
            base.join("touring").join("triad")
        }
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    set_mode(&dir, 0o700);
    Some(dir)
}

/// This process's real uid: `$UID` when the environment carries it, `getuid()` otherwise.
///
/// `$UID` is a bash builtin export — it is NOT inherited by a systemd unit, a cron job or a
/// scrubbed hook environment (measured: absent in a plain non-bash child). Falling back to
/// `0` there would put every baseline in `/tmp/touring-0`, a world-predictable path another
/// local user can pre-create and then read our source text out of. `getuid()` cannot be
/// spoofed by the environment, so it is the authority and `$UID` is only a shortcut.
///
/// Read once: a uid changes only across a login.
fn current_uid() -> u32 {
    static UID: OnceLock<u32> = OnceLock::new();
    *UID.get_or_init(|| {
        // SAFETY: `getuid()` is a pure syscall with no preconditions and no memory access;
        // it cannot fail and has no invariants to uphold.
        let real = unsafe { libc::getuid() };
        if real != 0 {
            return real;
        }
        // Genuinely root: there is no other local user to confuse us with, but keep the
        // env value only when it agrees with the syscall.
        std::env::var("UID")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0)
    })
}

/// Best-effort `chmod` — the store holds source text, so it must not be group/world readable.
fn set_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

/// Record file for `key`: a BLAKE3 of the session and canonical path — the same inputs the
/// in-memory key derives from, so a hash collision is not a realistic way to read another
/// session's baseline, and the filename itself never leaks either input.
fn record_path(dir: &Path, session_id: &str, path: &Path) -> PathBuf {
    let digest = blake3::hash(format!("{session_id}\u{0}{}", path.display()).as_bytes());
    dir.join(format!("{}.{DISK_SUFFIX}", digest.to_hex()))
}

/// Distinct temp name per write, so two concurrent arms of the SAME key cannot interleave
/// into one file (the loser's rename is simply overwritten — the record is idempotent).
static WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Upper bound on files the opportunistic sweeper inspects in one call. Bounded because it
/// runs on the pre-write path, which every write pays.
const SWEEP_MAX_SCAN: usize = 64;
/// Sweep budget: how many directories-sweeps happen per process before we stop trying.
const SWEEP_MAX_CALLS: u32 = 8;

/// Delete records whose TTL has elapsed and that no take has claimed.
///
/// Without this the store grows without bound: every pre-write whose post-write never arrives
/// (crashed tool call, aborted edit, or the very daemon/standalone split that motivated the
/// on-disk baseline) leaves up to 8 MiB of source text behind forever. `load_baseline` only
/// deletes what it READS, and `check_header` refuses an expired record without removing it.
///
/// Opportunistic and bounded — not a background task — and a failure here is silent by
/// design: a stale record is refused by TTL anyway, so skipping the sweep costs disk, never
/// correctness.
fn sweep_expired(dir: &Path) {
    static CALLS: AtomicU32 = AtomicU32::new(0);
    if CALLS.fetch_add(1, Ordering::Relaxed) >= SWEEP_MAX_CALLS {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = unix_now();
    for entry in entries.flatten().take(SWEEP_MAX_SCAN) {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != DISK_SUFFIX) {
            // Never touch a `.tmp` that another process may still be writing.
            continue;
        }
        let Ok(raw) = std::fs::read(&path) else {
            continue;
        };
        let Some(nl) = raw.iter().position(|b| *b == b'\n') else {
            continue;
        };
        let Ok(header) = serde_json::from_slice::<serde_json::Value>(&raw[..nl]) else {
            // Unparseable: it can never be used, so it is pure litter.
            let _ = std::fs::remove_file(&path);
            continue;
        };
        let age = header
            .get("at")
            .and_then(serde_json::Value::as_u64)
            .map_or(TRIAD_TTL.as_secs() + 1, |at| now.saturating_sub(at));
        if age > TRIAD_TTL.as_secs() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Persist `baseline` for `key`. Failure is logged and swallowed: the in-memory copy still
/// serves a same-process post-write, and losing the cross-process copy is a degradation,
/// never a block.
fn persist_baseline(key: &TriadKey, baseline: &Baseline) {
    // `Unknown` is not a baseline worth persisting: writing nothing keeps "unknown" honest
    // across the boundary too. `Absent` IS persisted, as a zero-length record — a NEW file's
    // baseline is exactly as decisive as an existing one's (every occurrence in it is new),
    // and dropping it would abstain on the very writes the gate exists to catch.
    let text = match baseline {
        Baseline::Content(text) => text.as_str(),
        Baseline::Absent => "",
        Baseline::Unknown => return,
    };
    if text.len() > TRIAD_DISK_MAX_BYTES {
        tracing::debug!(
            path = %key.1.display(),
            bytes = text.len(),
            "TRIAD: baseline above TRIAD_DISK_MAX_BYTES — not persisted"
        );
        return;
    }
    let Some(dir) = triad_state_dir() else {
        tracing::debug!("TRIAD: no usable state dir — baseline not persisted");
        return;
    };
    sweep_expired(&dir);
    let final_path = record_path(&dir, &key.0, &key.1);
    let seq = WRITE_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = final_path.with_extension(format!("{DISK_SUFFIX}.{seq}.tmp"));
    let header = serde_json::json!({
        "v": DISK_VERSION,
        "at": unix_now(),
        "path": key.1.display().to_string(),
        "session": key.0,
        "len": text.len(),
    });
    let write = || -> std::io::Result<()> {
        let mut f = create_private(&tmp)?;
        f.write_all(serde_json::to_string(&header)?.as_bytes())?;
        f.write_all(b"\n")?;
        f.write_all(text.as_bytes())?;
        f.sync_all()
    };
    if let Err(e) = write() {
        tracing::debug!(error = %e, "TRIAD: baseline write failed — cross-process copy lost");
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    set_mode(&tmp, 0o600);
    if let Err(e) = std::fs::rename(&tmp, &final_path) {
        tracing::debug!(error = %e, "TRIAD: baseline rename failed — cross-process copy lost");
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Create `path` for writing with mode 0600 from the start (never briefly world-readable),
/// refusing to follow a symlink planted at that name.
///
/// `O_NOFOLLOW` matters because the temp name is `{hash}.tbase.{seq}.tmp` with `seq` starting
/// at 0 per process — fully predictable to anyone who can write the state dir. Without it, a
/// pre-planted symlink turns this open into an arbitrary-file overwrite of the victim's own
/// files. (`rename`, which produces the FINAL record, replaces a symlink rather than
/// following it, so that step was never the exposure.)
fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
    use std::io::Write as _;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
        // SAFETY: `custom_flags` is passed straight to `open(2)`; `O_NOFOLLOW` is a
        // well-defined flag with no memory-safety implication.
        #[cfg(target_os = "linux")]
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    let mut f = opts.open(path)?;
    f.flush()?;
    Ok(f)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Read (and REMOVE) the persisted baseline for `key`. The record is consumed by the take,
/// so a stale one can never answer a later post-write.
fn load_baseline(key: &TriadKey) -> BaselineRead {
    let Some(dir) = triad_state_dir() else {
        return Err(TriadAbsent::Unavailable(
            "no per-user state directory".to_string(),
        ));
    };
    let path = record_path(&dir, &key.0, &key.1);
    let Ok(raw) = std::fs::read(&path) else {
        return Ok(None);
    };
    let _ = std::fs::remove_file(&path);
    parse_baseline(&raw, &key.1)
}

/// `Ok(None)` = no record; `Err` = a record existed but could not be used, and the reason
/// must reach the caller as a declared abstention.
///
/// Spelled out rather than `Result`: this module imports `Result` from [`crate::errors`],
/// which is the crate's one-parameter alias for `core::result::Result<T, TouringError>`.
type BaselineRead = std::result::Result<Option<Baseline>, TriadAbsent>;

/// Parse a record: header line of JSON, then exactly the declared number of text bytes.
/// Anything unexpected is [`TriadAbsent::Unreadable`] — an unreadable baseline must abstain,
/// never degrade into "the file did not exist" (which would count every hit as new).
fn parse_baseline(raw: &[u8], expect_path: &Path) -> BaselineRead {
    let Some(nl) = raw.iter().position(|b| *b == b'\n') else {
        return Err(TriadAbsent::Unreadable("no header".into()));
    };
    let header: serde_json::Value = serde_json::from_slice(&raw[..nl])
        .map_err(|e| TriadAbsent::Unreadable(format!("bad header: {e}")))?;
    check_header(&header, expect_path)?;
    let len = header
        .get("len")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| TriadAbsent::Unreadable("no length".into()))? as usize;
    if len > TRIAD_DISK_MAX_BYTES {
        return Err(TriadAbsent::Oversized);
    }
    let body = raw
        .get(nl + 1..)
        .ok_or_else(|| TriadAbsent::Unreadable("truncated body".into()))?;
    if body.len() != len {
        return Err(TriadAbsent::Unreadable(format!(
            "body {} bytes, header says {len}",
            body.len()
        )));
    }
    // A zero-length body is how `Absent` is persisted: the record's existence is the signal,
    // and an empty file is a legitimate pre-state (a file that existed and held nothing).
    if body.is_empty() {
        return Ok(Some(Baseline::Absent));
    }
    Ok(Some(Baseline::Content(
        String::from_utf8_lossy(body).into_owned(),
    )))
}

/// Version, age and path checks — every one of them a way a record can be refused instead of
/// trusted. The stored path makes a truncated or foreign file recognizable rather than
/// silently readable as this file's baseline.
fn check_header(
    header: &serde_json::Value,
    expect_path: &Path,
) -> std::result::Result<(), TriadAbsent> {
    if header.get("v").and_then(serde_json::Value::as_u64) != Some(DISK_VERSION as u64) {
        return Err(TriadAbsent::Unreadable(format!(
            "record version {:?}",
            header.get("v")
        )));
    }
    if let Some(at) = header.get("at").and_then(serde_json::Value::as_u64)
        && unix_now().saturating_sub(at) > TRIAD_TTL.as_secs()
    {
        return Err(TriadAbsent::Expired);
    }
    let stored = header
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if Path::new(stored) != expect_path {
        return Err(TriadAbsent::Unreadable("path mismatch".into()));
    }
    Ok(())
}

/// TRIAD state for one write operation (one armed snapshot).
#[derive(Debug)]
pub struct TriadState {
    /// Snapshot of files before the write operation.
    pub snapshot: BranchFs,
    /// Path that was written.
    pub written_path: String,
    /// Whether the write has been validated (success) or failed (rollback).
    pub validated: bool,
}

impl TriadState {
    /// Create a new TRIAD state by snapshotting the target file.
    ///
    /// Snapshots the file at `file_path` before any write occurs.
    /// If the file doesn't exist, it records the absence.
    pub fn pre_write(file_path: &str) -> Result<Self> {
        let path = Path::new(file_path);
        let snapshot = BranchFs::new(&[path])?;

        Ok(Self {
            snapshot,
            written_path: file_path.to_string(),
            validated: false,
        })
    }

    /// Mark the sequence as validated (success path).
    ///
    /// Calling this after a successful post_write commits the snapshot
    /// (discards it). The file content is kept.
    pub fn mark_validated(mut self) {
        self.validated = true;
        // Commit: drop snapshot without restoring
        self.snapshot.commit();
    }

    /// What the file held before the write, as a persistable [`Baseline`].
    ///
    /// The three-way split is the point: an unreadable snapshot is [`Baseline::Unknown`],
    /// never [`Baseline::Absent`] — conflating them is what let a lost baseline look like a
    /// clean file and made every antipattern count as new.
    pub fn baseline(&self) -> Baseline {
        match self
            .snapshot
            .snapshot_content(Path::new(&self.written_path))
        {
            Ok(Some(bytes)) => Baseline::Content(String::from_utf8_lossy(&bytes).into_owned()),
            Ok(None) => Baseline::Absent,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %self.written_path,
                    "TRIAD: baseline unreadable — the gate will abstain, not assume absence"
                );
                Baseline::Unknown
            }
        }
    }

    /// Rollback the write operation.
    ///
    /// Restores the file to its pre-write state.
    /// Should be called when post_write validation fails.
    pub fn rollback(&self) -> Result<()> {
        Ok(self.snapshot.restore()?)
    }
}

/// Run the TRIAD pre-write phase.
///
/// Call at the start of a write operation. Returns `Some(TriadState)` on success,
/// `None` if snapshotting failed (in which case, proceed without TRIAD protection).
pub fn run_pre_write(file_path: &str) -> Option<TriadState> {
    match TriadState::pre_write(file_path) {
        Ok(state) => {
            tracing::debug!(
                path = %file_path,
                branch_id = %state.snapshot.metadata().branch_id,
                "TRIAD: pre_write snapshot created"
            );
            Some(state)
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %file_path,
                "TRIAD: pre_write snapshot failed — proceeding without protection"
            );
            None
        }
    }
}

/// TRIAD post-write: pass the validation verdict through WITHOUT touching the disk (D1-A).
///
/// `Allow` and advisory `Context` are returned unchanged — an advisory note is not a
/// failure. On `Block` the verdict is kept and gains a one-line description of what the
/// write changed (`+N −M lines vs pre-write baseline`), so the model fixes forward knowing
/// the content IS on disk. The event name is always the real one, `PostToolUse`.
///
/// Takes the written path and its [`Baseline`] rather than a [`TriadState`], because the
/// baseline is the only part that has to be here — it survives the process boundary, while
/// the `BranchFs` snapshot does not. Passing the state would have made the "kept on disk"
/// note disappear exactly when the write was blocked from another process.
pub fn run_post_write(
    written_path: &str,
    baseline: &Baseline,
    response: HookResponse,
) -> HookResponse {
    match response {
        HookResponse::Block {
            reason, context, ..
        } => {
            let note = describe_write(written_path, baseline);
            tracing::info!(path = %written_path, %note, "TRIAD: blocked write kept on disk");
            let context = Some(match context {
                Some(c) if !c.is_empty() => format!("{c}\n{note}"),
                _ => note,
            });
            HookResponse::Block {
                reason,
                context,
                event_name: Some("PostToolUse".to_string()),
            }
        }
        other => other,
    }
}

/// `TRIAD: write kept on disk … +N −M lines vs pre-write baseline` (never panics).
///
/// The head is emitted even when the line delta cannot be computed: "the content is already
/// written and the model must fix forward" is true regardless, and dropping it would leave
/// the model guessing whether to retry.
pub fn describe_write(written_path: &str, baseline: &Baseline) -> String {
    let head = "TRIAD: write kept on disk (PostToolUse cannot undo; fix forward)";
    let Baseline::Content(before) = baseline else {
        return format!("{head} — no line summary (file was absent before the write)");
    };
    let after = match std::fs::read(written_path) {
        Ok(a) => a,
        Err(e) => return format!("{head} — line summary unavailable ({e})"),
    };
    if before.len().max(after.len()) > DIFF_SUMMARY_MAX_BYTES {
        return format!("{head} — file above {DIFF_SUMMARY_MAX_BYTES} bytes, no line summary");
    }
    let (added, removed) = line_delta(before, &String::from_utf8_lossy(&after));
    format!("{head} — +{added} −{removed} lines vs pre-write baseline")
}

/// Multiset line delta: how many lines the new text has beyond the old (added) and the
/// old beyond the new (removed). O(n + m); exact counts, not an edit script.
pub fn line_delta(before: &str, after: &str) -> (usize, usize) {
    let mut balance: HashMap<&str, isize> = HashMap::new();
    for line in before.lines() {
        *balance.entry(line).or_insert(0) -= 1;
    }
    for line in after.lines() {
        *balance.entry(line).or_insert(0) += 1;
    }
    balance.values().fold((0, 0), |(add, rem), &v| {
        if v > 0 {
            (add + v.unsigned_abs(), rem)
        } else {
            (add, rem + v.unsigned_abs())
        }
    })
}

/// Check if a file has drifted since the TRIAD snapshot was taken.
///
/// Use this in post_write to detect whether the file was modified externally
/// (e.g., by a linter) after the snapshot.
pub fn check_drift(state: &TriadState) -> Option<bool> {
    let path = Path::new(&state.written_path);
    state.snapshot.has_drifted(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_pre_write_snapshot_creates_state() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("test.txt");
        fs::write(&file, b"original content").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");
        assert!(!state.validated);
        assert_eq!(state.written_path, file.to_str().unwrap());
        state.snapshot.commit();
    }

    #[test]
    fn test_pre_write_absent_file() {
        let dir = tempdir().expect("tempdir");
        let absent = dir.path().join("nonexistent.txt");

        let state = TriadState::pre_write(absent.to_str().unwrap()).expect("pre_write");
        assert!(state.snapshot.was_absent(&absent));
        state.snapshot.commit();
    }

    #[test]
    fn test_rollback_restores_content() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("rollback_test.txt");
        fs::write(&file, b"original").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");

        // Simulate a failed write (content not properly written)
        fs::write(&file, b"corrupted").expect("write");

        // Rollback should restore original
        state.rollback().expect("rollback");
        assert_eq!(fs::read(&file).expect("read"), b"original");
    }

    #[test]
    fn test_rollback_deletes_created_file() {
        let dir = tempdir().expect("tempdir");
        let new_file = dir.path().join("new_file.txt");

        // File doesn't exist yet
        let state = TriadState::pre_write(new_file.to_str().unwrap()).expect("pre_write");

        // Simulate creating the file
        fs::write(&new_file, b"new content").expect("write");
        assert!(new_file.exists());

        // Rollback should delete it
        state.rollback().expect("rollback");
        assert!(!new_file.exists());
    }

    #[test]
    fn test_mark_validated_commits() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("commit_test.txt");
        fs::write(&file, b"content").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");

        // Modify file after pre_write
        fs::write(&file, b"modified").expect("write");

        // Mark validated should keep the modification
        state.mark_validated();
        assert_eq!(fs::read(&file).expect("read"), b"modified");
    }

    #[test]
    fn test_check_drift_detects_change() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("drift.txt");
        fs::write(&file, b"original").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");

        // Modify after snapshot
        fs::write(&file, b"changed").expect("write");

        let drifted = check_drift(&state);
        assert_eq!(drifted, Some(true));

        state.snapshot.commit();
    }

    #[test]
    fn test_check_drift_no_change() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("no_drift.txt");
        fs::write(&file, b"stable").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");

        let drifted = check_drift(&state);
        assert_eq!(drifted, Some(false));

        state.snapshot.commit();
    }

    #[test]
    fn test_run_pre_write_proceeds_without_protection_on_error() {
        // Passing a directory (not a file) should return None but not panic
        let dir = tempdir().expect("tempdir");
        let result = run_pre_write(dir.path().to_str().unwrap());
        // Directories don't exist as files, so snapshot may fail
        // The function should return None gracefully
        assert!(result.is_none());
    }

    /// S12 / D1-A (25/09/2026): PostToolUse cannot undo a tool call (Claude Code docs), so the
    /// TRIAD never touches the disk. The audit reproduced v1 → v2 → **v1** on a CLEAN write
    /// because an advisory post-write context counted as "validation failed".
    #[test]
    fn post_write_advisory_context_never_touches_disk() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("clean.py");
        fs::write(&file, b"v1\n").expect("write v1");
        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");
        fs::write(&file, b"v2\n").expect("write v2");
        let advisory = HookResponse::Context {
            context: "post-write: 2 issue(s)".into(),
            event_name: None,
        };
        let _ = run_post_write(file.to_str().expect("utf8"), &state.baseline(), advisory);
        assert_eq!(
            fs::read(&file).expect("read"),
            b"v2\n",
            "a post-write verdict must never revert the file"
        );
        let block = HookResponse::Block {
            reason: "4 new anti-patterns".into(),
            context: None,
            event_name: None,
        };
        let _ = run_post_write(file.to_str().expect("utf8"), &state.baseline(), block);
        assert_eq!(
            fs::read(&file).expect("read"),
            b"v2\n",
            "not even a Block reverts the file"
        );
    }

    #[test]
    fn advisory_context_passes_through_unchanged() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("ctx.py");
        fs::write(&file, b"a\n").expect("write");
        let state = TriadState::pre_write(file.to_str().expect("utf8")).expect("pre_write");
        let out = run_post_write(
            file.to_str().expect("utf8"),
            &state.baseline(),
            HookResponse::Context {
                context: "note".into(),
                event_name: None,
            },
        );
        match out {
            HookResponse::Context {
                context,
                event_name,
            } => {
                assert_eq!(context, "note");
                assert_eq!(event_name, None);
            }
            other => panic!("expected the advisory Context back, got {other:?}"),
        }
    }

    #[test]
    fn triad_response_uses_posttooluse_event_name() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("blocked.py");
        fs::write(&file, b"keep\nold\n").expect("write v1");
        let state = TriadState::pre_write(file.to_str().expect("utf8")).expect("pre_write");
        fs::write(&file, b"keep\nnew1\nnew2\n").expect("write v2");
        let out = run_post_write(
            file.to_str().expect("utf8"),
            &state.baseline(),
            HookResponse::Block {
                reason: "r".into(),
                context: None,
                event_name: None,
            },
        );
        let HookResponse::Block {
            reason,
            context,
            event_name,
        } = out
        else {
            panic!("a Block verdict must stay a Block");
        };
        assert_eq!(reason, "r");
        assert_eq!(event_name.as_deref(), Some("PostToolUse"));
        let note = context.expect("the block gains the write description");
        assert!(note.contains("+2 −1 lines"), "{note}");
        assert!(
            !note.contains("Triad") && !note.contains("rollback"),
            "{note}"
        );
    }

    #[test]
    fn triad_state_is_keyed_by_session_and_path() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("shared.py");
        fs::write(&file, b"x\n").expect("write");
        let path = file.to_str().expect("utf8");
        assert!(arm("keyed-s1", path));
        assert!(arm("keyed-s2", path));
        assert!(
            triad_store()
                .remove(&("keyed-s1".to_string(), canonical_path(path)))
                .is_some(),
            "session 1 finds its own snapshot"
        );
        assert!(
            triad_store()
                .remove(&("keyed-s1".to_string(), canonical_path(path)))
                .is_none(),
            "taking consumes it"
        );
        assert!(
            triad_store()
                .remove(&("keyed-s2".to_string(), canonical_path(path)))
                .is_some(),
            "session 2's snapshot survived session 1's post-write"
        );
    }

    #[test]
    fn post_write_of_a_never_consumes_snapshot_of_b() {
        let dir = tempdir().expect("tempdir");
        let a = dir.path().join("a.py");
        let b = dir.path().join("b.py");
        fs::write(&a, b"a\n").expect("write a");
        fs::write(&b, b"b\n").expect("write b");
        assert!(arm("ab-sess", a.to_str().expect("utf8")));
        assert!(arm("ab-sess", b.to_str().expect("utf8")));
        let got = triad_store()
            .remove(&(
                "ab-sess".to_string(),
                canonical_path(a.to_str().expect("utf8")),
            ))
            .expect("a armed");
        assert_eq!(got.written_path, a.to_str().expect("utf8"));
        assert!(
            triad_store()
                .remove(&(
                    "ab-sess".to_string(),
                    canonical_path(b.to_str().expect("utf8"))
                ))
                .is_some(),
            "b is still armed"
        );
    }

    #[test]
    fn relative_and_absolute_paths_share_one_key() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("rel.py");
        fs::write(&file, b"r\n").expect("write");
        let abs = file.to_str().expect("utf8").to_string();
        let dotted = format!("{}/./rel.py", dir.path().display());
        assert!(arm("rel-sess", &dotted));
        assert!(
            triad_store()
                .remove(&("rel-sess".to_string(), canonical_path(&abs)))
                .is_some(),
            "`dir/./rel.py` and `dir/rel.py` must meet"
        );
    }

    #[test]
    fn missing_session_id_gets_its_own_anon_key() {
        assert_eq!(session_of(&serde_json::json!({})), "anon");
        assert_eq!(session_of(&serde_json::json!({"session_id": ""})), "anon");
        assert_eq!(session_of(&serde_json::json!({"session_id": "s-9"})), "s-9");
    }

    #[test]
    fn line_delta_counts_added_and_removed_lines() {
        assert_eq!(line_delta("a\nb\nc\n", "a\nc\nd\ne\n"), (2, 1));
        assert_eq!(line_delta("", "x\n"), (1, 0));
        assert_eq!(line_delta("same\n", "same\n"), (0, 0));
    }

    #[test]
    fn test_run_post_write_allow_on_success() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("post_write.txt");
        fs::write(&file, b"ok").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");

        // Success path: an Allow verdict passes through as Allow
        let response = run_post_write(
            file.to_str().expect("utf8"),
            &state.baseline(),
            HookResponse::Allow,
        );
        match response {
            HookResponse::Allow => {}
            other => panic!("expected Allow, got {:?}", other),
        }

        // Commit the snapshot
        let _ = state.snapshot.commit();
    }
}

/// S28 cross-process baseline (17/10/2026) — the acceptance tests for the defect the `analise`
/// session measured: pre-write and post-write in DIFFERENT processes, and an abstention that
/// is always DECLARED.
///
/// These are unit-level proofs of the two contracts. The end-to-end proof that the Block still
/// arrives with `kept on disk` lives in `touring-hook-handlers` (`s28_gate.rs`), which drives
/// the real `pre_write`/`post_write` handlers.
#[cfg(test)]
mod s28_cross_process_tests {
    use super::*;
    use serial_test::serial;
    use std::fs;

    /// A private scratch directory for one test (the file under edit, mostly).
    ///
    /// It does NOT touch `TOURING_TRIAD_DIR` — the state dir is chosen separately by
    /// [`use_state_dir`], because a second call here would silently repoint the store at a
    /// work directory (which is exactly the bug this suite first had: it wrote the record
    /// into the scratch dir and then looked for it in the other one).
    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("touring-triad-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// Install a private `TOURING_TRIAD_DIR` for the test's duration.
    ///
    /// Env vars are process-global and `cargo test` runs a process' tests on parallel threads,
    /// so every test here is `#[serial]`: without it two cases would share one state dir and
    /// could read each other's records.
    fn use_state_dir(tag: &str) -> PathBuf {
        let dir = scratch(&format!("state-{tag}"));
        // SAFETY: `#[serial]` guarantees no other thread in this process reads it concurrently.
        unsafe { std::env::set_var("TOURING_TRIAD_DIR", &dir) };
        dir
    }

    fn file_with(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, body).expect("write");
        p
    }

    /// The record a given (session, path) maps to, inside `state`.
    fn record(state: &Path, session: &str, path: &Path) -> PathBuf {
        record_path(state, session, path)
    }

    /// (a) A baseline armed by process A is read by process B.
    ///
    /// The defect: `arm` wrote only to a process-local `moka` cache, so when the client fell
    /// back to the standalone binary (daemon slow, or `TOURING_NO_DAEMON=1`) the post-write
    /// found nothing and the gate abstained as `0` — silently. This test writes the record the
    /// way process A would and reads it the way process B would, with a FRESH in-memory store,
    /// so it fails if the disk fallback stops existing.
    #[test]
    #[serial(triad_state)]
    fn baseline_crosses_a_process_boundary() {
        let state = use_state_dir("xproc");
        let work = scratch("xproc-work");
        let file = file_with(&work, "t.py", "x = 1\n");
        let key = (
            "sess-a".to_string(),
            canonical_path(file.to_str().expect("utf8")),
        );

        // Process A arms.
        persist_baseline(&key, &Baseline::Content("x = 1\n".to_string()));
        assert!(
            record(&state, "sess-a", &key.1).exists(),
            "the arm must leave a record for the other process"
        );

        // Process B takes. `triad_store()` here holds nothing for this key.
        let got = load_baseline(&key).expect("readable");
        assert_eq!(
            got,
            Some(Baseline::Content("x = 1\n".to_string())),
            "the baseline armed in another process must be readable"
        );

        // Consumed on take: a second read finds nothing, so a stale record can never answer
        // a later post-write.
        assert_eq!(
            load_baseline(&key).expect("readable"),
            None,
            "the record is consumed by the take"
        );
    }

    /// The disk fallback is keyed by (session, canonical path): another session's baseline is
    /// not reachable, and the same path under a different session is a different record.
    #[test]
    #[serial(triad_state)]
    fn records_are_keyed_by_session_and_canonical_path() {
        let state = use_state_dir("keyed");
        let work = scratch("keyed-work");
        let file = file_with(&work, "k.py", "a\n");
        let path = canonical_path(file.to_str().expect("utf8"));
        let mine = ("sess-1".to_string(), path.clone());
        let theirs = ("sess-2".to_string(), path.clone());

        persist_baseline(&mine, &Baseline::Content("mine\n".into()));
        persist_baseline(&theirs, &Baseline::Content("theirs\n".into()));

        assert_ne!(
            record(&state, "sess-1", &path),
            record(&state, "sess-2", &path),
            "two sessions on one path must not share a record"
        );
        assert_eq!(
            load_baseline(&mine).expect("readable"),
            Some(Baseline::Content("mine\n".into()))
        );
        assert_eq!(
            load_baseline(&theirs).expect("readable"),
            Some(Baseline::Content("theirs\n".into()))
        );
    }

    /// `Baseline::Absent` (the file did not exist) must NOT be persisted as an empty string:
    /// an empty body would read back as "the file held nothing", and the S28 gate counts
    /// differently for "absent" (every occurrence is new) than for "empty" (none is new).
    #[test]
    #[serial(triad_state)]
    fn an_absent_baseline_survives_and_stays_distinct_from_empty() {
        let state = use_state_dir("absent");
        let work = scratch("absent-work");
        let fresh = work.join("never-written.py");
        let path = canonical_path(fresh.to_str().expect("utf8"));
        let key = ("sess-absent".to_string(), path.clone());

        // `Absent` IS persisted: for a NEW file every occurrence is new, so dropping the
        // baseline would abstain on exactly the writes the gate must catch.
        persist_baseline(&key, &Baseline::Absent);
        assert!(
            record(&state, "sess-absent", &path).exists(),
            "Absent must leave a record — its existence is the signal"
        );
        assert_eq!(
            load_baseline(&key).expect("readable"),
            Some(Baseline::Absent),
            "and it must decode back as Absent, not as empty content"
        );

        // `Unknown` is the opposite: it must leave nothing, so it stays unknown across the
        // boundary instead of turning into "the file did not exist".
        persist_baseline(&key, &Baseline::Unknown);
        assert!(
            !record(&state, "sess-absent", &path).exists(),
            "Unknown must leave no record"
        );
        assert_eq!(load_baseline(&key).expect("readable"), None);
    }

    /// A new file written in ONE process keeps its baseline: the in-memory state is
    /// authoritative, and nothing is persisted for a file that did not exist. Regression
    /// guard — reading only the disk here abstained on every new file.
    #[test]
    #[serial(triad_state)]
    fn a_new_file_keeps_its_baseline_in_a_single_process() {
        let _state = use_state_dir("new-file");
        let work = scratch("new-file-work");
        let file = work.join("brand_new.py");
        // The file does NOT exist yet: this is the pre-write.
        assert!(arm("new-file-sess", file.to_str().expect("utf8")));
        std::fs::write(&file, "x = 1\n").expect("create");

        let taken = take_lookup("new-file-sess", file.to_str().expect("utf8"));
        assert!(
            taken.lookup.is_found(),
            "a new file's baseline must be found — every occurrence in it is new"
        );
        assert!(
            matches!(&taken.lookup, TriadLookup::Found(Baseline::Absent)),
            "and it must be Absent, not empty content: {:?}",
            taken.lookup
        );
        assert_eq!(taken.lookup.before(), None, "Absent means 'did not exist'");
    }

    /// (b) No pre-write at all → the gate DECLARES, it does not pass as clean.
    #[test]
    #[serial(triad_state)]
    fn a_missing_baseline_declares_its_reason() {
        let _state = use_state_dir("missing");
        let work = scratch("missing-work");
        let file = file_with(&work, "m.py", "z\n");
        let path = file.to_str().expect("utf8");

        let taken = take_lookup("never-armed", path);
        assert!(
            !taken.lookup.is_found(),
            "nothing armed this pair — there is no baseline to judge with"
        );
        assert!(taken.state.is_none(), "and no same-process snapshot either");
        let note = match &taken.lookup {
            TriadLookup::Absent(reason) => reason.note(),
            other => panic!("expected an Absent lookup, got {other:?}"),
        };
        assert!(
            note.contains("S28 abstained") && note.contains("NEW anti-patterns were not counted"),
            "the abstention must say the gate did not run: {note}"
        );
    }

    /// `take_lookup` prefers the same-process snapshot but still reports the baseline, so the
    /// caller gates correctly even when armed and judged in one process.
    #[test]
    #[serial(triad_state)]
    fn a_same_process_take_reports_both_lookup_and_state() {
        let _state = use_state_dir("same");
        let work = scratch("same-work");
        let file = file_with(&work, "s.py", "before\n");
        let path = file.to_str().expect("utf8");

        assert!(arm("same-proc", path));
        // Simulate the write.
        fs::write(&file, "after\n").expect("write");

        let taken = take_lookup("same-proc", path);
        assert!(taken.lookup.is_found(), "the in-memory baseline is found");
        assert!(
            taken.state.is_some(),
            "and it carries the BranchFs snapshot, which only this process has"
        );
        assert_eq!(taken.lookup.before(), Some("before\n"));

        // A different session on the same path finds nothing — the key is (session, path).
        let other = take_lookup("other-proc", path);
        assert!(
            !other.lookup.is_found(),
            "a different session gets no baseline"
        );
    }

    /// (c) An expired record declares `Expired` rather than silently reading as absent.
    ///
    /// The record is written by hand with a timestamp past [`TRIAD_TTL`], because sleeping 600 s
    /// is not a test.
    #[test]
    #[serial(triad_state)]
    fn an_expired_record_declares_expiry() {
        let state = use_state_dir("expired");
        let work = scratch("expired-work");
        let file = file_with(&work, "e.py", "old\n");
        let path = canonical_path(file.to_str().expect("utf8"));
        let key = ("sess-exp".to_string(), path.clone());

        let header = serde_json::json!({
            "v": DISK_VERSION,
            "at": unix_now().saturating_sub(TRIAD_TTL.as_secs() + 1),
            "path": path.display().to_string(),
            "session": "sess-exp",
            "len": 4,
        });
        let rec = record(&state, "sess-exp", &path);
        let mut body = serde_json::to_string(&header).expect("json");
        body.push('\n');
        body.push_str("old\n");
        fs::write(&rec, body).expect("write record");

        let err = load_baseline(&key).expect_err("an expired record must not be used");
        assert_eq!(err, TriadAbsent::Expired);
        assert!(
            err.note().contains("expired"),
            "the abstention names the reason: {}",
            err.note()
        );
    }

    /// (c) A record above [`TRIAD_DISK_MAX_BYTES`] declares `Oversized`.
    #[test]
    #[serial(triad_state)]
    fn a_record_above_the_cap_declares_oversized() {
        let state = use_state_dir("oversize");
        let work = scratch("oversize-work");
        let file = file_with(&work, "big.py", "x\n");
        let path = canonical_path(file.to_str().expect("utf8"));
        let key = ("sess-big".to_string(), path.clone());

        // A header that claims more than the cap: the record is refused on the header alone,
        // without allocating the claimed bytes.
        let header = serde_json::json!({
            "v": DISK_VERSION,
            "at": unix_now(),
            "path": path.display().to_string(),
            "session": "sess-big",
            "len": TRIAD_DISK_MAX_BYTES + 1,
        });
        let rec = record(&state, "sess-big", &path);
        let mut body = serde_json::to_string(&header).expect("json");
        body.push('\n');
        fs::write(&rec, body).expect("write record");

        let err = load_baseline(&key).expect_err("an oversized record must not be used");
        assert_eq!(err, TriadAbsent::Oversized);
        assert!(
            err.note().contains("TRIAD_DISK_MAX_BYTES"),
            "the abstention names the cap: {}",
            err.note()
        );
    }

    /// The writer refuses to persist an oversized baseline in the first place.
    #[test]
    #[serial(triad_state)]
    fn an_oversized_baseline_is_never_written() {
        let state = use_state_dir("no-write-big");
        let work = scratch("no-write-big-work");
        let file = file_with(&work, "huge.py", "x\n");
        let path = canonical_path(file.to_str().expect("utf8"));
        let key = ("sess-huge".to_string(), path.clone());

        let big = "a".repeat(TRIAD_DISK_MAX_BYTES + 1);
        persist_baseline(&key, &Baseline::Content(big));

        assert!(
            !record(&state, "sess-huge", &path).exists(),
            "a baseline above the cap must leave no record at all"
        );
        assert_eq!(load_baseline(&key).expect("readable"), None);
    }

    /// A record is written 0600 under a 0700 directory: it holds source text, so it must not be
    /// readable by anyone else on the machine.
    #[cfg(unix)]
    #[test]
    #[serial(triad_state)]
    fn the_record_is_private_to_its_user() {
        use std::os::unix::fs::PermissionsExt;
        let state = use_state_dir("perms");
        let work = scratch("perms-work");
        let file = file_with(&work, "p.py", "secret\n");
        let path = canonical_path(file.to_str().expect("utf8"));
        let key = ("sess-perm".to_string(), path.clone());

        persist_baseline(&key, &Baseline::Content("secret\n".into()));

        let dir_mode = fs::metadata(&state).expect("stat").permissions().mode() & 0o777;
        assert_eq!(
            dir_mode, 0o700,
            "the state dir must be 0700, got {dir_mode:o}"
        );
        let file_mode = fs::metadata(record(&state, "sess-perm", &path))
            .expect("stat")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            file_mode, 0o600,
            "the record must be 0600, got {file_mode:o}"
        );
    }

    /// A corrupt or foreign record abstains with a reason; it never reads back as content, and
    /// never degrades into "the file did not exist" (which would make every hit count as new).
    #[test]
    #[serial(triad_state)]
    fn a_corrupt_record_declines_instead_of_guessing() {
        let state = use_state_dir("corrupt");
        let work = scratch("corrupt-work");
        let file = file_with(&work, "c.py", "body\n");
        let path = canonical_path(file.to_str().expect("utf8"));
        let cases: Vec<(&str, Vec<u8>, &str)> = vec![
            ("no header", b"just text".to_vec(), "no header"),
            ("bad json", b"{not json}\nbody".to_vec(), "bad header"),
            (
                "path mismatch",
                format!(
                    "{{\"v\":{DISK_VERSION},\"at\":{at},\"path\":\"/somewhere/else.py\",\
                     \"session\":\"s\",\"len\":4}}\nbody",
                    at = unix_now()
                )
                .into_bytes(),
                "path mismatch",
            ),
            (
                "truncated body",
                format!(
                    "{{\"v\":{DISK_VERSION},\"at\":{at},\"path\":\"{p}\",\
                     \"session\":\"s\",\"len\":99}}\nbody",
                    at = unix_now(),
                    p = path.display()
                )
                .into_bytes(),
                "body 4 bytes",
            ),
        ];
        for (tag, body, expect) in cases {
            let rec = record(&state, "s", &path);
            fs::write(&rec, &body).expect("write record");
            let err = load_baseline(&("s".to_string(), path.clone()))
                .expect_err("a corrupt record must not be used");
            assert!(
                matches!(&err, TriadAbsent::Unreadable(m) if m.contains(expect)),
                "{tag}: expected an Unreadable mentioning {expect:?}, got {err:?}"
            );
        }
    }

    /// A record from a FUTURE version of the format is refused, not guessed at.
    #[test]
    #[serial(triad_state)]
    fn a_foreign_record_version_is_refused() {
        let state = use_state_dir("version");
        let work = scratch("version-work");
        let file = file_with(&work, "v.py", "body\n");
        let path = canonical_path(file.to_str().expect("utf8"));
        let rec = record(&state, "s", &path);
        let header = serde_json::json!({
            "v": DISK_VERSION + 99,
            "at": unix_now(),
            "path": path.display().to_string(),
            "session": "s",
            "len": 4,
        });
        let mut body = serde_json::to_string(&header).expect("json");
        body.push('\n');
        body.push_str("body");
        fs::write(&rec, body).expect("write record");

        let err = load_baseline(&("s".to_string(), path)).expect_err("refused");
        assert!(matches!(&err, TriadAbsent::Unreadable(m) if m.contains("version")));
    }

    /// Content with newlines and non-UTF-8 bytes round-trips: the header length, not a
    /// delimiter, is what separates metadata from text.
    #[test]
    #[serial(triad_state)]
    fn content_with_newlines_and_invalid_utf8_round_trips() {
        let _state = use_state_dir("bytes");
        let work = scratch("bytes-work");
        let file = file_with(&work, "b.bin", "old\n");
        let path = canonical_path(file.to_str().expect("utf8"));
        let key = ("sess-bytes".to_string(), path.clone());
        // A line that ends in a lone continuation byte: invalid UTF-8, and it contains no '\n'.
        let text = "line one\nline two \u{80} tail";
        persist_baseline(&key, &Baseline::Content(text.to_string()));

        let got = load_baseline(&key).expect("readable");
        match got {
            // The record stores text; invalid sequences survive as U+FFFD via lossy conversion,
            // which is the same guarantee `before_content` already gave.
            Some(Baseline::Content(back)) => {
                assert!(back.starts_with("line one\nline two "), "got {back:?}");
                assert!(back.ends_with(" tail"), "got {back:?}");
            }
            other => panic!("expected the baseline back, got {other:?}"),
        }
    }

    /// The state dir is per-user and reusable: a second call yields the same directory.
    #[test]
    #[serial(triad_state)]
    fn the_state_dir_is_stable_and_private() {
        // Deliberately NOT an isolated dir: this exercises the real resolution chain, so the
        // override must be cleared first (serial, so no other thread reads it meanwhile).
        // SAFETY: `#[serial]` guarantees exclusivity.
        unsafe { std::env::remove_var("TOURING_TRIAD_DIR") };
        let a = triad_state_dir().expect("state dir");
        let b = triad_state_dir().expect("state dir");
        assert_eq!(a, b, "the dir must be stable within a process");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&a).expect("stat").permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "got {mode:o}");
        }
    }

    /// `take_lookup` still works when the state directory cannot be used at all — it reports the
    /// degradation rather than panicking, and the same-process snapshot still gates.
    #[test]
    #[serial(triad_state)]
    fn an_unusable_state_dir_degrades_without_panicking() {
        let _state = use_state_dir("nodir");
        let work = scratch("nodir-work");
        let file = file_with(&work, "n.py", "before\n");
        let path = file.to_str().expect("utf8");

        // Point the store at a path that cannot be a directory: a regular FILE where a
        // directory is required. `create_dir_all` then fails and the store is unavailable.
        let blocker = work.join("blocked");
        fs::write(&blocker, "not a directory").expect("write");
        // SAFETY: `#[serial]` guarantees no other thread reads the env meanwhile.
        unsafe { std::env::set_var("TOURING_TRIAD_DIR", &blocker) };
        assert!(
            triad_state_dir().is_none(),
            "a file where a directory belongs must be reported, not worked around"
        );

        assert!(arm("nodir-proc", path));
        fs::write(&file, "after\n").expect("write");

        let taken = take_lookup("nodir-proc", path);
        assert!(
            taken.lookup.is_found(),
            "the in-memory baseline still serves — losing the disk copy degrades, never blocks"
        );
        assert!(taken.state.is_some());
    }
}

/// S28 — `Baseline` distinguishes a new file (nothing before) from a rewritten one, and keeps
/// "could not read" (`Unknown`) distinct from both.
#[cfg(test)]
mod s28_before_tests {
    use super::*;

    #[test]
    fn baseline_distinguishes_new_file_from_existing() {
        let dir = tempfile::tempdir().expect("tmp");
        let existing = dir.path().join("a.rs");
        std::fs::write(&existing, "old body\n").expect("write");
        let state = TriadState::pre_write(existing.to_str().expect("utf8")).expect("snapshot");
        std::fs::write(&existing, "new body\n").expect("rewrite");
        assert_eq!(
            state.baseline(),
            Baseline::Content("old body\n".to_string()),
            "a rewritten file carries its old text"
        );

        let fresh = dir.path().join("new.rs");
        let state = TriadState::pre_write(fresh.to_str().expect("utf8")).expect("snapshot");
        std::fs::write(&fresh, "x").expect("create");
        assert_eq!(
            state.baseline(),
            Baseline::Absent,
            "a new file had no content before"
        );
    }

    /// The three-way split is load-bearing: `Absent` means "did not exist" (every occurrence
    /// is new) and `Unknown` means "could not read" (the gate must abstain). Collapsing
    /// either into the other is how a lost baseline looked like a clean file.
    #[test]
    fn absent_and_unknown_are_not_the_same_answer() {
        assert_eq!(Baseline::Absent.as_before(), None);
        assert!(Baseline::Absent.is_known(), "Absent is a KNOWN pre-state");
        assert_eq!(Baseline::Unknown.as_before(), None);
        assert!(
            !Baseline::Unknown.is_known(),
            "Unknown is NOT a known pre-state — it must abstain"
        );
        assert_eq!(Baseline::Content("a\n".into()).as_before(), Some("a\n"));
    }
}

/// PT3 (plan 2026-09-25) — TRIAD advisory (D1-A) with two sessions writing in parallel:
/// crossings = 0, reversions = 0, cost measured. Emits one `PT3 {json}` line (run with
/// `--nocapture` to record it in the plan's data/pt3.json).
#[cfg(test)]
mod pt3_tests {
    use super::*;
    use std::time::Instant;

    const N: usize = 200;

    fn pct(sorted: &[u128], p: f64) -> u128 {
        sorted[((sorted.len() - 1) as f64 * p).round() as usize]
    }

    /// One session's loop: its own file (reversion check) and a file SHARED with the other
    /// session (crossing check — the old single slot let one session consume the other's
    /// snapshot, so its own `take` came back empty).
    fn session_loop(session: &str, dir: &Path) -> (usize, usize, Vec<u128>, Vec<u128>) {
        let (mut crossed, mut reverted) = (0usize, 0usize);
        let (mut arm_us, mut take_us) = (Vec::with_capacity(N), Vec::with_capacity(N));
        let shared = dir.join("shared.txt");
        let shared = shared.to_str().expect("utf8");
        for i in 0..N {
            let own = dir.join(format!("{session}-{}.txt", i % 8));
            let own = own.to_str().expect("utf8");
            let t = Instant::now();
            arm(session, own);
            arm_us.push(t.elapsed().as_micros());
            let written = format!("{session} write {i}\n");
            std::fs::write(own, &written).expect("write own");
            let t = Instant::now();
            let state = triad_store().remove(&(session.to_string(), canonical_path(own)));
            take_us.push(t.elapsed().as_micros());
            match state {
                Some(s) if s.written_path == own => {
                    let block = HookResponse::Block {
                        reason: "pt3".to_string(),
                        context: None,
                        event_name: None,
                    };
                    let _ = run_post_write(own, &s.baseline(), block);
                    if std::fs::read_to_string(own).expect("read own") != written {
                        reverted += 1;
                    }
                }
                _ => crossed += 1,
            }
            arm(session, shared);
            std::fs::write(shared, format!("{session} {i}\n")).expect("write shared");
            if triad_store()
                .remove(&(session.to_string(), canonical_path(shared)))
                .is_none_or(|s| s.written_path != shared)
            {
                crossed += 1;
            }
        }
        (crossed, reverted, arm_us, take_us)
    }

    #[test]
    fn pt3_two_sessions_parallel_writes_never_cross_or_revert() {
        let dir = tempfile::tempdir().expect("tmp");
        let (a, b) = std::thread::scope(|s| {
            let ha = s.spawn(|| session_loop("pt3-session-A", dir.path()));
            let hb = s.spawn(|| session_loop("pt3-session-B", dir.path()));
            (ha.join().expect("A"), hb.join().expect("B"))
        });
        let crossed = a.0 + b.0;
        let reverted = a.1 + b.1;
        let mut arm_us: Vec<u128> = a.2.into_iter().chain(b.2).collect();
        let mut take_us: Vec<u128> = a.3.into_iter().chain(b.3).collect();
        arm_us.sort_unstable();
        take_us.sort_unstable();
        println!(
            "PT3 {}",
            serde_json::json!({
                "sessions": 2, "writes_per_session": N, "crossed": crossed, "reverted": reverted,
                "arm_us": {"p50": pct(&arm_us, 0.5), "p99": pct(&arm_us, 0.99), "max": arm_us[arm_us.len() - 1]},
                "take_us": {"p50": pct(&take_us, 0.5), "p99": pct(&take_us, 0.99), "max": take_us[take_us.len() - 1]},
            })
        );
        assert_eq!(
            crossed, 0,
            "a session's take must always find its OWN snapshot"
        );
        assert_eq!(reverted, 0, "a Block must never restore the file (D1-A)");
    }
}
