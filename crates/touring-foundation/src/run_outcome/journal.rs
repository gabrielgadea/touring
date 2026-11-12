//! The run journal's PATH and its WRITER: `~/.claude/touring/run_journal.jsonl`,
//! the record one run contributes ([`RunLog`]), and the atomic single-write append.
//!
//! One JSON object per line. The WRITER is `touring run`
//! (`ctx_execute_tools::journal_run`); everything else reads (see `reader` for
//! how a line is read back and `recurrence` for the searches over the tail).
//! Before this module the path literal was spelled in nine files, each with its
//! own idea of how to append.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::outcome::{FailureKind, RunOutcome};

/// The journal under a user's home, relative to it.
const JOURNAL_RELATIVE_PATH: &str = ".claude/touring/run_journal.jsonl";

/// `PIPE_BUF` — the size up to which POSIX guarantees an `O_APPEND` write to a
/// regular file is atomic between writers (Linux: 4096).
pub(super) const PIPE_BUF: usize = 4096;

/// The canonical journal path under `home`. Pure — no env reading — so tests
/// can pass a tempdir.
#[must_use]
pub fn journal_path(home: &Path) -> PathBuf {
    home.join(JOURNAL_RELATIVE_PATH)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ─────────────────────────────────────────────────────────────────────────────
// Writing a run
// ─────────────────────────────────────────────────────────────────────────────

/// Everything one run contributes to a journal record, as plain borrowed data.
///
/// The record carries, beyond the outcome: the origin of the program body
/// (`source`/`file`), `harvest`, the presentation flags, the bytes the run left
/// in its private tmp, the ladder hint, and the body's identity (`code_hash`,
/// exact; two SimHash-64 versions, by similarity) — the fields the reuse and
/// repeat-gate rulers read.
#[derive(Debug, Clone)]
pub struct RunLog<'a> {
    /// This execution's identity (`run-<epoch_ms>-<pid>`).
    pub run_id: &'a str,
    /// The language as the caller wrote it.
    pub language: &'a str,
    /// Full stdout size in bytes (journaled as `code_hash_stdout_bytes`: the
    /// legacy name carries a LENGTH, never a hash).
    pub stdout_bytes: usize,
    /// How the process ended and the failure class, if any.
    pub outcome: RunOutcome,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Bytes the elision kept out of the context.
    pub bytes_elided: u64,
    /// sha256[:16] of the program body — exact identity.
    pub code_hash: &'a str,
    /// SimHash-64 (v1) of the normalized body.
    pub code_simhash: u64,
    /// SimHash-64 (v2) of the normalized body.
    pub code_simhash_v2: u64,
    /// Origin of the body: `file` | `inline` | `stdin`, when the caller declared it.
    pub source_kind: Option<&'a str>,
    /// The script path when the body came from a file.
    pub source_file: Option<&'a str>,
    /// The `--harvest <slug>` of the run, if any.
    pub harvest: Option<&'a str>,
    /// `--brief` was requested.
    pub brief: bool,
    /// The `--orchestrate` SDK was injected.
    pub orchestrate: bool,
    /// The ladder's top similar block `(entry_key, similarity)` at run time.
    pub hint: Option<(&'a str, f64)>,
    /// Bytes the run left in its private tmp dir.
    pub tmp_bytes: u64,
    /// The CEG advisory the run proceeded under, as `(reason, composite)`: a deny the
    /// sandbox made waivable (subprocess-only X6). Before RAWRUN it existed only in the
    /// printed envelope, so a run whose output the caller read raw left no trace of it.
    /// Journaled in EVERY presentation, and only when there is one: absent, the record
    /// keeps the exact bytes it always had.
    pub ceg_advisory: Option<(&'a str, f64)>,
}

impl RunLog<'_> {
    /// The journal record. `ts` (unix seconds) and `prior_same_hash` (how many
    /// times this exact `code_hash` ran before, from the journal tail) are
    /// injected so the record is a pure function of its inputs.
    ///
    /// Keys serialize in `serde_json`'s map order (sorted), the same bytes the
    /// pre-consolidation writer emitted — pinned against real journal lines in
    /// the tests, because existing journals on disk must keep parsing the same.
    #[must_use]
    pub fn to_json(&self, ts: u64, prior_same_hash: u32) -> Value {
        let mut record = json!({
            "ts": ts,
            "run_id": self.run_id,
            "language": self.language,
            "code_hash_stdout_bytes": self.stdout_bytes,
            "code_hash": self.code_hash,
            "code_simhash": format!("{:016x}", self.code_simhash),
            "code_simhash_v2": format!("{:016x}", self.code_simhash_v2),
            "prior_same_hash": prior_same_hash,
            "exit_code": self.outcome.exit.to_wire(),
            "duration_ms": self.duration_ms,
            "failure_kind": self.outcome.failure.map(FailureKind::as_str),
            "bytes_elided": self.bytes_elided,
            "source": self.source_kind,
            "file": self.source_file,
            "harvest": self.harvest,
            "brief": self.brief,
            "orchestrate": self.orchestrate,
            "hint": self.hint.map(|(key, sim)| json!({ "key": key, "sim": sim })),
            "tmp_bytes": self.tmp_bytes,
        });
        // Additive and optional: an old reader ignores the key, and a record without an
        // advisory is byte-identical to what the writer always produced.
        if let (Some((reason, composite)), Some(map)) = (self.ceg_advisory, record.as_object_mut())
        {
            map.insert(
                "ceg_advisory".to_string(),
                json!({ "composite": composite, "reason": reason }),
            );
        }
        record
    }
}

/// A handle on a journal file.
#[derive(Debug, Clone)]
pub struct Journal {
    pub(super) path: PathBuf,
}

impl Journal {
    /// A journal at an explicit path (tests, a copy of the journal).
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The canonical journal under `home` ([`journal_path`]).
    #[must_use]
    pub fn under_home(home: &Path) -> Self {
        Self::at(journal_path(home))
    }

    /// The canonical journal under `$HOME`; `None` when `HOME` is unset.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        std::env::var_os("HOME").map(|home| Self::under_home(Path::new(&home)))
    }

    /// The journal's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The whole journal as text (the rulers read it whole).
    ///
    /// # Errors
    /// The file is missing, unreadable, or not valid UTF-8.
    pub fn read_to_string(&self) -> std::io::Result<String> {
        std::fs::read_to_string(&self.path)
    }

    /// Journal one run and return how many times its exact `code_hash` appeared
    /// in the journal tail BEFORE this record (the recurrence the harvest nudge
    /// reads). Fail-open: any I/O problem is swallowed and reads as `0` —
    /// observability never blocks a run.
    pub fn record_run(&self, run: &RunLog<'_>) -> u32 {
        if let Some(dir) = self.path.parent()
            && std::fs::create_dir_all(dir).is_err()
        {
            return 0;
        }
        // Counted BEFORE the append so the record does not see itself.
        let prior = self.count_prior_same_hash(run.code_hash);
        self.append_json(&run.to_json(unix_now(), prior));
        prior
    }

    /// Append ONE JSON record as one line — one write, the whole record.
    ///
    /// Two calls (the body, then the `\n`) let two writers splice between them:
    /// measured on the real journal, 400 invalid lines in 47.102, 99 of them
    /// with 2+ `run_id` (ids 1 ms apart). With `O_APPEND` and the whole record in
    /// ONE call the kernel positions at the end and writes the block at once.
    /// Above `PIPE_BUF` that guarantee does not cover the size, so an exclusive
    /// `flock` serializes the writers (same pattern as the daemon's lock).
    ///
    /// Returns whether the whole record was written. Fail-open: callers may
    /// ignore it.
    pub fn append_json(&self, record: &Value) -> bool {
        let Ok(mut line) = serde_json::to_string(record) else {
            return false;
        };
        line.push('\n');
        let bytes = line.as_bytes();
        let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return false;
        };
        if bytes.len() > PIPE_BUF {
            let _guard = lock::JournalLock::acquire(&f);
            return f.write_all(bytes).is_ok();
        }
        // Up to PIPE_BUF: ONE write call (the record and its `\n` in one buffer)
        // — `write`, NOT `write_all`: `write_all` may emit several syscalls on a
        // partial write, and each syscall is a splice point.
        match f.write(bytes) {
            Ok(n) if n == bytes.len() => true,
            Ok(n) => {
                // Partial write: the rest goes under the lock — otherwise the
                // record is cut in the middle and the line becomes garbage.
                let _guard = lock::JournalLock::acquire(&f);
                f.write_all(&bytes[n..]).is_ok()
            }
            Err(_) => false,
        }
    }
}

/// The journal's exclusive `flock` (writers above `PIPE_BUF`).
///
/// `flock(2)` is declared locally — the same pattern as the daemon's lock — so
/// the module pulls no `libc` for one syscall. Fail-open: if the lock fails the
/// write goes ahead; the journal never blocks a run.
mod lock {
    #[cfg(unix)]
    use std::os::raw::c_int;
    #[cfg(unix)]
    use std::os::unix::io::{AsRawFd, RawFd};

    #[cfg(unix)]
    const LOCK_EX: c_int = 2;
    #[cfg(unix)]
    const LOCK_UN: c_int = 8;

    #[cfg(unix)]
    unsafe extern "C" {
        fn flock(fd: RawFd, operation: c_int) -> c_int;
    }

    /// Holds the lock until dropped. Holds the FD, not the `&File`: the guard has
    /// to live beside a mutable write on the same file.
    pub(super) struct JournalLock {
        #[cfg(unix)]
        fd: RawFd,
    }

    #[cfg(unix)]
    impl JournalLock {
        pub(super) fn acquire(file: &std::fs::File) -> Self {
            let fd = file.as_raw_fd();
            // SAFETY: `flock(2)` on a valid fd; the return is only observed.
            let rc = unsafe { flock(fd, LOCK_EX) };
            if rc != 0 {
                tracing::debug!("journal: flock failed; the write proceeds without the lock");
            }
            Self { fd }
        }
    }

    #[cfg(unix)]
    impl Drop for JournalLock {
        fn drop(&mut self) {
            // SAFETY: same fd — release (the OS also releases on close).
            unsafe {
                flock(self.fd, LOCK_UN);
            }
        }
    }

    #[cfg(not(unix))]
    impl JournalLock {
        pub(super) fn acquire(_file: &std::fs::File) -> Self {
            Self {}
        }
    }
}
