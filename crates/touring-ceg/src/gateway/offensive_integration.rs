//! Re-exports from `touring-offensive` for use by the CEG gateway.
//!
//! ES1 P3 (2026-06-01) — added so that `Evidence::proof_report` and
//! `Execution<Verified>::prove_claim` can name SMT-solver types without
//! pulling the full `touring-offensive` crate path into the gateway's
//! public API. The gateway stays decoupled from the offensive crate's
//! internal module layout; if `touring_offensive::solver` is ever
//! reorganised, only this file changes.
//!
//! The stub contract (P1 honest scope) is preserved: when
//! `SolverBackendKind::Stub` is used, `prove_claim` returns
//! `ProofStatus::Void` and the gateway treats it as an explicit
//! "no real SMT performed" signal. No real Z3/CVC5 invocation happens
//! unless the corresponding feature is enabled at build time AND the
//! caller supplies a `SolverBackendKind::Z3` / `SolverBackendKind::Cvc5`.

pub use touring_offensive::solver::{
    ClaimContext, ClaimKind, ProofReport, ProofStatus, SolverBackendKind, prove_claim,
};

use touring_hooks_shared::action_signature::ActionSignature;

/// ES1 P4 (2026-06-02) — derive per-candidate `ClaimKind` from
/// `ActionSignature.intent_class`. Drives the per-candidate X3.5 PROVE
/// pre-filter (`filter_by_proof_per_candidate` in
/// `gateway/speculative.rs`).
///
/// **Conservative under-declaration policy** (false-negatives > false-positives,
/// same as `from_tool_payload_full` in ES3 P2). Unknown intents → `None`
/// (no claim derivable, identity transform in the filter).
///
/// # Intent → ClaimKind mapping (12 entries)
///
/// | intent_class                       | ClaimKind                                                          |
/// |------------------------------------|--------------------------------------------------------------------|
/// | `"cargo"` `"npm"` `"yarn"` `"pnpm"`| `Postcondition { predicate: "exit code == 0" }`                     |
/// | `"pytest"` `"jest"` `"mocha"` `"rspec"` | `Postcondition { predicate: "test suite passes" }`             |
/// | `"git"`                            | `Postcondition { predicate: "git exit code == 0" }`                |
/// | `"rs"` `"rust"`                    | `Postcondition { predicate: "rustc --edition 2024 succeeds" }`    |
/// | `"py"`                             | `Postcondition { predicate: "python -m py_compile succeeds" }`     |
/// | `"ts"` `"tsx"` `"js"` `"jsx"`      | `Postcondition { predicate: "tsc --noEmit succeeds" }`             |
/// | `"md"`                             | `None` (markdown edits — no static claim)                          |
/// | `"symbol"`                         | `Postcondition { predicate: "result non-empty" }`                  |
/// | `"free-text"`                      | `None` (search has no testable postcondition)                      |
/// | `"webfetch"` (and any `webfetch*`) | `Postcondition { predicate: "HTTP 200" }`                          |
/// | `"mcp-*"` (any `mcp-` prefix)      | `None` (MCP calls are external — no derivable claim)               |
/// | default (unknown)                  | `None`                                                             |
#[must_use]
pub fn claim_from_intent(signature: &ActionSignature) -> Option<ClaimKind> {
    match signature.intent_class.as_str() {
        "cargo" | "npm" | "yarn" | "pnpm" => Some(ClaimKind::Postcondition {
            predicate: "exit code == 0".to_owned(),
        }),
        "pytest" | "jest" | "mocha" | "rspec" => Some(ClaimKind::Postcondition {
            predicate: "test suite passes".to_owned(),
        }),
        "git" => Some(ClaimKind::Postcondition {
            predicate: "git exit code == 0".to_owned(),
        }),
        "rs" | "rust" => Some(ClaimKind::Postcondition {
            predicate: "rustc --edition 2024 succeeds".to_owned(),
        }),
        "py" => Some(ClaimKind::Postcondition {
            predicate: "python -m py_compile succeeds".to_owned(),
        }),
        "ts" | "tsx" | "js" | "jsx" => Some(ClaimKind::Postcondition {
            predicate: "tsc --noEmit succeeds".to_owned(),
        }),
        "md" => None,
        "symbol" => Some(ClaimKind::Postcondition {
            predicate: "result non-empty".to_owned(),
        }),
        "free-text" => None,
        s if s.starts_with("webfetch") => Some(ClaimKind::Postcondition {
            predicate: "HTTP 200".to_owned(),
        }),
        s if s.starts_with("mcp-") => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests_p4 {
    use super::*;
    use touring_hooks_shared::action_signature::ContextQualifier;

    fn sig_with_intent(intent: &str) -> ActionSignature {
        ActionSignature {
            tool_class: "Bash".to_owned(),
            intent_class: intent.to_owned(),
            context_qualifier: ContextQualifier::Plain,
        }
    }

    /// `cargo` → `Postcondition { "exit code == 0" }` (build/test runner postcondition).
    #[test]
    fn claim_from_intent_cargo_returns_exit_code_postcondition() {
        let claim = claim_from_intent(&sig_with_intent("cargo"));
        assert!(
            matches!(claim, Some(ClaimKind::Postcondition { ref predicate }) if predicate == "exit code == 0"),
            "cargo must derive exit-code postcondition; got {claim:?}"
        );
    }

    /// `rs` / `rust` → `Postcondition { "rustc --edition 2024 succeeds" }`.
    #[test]
    fn claim_from_intent_rust_returns_rustc_postcondition() {
        let claim = claim_from_intent(&sig_with_intent("rs"));
        assert!(
            matches!(claim, Some(ClaimKind::Postcondition { ref predicate }) if predicate == "rustc --edition 2024 succeeds"),
            "rs must derive rustc postcondition; got {claim:?}"
        );
    }

    /// `md` → `None` (markdown edits have no static claim).
    #[test]
    fn claim_from_intent_md_returns_none() {
        let claim = claim_from_intent(&sig_with_intent("md"));
        assert!(claim.is_none(), "md must be None; got {claim:?}");
    }

    /// Unknown intent → `None` (default under-declaration policy).
    #[test]
    fn claim_from_intent_unknown_returns_none() {
        let claim = claim_from_intent(&sig_with_intent("unknown"));
        assert!(
            claim.is_none(),
            "default under-declaration: unknown intents should map to None; got {claim:?}"
        );
    }
}
