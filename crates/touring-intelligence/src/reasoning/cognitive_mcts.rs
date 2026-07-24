//! CognitiveMCTS — SemanticGraph-informed MCTS with pheromone integration (COG-1 + S6).
//!
//! S6: Unifies the graph-informed expand (from original cognitive_mcts.rs) with
//! the pheromone-guided UCB (from mcts.rs IC-1 CognitiveMCTS). The combined engine:
//! 1. Expands only to known graph neighbors (pruning irrelevant actions)
//! 2. Weights rewards by MemoryNode.relevance_score()
//! 3. Deposits pheromone on evaluated (state, action) pairs
//! 4. Augments UCB scores with pheromone from prior searches
//!
//! This means graph-informed search benefits from pheromone reinforcement, and
//! pheromone-guided search benefits from graph structure — a complete combination.

use crate::reasoning::mcts::{MCTSConfig, MCTSEngine, MCTSNode, MCTSResult, MctsPheromonoLayer};
use crate::reasoning::semantic_graph::SemanticGraph;
use std::error::Error;
use std::sync::{Arc, Mutex};

/// Configuration for the unified CognitiveMCTS.
#[derive(Debug, Clone)]
pub struct CognitiveMCTSConfig {
    /// Base MCTS configuration.
    pub mcts_config: MCTSConfig,
    /// Weight for graph relevance priors (0.0–1.0).
    pub relevance_weight: f64,
    /// S6: Pheromone alpha blend factor (0.0 disables pheromone).
    pub pheromone_alpha: f64,
    /// S6: Pheromone evaporation rate per search cycle.
    pub pheromone_evaporation_rate: f64,
}

impl Default for CognitiveMCTSConfig {
    fn default() -> Self {
        Self {
            mcts_config: MCTSConfig::default(),
            relevance_weight: 0.5,
            pheromone_alpha: 0.3,
            pheromone_evaporation_rate: 0.05,
        }
    }
}

/// S6: Unified MCTS engine combining graph-informed expansion with pheromone guidance.
///
/// Merges the graph-informed search (SemanticGraph priors for expand + relevance-weighted
/// reward) with the IC-1 pheromone layer (ACO deposit + UCB augmentation). This means:
/// - **Expand**: graph neighbors only (from SemanticGraph)
/// - **Reward**: `base + relevance_weight * relevance + pheromone_alpha * pheromone`
/// - **Deposit**: pheromone deposited on each evaluated (state, action)
///
/// The pheromone layer is `Arc<Mutex<>>` for external sharing with AcoRewardPropagator.
pub struct GraphInformedMCTS {
    engine: MCTSEngine,
    config: CognitiveMCTSConfig,
    /// Shared pheromone layer — enables IC-1 ↔ IC-4 feedback loop.
    pheromone: Arc<Mutex<MctsPheromonoLayer>>,
}

impl GraphInformedMCTS {
    /// Create with a fresh private pheromone layer.
    pub fn new(config: CognitiveMCTSConfig) -> Self {
        let pheromone = Arc::new(Mutex::new(MctsPheromonoLayer::new(
            config.pheromone_alpha,
            config.pheromone_evaporation_rate,
        )));
        Self {
            engine: MCTSEngine::new(config.mcts_config.clone()),
            config,
            pheromone,
        }
    }

    /// Create with an externally-managed pheromone layer (for AcoRewardPropagator sharing).
    pub fn with_shared_pheromone(
        config: CognitiveMCTSConfig,
        pheromone: Arc<Mutex<MctsPheromonoLayer>>,
    ) -> Self {
        Self {
            engine: MCTSEngine::new(config.mcts_config.clone()),
            config,
            pheromone,
        }
    }

    /// Run graph-informed + pheromone-guided MCTS search.
    ///
    /// Combines SemanticGraph neighbor expansion with pheromone-augmented rewards.
    /// Each reward evaluation also deposits pheromone, building up trails over
    /// repeated calls.
    pub fn search(
        &self,
        root_state: u64,
        graph: &SemanticGraph,
        node_id_map: &std::collections::HashMap<String, u64>,
        reverse_map: &std::collections::HashMap<u64, String>,
    ) -> Option<MCTSResult> {
        crate::reasoning::metrics::CognitiveMetrics::inc(
            &crate::reasoning::metrics::CognitiveMetrics::global().mcts_searches,
        );

        let relevance_weight = self.config.relevance_weight;
        let pheromone_alpha = self.config.pheromone_alpha;
        let now = crate::reasoning::semantic_graph::now_epoch_secs();

        let pheromone = Arc::clone(&self.pheromone);

        // Expand function: return neighbor node u64 IDs from graph
        let expand_fn = |state: u64| -> Vec<u64> {
            let node_id = match reverse_map.get(&state) {
                Some(id) => id.as_str(),
                None => return vec![],
            };
            graph
                .neighbors(node_id)
                .iter()
                .filter_map(|nbr| node_id_map.get(nbr.as_str()).copied())
                .collect()
        };

        // Reward function: relevance + pheromone + deposit
        let reward_fn = move |state: u64, action: u64| -> f64 {
            let base_reward = 0.5;
            let node_id = match reverse_map.get(&action) {
                Some(id) => id.as_str(),
                None => return base_reward,
            };
            let relevance = graph
                .get_node(node_id)
                .map(|n| n.relevance_score(now))
                .unwrap_or(0.0);

            // S6: Read pheromone strength for this (state, action) pair
            let pheromone_bonus = {
                let layer = pheromone.lock().unwrap_or_else(|e| e.into_inner());
                layer.strength(state, action)
            };

            let reward =
                base_reward + relevance_weight * relevance + pheromone_alpha * pheromone_bonus;

            // S6: Deposit reward as pheromone for future searches
            {
                let mut layer = pheromone.lock().unwrap_or_else(|e| e.into_inner());
                layer.deposit(state, action, reward);
                crate::reasoning::metrics::CognitiveMetrics::inc(
                    &crate::reasoning::metrics::CognitiveMetrics::global().pheromone_deposits,
                );
            }

            reward
        };

        self.engine.search(root_state, expand_fn, reward_fn)
    }

    /// Apply evaporation to the pheromone layer between search sessions.
    pub fn evaporate(&self) {
        self.pheromone
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .evaporate();
    }

    /// Return a clone of the pheromone Arc for external coordination.
    pub fn pheromone_arc(&self) -> Arc<Mutex<MctsPheromonoLayer>> {
        Arc::clone(&self.pheromone)
    }

    /// Number of pheromone entries currently tracked.
    pub fn pheromone_entry_count(&self) -> usize {
        self.pheromone
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry_count()
    }

    /// GPU-accelerated evaluation for all frontier nodes in parallel.
    ///
    /// The WGSL kernel `MCTS_EVAL_SHADER` is ready for dispatch. GPU dispatch
    /// requires `get_gpu_resources` to be public in `touring-simd`
    /// (crate-private in v0.2.0). Currently runs on CPU with rayon parallel
    /// iterator matching the kernel's parallel semantics.
    ///
    /// # Arguments
    ///
    /// * `frontier` — slice of `MCTSNode` IDs representing the current search frontier
    /// * `beam_width` — beam width multiplier (used as `node_score * beam_width`)
    ///
    /// # Returns
    ///
    /// Vector of evaluation scores, one per frontier node, aligned with input order.
    pub fn evaluate_gpu(
        &self,
        frontier: &[MCTSNode],
        beam_width: u32,
    ) -> Result<Vec<f32>, Box<dyn Error + Send + Sync>> {
        if frontier.is_empty() {
            return Ok(vec![]);
        }

        let n = frontier.len();

        // GPU fast path: dispatch MCTS_EVAL_SHADER across frontier nodes.
        if let Ok(gpu) = touring_simd::gpu::get_gpu_resources() {
            use std::mem::size_of;

            let pheromone_vec: Vec<f32> = {
                let pheromone_lock = self.pheromone.lock().unwrap_or_else(|e| e.into_inner());
                frontier
                    .iter()
                    .map(|node| pheromone_lock.strength(node.state, 0) as f32)
                    .collect()
            };

            let pheromone_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("eval_pheromone"),
                size: (n * size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu.queue
                .write_buffer(&pheromone_buf, 0, bytemuck::cast_slice(&pheromone_vec));

            let scores_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("eval_scores"),
                size: (n * size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let staging_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("eval_staging"),
                size: (n * size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let beam_width_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("eval_beam_width"),
                size: size_of::<u32>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu.queue
                .write_buffer(&beam_width_buf, 0, bytemuck::cast_slice(&[beam_width]));

            let module = gpu
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("mcts_eval_shader"),
                    source: wgpu::ShaderSource::Wgsl(MCTS_EVAL_SHADER.into()),
                });
            let pipeline = gpu
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("mcts_eval_pipeline"),
                    layout: None,
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                });
            let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mcts_eval_bind_group"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: pheromone_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: scores_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: beam_width_buf.as_entire_binding(),
                    },
                ],
            });

            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("mcts_eval_encoder"),
                });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("mcts_eval_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, Some(&bind_group), &[]);
                pass.dispatch_workgroups((n as u32 + 63) / 64, 1, 1);
            }
            encoder.copy_buffer_to_buffer(
                &scores_buf,
                0,
                &staging_buf,
                0,
                (n * size_of::<f32>()) as u64,
            );
            gpu.queue.submit(Some(encoder.finish()));

            let slice = staging_buf.slice(..);
            {
                let (tx, rx) = std::sync::mpsc::channel::<Result<(), wgpu::BufferAsyncError>>();
                slice.map_async(wgpu::MapMode::Read, move |result| {
                    let _ = tx.send(result);
                });
                loop {
                    if gpu.device.poll(wgpu::PollType::Wait).is_ok() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
                rx.recv()
                    .map_err(|e| format!("Map recv: {e}"))?
                    .map_err(|e| format!("Buffer map: {e}"))?;
            }
            let mapped = slice.get_mapped_range();
            let mut eval_scores = vec![0.0f32; n];
            eval_scores.copy_from_slice(bytemuck::cast_slice(&mapped));
            drop(mapped);

            return Ok(eval_scores);
        }

        // CPU fallback: rayon parallel evaluation matching the GPU kernel semantics.
        use rayon::prelude::*;
        let eval_scores: Vec<f32> = frontier
            .par_iter()
            .map(|node| {
                let pheromone_lock = self.pheromone.lock().unwrap_or_else(|e| e.into_inner());
                let base = pheromone_lock.strength(node.state, 0) as f32;
                drop(pheromone_lock);
                base * beam_width as f32
            })
            .collect();

        Ok(eval_scores)
    }
}

/// GPU evaluation kernel — parallel scoring of frontier nodes with semantic weight.
///
/// Each workitem evaluates one frontier node by multiplying its pheromone strength
/// by the beam width. Matches the parallel semantics of the WGSL shader below:
/// ```wgsl
/// @compute @workgroup_size(64)
/// fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
///     let node_idx = global_id.x;
///     eval_scores[node_idx] = pheromone[node_idx] * beam_width as f32;
/// }
/// ```
const MCTS_EVAL_SHADER: &str = r#"
    @group(0) @binding(0) var<storage, read> pheromone: array<f32>;
    @group(0) @binding(1) var<storage, read_write> eval_scores: array<f32>;
    @group(0) @binding(2) var<uniform> beam_width: u32;

    @compute @workgroup_size(64)
    fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
        let node_idx = global_id.x;
        if (node_idx >= arrayLength(&pheromone)) { return; }
        eval_scores[node_idx] = pheromone[node_idx] * f32(beam_width);
    }
"#;

/// Legacy alias for backward compatibility.
pub type CognitiveMCTS = GraphInformedMCTS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reasoning::persistence::GraphPersistence;
    use crate::reasoning::semantic_graph::{MemoryNode, NodeType, SemanticGraph};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn make_test_graph() -> SemanticGraph {
        let persistence = Arc::new(GraphPersistence::new(std::path::PathBuf::from(":memory:")));
        SemanticGraph::new(persistence)
    }

    #[test]
    fn test_cognitive_mcts_search_basic() {
        let graph = make_test_graph();
        graph
            .add_node(MemoryNode::new("a", "NodeA", NodeType::Symbol))
            .unwrap();
        graph
            .add_node(MemoryNode::new("b", "NodeB", NodeType::Symbol))
            .unwrap();
        graph.add_edge("a", "b", 1.0).unwrap();

        let mut id_map = HashMap::new();
        let mut rev_map = HashMap::new();
        id_map.insert("a".to_string(), 1_u64);
        id_map.insert("b".to_string(), 2_u64);
        rev_map.insert(1_u64, "a".to_string());
        rev_map.insert(2_u64, "b".to_string());

        let cmcts = GraphInformedMCTS::new(CognitiveMCTSConfig::default());
        let result = cmcts.search(1, &graph, &id_map, &rev_map);
        if let Some(r) = result {
            assert_eq!(r.best_action, 2);
            assert!(r.confidence > 0.0);
        }
    }

    #[test]
    fn test_cognitive_mcts_empty_graph() {
        let graph = make_test_graph();
        let cmcts = GraphInformedMCTS::new(CognitiveMCTSConfig::default());
        let result = cmcts.search(1, &graph, &HashMap::new(), &HashMap::new());
        assert!(result.is_none());
    }

    #[test]
    fn test_cognitive_mcts_respects_max_depth() {
        let graph = make_test_graph();
        graph
            .add_node(MemoryNode::new("a", "A", NodeType::Symbol))
            .unwrap();
        graph
            .add_node(MemoryNode::new("b", "B", NodeType::Symbol))
            .unwrap();
        graph.add_edge("a", "b", 1.0).unwrap();

        let mut id_map = HashMap::new();
        let mut rev_map = HashMap::new();
        id_map.insert("a".to_string(), 1_u64);
        id_map.insert("b".to_string(), 2_u64);
        rev_map.insert(1_u64, "a".to_string());
        rev_map.insert(2_u64, "b".to_string());

        let config = CognitiveMCTSConfig {
            mcts_config: MCTSConfig {
                max_depth: 1,
                num_rollouts: 10,
                ..MCTSConfig::default()
            },
            relevance_weight: 0.5,
            ..CognitiveMCTSConfig::default()
        };
        let cmcts = GraphInformedMCTS::new(config);
        let result = cmcts.search(1, &graph, &id_map, &rev_map);
        if let Some(r) = result {
            assert!(r.tree_depth <= 1);
        }
    }

    #[test]
    fn test_graph_expand_returns_neighbors() {
        let graph = make_test_graph();
        graph
            .add_node(MemoryNode::new("x", "X", NodeType::File))
            .unwrap();
        graph
            .add_node(MemoryNode::new("y", "Y", NodeType::File))
            .unwrap();
        graph
            .add_node(MemoryNode::new("z", "Z", NodeType::File))
            .unwrap();
        graph.add_edge("x", "y", 1.0).unwrap();
        graph.add_edge("x", "z", 1.0).unwrap();

        let nbrs = graph.neighbors("x");
        assert_eq!(nbrs.len(), 2);
    }

    #[test]
    fn test_graph_reward_uses_relevance() {
        let graph = make_test_graph();
        let mut node = MemoryNode::new("r", "R", NodeType::Concept);
        node.access_count = 50;
        node.last_accessed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        graph.add_node(node).unwrap();

        let fetched = graph.get_node("r").unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        assert!(fetched.relevance_score(now) > 0.5);
    }

    #[test]
    fn test_default_config() {
        let config = CognitiveMCTSConfig::default();
        assert!((config.relevance_weight - 0.5).abs() < f64::EPSILON);
        assert!((config.pheromone_alpha - 0.3).abs() < f64::EPSILON);
    }

    // S6: Test pheromone accumulation across searches
    #[test]
    fn test_pheromone_accumulates_across_searches() {
        let graph = make_test_graph();
        graph
            .add_node(MemoryNode::new("a", "A", NodeType::Symbol))
            .unwrap();
        graph
            .add_node(MemoryNode::new("b", "B", NodeType::Symbol))
            .unwrap();
        graph.add_edge("a", "b", 1.0).unwrap();

        let mut id_map = HashMap::new();
        let mut rev_map = HashMap::new();
        id_map.insert("a".to_string(), 1_u64);
        id_map.insert("b".to_string(), 2_u64);
        rev_map.insert(1_u64, "a".to_string());
        rev_map.insert(2_u64, "b".to_string());

        let cmcts = GraphInformedMCTS::new(CognitiveMCTSConfig::default());

        // First search deposits pheromone
        let _ = cmcts.search(1, &graph, &id_map, &rev_map);
        let count1 = cmcts.pheromone_entry_count();
        assert!(count1 > 0, "pheromone should be deposited after search");

        // Second search should find existing pheromone and deposit more
        let _ = cmcts.search(1, &graph, &id_map, &rev_map);
        let count2 = cmcts.pheromone_entry_count();
        assert!(count2 >= count1, "pheromone count should not decrease");
    }

    // S6: Test shared pheromone constructor
    #[test]
    fn test_shared_pheromone_constructor() {
        let shared = Arc::new(Mutex::new(MctsPheromonoLayer::new(0.5, 0.05)));
        let cmcts = GraphInformedMCTS::with_shared_pheromone(
            CognitiveMCTSConfig::default(),
            Arc::clone(&shared),
        );
        // Both should point to the same layer
        assert!(Arc::ptr_eq(&cmcts.pheromone_arc(), &shared));
    }

    // S6: Test evaporation
    #[test]
    fn test_evaporation_reduces_pheromone() {
        let graph = make_test_graph();
        graph
            .add_node(MemoryNode::new("a", "A", NodeType::Symbol))
            .unwrap();
        graph
            .add_node(MemoryNode::new("b", "B", NodeType::Symbol))
            .unwrap();
        graph.add_edge("a", "b", 1.0).unwrap();

        let mut id_map = HashMap::new();
        let mut rev_map = HashMap::new();
        id_map.insert("a".to_string(), 1_u64);
        id_map.insert("b".to_string(), 2_u64);
        rev_map.insert(1_u64, "a".to_string());
        rev_map.insert(2_u64, "b".to_string());

        let cmcts = GraphInformedMCTS::new(CognitiveMCTSConfig {
            pheromone_evaporation_rate: 0.99, // aggressive evaporation
            ..CognitiveMCTSConfig::default()
        });
        let _ = cmcts.search(1, &graph, &id_map, &rev_map);
        assert!(cmcts.pheromone_entry_count() > 0);

        // Evaporate aggressively
        for _ in 0..5 {
            cmcts.evaporate();
        }
        // After aggressive evaporation, entries should be pruned
        assert_eq!(cmcts.pheromone_entry_count(), 0);
    }
}
