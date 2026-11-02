//! The two value types of a run outcome: how the process ended
//! ([`ExitStatus`]) and what class of failure, if any, that amounts to
//! ([`FailureKind`]) — plus the one place they are combined ([`RunOutcome`]).

use serde::{Serialize, Serializer};

/// Wire value of [`ExitStatus::TimedOut`] — what the sandbox executor reports
/// and what the journal stores in `exit_code`.
const WIRE_TIMED_OUT: i32 = -2;
/// Wire value of [`ExitStatus::NoExitCode`].
const WIRE_NO_EXIT_CODE: i32 = -1;

/// Orthogonal failure classes (E/A/M directive A5): a budget expiry is not an
/// exception, an abort is not a timeout, and a substrate death is neither.
/// Each class steers a DIFFERENT correction by the model.
///
/// This is the ONE taxonomy. The writer (`touring run`) and every reader
/// (KPI, SDK signal report, gates) share it, so the label a run is journaled
/// under and the label a ruler buckets it under cannot drift apart.
///
/// The serialized (kebab-case) label is the journal's `failure_kind` string and
/// the MCP envelope's `failure.kind`; both are on-disk/on-wire formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    /// The program itself failed: a non-zero exit with its own error output,
    /// or death by a signal.
    Exception,
    /// A budget (wall-clock or CPU busy time) expired and the process was killed.
    Timeout,
    /// The execution was denied/cancelled before or during the run (e.g. lock conflict).
    Abort,
    /// The subprocess died abnormally or could not be launched.
    ProcExit,
    /// The request itself was malformed (bad args / unsupported input).
    InvalidOutput,
    /// Captured output hit the sandbox byte cap and was cut — data was LOST,
    /// not merely elided; rerun producing less output or read the stored file.
    OutputLimit,
    /// READ side only: a label this build does not know (a journal written by a
    /// newer Touring), so a reader running on it never panics and never opens a
    /// bucket of its own. A run is never journaled as `Other`.
    Other,
}

impl FailureKind {
    /// Every class, in declaration order.
    pub const ALL: [Self; 7] = [
        Self::Exception,
        Self::Timeout,
        Self::Abort,
        Self::ProcExit,
        Self::InvalidOutput,
        Self::OutputLimit,
        Self::Other,
    ];

    /// The canonical label — the string the journal stores in `failure_kind`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exception => "exception",
            Self::Timeout => "timeout",
            Self::Abort => "abort",
            Self::ProcExit => "proc-exit",
            Self::InvalidOutput => "invalid-output",
            Self::OutputLimit => "output-limit",
            Self::Other => "other",
        }
    }

    /// Parse a journal `failure_kind` label. Total: an unknown label is
    /// [`FailureKind::Other`], never an error and never a bucket of its own.
    ///
    /// (`None` — a success — is the caller's `Option`; there is deliberately no
    /// "absent label" input here: the previous `from_str_opt(None)` answered
    /// `Exception`, a failure, for a success.)
    #[must_use]
    pub fn from_label(label: &str) -> Self {
        match label {
            "exception" => Self::Exception,
            "timeout" => Self::Timeout,
            "abort" => Self::Abort,
            "proc-exit" => Self::ProcExit,
            "invalid-output" => Self::InvalidOutput,
            "output-limit" => Self::OutputLimit,
            "other" => Self::Other,
            unknown => {
                tracing::debug!(label = %unknown, "unknown failure_kind, classified as Other");
                Self::Other
            }
        }
    }
}

/// How the process ended. "There was no process exit" is a variant, not a
/// magic integer: before this type, `-2` (timeout) and `-1` (no result) rode
/// through the executor interface as `i32` and each consumer re-decoded them.
///
/// The integer survives only at the serialization edge (the journal's
/// `exit_code`, the envelope's `exitCode`): [`ExitStatus::to_wire`] /
/// [`ExitStatus::from_wire`] are the ONLY place the two values are named.
///
/// Build a status from a wire integer with [`ExitStatus::from_wire`]: it decodes
/// `-1`/`-2` to the two variants below, so `Code(-1)`/`Code(-2)` (which
/// `to_wire` would emit as the sentinels) do not occur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExitStatus {
    /// The process exited by itself and reported this code (`0` = clean).
    Code(i32),
    /// No process exit: the wall-clock budget expired and the process was
    /// killed (wire `-2`).
    TimedOut,
    /// No exit code exists: the process was never launched, died by a signal,
    /// or the executor produced no result at all (wire `-1`).
    NoExitCode,
}

impl Default for ExitStatus {
    /// A clean exit (`Code(0)`) — what an `i32` field defaulted to.
    fn default() -> Self {
        Self::Code(0)
    }
}

impl ExitStatus {
    /// Decode a wire integer (journal `exit_code`, sandbox result).
    #[must_use]
    pub const fn from_wire(n: i32) -> Self {
        match n {
            WIRE_TIMED_OUT => Self::TimedOut,
            WIRE_NO_EXIT_CODE => Self::NoExitCode,
            code => Self::Code(code),
        }
    }

    /// Encode for the wire. `from_wire(to_wire(x)) == x` for every status
    /// `from_wire` can produce, and `to_wire(from_wire(n)) == n` for every `n`.
    #[must_use]
    pub const fn to_wire(self) -> i32 {
        match self {
            Self::Code(code) => code,
            Self::TimedOut => WIRE_TIMED_OUT,
            Self::NoExitCode => WIRE_NO_EXIT_CODE,
        }
    }

    /// The process exited with code `0`. A timeout or a missing exit is not zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        matches!(self, Self::Code(0))
    }

    /// The wall-clock budget killed the process.
    #[must_use]
    pub const fn is_timed_out(self) -> bool {
        matches!(self, Self::TimedOut)
    }
}

impl Serialize for ExitStatus {
    /// The wire integer — byte-identical to the `i32` it replaces.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i32(self.to_wire())
    }
}

/// What a finished run amounts to: how the process ended and the failure class
/// the executor assigned, if any.
///
/// # "Success" has exactly two meanings, named
///
/// Before this module the workspace spelled "did it succeed?" five ways. They
/// collapse into two predicates that differ on exactly ONE family of inputs —
/// a process that exited `0` yet carries a failure class (the sandbox cut its
/// output at the byte cap, or a file at the file-size cap, and the program
/// carried on and exited clean; measured on the real journal, 02/11/2026: 1 of
/// 49.258 journaled runs):
///
/// | exit | failure | [`is_success`](Self::is_success) | [`exited_zero`](Self::exited_zero) | [`has_failure`](Self::has_failure) |
/// |---|---|---|---|---|
/// | `Code(0)` | none | **yes** | yes | no |
/// | `Code(0)` | `Some(_)` | no | **yes** | yes |
/// | `Code(n≠0)` / `TimedOut` / `NoExitCode` | any | no | no | as the field says |
///
/// Which caller uses which, and why, is in the doc of each predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunOutcome {
    /// How the process ended.
    pub exit: ExitStatus,
    /// The failure class, `None` on a clean run.
    pub failure: Option<FailureKind>,
}

impl RunOutcome {
    /// Pair an exit status with the failure class (if any).
    #[must_use]
    pub const fn new(exit: ExitStatus, failure: Option<FailureKind>) -> Self {
        Self { exit, failure }
    }

    /// PROGRAM-level success: exited `0` AND carries no failure class.
    ///
    /// The envelope's `success` and the MCP result's `isError` — the answer to
    /// "did this program do what was asked, with its output intact?". An output
    /// cut by the byte cap exited `0` and is NOT a success here: data was lost.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.exit.is_zero() && self.failure.is_none()
    }

    /// PROCESS-level: exited `0`, whatever the failure class says.
    ///
    /// The journal rulers (KPI `code_mode_adherence`, the M1 gate), the trust
    /// ladder and the harvest eligibility ask only "did the process come back
    /// clean?" — a cut output still counts there, as it always has. Kept apart
    /// from [`is_success`](Self::is_success) on purpose: merging them would move
    /// the measured `success_rate` and the harvest set (differing input:
    /// `exit 0` + a failure class).
    #[must_use]
    pub const fn exited_zero(&self) -> bool {
        self.exit.is_zero()
    }

    /// The executor assigned a failure class, regardless of the exit code.
    ///
    /// The per-language `failure_count` of the signal report counts this: a
    /// "failed" run is one the executor classified, not one whose code is non-zero.
    #[must_use]
    pub const fn has_failure(&self) -> bool {
        self.failure.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The label table — every class, both directions, and serde. A swapped or
    /// misspelled label would re-file every journaled run under another class.
    const LABELS: [(FailureKind, &str); 7] = [
        (FailureKind::Exception, "exception"),
        (FailureKind::Timeout, "timeout"),
        (FailureKind::Abort, "abort"),
        (FailureKind::ProcExit, "proc-exit"),
        (FailureKind::InvalidOutput, "invalid-output"),
        (FailureKind::OutputLimit, "output-limit"),
        (FailureKind::Other, "other"),
    ];

    #[test]
    fn label_table_round_trips_in_both_directions() {
        for (kind, label) in LABELS {
            assert_eq!(kind.as_str(), label, "as_str({kind:?})");
            assert_eq!(FailureKind::from_label(label), kind, "from_label({label})");
        }
    }

    #[test]
    fn all_lists_every_class_exactly_once() {
        let listed: Vec<_> = FailureKind::ALL.iter().map(|k| k.as_str()).collect();
        let expected: Vec<_> = LABELS.iter().map(|(_, l)| *l).collect();
        assert_eq!(listed, expected);
    }

    #[test]
    fn serde_label_is_the_journal_label() {
        for (kind, label) in LABELS {
            assert_eq!(
                serde_json::to_value(kind).expect("serializes"),
                serde_json::json!(label),
                "serde label of {kind:?} must equal as_str (journal + envelope wire)"
            );
        }
    }

    #[test]
    fn an_unknown_label_is_other_never_a_failure_of_its_own() {
        for label in [
            "",
            "Exception",
            "TIMEOUT",
            "kind-que-nao-existe",
            "proc_exit",
            " timeout",
        ] {
            assert_eq!(
                FailureKind::from_label(label),
                FailureKind::Other,
                "{label:?} is not a canonical label"
            );
        }
    }

    #[test]
    fn exit_status_wire_decoding_names_the_two_sentinels() {
        assert_eq!(ExitStatus::from_wire(-2), ExitStatus::TimedOut);
        assert_eq!(ExitStatus::from_wire(-1), ExitStatus::NoExitCode);
        assert_eq!(ExitStatus::from_wire(0), ExitStatus::Code(0));
        assert_eq!(ExitStatus::from_wire(1), ExitStatus::Code(1));
        assert_eq!(ExitStatus::from_wire(141), ExitStatus::Code(141));
        assert_eq!(ExitStatus::from_wire(-3), ExitStatus::Code(-3));
        assert_eq!(ExitStatus::TimedOut.to_wire(), -2);
        assert_eq!(ExitStatus::NoExitCode.to_wire(), -1);
    }

    #[test]
    fn exit_status_wire_round_trip_is_the_identity_on_the_wire() {
        for n in (-300..=300).chain([i32::MIN, i32::MAX, 3_000_000]) {
            assert_eq!(ExitStatus::from_wire(n).to_wire(), n, "wire {n}");
        }
    }

    #[test]
    fn only_code_zero_is_zero() {
        assert!(ExitStatus::Code(0).is_zero());
        assert!(ExitStatus::default().is_zero());
        for s in [
            ExitStatus::Code(1),
            ExitStatus::Code(-3),
            ExitStatus::TimedOut,
            ExitStatus::NoExitCode,
        ] {
            assert!(!s.is_zero(), "{s:?}");
        }
        assert!(ExitStatus::TimedOut.is_timed_out());
        assert!(!ExitStatus::NoExitCode.is_timed_out());
        assert!(!ExitStatus::Code(0).is_timed_out());
        assert!(
            !ExitStatus::Code(-2).is_timed_out(),
            "a raw Code(-2) is not a wire decode"
        );
    }

    #[test]
    fn exit_status_serializes_as_the_wire_integer() {
        assert_eq!(
            serde_json::to_string(&ExitStatus::TimedOut).expect("ser"),
            "-2"
        );
        assert_eq!(
            serde_json::to_string(&ExitStatus::NoExitCode).expect("ser"),
            "-1"
        );
        assert_eq!(
            serde_json::to_string(&ExitStatus::Code(3)).expect("ser"),
            "3"
        );
    }

    /// The truth table of the module doc, over the whole domain (4 exits × 8
    /// failure states). The three predicates and the ONE input family where
    /// `is_success` and `exited_zero` disagree.
    #[test]
    fn success_predicates_match_the_documented_truth_table() {
        let exits = [
            ExitStatus::Code(0),
            ExitStatus::Code(1),
            ExitStatus::TimedOut,
            ExitStatus::NoExitCode,
        ];
        let failures: Vec<Option<FailureKind>> = std::iter::once(None)
            .chain(FailureKind::ALL.iter().copied().map(Some))
            .collect();
        let mut disagreements = Vec::new();
        for exit in exits {
            for failure in &failures {
                let o = RunOutcome::new(exit, *failure);
                assert_eq!(
                    o.is_success(),
                    exit == ExitStatus::Code(0) && failure.is_none(),
                    "{o:?}"
                );
                assert_eq!(o.exited_zero(), exit == ExitStatus::Code(0), "{o:?}");
                assert_eq!(o.has_failure(), failure.is_some(), "{o:?}");
                if o.is_success() != o.exited_zero() {
                    disagreements.push(o);
                }
            }
        }
        assert!(
            disagreements
                .iter()
                .all(|o| o.exit == ExitStatus::Code(0) && o.failure.is_some()),
            "the predicates may disagree ONLY on `exit 0` + a failure class: {disagreements:?}"
        );
        assert_eq!(
            disagreements.len(),
            FailureKind::ALL.len(),
            "one per failure class"
        );
    }

    #[test]
    fn a_default_outcome_is_a_clean_success() {
        let o = RunOutcome::default();
        assert!(o.is_success() && o.exited_zero() && !o.has_failure());
    }
}
