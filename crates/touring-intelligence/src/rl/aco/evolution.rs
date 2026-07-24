//! EvolutionPackage population logic (ACO-2).
//!
//! After graph execution, extracts learned patterns from successful nodes,
//! anti-patterns from failed nodes, and proposes system upgrades based on
//! quality metrics deltas.

use super::graph::MutableGeneratorGraph;
use super::models::{EvolutionPackage, ExecutionStatus, LearnedPattern, SystemUpgrade};
use std::collections::HashMap;
use std::path::Path;

/// Extract learned patterns from nodes that executed successfully.
///
/// Each successful node becomes a LearnedPattern: the node description is
/// the "pattern" that worked, tagged with the node ID and domain "aco".
pub fn extract_learned_patterns(graph: &MutableGeneratorGraph) -> Vec<LearnedPattern> {
    graph
        .iter()
        .filter(|(nid, _)| matches!(graph.execution_status(nid), Some(ExecutionStatus::Success)))
        .map(|(nid, node)| LearnedPattern {
            pattern_id: format!("pat_{nid}"),
            description: node.description.clone(),
            generator_template: String::new(),
            domain: "aco".to_string(),
            tags: vec![nid.to_string()],
        })
        .collect()
}

/// Discover anti-patterns from nodes that failed execution or validation.
///
/// Returns human-readable descriptions of what went wrong.
pub fn discover_anti_patterns(graph: &MutableGeneratorGraph) -> Vec<String> {
    graph
        .iter()
        .filter(|(nid, _)| {
            matches!(
                graph.execution_status(nid),
                Some(ExecutionStatus::ExecutionFailed)
                    | Some(ExecutionStatus::ValidationFailed)
                    | Some(ExecutionStatus::PreconditionFailed)
            )
        })
        .map(|(nid, node)| {
            let status = graph.execution_status(nid).map_or("Unknown", |s| match s {
                ExecutionStatus::ExecutionFailed => "ExecutionFailed",
                ExecutionStatus::ValidationFailed => "ValidationFailed",
                ExecutionStatus::PreconditionFailed => "PreconditionFailed",
                _ => "Other",
            });
            format!("[{status}] {nid}: {}", node.description)
        })
        .collect()
}

/// Propose system upgrades based on quality metrics.
///
/// Heuristic: if any metric is below 0.7, propose an upgrade for that component.
pub fn propose_system_upgrades(quality_metrics: &HashMap<String, f64>) -> Vec<SystemUpgrade> {
    quality_metrics
        .iter()
        .filter(|&(_, &v)| v < 0.7)
        .map(|(component, &value)| SystemUpgrade {
            component: component.clone(),
            change: format!("Improve {component} (current: {value:.2}, target: 0.7+)"),
            rationale: format!("{component} is below threshold at {value:.2}"),
        })
        .collect()
}

/// Populate a complete EvolutionPackage from a graph execution result.
///
/// This is the main entry point for ACO-2: after all nodes have been
/// executed (via `mark_executed` or `transition_status`), call this to
/// extract patterns, anti-patterns, and upgrade proposals.
pub fn populate_evolution_package(
    graph: &MutableGeneratorGraph,
    session_id: &str,
    objective_hash: &str,
    quality_metrics: HashMap<String, f64>,
) -> EvolutionPackage {
    let learned_patterns = extract_learned_patterns(graph);
    let anti_patterns_discovered = discover_anti_patterns(graph);
    let system_upgrades = propose_system_upgrades(&quality_metrics);
    let execution_report = graph.execution_report();
    let execution_report_json = serde_json::to_string(&execution_report).unwrap_or_default();

    let persistence_actions: Vec<String> = learned_patterns
        .iter()
        .map(|p| format!("store_pattern:{}", p.pattern_id))
        .collect();

    EvolutionPackage {
        session_id: session_id.to_string(),
        objective_hash: objective_hash.to_string(),
        execution_report_json,
        learned_patterns,
        anti_patterns_discovered,
        system_upgrades,
        quality_metrics,
        persistence_actions,
    }
}

// ---------------------------------------------------------------------------
// IL-2: EvolutionPersistor — cross-session persistence for EvolutionPackage
// ---------------------------------------------------------------------------

/// IL-2: Error type for persistence operations.
#[derive(Debug)]
pub enum PersistError {
    /// I/O error (file not found, permission denied, etc.)
    Io(String),
    /// Serialisation / deserialisation error.
    Serialization(String),
}

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "IO error: {msg}"),
            Self::Serialization(msg) => write!(f, "Serialization error: {msg}"),
        }
    }
}

/// IL-2: Trait for persisting [`EvolutionPackage`]s across sessions.
///
/// Implement this trait to store evolution packages in any backend
/// (file system, SQLite, in-memory, etc.).
pub trait EvolutionPersistor: Send + Sync {
    /// Persist `package` to the given path/key.
    fn persist(&self, package: &EvolutionPackage, path: &Path) -> Result<(), PersistError>;
    /// Load a previously persisted package.
    fn load(&self, path: &Path) -> Result<EvolutionPackage, PersistError>;
    /// Return `true` if a package exists at the given path/key.
    fn exists(&self, path: &Path) -> bool;
}

/// IL-2: File-based [`EvolutionPersistor`] using JSON serialisation.
///
/// Writes the package as pretty-printed JSON. Suitable for debugging and
/// cross-session pattern reuse without a database dependency.
pub struct FileEvolutionPersistor;

impl EvolutionPersistor for FileEvolutionPersistor {
    fn persist(&self, package: &EvolutionPackage, path: &Path) -> Result<(), PersistError> {
        let json = serde_json::to_string_pretty(package)
            .map_err(|e| PersistError::Serialization(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| PersistError::Io(e.to_string()))
    }

    fn load(&self, path: &Path) -> Result<EvolutionPackage, PersistError> {
        let data = std::fs::read_to_string(path).map_err(|e| PersistError::Io(e.to_string()))?;
        serde_json::from_str(&data).map_err(|e| PersistError::Serialization(e.to_string()))
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

// ---------------------------------------------------------------------------
// INS-L5: Pattern clustering
// ---------------------------------------------------------------------------

/// INS-L5: Merge patterns with Jaccard tag similarity above `threshold`.
///
/// For each pair (a, b) where `jaccard(a.tags, b.tags) >= threshold`:
/// - The pattern with fewer tags absorbs the other's tags (union).
/// - The domain is kept from the pattern that appears more frequently
///   (first occurrence wins when counts are equal).
/// - Duplicate pattern IDs are removed.
///
/// Returns the de-duplicated, clustered pattern list.
pub fn cluster_similar_patterns(patterns: &mut Vec<LearnedPattern>, threshold: f64) {
    if patterns.len() < 2 {
        return;
    }

    // Track which indices have been merged into another.
    let n = patterns.len();
    let mut merged_into: Vec<Option<usize>> = vec![None; n];

    for i in 0..n {
        if merged_into.get(i).copied().flatten().is_some() {
            continue;
        }
        for j in (i + 1)..n {
            if merged_into.get(j).copied().flatten().is_some() {
                continue;
            }
            let tags_i = patterns.get(i).map(|p| p.tags.clone()).unwrap_or_default();
            let tags_j = patterns.get(j).map(|p| p.tags.clone()).unwrap_or_default();
            let sim = jaccard_tag_similarity(&tags_i, &tags_j);
            if sim >= threshold {
                // Merge j into i: union of tags.
                let extra_tags: Vec<String> = tags_j
                    .iter()
                    .filter(|t| !tags_i.contains(t))
                    .cloned()
                    .collect();
                if let Some(p) = patterns.get_mut(i) {
                    p.tags.extend(extra_tags);
                }
                if let Some(slot) = merged_into.get_mut(j) {
                    *slot = Some(i);
                }
            }
        }
    }

    // Retain only patterns that were not merged into another.
    let mut idx = 0;
    patterns.retain(|_| {
        let keep = merged_into.get(idx).copied().flatten().is_none();
        idx += 1;
        keep
    });
}

/// Compute Jaccard similarity between two tag lists.
fn jaccard_tag_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let set_a: std::collections::HashSet<&str> = a.iter().map(String::as_str).collect();
    let set_b: std::collections::HashSet<&str> = b.iter().map(String::as_str).collect();
    let intersection = set_a.iter().filter(|t| set_b.contains(*t)).count();
    let union = set_a.len() + set_b.len() - intersection;
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::aco::graph::MutableGeneratorGraph;
    use crate::rl::aco::models::{
        ExecutionStatus, GeneratorContract, GeneratorNode, GeneratorOutputs, GeneratorType,
    };

    fn make_node(id: &str, desc: &str, depends_on: Vec<&str>) -> GeneratorNode {
        GeneratorNode {
            id: id.to_string(),
            description: desc.to_string(),
            generator_type: GeneratorType::Template,
            inputs_data: vec![],
            inputs_templates: vec![],
            inputs_constraints: vec![],
            outputs: GeneratorOutputs {
                execution_script: format!("exec_{id}.py"),
                validation_script: format!("val_{id}.py"),
                rollback_script: format!("rb_{id}.py"),
            },
            contract: GeneratorContract {
                precondition: "pre".into(),
                postcondition: "post".into(),
                invariant: "inv".into(),
            },
            acceptance_criteria: "pass".into(),
            depends_on: depends_on.into_iter().map(String::from).collect(),
        }
    }

    fn make_executed_graph() -> MutableGeneratorGraph {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("A", "Prepare data", vec![])).unwrap();
        g.add_node(make_node("B", "Process data", vec!["A"]))
            .unwrap();
        g.add_node(make_node("C", "Validate output", vec!["B"]))
            .unwrap();
        g.mark_executed("A", ExecutionStatus::Success).unwrap();
        g.mark_executed("B", ExecutionStatus::ExecutionFailed)
            .unwrap();
        g.mark_executed("C", ExecutionStatus::ValidationFailed)
            .unwrap();
        g
    }

    #[test]
    fn test_extract_patterns_from_success_nodes() {
        let g = make_executed_graph();
        let patterns = extract_learned_patterns(&g);
        assert_eq!(patterns.len(), 1); // Only A succeeded
        assert_eq!(patterns[0].pattern_id, "pat_A");
        assert_eq!(patterns[0].description, "Prepare data");
    }

    #[test]
    fn test_discover_anti_patterns_from_failed_nodes() {
        let g = make_executed_graph();
        let anti = discover_anti_patterns(&g);
        assert_eq!(anti.len(), 2); // B and C failed
        assert!(anti.iter().any(|a| a.contains("B")));
        assert!(anti.iter().any(|a| a.contains("C")));
    }

    #[test]
    fn test_propose_upgrades_from_low_metrics() {
        let mut metrics = HashMap::new();
        metrics.insert("coverage".to_string(), 0.5);
        metrics.insert("performance".to_string(), 0.9);
        let upgrades = propose_system_upgrades(&metrics);
        assert_eq!(upgrades.len(), 1); // Only coverage < 0.7
        assert_eq!(upgrades[0].component, "coverage");
    }

    #[test]
    fn test_populate_from_successful_execution() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("X", "Do work", vec![])).unwrap();
        g.mark_executed("X", ExecutionStatus::Success).unwrap();

        let pkg = populate_evolution_package(&g, "sess_1", "hash_1", HashMap::new());
        assert_eq!(pkg.session_id, "sess_1");
        assert_eq!(pkg.learned_patterns.len(), 1);
        assert!(pkg.anti_patterns_discovered.is_empty());
        assert!(pkg.system_upgrades.is_empty());
    }

    #[test]
    fn test_populate_from_mixed_execution() {
        let g = make_executed_graph();
        let mut metrics = HashMap::new();
        metrics.insert("reliability".to_string(), 0.3);

        let pkg = populate_evolution_package(&g, "sess_2", "hash_2", metrics);
        assert_eq!(pkg.learned_patterns.len(), 1);
        assert_eq!(pkg.anti_patterns_discovered.len(), 2);
        assert_eq!(pkg.system_upgrades.len(), 1);
        assert!(!pkg.persistence_actions.is_empty());
    }

    #[test]
    fn test_empty_graph_produces_empty_package() {
        let g = MutableGeneratorGraph::new();
        let pkg = populate_evolution_package(&g, "s", "h", HashMap::new());
        assert!(pkg.learned_patterns.is_empty());
        assert!(pkg.anti_patterns_discovered.is_empty());
        assert!(pkg.system_upgrades.is_empty());
    }

    #[test]
    fn test_integration_full_cycle() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("A", "Init", vec![])).unwrap();
        g.add_node(make_node("B", "Build", vec!["A"])).unwrap();
        g.add_node(make_node("C", "Test", vec!["B"])).unwrap();

        // Execute A→success, B→success, C→failed
        g.mark_executed("A", ExecutionStatus::Success).unwrap();
        g.mark_executed("B", ExecutionStatus::Success).unwrap();
        g.mark_executed("C", ExecutionStatus::ExecutionFailed)
            .unwrap();

        let hash = g.compute_objective_hash();
        let pkg = populate_evolution_package(&g, "integ", &hash, HashMap::new());

        assert_eq!(pkg.learned_patterns.len(), 2); // A and B
        assert_eq!(pkg.anti_patterns_discovered.len(), 1); // C
        assert!(
            pkg.execution_report_json
                .contains("=== Execution Report ===")
        );
        assert!(g.verify_invariant(&pkg.objective_hash));
    }
}
