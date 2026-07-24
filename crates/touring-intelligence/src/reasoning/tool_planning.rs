//! C12 — MCTS tool-planning: point the generic [`MCTSEngine`] at the command/tool
//! graph to find a low-cost chain of tools (the **geodesic of the tool graph** —
//! TCA-Space).
//!
//! The cognitive [`MCTSEngine`] is domain-agnostic: it is parameterized by an
//! `expand_fn` (state → candidate actions) and a `reward_fn` ((state, action) →
//! reward). C12 instantiates that domain as a **tool graph**:
//!
//! * **state / action** = a tool id (`u64`),
//! * **expand_fn** = the tools reachable in one step ([`ToolGraph::neighbors`]),
//! * **reward_fn** = `GOAL_BONUS − edge_cost` so the engine *maximizes* reward, i.e.
//!   prefers reaching the goal through cheap edges.
//!
//! [`plan_tool_chain`] descends greedily on the engine's `best_action` — at each step
//! the MCTS rollouts look ahead over the graph, favouring cheaper continuations toward
//! the goal. As an **MCTS heuristic** it *approximates* the geodesic (it can be lured
//! by an expensive edge that reaches the goal in one hop); it does not *guarantee* the
//! global optimum the way an exhaustive shortest-path search would — the trade is
//! anytime, sub-linear search over a large tool space. The descent is deterministic
//! (the engine uses no RNG: UCT selection + first-unvisited-child rollout) and fully
//! unit-tested here.

use std::collections::{HashMap, HashSet};

use super::mcts::{MCTSConfig, MCTSEngine};

/// Reward credited when an action lands on the goal, in the **same units as edge
/// costs**. It must exceed the cost of the optimal path (so reaching the goal is
/// worthwhile), yet a single edge whose cost exceeds `GOAL_BONUS` is *avoided* in
/// favour of a cheaper multi-hop route — that balance is what makes the planner
/// cost-sensitive rather than merely goal-greedy. Tune it to your cost scale; the
/// default suits per-edge costs on the order of tens.
const GOAL_BONUS: f64 = 50.0;

/// A directed, weighted tool graph: tool id → reachable `(next tool, invocation cost)`.
#[derive(Debug, Clone, Default)]
pub struct ToolGraph {
    edges: HashMap<u64, Vec<(u64, u32)>>,
}

impl ToolGraph {
    /// An empty tool graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a directed edge `from → to` with an invocation `cost`.
    pub fn add_edge(&mut self, from: u64, to: u64, cost: u32) {
        self.edges.entry(from).or_default().push((to, cost));
    }

    /// Tools reachable in one step from `state`.
    #[must_use]
    pub fn neighbors(&self, state: u64) -> Vec<u64> {
        self.edges
            .get(&state)
            .map(|v| v.iter().map(|(t, _)| *t).collect())
            .unwrap_or_default()
    }

    /// Cost of the edge `from → to`, if it exists.
    #[must_use]
    pub fn cost(&self, from: u64, to: u64) -> Option<u32> {
        self.edges
            .get(&from)?
            .iter()
            .find(|(t, _)| *t == to)
            .map(|(_, c)| *c)
    }
}

/// A planned tool chain and its total invocation cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPlan {
    /// The chain of tool ids, starting at `start` and (when reached) ending at `goal`.
    pub chain: Vec<u64>,
    /// `Σ` of the edge costs traversed.
    pub total_cost: u64,
    /// `true` when the chain reached the goal within `max_steps`.
    pub reached_goal: bool,
}

/// The MCTS configuration tuned for tool-planning: a deeper, more thorough search
/// than the default since tool chains are short but the cost landscape matters.
fn planner_config() -> MCTSConfig {
    MCTSConfig {
        max_depth: 8,
        num_rollouts: 128,
        exploration_constant: std::f64::consts::SQRT_2,
        discount: 0.95,
        max_rollouts: 512,
    }
}

/// Plan a low-cost tool chain from `start` to `goal` over `graph` by pointing the
/// MCTS engine at the current state at each step: the engine's `best_action` is the
/// next tool whose look-ahead best trades edge cost against reaching the goal. The
/// descent stops at the goal, a dead end, a forced revisit, or `max_steps`. Cycles
/// are avoided by excluding already-visited tools from expansion (except the goal).
#[must_use]
pub fn plan_tool_chain(graph: &ToolGraph, start: u64, goal: u64, max_steps: usize) -> ToolPlan {
    let engine = MCTSEngine::new(planner_config());
    let mut chain = vec![start];
    let mut visited: HashSet<u64> = HashSet::from([start]);
    let mut total_cost = 0u64;
    let mut current = start;

    for _ in 0..max_steps {
        if current == goal {
            break;
        }
        let result = engine.search(
            current,
            |s| {
                graph
                    .neighbors(s)
                    .into_iter()
                    .filter(|n| *n == goal || !visited.contains(n))
                    .collect()
            },
            |s, a| {
                let edge = f64::from(graph.cost(s, a).unwrap_or(u32::MAX));
                let bonus = if a == goal { GOAL_BONUS } else { 0.0 };
                bonus - edge
            },
        );
        let Some(r) = result else { break }; // dead end — no further tools
        let next = r.best_action;
        total_cost += u64::from(graph.cost(current, next).unwrap_or(0));
        chain.push(next);
        visited.insert(next);
        current = next;
    }

    ToolPlan {
        chain,
        total_cost,
        reached_goal: current == goal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_primitives() {
        let mut g = ToolGraph::new();
        g.add_edge(1, 2, 10);
        g.add_edge(1, 3, 5);
        assert_eq!(g.neighbors(1), vec![2, 3]);
        assert!(g.neighbors(9).is_empty());
        assert_eq!(g.cost(1, 3), Some(5));
        assert_eq!(g.cost(1, 9), None);
    }

    #[test]
    fn start_equals_goal_is_trivial() {
        let g = ToolGraph::new();
        let plan = plan_tool_chain(&g, 7, 7, 8);
        assert!(plan.reached_goal);
        assert_eq!(plan.chain, vec![7]);
        assert_eq!(plan.total_cost, 0);
    }

    #[test]
    fn plans_direct_edge_when_cheapest() {
        // start→goal cost 1 (1 hop) vs start→mid→goal cost 2 (2 hops): direct wins.
        let mut g = ToolGraph::new();
        g.add_edge(1, 99, 1); // direct to goal
        g.add_edge(1, 2, 1);
        g.add_edge(2, 99, 1);
        let plan = plan_tool_chain(&g, 1, 99, 8);
        assert!(plan.reached_goal);
        assert_eq!(plan.chain, vec![1, 99]);
        assert_eq!(plan.total_cost, 1);
    }

    #[test]
    fn plans_multi_hop_when_only_route() {
        // The goal is reachable ONLY via a forced 3-hop chain 1→2→3→99 (no
        // shortcuts). MCTS must discover and follow the full chain to the goal —
        // proving genuine multi-step lookahead planning (not just 1-hop greed).
        let mut g = ToolGraph::new();
        g.add_edge(1, 2, 1);
        g.add_edge(2, 3, 1);
        g.add_edge(3, 99, 1);
        let plan = plan_tool_chain(&g, 1, 99, 8);
        assert!(plan.reached_goal);
        assert_eq!(plan.chain, vec![1, 2, 3, 99]);
        assert_eq!(plan.total_cost, 3);
    }

    #[test]
    fn cost_accounting_is_exact_along_the_chosen_chain() {
        // Whatever chain MCTS chooses, `total_cost` must equal the Σ of the traversed
        // edge costs — the planner's accounting is exact even though its *choice* is a
        // heuristic (it approximates, not guarantees, the geodesic — see module docs).
        let mut g = ToolGraph::new();
        g.add_edge(1, 2, 3);
        g.add_edge(2, 99, 4);
        let plan = plan_tool_chain(&g, 1, 99, 8);
        assert!(plan.reached_goal);
        assert_eq!(plan.chain, vec![1, 2, 99]);
        assert_eq!(plan.total_cost, 7); // 3 + 4, exactly
    }

    #[test]
    fn dead_end_does_not_reach_goal() {
        // start→mid, but mid has no outgoing edges and the goal is unreachable.
        let mut g = ToolGraph::new();
        g.add_edge(1, 2, 1);
        let plan = plan_tool_chain(&g, 1, 99, 8);
        assert!(!plan.reached_goal);
        assert_eq!(plan.chain, vec![1, 2]);
    }

    #[test]
    fn respects_max_steps_budget() {
        // A long chain 1→2→3→4→5 with goal=5 but only 2 steps allowed.
        let mut g = ToolGraph::new();
        for i in 1..5u64 {
            g.add_edge(i, i + 1, 1);
        }
        let plan = plan_tool_chain(&g, 1, 5, 2);
        assert!(
            !plan.reached_goal,
            "goal is 4 hops away but only 2 steps allowed"
        );
        assert!(plan.chain.len() <= 3); // start + at most 2 steps
    }
}
