//! `touring budget-verify` / `plan-chain` / `consistency` — orchestrator-facing CLI
//! surfaces over three pure reasoning engines from the coupling backlog. Each engine
//! is pure and unit-tested in its own crate; these handlers are the **production
//! consumer** the TACO orchestrator invokes, supplying the inputs it alone holds:
//!
//! * **C11** [`verify_conservation`] — `Σ subtask budgets ≤ root` per dimension
//!   (`B ∈ ℕ⁶`). The orchestrator supplies the root budget from the Workflow-tool.
//! * **C12** [`plan_tool_chain`] — MCTS heuristic over a weighted tool graph (the
//!   geodesic of TCA-Space). The orchestrator supplies the candidate tool graph.
//! * **C14** [`mod@consistency_gate`] — `GED_norm + α·(1 − cos)` merge gate for parallel
//!   engineers. The orchestrator supplies the two produced ASTs as labelled graphs.
//!
//! Output is `-j/--json` or a human line; handlers always exit `Ok` (advisory).

use crate::reasoning::budget::{BudgetVector, verify_conservation};
use touring_intelligence::reasoning::{LabeledGraph, ToolGraph, consistency_gate, plan_tool_chain};

/// Collect every value following an occurrence of `flag` (repeatable flags).
fn flag_values<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == flag {
            if let Some(v) = args.get(i + 1) {
                out.push(v.as_str());
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

/// Parse the single value following `flag`, or return `default`.
fn flag_value<T: std::str::FromStr>(args: &[String], flag: &str, default: T) -> T {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Whether `-j`/`--json` was requested.
fn wants_json(args: &[String]) -> bool {
    args.iter().any(|a| a == "-j" || a == "--json")
}

/// Parse a CSV of up to six `u32` into a `BudgetVector`
/// (`tokens,wall_ms,subtasks,dependencies,max_retries,attempts_used`); missing
/// trailing fields default to `0`.
fn parse_budget(csv: &str) -> BudgetVector {
    let mut it = csv.split(',').map(|s| s.trim().parse::<u32>().unwrap_or(0));
    BudgetVector {
        tokens: it.next().unwrap_or(0),
        wall_ms: it.next().unwrap_or(0),
        subtasks: it.next().unwrap_or(0),
        dependencies: it.next().unwrap_or(0),
        max_retries: it.next().unwrap_or(0),
        attempts_used: it.next().unwrap_or(0),
    }
}

/// Build a [`LabeledGraph`] from a node-label CSV (`a,b,c`) and an edge spec
/// (`from,to;from,to`) of index pairs into the node list.
fn parse_graph(nodes_csv: &str, edges_spec: &str) -> LabeledGraph {
    let nodes: Vec<String> = nodes_csv
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .collect();
    let edges: Vec<(usize, usize)> = edges_spec
        .split(';')
        .filter_map(|pair| {
            let mut it = pair.split(',').map(|s| s.trim().parse::<usize>().ok());
            match (it.next().flatten(), it.next().flatten()) {
                (Some(a), Some(b)) => Some((a, b)),
                _ => None,
            }
        })
        .collect();
    LabeledGraph::new(nodes, edges)
}

/// **C11** — `touring budget-verify --root T,W,S,D,R,A --node ... [--node ...]`.
/// Verifies the decompose budget-conservation law `Σ nodes ≤ root` per dimension.
pub fn run_budget(args: &[String]) -> anyhow::Result<()> {
    let root = parse_budget(&flag_value::<String>(args, "--root", String::new()));
    let nodes: Vec<BudgetVector> = flag_values(args, "--node")
        .iter()
        .map(|s| parse_budget(s))
        .collect();
    let result = verify_conservation(&root, &nodes);

    if wants_json(args) {
        let violations = result
            .as_ref()
            .err()
            .map(|vs| {
                vs.iter()
                    .map(|v| {
                        serde_json::json!({
                            "dimension": v.dimension,
                            "allocated": v.allocated,
                            "root_budget": v.root_budget,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let json = serde_json::json!({
            "conserved": result.is_ok(),
            "nodes": nodes.len(),
            "violations": violations,
        });
        println!("{}", serde_json::to_string(&json)?);
    } else {
        match &result {
            Ok(()) => println!(
                "conserved: Σ {} node budgets ≤ root on all 6 dimensions",
                nodes.len()
            ),
            Err(vs) => {
                println!("OVER-COMMIT on {} dimension(s):", vs.len());
                for v in vs {
                    println!(
                        "  {} : allocated {} > root {}",
                        v.dimension, v.allocated, v.root_budget
                    );
                }
            }
        }
    }
    Ok(())
}

/// **C12** — `touring plan-chain --edge from,to,cost [--edge ...] --start N --goal N [--max-steps N]`.
/// Plans a low-cost tool chain over the weighted tool graph via the MCTS engine.
pub fn run_plan_chain(args: &[String]) -> anyhow::Result<()> {
    let mut graph = ToolGraph::new();
    for e in flag_values(args, "--edge") {
        let parts: Vec<&str> = e.split(',').map(|s| s.trim()).collect();
        if let [f, t, c] = parts[..]
            && let (Ok(f), Ok(t), Ok(c)) = (f.parse::<u64>(), t.parse::<u64>(), c.parse::<u32>())
        {
            graph.add_edge(f, t, c);
        }
    }
    let start = flag_value::<u64>(args, "--start", 0);
    let goal = flag_value::<u64>(args, "--goal", 0);
    let max_steps = flag_value::<usize>(args, "--max-steps", 8);
    let plan = plan_tool_chain(&graph, start, goal, max_steps);

    if wants_json(args) {
        let json = serde_json::json!({
            "chain": plan.chain,
            "total_cost": plan.total_cost,
            "reached_goal": plan.reached_goal,
        });
        println!("{}", serde_json::to_string(&json)?);
    } else {
        let chain = plan
            .chain
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(" → ");
        println!("chain: {chain}");
        println!(
            "cost: {}  reached_goal: {}",
            plan.total_cost, plan.reached_goal
        );
    }
    Ok(())
}

/// **C14** — `touring consistency --a-nodes a,b --a-edges 0,1;1,2 --b-nodes ... --b-edges ...
/// [--alpha f] [--threshold f]`. Gates a parallel-engineer merge on GED + cosine distance.
pub fn run_consistency(args: &[String]) -> anyhow::Result<()> {
    let a = parse_graph(
        &flag_value::<String>(args, "--a-nodes", String::new()),
        &flag_value::<String>(args, "--a-edges", String::new()),
    );
    let b = parse_graph(
        &flag_value::<String>(args, "--b-nodes", String::new()),
        &flag_value::<String>(args, "--b-edges", String::new()),
    );
    let alpha = flag_value::<f64>(args, "--alpha", 0.5);
    let threshold = flag_value::<f64>(args, "--threshold", 0.2);
    let verdict = consistency_gate(&a, &b, None, None, alpha, threshold);

    if wants_json(args) {
        let json = serde_json::json!({
            "ged": verdict.ged,
            "cosine_sim": verdict.cosine_sim,
            "distance": verdict.distance,
            "consistent": verdict.consistent,
        });
        println!("{}", serde_json::to_string(&json)?);
    } else {
        println!(
            "ged={}  distance={:.3}  consistent={}",
            verdict.ged, verdict.distance, verdict.consistent
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn parse_budget_fills_missing_with_zero() {
        let b = parse_budget("100,200,3");
        assert_eq!(b.tokens, 100);
        assert_eq!(b.wall_ms, 200);
        assert_eq!(b.subtasks, 3);
        assert_eq!(b.dependencies, 0);
    }

    #[test]
    fn parse_graph_builds_nodes_and_edges() {
        let g = parse_graph("fn:foo,let,ret", "0,1;1,2");
        assert_eq!(g.node_labels, vec!["fn:foo", "let", "ret"]);
        assert_eq!(g.edges, vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn budget_conserved_and_over_commit() {
        // Conserved: every dim sum ≤ root.
        assert!(
            run_budget(&args(&[
                "budget-verify",
                "--root",
                "1000,5000,10,0,0,0",
                "--node",
                "300,1000,3",
                "--node",
                "400,2000,4"
            ]))
            .is_ok()
        );
        // Over-commit (subtasks 6 > 5): handler still exits Ok (advisory) but the engine reports it.
        let root = parse_budget("1000,5000,5");
        let nodes = [parse_budget("300,1000,3"), parse_budget("400,2000,3")];
        assert!(verify_conservation(&root, &nodes).is_err());
    }

    #[test]
    fn plan_chain_reaches_goal() {
        // 1→2→3→99 forced chain; handler must exit Ok and the engine must reach the goal.
        assert!(
            run_plan_chain(&args(&[
                "plan-chain",
                "--edge",
                "1,2,1",
                "--edge",
                "2,3,1",
                "--edge",
                "3,99,1",
                "--start",
                "1",
                "--goal",
                "99"
            ]))
            .is_ok()
        );
        let mut g = ToolGraph::new();
        g.add_edge(1, 2, 1);
        g.add_edge(2, 3, 1);
        g.add_edge(3, 99, 1);
        let plan = plan_tool_chain(&g, 1, 99, 8);
        assert!(plan.reached_goal);
        assert_eq!(plan.chain, vec![1, 2, 3, 99]);
    }

    #[test]
    fn consistency_identical_is_consistent_divergent_is_not() {
        assert!(
            run_consistency(&args(&[
                "consistency",
                "--a-nodes",
                "x,y",
                "--a-edges",
                "0,1",
                "--b-nodes",
                "x,y",
                "--b-edges",
                "0,1"
            ]))
            .is_ok()
        );
        // identical → consistent
        let a = parse_graph("x,y", "0,1");
        let v = consistency_gate(&a, &a, None, None, 0.5, 0.2);
        assert!(v.consistent && v.ged == 0);
        // divergent → gated
        let b = parse_graph("p,q,r", "0,1;0,2");
        let v2 = consistency_gate(&a, &b, None, None, 0.5, 0.2);
        assert!(!v2.consistent && v2.ged > 0);
    }

    #[test]
    fn flag_helpers_work() {
        let a = args(&["x", "--root", "1,2,3", "--node", "4", "--node", "5", "-j"]);
        assert_eq!(flag_value::<String>(&a, "--root", String::new()), "1,2,3");
        assert_eq!(flag_values(&a, "--node"), vec!["4", "5"]);
        assert!(wants_json(&a));
    }
}
