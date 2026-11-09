//! The session's recent history: the ledgers the gates read and write, and the keys that
//! scope them (project root, actor, class, file).

use moka::sync::Cache;
use serde_json::Value;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use super::spec::*;

/// The session enters the key (30/08/2026): the ledgers live in the daemon, a
/// single process for N CC sessions — keyed by (project, tag), a deny in this
/// session carried commands from ANOTHER one (measured by the peer `analise-a2`: other sessions'
/// scratchpad paths in the remedy) and inflated the count of those who did not make the
/// burst. The cost: a burst distributed across 2 sessions no longer adds up — the
/// right design, each session answers for its own loop.
pub(super) fn write_run_key(project_root: &Path, session: &str, tag: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    project_root.hash(&mut h);
    session.hash(&mut h);
    tag.hash(&mut h);
    h.finish()
}

pub(super) type ExecBurstEntry = (u32, Vec<String>);

/// Session in the key for the same reason as [`write_run_key`]: the ledger belongs to the
/// daemon, and without it the window leaked between CC sessions of the same project.
pub(super) fn inspect_burst_key(project_root: &Path, session: &str, class: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}s3-inspect\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    session.hash(&mut hasher);
    class.hash(&mut hasher);
    hasher.finish()
}

/// Session in the key for the same reason as [`write_run_key`].
pub(super) fn exec_burst_key(project_root: &Path, session: &str, class: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}g10-exec\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    session.hash(&mut hasher);
    class.hash(&mut hasher);
    hasher.finish()
}

/// Accounts of the bypass budget of a scope, within the window.
///
/// There are TWO accounts because the two rules are distinct, and a single one conflated them:
/// measured at the debut, a bypass separated by a neutral command was denied as
/// "two in a row", which was not true.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BypassBudget {
    /// Bypasses stuck to one another; any other command zeroes it.
    pub(crate) strict_run: u32,
    /// Bypasses since the last `touring run`; only the sanctioned route zeroes it.
    pub(crate) since_route: u32,
}

/// Ledger key: the scope, as in the other gates of this module.
pub(super) fn bypass_key(project_root: &Path, actor: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}code-mode-bypass\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    actor.hash(&mut hasher);
    hasher.finish()
}

/// W1 S-1.2 — exact-repeat ledger: input-hash → (repeats, project mutation
/// epoch at first sight). TTL mirrors the anti-spam (300s).
/// G6 ledger key: the session AND the input. A blind retry is the same session repeating
/// what it already holds in context; another session running the same command has never
/// seen the result. Keyed by input alone, fresh sessions were denied as "retries" of each
/// other (26/09/2026 — the contract suite, one session per call, within 300 s).
pub(super) fn g6_key(
    project_root: &Path,
    session: &str,
    tool_name: &str,
    tool_input: &Value,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    project_root.hash(&mut hasher);
    session.hash(&mut hasher);
    tool_name.hash(&mut hasher);
    tool_input.to_string().hash(&mut hasher);
    hasher.finish()
}

/// W2 — burst ledger per (project, class): count + the REAL commands
/// accumulated (cap 8) — they become the program body in the deny remedy.
/// TTL = the window of the legacy counter (the same signal, now with memory).
/// Burst entry: `(count, commands, mutation epoch of the last insertion)`.
///
/// The epoch came in together with the similarity calibration: without it, a
/// re-read AFTER an Edit was denied as a repetition — and re-reading what
/// just changed is legitimate, not redundancy. It is the same protection G6 already
/// had (`mutation_epoch`), now extended to the new trigger. The test
/// `g6_mutacao_no_meio_reseta_a_repeticao` caught the regression.
pub(super) type BurstEntry = (u32, Vec<String>, u64);

pub(super) fn burst_key(project_root: &Path, actor: &str, class: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}g1-burst\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    actor.hash(&mut hasher);
    class.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn g3_read_key(project_root: &Path, session: &str, file: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}g3-read\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    session.hash(&mut hasher);
    file.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn g7_key(project_root: &Path, actor: &str, file: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}g7-target\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    actor.hash(&mut hasher);
    file.hash(&mut hasher);
    hasher.finish()
}

/// 30.4.97 (G7G3G9) — (mtime, size) of a file: what the Claude Code harness compares in
/// «File has been modified since read, either by the user or by a linter. Read it again before
/// attempting to write it.»
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct FileStamp {
    pub(super) mtime_ns: u128,
    pub(super) len: u64,
}

/// Every ledger the gates remember a session by. ONE instance serves the daemon for its whole
/// life (`GateState::global`); a test builds its own (`GateState::new`) and so shares nothing
/// with its neighbours — the isolation the old process-global statics had to fake with unique
/// project roots and `#[serial]` markers.
///
/// Each field is the former `fn name() -> &'static Cache<…>` accessor, with the same capacity
/// and the same window; the windows are named in [`super::spec`].
pub(crate) struct GateState {
    /// G6 — byte-identical repeats: (repeats, mutation epoch at first sight).
    pub(super) g6_seen: Cache<u64, (u32, u64)>,
    /// Per-project mutation epoch (every Edit/Write advances it).
    pub(super) mutation_epoch: Cache<String, u64>,
    /// G9 deny awaiting its follow-through (the next Edit/Write of the session).
    pub(super) pending_g9: Cache<String, ()>,
    /// G2 deny awaiting `pipefail` on the next Bash.
    pub(super) pending_g2: Cache<String, ()>,
    /// G1 burst per (project, actor, class).
    pub(super) burst_ledger: Cache<u64, BurstEntry>,
    /// G1 deny awaiting its continuation check (carries the denied class).
    pub(super) pending_g1: Cache<String, String>,
    /// G3 — Edits without a recent Read, per session.
    pub(super) g3_streak: Cache<String, u32>,
    /// G3 — (project, session, file) read recently.
    pub(super) g3_read_files: Cache<u64, ()>,
    /// G7 — re-reads of the same file.
    pub(super) g7_seen: Cache<u64, u32>,
    /// G7 — the (mtime, len) the last read saw.
    pub(super) g7_stamps: Cache<u64, FileStamp>,
    /// M3 — distinct files read per session.
    pub(super) m3_distinct_reads: Cache<String, u32>,
    /// G4 — the session located something (Grep/Glob) recently.
    pub(super) g4_last_locate: Cache<String, ()>,
    /// G5 — the streak of Edit/Write of the session.
    pub(super) g5_edit_streak: Cache<String, u32>,
    /// S5 — scripts written in the window, per (project, session, path).
    pub(super) written_scripts_ledger: Cache<u64, ()>,
    /// S5 — the write→run pairs counted (with the scripts they named).
    pub(super) write_run_pair_ledger: Cache<u64, (u32, Vec<String>)>,
    /// G10 — serial calls of one executor class.
    pub(super) exec_burst_ledger: Cache<u64, ExecBurstEntry>,
    /// S3 — inspections of one class in the window.
    pub(super) inspect_burst_ledger: Cache<u64, ExecBurstEntry>,
    /// G10 deny awaiting its follow-through.
    pub(super) pending_g10: Cache<String, ()>,
    /// G11 — the bypass budget.
    pub(super) bypass_ledger: Cache<u64, BypassBudget>,
}

fn ttl(secs: u64) -> Duration {
    Duration::from_secs(secs)
}

impl GateState {
    /// A fresh, empty store.
    pub(crate) fn new() -> Self {
        Self {
            g6_seen: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(SUGGESTION_TTL_SECS))
                .build(),
            mutation_epoch: Cache::builder()
                .max_capacity(CONFORMAL_TAU_MAX_PROJECTS)
                .build(),
            pending_g9: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(PENDING_DENY_TTL_SECS))
                .build(),
            pending_g2: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(PENDING_DENY_TTL_SECS))
                .build(),
            burst_ledger: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(CODE_MODE_WINDOW_SECS))
                .build(),
            pending_g1: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(PENDING_DENY_TTL_SECS))
                .build(),
            g3_streak: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(SESSION_STATE_TTL_SECS))
                .build(),
            g3_read_files: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(SESSION_STATE_TTL_SECS))
                .build(),
            g7_seen: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(SESSION_STATE_TTL_SECS))
                .build(),
            g7_stamps: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(SESSION_STATE_TTL_SECS))
                .build(),
            m3_distinct_reads: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(SESSION_STATE_TTL_SECS))
                .build(),
            g4_last_locate: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(G4_LOCATE_TTL_SECS))
                .build(),
            g5_edit_streak: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(SESSION_STATE_TTL_SECS))
                .build(),
            written_scripts_ledger: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(WRITE_RUN_WINDOW_SECS))
                .build(),
            write_run_pair_ledger: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(WRITE_RUN_WINDOW_SECS))
                .build(),
            exec_burst_ledger: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(EXEC_BURST_WINDOW_SECS))
                .build(),
            inspect_burst_ledger: Cache::builder()
                .max_capacity(CACHE_MAX_CAPACITY)
                .time_to_live(ttl(INSPECT_BURST_WINDOW_SECS))
                .build(),
            pending_g10: Cache::builder()
                .max_capacity(SESSION_LEDGER_CAPACITY)
                .time_to_live(ttl(PENDING_DENY_TTL_SECS))
                .build(),
            bypass_ledger: Cache::builder()
                .max_capacity(BYPASS_LEDGER_CAPACITY)
                .time_to_live(ttl(G11_WINDOW_SECS))
                .build(),
        }
    }

    /// The daemon's store: one per process, created on first use.
    pub(crate) fn global() -> &'static Self {
        static GLOBAL: OnceLock<GateState> = OnceLock::new();
        GLOBAL.get_or_init(Self::new)
    }
}

impl Default for GateState {
    fn default() -> Self {
        Self::new()
    }
}
