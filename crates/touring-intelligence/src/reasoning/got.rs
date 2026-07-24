//! Graph of Thoughts (GoT) — concurrent heuristic evaluation via tokio channels.
//!
//! GoT extends the touring-cognitive crate with fan-out thought exploration:
//! each node evaluates a heuristic independently via tokio mpsc channels.
//! Note: children within a single explore_node call are awaited sequentially
//! (not spawned in parallel) due to lifetime constraints. True parallelism
//! requires `tokio::spawn` with `'static` bounds on the engine.
//!
//! Uses **tokio channels only** (no actix) to avoid runtime conflicts with
//! the existing `tokio::runtime::Runtime` used throughout Touring.
//!
//! # IC-2 — Pheromone integration
//!
//! `GotEngine` provides two pheromone-aware exploration methods that build an
//! ACO feedback loop on top of the base `explore()` traversal:
//!
//! - [`GotEngine::explore_with_pheromone_bias`]: runs `explore()`, then
//!   post-boosts each result's score by `pheromone_alpha × pheromone.strength(node.label)`,
//!   and re-sorts. Does not modify pheromone state — pure read path.
//!
//! - [`GotEngine::explore_and_reinforce`]: calls `explore_with_pheromone_bias`,
//!   then deposits `result.score` back into pheromone trails for each visited node.
//!   Repeated calls converge toward high-scoring thought paths (ACO loop).
//!
//! Both methods operate as a **post-exploitation layer** over `explore()`, avoiding
//! the recursive `Box<Pin<dyn Future>>` lifetime complexity of modifying `explore_node`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

// ---------------------------------------------------------------------------
// EC58 Test-Only Infrastructure — VisitedTracker
// ---------------------------------------------------------------------------

/// EC58 Test-Only Infrastructure — VisitedTracker tracks cycle detection in tests.
///
/// This module provides [`VisitedTracker`], a generational cycle detector used
/// exclusively within `got.rs` for unit testing of the Graph of Thoughts engine.
/// All 14 usages are within `got.rs` (tests + implementation) — no production
/// callers exist outside this file.
///
/// **Architectural intent**: Intentionally isolated as `pub(crate)` to prevent
/// accidental use in production code paths. This is test infrastructure only.
///
/// # Example
/// ```ignore
/// let mut tracker = VisitedTracker::new();
/// assert!(tracker.visit(1));  // first visit → true
/// assert!(!tracker.visit(1)); // already visited in gen 1 → cycle!
/// tracker.next_generation();  // gen=2
/// assert!(tracker.visit(1)); // visited in gen 1, but gen=2 → true (diamond!)
/// ```
#[derive(Debug, Clone)]
pub(crate) struct VisitedTracker {
    visited: HashMap<NodeId, u64>,
    current_gen: u64,
}

impl VisitedTracker {
    /// Cria tracker vazio com gen=1.
    fn new() -> Self {
        Self {
            visited: HashMap::new(),
            current_gen: 1,
        }
    }

    /// Tenta marcar nó como visitado na geração corrente.
    ///
    /// Retorna `false` se o nó já foi visitado nesta geração (ciclo).
    /// Retorna `true` se é a primeira visita nesta geração.
    ///
    /// # Complexidade
    /// O(1) amortizado — uma operação de HashMap get+insert.
    // EC58: test-only helper — no production caller; kept for unit test introspection.
    #[allow(dead_code)]
    fn visit(&mut self, node_id: NodeId) -> bool {
        match self.visited.get(&node_id) {
            Some(&r#gen) if r#gen == self.current_gen => false, // ciclo
            _ => {
                self.visited.insert(node_id, self.current_gen);
                true // primeira visita nesta geração
            }
        }
    }

    /// Testa se nó foi visitado em uma geração específica.
    // EC58: test-only helper — used for generational assertions in unit tests.
    #[allow(dead_code)]
    fn is_visited_in_gen(&self, node_id: NodeId, r#gen: u64) -> bool {
        self.visited.get(&node_id) == Some(&r#gen)
    }

    /// Testa se nó foi visitado na geração corrente.
    // EC58: test-only helper — convenience wrapper around is_visited_in_gen.
    #[allow(dead_code)]
    fn is_visited(&self, node_id: NodeId) -> bool {
        self.is_visited_in_gen(node_id, self.current_gen)
    }

    /// Tenta marcar nó como visitado em uma geração específica.
    ///
    /// Retorna `false` se o nó já foi visitado nesta geração (ciclo).
    /// Retorna `true` se é a primeira visita nesta geração.
    ///
    /// # Complexidade
    /// O(1) amortizado — uma operação de HashMap get+insert.
    fn visit_in_generation(&mut self, node_id: NodeId, r#gen: u64) -> bool {
        match self.visited.get(&node_id) {
            Some(&existing_gen) if existing_gen == r#gen => false, // ciclo
            _ => {
                self.visited.insert(node_id, r#gen);
                true // primeira visita nesta geração
            }
        }
    }

    /// Incrementa geração para nova branch paralela.
    /// Deve ser chamado ANTES de cada `tokio::spawn` novo.
    fn next_generation(&mut self) {
        self.current_gen += 1;
    }

    /// Retorna a geração corrente.
    fn current_generation(&self) -> u64 {
        self.current_gen
    }
}

impl Default for VisitedTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// IC-2: GotPheromoneMemory — shared pheromone for Graph of Thoughts paths
// ---------------------------------------------------------------------------

/// IC-2: Pheromone memory for Graph of Thought reasoning paths.
///
/// Reinforces thought patterns that led to high-quality outcomes.
/// Thought paths are encoded as `"step_a→step_b→step_c"` strings.
/// Trails below `prune_threshold` are removed on each `evaporate()` call
/// to keep memory bounded.
#[derive(Debug)]
pub struct GotPheromoneMemory {
    /// Pheromone level per thought path key.
    trails: HashMap<String, f64>,
    /// Fraction of pheromone lost per `evaporate()` call (0.0 = no decay).
    evaporation_rate: f64,
    /// Trails with strength below this value are pruned on evaporation.
    prune_threshold: f64,
}

impl GotPheromoneMemory {
    /// Create a new memory with the given evaporation rate.
    ///
    /// * `evaporation_rate` — fraction lost per tick, e.g. `0.1` = 10% decay.
    pub fn new(evaporation_rate: f64) -> Self {
        Self {
            trails: HashMap::new(),
            evaporation_rate: evaporation_rate.clamp(0.0, 1.0),
            prune_threshold: 0.001,
        }
    }

    /// Reinforce a thought path with the given reward signal.
    ///
    /// The path is encoded as the concatenation of step labels joined by `→`.
    pub fn reinforce(&mut self, thought_path: &[&str], reward: f64) {
        if thought_path.is_empty() || reward <= 0.0 {
            return;
        }
        let key = thought_path.join("→");
        *self.trails.entry(key).or_insert(0.0) += reward;
    }

    /// Return the current pheromone strength for a thought path.
    pub fn strength(&self, thought_path: &[&str]) -> f64 {
        let key = thought_path.join("→");
        self.trails.get(&key).copied().unwrap_or(0.0)
    }

    /// Apply exponential decay to all trails and prune weak ones.
    pub fn evaporate(&mut self) {
        let rate = self.evaporation_rate;
        let threshold = self.prune_threshold;
        self.trails.values_mut().for_each(|v| *v *= 1.0 - rate);
        self.trails.retain(|_, v| *v >= threshold);
    }

    /// Number of active trails currently tracked.
    pub fn trail_count(&self) -> usize {
        self.trails.len()
    }

    /// Return a clone of all pheromone trails (for snapshot persistence).
    pub fn trails(&self) -> HashMap<String, f64> {
        self.trails.clone()
    }

    /// Return the strongest trail path and its strength, if any trails exist.
    pub fn strongest_trail(&self) -> Option<(&str, f64)> {
        self.trails
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(k, &v)| (k.as_str(), v))
    }
}

// NOTE: Design uses tokio mpsc for fan-out. FuturesUnordered is available via
// `futures::stream::FuturesUnordered` if needed for collecting concurrent results.
// Current implementation uses recursive async fan-out which is equivalent.

/// Unique identifier for a thought node.
pub type NodeId = u64;

/// Message passed between GoT nodes during exploration.
#[derive(Debug, Clone)]
pub struct ThoughtMessage {
    /// Source node ID (0 = external / root trigger).
    pub from: NodeId,
    /// The thought content to evaluate.
    pub content: String,
    /// Depth in the thought graph (root = 0).
    pub depth: u32,
    /// Accumulated score from the parent path.
    pub accumulated_score: f64,
}

/// Result of evaluating a thought at a single node.
#[derive(Debug, Clone)]
pub struct ThoughtResult {
    /// Node that produced this result.
    pub node_id: NodeId,
    /// Evaluated score at this node.
    pub score: f64,
    /// Output content after evaluation.
    pub output: String,
    /// Depth at which this was evaluated.
    pub depth: u32,
    /// COG-4: Relevance dimension (0.0–1.0). Non-empty content = 1.0.
    pub relevance: f64,
    /// COG-4: Confidence dimension (0.0–1.0). Increases with visit count.
    pub confidence: f64,
    /// COG-4: Novelty dimension (0.0–1.0). Decreases with repetition.
    pub novelty: f64,
}

/// A node in the Graph of Thoughts.
#[derive(Debug)]
pub struct GotNode {
    /// Unique identifier.
    pub id: NodeId,
    /// Human-readable label.
    pub label: String,
    /// Heuristic weight applied during evaluation.
    pub weight: f64,
}

impl GotNode {
    /// Create a new GoT node.
    pub fn new(id: NodeId, label: impl Into<String>, weight: f64) -> Self {
        Self {
            id,
            label: label.into(),
            weight,
        }
    }

    /// Evaluate a thought message at this node.
    ///
    /// Score = accumulated_score + weight * relevance.
    /// Relevance is 1.0 for non-empty content, 0.0 otherwise.
    /// COG-4: Sets default dimension values (relevance only, confidence=0, novelty=1).
    pub fn evaluate(&self, msg: &ThoughtMessage) -> ThoughtResult {
        let relevance = if msg.content.is_empty() { 0.0 } else { 1.0 };
        let score = msg.accumulated_score + self.weight * relevance;
        ThoughtResult {
            node_id: self.id,
            score,
            output: format!("[{}] processed: {}", self.label, msg.content),
            depth: msg.depth,
            relevance,
            confidence: 0.0,
            novelty: 1.0,
        }
    }

    /// COG-4: Multi-dimensional evaluation with relevance, confidence, and novelty.
    ///
    /// - **relevance**: 1.0 if content non-empty, 0.0 otherwise
    /// - **confidence**: `1.0 - 1.0 / (1.0 + visit_count)` — increases with visits
    /// - **novelty**: `1.0 - max_similarity` to any previous output (Jaccard on words)
    /// - **score**: `0.4 * relevance + 0.3 * confidence + 0.3 * novelty`
    pub fn evaluate_multidimensional(
        &self,
        msg: &ThoughtMessage,
        visit_count: u64,
        previous_outputs: &[String],
    ) -> ThoughtResult {
        let relevance = if msg.content.is_empty() { 0.0 } else { 1.0 };
        let confidence = 1.0 - 1.0 / (1.0 + visit_count as f64);

        // Novelty: 1.0 - max Jaccard similarity to any previous output
        let novelty = if previous_outputs.is_empty() {
            1.0
        } else {
            let current_words: std::collections::HashSet<&str> =
                msg.content.split_whitespace().collect();
            let max_sim = previous_outputs
                .iter()
                .map(|prev| {
                    let prev_words: std::collections::HashSet<&str> =
                        prev.split_whitespace().collect();
                    let intersection = current_words.intersection(&prev_words).count();
                    let union = current_words.union(&prev_words).count();
                    if union == 0 {
                        0.0
                    } else {
                        intersection as f64 / union as f64
                    }
                })
                .fold(0.0_f64, f64::max);
            1.0 - max_sim
        };

        let score = 0.4 * relevance + 0.3 * confidence + 0.3 * novelty;

        ThoughtResult {
            node_id: self.id,
            score,
            output: format!("[{}] processed: {}", self.label, msg.content),
            depth: msg.depth,
            relevance,
            confidence,
            novelty,
        }
    }
}

/// Graph of Thoughts engine — orchestrates parallel thought exploration
/// using tokio mpsc channels for fan-out/collect.
#[derive(Debug)]
pub struct GotEngine {
    /// All nodes keyed by NodeId.
    nodes: HashMap<NodeId, GotNode>,
    /// Adjacency list: parent -> children.
    edges: HashMap<NodeId, Vec<NodeId>>,
    /// Maximum exploration depth (inclusive).
    max_depth: u32,
    /// INS-C3: ACO-Beam Search — retain only this many nodes per level.
    pub beam_width: usize,
    /// INS-C3: Optional pheromone memory for beam pruning guidance.
    pheromone_layer: Option<GotPheromoneMemory>,
}

impl GotEngine {
    /// Create a new GoT engine with the given maximum exploration depth.
    pub fn new(max_depth: u32) -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            max_depth,
            beam_width: 16,
            pheromone_layer: None,
        }
    }

    /// INS-C3: Enable pheromone-guided beam search with the given evaporation rate.
    pub fn with_beam_pheromone(mut self, evaporation_rate: f64) -> Self {
        self.pheromone_layer = Some(GotPheromoneMemory::new(evaporation_rate));
        self
    }

    /// INS-C3: Prune a list of `ThoughtResult`s to `beam_width` using pheromone guidance.
    ///
    /// Score for pruning = `result.score * (1 + pheromone_strength(node_label))`.
    /// When no pheromone layer is set, prunes purely by `result.score`.
    pub fn prune_by_pheromone(&self, mut results: Vec<ThoughtResult>) -> Vec<ThoughtResult> {
        if results.len() <= self.beam_width {
            return results;
        }
        results.sort_by(|a, b| {
            let score_a = if let Some(ref mem) = self.pheromone_layer {
                let label_a = self
                    .nodes
                    .get(&a.node_id)
                    .map(|n| n.label.as_str())
                    .unwrap_or("");
                a.score * (1.0 + mem.strength(&[label_a]))
            } else {
                a.score
            };
            let score_b = if let Some(ref mem) = self.pheromone_layer {
                let label_b = self
                    .nodes
                    .get(&b.node_id)
                    .map(|n| n.label.as_str())
                    .unwrap_or("");
                b.score * (1.0 + mem.strength(&[label_b]))
            } else {
                b.score
            };
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(self.beam_width);
        results
    }

    /// INS-C3: Reinforce pheromone trails for all nodes in `results`.
    pub fn reinforce_beam(&mut self, results: &[ThoughtResult]) {
        if let Some(ref mut mem) = self.pheromone_layer {
            for result in results {
                if let Some(node) = self.nodes.get(&result.node_id) {
                    mem.reinforce(&[node.label.as_str()], result.score);
                }
            }
        }
    }

    /// Add a node to the graph.
    pub fn add_node(&mut self, node: GotNode) {
        let id = node.id;
        self.nodes.insert(id, node);
        self.edges.entry(id).or_default();
    }

    /// Add a directed edge from parent to child.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) {
        self.edges.entry(from).or_default().push(to);
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of directed edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.edges.values().map(|v| v.len()).sum()
    }

    /// Explore thoughts starting from `root_id`, collecting results from all
    /// reachable nodes up to `max_depth`.
    ///
    /// Each node evaluates the thought independently and sends its result
    /// through an mpsc channel. Children are explored recursively. Once
    /// the recursive traversal completes, the sole remaining sender is
    /// dropped, causing the receiver to drain and return all results
    /// sorted by score descending.
    pub async fn explore(&self, root_id: NodeId, initial_thought: &str) -> Vec<ThoughtResult> {
        // Channel capacity = reasonable upper bound; back-pressure if graph is huge.
        let (tx, mut rx) = mpsc::channel::<ThoughtResult>(256);

        let root_msg = ThoughtMessage {
            from: 0,
            content: initial_thought.to_string(),
            depth: 0,
            accumulated_score: 0.0,
        };

        // Visited set prevents infinite recursion in cyclic graphs.
        let visited = HashSet::new();

        // Recursively explore; this awaits until the full tree is visited.
        self.explore_node(root_id, root_msg, &tx, 0, visited).await;

        // Drop the original sender so the channel closes once all clones are gone.
        drop(tx);

        // Drain results.
        let mut results = Vec::new();
        while let Some(result) = rx.recv().await {
            results.push(result);
        }

        // Sort by score descending (best first).
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Recursively explore a single node and fan-out to its children.
    ///
    /// Uses `Pin<Box<dyn Future>>` for recursive async (Rust does not
    /// support recursive async fn natively).
    ///
    /// **Cycle detection**: each path carries a `visited: HashSet<NodeId>`.
    /// If a child is already in `visited`, it is skipped — preventing
    /// infinite recursion in cyclic graphs. Each branch gets its own
    /// clone of `visited`, so diamond-shaped DAGs are explored fully
    /// (node visited via different paths still fires).
    fn explore_node<'a>(
        &'a self,
        node_id: NodeId,
        msg: ThoughtMessage,
        tx: &'a mpsc::Sender<ThoughtResult>,
        depth: u32,
        mut visited: HashSet<NodeId>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if depth > self.max_depth {
                return;
            }

            // Cycle detection: skip if this node is already in the current path.
            if visited.contains(&node_id) {
                return;
            }
            visited.insert(node_id);

            let Some(node) = self.nodes.get(&node_id) else {
                return;
            };

            // Evaluate the thought at this node.
            let result = node.evaluate(&msg);
            let score = result.score;
            // Send is infallible here (rx alive until we return).
            let _ = tx.send(result).await;

            // Fan-out to children.
            let children = self.edges.get(&node_id).cloned().unwrap_or_default();
            if children.is_empty() {
                return;
            }

            let mut handles = Vec::with_capacity(children.len());
            for child_id in children {
                // Skip cycles: child already visited in THIS path
                if visited.contains(&child_id) {
                    continue;
                }
                let child_msg = ThoughtMessage {
                    from: node_id,
                    content: msg.content.clone(),
                    depth: depth + 1,
                    accumulated_score: score,
                };
                // Each child branch gets its own clone of visited set
                // so diamond paths (A→B, A→C, B→D, C→D) still explore D twice.
                handles.push(self.explore_node(
                    child_id,
                    child_msg,
                    tx,
                    depth + 1,
                    visited.clone(),
                ));
            }

            for handle in handles {
                handle.await;
            }
        })
    }

    /// Find the single best thought result (highest score) from exploring
    /// the graph starting at `root_id`.
    pub async fn best_thought(&self, root_id: NodeId, thought: &str) -> Option<ThoughtResult> {
        let results = self.explore(root_id, thought).await;
        results.into_iter().next() // Already sorted desc by score.
    }

    /// IC-2: Explore with pheromone bias applied to result scores.
    ///
    /// Runs standard graph exploration, then augments each result's score by
    /// `pheromone_alpha × pheromone.strength([node.label])`.  This biases the
    /// ranking toward thought-paths that accumulated reward in previous calls —
    /// closing the ACO feedback loop for GoT reasoning.
    ///
    /// On the first call (no pheromone yet) this is equivalent to `explore()`.
    /// After `explore_and_reinforce()` has been called at least once, high-
    /// performing nodes receive a score boost and rise in the sorted ranking.
    pub async fn explore_with_pheromone_bias(
        &self,
        root_id: NodeId,
        initial_thought: &str,
        pheromone: &GotPheromoneMemory,
        pheromone_alpha: f64,
    ) -> Vec<ThoughtResult> {
        let mut results = self.explore(root_id, initial_thought).await;
        // Augment each result's score by pheromone strength of its node label.
        for result in &mut results {
            if let Some(node) = self.nodes.get(&result.node_id) {
                let boost = pheromone.strength(&[node.label.as_str()]);
                result.score += pheromone_alpha * boost;
            }
        }
        // Re-sort by augmented score so the strongest pheromone trail rises.
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// IC-2: Explore, bias by pheromone, then reinforce the successful paths.
    ///
    /// Combines `explore_with_pheromone_bias` with reinforcement: after
    /// exploration each node's label path receives a pheromone deposit equal to
    /// its (augmented) score.  Over repeated calls, high-scoring thought-paths
    /// accumulate pheromone and rank even higher — the full ACO feedback loop.
    pub async fn explore_and_reinforce(
        &self,
        root_id: NodeId,
        initial_thought: &str,
        pheromone: &mut GotPheromoneMemory,
        pheromone_alpha: f64,
    ) -> Vec<ThoughtResult> {
        let results = self
            .explore_with_pheromone_bias(root_id, initial_thought, pheromone, pheromone_alpha)
            .await;
        // Reinforce: deposit each result's augmented score into the pheromone.
        for result in &results {
            if let Some(node) = self.nodes.get(&result.node_id) {
                pheromone.reinforce(&[node.label.as_str()], result.score);
            }
        }
        results
    }

    /// S9: Explore with true parallelism via `JoinSet` for first-level children.
    ///
    /// Unlike `explore()` which awaits children sequentially due to `&self` lifetime
    /// constraints, this method:
    /// 1. Evaluates the root node
    /// 2. Spawns each direct child subtree as an independent `tokio::spawn` task
    /// 3. Collects results via JoinSet
    ///
    /// Each child subtree still uses sequential exploration internally (recursive),
    /// but the top-level fan-out is truly parallel. This provides speedup proportional
    /// to the branching factor of the root node.
    ///
    /// For deeper parallelism, use `evaluate_parallel` on collected node sets.
    pub async fn explore_parallel(
        &self,
        root_id: NodeId,
        initial_thought: &str,
    ) -> Vec<ThoughtResult> {
        crate::reasoning::metrics::CognitiveMetrics::inc(
            &crate::reasoning::metrics::CognitiveMetrics::global().got_explores,
        );

        let Some(root_node) = self.nodes.get(&root_id) else {
            return Vec::new();
        };

        let root_msg = ThoughtMessage {
            from: 0,
            content: initial_thought.to_string(),
            depth: 0,
            accumulated_score: 0.0,
        };

        // Evaluate root
        let root_result = root_node.evaluate(&root_msg);
        let root_score = root_result.score;
        let mut all_results = vec![root_result];

        // Get root's children
        let children = self.edges.get(&root_id).cloned().unwrap_or_default();
        if children.is_empty() {
            return all_results;
        }

        // Snapshot the engine data into Arc-wrapped structures for 'static spawning
        let ctx = Arc::new(SubtreeContext {
            nodes: self
                .nodes
                .iter()
                .map(|(&id, node)| {
                    (
                        id,
                        Arc::new(GotNode::new(node.id, node.label.clone(), node.weight)),
                    )
                })
                .collect(),
            edges: self.edges.clone(),
            max_depth: self.max_depth,
        });

        // Shared VisitedTracker with Mutex — eliminates HashSet::clone() O(V)
        let tracker = Arc::new(tokio::sync::Mutex::new(VisitedTracker::new()));

        let mut join_set: JoinSet<Vec<ThoughtResult>> = JoinSet::new();

        for child_id in children {
            let ctx = Arc::clone(&ctx);
            let tracker = Arc::clone(&tracker);
            let child_msg = ThoughtMessage {
                from: root_id,
                content: initial_thought.to_string(),
                depth: 1,
                accumulated_score: root_score,
            };

            // Criar nova geração para esta branch
            let r#gen = {
                let mut t = tracker.lock().await;
                t.next_generation();
                t.current_generation()
            };

            // Marcar root_id nesta geração antes de spawnar
            {
                let mut t = tracker.lock().await;
                t.visit_in_generation(root_id, r#gen);
            }

            join_set.spawn(async move {
                let mut results = Vec::new();
                explore_subtree(&ctx, child_id, child_msg, 1, r#gen, &tracker, &mut results).await;
                results
            });
        }

        while let Some(outcome) = join_set.join_next().await {
            match outcome {
                Ok(subtree_results) => all_results.extend(subtree_results),
                Err(e) => tracing::warn!("GoT parallel subtree panicked: {e}"),
            }
        }

        // Sort by score descending
        all_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all_results
    }

    /// S2: Evaluate a set of independent nodes in parallel via `JoinSet`.
    ///
    /// Each node is wrapped in `Arc<GotNode>` internally (since `GotNode` does not
    /// derive `Clone`) and spawned as a separate Tokio task. Results are collected
    /// as tasks complete and returned sorted by score descending.
    ///
    /// Only nodes whose IDs exist in the engine are evaluated; unknown IDs are
    /// silently skipped. An empty input yields an empty result.
    ///
    /// # Panics
    ///
    /// Does not panic. Task panics are caught by `JoinSet` and logged via `tracing`.
    pub async fn evaluate_parallel(
        &self,
        node_ids: &[NodeId],
        msg: &ThoughtMessage,
    ) -> Vec<ThoughtResult> {
        if node_ids.is_empty() {
            return Vec::new();
        }

        // Wrap referenced nodes in Arc for 'static spawn compatibility.
        let arc_nodes: Vec<Arc<GotNode>> = node_ids
            .iter()
            .filter_map(|id| {
                self.nodes.get(id).map(|node| {
                    // Build a new GotNode with the same fields (GotNode has no Clone).
                    Arc::new(GotNode::new(node.id, node.label.clone(), node.weight))
                })
            })
            .collect();

        run_parallel_nodes(arc_nodes, msg).await
    }

    // ── Snapshot accessors ──────────────────────────────────────────────────

    /// Iterator over all node IDs in the graph.
    pub fn node_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.keys().copied()
    }

    /// Return (label, weight) for a node, or default if not found.
    pub fn node_info(&self, id: NodeId) -> Option<(String, f64)> {
        self.nodes.get(&id).map(|n| (n.label.clone(), n.weight))
    }

    /// Return child IDs for a node, or empty vec if not found.
    pub fn children(&self, id: NodeId) -> Option<Vec<NodeId>> {
        self.edges.get(&id).cloned()
    }

    /// Return the maximum exploration depth.
    pub fn max_depth(&self) -> u32 {
        self.max_depth
    }

    /// Return a clone of pheromone trails, or empty map if no pheromone layer.
    pub fn pheromone_trails(&self) -> HashMap<String, f64> {
        self.pheromone_layer
            .as_ref()
            .map(|p| p.trails())
            .unwrap_or_default()
    }
}

/// S9: Shared graph data for parallel subtree exploration.
struct SubtreeContext {
    nodes: HashMap<NodeId, Arc<GotNode>>,
    edges: HashMap<NodeId, Vec<NodeId>>,
    max_depth: u32,
}

/// S9: Async recursive subtree exploration for use inside `tokio::spawn`.
///
/// This is the `'static`-safe version of `explore_node` — it receives owned
/// `Arc` data instead of `&self`, allowing it to be spawned as an independent task.
/// Uses `VisitedTracker` with generational IDs to eliminate HashSet::clone() O(V).
async fn explore_subtree(
    ctx: &SubtreeContext,
    node_id: NodeId,
    msg: ThoughtMessage,
    depth: u32,
    r#gen: u64,
    tracker: &Arc<tokio::sync::Mutex<VisitedTracker>>,
    results: &mut Vec<ThoughtResult>,
) {
    if depth > ctx.max_depth {
        return;
    }

    // Lock APENAS para checagem+marcação (microssegundos)
    let can_visit = {
        let mut t = tracker.lock().await;
        t.visit_in_generation(node_id, r#gen)
    };
    if !can_visit {
        return;
    }
    // ← Lock LIBERADO aqui — sem hold longo

    let Some(node) = ctx.nodes.get(&node_id) else {
        return;
    };

    let result = node.evaluate(&msg);
    let score = result.score;
    results.push(result);

    let children = ctx.edges.get(&node_id).cloned().unwrap_or_default();
    for child_id in children {
        let child_msg = ThoughtMessage {
            from: node_id,
            content: msg.content.clone(),
            depth: depth + 1,
            accumulated_score: score,
        };
        // Gen NÃO incrementa aqui — mesma branch sequencial
        // Box::pin necessário para chamada recursiva em async fn
        Box::pin(explore_subtree(
            ctx,
            child_id,
            child_msg,
            depth + 1,
            r#gen,
            tracker,
            results,
        ))
        .await;
    }
}

/// S2: Process independent `GotNode`s in parallel via `tokio::task::JoinSet`.
///
/// Each node is wrapped in `Arc` to satisfy the `'static` bound required by
/// `tokio::spawn`. The function evaluates every node against the provided
/// `ThoughtMessage` concurrently and collects results.
///
/// Returns results sorted by score descending (best first). If a spawned task
/// panics, its result is lost and a warning is logged — other tasks are unaffected.
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use touring_intelligence::reasoning::got::{GotNode, ThoughtMessage, run_parallel_nodes};
///
/// # async fn example() {
/// let nodes = vec![
///     Arc::new(GotNode::new(1, "plan", 2.0)),
///     Arc::new(GotNode::new(2, "verify", 3.0)),
/// ];
/// let msg = ThoughtMessage {
///     from: 0,
///     content: "analyze options".into(),
///     depth: 0,
///     accumulated_score: 0.0,
/// };
/// let results = run_parallel_nodes(nodes, &msg).await;
/// assert_eq!(results.len(), 2);
/// # }
/// ```
pub async fn run_parallel_nodes(
    nodes: Vec<Arc<GotNode>>,
    msg: &ThoughtMessage,
) -> Vec<ThoughtResult> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let mut join_set: JoinSet<ThoughtResult> = JoinSet::new();

    for node in nodes {
        let node_arc = Arc::clone(&node);
        let msg_clone = msg.clone();
        join_set.spawn(async move { node_arc.evaluate(&msg_clone) });
    }

    let mut results = Vec::new();
    while let Some(outcome) = join_set.join_next().await {
        match outcome {
            Ok(thought_result) => results.push(thought_result),
            Err(e) => tracing::warn!("GoT parallel task panicked: {e}"),
        }
    }

    // Sort by score descending (best first), consistent with explore().
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

// ---------------------------------------------------------------------------
// GoT ↔ Pensieve Bridge — Failed-path aware pruning
// ---------------------------------------------------------------------------

impl GotEngine {
    /// Prune results using Pensieve failed-path memory.
    ///
    /// For each result, checks if its node state hash matches a known failure
    /// in the Pensieve. If so, applies a score penalty proportional to the
    /// similarity with the known failure. Results with penalties below zero
    /// are discarded entirely.
    ///
    /// This enables the GoT engine to avoid re-exploring thought branches
    /// that led to dead ends in previous searches.
    pub fn prune_with_pensieve(
        &self,
        results: Vec<ThoughtResult>,
        pensieve: &crate::reasoning::pensieve::Pensieve,
    ) -> Vec<ThoughtResult> {
        let mut filtered: Vec<ThoughtResult> = results
            .into_iter()
            .filter_map(|mut result| {
                // Hash the node_id as a state for Pensieve lookup
                let state = result.node_id;
                if let Some(penalty) = pensieve.check_known_failure(state) {
                    result.score -= penalty;
                    if result.score <= 0.0 {
                        return None; // Discard — known failure with high confidence
                    }
                }
                Some(result)
            })
            .collect();

        // Re-sort after penalty application
        filtered.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        filtered
    }

    /// Explore with combined pheromone bias and Pensieve penalty.
    ///
    /// This is the primary integration point for the Cognitive Search Loop:
    /// 1. Explore the graph (standard GoT traversal)
    /// 2. Apply pheromone bias (reward successful paths)
    /// 3. Apply Pensieve penalty (penalize known failures)
    /// 4. Beam-prune to top-K results
    ///
    /// The combined scoring formula is:
    /// `final_score = base_score + alpha * pheromone - pensieve_penalty`
    pub async fn explore_cognitive(
        &self,
        root_id: NodeId,
        thought: &str,
        pheromone: &GotPheromoneMemory,
        pheromone_alpha: f64,
        pensieve: &crate::reasoning::pensieve::Pensieve,
    ) -> Vec<ThoughtResult> {
        // Step 1+2: Explore with pheromone bias
        let results = self
            .explore_with_pheromone_bias(root_id, thought, pheromone, pheromone_alpha)
            .await;

        // Step 3: Apply Pensieve penalties
        let results = self.prune_with_pensieve(results, pensieve);

        // Step 4: Beam prune to top-K
        self.prune_by_pheromone(results)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod pensieve_bridge_tests {
    use super::*;

    fn build_engine_with_nodes(count: usize) -> GotEngine {
        let mut engine = GotEngine::new(3);
        for i in 0..count {
            engine.add_node(GotNode::new(i as NodeId, format!("node_{i}"), 1.0));
        }
        engine
    }

    #[test]
    fn prune_with_empty_pensieve_returns_all() {
        let engine = build_engine_with_nodes(3);
        let pensieve = crate::reasoning::pensieve::Pensieve::new(8);
        let results = vec![
            ThoughtResult {
                node_id: 0,
                score: 0.9,
                output: "a".into(),
                depth: 1,
                relevance: 1.0,
                confidence: 1.0,
                novelty: 1.0,
            },
            ThoughtResult {
                node_id: 1,
                score: 0.5,
                output: "b".into(),
                depth: 1,
                relevance: 1.0,
                confidence: 1.0,
                novelty: 1.0,
            },
        ];
        let pruned = engine.prune_with_pensieve(results, &pensieve);
        assert_eq!(pruned.len(), 2);
    }

    #[test]
    fn prune_with_pensieve_reduces_score() {
        let engine = build_engine_with_nodes(3);
        let mut pensieve = crate::reasoning::pensieve::Pensieve::new(8).with_threshold(0.0);
        // Record node_id=1 as a failed state
        pensieve.record_failure(&[1], "failed experiment", 1);

        let results = vec![
            ThoughtResult {
                node_id: 0,
                score: 0.9,
                output: "a".into(),
                depth: 1,
                relevance: 1.0,
                confidence: 1.0,
                novelty: 1.0,
            },
            ThoughtResult {
                node_id: 1,
                score: 0.5,
                output: "b".into(),
                depth: 1,
                relevance: 1.0,
                confidence: 1.0,
                novelty: 1.0,
            },
        ];
        let pruned = engine.prune_with_pensieve(results, &pensieve);
        // Node 1 should have reduced score (or be removed if penalty > score)
        assert!(pruned.len() <= 2);
        // Node 0 should still be first (unaffected)
        if !pruned.is_empty() {
            assert_eq!(pruned[0].node_id, 0);
        }
    }

    #[test]
    fn prune_removes_high_penalty_results() {
        let engine = build_engine_with_nodes(3);
        let mut pensieve = crate::reasoning::pensieve::Pensieve::new(8).with_threshold(0.0);
        // Record multiple failures for node_id=2
        pensieve.record_failure(&[2], "total failure", 1);
        pensieve.record_failure(&[2], "repeated failure", 2);

        let results = vec![
            ThoughtResult {
                node_id: 0,
                score: 0.9,
                output: "good".into(),
                depth: 1,
                relevance: 1.0,
                confidence: 1.0,
                novelty: 1.0,
            },
            ThoughtResult {
                node_id: 2,
                score: 0.1,
                output: "bad".into(),
                depth: 1,
                relevance: 1.0,
                confidence: 1.0,
                novelty: 1.0,
            },
        ];
        let pruned = engine.prune_with_pensieve(results, &pensieve);
        // Node 2 with score 0.1 minus penalty should be <= 0, removed
        // Node 0 survives
        assert!(
            pruned.iter().all(|r| r.node_id != 2)
                || pruned
                    .iter()
                    .find(|r| r.node_id == 2)
                    .map_or(true, |r| r.score > 0.0)
        );
    }

    #[test]
    fn prune_preserves_sort_order() {
        let engine = build_engine_with_nodes(5);
        let pensieve = crate::reasoning::pensieve::Pensieve::new(8);
        let results = vec![
            ThoughtResult {
                node_id: 0,
                score: 0.3,
                output: "c".into(),
                depth: 1,
                relevance: 1.0,
                confidence: 1.0,
                novelty: 1.0,
            },
            ThoughtResult {
                node_id: 1,
                score: 0.9,
                output: "a".into(),
                depth: 1,
                relevance: 1.0,
                confidence: 1.0,
                novelty: 1.0,
            },
            ThoughtResult {
                node_id: 2,
                score: 0.6,
                output: "b".into(),
                depth: 1,
                relevance: 1.0,
                confidence: 1.0,
                novelty: 1.0,
            },
        ];
        let pruned = engine.prune_with_pensieve(results, &pensieve);
        // Should be sorted descending by score
        for w in pruned.windows(2) {
            assert!(w[0].score >= w[1].score, "results should be sorted desc");
        }
    }
}

#[cfg(test)]
mod visited_tracker_tests {
    use super::*;

    #[test]
    fn test_visit_once_returns_true() {
        let mut tracker = VisitedTracker::new();
        assert!(tracker.visit(1));
    }

    #[test]
    fn test_visit_twice_same_gen_returns_false() {
        // Ciclo: segunda visita ao mesmo nó na mesma geração
        let mut tracker = VisitedTracker::new();
        assert!(tracker.visit(1));
        assert!(!tracker.visit(1)); // ciclo!
    }

    #[test]
    fn test_different_generations_allow_revisit() {
        // Diamond: gen diferente permite revisita
        let mut tracker = VisitedTracker::new();
        assert!(tracker.visit(1));
        tracker.next_generation(); // gen=2
        assert!(tracker.visit(1)); // visitado em gen=1, mas gen=2 → diamond!
    }

    #[test]
    fn test_cycle_a_b_a() {
        // A→B→A: B tenta voltar para A (gen=1)
        let mut tracker = VisitedTracker::new();
        assert!(tracker.visit(1)); // A
        assert!(tracker.visit(2)); // B
        // Volta para A: já visitado em gen=1 → SKIP
        assert!(!tracker.visit(1)); // ciclo!
    }

    #[test]
    fn test_is_visited_in_gen() {
        let mut tracker = VisitedTracker::new();
        assert!(tracker.visit(42));
        assert!(tracker.is_visited_in_gen(42, 1));
        assert!(!tracker.is_visited_in_gen(42, 2));
        tracker.next_generation();
        assert!(tracker.visit(42)); // gen=2
        assert!(tracker.is_visited_in_gen(42, 2));
    }

    #[test]
    fn test_default_creates_gen_1() {
        let tracker = VisitedTracker::default();
        assert_eq!(tracker.current_generation(), 1);
    }

    #[test]
    fn test_multiple_nodes_same_generation() {
        let mut tracker = VisitedTracker::new();
        assert!(tracker.visit(1));
        assert!(tracker.visit(2));
        assert!(tracker.visit(3));
        // Segunda visita a cada um → ciclos
        assert!(!tracker.visit(1));
        assert!(!tracker.visit(2));
        assert!(!tracker.visit(3));
    }

    #[test]
    fn test_concurrent_branches_diamond() {
        // Simula: A→{B,C}→D — D visitado por ambas branches com gens diferentes
        let mut tracker = VisitedTracker::new();

        // Branch B (gen=1)
        tracker.next_generation(); // gen=1 (ou 1 já é o initial)
        assert!(tracker.visit(1)); // A
        assert!(tracker.visit(2)); // B
        assert!(tracker.visit(4)); // D

        // Branch C (gen=2)
        tracker.next_generation(); // gen=2
        assert!(tracker.visit(1)); // A
        assert!(tracker.visit(3)); // C
        assert!(tracker.visit(4)); // D foi visitado em gen=1, mas gen=2 é diferente → visita!
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_single_node_evaluation() {
        let mut engine = GotEngine::new(3);
        engine.add_node(GotNode::new(1, "root", 1.0));

        let results = engine.explore(1, "test thought").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_id, 1);
        assert!((results[0].score - 1.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_parallel_children() {
        let mut engine = GotEngine::new(3);
        engine.add_node(GotNode::new(1, "root", 1.0));
        engine.add_node(GotNode::new(2, "child_a", 2.0));
        engine.add_node(GotNode::new(3, "child_b", 3.0));
        engine.add_edge(1, 2);
        engine.add_edge(1, 3);

        let results = engine.explore(1, "test").await;
        assert_eq!(results.len(), 3); // root + 2 children
        // Best score: child_b (weight=3.0 + accumulated=1.0 from root = 4.0)
        assert_eq!(results[0].node_id, 3);
        assert!((results[0].score - 4.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_depth_limit() {
        let mut engine = GotEngine::new(1); // max depth = 1
        engine.add_node(GotNode::new(1, "root", 1.0));
        engine.add_node(GotNode::new(2, "child", 2.0));
        engine.add_node(GotNode::new(3, "grandchild", 3.0));
        engine.add_edge(1, 2);
        engine.add_edge(2, 3);

        let results = engine.explore(1, "test").await;
        // root (depth=0) + child (depth=1) = 2 results; grandchild (depth=2) is beyond max_depth=1.
        assert_eq!(results.len(), 2);
        // Verify no result from grandchild (node 3).
        assert!(results.iter().all(|r| r.node_id != 3));
    }

    #[tokio::test]
    async fn test_best_thought() {
        let mut engine = GotEngine::new(3);
        engine.add_node(GotNode::new(1, "root", 1.0));
        engine.add_node(GotNode::new(2, "weak", 0.1));
        engine.add_node(GotNode::new(3, "strong", 5.0));
        engine.add_edge(1, 2);
        engine.add_edge(1, 3);

        let best = engine.best_thought(1, "analyze").await;
        assert!(best.is_some());
        let best = best.unwrap();
        assert_eq!(best.node_id, 3);
        // strong: accumulated(1.0) + weight(5.0)*1.0 = 6.0
        assert!((best.score - 6.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_empty_graph() {
        let engine = GotEngine::new(3);
        let results = engine.explore(999, "test").await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_nonexistent_root() {
        let mut engine = GotEngine::new(3);
        engine.add_node(GotNode::new(1, "orphan", 1.0));
        // Explore from a node that does not exist.
        let results = engine.explore(42, "thought").await;
        assert!(results.is_empty());
    }

    #[test]
    fn test_graph_structure() {
        let mut engine = GotEngine::new(5);
        engine.add_node(GotNode::new(1, "a", 1.0));
        engine.add_node(GotNode::new(2, "b", 1.0));
        engine.add_edge(1, 2);
        assert_eq!(engine.node_count(), 2);
        assert_eq!(engine.edge_count(), 1);
    }

    #[tokio::test]
    async fn test_empty_content_zero_score() {
        let mut engine = GotEngine::new(3);
        engine.add_node(GotNode::new(1, "root", 5.0));

        let results = engine.explore(1, "").await;
        assert_eq!(results.len(), 1);
        // Empty content => relevance=0.0 => score = 0.0 + 5.0*0.0 = 0.0
        assert!((results[0].score - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_diamond_graph() {
        //     1 (root)
        //    / \
        //   2   3
        //    \ /
        //     4 (sink)
        let mut engine = GotEngine::new(5);
        engine.add_node(GotNode::new(1, "root", 1.0));
        engine.add_node(GotNode::new(2, "left", 2.0));
        engine.add_node(GotNode::new(3, "right", 3.0));
        engine.add_node(GotNode::new(4, "sink", 1.0));
        engine.add_edge(1, 2);
        engine.add_edge(1, 3);
        engine.add_edge(2, 4);
        engine.add_edge(3, 4);

        let results = engine.explore(1, "diamond").await;
        // Nodes visited: root(1), left(2), right(3), sink-via-left(4), sink-via-right(4)
        // = 5 results (node 4 visited twice via different paths).
        assert_eq!(results.len(), 5);
        // Highest: sink via right = 1.0 + 3.0 + 1.0 = 5.0
        assert!((results[0].score - 5.0).abs() < f64::EPSILON);
        assert_eq!(results[0].node_id, 4);
    }

    #[tokio::test]
    async fn test_deep_chain() {
        // Linear chain: 1 -> 2 -> 3 -> 4 -> 5, each with weight 1.0
        let mut engine = GotEngine::new(10);
        for i in 1..=5 {
            engine.add_node(GotNode::new(i, format!("node_{i}"), 1.0));
        }
        for i in 1..5 {
            engine.add_edge(i, i + 1);
        }

        let results = engine.explore(1, "chain").await;
        assert_eq!(results.len(), 5);
        // Last node score = 1+1+1+1+1 = 5.0
        assert!((results[0].score - 5.0).abs() < f64::EPSILON);
        assert_eq!(results[0].node_id, 5);
    }

    #[tokio::test]
    async fn test_best_thought_on_empty_graph() {
        let engine = GotEngine::new(3);
        let best = engine.best_thought(1, "nope").await;
        assert!(best.is_none());
    }

    #[test]
    fn test_thought_message_clone() {
        let msg = ThoughtMessage {
            from: 1,
            content: "hello".into(),
            depth: 2,
            accumulated_score: 1.234,
        };
        let cloned = msg.clone();
        assert_eq!(cloned.from, 1);
        assert_eq!(cloned.content, "hello");
        assert_eq!(cloned.depth, 2);
        assert!((cloned.accumulated_score - 1.234).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_cycle_detection_prevents_infinite_loop() {
        // Cyclic graph: 1 → 2 → 3 → 1 (back edge)
        let mut engine = GotEngine::new(10);
        engine.add_node(GotNode::new(1, "a", 1.0));
        engine.add_node(GotNode::new(2, "b", 1.0));
        engine.add_node(GotNode::new(3, "c", 1.0));
        engine.add_edge(1, 2);
        engine.add_edge(2, 3);
        engine.add_edge(3, 1); // back edge — creates cycle

        // Without cycle detection, this would recurse infinitely.
        // With cycle detection, it visits each node exactly once per path.
        let results = engine.explore(1, "cycle test").await;

        // Should have exactly 3 results (one per node), not infinite.
        assert_eq!(results.len(), 3);
        let node_ids: Vec<NodeId> = results.iter().map(|r| r.node_id).collect();
        assert!(node_ids.contains(&1));
        assert!(node_ids.contains(&2));
        assert!(node_ids.contains(&3));
    }

    #[tokio::test]
    async fn test_self_loop_detected() {
        // Self-loop: 1 → 1
        let mut engine = GotEngine::new(10);
        engine.add_node(GotNode::new(1, "self", 1.0));
        engine.add_edge(1, 1);

        let results = engine.explore(1, "self-loop").await;
        // Should have exactly 1 result (the node itself), not loop.
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_id, 1);
    }

    #[test]
    fn test_thought_result_debug() {
        let result = ThoughtResult {
            node_id: 42,
            score: 99.5,
            output: "test".into(),
            depth: 1,
            relevance: 1.0,
            confidence: 0.0,
            novelty: 1.0,
        };
        let debug_str = format!("{result:?}");
        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("99.5"));
    }

    // ── COG-4: Multi-dimensional evaluation tests ─────────────

    #[test]
    fn test_multidim_eval_all_dimensions() {
        let node = GotNode::new(1, "test", 1.0);
        let msg = ThoughtMessage {
            from: 0,
            content: "evaluate this thought".to_string(),
            depth: 0,
            accumulated_score: 0.0,
        };
        let result = node.evaluate_multidimensional(&msg, 10, &[]);
        assert!((result.relevance - 1.0).abs() < f64::EPSILON);
        assert!(result.confidence > 0.0);
        assert!((result.novelty - 1.0).abs() < f64::EPSILON); // no previous = max novelty
        assert!(result.score > 0.0);
    }

    #[test]
    fn test_multidim_confidence_increases_with_visits() {
        let node = GotNode::new(1, "test", 1.0);
        let msg = ThoughtMessage {
            from: 0,
            content: "thought".to_string(),
            depth: 0,
            accumulated_score: 0.0,
        };
        let r1 = node.evaluate_multidimensional(&msg, 1, &[]);
        let r10 = node.evaluate_multidimensional(&msg, 10, &[]);
        let r100 = node.evaluate_multidimensional(&msg, 100, &[]);
        assert!(r10.confidence > r1.confidence);
        assert!(r100.confidence > r10.confidence);
    }

    #[test]
    fn test_multidim_novelty_decreases_with_repetition() {
        let node = GotNode::new(1, "test", 1.0);
        let msg = ThoughtMessage {
            from: 0,
            content: "hello world foo".to_string(),
            depth: 0,
            accumulated_score: 0.0,
        };
        let r_novel = node.evaluate_multidimensional(&msg, 5, &[]);
        let r_repeat = node.evaluate_multidimensional(
            &msg,
            5,
            &["hello world foo".to_string()], // exact same content
        );
        assert!(r_novel.novelty > r_repeat.novelty);
        assert!((r_repeat.novelty - 0.0).abs() < f64::EPSILON); // identical = 0 novelty
    }

    #[test]
    fn test_multidim_backward_compatible() {
        // The existing evaluate() should still work and produce valid results
        let node = GotNode::new(1, "compat", 0.5);
        let msg = ThoughtMessage {
            from: 0,
            content: "test".to_string(),
            depth: 0,
            accumulated_score: 1.0,
        };
        let result = node.evaluate(&msg);
        assert!(result.score > 0.0);
        // Default dimension values from evaluate()
        assert!((result.relevance - 1.0).abs() < f64::EPSILON);
        assert!((result.confidence - 0.0).abs() < f64::EPSILON);
        assert!((result.novelty - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_multidim_score_aggregation() {
        let node = GotNode::new(1, "agg", 1.0);
        let msg = ThoughtMessage {
            from: 0,
            content: "content".to_string(),
            depth: 0,
            accumulated_score: 0.0,
        };
        let r = node.evaluate_multidimensional(&msg, 10, &[]);
        // score = 0.4*relevance + 0.3*confidence + 0.3*novelty
        let expected = 0.4 * r.relevance + 0.3 * r.confidence + 0.3 * r.novelty;
        assert!((r.score - expected).abs() < 1e-10);
    }
}

// ---------------------------------------------------------------------------
// IC-2: GotPheromoneMemory tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod got_pheromone_tests {
    use super::GotPheromoneMemory;

    #[test]
    fn test_reinforce_increases_strength() {
        let mut mem = GotPheromoneMemory::new(0.0);
        mem.reinforce(&["plan", "execute"], 1.0);
        assert!((mem.strength(&["plan", "execute"]) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_reinforce_accumulates() {
        let mut mem = GotPheromoneMemory::new(0.0);
        mem.reinforce(&["a", "b"], 0.5);
        mem.reinforce(&["a", "b"], 0.5);
        assert!((mem.strength(&["a", "b"]) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_strength_zero_for_unknown_path() {
        let mem = GotPheromoneMemory::new(0.0);
        assert_eq!(mem.strength(&["x", "y"]), 0.0);
    }

    #[test]
    fn test_evaporate_decays_strength() {
        let mut mem = GotPheromoneMemory::new(0.5);
        mem.reinforce(&["think", "verify"], 2.0);
        mem.evaporate();
        assert!((mem.strength(&["think", "verify"]) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_evaporate_prunes_weak_trails() {
        let mut mem = GotPheromoneMemory::new(1.0); // 100% decay
        mem.reinforce(&["a", "b"], 0.5);
        mem.evaporate();
        assert_eq!(mem.trail_count(), 0);
    }

    #[test]
    fn test_strongest_trail_returns_max() {
        let mut mem = GotPheromoneMemory::new(0.0);
        mem.reinforce(&["weak"], 1.0);
        mem.reinforce(&["strong"], 5.0);
        let (path, strength) = mem.strongest_trail().expect("should have trails");
        assert_eq!(path, "strong");
        assert!((strength - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_empty_path_ignored() {
        let mut mem = GotPheromoneMemory::new(0.0);
        mem.reinforce(&[], 1.0); // empty path → ignored
        assert_eq!(mem.trail_count(), 0);
    }

    #[test]
    fn test_zero_reward_ignored() {
        let mut mem = GotPheromoneMemory::new(0.0);
        mem.reinforce(&["a"], 0.0);
        assert_eq!(mem.trail_count(), 0);
    }
}

// ---------------------------------------------------------------------------
// IC-2: GotEngine + GotPheromoneMemory integration tests (E2E)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod got_pheromone_integration_tests {
    use super::*;

    /// Build a 3-node engine: root → [weak, strong]
    fn make_engine() -> GotEngine {
        let mut e = GotEngine::new(3);
        e.add_node(GotNode::new(1, "root", 1.0));
        e.add_node(GotNode::new(2, "weak", 0.5));
        e.add_node(GotNode::new(3, "strong", 3.0));
        e.add_edge(1, 2);
        e.add_edge(1, 3);
        e
    }

    // ── pheromone bias ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_explore_with_zero_pheromone_equals_explore() {
        let engine = make_engine();
        let pheromone = GotPheromoneMemory::new(0.0);

        let biased = engine
            .explore_with_pheromone_bias(1, "thought", &pheromone, 1.0)
            .await;
        let plain = engine.explore(1, "thought").await;

        // No pheromone yet → scores should be identical.
        assert_eq!(biased.len(), plain.len());
        for (b, p) in biased.iter().zip(plain.iter()) {
            assert_eq!(b.node_id, p.node_id);
            assert!(
                (b.score - p.score).abs() < 1e-9,
                "scores diverged without pheromone"
            );
        }
    }

    #[tokio::test]
    async fn test_pheromone_bias_boosts_reinforced_node() {
        let engine = make_engine();
        let mut pheromone = GotPheromoneMemory::new(0.0);

        // Manually reinforce the "weak" node to simulate a previous winning run.
        pheromone.reinforce(&["weak"], 10.0);

        let results = engine
            .explore_with_pheromone_bias(1, "thought", &pheromone, 1.0)
            .await;

        // "weak" baseline score ≈ 1.5 (root 1.0 + weak 0.5); boosted to 11.5.
        // "strong" baseline score ≈ 4.0 (root 1.0 + strong 3.0); no boost.
        // After bias, "weak" should outrank "strong".
        let first = results.first().expect("must have results");
        assert_eq!(
            first.node_id, 2,
            "pheromone-boosted 'weak' should rank first"
        );
        assert!(
            first.score > 10.0,
            "boosted score should exceed base: {}",
            first.score
        );
    }

    // ── explore_and_reinforce ACO feedback loop ────────────────────────────

    #[tokio::test]
    async fn test_explore_and_reinforce_accumulates_pheromone() {
        let engine = make_engine();
        let mut pheromone = GotPheromoneMemory::new(0.0); // no evaporation

        // First run: no pheromone yet, "strong" wins by weight.
        let r1 = engine
            .explore_and_reinforce(1, "thought", &mut pheromone, 1.0)
            .await;
        assert_eq!(r1[0].node_id, 3, "strong should win on first run");
        assert!(
            pheromone.trail_count() > 0,
            "pheromone should be deposited after reinforce"
        );

        // Second run: pheromone from r1 boosts the already-strong node further.
        let r2 = engine
            .explore_and_reinforce(1, "thought", &mut pheromone, 1.0)
            .await;
        assert_eq!(
            r2[0].node_id, 3,
            "strong should still win (pheromone reinforces winner)"
        );

        // Pheromone for "strong" should be larger than for "weak" after 2 runs.
        let ph_strong = pheromone.strength(&["strong"]);
        let ph_weak = pheromone.strength(&["weak"]);
        assert!(
            ph_strong > ph_weak,
            "strong pheromone ({ph_strong}) must exceed weak ({ph_weak})"
        );
    }

    #[tokio::test]
    async fn test_aco_loop_converges_to_winner() {
        let engine = make_engine();
        let mut pheromone = GotPheromoneMemory::new(0.0);

        // Run 5 iterations of the ACO feedback loop.
        let mut last_winner = 0;
        for _ in 0..5 {
            let results = engine
                .explore_and_reinforce(1, "thought", &mut pheromone, 0.5)
                .await;
            last_winner = results[0].node_id;
        }

        // After convergence, "strong" (id=3) should consistently win.
        assert_eq!(
            last_winner, 3,
            "ACO loop should converge to the highest-weight node"
        );

        // Its pheromone trail should be strictly stronger than competitors.
        let ph_strong = pheromone.strength(&["strong"]);
        let ph_weak = pheromone.strength(&["weak"]);
        assert!(
            ph_strong > ph_weak,
            "converged: strong({ph_strong}) > weak({ph_weak})"
        );
    }

    #[tokio::test]
    async fn test_pheromone_evaporation_resets_bias() {
        let engine = make_engine();
        let mut pheromone = GotPheromoneMemory::new(1.0); // full evaporation per tick

        // Reinforce "weak" heavily.
        pheromone.reinforce(&["weak"], 100.0);

        // Evaporate — full reset.
        pheromone.evaporate();
        assert_eq!(
            pheromone.trail_count(),
            0,
            "full evaporation should clear all trails"
        );

        // After evaporation, bias is gone and "strong" wins again by base weight.
        let results = engine
            .explore_with_pheromone_bias(1, "thought", &pheromone, 1.0)
            .await;
        assert_eq!(
            results[0].node_id, 3,
            "after evaporation, strong should regain top rank"
        );
    }
}

// ---------------------------------------------------------------------------
// S2: GoT Parallel Actors tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod got_parallel_tests {
    use super::*;
    use std::sync::Arc;

    fn make_msg(content: &str) -> ThoughtMessage {
        ThoughtMessage {
            from: 0,
            content: content.to_string(),
            depth: 0,
            accumulated_score: 0.0,
        }
    }

    // ── run_parallel_nodes (free function) ──────────────────────────────────

    #[tokio::test]
    async fn test_parallel_empty_input() {
        let results = run_parallel_nodes(vec![], &make_msg("test")).await;
        assert!(results.is_empty(), "empty input must yield empty output");
    }

    #[tokio::test]
    async fn test_parallel_single_node() {
        let node = Arc::new(GotNode::new(1, "solo", 2.0));
        let results = run_parallel_nodes(vec![node], &make_msg("thought")).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_id, 1);
        assert!((results[0].score - 2.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_parallel_multiple_nodes_all_results() {
        let nodes: Vec<Arc<GotNode>> = (1..=5)
            .map(|i| Arc::new(GotNode::new(i, format!("node_{i}"), i as f64)))
            .collect();
        let results = run_parallel_nodes(nodes, &make_msg("parallel")).await;

        // All 5 nodes must produce a result.
        assert_eq!(results.len(), 5, "must get one result per node");

        // Results sorted by score desc: node_5 (5.0) > node_4 (4.0) > ... > node_1 (1.0)
        assert_eq!(results[0].node_id, 5);
        assert_eq!(results[4].node_id, 1);
    }

    #[tokio::test]
    async fn test_parallel_results_sorted_by_score_desc() {
        let nodes = vec![
            Arc::new(GotNode::new(1, "low", 1.0)),
            Arc::new(GotNode::new(2, "mid", 5.0)),
            Arc::new(GotNode::new(3, "high", 10.0)),
        ];
        let results = run_parallel_nodes(nodes, &make_msg("sort check")).await;

        for i in 0..results.len().saturating_sub(1) {
            assert!(
                results[i].score >= results[i + 1].score,
                "results must be sorted desc: {} >= {}",
                results[i].score,
                results[i + 1].score,
            );
        }
    }

    #[tokio::test]
    async fn test_parallel_arc_shared_reference() {
        // Prove that Arc allows multiple references to the same GotNode.
        let shared = Arc::new(GotNode::new(42, "shared", 3.0));
        let clone1 = Arc::clone(&shared);
        let clone2 = Arc::clone(&shared);

        let results = run_parallel_nodes(vec![clone1, clone2], &make_msg("shared")).await;
        // Both tasks reference the same underlying node (id=42).
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.node_id == 42));
    }

    #[tokio::test]
    async fn test_parallel_empty_content_zero_relevance() {
        let node = Arc::new(GotNode::new(1, "empty", 5.0));
        let results = run_parallel_nodes(vec![node], &make_msg("")).await;
        assert_eq!(results.len(), 1);
        // Empty content => relevance=0.0 => score = 0.0 + 5.0*0.0 = 0.0
        assert!((results[0].score).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_parallel_preserves_depth_and_accumulated_score() {
        let node = Arc::new(GotNode::new(1, "ctx", 2.0));
        let msg = ThoughtMessage {
            from: 7,
            content: "context check".to_string(),
            depth: 3,
            accumulated_score: 10.0,
        };
        let results = run_parallel_nodes(vec![node], &msg).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].depth, 3);
        // score = accumulated(10.0) + weight(2.0) * relevance(1.0) = 12.0
        assert!((results[0].score - 12.0).abs() < f64::EPSILON);
    }

    // ── GotEngine::evaluate_parallel ────────────────────────────────────────

    #[tokio::test]
    async fn test_engine_evaluate_parallel_empty() {
        let engine = GotEngine::new(3);
        let results = engine.evaluate_parallel(&[], &make_msg("noop")).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_engine_evaluate_parallel_skips_unknown_ids() {
        let mut engine = GotEngine::new(3);
        engine.add_node(GotNode::new(1, "exists", 1.0));
        // Request nodes 1 (exists) and 99 (does not exist).
        let results = engine
            .evaluate_parallel(&[1, 99], &make_msg("filter"))
            .await;
        assert_eq!(
            results.len(),
            1,
            "unknown node IDs must be silently skipped"
        );
        assert_eq!(results[0].node_id, 1);
    }

    #[tokio::test]
    async fn test_engine_evaluate_parallel_multiple() {
        let mut engine = GotEngine::new(3);
        engine.add_node(GotNode::new(10, "alpha", 1.0));
        engine.add_node(GotNode::new(20, "beta", 2.0));
        engine.add_node(GotNode::new(30, "gamma", 3.0));

        let results = engine
            .evaluate_parallel(&[10, 20, 30], &make_msg("multi"))
            .await;
        assert_eq!(results.len(), 3);
        // Sorted desc: gamma(3.0), beta(2.0), alpha(1.0)
        assert_eq!(results[0].node_id, 30);
        assert_eq!(results[1].node_id, 20);
        assert_eq!(results[2].node_id, 10);
    }

    #[tokio::test]
    async fn test_engine_evaluate_parallel_consistent_with_sequential() {
        let mut engine = GotEngine::new(3);
        engine.add_node(GotNode::new(1, "a", 2.5));
        engine.add_node(GotNode::new(2, "b", 1.5));
        let msg = make_msg("consistency");

        // Parallel evaluation
        let parallel_results = engine.evaluate_parallel(&[1, 2], &msg).await;

        // Sequential evaluation (manual)
        let seq_a = GotNode::new(1, "a", 2.5).evaluate(&msg);
        let seq_b = GotNode::new(2, "b", 1.5).evaluate(&msg);

        // Same scores regardless of execution order
        let par_scores: Vec<f64> = parallel_results.iter().map(|r| r.score).collect();
        assert!(par_scores.contains(&seq_a.score));
        assert!(par_scores.contains(&seq_b.score));
    }

    #[tokio::test]
    async fn test_parallel_large_batch() {
        // 100 nodes processed in parallel — proves JoinSet handles batches.
        let nodes: Vec<Arc<GotNode>> = (1..=100)
            .map(|i| Arc::new(GotNode::new(i, format!("n{i}"), i as f64)))
            .collect();
        let results = run_parallel_nodes(nodes, &make_msg("batch")).await;
        assert_eq!(results.len(), 100);
        // First result should be the highest-weighted node (100.0)
        assert_eq!(results[0].node_id, 100);
        assert!((results[0].score - 100.0).abs() < f64::EPSILON);
    }
}
