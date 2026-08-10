//! Pre-Tool-Use Hook — intercepts and moderates tool invocations.
//!
//! Invoked BEFORE each tool call. Allows Claude Code to:
//! - **proceed**: Allow the tool call to execute normally
//! - **skip**: Skip the tool call (return early with no-op)
//! - **modify_args**: Mutate tool arguments before execution
//!
//! This hook integrates with:
//! - ACO pheromone system: consults historical tool quality for adaptive gating
//! - CLI pre-task-scout: caches scouting results for the current task
//! - Circuit breaker: halts session if repeated failures detected
//!
//! Target latency: <5ms (hot path).

use crate::n1_bridge::N1Bridge;
use crate::runtime::{HookResponse, HookRuntime};
use crate::shared::cila::cila_budget_edit;
use crate::shared::hook_helpers;
use touring_foundation::diagnostic::DiagnosticCode;
use touring_intelligence::rl::aco::pheromone_bus::PheroKey;

/// Decision returned by the pre-tool-use signal computation.
#[derive(Debug, Clone)]
pub enum ToolUseDecision {
    /// Allow the tool to execute normally.
    Proceed,
    /// Skip the tool call entirely with a reason.
    Skip {
        /// Human-readable explanation for why the tool call was skipped.
        reason: String,
    },
    /// Execute with modified arguments.
    ModifyArgs {
        /// Replacement tool arguments to use in place of the originals.
        modified_args: serde_json::Value,
        /// Optional `additionalContext` to inject alongside the modified call.
        context: Option<String>,
    },
}

/// Run the pre-tool-use hook (diverging version — for CLI entry point).
#[tracing::instrument(skip(runtime, input), fields(hook = "pre_tool_use"))]
pub fn run(
    runtime: &HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

/// Run the pre-tool-use hook, returning a `HookResponse` instead of diverging.
///
/// Used by the daemon to handle the hook without calling `process::exit`.
pub fn run_returning(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    let tool_name = parse_tool_name(input);
    let tool_args = parse_tool_args(input);

    // Empty tool name = allow by default (safety-first)
    if tool_name.is_empty() {
        return HookResponse::Allow;
    }

    // ── Circuit breaker: check for repeated failures ──────────────────────
    if let Some(halt) = check_circuit_breaker(runtime, &tool_name) {
        return halt;
    }

    // ── D2.4: Tool output routing (PreToolUse sandbox intercept) ────────
    // Intercepts large-output tools (>10KB) before execution and routes
    // them to sandbox subprocess + Tantivy storage, returning content_hash.
    // R1 mitigation: feature flag `TOURING_HOOK_ROUTING=0` disables this.
    if let Some(routing_response) = check_tool_output_routing(runtime, &tool_name, &tool_args) {
        return routing_response;
    }

    // ── ACO pheromone: consult historical tool quality ─────────────────────
    let pheromone_decision = consult_aco_pheromone(runtime, &tool_name);

    // ── CLI pre-task-scout: inject cached scouting context ────────────────
    let scout_context = inject_task_scout_context(runtime, &tool_name);

    // ── N1 delegation: check CILA level and delegate L4+ to N1 ────────────
    let n1_sequence = compute_n1_delegation(runtime, &tool_name, &tool_args);

    // ── Merge N1 sequence into scout_context when present ──────────────────
    // N1-2: integrate n1_sequence output into decision context
    let scout_context = match (scout_context, n1_sequence) {
        (Some(ctx_str), Some(seq)) => {
            if let Ok(mut ctx_val) = serde_json::from_str::<serde_json::Value>(&ctx_str) {
                if let Some(obj) = ctx_val.as_object_mut()
                    && let Ok(seq_val) = serde_json::to_value(&seq)
                {
                    obj.insert("n1_sequence".to_string(), seq_val);
                }
                serde_json::to_string(&ctx_val).ok()
            } else {
                Some(ctx_str)
            }
        }
        (ctx, _) => ctx,
    };

    // ── Predictive blast radius injection for Task* tools ─────────────────
    // D2: When TaskCreate/TaskUpdate mentions high-blast symbols, prepend
    // sub-task guidance into the subject before Claude Code executes.
    let blast_injection = compute_predictive_blast_injection(runtime, &tool_name, &tool_args);

    // ── Assemble final decision ────────────────────────────────────────────
    assemble_response(pheromone_decision, scout_context, blast_injection)
}

/// Compute N1 delegation for CILA L4+ complex tool invocations.
///
/// Returns `Some(GeneratedSequence)` if N1 generated a sequence for this task,
/// `None` if the task is below L4 threshold or N1 is unavailable.
fn compute_n1_delegation(
    runtime: &HookRuntime,
    tool_name: &str,
    tool_args: &serde_json::Value,
) -> Option<touring_intelligence::rl::n1::GeneratedSequence> {
    // Read CILA level from stable session (canonical pattern)
    let cila_level: u8 = hook_helpers::cila_level_from_runtime(runtime, 0);

    // Only delegate L4+ tasks to N1
    if cila_level < 4 {
        return None;
    }

    // S-7: n1_bridge is eagerly initialized (not Option anymore)
    let n1_bridge = runtime.n1_bridge.clone();

    // Build a descriptive objective for this tool invocation
    let description = format!(
        "tool={} args={}",
        tool_name,
        tool_args_to_description(tool_args)
    );
    let file_path = tool_args
        .pointer("/file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let objective = N1Bridge::objective_from_hook(&description, file_path, true);

    // generate_if_complex returns None for L0-L3, Some for L4+
    n1_bridge.generate_if_complex(&objective, cila_level)
}

/// Convert tool arguments to a human-readable description string.
fn tool_args_to_description(args: &serde_json::Value) -> String {
    // Truncate large argument payloads to avoid oversized objectives
    let summary = args.to_string();
    if summary.len() > 200 {
        format!(
            "{}... [truncated {} bytes]",
            &summary[..200],
            summary.len() - 200
        )
    } else {
        summary
    }
}

// ── D2.4: Tool output routing (PreToolUse sandbox intercept) ────────

/// Check whether a tool invocation should be routed to sandbox execution.
///
/// Returns `Some(HookResponse)` with `HookResponse::ContextWithUpdatedInput`
/// when the tool should run in sandbox (large output detected).
/// Returns `None` when the tool should proceed normally (PassThrough).
///
/// R1 mitigation: `TOURING_HOOK_ROUTING=0` disables routing entirely.
fn check_tool_output_routing(
    runtime: &HookRuntime,
    tool_name: &str,
    tool_args: &serde_json::Value,
) -> Option<HookResponse> {
    // Feature flag gate — R1 mitigation
    if !crate::shared::feature_flags::touring_hook_routing_enabled() {
        return None;
    }

    // Classify the routing path based on tool name + args heuristics
    let decision = crate::tool_output_router::classify_tool_routing(tool_name, tool_args)?;

    match decision {
        crate::tool_output_router::RoutingDecision::PassThrough => None,
        crate::tool_output_router::RoutingDecision::RouteToSandbox => {
            // D2.4 + D2.2 + D2.3 closure: actually run subprocess +
            // persist to global tool_outputs index. The returned `modified`
            // payload carries `content_hash` so the LLM can retrieve via
            // ctx_retrieve(content_hash) without re-running the tool.
            let modified = crate::tool_output_router::build_sandbox_wrapper_args(
                Some(&runtime.project_root),
                tool_name,
                tool_args.clone(),
            );

            // Record metric for gate-metrics observability
            crate::shared::gate_metrics::record_tool_output_routed();

            // D2.4: tag the routing decision with the runtime's session turn
            // so downstream consumers (memory recall, logs) can correlate
            // routing events with the LLM turn that triggered them.
            let turn = runtime.session_turn();
            let content_hash = modified
                .get("content_hash")
                .and_then(|v| v.as_str())
                .unwrap_or("<pending>");
            tracing::debug!(
                target: "touring::routing",
                tool = tool_name,
                session_turn = turn,
                content_hash,
                "tool output routed to sandbox + stored"
            );

            let context = format!(
                "Tool output routed to sandbox for context efficiency \
                 (D2.4, turn={turn}, hash={content_hash}). \
                 Retrieve full output via ctx_retrieve(content_hash)."
            );
            // The envelope describes the result; it is NOT a tool input. Only
            // substitute an input the tool can actually run — otherwise stay
            // advisory. Handing Bash an input with no `command` (which is what
            // this did until 2026-08-08) breaks the call outright.
            match crate::tool_output_router::envelope_as_tool_input(tool_name, &modified) {
                Some(updated_input) => Some(HookResponse::ContextWithUpdatedInput {
                    context,
                    event_name: Some("pre_tool_use".into()),
                    updated_input,
                }),
                None => {
                    // No safe substitution for this tool's schema: let the
                    // original call proceed and only add the hash to context.
                    Some(HookResponse::Context {
                        context,
                        event_name: Some("pre_tool_use".into()),
                    })
                }
            }
        }
    }
}

// ── Helper functions ─────────────────────────────────────────────────────────

/// Parse tool name from the hook input payload.
fn parse_tool_name(input: &serde_json::Value) -> String {
    input
        .pointer("/tool_name")
        .and_then(|v| v.as_str())
        .or_else(|| {
            input
                .pointer("/tool_input/tool_name")
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string()
}

/// Parse tool arguments as a JSON Value from the hook input payload.
fn parse_tool_args(input: &serde_json::Value) -> serde_json::Value {
    input
        .pointer("/tool_input")
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

/// Check circuit breaker for repeated tool failures.
///
/// Returns `Some(HookResponse::Halt)` if the circuit breaker has tripped,
/// `None` otherwise.
fn check_circuit_breaker(runtime: &HookRuntime, tool_name: &str) -> Option<HookResponse> {
    let failure_key = format!("{}_failure_count", tool_name);
    let count: u32 = runtime
        .ctx
        .result_cache
        .get_result("__circuit__", &failure_key)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Trip circuit breaker after 5 consecutive failures
    if count >= 5 {
        let reason = format!(
            "Circuit breaker: tool '{}' has failed {} consecutive times. \
             Halt to prevent further damage. Please investigate manually.",
            tool_name, count
        );
        return Some(HookResponse::Halt { reason });
    }
    None
}

/// Consult ACO pheromone system for adaptive tool-use gating.
///
/// Returns `ToolUseDecision::Proceed` by default (no pheromone = allow).
fn consult_aco_pheromone(runtime: &HookRuntime, tool_name: &str) -> ToolUseDecision {
    // Access ACO wiring state via mutex (interior mutability)
    let Ok(aco_guard) = runtime.aco_wiring.try_lock() else {
        return ToolUseDecision::Proceed; // Lock contention = allow
    };

    // Query pheromone level for this tool using TemplateId key
    let pheromone_key = PheroKey::TemplateId(tool_name.to_string());
    let pheromone_level = aco_guard.bus.get(&pheromone_key);

    // Low pheromone (< 0.3) = historical quality is poor → skip with reason
    if pheromone_level < 0.3 && pheromone_level > 0.0 {
        return ToolUseDecision::Skip {
            reason: format!(
                "ACO: tool '{}' has low historical quality (pheromone={:.2}). \
                 Skipping to avoid repeated failures.",
                tool_name, pheromone_level
            ),
        };
    }

    ToolUseDecision::Proceed
}

/// Inject cached task-scout context if available for this tool invocation.
///
/// Returns `Some(context_string)` if cli-pre-task-scout cached results exist,
/// `None` otherwise.
fn inject_task_scout_context(runtime: &HookRuntime, tool_name: &str) -> Option<String> {
    let scout_key = format!("scout:{}", tool_name);
    runtime
        .ctx
        .result_cache
        .get_result("__scout__", &scout_key)
        .filter(|s| !s.is_empty())
        .map(|s| format!("[scout] {s}"))
}

/// D2: Predictive blast radius injection for Task* tool invocations.
///
/// Orchestrates the three sub-steps: subject extraction, blast scanning, and
/// output construction. Returns `Some((updated_input, context))` when injection
/// is warranted, `None` otherwise (NOOP — exit 0 guaranteed by caller).
fn compute_predictive_blast_injection(
    runtime: &HookRuntime,
    tool_name: &str,
    tool_args: &serde_json::Value,
) -> Option<(serde_json::Value, String)> {
    if !matches!(tool_name, "TaskCreate" | "TaskUpdate") {
        return None;
    }
    let subject = extract_task_subject(tool_args)?;
    let symbols = extract_pascal_symbols(subject);
    if symbols.is_empty() {
        return None;
    }
    // Predictive Wave D2: every invocation that reaches the HNSW scan is
    // a "blast_inject" event — whether or not the threshold is crossed.
    crate::shared::gate_metrics::record_blast_inject();
    let high_blast_modules = scan_blast_modules(runtime, tool_name, &symbols)?;
    let out = build_blast_output(runtime, tool_args, subject, &high_blast_modules);
    // Reached this arm ⇒ the response mutates `updated_input`.
    crate::shared::gate_metrics::record_blast_mutation();
    Some(out)
}

/// Extract the task subject string from tool arguments.
///
/// Tries fields `subject`, `description`, and `prompt` in order.
/// Returns `None` when all fields are absent or empty.
fn extract_task_subject(tool_args: &serde_json::Value) -> Option<&str> {
    let s = tool_args
        .pointer("/subject")
        .or_else(|| tool_args.pointer("/description"))
        .or_else(|| tool_args.pointer("/prompt"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if s.is_empty() { None } else { Some(s) }
}

/// Scan PascalCase symbols for high blast radius impact.
///
/// Resolves each symbol to a file via the symbol store, then computes blast
/// radius using `BlastRadiusEngine::compute_with_timeout` when the symbol index
/// is available (primary path), falling back to `petgraph_blast_radius` (pre-built
/// cache). Returns `None` when no symbol exceeds the 3-module threshold.
/// Budget: 40 ms across all symbols combined.
fn scan_blast_modules(
    runtime: &HookRuntime,
    tool_name: &str,
    symbols: &[&str],
) -> Option<Vec<String>> {
    let budget = std::time::Duration::from_millis(40);
    let start = std::time::Instant::now();
    let mut high_blast_modules: Vec<String> = Vec::new();

    // Build a BlastRadiusEngine once from the symbol index if available.
    // This wires BlastRadiusEngine::compute_with_timeout into the hot path,
    // providing per-call latency budgets derived from the remaining pipeline budget.
    let engine = runtime.infra.symbol_index.as_ref().map(|idx| {
        use std::sync::Arc;
        use touring_analysis::blast_radius::BlastRadiusEngine;
        BlastRadiusEngine::bfs_only(Arc::new(idx.clone()))
    });

    for sym in symbols {
        let elapsed = start.elapsed();
        if elapsed > budget {
            tracing::debug!(tool = %tool_name, "predictive blast: budget exhausted");
            crate::shared::gate_metrics::record_blast_timeout();
            break;
        }
        let remaining = budget - elapsed;
        accumulate_blast_modules(
            runtime,
            tool_name,
            sym,
            engine.as_ref(),
            remaining,
            &mut high_blast_modules,
        );
    }

    if high_blast_modules.is_empty() {
        None
    } else {
        Some(high_blast_modules)
    }
}

/// Resolve one symbol to its file path and accumulate high-blast module names.
///
/// Dispatches to [`blast_via_engine`] (primary) or [`blast_via_petgraph`] (fallback).
/// No-ops silently when the symbol store is absent or the symbol is not found.
fn accumulate_blast_modules(
    runtime: &HookRuntime,
    tool_name: &str,
    sym: &str,
    engine: Option<&touring_analysis::blast_radius::BlastRadiusEngine>,
    remaining_budget: std::time::Duration,
    high_blast_modules: &mut Vec<String>,
) {
    // Resolve symbol → file path via symbol store (immutable borrow).
    let fp = match runtime
        .infra
        .symbol_store
        .as_ref()
        .and_then(|store| store.find_symbol(sym).ok())
        .and_then(|locs| locs.into_iter().next())
        .map(|loc| loc.file_path)
    {
        Some(p) => p,
        None => return,
    };

    let modules = if let Some(eng) = engine {
        blast_via_engine(eng, tool_name, sym, &fp, remaining_budget)
    } else {
        blast_via_petgraph(runtime, tool_name, sym, &fp)
    };

    for m in modules {
        push_unique_module(m, high_blast_modules);
    }
}

/// Compute blast radius via `BlastRadiusEngine::compute_with_timeout` (primary path).
///
/// Returns module names for files with blast radius > 3. Returns an empty `Vec` when
/// the budget is exceeded (graceful NOOP — `compute_with_timeout` returns `None`).
fn blast_via_engine(
    engine: &touring_analysis::blast_radius::BlastRadiusEngine,
    tool_name: &str,
    sym: &str,
    fp: &str,
    budget: std::time::Duration,
) -> Vec<String> {
    let result = match engine.compute_with_timeout(fp, budget) {
        Some(r) => r,
        None => {
            tracing::debug!(
                symbol = %sym,
                "predictive blast: compute_with_timeout budget exceeded"
            );
            return Vec::new();
        }
    };
    let affected_count = result.affected_files.len();
    tracing::info!(
        tool = %tool_name,
        symbol = %sym,
        blast_size = affected_count,
        mutation_applied = affected_count > 3,
        strategy = %result.strategy_used,
        "predictive blast radius computed (BlastRadiusEngine)"
    );
    if affected_count <= 3 {
        return Vec::new();
    }
    result
        .affected_files
        .iter()
        .map(|f| parent_module_name(&f.path))
        .collect()
}

/// Compute blast radius via petgraph BFS (fallback — pre-built dependency cache).
///
/// Returns module names for files with blast radius > 3. Returns an empty `Vec` when
/// the dependency cache is not initialized (`petgraph_blast_radius` returns `None`).
fn blast_via_petgraph(runtime: &HookRuntime, tool_name: &str, sym: &str, fp: &str) -> Vec<String> {
    let affected = match runtime.petgraph_blast_radius(std::path::Path::new(fp)) {
        Some(files) => files,
        None => {
            tracing::debug!(symbol = %sym, "predictive blast: dependency_cache not init");
            return Vec::new();
        }
    };
    let affected_count = affected.len();
    tracing::info!(
        tool = %tool_name,
        symbol = %sym,
        blast_size = affected_count,
        mutation_applied = affected_count > 3,
        "predictive blast radius computed (petgraph fallback)"
    );
    if affected_count <= 3 {
        return Vec::new();
    }
    affected
        .iter()
        .map(|p| parent_module_name(p.to_str().unwrap_or("unknown")))
        .collect()
}

/// Extract the immediate parent directory name from a file path string.
///
/// Used to derive the module name from an affected file path. Falls back to
/// `"unknown"` when the path has no parent or the name is not valid UTF-8.
fn parent_module_name(path: &str) -> String {
    std::path::Path::new(path)
        .parent()
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Push `module` into `vec` only when it is not already present (dedup by value).
///
/// Linear scan is acceptable here: the expected vec length is < 20 modules.
fn push_unique_module(module: String, vec: &mut Vec<String>) {
    if !vec.contains(&module) {
        vec.push(module);
    }
}

/// Build the updated tool input and CILA-budgeted context string.
///
/// Prepends the `[TOURING-INJECT]` prefix to the subject field and truncates
/// the context to the current CILA budget.
fn build_blast_output(
    runtime: &HookRuntime,
    tool_args: &serde_json::Value,
    subject: &str,
    high_blast_modules: &[String],
) -> (serde_json::Value, String) {
    let module_list = high_blast_modules.join(", ");
    let n = high_blast_modules.len();
    let injection = format!(
        "[TOURING-INJECT] Blast Radius affects {n} modules: {{{module_list}}}. Suggested sub-tasks:\n\
         - Pre-check touring ast blast <primary_file>\n\
         - Run cargo check after each atomic edit\n\
         - Update wiring audit post-merge"
    );
    let updated_subject = format!("{injection}\n\n{subject}");

    // B-310: BlastInjection — emit when predictive blast injects symbols into task input.
    if n > 0 {
        use touring_analysis::blast_radius::BlastWarning;
        let w = BlastWarning::BlastInjection {
            symbols: high_blast_modules.to_vec(),
            module_count: n,
        };
        let diag = w.to_diagnostic();
        tracing::warn!(
            code = %diag.code,
            severity = %diag.severity,
            message = %diag.message,
            tool = "TaskCreate",
            "B-310 BlastInjection emitted"
        );
    }

    // Determine which field to update.
    let target_field = if tool_args.pointer("/subject").is_some() {
        "subject"
    } else if tool_args.pointer("/description").is_some() {
        "description"
    } else {
        "prompt"
    };

    let mut updated_input = tool_args.clone();
    if let Some(obj) = updated_input.as_object_mut() {
        obj.insert(
            target_field.to_string(),
            serde_json::Value::String(updated_subject),
        );
    }

    // Truncate context to CILA budget.
    let cila_level: u8 = runtime
        .ctx
        .stable_session
        .borrow()
        .as_ref()
        .map(|s| s.cila_level)
        .unwrap_or(2);
    let budget_chars = cila_budget_edit(cila_level);
    let context = if injection.len() <= budget_chars {
        injection
    } else {
        format!("{}…[truncated]", &injection[..budget_chars])
    };

    (updated_input, context)
}

/// Extract PascalCase identifiers from a text string.
///
/// Scans for byte sequences starting with an ASCII uppercase letter followed
/// by at least one alphanumeric or underscore character. Returns slices into
/// the original string — zero allocation beyond the Vec of references.
fn extract_pascal_symbols(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut result = Vec::new();
    let mut i = 0;

    while i < len {
        // Find start of a PascalCase token: uppercase ASCII letter.
        if bytes[i].is_ascii_uppercase() {
            // Ensure the preceding byte is a word boundary (not alphanumeric).
            let preceded_by_alnum =
                i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            if !preceded_by_alnum {
                let start = i;
                i += 1;
                // Consume remaining word characters.
                while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                // Require at least 2 more chars after the capital (e.g. "Foo", not "I").
                if i - start >= 3 {
                    // Safety: start and i are both valid UTF-8 boundaries because
                    // we only advanced past ASCII bytes.
                    result.push(&text[start..i]);
                }
                continue;
            }
        }
        i += 1;
    }

    result
}

/// Assemble the final `HookResponse` from pheromone decision, scout context,
/// and optional blast injection.
///
/// Extracted to keep `run_returning` below the CC=15 threshold.
fn assemble_response(
    pheromone_decision: ToolUseDecision,
    scout_context: Option<String>,
    blast_injection: Option<(serde_json::Value, String)>,
) -> HookResponse {
    // Blast injection takes precedence on Proceed: it modifies the tool input.
    if let (ToolUseDecision::Proceed, Some((updated_input, blast_ctx))) =
        (&pheromone_decision, blast_injection)
    {
        let combined_ctx = match scout_context {
            Some(sc) => format!("{sc}\n{blast_ctx}"),
            None => blast_ctx,
        };
        return HookResponse::ContextWithUpdatedInput {
            context: combined_ctx,
            event_name: Some("TaskCreate".to_string()),
            updated_input,
        };
    }

    match pheromone_decision {
        ToolUseDecision::Proceed => {
            if let Some(ctx) = scout_context {
                HookResponse::Context {
                    context: ctx,
                    event_name: Some("PreToolUse".to_string()),
                }
            } else {
                HookResponse::Allow
            }
        }
        ToolUseDecision::Skip { reason } => HookResponse::Deny {
            reason,
            context: scout_context,
            event_name: Some("PreToolUse".to_string()),
        },
        ToolUseDecision::ModifyArgs {
            modified_args,
            context,
        } => HookResponse::ContextWithUpdatedInput {
            context: context.or(scout_context).unwrap_or_default(),
            event_name: Some("PreToolUse".to_string()),
            updated_input: modified_args,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_name_from_tool_name() {
        let input = serde_json::json!({
            "tool_name": "Read",
            "tool_input": {"file_path": "/tmp/foo"}
        });
        assert_eq!(parse_tool_name(&input), "Read");
    }

    #[test]
    fn test_parse_tool_name_from_tool_input() {
        let input = serde_json::json!({
            "tool_input": {"tool_name": "Edit"}
        });
        assert_eq!(parse_tool_name(&input), "Edit");
    }

    #[test]
    fn test_parse_tool_name_empty() {
        let input = serde_json::json!({});
        assert_eq!(parse_tool_name(&input), "");
    }

    #[test]
    fn test_parse_tool_args() {
        let input = serde_json::json!({
            "tool_input": {"file_path": "/tmp/foo", "offset": 10}
        });
        let args = parse_tool_args(&input);
        assert_eq!(
            args.get("file_path").and_then(|v| v.as_str()),
            Some("/tmp/foo")
        );
    }
}
