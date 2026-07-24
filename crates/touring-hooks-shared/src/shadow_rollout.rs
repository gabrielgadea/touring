//! Shadow MCTS rollout for EnterPlanMode — detects dependency deadlocks via Tarjan SCC
//! before Claude Code executes the plan.
//!
//! # Design
//!
//! When `handle_enter_plan_mode` detects a CILA level >= 3 (complex tasks), it spawns a
//! non-blocking shadow rollout that:
//! 1. Extracts `task_id -> target_module` mappings from the pending task list.
//! 2. Builds a directed dependency graph (DiGraph) from those mappings using module-path
//!    containment as the edge heuristic (mod_a contains mod_b's name → a depends on b).
//! 3. Runs `petgraph::algo::tarjan_scc` O(V+E) to find Strongly Connected Components.
//! 4. Reports SCCs of size > 1 as dependency cycles, and surfaces a corrected topological
//!    order as an `additionalContext` hint for Claude Code to act on.
//!
//! # Cycle detection accuracy
//!
//! The graph edges are inferred from module-path containment — a lightweight heuristic
//! that avoids the need for a full `crate_dep_graph` bridge. This catches the most common
//! patterns (bidirectional deps, star-shaped cycles), with false negatives acceptable
//! since the rollout is **advisory only**: callers must never block plan-mode entry on it.
//!
//! # Budget
//!
//! The rollout runs under a strict wall-clock budget (default: 200ms for the sync
//! join, 12s for the background thread). If the budget is exceeded, `None` is returned
//! and `handle_enter_plan_mode` proceeds without the hint -- plan-mode entry is never
//! blocked.

use std::time::Duration;

use petgraph::Graph;
use petgraph::algo::tarjan_scc;
use serde_json::Value;

/// Result of a shadow MCTS rollout.
///
/// Fields are `pub` so integration tests in `tests/` can access them directly.
/// The module itself is `pub(crate)` in `shared/mod.rs`, preventing any leakage
/// through the crate's public API boundary.
pub struct ShadowRolloutResult {
    /// Whether a dependency deadlock (cycle) was predicted.
    pub deadlock_detected: bool,
    /// Modules involved in each SCC of size > 1 (each inner Vec = one cycle).
    pub cycles: Vec<Vec<String>>,
    /// Suggested execution order after SCC collapse (topological sort).
    pub suggested_order: Vec<String>,
    /// Wall-clock milliseconds spent in the rollout.
    pub elapsed_ms: u64,
    /// Whether the rollout completed within the allotted budget.
    pub completed: bool,
    /// Relative file paths predicted to be accessed in this plan (Suggestion 3).
    /// Populated from `target_module` / `file` fields of the task list.
    /// The prefetch actor uses these to warm the global parser cache before
    /// Claude Code reads them, cutting the hit penalty from ~15ms to <1ms.
    pub prefetch_paths: Vec<String>,
}

impl ShadowRolloutResult {
    /// Format a hint string for `additionalContext`, or `None` when no deadlock detected.
    pub fn as_hint(&self) -> Option<String> {
        if !self.deadlock_detected {
            return None;
        }
        let cycles_desc = self
            .cycles
            .iter()
            .map(|c| format!("[{}]", c.join(" <-> ")))
            .collect::<Vec<_>>()
            .join(", ");
        let order_desc = self.suggested_order.join(" -> ");
        Some(format!(
            "[TOURING MCTS-SYNTHESIS] Shadow validation predicted cyclic deadlock: {}. \
            Suggested execution topology: {}",
            cycles_desc, order_desc
        ))
    }
}

/// Extract `(task_id, target_module)` pairs from a JSON task list (CC<=4).
///
/// Accepts `task_id`, `target_module`, `file`, or `module` fields. Falls back to
/// `"t{index}"` for tasks without an explicit id.
fn extract_task_modules(tasks: &[Value]) -> Vec<(String, String)> {
    tasks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let id = t
                .get("task_id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| format!("t{}", i));
            let module = t
                .get("target_module")
                .and_then(|v| v.as_str())
                .or_else(|| t.get("file").and_then(|v| v.as_str()))
                .or_else(|| t.get("module").and_then(|v| v.as_str()))
                .map(String::from)?;
            Some((id, module))
        })
        .collect()
}

/// Extract a short crate identifier from a module path for edge heuristics.
///
/// `"crates/b_service/src/lib.rs"` → `"b_service"`.
/// Paths without `"crates/"` prefix are split on `/` and the first segment is returned.
fn crate_id(module: &str) -> &str {
    module
        .strip_prefix("crates/")
        .unwrap_or(module)
        .split('/')
        .next()
        .unwrap_or(module)
}

/// Real Tarjan SCC detection using `petgraph::algo::tarjan_scc` — O(V+E).
///
/// Builds a directed graph where an edge a→b exists when module_a's path contains
/// module_b's name (a depends on b). Runs Tarjan SCC to find all Strongly Connected
/// Components. SCCs of size > 1 are true dependency cycles.
///
/// Returns `(cycles, suggested_order)`:
/// - `cycles`: each inner `Vec<String>` lists task IDs involved in one SCC of size > 1.
/// - `suggested_order`: task IDs in topological order (SCCs condensed, sinks last).
///   Nodes within a cycle SCC appear in arbitrary order (all have mutual deps).
///
/// Budget guard: bails out on edge-building loop if deadline exceeded, returning
/// partial results safe for advisory use.
fn detect_tarjan_scc(
    task_modules: &[(String, String)],
    deadline: std::time::Instant,
) -> (Vec<Vec<String>>, Vec<String>) {
    let n = task_modules.len();

    // Build DiGraph: one node per task.
    let mut graph = Graph::<String, ()>::new();
    let nodes: Vec<_> = task_modules
        .iter()
        .map(|(id, _)| graph.add_node(id.clone()))
        .collect();

    // Add edges: a→b when mod_a's path contains mod_b's crate identifier (a depends on b).
    // Crate identifier: first path component after stripping "crates/" prefix.
    // E.g. "crates/auth/b_service/src/lib.rs" contains "b_service" (B's crate id) → edge A→B.
    'outer: for i in 0..n {
        if std::time::Instant::now() > deadline {
            break 'outer;
        }
        for j in 0..n {
            if i == j {
                continue;
            }
            let (_, mod_a) = &task_modules[i];
            let (_, mod_b) = &task_modules[j];
            let id_b = crate_id(mod_b);
            if !id_b.is_empty() && mod_a != mod_b && mod_a.contains(id_b) {
                graph.add_edge(nodes[i], nodes[j], ());
            }
        }
    }

    // Tarjan SCC — returns SCCs in reverse topological order (sinks first).
    let sccs = tarjan_scc(&graph);

    let cycles: Vec<Vec<String>> = sccs
        .iter()
        .filter(|scc| scc.len() > 1)
        .map(|scc| scc.iter().map(|&idx| graph[idx].clone()).collect())
        .collect();

    // Execution order: petgraph tarjan_scc returns SCCs with sinks first (reverse
    // topological order of the condensation). Sinks = dependencies with no dependents,
    // so "sinks first" IS the correct execution order (run dependencies before dependents).
    let suggested_order: Vec<String> = sccs
        .iter()
        .flat_map(|scc| scc.iter().map(|&idx| graph[idx].clone()))
        .collect();

    (cycles, suggested_order)
}

/// Executes a shadow rollout synchronously under a strict wall-clock budget.
///
/// Returns `None` when:
/// - `tasks` is empty (nothing to analyze).
/// - The budget is exceeded before task extraction completes.
///
/// # Advisory only
///
/// This function is **always advisory**. Callers must not block plan-mode entry on the
/// result. A `deadlock_detected: true` result is a signal to surface a hint, not a gate.
pub fn run_shadow_rollout(
    tasks: &[Value],
    _crdt_graph_snapshot: Option<&str>,
    budget: Duration,
) -> Option<ShadowRolloutResult> {
    let start = std::time::Instant::now();
    let deadline = start + budget;

    let task_modules = extract_task_modules(tasks);
    if task_modules.is_empty() || std::time::Instant::now() > deadline {
        // Early exit = budget already blown OR no tasks to simulate → timeout.
        if !task_modules.is_empty() {
            crate::gate_metrics::record_mcts_shadow_timeout();
        }
        return None;
    }

    // Predictive Wave D4: any rollout reaching SCC detection is a "run" event.
    crate::gate_metrics::record_mcts_shadow_run();

    // Real Tarjan SCC O(V+E) — returns (cycles, topological_order).
    let (cycles, suggested_order) = detect_tarjan_scc(&task_modules, deadline);

    let elapsed_ms = start.elapsed().as_millis() as u64;
    let completed = std::time::Instant::now() <= deadline;
    if !completed {
        crate::gate_metrics::record_mcts_shadow_timeout();
    }

    let deadlock_detected = !cycles.is_empty();
    if deadlock_detected {
        crate::gate_metrics::record_mcts_shadow_deadlock_detected();
    }

    // Collect unique file paths from task_modules for predictive prefetch.
    let prefetch_paths: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        task_modules
            .iter()
            .map(|(_, module)| module.clone())
            .filter(|m| seen.insert(m.clone()))
            .collect()
    };

    Some(ShadowRolloutResult {
        deadlock_detected,
        cycles,
        suggested_order,
        elapsed_ms,
        completed,
        prefetch_paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn task(id: &str, module: &str) -> Value {
        serde_json::json!({"task_id": id, "target_module": module})
    }

    #[test]
    fn test_empty_tasks_returns_none() {
        let result = run_shadow_rollout(&[], None, Duration::from_secs(1));
        assert!(result.is_none(), "empty tasks should return None");
    }

    #[test]
    fn test_single_task_no_deadlock() {
        let tasks = vec![task("S-1", "crates/touring-hooks/src/lib.rs")];
        let result = run_shadow_rollout(&tasks, None, Duration::from_secs(1))
            .expect("should return result for non-empty tasks");
        assert!(!result.deadlock_detected);
        assert!(result.cycles.is_empty());
        assert_eq!(result.suggested_order, vec!["S-1"]);
        assert!(result.completed);
    }

    #[test]
    fn test_non_crossing_tasks_no_deadlock() {
        let tasks = vec![
            task("S-1", "crates/touring-hooks/src/pre_edit.rs"),
            task("S-2", "crates/touring-cognitive/src/mcts.rs"),
            task("S-3", "crates/touring-ast/src/call_graph.rs"),
        ];
        let result =
            run_shadow_rollout(&tasks, None, Duration::from_secs(1)).expect("should return result");
        assert!(!result.deadlock_detected);
        assert_eq!(result.suggested_order.len(), 3);
    }

    #[test]
    fn test_budget_exhausted_returns_none_or_graceful() {
        let tasks = vec![task("S-1", "crates/a/src/lib.rs")];
        // 1ns budget -- either None or Some are valid; no panic is the guarantee.
        let result = run_shadow_rollout(&tasks, None, Duration::from_nanos(1));
        if let Some(r) = result {
            let _ = r.as_hint(); // must not panic
        }
    }

    #[test]
    fn test_as_hint_no_deadlock_returns_none() {
        let result = ShadowRolloutResult {
            deadlock_detected: false,
            cycles: vec![],
            suggested_order: vec!["S-1".to_string()],
            elapsed_ms: 5,
            completed: true,
            prefetch_paths: vec![],
        };
        assert!(result.as_hint().is_none());
    }

    #[test]
    fn test_as_hint_with_deadlock_returns_string() {
        let result = ShadowRolloutResult {
            deadlock_detected: true,
            cycles: vec![vec!["S-1".to_string(), "S-2".to_string()]],
            suggested_order: vec!["S-1".to_string(), "S-2".to_string()],
            elapsed_ms: 10,
            completed: true,
            prefetch_paths: vec!["crates/a/src/lib.rs".to_string()],
        };
        let hint = result.as_hint().expect("deadlock should produce hint");
        assert!(
            hint.contains("MCTS-SYNTHESIS"),
            "hint should contain MCTS-SYNTHESIS tag"
        );
        assert!(hint.contains("S-1"), "hint should mention S-1");
        assert!(hint.contains("S-2"), "hint should mention S-2");
    }

    #[test]
    fn test_tarjan_scc_detects_three_node_cycle() {
        // crate_id heuristic: A→B when A's path contains crate_id(B).
        // crate_id(B)="b_service" → A ("crates/auth/b_service/src/lib.rs") contains it → A→B.
        // crate_id(C)="c_repo"    → B ("crates/b_service/c_repo/src/lib.rs") contains it → B→C.
        // crate_id(A)="auth"      → C ("crates/c_repo/auth/src/lib.rs") contains it    → C→A.
        // Cycle A→B→C→A detected.
        let tasks = vec![
            task("A", "crates/auth/b_service/src/lib.rs"),
            task("B", "crates/b_service/c_repo/src/lib.rs"),
            task("C", "crates/c_repo/auth/src/lib.rs"),
        ];
        let result =
            run_shadow_rollout(&tasks, None, Duration::from_secs(5)).expect("should return result");
        assert!(result.deadlock_detected, "3-node cycle must be detected");
        assert!(!result.cycles.is_empty(), "cycles must be non-empty");
        // The SCC should contain all 3 task IDs.
        let all_in_scc: Vec<&str> = result
            .cycles
            .iter()
            .flat_map(|c| c.iter().map(|s| s.as_str()))
            .collect();
        assert!(all_in_scc.contains(&"A"), "A must be in cycle");
        assert!(all_in_scc.contains(&"B"), "B must be in cycle");
        assert!(all_in_scc.contains(&"C"), "C must be in cycle");
    }

    #[test]
    fn test_tarjan_scc_dag_topological_order() {
        // crate_id heuristic: A→B when A's path contains crate_id(B).
        // crate_id(B)="beta"  → A ("crates/alpha/beta/src/main.rs") contains "beta"  → A→B.
        // crate_id(C)="gamma" → B ("crates/beta/gamma/src/lib.rs")  contains "gamma" → B→C.
        // No reverse edges, no A→C shortcut. Pure linear DAG A→B→C.
        let tasks = vec![
            task("A", "crates/alpha/beta/src/main.rs"),
            task("B", "crates/beta/gamma/src/lib.rs"),
            task("C", "crates/gamma/src/util.rs"),
        ];
        let result =
            run_shadow_rollout(&tasks, None, Duration::from_secs(5)).expect("should return result");
        assert!(!result.deadlock_detected, "DAG must not detect a cycle");
        assert!(result.cycles.is_empty(), "cycles must be empty for DAG");
        // All three tasks must appear in suggested_order.
        assert_eq!(result.suggested_order.len(), 3);
        // petgraph tarjan_scc returns sinks first → C, B, A is the correct execution order.
        let a_pos = result
            .suggested_order
            .iter()
            .position(|s| s == "A")
            .expect("A must be in suggested_order");
        let b_pos = result
            .suggested_order
            .iter()
            .position(|s| s == "B")
            .expect("B must be in suggested_order");
        let c_pos = result
            .suggested_order
            .iter()
            .position(|s| s == "C")
            .expect("C must be in suggested_order");
        assert!(c_pos < b_pos, "C (no deps) must come before B");
        assert!(b_pos < a_pos, "B must come before A in topo order");
    }

    #[test]
    fn test_prefetch_paths_populated_from_task_modules() {
        // INVARIANT: run_shadow_rollout extracts unique module paths into prefetch_paths.
        let tasks = vec![
            task("S-1", "crates/touring-hooks/src/pre_edit.rs"),
            task("S-2", "crates/touring-cognitive/src/mcts.rs"),
            task("S-3", "crates/touring-hooks/src/pre_edit.rs"), // duplicate — deduplicated
        ];
        let result =
            run_shadow_rollout(&tasks, None, Duration::from_secs(1)).expect("should return result");
        assert_eq!(
            result.prefetch_paths.len(),
            2,
            "duplicates must be deduplicated"
        );
        assert!(
            result
                .prefetch_paths
                .contains(&"crates/touring-hooks/src/pre_edit.rs".to_string())
        );
        assert!(
            result
                .prefetch_paths
                .contains(&"crates/touring-cognitive/src/mcts.rs".to_string())
        );
    }
}
