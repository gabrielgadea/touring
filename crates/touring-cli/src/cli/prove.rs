//! `cli-prove-claim` handler (Master Plan A.W2.P5 extraction).
//!
//! Mechanical extraction from `cli_handlers.rs`. Wraps the SMT-backed
//! `touring_offensive::solver::prove_claim` engine behind the standard
//! `cli-*` JSON handler contract. The `error_json` envelope helper lives
//! here because `cli_prove_claim` is its only consumer.

use crate::runtime::HookRuntime;

/// `cli-prove-claim` handler: parse a claim payload, dispatch to the
/// chosen SMT backend, and return a JSON proof report.
pub fn cli_prove_claim(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_offensive::solver::{
        ClaimContext, ClaimKind, SolverBackendKind, prove_claim as engine_prove_claim,
    };

    let claim_kind_str = payload
        .get("claim_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("Postcondition");

    // Build the ClaimKind from the claim_kind + payload. The shared
    // "claim_text" key covers the most common variants; type-safety
    // and memory-safety carry richer payloads.
    let claim = match claim_kind_str {
        "Postcondition" | "postcondition" => {
            let predicate = payload
                .get("claim_text")
                .or_else(|| payload.get("predicate"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if predicate.is_empty() {
                return error_json("Postcondition requires non-empty `claim_text`");
            }
            ClaimKind::Postcondition { predicate }
        }
        "LoopInvariant" | "loop_invariant" => {
            let var = payload
                .get("var")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let init = payload.get("init").and_then(|v| v.as_i64()).unwrap_or(0);
            let body_smtlib = payload
                .get("body_smtlib")
                .or_else(|| payload.get("claim_text"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if var.is_empty() || body_smtlib.is_empty() {
                return error_json("LoopInvariant requires `var` and `body_smtlib`");
            }
            ClaimKind::LoopInvariant {
                var,
                init,
                body_smtlib,
            }
        }
        "RefactorEquivalence" | "refactor_equivalence" => {
            let before = payload
                .get("before")
                .or_else(|| payload.get("claim_text"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let after = payload
                .get("after")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if before.is_empty() || after.is_empty() {
                return error_json("RefactorEquivalence requires `before` and `after`");
            }
            ClaimKind::RefactorEquivalence { before, after }
        }
        "TypeSafety" | "type_safety" => {
            let var = payload
                .get("var")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let sort = payload
                .get("sort")
                .and_then(|v| v.as_str())
                .unwrap_or("Int")
                .to_string();
            let lower = payload.get("lower").and_then(|v| v.as_i64()).unwrap_or(0);
            let upper = payload.get("upper").and_then(|v| v.as_i64()).unwrap_or(0);
            if var.is_empty() {
                return error_json("TypeSafety requires `var`");
            }
            ClaimKind::TypeSafety {
                var,
                sort,
                lower,
                upper,
            }
        }
        "MemorySafety" | "memory_safety" => {
            let ptr = payload
                .get("ptr")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let base = payload
                .get("base")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let offset_lo = payload
                .get("offset_lo")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let offset_hi = payload
                .get("offset_hi")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if ptr.is_empty() || base.is_empty() {
                return error_json("MemorySafety requires `ptr` and `base`");
            }
            ClaimKind::MemorySafety {
                ptr,
                base,
                offset_lo,
                offset_hi,
            }
        }
        other => {
            return error_json(&format!("unknown claim_kind: {}", other));
        }
    };

    let backend_str = payload
        .get("backend")
        .and_then(|v| v.as_str())
        .unwrap_or("auto");
    let backend = match backend_str {
        "Stub" | "stub" | "auto" => SolverBackendKind::Stub,
        "Z3" | "z3" => SolverBackendKind::Z3,
        "Cvc5" | "CVC5" | "cvc5" => SolverBackendKind::Cvc5,
        other => {
            return error_json(&format!("unknown backend: {}", other));
        }
    };

    let variables = payload
        .get("variables")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.to_string();
                    let sort = item
                        .get("sort")
                        .and_then(|s| s.as_str())
                        .unwrap_or("Int")
                        .to_string();
                    Some((name, sort))
                })
                .collect()
        })
        .unwrap_or_default();
    let depth_budget = payload
        .get("depth_budget")
        .and_then(|v| v.as_u64())
        .unwrap_or(8) as u32;
    let ctx = ClaimContext {
        variables,
        depth_budget,
    };

    let report = engine_prove_claim(&claim, &ctx, backend);

    let backend_used_str = match report.backend_used {
        SolverBackendKind::Stub => "Stub",
        SolverBackendKind::Z3 => "Z3",
        SolverBackendKind::Cvc5 => "Cvc5",
    };
    let counterexample_json = report
        .counterexample
        .as_ref()
        .map(|ce| serde_json::to_value(ce).unwrap_or(serde_json::Value::Null));
    let proof_report_json = serde_json::json!({
        "status": report.status.to_string(),
        "counterexample": counterexample_json,
        "model": report.model,
        "backend_used": backend_used_str,
        "latency_ms": report.latency_ms,
        "claim_text": report.claim_text,
        "smtlib": report.smtlib,
        "timestamp_unix_ms": report.timestamp_unix_ms,
    });
    let out = serde_json::json!({
        "status": "completed",
        "result": {
            "proof_report": proof_report_json,
            "claim_kind": claim_kind_str,
            "backend_selected": backend_used_str,
        }
    });
    serde_json::to_string(&out)
        .unwrap_or_else(|_| r#"{"status":"error","error":"serialization failed"}"#.to_string())
}

/// Build a JSON error envelope with the standard shape used by the
/// `cli-*` handlers. Centralizes the error format so callers don't
/// duplicate the boilerplate.
fn error_json(msg: &str) -> String {
    let out = serde_json::json!({
        "status": "error",
        "error": msg,
    });
    serde_json::to_string(&out)
        .unwrap_or_else(|_| r#"{"status":"error","error":"serialization failed"}"#.to_string())
}

#[cfg(test)]
mod cli_prove_claim_e2e {
    //! P4-S3: 1 E2E test for `cli_prove_claim`.
    //!
    //! Verifies the full path: JSON payload → `cli_prove_claim` →
    //! JSON output → assertions on proof_report. The stub backend
    //! is the only one wired in P1, so we exercise that path.

    use super::*;

    #[test]
    fn cli_prove_claim_stub_postcondition_void() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut rt = HookRuntime::new(tmp.path()).expect("HookRuntime::new");
        let payload = serde_json::json!({
            "claim_kind": "Postcondition",
            "claim_text": "x > 0",
            "variables": [{"name": "x", "sort": "Int"}],
            "backend": "Stub"
        });
        let out = cli_prove_claim(&mut rt, &payload);
        let v: serde_json::Value =
            serde_json::from_str(&out).expect("handler output must be valid JSON");
        assert_eq!(v["status"], "completed", "{}", out);
        let r = &v["result"];
        assert_eq!(r["claim_kind"], "Postcondition");
        assert_eq!(r["backend_selected"], "Stub");
        let pr = &r["proof_report"];
        // CRITICAL: stub MUST return Void, not Sat.
        assert_eq!(pr["status"], "void", "Stub backend must yield Void status");
        assert_eq!(pr["backend_used"], "Stub");
        assert!(
            pr["claim_text"]
                .as_str()
                .unwrap_or("")
                .contains("Postcondition")
        );
        assert!(
            pr["smtlib"].as_str().unwrap_or("").contains("x > 0"),
            "smtlib echoes predicate"
        );
    }
}
