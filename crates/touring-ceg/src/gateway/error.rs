//! [`GatewayError`] — the exhaustive error type for the Code Execution Gateway
//! entry layer. Phase **P3.7** of CEG Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! The `X0..X7` pipeline itself is **infallible by construction**: every
//! typestate transition ([`Execution`](super::typestate::Execution)) is total,
//! and the X3 / X4 / X5 closures fold their own failures into evidence rather
//! than propagating them. So the gateway's only failure surface is the *entry
//! layer* — turning a raw tool call, a CLI argument list, or a hook input JSON
//! into something the pipeline can accept.
//!
//! `GatewayError` enumerates exactly those failure modes, and no others. It is
//! a **closed** enum: there is no `Internal` / `Other` catch-all, because the
//! pipeline cannot fail once entered — a property the typestate guarantees.

use std::fmt;

/// Every way the gateway entry layer can fail to produce a
/// [`GateDecision`](super::decision::GateDecision).
///
/// Exhaustive and closed — see the module documentation for why no catch-all
/// variant exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayError {
    /// X0 CAPTURE rejected the tool call: the tool does not run code (a
    /// `Read`, a `Glob`, …), so the gateway has nothing to gate.
    NotCodeBearing {
        /// The rejected tool name.
        tool: String,
    },
    /// The code or command body is empty — there is nothing to analyse.
    EmptyPayload,
    /// `touring exec` was invoked without a command to gate.
    MissingCommand,
    /// The pre-exec hook received an input JSON it could not read a tool name
    /// and a command / code body from.
    MalformedHookInput {
        /// What specifically could not be parsed.
        detail: String,
    },
}

impl GatewayError {
    /// A short, stable kind label — for JSON output, logs and metrics.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            GatewayError::NotCodeBearing { .. } => "not_code_bearing",
            GatewayError::EmptyPayload => "empty_payload",
            GatewayError::MissingCommand => "missing_command",
            GatewayError::MalformedHookInput { .. } => "malformed_hook_input",
        }
    }

    /// `true` when the error is *benign* — the gateway simply has nothing to
    /// do, and the caller should proceed (allow), not treat it as a fault.
    ///
    /// [`NotCodeBearing`](GatewayError::NotCodeBearing) and
    /// [`EmptyPayload`](GatewayError::EmptyPayload) are benign: a non-running
    /// tool call or an empty body is not a failure, just a no-op. A malformed
    /// invocation ([`MissingCommand`](GatewayError::MissingCommand),
    /// [`MalformedHookInput`](GatewayError::MalformedHookInput)) is not benign
    /// — the caller asked for a gate decision and could not be given one.
    #[must_use]
    pub fn is_benign(&self) -> bool {
        matches!(
            self,
            GatewayError::NotCodeBearing { .. } | GatewayError::EmptyPayload
        )
    }
}

impl fmt::Display for GatewayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GatewayError::NotCodeBearing { tool } => write!(
                f,
                "tool '{tool}' does not run code — nothing for the gateway to gate"
            ),
            GatewayError::EmptyPayload => {
                f.write_str("the code body is empty — nothing to analyse")
            }
            GatewayError::MissingCommand => {
                f.write_str("`touring exec` requires a command to gate")
            }
            GatewayError::MalformedHookInput { detail } => {
                write!(f, "malformed hook input: {detail}")
            }
        }
    }
}

impl std::error::Error for GatewayError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_label_is_stable_per_variant() {
        assert_eq!(
            GatewayError::NotCodeBearing {
                tool: "Read".to_owned()
            }
            .kind(),
            "not_code_bearing"
        );
        assert_eq!(GatewayError::EmptyPayload.kind(), "empty_payload");
        assert_eq!(GatewayError::MissingCommand.kind(), "missing_command");
        assert_eq!(
            GatewayError::MalformedHookInput {
                detail: "x".to_owned()
            }
            .kind(),
            "malformed_hook_input"
        );
    }

    #[test]
    fn benign_errors_are_no_ops_not_faults() {
        assert!(
            GatewayError::NotCodeBearing {
                tool: "Glob".to_owned()
            }
            .is_benign()
        );
        assert!(GatewayError::EmptyPayload.is_benign());
        // A malformed invocation is a real fault — the caller wanted a decision.
        assert!(!GatewayError::MissingCommand.is_benign());
        assert!(
            !GatewayError::MalformedHookInput {
                detail: "no tool_name".to_owned()
            }
            .is_benign()
        );
    }

    #[test]
    fn display_is_non_empty_and_names_the_cause() {
        let cases = [
            GatewayError::NotCodeBearing {
                tool: "Read".to_owned(),
            },
            GatewayError::EmptyPayload,
            GatewayError::MissingCommand,
            GatewayError::MalformedHookInput {
                detail: "bad json".to_owned(),
            },
        ];
        for err in cases {
            let rendered = err.to_string();
            assert!(!rendered.is_empty(), "Display must not be empty");
        }
        assert!(
            GatewayError::NotCodeBearing {
                tool: "Read".to_owned()
            }
            .to_string()
            .contains("Read")
        );
        assert!(
            GatewayError::MalformedHookInput {
                detail: "bad json".to_owned()
            }
            .to_string()
            .contains("bad json")
        );
    }

    #[test]
    fn is_a_std_error() {
        // `GatewayError` must be usable as a boxed `std::error::Error`.
        let boxed: Box<dyn std::error::Error> = Box::new(GatewayError::EmptyPayload);
        assert!(!boxed.to_string().is_empty());
    }

    #[test]
    fn equality_distinguishes_variants_and_payloads() {
        assert_eq!(GatewayError::EmptyPayload, GatewayError::EmptyPayload);
        assert_ne!(GatewayError::EmptyPayload, GatewayError::MissingCommand);
        assert_ne!(
            GatewayError::NotCodeBearing {
                tool: "Read".to_owned()
            },
            GatewayError::NotCodeBearing {
                tool: "Glob".to_owned()
            }
        );
    }
}
