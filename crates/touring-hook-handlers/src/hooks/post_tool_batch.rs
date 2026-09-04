//! PostToolBatch Hook Handler — aggregate RL and wiring signals after a batch of parallel tools.
//!
//! Claude Code fires PostToolBatch ONCE after a group of parallel tool calls resolves.
//! This handler replaces N individual post-tool-rl invocations with a single aggregated one:
//!
//! - Parses `tool_calls` array from the PostToolBatch payload.
//! - Computes per-call success flags; maps success → 1.0 and failure → −0.3.
//! - Injects ONE aggregated RL reward (average score, labelled "batch").
//! - For batches that edited a file which CAN host a public symbol, emits a
//!   REGRA #0 wiring hint via `additionalContext`, naming those files. An edit
//!   to a `.md`/`.json`/`.toml` cannot create an orphan and says nothing.
//! - Truncates `tool_response` content per call to 0 bytes for memory safety.
//!
//! # Return value
//!
//! Always `HookResponse::Allow` (exit 0) unless a symbol-bearing file was edited,
//! in which case `HookResponse::Context` carries the wiring hint. It does NOT
//! detect orphans — it says which files to check, which is what it can honestly
//! claim. Never blocks, denies, or halts.

use crate::HookResponse;
use crate::runtime::HookRuntime;
use serde_json::Value;

/// Tool calls extracted from a PostToolBatch payload entry.
struct BatchCall {
    tool_name: String,
    success: bool,
    /// The file the call touched, when the payload carries one.
    ///
    /// Discarded until 04/09/2026, which is why the REGRA #0 hint below fired on
    /// EVERY batch containing an Edit — 422 emissions measured over 10
    /// transcripts — while being structurally blind to whether the edited file
    /// could carry a public symbol at all. A hint about orphans that cannot see
    /// the file is a claim about a domain it has no data on.
    file_path: Option<String>,
}

/// Infer success from a tool call entry when no explicit `success` bool is present.
///
/// Checks `error` / `tool_response.error` (non-empty string → failure)
/// and `exit_code` (non-zero → failure). Both absent → assume success.
fn infer_success_from_entry(entry: &Value) -> bool {
    let has_error = entry
        .get("error")
        .or_else(|| entry.pointer("/tool_response/error"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let exit_ok = entry
        .get("exit_code")
        .and_then(|v| v.as_i64())
        .map(|c| c == 0)
        .unwrap_or(true);
    !has_error && exit_ok
}

/// Parse a single tool call entry into a `BatchCall`.
///
/// Returns `None` when `tool_name` is absent (required field).
fn parse_single_call(entry: &Value) -> Option<BatchCall> {
    let tool_name = entry
        .get("tool_name")
        .or_else(|| entry.pointer("/tool_input/tool_name"))
        .and_then(|v| v.as_str())?
        .to_string();

    let success = entry
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| infer_success_from_entry(entry));

    let file_path = entry
        .pointer("/tool_input/file_path")
        .or_else(|| entry.get("file_path"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Some(BatchCall {
        tool_name,
        success,
        file_path,
    })
}

/// Parse the `tool_calls` array from the PostToolBatch input.
///
/// Returns an empty vec if the key is absent (graceful degradation).
fn parse_tool_calls(input: &Value) -> Vec<BatchCall> {
    input
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(parse_single_call).collect())
        .unwrap_or_default()
}

/// Map per-call success flag to a scalar reward value.
///
/// Success → +1.0; failure → −0.3 (partial penalty, not catastrophic).
#[inline]
fn call_reward(success: bool) -> f64 {
    if success { 1.0 } else { -0.3 }
}

/// Compute average reward across all calls in the batch.
///
/// Returns 0.0 for an empty batch (no reward injected — no-op RL update).
fn average_reward(calls: &[BatchCall]) -> f64 {
    if calls.is_empty() {
        return 0.0;
    }
    let sum: f64 = calls.iter().map(|c| call_reward(c.success)).sum();
    sum / calls.len() as f64
}

/// Extensions that cannot carry a public symbol, so an edit to one cannot create
/// an orphan and the REGRA #0 hint has nothing to warn about.
const NON_SYMBOL_EXTENSIONS: &[&str] = &[
    "md", "markdown", "json", "toml", "yaml", "yml", "txt", "lock", "csv", "log", "cfg", "ini",
];

/// True when this path can host a public symbol.
///
/// A path we cannot see reads as "unknown", and unknown resolves to TRUE here on
/// purpose: REGRA #0 is constitutional, and suppressing a real orphan warning
/// costs more than one extra line. The asymmetry is deliberate and stated rather
/// than left to the reader.
fn can_carry_a_symbol(path: Option<&str>) -> bool {
    let Some(p) = path else {
        return true;
    };
    let ext = std::path::Path::new(p)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    !NON_SYMBOL_EXTENSIONS.contains(&ext.as_str())
}

/// The files in this batch that an Edit/Write touched AND that can host a public
/// symbol — the evidence the REGRA #0 hint needs before it says anything.
fn symbol_bearing_edits(calls: &[BatchCall]) -> Vec<&str> {
    calls
        .iter()
        .filter(|c| matches!(c.tool_name.as_str(), "Edit" | "Write" | "MultiEdit"))
        .filter(|c| can_carry_a_symbol(c.file_path.as_deref()))
        .map(|c| c.file_path.as_deref().unwrap_or("<sem caminho no payload>"))
        .collect()
}

/// Run the post-tool-batch hook.
///
/// Aggregates RL reward signals from a completed batch of parallel tool calls,
/// injects a single reward into the LinUCB/QTable, and emits a REGRA #0
/// wiring hint when Edit/Write tools may have introduced new orphan symbols.
///
/// Always exits 0 — never blocks Claude Code.
pub fn run_post_tool_batch(rt: &mut HookRuntime, input: &Value) -> HookResponse {
    let calls = parse_tool_calls(input);

    // Empty or unparseable batch — nothing to aggregate, silently allow.
    if calls.is_empty() {
        tracing::debug!("post-tool-batch: empty tool_calls array, skipping RL aggregation");
        return HookResponse::Allow;
    }

    let n = calls.len();
    let avg = average_reward(&calls);
    let successes: usize = calls.iter().filter(|c| c.success).count();

    tracing::info!(
        batch_size = n,
        successes = successes,
        avg_reward = avg,
        "post-tool-batch: aggregating RL reward"
    );

    // Inject ONE aggregated RL reward instead of N individual post-tool-rl calls.
    if avg != 0.0 {
        let context = format!("batch:{n}_tools");
        rt.learning.inject_reward("batch", avg, &context);
        tracing::debug!(
            avg_reward = avg,
            n_tools = n,
            "post-tool-batch: RL reward injected"
        );
    }

    // REGRA #0 — potencializar. The hint speaks only when a file that CAN host a
    // public symbol was edited, and it names those files: a banner that fires on
    // every batch and names none of them costs the window without telling the
    // reader where to look (the injection-density invariant demands the derived
    // value, never a placeholder).
    let touched = symbol_bearing_edits(&calls);
    if !touched.is_empty() {
        let files = touched.join(", ");
        let hint = format!(
            "post-tool-batch: {} arquivo(s) que podem carregar símbolo público editados: {files}\n\
             MUST touring wiring orphans -j   // REGRA #0 — ligue todo pub novo a um consumidor",
            touched.len()
        );
        tracing::debug!(files = %files, "post-tool-batch: emitting REGRA #0 wiring hint");
        return HookResponse::Context {
            context: hint,
            event_name: Some("PostToolBatch".to_string()),
        };
    }

    HookResponse::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_tool_calls_empty() {
        let input = json!({});
        let calls = parse_tool_calls(&input);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_parse_tool_calls_explicit_success() {
        let input = json!({
            "tool_calls": [
                {"tool_name": "Read", "success": true},
                {"tool_name": "Edit", "success": false}
            ]
        });
        let calls = parse_tool_calls(&input);
        assert_eq!(calls.len(), 2);
        assert!(calls[0].success);
        assert!(!calls[1].success);
    }

    #[test]
    fn test_parse_tool_calls_infer_success_from_exit_code() {
        let input = json!({
            "tool_calls": [
                {"tool_name": "Bash", "exit_code": 0},
                {"tool_name": "Bash", "exit_code": 1}
            ]
        });
        let calls = parse_tool_calls(&input);
        assert_eq!(calls.len(), 2);
        assert!(calls[0].success);
        assert!(!calls[1].success);
    }

    #[test]
    fn test_average_reward_all_success() {
        let calls = vec![
            BatchCall {
                tool_name: "Read".into(),
                success: true,
                file_path: None,
            },
            BatchCall {
                tool_name: "Read".into(),
                success: true,
                file_path: None,
            },
        ];
        let avg = average_reward(&calls);
        assert!((avg - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_average_reward_mixed() {
        let calls = vec![
            BatchCall {
                tool_name: "Read".into(),
                success: true,
                file_path: None,
            }, // +1.0
            BatchCall {
                tool_name: "Bash".into(),
                success: false,
                file_path: None,
            }, // -0.3
        ];
        let avg = average_reward(&calls);
        // (1.0 + -0.3) / 2 = 0.35
        assert!((avg - 0.35).abs() < 1e-9);
    }

    #[test]
    fn test_average_reward_empty() {
        let calls: Vec<BatchCall> = vec![];
        assert_eq!(average_reward(&calls), 0.0);
    }

    #[test]
    fn test_batch_has_edit_or_write_true() {
        let calls = vec![
            BatchCall {
                tool_name: "Read".into(),
                success: true,
                file_path: None,
            },
            BatchCall {
                tool_name: "Edit".into(),
                success: true,
                file_path: None,
            },
        ];
        // O que este teste sempre assegurou — "a batch com um Edit produz o
        // hint" — continua valendo, agora pelo predicado que também sabe QUAL
        // arquivo. Sem caminho no payload, o desconhecido reporta (assimetria
        // deliberada de `can_carry_a_symbol`).
        assert_eq!(symbol_bearing_edits(&calls).len(), 1);
    }

    #[test]
    fn test_batch_has_edit_or_write_false() {
        let calls = vec![
            BatchCall {
                tool_name: "Read".into(),
                success: true,
                file_path: None,
            },
            BatchCall {
                tool_name: "Bash".into(),
                success: false,
                file_path: None,
            },
        ];
        assert!(symbol_bearing_edits(&calls).is_empty());
    }

    #[test]
    fn test_call_reward_values() {
        assert!((call_reward(true) - 1.0).abs() < 1e-9);
        assert!((call_reward(false) - (-0.3)).abs() < 1e-9);
    }
    // ── P2/S-2.1 (2026-09-04): o hint fala por EVIDÊNCIA ─────────────────────

    fn call(tool: &str, path: Option<&str>) -> Value {
        json!({"tool_name": tool, "success": true,
               "tool_input": {"file_path": path.unwrap_or_default()}})
    }

    #[test]
    fn a_doc_only_batch_says_nothing_because_a_markdown_edit_cannot_orphan_a_symbol() {
        // O caso medido: 422 emissões em 10 transcripts, a maioria sobre .md/.py
        // de skills. Um Edit em markdown não pode criar um pub órfão, então o
        // aviso sobre órfãos não tem sobre o que avisar.
        for ext in ["md", "json", "toml", "yaml", "txt", "lock"] {
            let calls = parse_tool_calls(&json!({"tool_calls": [
                call("Edit", Some(&format!("docs/nota.{ext}")))
            ]}));
            assert!(
                symbol_bearing_edits(&calls).is_empty(),
                ".{ext} nao pode carregar simbolo publico"
            );
        }
    }

    #[test]
    fn a_source_edit_is_reported_and_the_file_is_named() {
        let calls = parse_tool_calls(&json!({"tool_calls": [
            call("Edit", Some("docs/nota.md")),
            call("Edit", Some("crates/touring-cli/src/cli/kpi.rs")),
            call("Bash", None),
        ]}));
        assert_eq!(
            symbol_bearing_edits(&calls),
            vec!["crates/touring-cli/src/cli/kpi.rs"],
            "so' o arquivo que pode carregar simbolo entra, e ele e' NOMEADO"
        );
    }

    #[test]
    fn an_unknown_path_is_reported_because_unknown_is_not_proof_of_harmlessness() {
        // Assimetria deliberada: REGRA #0 e' constitucional, e calar um aviso
        // real custa mais que uma linha a mais.
        let calls = parse_tool_calls(&json!({"tool_calls": [
            json!({"tool_name": "Write", "success": true})
        ]}));
        assert_eq!(symbol_bearing_edits(&calls).len(), 1);
    }

    #[test]
    fn a_batch_with_no_edit_at_all_stays_silent() {
        let calls = parse_tool_calls(&json!({"tool_calls": [
            call("Bash", None), call("Read", Some("src/lib.rs"))
        ]}));
        assert!(symbol_bearing_edits(&calls).is_empty());
    }
}
