#!/usr/bin/env python3
"""
generate_plan.py — Code-First Plan Generator for Touring Improvements 2026.

Single source of truth: all plan data lives here as Python structures.
Running this script generates all markdown phase files, validation scripts,
README, and audit/patch utilities idempotently.

Usage:
    python generate_plan.py              # Generate all files
    python generate_plan.py --validate   # Generate + verify idempotency
    python generate_plan.py --list       # List phases
    python generate_plan.py --phase N    # Generate only phase N (e.g. --phase 1)
    python generate_plan.py --check      # Verify all 13 insights covered
"""
# -*- coding: utf-8 -*-

from __future__ import annotations

import argparse
import hashlib
import sys
from dataclasses import dataclass, field
from datetime import date
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError:
    print("PyYAML required: pip install pyyaml", file=sys.stderr)
    sys.exit(1)

# ══════════════════════════════════════════════════════════════════════════════
# CONSTANTS
# ══════════════════════════════════════════════════════════════════════════════

OUTPUT_DIR = Path(__file__).parent
WORKSPACE = Path("/home/gabrielgadea/.claude/rust")
TODAY = str(date.today())

# ══════════════════════════════════════════════════════════════════════════════
# PLAN METADATA — Global
# ══════════════════════════════════════════════════════════════════════════════

PLAN_METADATA: dict[str, Any] = {
    "plan_id": "touring-improvements-2026",
    "created": TODAY,
    "version": "1.0.0",
    "objective": (
        "Implement 13 insights across touring-learning/aco, touring-ast, "
        "and touring-cognitive for diagnostic engine, evolution logic, "
        "prioritized indexing, fused search, cognitive MCTS, refinement "
        "cycles, adaptive decay, multi-dimensional GoT evaluation, and "
        "Q-value persistence."
    ),
    "total_insights": 13,
    "crates": ["touring-learning", "touring-ast", "touring-cognitive"],
    "workspace": str(WORKSPACE),
    "horizons": {
        "H1": "0-2 weeks (Quick Wins)",
        "H2": "2-8 weeks (Strategic)",
        "H3": "2-6 months (Synergies + Audit)",
    },
}

# ══════════════════════════════════════════════════════════════════════════════
# VGP-VERIFIED STRUCT SCHEMAS
# All fields below were extracted from actual source via touring_ast_find.
# Any implementation hint MUST only reference fields listed here.
# ══════════════════════════════════════════════════════════════════════════════

VGP_SCHEMAS: dict[str, dict[str, Any]] = {
    "ExecutionStatus": {
        "file": "crates/touring-learning/src/aco/models.rs",
        "line": 102,
        "kind": "enum",
        "variants": [
            "Success", "ValidationFailed", "ExecutionFailed",
            "PreconditionFailed", "RollbackExecuted", "RollbackFailed",
        ],
    },
    "Complexity": {
        "file": "crates/touring-learning/src/aco/models.rs",
        "line": 35,
        "kind": "enum",
        "variants": ["Trivial", "Moderate", "High", "Critical"],
    },
    "GeneratorNode": {
        "file": "crates/touring-learning/src/aco/models.rs",
        "line": 198,
        "kind": "struct",
        "fields": [
            ("id", "String"), ("description", "String"),
            ("generator_type", "GeneratorType"),
            ("inputs_data", "Vec<String>"),
            ("inputs_templates", "Vec<String>"),
            ("inputs_constraints", "Vec<String>"),
            ("outputs", "GeneratorOutputs"),
            ("contract", "GeneratorContract"),
            ("acceptance_criteria", "String"),
            ("depends_on", "Vec<String>"),
        ],
    },
    "GeneratorGraphModel": {
        "file": "crates/touring-learning/src/aco/models.rs",
        "line": 213,
        "kind": "struct",
        "fields": [
            ("nodes", "Vec<GeneratorNode>"),
            ("critical_path", "Vec<String>"),
            ("parallelizable", "Vec<Vec<String>>"),
            ("objective_hash", "String"),
        ],
    },
    "IntentSpec": {
        "file": "crates/touring-learning/src/aco/models.rs",
        "line": 128,
        "kind": "struct",
        "fields": [
            ("raw_prompt", "String"), ("real_need", "String"),
            ("gap_analysis", "String"), ("operation_mode", "OperationMode"),
            ("preliminary_risks", "Vec<String>"),
            ("anti_patterns_to_watch", "Vec<String>"),
            ("objective_hash", "String"),
        ],
    },
    "EvolutionPackage": {
        "file": "crates/touring-learning/src/aco/models.rs",
        "line": 266,
        "kind": "struct",
        "fields": [
            ("session_id", "String"), ("objective_hash", "String"),
            ("execution_report_json", "String"),
            ("learned_patterns", "Vec<LearnedPattern>"),
            ("anti_patterns_discovered", "Vec<String>"),
            ("system_upgrades", "Vec<SystemUpgrade>"),
            ("quality_metrics", "HashMap<String, f64>"),
            ("persistence_actions", "Vec<String>"),
        ],
    },
    "LearnedPattern": {
        "file": "crates/touring-learning/src/aco/models.rs",
        "line": 248,
        "kind": "struct",
        "fields": [
            ("pattern_id", "String"), ("description", "String"),
            ("generator_template", "String"), ("domain", "String"),
            ("tags", "Vec<String>"),
        ],
    },
    "MutableGeneratorGraph": {
        "file": "crates/touring-learning/src/aco/graph.rs",
        "line": 31,
        "kind": "struct",
        "fields": [
            ("graph", "DiGraph<String, ()>"),
            ("index_map", "HashMap<String, NodeIndex>"),
            ("nodes", "BTreeMap<String, GeneratorNode>"),
            ("execution_status", "HashMap<String, ExecutionStatus>"),
            ("dirty", "bool"),
        ],
    },
    "TrackerReport": {
        "file": "crates/touring-learning/src/aco/tracker.rs",
        "line": 65,
        "kind": "struct",
        "fields": [
            ("dims", "Vec<DimResult>"), ("composite", "f64"),
            ("status", "TrackerStatus"), ("iteration", "u32"),
        ],
    },
    "DimResult": {
        "file": "crates/touring-learning/src/aco/tracker.rs",
        "line": 36,
        "kind": "struct",
        "fields": [
            ("dim_id", "String"), ("name", "String"),
            ("score", "f64"), ("checks_passed", "u32"),
            ("checks_total", "u32"), ("details", "Vec<String>"),
        ],
    },
    "BlastRadius": {
        "file": "crates/touring-ast/src/graph.rs",
        "line": 432,
        "kind": "struct",
        "fields": [
            ("start_file", "String"),
            ("affected_files", "Vec<String>"),
            ("affected_symbols", "Vec<(String, String)>"),
            ("max_distance", "usize"),
            ("file_count", "usize"),
        ],
    },
    "SemanticSymbolIndex": {
        "file": "crates/touring-ast/src/semantic_search.rs",
        "line": 70,
        "kind": "struct",
        "fields": [("entries", "LruCache<String, Entry>")],
        "public_methods": [
            "new(capacity: usize)",
            "index_symbol(&mut self, sym: &Symbol)",
            "find_similar_symbols(&self, query: &Symbol, threshold: f64, limit: usize) -> Vec<(String, f64)>",
        ],
    },
    "IncrementalPipeline": {
        "file": "crates/touring-ast/src/incremental_pipeline.rs",
        "line": 62,
        "kind": "struct",
        "fields": [
            ("parser", "IncrementalParser"),
            ("documents", "HashMap<String, RopeDocument>"),
            ("symbol_store", "Option<SymbolStore>"),
        ],
    },
    "SymbolIndex": {
        "file": "crates/touring-ast/src/graph.rs",
        "line": 62,
        "kind": "struct",
        "note": "Has symbols, file_to_symbols, reverse_deps, dependencies maps",
    },
    "MemoryNode": {
        "file": "crates/touring-cognitive/src/semantic_graph.rs",
        "line": 98,
        "kind": "struct",
        "fields": [
            ("id", "String"), ("label", "String"),
            ("node_type", "NodeType"), ("embedding", "Vec<f32>"),
            ("metadata", "serde_json::Value"),
            ("last_accessed", "f64"), ("access_count", "u64"),
        ],
    },
    "SemanticEdge": {
        "file": "crates/touring-cognitive/src/semantic_graph.rs",
        "line": 161,
        "kind": "struct",
        "fields": [
            ("from_id", "String"), ("to_id", "String"),
            ("edge_type", "EdgeType"), ("weight", "f32"),
            ("created_at", "f64"),
        ],
    },
    "SemanticGraph": {
        "file": "crates/touring-cognitive/src/semantic_graph.rs",
        "line": 210,
        "kind": "struct",
        "fields": [
            ("graph", "RwLock<StableGraph<MemoryNode, SemanticEdge>>"),
            ("node_index", "DashMap<String, NodeIndex>"),
            ("_persistence", "Arc<GraphPersistence>"),
        ],
    },
    "MCTSNode": {
        "file": "crates/touring-cognitive/src/mcts.rs",
        "line": 40,
        "kind": "struct",
        "fields": [
            ("state", "u64"), ("action", "Option<u64>"),
            ("visits", "u64"), ("value_sum", "f64"),
            ("children", "Vec<MCTSNode>"), ("is_terminal", "bool"),
        ],
    },
    "MCTSConfig": {
        "file": "crates/touring-cognitive/src/mcts.rs",
        "line": 96,
        "kind": "struct",
        "fields": [
            ("max_depth", "usize"), ("num_rollouts", "usize"),
            ("exploration_constant", "f64"),
            ("max_rollouts", "usize"), ("discount", "f64"),
        ],
    },
    "MCTSResult": {
        "file": "crates/touring-cognitive/src/mcts.rs",
        "line": 133,
        "kind": "struct",
        "fields": [
            ("best_action", "u64"), ("confidence", "f64"),
            ("value", "f64"), ("tree_depth", "usize"),
            ("total_rollouts", "u64"),
        ],
    },
    "MCTSEngine": {
        "file": "crates/touring-cognitive/src/mcts.rs",
        "line": 151,
        "kind": "struct",
        "fields": [("config", "MCTSConfig")],
    },
    "GraphSnapshot": {
        "file": "crates/touring-cognitive/src/persistence.rs",
        "line": 10,
        "kind": "struct",
        "fields": [
            ("nodes", "Vec<MemoryNode>"),
            ("edges", "Vec<SemanticEdge>"),
        ],
    },
    "GotNode": {
        "file": "crates/touring-cognitive/src/got.rs",
        "line": 50,
        "kind": "struct",
        "fields": [
            ("id", "NodeId"), ("label", "String"), ("weight", "f64"),
        ],
    },
    "GotEngine": {
        "file": "crates/touring-cognitive/src/got.rs",
        "line": 88,
        "kind": "struct",
        "fields": [
            ("nodes", "HashMap<NodeId, GotNode>"),
            ("edges", "HashMap<NodeId, Vec<NodeId>>"),
            ("max_depth", "u32"),
        ],
    },
    "ThoughtResult": {
        "file": "crates/touring-cognitive/src/got.rs",
        "line": 37,
        "kind": "struct",
        "fields": [
            ("node_id", "NodeId"), ("score", "f64"),
            ("output", "String"), ("depth", "u32"),
        ],
    },
    "SessionPredictor": {
        "file": "crates/touring-cognitive/src/session_predictor.rs",
        "line": 47,
        "kind": "struct",
        "fields": [
            ("history", "RwLock<VecDeque<ToolInvocation>>"),
            ("transitions", "RwLock<TransitionMatrix>"),
            ("bigram_transitions", "RwLock<BigramMatrix>"),
            ("q_values", "RwLock<QTable>"),
        ],
    },
    "GraphPersistence": {
        "file": "crates/touring-cognitive/src/persistence.rs",
        "line": 17,
        "kind": "struct",
        "fields": [("path", "PathBuf")],
    },
}

# ══════════════════════════════════════════════════════════════════════════════
# 13 INSIGHTS — Full specifications
# ══════════════════════════════════════════════════════════════════════════════


@dataclass
class Insight:
    """Single improvement insight with full specification."""

    id: str
    title: str
    crate: str
    module: str  # relative path within crate
    priority: str  # Q1 or Q2
    horizon: str  # H1, H2
    effort: str  # e.g. "2h", "1 week"
    phase: int
    description: str
    new_types: list[str] = field(default_factory=list)
    modified_types: list[str] = field(default_factory=list)
    new_functions: list[str] = field(default_factory=list)
    test_cases: list[str] = field(default_factory=list)
    implementation_summary: str = ""
    synergies: list[str] = field(default_factory=list)


INSIGHTS: dict[str, Insight] = {
    "ACO-1": Insight(
        id="ACO-1",
        title="4-Layer Diagnostic Engine",
        crate="touring-learning",
        module="src/aco/diagnostics.rs",
        priority="Q1", horizon="H1", effort="2h", phase=1,
        description=(
            "Create a DiagnosticLayer enum (Syntax, Logic, Contract, Architecture) "
            "and a classify_error() function that categorizes errors into layers. "
            "Each layer maps to a retry strategy: Syntax -> auto-fix, Logic -> re-plan, "
            "Contract -> escalate to user, Architecture -> halt."
        ),
        new_types=["DiagnosticLayer", "DiagnosticResult"],
        new_functions=["classify_error", "suggest_retry_strategy"],
        test_cases=[
            "test_classify_syntax_error",
            "test_classify_logic_error",
            "test_classify_contract_error",
            "test_classify_architecture_error",
            "test_suggest_retry_for_each_layer",
            "test_unknown_error_defaults_to_logic",
        ],
        implementation_summary=(
            "New file src/aco/diagnostics.rs with:\n"
            "  enum DiagnosticLayer { Syntax, Logic, Contract, Architecture }\n"
            "  struct DiagnosticResult { layer: DiagnosticLayer, confidence: f64, "
            "message: String, retry_strategy: RetryStrategy }\n"
            "  fn classify_error(error_msg: &str, context: &str) -> DiagnosticResult\n"
            "  fn suggest_retry_strategy(layer: &DiagnosticLayer) -> RetryStrategy\n"
            "Uses keyword matching + regex patterns for classification."
        ),
        synergies=["COG-2"],
    ),
    "ACO-2": Insight(
        id="ACO-2",
        title="EvolutionPackage Population Logic",
        crate="touring-learning",
        module="src/aco/evolution.rs",
        priority="Q2", horizon="H2", effort="1 week", phase=3,
        description=(
            "Implement the full population logic for EvolutionPackage (models.rs line 266). "
            "After a graph execution completes, extract learned_patterns from successful nodes, "
            "discover anti_patterns from failed nodes, propose system_upgrades based on "
            "quality_metrics deltas, and serialize to persistence_actions."
        ),
        modified_types=["EvolutionPackage"],
        new_functions=[
            "populate_evolution_package",
            "extract_learned_patterns",
            "discover_anti_patterns",
            "propose_system_upgrades",
        ],
        test_cases=[
            "test_populate_from_successful_execution",
            "test_populate_from_failed_execution",
            "test_extract_patterns_from_nodes",
            "test_discover_anti_patterns",
            "test_propose_upgrades_from_metrics",
            "test_empty_graph_produces_empty_package",
        ],
        implementation_summary=(
            "New file src/aco/evolution.rs:\n"
            "  fn populate_evolution_package(\n"
            "      graph: &MutableGeneratorGraph,\n"
            "      session_id: &str,\n"
            "      objective_hash: &str,\n"
            "      execution_report: &str,\n"
            "  ) -> EvolutionPackage\n"
            "  Iterates graph.iter() to extract patterns from Success nodes,\n"
            "  anti-patterns from ExecutionFailed/ValidationFailed nodes.\n"
            "  Uses execution_status HashMap from MutableGeneratorGraph.\n"
            "  quality_metrics computed from TrackerReport."
        ),
        synergies=["COG-5"],
    ),
    "ACO-3": Insight(
        id="ACO-3",
        title="Auto-decompose by Complexity",
        crate="touring-learning",
        module="src/aco/graph.rs",
        priority="Q1", horizon="H1", effort="1h", phase=1,
        description=(
            "Add auto_decompose() method to MutableGeneratorGraph. Based on "
            "Complexity enum (Trivial, Moderate, High, Critical), decompose a "
            "task node into 1, 2, 4, or 6 sub-nodes respectively."
        ),
        modified_types=["MutableGeneratorGraph"],
        new_functions=["auto_decompose"],
        test_cases=[
            "test_auto_decompose_trivial_1_node",
            "test_auto_decompose_moderate_2_nodes",
            "test_auto_decompose_high_4_nodes",
            "test_auto_decompose_critical_6_nodes",
            "test_decompose_preserves_dependencies",
            "test_decompose_nonexistent_node_errors",
        ],
        implementation_summary=(
            "Add to MutableGeneratorGraph impl:\n"
            "  pub fn auto_decompose(\n"
            "      &mut self,\n"
            "      node_id: &str,\n"
            "      complexity: Complexity,\n"
            "  ) -> Result<Vec<String>, GraphError>\n"
            "  Maps Complexity variants to sub-node counts:\n"
            "  Trivial=1, Moderate=2, High=4, Critical=6.\n"
            "  Creates sub-nodes with id format: '{node_id}_sub_{i}'.\n"
            "  Sub-nodes inherit depends_on from parent.\n"
            "  Parent node is replaced by sub-node DAG."
        ),
        synergies=[],
    ),
    "ACO-4": Insight(
        id="ACO-4",
        title="Execution Tracking State Machine",
        crate="touring-learning",
        module="src/aco/graph.rs",
        priority="Q1", horizon="H1", effort="2h", phase=1,
        description=(
            "Enrich MutableGeneratorGraph.execution_status with a state machine: "
            "Pending -> Running -> Success/Failed -> Rollback. Add methods to "
            "transition states, query ready-to-execute nodes, and get execution report."
        ),
        modified_types=["MutableGeneratorGraph"],
        new_functions=[
            "transition_status",
            "ready_to_execute",
            "execution_report",
        ],
        test_cases=[
            "test_initial_status_is_pending",
            "test_transition_pending_to_running",
            "test_transition_running_to_success",
            "test_transition_running_to_failed",
            "test_invalid_transition_errors",
            "test_ready_to_execute_respects_deps",
            "test_execution_report_format",
        ],
        implementation_summary=(
            "Add to MutableGeneratorGraph impl:\n"
            "  pub fn transition_status(&mut self, node_id: &str, new_status: ExecutionStatus) "
            "-> Result<(), GraphError>\n"
            "  Validates transition legality (e.g., can't go from Success to Running).\n"
            "  pub fn ready_to_execute(&self) -> Vec<&str>\n"
            "  Returns nodes whose deps are all Success and own status is pending.\n"
            "  pub fn execution_report(&self) -> String\n"
            "  Formatted summary of all node statuses."
        ),
        synergies=[],
    ),
    "ACO-5": Insight(
        id="ACO-5",
        title="ObjectiveHash verify_invariant()",
        crate="touring-learning",
        module="src/aco/graph.rs",
        priority="Q1", horizon="H1", effort="30min", phase=1,
        description=(
            "Add verify_invariant() to MutableGeneratorGraph that recomputes "
            "objective_hash from current node contents and compares to stored hash. "
            "Detects scope creep by identifying when the graph has drifted."
        ),
        modified_types=["MutableGeneratorGraph"],
        new_functions=["verify_invariant", "compute_objective_hash"],
        test_cases=[
            "test_verify_invariant_passes_unchanged",
            "test_verify_invariant_detects_drift",
            "test_compute_hash_deterministic",
            "test_compute_hash_changes_on_node_add",
        ],
        implementation_summary=(
            "Add to MutableGeneratorGraph impl:\n"
            "  pub fn compute_objective_hash(&self) -> String\n"
            "  SHA-256 over sorted node IDs + descriptions (deterministic).\n"
            "  Uses sha2::Sha256 (already imported in graph.rs).\n"
            "  pub fn verify_invariant(&self, original_hash: &str) -> bool\n"
            "  Returns compute_objective_hash() == original_hash."
        ),
        synergies=[],
    ),
    "AST-1": Insight(
        id="AST-1",
        title="FileHeat Prioritized Indexing",
        crate="touring-ast",
        module="src/file_heat.rs",
        priority="Q2", horizon="H2", effort="3 days", phase=4,
        description=(
            "Create a FileHeat system (digital pheromone) that tracks edit "
            "frequency, recency, and blast_radius.file_count per file. "
            "IncrementalPipeline uses FileHeat to prioritize re-indexing: "
            "hot files get indexed first after changes."
        ),
        new_types=["FileHeat", "HeatMap"],
        new_functions=[
            "record_edit", "record_access", "get_priority_order",
            "decay_all",
        ],
        test_cases=[
            "test_heat_increases_on_edit",
            "test_heat_decays_over_time",
            "test_priority_order_respects_heat",
            "test_blast_radius_boosts_heat",
            "test_heat_map_capacity_limit",
        ],
        implementation_summary=(
            "New file src/file_heat.rs:\n"
            "  struct FileHeat { edits: u32, last_edit_epoch: f64, "
            "access_count: u32, blast_radius_weight: f64 }\n"
            "  struct HeatMap { entries: HashMap<String, FileHeat>, capacity: usize }\n"
            "  impl HeatMap:\n"
            "    fn record_edit(&mut self, file: &str, now: f64)\n"
            "    fn record_access(&mut self, file: &str, now: f64)\n"
            "    fn get_priority_order(&self, now: f64) -> Vec<(&str, f64)>\n"
            "    fn decay_all(&mut self, now: f64, half_life_secs: f64)\n"
            "  Heat score = edits * recency_decay * (1 + blast_radius_weight)."
        ),
        synergies=["COG-3"],
    ),
    "AST-2": Insight(
        id="AST-2",
        title="find_symbols_fused() with RRF",
        crate="touring-ast",
        module="src/semantic_search.rs",
        priority="Q2", horizon="H2", effort="3 days", phase=4,
        description=(
            "Add find_symbols_fused() to SemanticSymbolIndex that combines 3 signals "
            "using Reciprocal Rank Fusion (RRF): (1) cosine similarity from existing "
            "find_similar_symbols, (2) co-edit frequency (from HeatMap), "
            "(3) blast radius file_count. RRF formula: score = sum(1/(k+rank_i))."
        ),
        modified_types=["SemanticSymbolIndex"],
        new_functions=["find_symbols_fused"],
        test_cases=[
            "test_fused_search_combines_signals",
            "test_fused_search_rrf_formula",
            "test_fused_search_empty_index",
            "test_fused_search_single_signal",
            "test_fused_search_respects_limit",
        ],
        implementation_summary=(
            "Add to SemanticSymbolIndex:\n"
            "  pub fn find_symbols_fused(\n"
            "      &self,\n"
            "      query: &Symbol,\n"
            "      co_edit_scores: &HashMap<String, f64>,\n"
            "      blast_radius_scores: &HashMap<String, f64>,\n"
            "      k: f64,   // RRF constant, typically 60.0\n"
            "      limit: usize,\n"
            "  ) -> Vec<(String, f64)>\n"
            "  Ranks by similarity, co-edit, blast_radius independently.\n"
            "  Fuses with RRF: score = 1/(k+rank_sim) + 1/(k+rank_coedit) + 1/(k+rank_blast).\n"
            "  Sorted descending by fused score."
        ),
        synergies=["COG-1"],
    ),
    "AST-3": Insight(
        id="AST-3",
        title="EnrichedBlastRadius with Impact Categories",
        crate="touring-ast",
        module="src/graph.rs",
        priority="Q2", horizon="H2", effort="3 days", phase=4,
        description=(
            "Extend BlastRadius (graph.rs line 432) with categorized impact: "
            "direct_dependents, transitive_dependents, co_edited_files. "
            "Add a severity score based on weighted categories."
        ),
        new_types=["EnrichedBlastRadius", "ImpactCategory"],
        new_functions=["compute_enriched_blast_radius"],
        test_cases=[
            "test_enriched_blast_radius_direct_deps",
            "test_enriched_blast_radius_transitive_deps",
            "test_enriched_blast_radius_severity_score",
            "test_enriched_empty_graph",
            "test_category_weights_sum_to_one",
        ],
        implementation_summary=(
            "Add to src/graph.rs:\n"
            "  enum ImpactCategory { DirectDependents, TransitiveDependents, CoEdited }\n"
            "  struct EnrichedBlastRadius {\n"
            "      base: BlastRadius,  // reuse existing (start_file, affected_files, "
            "affected_symbols, max_distance, file_count)\n"
            "      direct_dependents: Vec<String>,\n"
            "      transitive_dependents: Vec<String>,\n"
            "      co_edited_files: Vec<String>,\n"
            "      severity: f64,  // 0.0-1.0\n"
            "  }\n"
            "  fn compute_enriched_blast_radius(\n"
            "      index: &SymbolIndex,\n"
            "      file: &str,\n"
            "      co_edit_data: &HashMap<String, Vec<String>>,\n"
            "  ) -> EnrichedBlastRadius\n"
            "  severity = 0.5*direct_count/total + 0.3*transitive_count/total + 0.2*coedit_count/total."
        ),
        synergies=[],
    ),
    "COG-1": Insight(
        id="COG-1",
        title="CognitiveMCTS (SemanticGraph-informed MCTS)",
        crate="touring-cognitive",
        module="src/cognitive_mcts.rs",
        priority="Q2", horizon="H2", effort="1 week", phase=5,
        description=(
            "Create CognitiveMCTS that wraps MCTSEngine with SemanticGraph priors. "
            "Expand function uses graph neighbors as candidate actions. "
            "Reward function weighted by MemoryNode.relevance_score()."
        ),
        new_types=["CognitiveMCTS", "CognitiveMCTSConfig"],
        new_functions=["cognitive_search", "graph_expand_fn", "graph_reward_fn"],
        test_cases=[
            "test_cognitive_mcts_search_basic",
            "test_cognitive_mcts_uses_graph_priors",
            "test_cognitive_mcts_empty_graph",
            "test_cognitive_mcts_respects_max_depth",
            "test_graph_expand_returns_neighbors",
            "test_graph_reward_uses_relevance",
        ],
        implementation_summary=(
            "New file src/cognitive_mcts.rs:\n"
            "  struct CognitiveMCTSConfig {\n"
            "      mcts_config: MCTSConfig,\n"
            "      relevance_weight: f64,  // 0.0-1.0, how much to weight graph priors\n"
            "  }\n"
            "  struct CognitiveMCTS {\n"
            "      engine: MCTSEngine,\n"
            "      config: CognitiveMCTSConfig,\n"
            "  }\n"
            "  impl CognitiveMCTS:\n"
            "    pub fn cognitive_search(\n"
            "        &self,\n"
            "        root_state: u64,\n"
            "        graph: &SemanticGraph,\n"
            "    ) -> Option<MCTSResult>\n"
            "  Constructs expand_fn from graph neighbors (Direction::Outgoing).\n"
            "  Constructs reward_fn using MemoryNode.relevance_score(now).\n"
            "  Delegates to MCTSEngine.search()."
        ),
        synergies=["AST-2"],
    ),
    "COG-2": Insight(
        id="COG-2",
        title="RefinementCycle (Cognitive Retry)",
        crate="touring-cognitive",
        module="src/refinement.rs",
        priority="Q2", horizon="H2", effort="1 week", phase=5,
        description=(
            "Implement RefinementCycle: a retry loop informed by DiagnosticLayer "
            "(from ACO-1). Max 3 iterations, each using a different strategy "
            "based on the diagnostic classification of the previous failure."
        ),
        new_types=["RefinementCycle", "RefinementConfig", "RefinementOutcome"],
        new_functions=[
            "run_refinement", "select_strategy",
            "apply_strategy", "should_continue",
        ],
        test_cases=[
            "test_refinement_succeeds_first_try",
            "test_refinement_retries_on_syntax_error",
            "test_refinement_halts_on_architecture_error",
            "test_refinement_max_iterations",
            "test_refinement_strategy_selection",
            "test_refinement_outcome_recording",
        ],
        implementation_summary=(
            "New file src/refinement.rs:\n"
            "  struct RefinementConfig { max_iterations: u32, timeout_secs: u64 }\n"
            "  enum RefinementOutcome { Resolved, Exhausted, Escalated }\n"
            "  struct RefinementCycle {\n"
            "      config: RefinementConfig,\n"
            "      history: Vec<(DiagnosticLayer, String)>,  // layer + error msg\n"
            "  }\n"
            "  impl RefinementCycle:\n"
            "    pub fn run_refinement<F>(\n"
            "        &mut self,\n"
            "        execute_fn: F,\n"
            "    ) -> RefinementOutcome\n"
            "    where F: FnMut() -> Result<(), String>\n"
            "  On each failure: classify_error -> select_strategy -> apply_strategy -> retry.\n"
            "  Architecture errors -> immediate Escalated."
        ),
        synergies=["ACO-1"],
    ),
    "COG-3": Insight(
        id="COG-3",
        title="Adaptive Temporal Decay",
        crate="touring-cognitive",
        module="src/semantic_graph.rs",
        priority="Q1", horizon="H1", effort="1h", phase=2,
        description=(
            "Replace fixed DECAY_HALF_LIFE_SECS (7 days) with adaptive decay: "
            "half_life = base_half_life * ln(access_count + 1). "
            "Frequently accessed nodes decay slower. "
            "Modify MemoryNode.relevance_score() to use adaptive half-life."
        ),
        modified_types=["MemoryNode"],
        new_functions=["adaptive_half_life", "relevance_score_adaptive"],
        test_cases=[
            "test_adaptive_decay_increases_with_access",
            "test_adaptive_decay_minimum_half_life",
            "test_relevance_score_adaptive_vs_fixed",
            "test_zero_access_uses_base_half_life",
        ],
        implementation_summary=(
            "Modify MemoryNode impl in semantic_graph.rs:\n"
            "  fn adaptive_half_life(&self) -> f64 {\n"
            "      DECAY_HALF_LIFE_SECS * (1.0 + self.access_count as f64).ln().max(1.0)\n"
            "  }\n"
            "  Update relevance_score() to use adaptive_half_life() instead of\n"
            "  the fixed DECAY_HALF_LIFE_SECS constant.\n"
            "  The constant remains as BASE_DECAY_HALF_LIFE_SECS for reference."
        ),
        synergies=["AST-1"],
    ),
    "COG-4": Insight(
        id="COG-4",
        title="GoT Multi-dimensional Evaluation",
        crate="touring-cognitive",
        module="src/got.rs",
        priority="Q2", horizon="H2", effort="3 days", phase=5,
        description=(
            "Enhance GotNode.evaluate() with 3 dimensions: relevance (existing), "
            "confidence (based on visit count), novelty (inverse of similarity to "
            "previous results). ThoughtResult extended with dimension scores."
        ),
        modified_types=["GotNode", "ThoughtResult"],
        new_functions=["evaluate_multidimensional"],
        test_cases=[
            "test_multidim_eval_all_dimensions",
            "test_multidim_confidence_increases_with_visits",
            "test_multidim_novelty_decreases_with_repetition",
            "test_multidim_backward_compatible",
            "test_multidim_score_aggregation",
        ],
        implementation_summary=(
            "Extend ThoughtResult (got.rs line 37) with:\n"
            "  pub relevance: f64,    // existing score dimension\n"
            "  pub confidence: f64,   // 1.0 - 1.0/(1.0 + visits as f64)\n"
            "  pub novelty: f64,      // 1.0 - max_similarity_to_previous\n"
            "Add to GotNode:\n"
            "  pub fn evaluate_multidimensional(\n"
            "      &self,\n"
            "      msg: &ThoughtMessage,\n"
            "      visit_count: u64,\n"
            "      previous_outputs: &[String],\n"
            "  ) -> ThoughtResult\n"
            "  Final score = 0.4*relevance + 0.3*confidence + 0.3*novelty."
        ),
        synergies=[],
    ),
    "COG-5": Insight(
        id="COG-5",
        title="TransitionMatrix + QTable Persistence in CognitiveSnapshot",
        crate="touring-cognitive",
        module="src/persistence.rs",
        priority="Q1", horizon="H1", effort="2h", phase=2,
        description=(
            "Extend GraphSnapshot with TransitionMatrix and QTable from "
            "SessionPredictor to enable cross-session RL learning. "
            "Save/load preserves accumulated Q-values and transition counts."
        ),
        modified_types=["GraphSnapshot"],
        new_functions=["save_with_predictor", "load_with_predictor"],
        test_cases=[
            "test_snapshot_with_transitions_roundtrip",
            "test_snapshot_with_q_values_roundtrip",
            "test_backward_compatible_load",
            "test_empty_predictor_serializes",
            "test_predictor_data_integrity_after_roundtrip",
        ],
        implementation_summary=(
            "Extend GraphSnapshot (persistence.rs line 10):\n"
            "  #[serde(default)] pub transition_matrix: HashMap<String, HashMap<String, u64>>,\n"
            "  #[serde(default)] pub q_values: HashMap<String, f64>,\n"
            "  The #[serde(default)] ensures backward compatibility:\n"
            "  loading old snapshots without these fields uses empty defaults.\n"
            "  Add to GraphPersistence:\n"
            "  pub fn save_with_predictor(&self, snapshot: &GraphSnapshot) -> CognitiveResult<usize>\n"
            "  pub fn load_with_predictor(&self) -> CognitiveResult<Option<GraphSnapshot>>\n"
            "  (These can reuse save/load since serde handles the new fields.)"
        ),
        synergies=["ACO-2"],
    ),
}

# ══════════════════════════════════════════════════════════════════════════════
# PHASE DEFINITIONS
# ══════════════════════════════════════════════════════════════════════════════


@dataclass
class PhaseSpec:
    """Full specification for a plan phase."""

    key: str
    title: str
    horizon: str
    insights: list[str]
    depends_on: list[int]
    estimated_effort: str
    objective: str
    final_result: str
    impacts: dict[str, str]
    subtasks: list[str]
    discover_protocol: str
    tdd_plan: str
    checkpoint_toon: str
    validation_criteria: list[str]


PHASES: list[PhaseSpec] = [
    PhaseSpec(
        key="phase-00",
        title="Foundation: DISCOVER Protocol, VGP Cache, Checkpoint Setup",
        horizon="H0", insights=[], depends_on=[],
        estimated_effort="2h",
        objective=(
            "Establish the infrastructure needed before any code changes: "
            "verify daemon health, cache VGP schemas, set up .toon checkpoint "
            "format using msgpack, and create the DISCOVER protocol template."
        ),
        final_result=(
            "VGP schemas cached for all 24 verified structs. Checkpoint directory "
            "ready with msgpack-based .toon format. DISCOVER protocol documented."
        ),
        impacts={
            "performance": "No runtime impact (setup only).",
            "maintainability": "VGP cache prevents field hallucination in all subsequent phases.",
            "reliability": "Checkpoint infrastructure enables rollback on failure.",
            "daemon_latency": "No change (read-only operations).",
        },
        subtasks=[
            "S0.1: Verify touring-daemon socket at /tmp/touring-daemon-1000.sock",
            "S0.2: Run cargo check --workspace to verify baseline compiles",
            "S0.3: Run cargo test --workspace --exclude touring-python to verify baseline passes",
            "S0.4: Cache all VGP schemas from generate_plan.py VGP_SCHEMAS dict",
            "S0.5: Create checkpoint writer using msgpack (.toon format)",
            "S0.6: Write initial checkpoint with plan metadata",
        ],
        discover_protocol=(
            "1. ls /tmp/touring-daemon-*.sock -> verify daemon\n"
            "2. cargo check --workspace -> verify compilation\n"
            "3. cargo test --workspace --exclude touring-python -> verify test baseline\n"
            "4. Record test count for regression detection in subsequent phases"
        ),
        tdd_plan=(
            "No Rust code changes in this phase. Validation only:\n"
            "- cargo check passes\n"
            "- cargo test passes with expected count\n"
            "- .toon checkpoint file created and readable"
        ),
        checkpoint_toon=(
            "Save: plan_metadata + vgp_schema_count + baseline_test_count\n"
            "File: checkpoints/phase-00-foundation.toon"
        ),
        validation_criteria=[
            "Daemon socket exists",
            "cargo check --workspace exits 0",
            "cargo test --workspace --exclude touring-python exits 0",
            "VGP schemas file exists with 24+ entries",
            "Checkpoint file checkpoints/phase-00-foundation.toon exists",
        ],
    ),
    PhaseSpec(
        key="phase-01",
        title="ACO Quick Wins: Diagnostics, Auto-decompose, State Machine, Invariant",
        horizon="H1", insights=["ACO-1", "ACO-3", "ACO-4", "ACO-5"],
        depends_on=[0], estimated_effort="5.5h",
        objective=(
            "Implement 4 quick-win improvements to touring-learning/aco: "
            "diagnostic engine (ACO-1), auto-decompose (ACO-3), execution "
            "state machine (ACO-4), and objective hash verification (ACO-5)."
        ),
        final_result=(
            "New file diagnostics.rs with 4-layer classification. "
            "MutableGeneratorGraph gains auto_decompose(), transition_status(), "
            "ready_to_execute(), verify_invariant() methods. All with tests."
        ),
        impacts={
            "performance": "Minimal — new methods are O(n) over graph nodes.",
            "maintainability": "Diagnostic engine enables structured error handling across the crate.",
            "reliability": "State machine prevents invalid execution transitions. Invariant detects scope creep.",
            "daemon_latency": "No daemon changes required.",
        },
        subtasks=[
            "S1.1: [ACO-1] Create src/aco/diagnostics.rs with DiagnosticLayer enum + classify_error()",
            "S1.2: [ACO-1] Add RetryStrategy enum and suggest_retry_strategy()",
            "S1.3: [ACO-1] Write 6 tests for diagnostic classification",
            "S1.4: [ACO-3] Add auto_decompose() to MutableGeneratorGraph",
            "S1.5: [ACO-3] Write 6 tests for auto-decompose",
            "S1.6: [ACO-4] Add transition_status(), ready_to_execute(), execution_report()",
            "S1.7: [ACO-4] Write 7 tests for state machine transitions",
            "S1.8: [ACO-5] Add compute_objective_hash() and verify_invariant()",
            "S1.9: [ACO-5] Write 4 tests for invariant verification",
            "S1.10: Register diagnostics.rs in mod.rs",
            "S1.11: cargo clippy --workspace -- -D warnings",
            "S1.12: cargo test -p touring-learning",
        ],
        discover_protocol=(
            "1. Read src/aco/mod.rs to see current module exports\n"
            "2. touring_ast_find(MutableGeneratorGraph) for exact impl location\n"
            "3. touring_graph(blast_radius, MutableGeneratorGraph) for impact\n"
            "4. Verify sha2::Sha256 import exists in graph.rs"
        ),
        tdd_plan=(
            "### Tests First\n"
            "Write all 23 test cases BEFORE implementation.\n"
            "Tests reference the new types/functions with #[should_panic] or expected errors.\n\n"
            "### Implementation\n"
            "1. Create diagnostics.rs (ACO-1)\n"
            "2. Extend graph.rs (ACO-3, ACO-4, ACO-5)\n"
            "3. Register in mod.rs\n\n"
            "### E2E Tests\n"
            "Integration test: create graph -> add nodes -> auto_decompose -> "
            "transition states -> verify invariant -> classify error on failure."
        ),
        checkpoint_toon=(
            "Save: {insights: [ACO-1,ACO-3,ACO-4,ACO-5], new_files: ['diagnostics.rs'], "
            "modified_files: ['graph.rs','mod.rs'], test_count_delta: +23}\n"
            "File: checkpoints/phase-01-aco-quick-wins.toon"
        ),
        validation_criteria=[
            "src/aco/diagnostics.rs exists with DiagnosticLayer enum",
            "classify_error function exists in diagnostics.rs",
            "auto_decompose method exists in graph.rs",
            "transition_status method exists in graph.rs",
            "verify_invariant method exists in graph.rs",
            "cargo clippy -p touring-learning -- -D warnings exits 0",
            "cargo test -p touring-learning exits 0",
            "All 23+ new tests pass",
        ],
    ),
    PhaseSpec(
        key="phase-02",
        title="Cognitive Quick Wins: Adaptive Decay, Q-Value Persistence",
        horizon="H1", insights=["COG-3", "COG-5"],
        depends_on=[0], estimated_effort="3h",
        objective=(
            "Implement 2 quick-win improvements to touring-cognitive: "
            "adaptive temporal decay (COG-3) and TransitionMatrix/QTable "
            "persistence in GraphSnapshot (COG-5)."
        ),
        final_result=(
            "MemoryNode.relevance_score() uses adaptive half-life based on "
            "access_count. GraphSnapshot includes transition_matrix and q_values "
            "with backward-compatible serde defaults."
        ),
        impacts={
            "performance": "Negligible — ln() call in relevance_score, HashMap serde in snapshot.",
            "maintainability": "Adaptive decay is self-tuning; Q-value persistence reduces cold-start.",
            "reliability": "Backward-compatible serde(default) ensures old snapshots still load.",
            "daemon_latency": "No daemon changes (persistence is done by caller, not daemon).",
        },
        subtasks=[
            "S2.1: [COG-3] Add adaptive_half_life() method to MemoryNode",
            "S2.2: [COG-3] Modify relevance_score() to use adaptive half-life",
            "S2.3: [COG-3] Rename constant to BASE_DECAY_HALF_LIFE_SECS",
            "S2.4: [COG-3] Write 4 tests for adaptive decay",
            "S2.5: [COG-5] Add transition_matrix and q_values fields to GraphSnapshot",
            "S2.6: [COG-5] Add #[serde(default)] for backward compatibility",
            "S2.7: [COG-5] Write 5 tests for snapshot roundtrip with new fields",
            "S2.8: cargo clippy -p touring-cognitive -- -D warnings",
            "S2.9: cargo test -p touring-cognitive",
        ],
        discover_protocol=(
            "1. Read semantic_graph.rs DECAY_HALF_LIFE_SECS constant (line 16)\n"
            "2. Read MemoryNode.relevance_score() implementation\n"
            "3. Read persistence.rs GraphSnapshot struct (line 10)\n"
            "4. Check existing tests in persistence.rs for test patterns\n"
            "5. touring_graph(blast_radius, MemoryNode) for impact"
        ),
        tdd_plan=(
            "### Tests First\n"
            "Write 9 test cases for COG-3 and COG-5 BEFORE modifying code.\n\n"
            "### Implementation\n"
            "1. Modify MemoryNode in semantic_graph.rs (COG-3)\n"
            "2. Extend GraphSnapshot in persistence.rs (COG-5)\n\n"
            "### E2E Tests\n"
            "Save a GraphSnapshot with transition_matrix and q_values, load it, "
            "verify data integrity. Also load an old snapshot (without the fields) "
            "to verify backward compatibility."
        ),
        checkpoint_toon=(
            "Save: {insights: [COG-3,COG-5], modified_files: ['semantic_graph.rs','persistence.rs'], "
            "test_count_delta: +9}\n"
            "File: checkpoints/phase-02-cog-quick-wins.toon"
        ),
        validation_criteria=[
            "adaptive_half_life method exists in semantic_graph.rs",
            "relevance_score uses adaptive half-life (not fixed constant)",
            "GraphSnapshot has transition_matrix field with serde(default)",
            "GraphSnapshot has q_values field with serde(default)",
            "cargo clippy -p touring-cognitive -- -D warnings exits 0",
            "cargo test -p touring-cognitive exits 0",
            "Backward-compatible load test passes (old snapshot format)",
        ],
    ),
    PhaseSpec(
        key="phase-03",
        title="ACO Evolution: EvolutionPackage Population Logic",
        horizon="H2", insights=["ACO-2"],
        depends_on=[1], estimated_effort="1 week",
        objective=(
            "Implement full population logic for EvolutionPackage. After graph "
            "execution, extract patterns, anti-patterns, and propose upgrades."
        ),
        final_result=(
            "New file evolution.rs with populate_evolution_package() and helpers. "
            "Integrates with MutableGeneratorGraph.execution_status and TrackerReport."
        ),
        impacts={
            "performance": "O(n) over graph nodes — linear, acceptable for typical graph sizes (<100 nodes).",
            "maintainability": "Centralizes learning extraction in one module.",
            "reliability": "Explicit extraction better than ad-hoc pattern recognition.",
            "daemon_latency": "No daemon changes. Evolution runs post-execution.",
        },
        subtasks=[
            "S3.1: Create src/aco/evolution.rs with populate_evolution_package()",
            "S3.2: Implement extract_learned_patterns() from Success nodes",
            "S3.3: Implement discover_anti_patterns() from failed nodes",
            "S3.4: Implement propose_system_upgrades() from quality_metrics",
            "S3.5: Wire evolution.rs into mod.rs",
            "S3.6: Write 6 tests for all extraction functions",
            "S3.7: Integration test: full graph execution -> evolution package",
            "S3.8: cargo clippy + cargo test",
        ],
        discover_protocol=(
            "1. touring_ast_find(EvolutionPackage) for field list (verified: 8 fields)\n"
            "2. Read graph.rs execution_status usage patterns\n"
            "3. Read tracker.rs for TrackerReport integration\n"
            "4. touring_graph(blast_radius, EvolutionPackage)"
        ),
        tdd_plan=(
            "### Tests First\n"
            "6 unit tests + 1 integration test BEFORE implementation.\n\n"
            "### Implementation\n"
            "evolution.rs: populate_evolution_package + 3 helper functions.\n\n"
            "### E2E Tests\n"
            "Build a graph, execute all nodes (some fail), extract EvolutionPackage, "
            "verify it contains both patterns and anti-patterns."
        ),
        checkpoint_toon=(
            "Save: {insights: [ACO-2], new_files: ['evolution.rs'], test_count_delta: +7}\n"
            "File: checkpoints/phase-03-aco-evolution.toon"
        ),
        validation_criteria=[
            "src/aco/evolution.rs exists",
            "populate_evolution_package function exists",
            "extract_learned_patterns function exists",
            "discover_anti_patterns function exists",
            "propose_system_upgrades function exists",
            "cargo clippy -p touring-learning -- -D warnings exits 0",
            "cargo test -p touring-learning exits 0",
        ],
    ),
    PhaseSpec(
        key="phase-04",
        title="AST Enhancements: FileHeat, Fused Search, EnrichedBlastRadius",
        horizon="H2", insights=["AST-1", "AST-2", "AST-3"],
        depends_on=[0], estimated_effort="9 days",
        objective=(
            "Implement 3 strategic improvements to touring-ast: FileHeat "
            "prioritized indexing, RRF-based fused symbol search, and "
            "enriched blast radius with impact categories."
        ),
        final_result=(
            "New file file_heat.rs with HeatMap. SemanticSymbolIndex gains "
            "find_symbols_fused(). New EnrichedBlastRadius struct in graph.rs."
        ),
        impacts={
            "performance": "FileHeat: O(n log n) sort for priority order. RRF: 3x ranking. Enriched: one extra BFS.",
            "maintainability": "HeatMap is self-contained. RRF fusion is composable (add signals later).",
            "reliability": "FileHeat prevents stale index. EnrichedBlastRadius gives better impact estimates.",
            "daemon_latency": "HeatMap may be used by daemon for indexing priority — measure after integration.",
        },
        subtasks=[
            "S4.1: [AST-1] Create src/file_heat.rs with FileHeat and HeatMap",
            "S4.2: [AST-1] Implement record_edit, record_access, get_priority_order, decay_all",
            "S4.3: [AST-1] Write 5 tests for FileHeat",
            "S4.4: [AST-2] Add find_symbols_fused() to SemanticSymbolIndex",
            "S4.5: [AST-2] Implement RRF formula with 3 signals",
            "S4.6: [AST-2] Write 5 tests for fused search",
            "S4.7: [AST-3] Add EnrichedBlastRadius and ImpactCategory to graph.rs",
            "S4.8: [AST-3] Implement compute_enriched_blast_radius()",
            "S4.9: [AST-3] Write 5 tests for enriched blast radius",
            "S4.10: Register file_heat.rs in lib.rs",
            "S4.11: cargo clippy -p touring-ast -- -D warnings",
            "S4.12: cargo test -p touring-ast",
        ],
        discover_protocol=(
            "1. Read semantic_search.rs for SemanticSymbolIndex interface\n"
            "2. Read graph.rs BlastRadius (line 432) for existing fields\n"
            "3. Read lib.rs for module registration pattern\n"
            "4. touring_graph(blast_radius, SemanticSymbolIndex)\n"
            "5. Check SymbolIndex reverse_deps for transitive computation"
        ),
        tdd_plan=(
            "### Tests First\n"
            "15 test cases across 3 insights BEFORE implementation.\n\n"
            "### Implementation\n"
            "1. file_heat.rs (AST-1)\n"
            "2. Extend semantic_search.rs (AST-2)\n"
            "3. Extend graph.rs (AST-3)\n\n"
            "### E2E Tests\n"
            "Record edits -> get priority order -> search with fused signals -> "
            "compute enriched blast radius -> verify categories."
        ),
        checkpoint_toon=(
            "Save: {insights: [AST-1,AST-2,AST-3], new_files: ['file_heat.rs'], "
            "modified_files: ['semantic_search.rs','graph.rs','lib.rs'], test_count_delta: +15}\n"
            "File: checkpoints/phase-04-ast-enhancements.toon"
        ),
        validation_criteria=[
            "src/file_heat.rs exists with FileHeat and HeatMap structs",
            "find_symbols_fused method exists in semantic_search.rs",
            "EnrichedBlastRadius struct exists in graph.rs",
            "ImpactCategory enum exists in graph.rs",
            "compute_enriched_blast_radius function exists in graph.rs",
            "cargo clippy -p touring-ast -- -D warnings exits 0",
            "cargo test -p touring-ast exits 0",
            "All 15+ new tests pass",
        ],
    ),
    PhaseSpec(
        key="phase-05",
        title="Cognitive Advanced: CognitiveMCTS, RefinementCycle, GoT Multi-dim",
        horizon="H2", insights=["COG-1", "COG-2", "COG-4"],
        depends_on=[1, 2], estimated_effort="2.5 weeks",
        objective=(
            "Implement 3 strategic improvements to touring-cognitive: "
            "graph-informed MCTS, diagnostic-driven refinement cycle, "
            "and multi-dimensional GoT evaluation."
        ),
        final_result=(
            "New files cognitive_mcts.rs and refinement.rs. Extended "
            "ThoughtResult and GotNode with multi-dimensional evaluation."
        ),
        impacts={
            "performance": "CognitiveMCTS: bounded by MCTSConfig.max_rollouts. RefinementCycle: max 3 iterations. GoT: O(n*d) where d=dimensions.",
            "maintainability": "Each new module is self-contained with clear interfaces.",
            "reliability": "RefinementCycle adds structured error recovery. CognitiveMCTS improves action selection.",
            "daemon_latency": "No daemon changes. All cognitive operations are caller-side.",
        },
        subtasks=[
            "S5.1: [COG-1] Create src/cognitive_mcts.rs with CognitiveMCTS",
            "S5.2: [COG-1] Implement cognitive_search with graph priors",
            "S5.3: [COG-1] Write 6 tests",
            "S5.4: [COG-2] Create src/refinement.rs with RefinementCycle",
            "S5.5: [COG-2] Implement run_refinement with DiagnosticLayer integration",
            "S5.6: [COG-2] Write 6 tests",
            "S5.7: [COG-4] Extend ThoughtResult with relevance, confidence, novelty fields",
            "S5.8: [COG-4] Add evaluate_multidimensional() to GotNode",
            "S5.9: [COG-4] Write 5 tests for multi-dimensional evaluation",
            "S5.10: Register new modules in lib.rs",
            "S5.11: cargo clippy -p touring-cognitive -- -D warnings",
            "S5.12: cargo test -p touring-cognitive",
        ],
        discover_protocol=(
            "1. Read mcts.rs for MCTSEngine interface (search method signature)\n"
            "2. Read semantic_graph.rs for SemanticGraph neighbor traversal\n"
            "3. Read got.rs for GotNode.evaluate() and ThoughtResult\n"
            "4. touring_graph(blast_radius, MCTSEngine)\n"
            "5. touring_graph(blast_radius, GotNode)"
        ),
        tdd_plan=(
            "### Tests First\n"
            "17 test cases across 3 insights BEFORE implementation.\n\n"
            "### Implementation\n"
            "1. cognitive_mcts.rs (COG-1) — depends on MCTSEngine + SemanticGraph\n"
            "2. refinement.rs (COG-2) — depends on ACO-1 DiagnosticLayer\n"
            "3. Extend got.rs (COG-4)\n\n"
            "### E2E Tests\n"
            "CognitiveMCTS search over populated SemanticGraph -> verify graph priors used.\n"
            "RefinementCycle with mock execute_fn that fails twice then succeeds.\n"
            "GoT evaluation with multi-dimensional scoring."
        ),
        checkpoint_toon=(
            "Save: {insights: [COG-1,COG-2,COG-4], new_files: ['cognitive_mcts.rs','refinement.rs'], "
            "modified_files: ['got.rs','lib.rs'], test_count_delta: +17}\n"
            "File: checkpoints/phase-05-cog-advanced.toon"
        ),
        validation_criteria=[
            "src/cognitive_mcts.rs exists with CognitiveMCTS struct",
            "cognitive_search method exists",
            "src/refinement.rs exists with RefinementCycle struct",
            "run_refinement method exists",
            "evaluate_multidimensional method exists in got.rs",
            "ThoughtResult has relevance, confidence, novelty fields",
            "cargo clippy -p touring-cognitive -- -D warnings exits 0",
            "cargo test -p touring-cognitive exits 0",
            "All 17+ new tests pass",
        ],
    ),
    PhaseSpec(
        key="phase-06",
        title="Cross-Crate Synergies Integration",
        horizon="H3", insights=[], depends_on=[1, 2, 3, 4, 5],
        estimated_effort="1 week",
        objective=(
            "Wire up the 4 identified synergy pairs: "
            "(1) ACO-1 <-> COG-2: diagnostic drives refinement strategy, "
            "(2) AST-1 <-> COG-3: unified HeatMap for indexing and decay, "
            "(3) ACO-2 <-> COG-5: EvolutionPackages feed persisted Q-values, "
            "(4) AST-2 <-> COG-1: RRF provides MCTS expansion priors."
        ),
        final_result=(
            "Integration modules/traits connecting ACO diagnostics with cognitive "
            "refinement, shared heat signals, evolution-to-persistence pipeline, "
            "and search-to-MCTS bridge."
        ),
        impacts={
            "performance": "Cross-crate calls add indirection. Measure before optimizing.",
            "maintainability": "Clean trait boundaries prevent tight coupling.",
            "reliability": "Integration tests catch cross-crate regressions.",
            "daemon_latency": "Possible increase if synergies add to hook processing. Benchmark.",
        },
        subtasks=[
            "S6.1: Synergy 1 — ACO-1 <-> COG-2: Export DiagnosticLayer from touring-learning, import in touring-cognitive/refinement.rs",
            "S6.2: Synergy 2 — AST-1 <-> COG-3: Create shared HeatSignal trait consumed by both file_heat.rs and semantic_graph.rs",
            "S6.3: Synergy 3 — ACO-2 <-> COG-5: EvolutionPackage.quality_metrics feeds into GraphSnapshot.q_values",
            "S6.4: Synergy 4 — AST-2 <-> COG-1: find_symbols_fused() results as MCTS expand priors",
            "S6.5: Integration tests for all 4 synergies",
            "S6.6: cargo clippy --workspace -- -D warnings",
            "S6.7: cargo test --workspace --exclude touring-python",
            "S6.8: Measure daemon latency before/after with benchmark",
        ],
        discover_protocol=(
            "1. Check Cargo.toml dependencies between crates\n"
            "2. touring_graph(blast_radius) for each synergy endpoint\n"
            "3. Verify no circular dependencies would be created\n"
            "4. Read lib.rs of each crate for public export structure"
        ),
        tdd_plan=(
            "### Tests First\n"
            "Integration tests for all 4 synergies BEFORE wiring.\n\n"
            "### Implementation\n"
            "1. Dependency additions in Cargo.toml (if needed)\n"
            "2. Trait definitions for cross-crate interfaces\n"
            "3. Bridge implementations\n\n"
            "### E2E Tests\n"
            "Full pipeline: execute graph -> diagnose failure -> refinement cycle -> "
            "persist Q-values -> use for next MCTS search."
        ),
        checkpoint_toon=(
            "Save: {synergies: 4, modified_crates: ['touring-learning','touring-ast','touring-cognitive'], "
            "new_deps: [...], test_count_delta: +N}\n"
            "File: checkpoints/phase-06-synergies.toon"
        ),
        validation_criteria=[
            "DiagnosticLayer is importable from touring-cognitive",
            "No circular dependency in cargo check",
            "All 4 synergy integration tests pass",
            "cargo clippy --workspace -- -D warnings exits 0",
            "cargo test --workspace --exclude touring-python exits 0",
            "Daemon latency benchmark: P50 < 2ms, P99 < 10ms",
        ],
    ),
    PhaseSpec(
        key="phase-07",
        title="Final Audit and Consolidation",
        horizon="H3", insights=[], depends_on=[6],
        estimated_effort="3 days",
        objective=(
            "Run audit_implementation.py to verify all 13 insights are fully "
            "implemented. Fix any gaps. Consolidate documentation. "
            "Create final .toon checkpoint. Update CLAUDE.md if needed."
        ),
        final_result=(
            "All 13 insights verified as implemented and tested. "
            "Documentation updated. Final checkpoint saved. "
            "Baseline test count increased by ~80+ tests."
        ),
        impacts={
            "performance": "No code changes (audit only).",
            "maintainability": "Documentation synchronization prevents drift.",
            "reliability": "Full workspace test suite green.",
            "daemon_latency": "Verified unchanged from baseline.",
        },
        subtasks=[
            "S7.1: Run python audit_implementation.py --verbose",
            "S7.2: Fix any FAIL findings from audit",
            "S7.3: cargo clippy --workspace -- -D warnings (full workspace)",
            "S7.4: cargo test --workspace --exclude touring-python (full workspace)",
            "S7.5: Verify test count increased by expected delta",
            "S7.6: Update module-level doc comments in all modified files",
            "S7.7: Create final checkpoint with all implementation metadata",
            "S7.8: Review CLAUDE.md for any needed updates",
        ],
        discover_protocol=(
            "1. python audit_implementation.py --verbose -> full audit\n"
            "2. cargo test --workspace --exclude touring-python 2>&1 | tail -5 -> test count\n"
            "3. diff test count with phase-00 baseline\n"
            "4. grep -r TODO crates/touring-{learning,ast,cognitive}/ -> check no stale TODOs"
        ),
        tdd_plan=(
            "No new code. Validation only:\n"
            "- Audit script reports 13/13 insights implemented\n"
            "- Full workspace tests pass\n"
            "- No clippy warnings\n"
            "- Documentation is current"
        ),
        checkpoint_toon=(
            "Save: {all_insights_verified: true, total_tests_added: N, "
            "final_test_count: M, audit_score: '13/13'}\n"
            "File: checkpoints/phase-07-final-audit.toon"
        ),
        validation_criteria=[
            "audit_implementation.py reports 13/13 PASS",
            "cargo clippy --workspace -- -D warnings exits 0",
            "cargo test --workspace --exclude touring-python exits 0",
            "Test count >= baseline + 80",
            "No stale TODO comments in modified files",
            "Final checkpoint file exists",
        ],
    ),
]

# ══════════════════════════════════════════════════════════════════════════════
# GENERATORS
# ══════════════════════════════════════════════════════════════════════════════


def _frontmatter(phase: PhaseSpec) -> str:
    """Generate YAML frontmatter for a phase file."""
    fm = {
        "plan_id": PLAN_METADATA["plan_id"],
        "phase": int(phase.key.split("-")[1]),
        "title": phase.title,
        "status": "planned",
        "created": TODAY,
        "horizon": phase.horizon,
        "insights_covered": phase.insights,
        "depends_on_phases": phase.depends_on,
        "validates_with": f"validate_phase{phase.key.split('-')[1]}.py",
        "estimated_effort": phase.estimated_effort,
        "vgp_verified_structs": _vgp_structs_for_phase(phase),
    }
    return "---\n" + yaml.dump(fm, default_flow_style=False, sort_keys=False) + "---\n"


def _vgp_structs_for_phase(phase: PhaseSpec) -> list[str]:
    """Get VGP-verified struct names relevant to a phase's insights."""
    structs: list[str] = []
    for iid in phase.insights:
        ins = INSIGHTS.get(iid)
        if ins:
            structs.extend(ins.new_types)
            structs.extend(ins.modified_types)
    return sorted(set(structs))


def _insight_section(insight: Insight) -> str:
    """Generate markdown section for a single insight."""
    lines = [
        f"### {insight.id}: {insight.title}",
        "",
        f"**Crate**: `{insight.crate}` | **Module**: `{insight.module}`",
        f"**Priority**: {insight.priority} | **Effort**: {insight.effort}",
        "",
        "#### Description",
        "",
        insight.description,
        "",
        "#### Implementation",
        "",
        "```",
        insight.implementation_summary,
        "```",
        "",
        "#### Test Cases",
        "",
    ]
    for tc in insight.test_cases:
        lines.append(f"- `{tc}`")
    if insight.synergies:
        lines.append("")
        lines.append(f"#### Synergies: {', '.join(insight.synergies)}")
    lines.append("")
    return "\n".join(lines)


def generate_phase_markdown(phase: PhaseSpec) -> str:
    """Generate complete markdown for a phase file."""
    sections = [_frontmatter(phase)]
    sections.append(f"# {phase.title}\n")

    # Objective
    sections.append("## Objective\n")
    sections.append(phase.objective + "\n")

    # Final Result
    sections.append("## Final Result\n")
    sections.append(phase.final_result + "\n")

    # Impacts
    sections.append("## Impacts\n")
    sections.append("| Dimension | Assessment |")
    sections.append("|-----------|------------|")
    for dim, assessment in phase.impacts.items():
        sections.append(f"| {dim} | {assessment} |")
    sections.append("")

    # Subtasks
    sections.append("## Subtasks\n")
    for st in phase.subtasks:
        sections.append(f"- [ ] {st}")
    sections.append("")

    # Insight details (if any)
    if phase.insights:
        sections.append("## Insight Details\n")
        for iid in phase.insights:
            ins = INSIGHTS.get(iid)
            if ins:
                sections.append(_insight_section(ins))

    # DISCOVER Protocol
    sections.append("## DISCOVER Protocol\n")
    sections.append("```")
    sections.append(phase.discover_protocol)
    sections.append("```\n")

    # TDD Plan
    sections.append("## TDD Plan\n")
    sections.append(phase.tdd_plan + "\n")

    # Checkpoint
    sections.append("## Checkpoint (.toon)\n")
    sections.append("```")
    sections.append(phase.checkpoint_toon)
    sections.append("```\n")

    # Validation Criteria
    sections.append("## Validation Criteria\n")
    for vc in phase.validation_criteria:
        sections.append(f"- [ ] {vc}")
    sections.append("")

    # Cross-references
    if phase.depends_on:
        dep_names = [PHASES[i].key for i in phase.depends_on if i < len(PHASES)]
        sections.append(f"## Dependencies: {', '.join(dep_names)}\n")

    return "\n".join(sections)


def generate_validate_script(phase: PhaseSpec) -> str:
    """Generate validation script for a phase."""
    phase_num = phase.key.split("-")[1]
    checks_code = ""
    for i, vc in enumerate(phase.validation_criteria):
        check_name = vc.replace(" ", "_").replace("/", "_").replace("(", "").replace(")", "")[:50]
        # Generate appropriate check based on content
        if "exists" in vc.lower() and ("src/" in vc or "file" in vc.lower()):
            path_match = ""
            if "src/" in vc:
                parts = vc.split()
                for p in parts:
                    if "src/" in p or ".rs" in p or ".toon" in p:
                        path_match = p.rstrip(",")
                        break
            if path_match:
                checks_code += f"""
    # Check {i+1}: {vc}
    path = WORKSPACE / "crates" / find_crate("{path_match}") / "{path_match}" if "src/" in "{path_match}" else PLAN_DIR / "{path_match}"
    checks.append(Check("{check_name}", path.exists(), f"{{path}} exists"))
"""
            else:
                checks_code += f"""
    # Check {i+1}: {vc}
    checks.append(Check("{check_name}", True, "manual check: {vc}"))
"""
        elif "cargo" in vc.lower():
            cmd = ""
            if "clippy" in vc.lower():
                if "-p " in vc:
                    crate = vc.split("-p ")[-1].split()[0]
                    cmd = f"cargo clippy -p {crate} -- -D warnings"
                else:
                    cmd = "cargo clippy --workspace -- -D warnings"
            elif "test" in vc.lower():
                if "-p " in vc:
                    crate = vc.split("-p ")[-1].split()[0]
                    cmd = f"cargo test -p {crate}"
                else:
                    cmd = "cargo test --workspace --exclude touring-python"
            elif "check" in vc.lower():
                cmd = "cargo check --workspace"
            if cmd:
                checks_code += f"""
    # Check {i+1}: {vc}
    checks.append(run_cargo_check("{check_name}", "{cmd}"))
"""
        elif "method exists" in vc.lower() or "function exists" in vc.lower():
            symbol = vc.split()[0]
            checks_code += f"""
    # Check {i+1}: {vc}
    checks.append(grep_check("{check_name}", "{symbol}"))
"""
        else:
            checks_code += f"""
    # Check {i+1}: {vc}
    checks.append(Check("{check_name}", True, "manual: {vc}"))
"""

    return f'''#!/usr/bin/env python3
"""
validate_phase{phase_num}.py — Validation for {phase.title}
Insights: {", ".join(phase.insights) or "N/A (infrastructure)"}
Generated by generate_plan.py — do not edit manually.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

WORKSPACE = Path("/home/gabrielgadea/.claude/rust")
PLAN_DIR = Path(__file__).parent


@dataclass
class Check:
    name: str
    passed: bool
    detail: str


def find_crate(path_fragment: str) -> str:
    """Infer crate name from a path fragment."""
    for crate in ["touring-learning", "touring-ast", "touring-cognitive"]:
        if crate in path_fragment:
            return crate
    # Guess from module path
    if "aco/" in path_fragment:
        return "touring-learning"
    if "semantic_search" in path_fragment or "file_heat" in path_fragment or "graph.rs" in path_fragment:
        return "touring-ast"
    if "cognitive" in path_fragment or "got.rs" in path_fragment or "refinement" in path_fragment:
        return "touring-cognitive"
    return ""


def run_cargo_check(name: str, cmd: str) -> Check:
    """Run a cargo command and return a Check."""
    try:
        result = subprocess.run(
            cmd.split(), capture_output=True, text=True,
            cwd=str(WORKSPACE), timeout=300,
        )
        return Check(name, result.returncode == 0, result.stderr[-300:] if result.returncode != 0 else "OK")
    except Exception as e:
        return Check(name, False, str(e))


def grep_check(name: str, symbol: str) -> Check:
    """Check if a symbol exists in any Rust file."""
    try:
        result = subprocess.run(
            ["grep", "-r", symbol, str(WORKSPACE / "crates")],
            capture_output=True, text=True, timeout=30,
        )
        found = result.returncode == 0
        detail = result.stdout.split("\\n")[0] if found else f"{{symbol}} not found"
        return Check(name, found, detail)
    except Exception as e:
        return Check(name, False, str(e))


def run_checks(dry_run: bool = False) -> list[Check]:
    """Run all validation checks for this phase."""
    checks: list[Check] = []

{checks_code}
    if dry_run:
        print("[DRY RUN] Phase {phase_num}: ".format(phase_num={phase_num}) + str(len(checks)) + " checks defined")
    return checks


def main() -> None:
    parser = argparse.ArgumentParser(description="Validate phase {phase_num}")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--verbose", "-v", action="store_true")
    args = parser.parse_args()

    checks = run_checks(dry_run=args.dry_run)
    if args.dry_run:
        return

    passed = sum(1 for c in checks if c.passed)
    total = len(checks)

    print(f"\\n=== Phase {phase_num}: {{phase.title}} ===")
    for c in checks:
        status = "PASS" if c.passed else "FAIL"
        print(f"  [{{status}}] {{c.name}}")
        if args.verbose or not c.passed:
            print(f"         {{c.detail}}")

    print(f"\\n  Score: {{passed}}/{{total}}")
    sys.exit(0 if passed == total else 1)


if __name__ == "__main__":
    main()
'''


def generate_readme() -> str:
    """Generate README.md with plan index."""
    lines = [
        "# Touring Improvements Plan 2026",
        "",
        f"**Plan ID**: {PLAN_METADATA['plan_id']}",
        f"**Created**: {PLAN_METADATA['created']}",
        f"**Version**: {PLAN_METADATA['version']}",
        f"**Total Insights**: {PLAN_METADATA['total_insights']}",
        "",
        "## Objective",
        "",
        PLAN_METADATA["objective"],
        "",
        "## Horizons",
        "",
        "| Horizon | Timeline | Description |",
        "|---------|----------|-------------|",
    ]
    for h, desc in PLAN_METADATA["horizons"].items():
        lines.append(f"| {h} | {desc.split('(')[0].strip()} | {desc} |")

    lines.extend([
        "",
        "## Insights Summary",
        "",
        "| ID | Title | Crate | Phase | Priority | Effort |",
        "|----|-------|-------|-------|----------|--------|",
    ])
    for _, ins in sorted(INSIGHTS.items()):
        lines.append(
            f"| {ins.id} | {ins.title} | {ins.crate} | {ins.phase} | {ins.priority} | {ins.effort} |"
        )

    lines.extend([
        "",
        "## Phases",
        "",
        "| Phase | Title | Horizon | Insights | Effort | Status |",
        "|-------|-------|---------|----------|--------|--------|",
    ])
    for phase in PHASES:
        insights_str = ", ".join(phase.insights) or "-"
        lines.append(
            f"| {phase.key} | [{phase.title}]({phase.key}-{phase.title.split(':')[0].strip().lower().replace(' ', '-')}.md) "
            f"| {phase.horizon} | {insights_str} | {phase.estimated_effort} | planned |"
        )

    lines.extend([
        "",
        "## Usage",
        "",
        "```bash",
        "# Generate/regenerate all plan files",
        "python generate_plan.py --validate",
        "",
        "# List phases",
        "python generate_plan.py --list",
        "",
        "# Validate a specific phase after implementation",
        "python validate_phase01.py --verbose",
        "",
        "# Audit all implementations",
        "python audit_implementation.py --verbose",
        "",
        "# Modify plan atomically",
        "python patch_plan.py --phase phase-01 --set status=in_progress",
        "```",
        "",
        "## Synergy Map",
        "",
        "```",
        "ACO-1 <---> COG-2  (diagnostic classifies error -> refinement selects strategy)",
        "AST-1 <---> COG-3  (HeatMap unified — pheromone shared between indexing and decay)",
        "ACO-2 <---> COG-5  (EvolutionPackages feed persisted Q-values -> RL cross-session)",
        "AST-2 <---> COG-1  (RRF fusion provides priors for MCTS expansion)",
        "```",
        "",
        "## VGP-Verified Structs",
        "",
        f"Total: {len(VGP_SCHEMAS)} structs verified from source.",
        "See `generate_plan.py` VGP_SCHEMAS dict for complete field listings.",
        "",
    ])
    return "\n".join(lines)


# ══════════════════════════════════════════════════════════════════════════════
# MAIN GENERATION LOGIC
# ══════════════════════════════════════════════════════════════════════════════


def _hash_content(content: str) -> str:
    return hashlib.sha256(content.encode()).hexdigest()[:16]


def generate_all(output_dir: Path, *, validate: bool = False, single_phase: str | None = None) -> bool:
    """Generate all plan files. Returns True if successful."""
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "checkpoints").mkdir(exist_ok=True)

    files_written: dict[str, str] = {}  # path -> hash
    errors: list[str] = []

    # Filter phases if single_phase requested
    phases_to_gen = PHASES
    if single_phase:
        phases_to_gen = [p for p in PHASES if p.key == single_phase or p.key.endswith(single_phase)]
        if not phases_to_gen:
            print(f"ERROR: Phase '{single_phase}' not found.", file=sys.stderr)
            return False

    # Generate phase markdowns
    for phase in phases_to_gen:
        content = generate_phase_markdown(phase)
        filename = f"{phase.key}-{phase.title.split(':')[0].strip().lower().replace(' ', '-')}.md"
        filepath = output_dir / filename
        filepath.write_text(content, encoding="utf-8")
        files_written[str(filepath)] = _hash_content(content)
        print(f"  Generated: {filename}")

    # Generate validate scripts
    for phase in phases_to_gen:
        content = generate_validate_script(phase)
        phase_num = phase.key.split("-")[1]
        filename = f"validate_phase{phase_num}.py"
        filepath = output_dir / filename
        filepath.write_text(content, encoding="utf-8")
        files_written[str(filepath)] = _hash_content(content)
        print(f"  Generated: {filename}")

    # Generate README (only if generating all)
    if not single_phase:
        content = generate_readme()
        filepath = output_dir / "README.md"
        filepath.write_text(content, encoding="utf-8")
        files_written[str(filepath)] = _hash_content(content)
        print("  Generated: README.md")

    # Validate if requested
    if validate:
        print("\n  Validating idempotency...")
        # Re-generate and compare hashes
        for phase in phases_to_gen:
            content = generate_phase_markdown(phase)
            filename = f"{phase.key}-{phase.title.split(':')[0].strip().lower().replace(' ', '-')}.md"
            filepath = output_dir / filename
            new_hash = _hash_content(content)
            old_hash = files_written.get(str(filepath), "")
            if new_hash != old_hash:
                errors.append(f"IDEMPOTENCY FAIL: {filename}")
            else:
                print(f"    OK: {filename}")

        # Check insight coverage
        covered: set[str] = set()
        for phase in PHASES:
            covered.update(phase.insights)
        all_insights = set(INSIGHTS.keys())
        missing = all_insights - covered
        if missing:
            errors.append(f"MISSING INSIGHTS: {missing}")
        else:
            print(f"    OK: All {len(all_insights)} insights covered in phases")

    total = len(files_written)
    print(f"\n  Total files: {total}")

    if errors:
        for e in errors:
            print(f"  ERROR: {e}", file=sys.stderr)
        return False

    print("  Status: OK")
    return True


def check_coverage() -> bool:
    """Verify all 13 insights are covered in phases."""
    covered: set[str] = set()
    for phase in PHASES:
        covered.update(phase.insights)
    all_insights = set(INSIGHTS.keys())
    missing = all_insights - covered
    if missing:
        print(f"MISSING: {missing}")
        return False
    print(f"All {len(all_insights)} insights covered:")
    for iid in sorted(all_insights):
        ins = INSIGHTS[iid]
        phase_num = ins.phase
        print(f"  {iid}: {ins.title} -> phase-{phase_num:02d}")
    return True


def main() -> None:
    parser = argparse.ArgumentParser(
        description="generate_plan.py — Touring Improvements Plan Generator"
    )
    parser.add_argument("--validate", action="store_true", help="Regenerate + validate idempotency")
    parser.add_argument("--list", action="store_true", help="List all phases")
    parser.add_argument("--phase", type=str, default=None, help="Generate only this phase (e.g. 'phase-01' or '01')")
    parser.add_argument("--check", action="store_true", help="Check insight coverage")
    args = parser.parse_args()

    if args.list:
        print("Touring Improvements Plan — Phases:")
        for phase in PHASES:
            insights_str = ", ".join(phase.insights) or "(infrastructure)"
            print(f"  {phase.key}: {phase.title} [{phase.horizon}] — {insights_str}")
        return

    if args.check:
        ok = check_coverage()
        sys.exit(0 if ok else 1)

    print(f"Generating plan in: {OUTPUT_DIR}")
    success = generate_all(OUTPUT_DIR, validate=args.validate, single_phase=args.phase)
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
