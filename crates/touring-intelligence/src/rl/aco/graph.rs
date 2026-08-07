//! ACO DAG — MutableGeneratorGraph with pheromone-enhanced routing.
//!
//! Provides a `petgraph`-backed directed acyclic graph of generator nodes
//! with integrated pheromone state via `UnifiedPheromoneBus`.

use crate::rl::aco::models::{ExecutionStatus, GeneratorNode, GeneratorType};
use crate::rl::aco::pheromone_bus::{PheroKey, UnifiedPheromoneBus};
use petgraph::Direction::Incoming;
use petgraph::visit::EdgeRef;
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Graph-level errors.
#[derive(Debug, Error)]
pub enum GraphError {
    /// Referenced node id is not present in the graph.
    #[error("node not found: {0}")]
    NodeNotFound(String),

    /// A node with the given id already exists.
    #[error("node already exists: {0}")]
    DuplicateNode(String),

    /// The requested edge would introduce a cycle.
    #[error("cycle detected")]
    CycleDetected,
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Graph analytics — density, fan-in/out, critical path.
#[derive(Debug, Clone)]
pub struct GraphMetrics {
    /// Total number of nodes in the graph.
    pub node_count: usize,
    /// Total number of edges in the graph.
    pub edge_count: usize,
    /// Edge density relative to a fully connected DAG.
    pub density: f64,
    /// Largest in-degree observed across all nodes.
    pub max_fan_in: usize,
    /// Largest out-degree observed across all nodes.
    pub max_fan_out: usize,
    /// Length of the longest dependency (critical) path.
    pub critical_path_length: usize,
}

impl Default for GraphMetrics {
    fn default() -> Self {
        Self {
            node_count: 0,
            edge_count: 0,
            density: 0.0,
            max_fan_in: 0,
            max_fan_out: 0,
            critical_path_length: 0,
        }
    }
}

/// Read-only view of a node's data.
#[derive(Debug, Clone)]
pub struct NodeDataView {
    /// The generator kind of the node, as a string label.
    pub kind: String,
}

// ---------------------------------------------------------------------------
// MutableGeneratorGraph
// ---------------------------------------------------------------------------

/// ACO generator DAG with integrated pheromone state.
///
/// Each node corresponds to a generator. Edges encode the dependency graph.
/// Pheromone levels track traversal quality and guide future routing decisions.
#[derive(Debug)]
pub struct MutableGeneratorGraph {
    /// petgraph directed graph.
    graph: petgraph::graph::DiGraph<GeneratorNode, ()>,
    /// node_id → petgraph node index.
    node_index_by_id: HashMap<String, petgraph::graph::NodeIndex>,
    /// petgraph node index → node_id.
    id_by_node_index: HashMap<petgraph::graph::NodeIndex, String>,
    /// Outbound: node_id → Vec of node_ids this node depends on.
    dependents: BTreeMap<String, Vec<String>>,
    /// Inbound: node_id → Vec of node_ids that depend on it.
    dependencies: BTreeMap<String, Vec<String>>,
    /// Execution status per node (separate from GeneratorNode to allow mutation).
    execution_status: HashMap<String, ExecutionStatus>,
    /// Pheromone backbone.
    pheromone: UnifiedPheromoneBus,
    /// Incremental generation counter (for snapshot invalidation).
    generation: u64,
}

impl MutableGeneratorGraph {
    /// Create a new empty graph.
    pub fn new() -> Self {
        Self {
            graph: petgraph::graph::DiGraph::new(),
            node_index_by_id: HashMap::new(),
            id_by_node_index: HashMap::new(),
            dependents: BTreeMap::new(),
            dependencies: BTreeMap::new(),
            execution_status: HashMap::new(),
            pheromone: UnifiedPheromoneBus::new(0.05),
            generation: 0,
        }
    }

    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Iterator over all (node_id, node) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &GeneratorNode)> {
        self.graph.node_indices().filter_map(move |idx| {
            let nid = self.id_by_node_index.get(&idx)?;
            let node = self.graph.node_weight(idx)?;
            Some((nid.as_str(), node))
        })
    }

    /// Get execution status for a node.
    pub fn execution_status(&self, id: &str) -> Option<&ExecutionStatus> {
        self.execution_status.get(id)
    }

    /// Set execution status for a node.
    pub fn set_execution_status(&mut self, id: String, status: ExecutionStatus) {
        self.execution_status.insert(id, status);
    }

    /// Transition a node's execution status, validating the state machine.
    ///
    /// Returns `Ok(())` if the transition is valid. Invalid transitions return `Err`.
    pub fn transition_status(&mut self, id: &str, next: ExecutionStatus) -> Result<(), GraphError> {
        let current = self
            .execution_status
            .get(id)
            .copied()
            .unwrap_or(ExecutionStatus::Pending);

        let valid = match (&current, &next) {
            // Pending can transition to Running or PreconditionFailed
            (ExecutionStatus::Pending, ExecutionStatus::Running) => true,
            (ExecutionStatus::Pending, ExecutionStatus::PreconditionFailed) => true,
            // Running can transition to Success, ValidationFailed, ExecutionFailed
            (ExecutionStatus::Running, ExecutionStatus::Success) => true,
            (ExecutionStatus::Running, ExecutionStatus::ValidationFailed) => true,
            (ExecutionStatus::Running, ExecutionStatus::ExecutionFailed) => true,
            // ExecutionFailed can transition to RollbackExecuted
            (ExecutionStatus::ExecutionFailed, ExecutionStatus::RollbackExecuted) => true,
            // RollbackExecuted can transition to RollbackFailed
            (ExecutionStatus::RollbackExecuted, ExecutionStatus::RollbackFailed) => true,
            // Success is terminal
            (ExecutionStatus::Success, _) => false,
            // RollbackFailed is terminal
            (ExecutionStatus::RollbackFailed, _) => false,
            // Same status is always valid (no-op)
            (a, b) if a == b => true,
            _ => false,
        };

        if valid {
            self.execution_status.insert(id.to_string(), next);
            self.generation += 1;
            Ok(())
        } else {
            Err(GraphError::NodeNotFound(format!(
                "invalid transition {:?} -> {:?}",
                current, next
            )))
        }
    }

    /// Get a reference to a node by id.
    pub fn get_node(&self, id: &str) -> Option<&GeneratorNode> {
        let idx = self.node_index_by_id.get(id)?;
        self.graph.node_weight(*idx)
    }

    /// Get a mutable reference to a node by id.
    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut GeneratorNode> {
        let idx = self.node_index_by_id.get(id)?;
        self.graph.node_weight_mut(*idx)
    }

    /// All node ids (in arbitrary order).
    pub fn node_ids(&self) -> Vec<String> {
        self.node_index_by_id.keys().cloned().collect()
    }

    /// Deterministic topological order (same input always gives same output).
    /// Uses node indices as secondary sort key for stability.
    pub fn topological_sort_deterministic(&self) -> Result<Vec<String>, GraphError> {
        let sorted =
            petgraph::algo::toposort(&self.graph, None).map_err(|_| GraphError::CycleDetected)?;
        let mut result: Vec<String> = sorted
            .into_iter()
            .filter_map(|idx| self.id_by_node_index.get(&idx).cloned())
            .collect();
        // Secondary sort by node ID for deterministic ordering of parallel nodes
        result.sort();
        Ok(result)
    }

    /// Alias for `topological_sort_deterministic`.
    pub fn topological_order(&self) -> Result<Vec<String>, GraphError> {
        self.topological_sort_deterministic()
    }

    /// Compute graph analytics (density, fan-in/out, critical path).
    pub fn compute_graph_metrics(&self) -> GraphMetrics {
        let n = self.graph.node_count();
        let e = self.graph.edge_count();
        let density = if n > 1 {
            e as f64 / (n * (n - 1)) as f64
        } else {
            0.0
        };

        let mut fan_in = HashMap::new();
        let mut fan_out = HashMap::new();
        for nid in self.node_index_by_id.keys() {
            fan_in.entry(nid.clone()).or_insert(0);
            fan_out.entry(nid.clone()).or_insert(0);
        }

        for edge in self.graph.edge_indices() {
            if let Some((src, dst)) = self.graph.edge_endpoints(edge)
                && let (Some(sid), Some(did)) = (
                    self.id_by_node_index.get(&src),
                    self.id_by_node_index.get(&dst),
                )
            {
                *fan_out.entry(sid.clone()).or_insert(0) += 1;
                *fan_in.entry(did.clone()).or_insert(0) += 1;
            }
        }

        let max_fan_in = *fan_in.values().max().unwrap_or(&0);
        let max_fan_out = *fan_out.values().max().unwrap_or(&0);

        // Critical path = longest path in DAG (using BFS/DFS longest path)
        let critical_path_length = self.critical_path_length();

        GraphMetrics {
            node_count: n,
            edge_count: e,
            density,
            max_fan_in,
            max_fan_out,
            critical_path_length,
        }
    }

    /// Longest dependency chain length (for critical path length metric).
    fn critical_path_length(&self) -> usize {
        let topo = match petgraph::algo::toposort(&self.graph, None) {
            Ok(t) => t,
            Err(_) => return 0,
        };

        let mut dist: HashMap<petgraph::graph::NodeIndex, usize> = HashMap::new();
        let mut max_dist = 0usize;

        for idx in topo {
            let mut max_pred = 0usize;
            for edge in self.graph.edges_directed(idx, Incoming) {
                let src = edge.source();
                max_pred = max_pred.max(*dist.get(&src).unwrap_or(&0));
            }
            let d = max_pred + 1;
            dist.insert(idx, d);
            max_dist = max_dist.max(d);
        }
        max_dist.saturating_sub(1)
    }

    /// Return the critical (longest dependency) path as node IDs.
    pub fn get_critical_path(&self) -> Result<Vec<String>, GraphError> {
        let topo =
            petgraph::algo::toposort(&self.graph, None).map_err(|_| GraphError::CycleDetected)?;

        let mut dist: HashMap<petgraph::graph::NodeIndex, usize> = HashMap::new();
        let mut parent: HashMap<petgraph::graph::NodeIndex, petgraph::graph::NodeIndex> =
            HashMap::new();
        let mut max_dist = 0usize;
        let mut end_node = *topo.first().unwrap_or(&petgraph::graph::NodeIndex::new(0));

        // Build incoming edge lookup: for each node, which nodes point TO it
        let mut incoming: HashMap<petgraph::graph::NodeIndex, Vec<petgraph::graph::NodeIndex>> =
            HashMap::new();
        for edge in self.graph.edge_indices() {
            if let Some((src, dst)) = self.graph.edge_endpoints(edge) {
                incoming.entry(dst).or_default().push(src);
            }
        }

        for idx in &topo {
            let preds = incoming.get(idx).map(|v| v.as_slice()).unwrap_or(&[]);
            let mut max_pred = 0usize;
            let mut best_pred = None;
            for &src in preds {
                let pred_dist = *dist.get(&src).unwrap_or(&0);
                if pred_dist >= max_pred {
                    max_pred = pred_dist;
                    best_pred = Some(src);
                }
            }
            dist.insert(*idx, max_pred + 1);
            if max_pred + 1 > max_dist {
                max_dist = max_pred + 1;
                end_node = *idx;
            }
            if let Some(bp) = best_pred {
                parent.insert(*idx, bp);
            }
        }

        // Reconstruct path
        let mut path = Vec::new();
        let mut current = end_node;
        // Limit to prevent infinite loop if something is wrong
        for _ in 0..self.graph.node_count() {
            if let Some(nid) = self.id_by_node_index.get(&current).cloned() {
                path.push(nid);
            }
            match parent.get(&current) {
                Some(&prev) => current = prev,
                None => break,
            }
        }
        path.reverse();
        Ok(path)
    }

    /// Snapshot generation — increments each time the graph mutates.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Human-readable execution report with a header.
    pub fn execution_report(&self) -> String {
        let mut lines = vec!["=== Execution Report ===".to_string()];
        for (id, status) in &self.execution_status {
            lines.push(format!("  {id}: {status}"));
        }
        lines.join("\n")
    }

    /// Mark a node as executed with the given status.
    ///
    /// Auto-transitions through Running if needed: Pending → Running → terminal_status.
    pub fn mark_executed(&mut self, id: &str, status: ExecutionStatus) -> Result<(), GraphError> {
        if !self.node_index_by_id.contains_key(id) {
            return Err(GraphError::NodeNotFound(id.into()));
        }
        // Auto-transition through Running for Pending → terminal transitions
        let current = self
            .execution_status
            .get(id)
            .copied()
            .unwrap_or(ExecutionStatus::Pending);
        if current == ExecutionStatus::Pending
            && matches!(
                status,
                ExecutionStatus::Success
                    | ExecutionStatus::ValidationFailed
                    | ExecutionStatus::ExecutionFailed
                    | ExecutionStatus::PreconditionFailed
            )
        {
            self.transition_status(id, ExecutionStatus::Running)?;
        }
        self.transition_status(id, status)
    }

    /// Compute a hash of the current graph state for objective tracking.
    ///
    /// Hashes: node count + edge count + sorted node IDs + generation counter.
    pub fn compute_objective_hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.node_count().hash(&mut hasher);
        self.edge_count().hash(&mut hasher);
        self.generation.hash(&mut hasher);
        let mut sorted_ids: Vec<_> = self.node_ids();
        sorted_ids.sort();
        sorted_ids.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Verify that the graph's objective hash matches the expected value.
    ///
    /// Returns `true` if the computed hash equals the expected hash.
    pub fn verify_invariant(&self, expected_hash: &str) -> bool {
        self.compute_objective_hash() == expected_hash
    }

    // ── Node mutation ─────────────────────────────────────────────────────────

    /// Add a node using a pre-built GeneratorNode.
    ///
    /// Also creates edges for any `depends_on` relationships in the node.
    pub fn add_node(&mut self, node: GeneratorNode) -> Result<(), GraphError> {
        let id = node.id.clone();
        if self.node_index_by_id.contains_key(&id) {
            return Err(GraphError::DuplicateNode(id));
        }

        // Capture depends_on before we consume the node
        let depends_on = node.depends_on.clone();

        let node_idx = self.graph.add_node(node);
        self.node_index_by_id.insert(id.clone(), node_idx);
        self.id_by_node_index.insert(node_idx, id.clone());
        self.dependents.insert(id.clone(), Vec::new());
        self.dependencies.insert(id.clone(), Vec::new());
        self.execution_status
            .insert(id.clone(), ExecutionStatus::Pending);
        self.generation += 1;

        // Process dependencies: for each dep, add edge dep -> this_node
        for dep_id in &depends_on {
            // Auto-create missing dependency nodes as stubs
            if !self.node_index_by_id.contains_key(dep_id) {
                let stub = GeneratorNode {
                    id: dep_id.clone(),
                    description: format!("auto-created dependency of {}", id),
                    generator_type: GeneratorType::Template,
                    inputs_data: Vec::new(),
                    inputs_templates: Vec::new(),
                    inputs_constraints: Vec::new(),
                    outputs: Default::default(),
                    contract: Default::default(),
                    acceptance_criteria: String::new(),
                    depends_on: Vec::new(),
                };
                let dep_idx = self.graph.add_node(stub);
                self.node_index_by_id.insert(dep_id.clone(), dep_idx);
                self.id_by_node_index.insert(dep_idx, dep_id.clone());
                self.dependents.insert(dep_id.clone(), Vec::new());
                self.dependencies.insert(dep_id.clone(), Vec::new());
                self.execution_status
                    .insert(dep_id.clone(), ExecutionStatus::Pending);
            }

            if let (Some(&dep_idx), Some(&node_idx)) = (
                self.node_index_by_id.get(dep_id),
                self.node_index_by_id.get(&id),
            ) {
                self.graph.add_edge(dep_idx, node_idx, ());
                self.dependents
                    .entry(dep_id.clone())
                    .or_default()
                    .push(id.clone());
                self.dependencies
                    .entry(id.clone())
                    .or_default()
                    .push(dep_id.clone());
            }
        }

        Ok(())
    }

    /// Add a node with auto-generated fields (convenience form for tests).
    ///
    /// Deposits an initial pheromone trail of 0.0 for this node.
    pub fn add_node_with_deps(
        &mut self,
        id: impl Into<String>,
        kind: impl Into<String>,
        deps: Option<Vec<String>>,
    ) -> Result<(), GraphError> {
        let id_str = id.into();
        let kind_str = kind.into();

        if self.node_index_by_id.contains_key(&id_str) {
            return Err(GraphError::DuplicateNode(id_str));
        }

        let node = GeneratorNode {
            id: id_str.clone(),
            description: String::new(),
            generator_type: GeneratorType::Template,
            inputs_data: Vec::new(),
            inputs_templates: Vec::new(),
            inputs_constraints: Vec::new(),
            outputs: Default::default(),
            contract: Default::default(),
            acceptance_criteria: String::new(),
            depends_on: deps.clone().unwrap_or_default(),
        };

        let node_idx = self.graph.add_node(node);
        self.node_index_by_id.insert(id_str.clone(), node_idx);
        self.id_by_node_index.insert(node_idx, id_str.clone());
        self.dependents.insert(id_str.clone(), Vec::new());
        self.dependencies.insert(id_str.clone(), Vec::new());
        self.execution_status
            .insert(id_str.clone(), ExecutionStatus::Pending);
        self.generation += 1;

        // Initialize pheromone for this node
        self.pheromone.deposit(
            PheroKey::ActionPair(hash_string(&id_str), hash_string(&kind_str)),
            0.0,
        );

        // Handle dependencies: for each dep, add edge dep -> this_node
        if let Some(dep_list) = deps {
            for dep_id in &dep_list {
                if !self.node_index_by_id.contains_key(dep_id) {
                    let placeholder = GeneratorNode {
                        id: dep_id.clone(),
                        description: format!("auto-created dependency of {}", id_str),
                        generator_type: GeneratorType::Template,
                        inputs_data: Vec::new(),
                        inputs_templates: Vec::new(),
                        inputs_constraints: Vec::new(),
                        outputs: Default::default(),
                        contract: Default::default(),
                        acceptance_criteria: String::new(),
                        depends_on: Vec::new(),
                    };
                    let dep_idx = self.graph.add_node(placeholder);
                    self.node_index_by_id.insert(dep_id.clone(), dep_idx);
                    self.id_by_node_index.insert(dep_idx, dep_id.clone());
                    self.dependents.insert(dep_id.clone(), Vec::new());
                    self.dependencies.insert(dep_id.clone(), Vec::new());
                    self.execution_status
                        .insert(dep_id.clone(), ExecutionStatus::Pending);
                }

                self.graph.add_edge(
                    *self
                        .node_index_by_id
                        .get(dep_id)
                        .expect("dep node indexed before edge add"),
                    node_idx,
                    (),
                );

                self.dependents
                    .entry(dep_id.clone())
                    .or_default()
                    .push(id_str.clone());
                self.dependencies
                    .entry(id_str.clone())
                    .or_default()
                    .push(dep_id.clone());
            }
        }

        Ok(())
    }

    /// Add an edge. Auto-creates missing endpoint nodes as stubs.
    pub fn add_edge(
        &mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        _weight: Option<f64>,
    ) -> Result<(), GraphError> {
        let from_str = from.into();
        let to_str = to.into();

        let from_idx = *self
            .node_index_by_id
            .get(&from_str)
            .ok_or_else(|| GraphError::NodeNotFound(from_str.clone()))?;
        let to_idx = *self
            .node_index_by_id
            .get(&to_str)
            .ok_or_else(|| GraphError::NodeNotFound(to_str.clone()))?;

        let mut test_graph = self.graph.clone();
        test_graph.add_edge(from_idx, to_idx, ());
        if petgraph::algo::toposort(&test_graph, None).is_err() {
            return Err(GraphError::CycleDetected);
        }

        self.graph.add_edge(from_idx, to_idx, ());
        self.dependents
            .entry(from_str.clone())
            .or_default()
            .push(to_str.clone());
        self.dependencies
            .entry(to_str.clone())
            .or_default()
            .push(from_str.clone());
        self.generation += 1;

        Ok(())
    }

    /// Remove a node and all its incident edges.
    pub fn remove_node(&mut self, id: impl Into<String>) -> Result<(), GraphError> {
        let id_str = id.into();
        let idx = *self
            .node_index_by_id
            .get(&id_str)
            .ok_or_else(|| GraphError::NodeNotFound(id_str.clone()))?;

        self.graph.remove_node(idx);
        self.node_index_by_id.remove(&id_str);
        self.id_by_node_index.remove(&idx);
        self.dependents.remove(&id_str);
        self.dependencies.remove(&id_str);
        self.execution_status.remove(&id_str);

        for deps in self.dependencies.values_mut() {
            deps.retain(|d| d != &id_str);
        }
        for succs in self.dependents.values_mut() {
            succs.retain(|s| s != &id_str);
        }
        self.generation += 1;

        Ok(())
    }

    // ── Pheromone ─────────────────────────────────────────────────────────────

    /// Current pheromone level for a node (0.0 if absent).
    pub fn get_pheromone(&self, id: &str) -> f64 {
        let node_idx = self.node_index_by_id.get(id);
        match node_idx {
            Some(idx) => {
                let nid_str = self
                    .id_by_node_index
                    .get(idx)
                    .map(|s| s.as_str())
                    .unwrap_or(id);
                let kind_name = self
                    .graph
                    .node_weight(*idx)
                    .map(|n| format!("{:?}", n.generator_type))
                    .unwrap_or_default();
                self.pheromone.get(&PheroKey::ActionPair(
                    hash_string(nid_str),
                    hash_string(&kind_name),
                ))
            }
            None => self
                .pheromone
                .get(&PheroKey::ActionPair(hash_string(id), hash_string(""))),
        }
    }

    /// Update pheromone level for a node.
    pub fn update_pheromone(&mut self, id: &str, value: f64) -> Result<(), GraphError> {
        if !self.node_index_by_id.contains_key(id) {
            return Err(GraphError::NodeNotFound(id.into()));
        }
        let idx = *self
            .node_index_by_id
            .get(id)
            .expect("guarded by contains_key above");
        let kind_name = self
            .graph
            .node_weight(idx)
            .map(|n| format!("{:?}", n.generator_type))
            .unwrap_or_default();
        self.pheromone.deposit(
            PheroKey::ActionPair(hash_string(id), hash_string(&kind_name)),
            value,
        );
        Ok(())
    }

    /// Set global evaporation rate.
    pub fn set_evaporation_rate(&mut self, rate: f64) {
        self.pheromone.set_evaporation_rate(rate);
    }

    /// Apply global evaporation to all trails.
    pub fn evaporate_all(&mut self) {
        self.pheromone.evaporate_all();
    }

    // ── Read-only views ───────────────────────────────────────────────────────

    /// Return a read-only view of a node's data.
    pub fn node_data(&self, id: String) -> Result<NodeDataView, GraphError> {
        let idx = *self
            .node_index_by_id
            .get(&id)
            .ok_or_else(|| GraphError::NodeNotFound(id.clone()))?;
        let node = self
            .graph
            .node_weight(idx)
            .ok_or(GraphError::NodeNotFound(id))?;
        Ok(NodeDataView {
            kind: format!("{:?}", node.generator_type),
        })
    }

    /// Return nodes that directly depend on `id`.
    pub fn successors(&mut self, id: String) -> Result<Vec<String>, GraphError> {
        if !self.node_index_by_id.contains_key(&id) {
            return Err(GraphError::NodeNotFound(id));
        }
        let succs: Vec<String> = self
            .dependencies
            .iter()
            .filter(|(_, deps)| deps.contains(&id))
            .map(|(node_id, _)| node_id.clone())
            .collect();
        Ok(succs)
    }

    // ---------------------------------------------------------------------
    // Python-facing API (touring-python/src/aco_bindings.rs calls these
    // method names. Keep them as thin wrappers over the canonical
    // implementations so the two APIs cannot drift.)
    // ---------------------------------------------------------------------

    /// Alias for [`Self::node_count`] — matches the `PyAcoGraph.__len__` binding.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.node_count()
    }

    /// Public deterministic topological order.
    pub fn topological_sort(&self) -> Result<Vec<String>, GraphError> {
        self.topological_sort_deterministic()
    }

    /// Validate the graph is acyclic. Returns `Err(CycleDetected)` if not.
    pub fn validate_acyclic(&self) -> Result<(), GraphError> {
        self.topological_sort_deterministic().map(|_| ())
    }

    /// Compute execution levels (Kahn-style BFS layers) deterministically.
    fn compute_levels(&self) -> Result<Vec<Vec<String>>, GraphError> {
        self.validate_acyclic()?;

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for id in self.node_index_by_id.keys() {
            in_degree.insert(id.clone(), 0);
        }
        for deps in self.dependencies.values() {
            for dep in deps {
                if let Some(entry) = in_degree.get_mut(dep) {
                    *entry += 1;
                }
            }
        }

        let mut levels: Vec<Vec<String>> = Vec::new();
        let mut ready: Vec<String> = in_degree
            .iter()
            .filter(|&(_, &d)| d == 0)
            .map(|(id, _)| id.clone())
            .collect();
        ready.sort();

        while !ready.is_empty() {
            levels.push(ready.clone());
            let mut next: Vec<String> = Vec::new();
            for id in &ready {
                if let Some(downstream) = self.dependents.get(id) {
                    for d in downstream {
                        if let Some(deg) = in_degree.get_mut(d)
                            && *deg > 0
                        {
                            *deg -= 1;
                            if *deg == 0 {
                                next.push(d.clone());
                            }
                        }
                    }
                }
                in_degree.remove(id);
            }
            next.sort();
            ready = next;
        }

        if !in_degree.is_empty() {
            return Err(GraphError::CycleDetected);
        }
        Ok(levels)
    }

    /// All execution levels (including single-node groups).
    pub fn get_all_levels(&self) -> Result<Vec<Vec<String>>, GraphError> {
        self.compute_levels()
    }

    /// Levels with 2+ nodes (parallel-execution candidates).
    pub fn detect_parallelizable(&self) -> Result<Vec<Vec<String>>, GraphError> {
        Ok(self
            .compute_levels()?
            .into_iter()
            .filter(|lvl| lvl.len() >= 2)
            .collect())
    }

    /// Return node ids whose `GeneratorContract` is incomplete.
    pub fn validate_contracts(&self) -> Vec<String> {
        self.graph
            .node_indices()
            .filter_map(|idx| {
                let node = self.graph.node_weight(idx)?;
                if !node.contract.is_complete() {
                    self.id_by_node_index.get(&idx).cloned()
                } else {
                    None
                }
            })
            .collect()
    }

    /// Return node ids whose `depends_on` list references a missing node.
    pub fn validate_dependencies(&self) -> Vec<String> {
        let known: std::collections::HashSet<&String> = self.node_index_by_id.keys().collect();
        self.graph
            .node_indices()
            .filter_map(|idx| {
                let node = self.graph.node_weight(idx)?;
                let has_unresolved = node.depends_on.iter().any(|dep_id| !known.contains(dep_id));
                if has_unresolved {
                    self.id_by_node_index.get(&idx).cloned()
                } else {
                    None
                }
            })
            .collect()
    }

    /// Immutable snapshot of the DAG with critical path + parallel groups.
    pub fn freeze(
        &self,
        objective_hash: Option<&str>,
    ) -> Result<crate::rl::aco::models::GeneratorGraphModel, GraphError> {
        let critical_path = self.get_critical_path()?;
        let parallelizable = self.detect_parallelizable()?;
        let nodes: Vec<crate::rl::aco::models::GeneratorNode> = self
            .graph
            .node_indices()
            .filter_map(|idx| self.graph.node_weight(idx).cloned())
            .collect();
        Ok(crate::rl::aco::models::GeneratorGraphModel {
            nodes,
            critical_path,
            parallelizable,
            objective_hash: objective_hash.unwrap_or("").to_string(),
        })
    }
}

impl Default for MutableGeneratorGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn hash_string(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn make_node(id: &str) -> GeneratorNode {
        GeneratorNode {
            id: id.into(),
            description: String::new(),
            generator_type: GeneratorType::Template,
            inputs_data: Vec::new(),
            inputs_templates: Vec::new(),
            inputs_constraints: Vec::new(),
            outputs: Default::default(),
            contract: Default::default(),
            acceptance_criteria: String::new(),
            depends_on: Vec::new(),
        }
    }

    #[test]
    fn graph_new_is_empty() {
        let g = MutableGeneratorGraph::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn graph_add_node_increments_count() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node_with_deps("gen_a", "echo", None).unwrap();
        assert_eq!(g.node_count(), 1);
    }

    #[test]
    fn graph_add_edge_increments_edge_count() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node_with_deps("gen_a", "echo", None).unwrap();
        g.add_node_with_deps("gen_b", "echo", None).unwrap();
        g.add_edge("gen_a", "gen_b", None).unwrap();
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn graph_topological_order_respects_edges() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node_with_deps("a", "echo", None).unwrap();
        g.add_node_with_deps("b", "echo", None).unwrap();
        g.add_node_with_deps("c", "echo", None).unwrap();
        g.add_edge("a", "b", None).unwrap();
        g.add_edge("b", "c", None).unwrap();
        let order = g.topological_sort_deterministic().unwrap();
        let pos = |id: &str| order.iter().position(|x| x == id).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("b") < pos("c"));
    }

    #[test]
    fn graph_cycle_detection_rejects_cycle() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node_with_deps("a", "echo", None).unwrap();
        g.add_node_with_deps("b", "echo", None).unwrap();
        g.add_node_with_deps("c", "echo", None).unwrap();
        g.add_edge("a", "b", None).unwrap();
        g.add_edge("b", "c", None).unwrap();
        let result = g.add_edge("c", "a", None);
        assert!(result.is_err());
    }

    #[test]
    fn graph_remove_node_removes_from_adjacency() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node_with_deps("a", "echo", None).unwrap();
        g.add_node_with_deps("b", "echo", None).unwrap();
        g.add_edge("a", "b", None).unwrap();
        g.remove_node("a").unwrap();
        assert_eq!(g.node_count(), 1);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn graph_get_pheromone_returns_default() {
        let g = MutableGeneratorGraph::new();
        let p = g.get_pheromone("missing");
        assert!((p - 0.0).abs() < 1e-9);
    }

    #[test]
    fn graph_update_pheromone_modifies_value() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node_with_deps("gen", "echo", None).unwrap();
        g.update_pheromone("gen", 0.8).unwrap();
        let p = g.get_pheromone("gen");
        assert!((p - 0.8).abs() < 1e-9);
    }

    #[test]
    fn graph_evaporate_all_decays_pheromones() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node_with_deps("gen", "echo", None).unwrap();
        g.update_pheromone("gen", 1.0).unwrap();
        g.set_evaporation_rate(0.5);
        g.evaporate_all();
        let p = g.get_pheromone("gen");
        assert!((p - 0.5).abs() < 1e-9);
    }

    #[test]
    fn graph_node_data_returns_correct_kind() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node_with_deps("gen", "test_kind", None).unwrap();
        let data = g.node_data("gen".to_string()).unwrap();
        assert_eq!(data.kind, "Template");
    }

    #[test]
    fn graph_successors_returns_direct_children() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node_with_deps("a", "echo", None).unwrap();
        g.add_node_with_deps("b", "echo", None).unwrap();
        g.add_node_with_deps("c", "echo", None).unwrap();
        g.add_edge("a", "b", None).unwrap();
        g.add_edge("a", "c", None).unwrap();
        let succ = g.successors("a".to_string()).unwrap();
        assert_eq!(succ.len(), 2);
    }
}
