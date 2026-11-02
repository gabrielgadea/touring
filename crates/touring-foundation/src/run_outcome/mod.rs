//! The outcome of one `touring run`, and the journal that remembers it —
//! described ONCE.
//!
//! Before this module the same fact was spelled in several places and decoded in
//! many more:
//!
//! - **two failure taxonomies** — `RunFailureKind` (what the executor assigns)
//!   and `journal::FailureKind` (what the readers bucket by) — with different
//!   docs for the same labels;
//! - **a `-2` sentinel** that rode through the executor interface as an `i32`
//!   meaning "not a process exit", re-decoded by each consumer;
//! - **five spellings of "success"** (`exit == 0`; `exit == 0 && no failure`;
//!   `failure_kind.is_some()`; …) — see [`RunOutcome`] for the two that remain
//!   and the one input family where they differ;
//! - **the journal path** built in nine files and its lines parsed in five, each
//!   with its own idea of a malformed line.
//!
//! Now:
//!
//! | what | where |
//! |---|---|
//! | the failure taxonomy (one) | [`FailureKind`] |
//! | how the process ended, "no exit" in the type | [`ExitStatus`] |
//! | what a finished run amounts to, the success predicates | [`RunOutcome`] |
//! | the journal: path, the writer, the tail searches | [`Journal`], [`RunLog`] |
//! | the journal: reading a line (lenient rulers / strict aggregate) | [`classify_line`], [`scan_entries`] |
//!
//! # On-disk compatibility
//!
//! The journal's bytes did not change: the labels ([`FailureKind::as_str`]), the
//! wire exit codes ([`ExitStatus::to_wire`]: `-2` timeout, `-1` no exit code) and
//! the record layout ([`RunLog::to_json`]) are pinned by tests against real
//! journal lines, because journals already on disk must keep parsing the same.
//!
//! The integers `-1`/`-2` exist ONLY at that serialization edge
//! ([`ExitStatus::from_wire`] / [`ExitStatus::to_wire`]).

mod journal;
mod outcome;
mod reader;
mod recurrence;

pub use journal::{Journal, RunLog, journal_path};
pub use outcome::{ExitStatus, FailureKind, RunOutcome};
pub use reader::{
    EntryScan, JournalEntry, JournalHint, Line, RunRecord, classify_line, count_runs, scan_entries,
};
pub use recurrence::{PriorSearch, PriorSimilar, SimhashVersion};

#[cfg(test)]
mod replay;
#[cfg(test)]
mod tests;
