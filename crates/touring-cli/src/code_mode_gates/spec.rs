//! The contract in numbers: every window, threshold and budget of the code-mode gates.
//!
//! Workspace `CLAUDE.md` §10 documents the same table for the humans; the unit test
//! `the_declared_windows_are_the_windows_the_ledgers_enforce` pins this file to the ledgers
//! (`moka` exposes each cache's `time_to_live`), so a window cannot drift from its declaration
//! without a red test.

pub(crate) const SUGGESTION_TTL_SECS: u64 = 300;

pub(crate) const CACHE_MAX_CAPACITY: u64 = 4096;

/// Cap on projects with a conformal τ memoized simultaneously. A daemon serves
/// few projects per window; the limit exists so the cache does not grow
/// indefinitely in a process that lives much longer than any session.
pub(crate) const CONFORMAL_TAU_MAX_PROJECTS: u64 = 64;

/// Window length for the repeated-scan counter. Shorter than the suggestion TTL
/// so the "repeated scanning" signal reflects the *current* burst of activity.
pub(crate) const CODE_MODE_WINDOW_SECS: u64 = 180;

/// S4 — execution-burst window: 600s. The pace of a test suite
/// (minutes per call) is slower than that of an inspection burst (G1's
/// 180s) — a short window would never see the 10th step.
pub(super) const EXEC_BURST_WINDOW_SECS: u64 = 600;

/// S4 — trigger of the homogeneous execution burst. It was born at 10 (R4: real sessions
/// went to 30-75); **tightened 29/08 (Gabriel's order): 10→5** — 5 has already paid
/// 4 round-trips, and the legitimate occasional use (2-3 debug pytest runs) stays out.
pub(super) const EXEC_BURST_DENY_AT: u32 = 5;

/// S5 (29/08) — window of the write→run pair: the same as the execution burst.
pub(super) const WRITE_RUN_WINDOW_SECS: u64 = 600;

/// S5 — trigger of the write→run pair: writing a script and running it right after
/// is the execute-observe loop by definition (28 pairs in the 60-turn run of
/// `analise`, all invisible because each half is individually legitimate).
/// **Tightened 29/08 (Gabriel's order): 3→2** — the 1st pair always passes (creating and
/// testing ONE script is legitimate); the 2nd in the same window IS already the loop. The low
/// threshold does not tax the common case: pytest/gates/tools never match — the path
/// they execute was not written via `cat >`/`tee` in the window.
pub(super) const WRITE_RUN_DENY_AT: u32 = 2;

/// S3 — window of the INSPECTION burst, derived from measurement and not from intuition.
///
/// Measured over 115 transcripts since 01/08/2026 (`scripts/s3_burst_distribution.py`),
/// over the 1,118 calls of the classes that the `code` mode denied by a fixed list:
///
/// | window | isolated | volume in burst ≥2 |
/// |---|---|---|
/// | 60s  | 41.6% | 58.4% |
/// | 120s | 32.1% | 67.9% |
/// | **300s** | **22.5%** | **77.5%** |
/// | 600s | 17.8% | 82.2% |
///
/// 300s is the knee: from 300 to 600 the window DOUBLES to buy 4.7 points of
/// volume, and every extra second is a chance to group two inspections that have
/// nothing to do with each other — a burst false positive is exactly the
/// friction on the common case that S3 exists to remove.
pub(super) const INSPECT_BURST_WINDOW_SECS: u64 = 300;

/// S3 — the 2nd warns (D, 09/10/2026: it used to deny); the 1st runs untouched.
///
/// The measurement is unambiguous at the threshold: with a 300s window, 77.5% of the volume of the
/// inspection classes is in bursts of size ≥ 2. Letting the isolated one pass
/// gives up 22.5% of the denies and keeps the other 77.5% — the opposite of what the
/// fixed list did, which charged 100% of the friction to capture the same 77.5%.
/// It is the rule that DeepSeek itself documented and that T3-B already declared without
/// ever being able to apply: *"forcing every edit through a program taxes the
/// common case"*.
pub(super) const INSPECT_BURST_ADVISE_AT: u32 = 2;

/// Budget window, aligned with G10's (the burst execution gate).
pub(super) const G11_WINDOW_SECS: u64 = 600;

/// Bypasses STRICTLY consecutive tolerated. `1` means: the first passes, the
/// second right after the first does not.
pub(super) const G11_MAX_STRICT_RUN: u32 = 1;

/// Bypasses tolerated since the last use of the sanctioned route, even if
/// interleaved with neutral commands. `2` means: the third does not pass.
pub(super) const G11_MAX_SINCE_ROUTE: u32 = 2;

/// PER-COMMAND bypass token: it travels in the command itself (the
/// `GIT_DESTRUCTIVE_OK` pattern — each use is a decision, never an exported state).
pub(super) const GATE_BYPASS_TOKEN: &str = "TOURING_GATE_OK=1";

/// Longest single-line command a reason inlines as a ready-to-paste bypass line; beyond it the
/// reason points at the command it already shows instead of repeating it.
pub(super) const READY_LINE_MAX_CHARS: usize = 600;

/// W2 S-2.1 — repeated inspection of the SAME class in the window denies (the legacy
/// advisory speaks one earlier). 90% proxy precision measured at the 4th (51/55 sessions);
/// **tightened 29/08 (Gabriel's order): 4→3** — at the 3rd the pattern is already unambiguous
/// (the same rationale as CODE_MODE_SCAN_THRESHOLD) and the cost of an FP is still 1 bypass.
pub(super) const G1_DENY_AT: u32 = 3;

/// W2 S-2.3 — live precision floor below which G1 demotes itself to
/// advisory (F7: demote is code, not a meeting)…
pub(super) const G1_DEMOTE_FLOOR: f64 = 0.70;

/// …provided there is real volume: a handful of initial events never demotes.
pub(super) const G1_DEMOTE_MIN_EVENTS: u64 = 100;

/// Files above this are not scanned for the Read-window hint (the deny path stays cheap).
pub(super) const G3_WINDOW_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Context lines kept on each side of the Edit's `old_string` in the Read-window hint.
pub(super) const G3_WINDOW_CONTEXT_LINES: usize = 5;

/// Files shorter than this are read whole: a window would save nothing.
pub(super) const G3_WINDOW_MIN_FILE_LINES: usize = 80;

/// M3 — how many distinct files trigger the delegation advisory.
pub(super) const M3_DELEGATION_AT: u32 = 10;

/// Total budget, in chars, of the program body that the remedy hands over.
pub(super) const G1_BODY_BUDGET: usize = 3000;

/// Threshold of the similarity calibration (Gabriel, 26/08/2026).
pub(crate) const G1_SIMILARITY_AT: f64 = 0.5;

// ── Ledger sizing and the windows that were literals inside the old accessors ────────────

/// Capacity of the small per-session ledgers (pending denies, streaks, M3, G4, G5).
pub(super) const SESSION_LEDGER_CAPACITY: u64 = 1024;
/// A pending deny waits this long for its follow-through (G1/G2/G9/G10 `followed` metric).
pub(super) const PENDING_DENY_TTL_SECS: u64 = 60;
/// G3 streak and reads, G7 counts and stamps, M3 and G5: one hour of session memory.
pub(super) const SESSION_STATE_TTL_SECS: u64 = 3600;
/// G4: a Grep/Glob this recent counts as "the session located something".
pub(super) const G4_LOCATE_TTL_SECS: u64 = 120;
/// Capacity of the G11 bypass budget ledger.
pub(super) const BYPASS_LEDGER_CAPACITY: u64 = 256;
