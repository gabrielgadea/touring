// MCTS Rollout Shader — WGSL compute for GPU-accelerated Monte Carlo Tree Search
//
// Each workitem evaluates one frontier node by computing:
//   score = sum over depth of pheromone[node_state] * d
// for d in [0, depth).
//
// Buffer layout:
//   @group(0) @binding(0) frontier_nodes: array<u32>   — node state IDs
//   @group(0) @binding(1) pheromone: array<f32>           — pheromone strength per state
//   @group(0) @binding(2) rollout_scores: array<f32>     — output scores (read_write)
//   @group(0) @binding(3) depth: u32                     — uniform rollout depth
//
// Workgroup size: 64 (hardware-aligned for NVIDIA RTX 4060)
// Invocation: dispatch_workgroups((frontier_len + 63) / 64, 1, 1)

@group(0) @binding(0) var<storage, read> frontier_nodes: array<u32>;
@group(0) @binding(1) var<storage, read> pheromone: array<f32>;
@group(0) @binding(2) var<storage, read_write> rollout_scores: array<f32>;
@group(0) @binding(3) var<uniform> depth: u32;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let node_idx = global_id.x;

    // Bound check — guard against out-of-bounds dispatch
    if (node_idx >= arrayLength(&frontier_nodes)) {
        return;
    }

    // Read this node's state ID from the frontier array
    let node_state = frontier_nodes[node_idx];

    // Pheromone lookup — out-of-bounds index defaults to 0.0 (no trail)
    var pheromone_strength: f32;
    if (node_state < arrayLength(&pheromone)) {
        pheromone_strength = pheromone[node_state];
    } else {
        pheromone_strength = 0.0;
    }

    // Accumulate score: sum_{d=0}^{depth-1} pheromone_strength * d
    var score: f32 = 0.0;
    for (var d: u32 = 0u; d < depth; d++) {
        score = score + pheromone_strength * f32(d);
    }

    rollout_scores[node_idx] = score;
}
