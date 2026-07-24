#!/usr/bin/env python3
"""
generate_plan.py — Code-First Plan Generator for Touring v19 Improvements.

Single source of truth: all plan data lives here as Python structures.
Running this script generates all markdown phase files, validation scripts,
README, audit/patch utilities and the cross-audit script idempotently.

Usage:
    python generate_plan.py              # Generate all files
    python generate_plan.py --validate   # Generate + verify idempotency
    python generate_plan.py --list       # List phases
    python generate_plan.py --phase N    # Generate only phase N
    python generate_plan.py --check      # Verify all 10 insights covered
"""
# -*- coding: utf-8 -*-

from __future__ import annotations

import argparse
import sys
from datetime import date
from pathlib import Path
from textwrap import dedent
from typing import Any

OUTPUT_DIR = Path(__file__).parent
WORKSPACE = Path("/home/gabrielgadea/.claude/rust")
TODAY = str(date.today())

# ══════════════════════════════════════════════════════════════════════════════
# PLAN METADATA
# ══════════════════════════════════════════════════════════════════════════════

PLAN_METADATA: dict[str, Any] = {
    "plan_id": "touring-v19-improvements",
    "created": TODAY,
    "version": "1.0.0",
    "objective": (
        "Implement 10 insights across touring-learning/aco, touring-ast, and "
        "touring-cognitive: TemplateLibrary with learning, GoalTracker 9x9 "
        "computational, PhaseRegistry dynamic, deterministic graph analytics, "
        "CQRS read model, Saga pattern, parallel generator engine, ESAA 24 "
        "subsystems, time-travel debugging, and agent state machine."
    ),
    "total_insights": 10,
    "crates": ["touring-learning", "touring-ast", "touring-cognitive"],
    "workspace": str(WORKSPACE),
    "baseline_tests": 2152,
    "expected_new_tests": 84,
    "expected_final_tests": 2236,
    "horizons": {
        "H0": "Setup (2h)",
        "H1": "Quick Wins (0-2 weeks)",
        "H2": "Strategic (2-8 weeks)",
        "H3": "Synergies + Audit (2-6 months)",
    },
}

# ══════════════════════════════════════════════════════════════════════════════
# VGP-VERIFIED STRUCT SCHEMAS
# All fields extracted from actual source via touring_ast_find.
# ══════════════════════════════════════════════════════════════════════════════

VGP_SCHEMAS: dict[str, dict[str, Any]] = {
    "EvolutionPackage": {
        "file": "crates/touring-learning/src/aco/models.rs",
        "line": 272,
        "kind": "struct",
        "fields": [
            ("session_id", "String"),
            ("objective_hash", "String"),
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
        "line": 254,
        "kind": "struct",
        "fields": [
            ("pattern_id", "String"),
            ("description", "String"),
            ("generator_template", "String"),
            ("domain", "String"),
            ("tags", "Vec<String>"),
        ],
    },
    "GoalTrackerState": {
        "file": "crates/touring-learning/src/aco/models.rs",
        "line": 240,
        "kind": "struct",
        "fields": [
            ("objective_hash", "String"),
            ("phase", "String"),
            ("dimension_scores", "Vec<DimensionScore>"),
            ("overall_score", "f64"),
            ("drift_level", "DriftLevel"),
            ("messages", "Vec<String>"),
            ("iteration", "u32"),
        ],
    },
    "DimensionScore": {
        "file": "crates/touring-learning/src/aco/models.rs",
        "line": 230,
        "kind": "struct",
        "fields": [
            ("name", "String"),
            ("score", "f64"),
            ("sub_scores", "HashMap<String, f64>"),
            ("threshold", "f64"),
            ("drift_level", "DriftLevel"),
        ],
    },
    "GeneratorGraphModel": {
        "file": "crates/touring-learning/src/aco/models.rs",
        "line": 219,
        "kind": "struct",
        "fields": [
            ("nodes", "Vec<GeneratorNode>"),
            ("critical_path", "Vec<String>"),
            ("parallelizable", "Vec<Vec<String>>"),
            ("objective_hash", "String"),
        ],
    },
    "MutableGeneratorGraph": {
        "file": "crates/touring-learning/src/aco/graph.rs",
        "line": 33,
        "kind": "struct",
        "fields": [
            ("graph", "DiGraph<String, ()>"),
            ("index_map", "HashMap<String, NodeIndex>"),
            ("nodes", "BTreeMap<String, GeneratorNode>"),
            ("execution_status", "HashMap<String, ExecutionStatus>"),
            ("dirty", "bool"),
        ],
        "note": "Private fields — accessed via pub methods only",
    },
    "DiagnosticResult": {
        "file": "crates/touring-learning/src/aco/diagnostics.rs",
        "line": 59,
        "kind": "struct",
        "fields": [
            ("layer", "DiagnosticLayer"),
            ("confidence", "f64"),
            ("message", "String"),
            ("retry_strategy", "RetryStrategy"),
        ],
    },
    "DiagnosticLayer": {
        "file": "crates/touring-learning/src/aco/diagnostics.rs",
        "line": 10,
        "kind": "enum",
        "variants": ["Syntax", "Logic", "Contract", "Architecture"],
    },
    "TrackerReport": {
        "file": "crates/touring-learning/src/aco/tracker.rs",
        "line": 65,
        "kind": "struct",
        "fields": [
            ("dims", "Vec<DimResult>"),
            ("composite", "f64"),
            ("status", "TrackerStatus"),
            ("iteration", "u32"),
        ],
    },
    "DimResult": {
        "file": "crates/touring-learning/src/aco/tracker.rs",
        "line": 36,
        "kind": "struct",
        "fields": [
            ("dim_id", "String"),
            ("name", "String"),
            ("score", "f64"),
            ("checks_passed", "u32"),
            ("checks_total", "u32"),
            ("details", "Vec<String>"),
        ],
    },
}

# ══════════════════════════════════════════════════════════════════════════════
# 10 INSIGHTS — Full Specifications
# ══════════════════════════════════════════════════════════════════════════════

INSIGHTS: list[dict[str, Any]] = [
    {
        "id": "INS-1",
        "title": "TemplateLibrary with Learning",
        "crate": "touring-learning",
        "module": "src/aco/template_library.rs",
        "action": "NEW",
        "priority": "Q1",
        "effort": "3 days",
        "roi": 2.00,
        "phase": 1,
        "description": (
            "Create TemplateLibrary that persists LearnedPatterns extracted by "
            "evolution.rs. Currently evolution.rs extracts templates per-session "
            "but discards them. TemplateLibrary stores them in SQLite via Touring "
            "knowledge graph, with similarity search, usage_count tracking, and "
            "version-controlled persistence."
        ),
        "implementation": dedent("""\
            New file src/aco/template_library.rs:
              struct TemplateEntry { pattern: LearnedPattern, usage_count: u32, last_used: u64 }
              struct TemplateLibrary { entries: HashMap<String, TemplateEntry>, version: u32 }
              impl TemplateLibrary:
                pub fn record_template(&mut self, pattern: LearnedPattern)
                  → dedup by pattern_id, increment usage_count if exists
                pub fn find_similar(&self, domain: &str, tags: &[String]) -> Vec<&LearnedPattern>
                  → filter by domain match + tag intersection score >= 0.5
                pub fn top_k(&self, k: usize) -> Vec<&LearnedPattern>
                  → sort by usage_count desc, return first k
                pub fn persist(&self, store: &mut SymbolStore) -> Result<()>
                  → serialize to JSON, store via touring_memory_store key "template::{pattern_id}"
                pub fn load(store: &SymbolStore) -> Result<Self>
                  → recall all entries with prefix "template::"
              Integrate with evolution.rs:
                → In extract_learned_patterns(), call library.record_template(p) for each pattern
                → Library stored in EvolutionPackage.persistence_actions as metadata note
            """),
        "test_cases": [
            "test_record_template_new",
            "test_record_template_dedup_increments_usage",
            "test_find_similar_by_domain",
            "test_find_similar_by_tags",
            "test_top_k_returns_most_used",
            "test_top_k_empty_library",
            "test_persist_and_load_roundtrip",
            "test_version_bump_on_mutation",
        ],
        "synergies": ["COG-MCTS: shared pattern store for MCTS expansion priors"],
        "vgp_structs": ["LearnedPattern", "EvolutionPackage"],
    },
    {
        "id": "INS-2",
        "title": "GoalTracker 9×9 Computational",
        "crate": "touring-learning",
        "module": "src/aco/tracker.rs",
        "action": "MODIFY",
        "priority": "Q1",
        "effort": "2 days",
        "roi": 1.60,
        "phase": 1,
        "description": (
            "Replace scoring math approximations in GoalTracker with real 9×9 "
            "computational verification. Each of the 9 dimensions has 9 sub-checks "
            "(81 total). Currently tracker.rs uses hardcoded score formulas. New "
            "version performs actual verification via DimResult.checks_passed / "
            "checks_total with traceable details per check."
        ),
        "implementation": dedent("""\
            Modify src/aco/tracker.rs:
              Add struct CheckSpec { id: String, description: String, weight: f64 }
              Add const DIM_CHECKS: [(dim_id, [CheckSpec; 9]); 9]
                → 9 dimensions × 9 checks each = 81 specs
              Add fn verify_check(spec: &CheckSpec, context: &CheckContext) -> bool
                → real computation: file exists, symbol count >= threshold, etc.
              Add pub fn compute_dimensional_score_real(
                  &mut self,
                  context: &CheckContext,
              ) -> TrackerReport
                → iterates all 81 checks, fills DimResult.checks_passed/checks_total/details
              Add pub fn verify_9x9_matrix(&self, report: &TrackerReport) -> bool
                → asserts all 9 dims present, each with exactly 9 checks
            """),
        "test_cases": [
            "test_9x9_matrix_completeness",
            "test_compute_dimensional_score_passes_on_valid",
            "test_compute_dimensional_score_fails_on_missing_context",
            "test_verify_9x9_matrix_structure",
            "test_dim_result_checks_tracked",
            "test_tracker_report_composite_from_real_checks",
        ],
        "synergies": ["AST FileHeat: heat-informed dimensional scoring context"],
        "vgp_structs": ["TrackerReport", "DimResult"],
    },
    {
        "id": "INS-3",
        "title": "Plugin/Phase Registry Dynamic",
        "crate": "touring-learning",
        "module": "src/aco/registry.rs",
        "action": "NEW",
        "priority": "Q1",
        "effort": "2 days",
        "roi": 1.50,
        "phase": 2,
        "description": (
            "Create PhaseRegistry that allows phases to be registered at runtime "
            "rather than hardcoded. Current ACO has fixed phase list. Registry "
            "enables plugin-style extension: new phases can be added without "
            "modifying core graph.rs."
        ),
        "implementation": dedent("""\
            New file src/aco/registry.rs:
              pub trait PhaseHandler: Send + Sync {
                  fn phase_id(&self) -> &str;
                  fn execute(&self, graph: &mut MutableGeneratorGraph) -> Result<(), GraphError>;
                  fn rollback(&self, graph: &mut MutableGeneratorGraph) -> Result<(), GraphError>;
              }
              pub struct PhaseRegistry {
                  phases: IndexMap<String, Box<dyn PhaseHandler>>,
              }
              impl PhaseRegistry:
                pub fn new() -> Self
                pub fn register<H: PhaseHandler + 'static>(&mut self, handler: H) -> Result<()>
                  → returns Err if phase_id already registered
                pub fn execute_phase(&self, id: &str, graph: &mut MutableGeneratorGraph) -> Result<()>
                pub fn rollback_phase(&self, id: &str, graph: &mut MutableGeneratorGraph) -> Result<()>
                pub fn phase_ids(&self) -> Vec<&str>
                pub fn len(&self) -> usize
                pub fn is_empty(&self) -> bool
            Add indexmap = "2" to touring-learning Cargo.toml
            """),
        "test_cases": [
            "test_registry_register_and_execute",
            "test_registry_duplicate_registration_fails",
            "test_registry_execute_unknown_phase_fails",
            "test_registry_rollback_called_on_failure",
            "test_registry_phase_ids_in_insertion_order",
        ],
        "synergies": ["ESAA: each of 24 subsystems registered as PhaseHandler"],
        "vgp_structs": ["MutableGeneratorGraph"],
    },
    {
        "id": "INS-4",
        "title": "Deterministic Topological Sort + Graph Analytics",
        "crate": "touring-learning",
        "module": "src/aco/graph.rs",
        "action": "MODIFY",
        "priority": "Q1",
        "effort": "3 days",
        "roi": 1.67,
        "phase": 2,
        "description": (
            "Add deterministic topological sort (BTreeMap-based stable ordering) "
            "and graph analytics metrics to MutableGeneratorGraph. Current sort "
            "is non-deterministic (HashMap iteration). Add GraphMetrics struct "
            "with density, critical_path_length, max_fan_out, max_fan_in."
        ),
        "implementation": dedent("""\
            Modify src/aco/graph.rs:
              Add pub struct GraphMetrics {
                  pub node_count: usize,
                  pub edge_count: usize,
                  pub density: f64,       // edges / (n*(n-1))
                  pub critical_path_length: usize,
                  pub max_fan_out: usize,
                  pub max_fan_in: usize,
              }
              Add pub fn topological_sort_deterministic(&self) -> Result<Vec<String>, GraphError>
                → uses BTreeSet for tie-breaking (ensures same order across runs)
                → returns Err(GraphError::CycleDetected) if cycle found
              Add pub fn compute_graph_metrics(&self) -> GraphMetrics
                → counts nodes, edges, computes density, walks critical path,
                   finds max fan_out/fan_in using BTreeMap iteration for stability
              Update existing topological_sort() to call topological_sort_deterministic()
            """),
        "test_cases": [
            "test_topo_sort_deterministic_simple",
            "test_topo_sort_deterministic_stable_across_insertions",
            "test_topo_sort_detects_cycle",
            "test_graph_metrics_empty",
            "test_graph_metrics_linear_chain",
            "test_graph_metrics_diamond_dag",
        ],
        "synergies": ["PAR-1: parallelizable groups detected via fan-out metrics"],
        "vgp_structs": ["MutableGeneratorGraph", "GeneratorGraphModel"],
    },
    {
        "id": "INS-5",
        "title": "CQRS Read Model",
        "crate": "touring-learning",
        "module": "src/aco/read_model.rs",
        "action": "NEW",
        "priority": "Q2",
        "effort": "2 days",
        "roi": 1.40,
        "phase": 3,
        "description": (
            "Implement CQRS read model for ACO: separate read projections from "
            "write commands. AcoReadModel caches ExecutionStatus per node with "
            "version-based cache invalidation. Commands (execute, rollback) update "
            "write side; reads go through cached projection."
        ),
        "implementation": dedent("""\
            New file src/aco/read_model.rs:
              pub struct AcoSnapshot { pub status: HashMap<String, ExecutionStatus>, pub version: u64 }
              pub struct AcoReadModel {
                  snapshot: AcoSnapshot,
                  cache_valid: bool,
                  rebuild_count: u64,
              }
              impl AcoReadModel:
                pub fn new() -> Self
                pub fn apply_execution(&mut self, node_id: &str, status: ExecutionStatus)
                  → updates snapshot.status, increments version, sets cache_valid=true
                pub fn invalidate(&mut self)
                  → cache_valid = false, increments rebuild_count
                pub fn get_status(&self, node_id: &str) -> Option<&ExecutionStatus>
                  → returns None if !cache_valid (caller must rebuild)
                pub fn rebuild_from_graph(&mut self, graph: &MutableGeneratorGraph)
                  → iterates graph.iter(), populates snapshot from execution_status
                pub fn stats(&self) -> (u64, u64)
                  → (version, rebuild_count)
            """),
        "test_cases": [
            "test_read_model_apply_and_get",
            "test_read_model_invalidate_clears_access",
            "test_read_model_rebuild_from_graph",
            "test_read_model_version_increments",
            "test_read_model_rebuild_count_tracked",
        ],
        "synergies": ["TimeTravelDebugger: snapshots the read model at each epoch"],
        "vgp_structs": ["MutableGeneratorGraph"],
    },
    {
        "id": "INS-6",
        "title": "Saga Pattern with Compensating Transactions",
        "crate": "touring-learning",
        "module": "src/aco/saga.rs",
        "action": "NEW",
        "priority": "Q2",
        "effort": "3 days",
        "roi": 1.40,
        "phase": 3,
        "description": (
            "Implement Saga pattern for multi-step ACO operations with compensating "
            "transactions. Each step has execute + compensate. On failure, all "
            "previous steps are rolled back in reverse order. Enables safe "
            "multi-step phase execution."
        ),
        "implementation": dedent("""\
            New file src/aco/saga.rs:
              pub struct SagaStep {
                  pub name: String,
                  pub execute: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
                  pub compensate: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
              }
              pub enum SagaOutcome { Completed, RolledBack { failed_step: String, reason: String } }
              pub struct SagaOrchestrator {
                  steps: Vec<SagaStep>,
                  history: Vec<String>,   // executed step names (for rollback tracking)
              }
              impl SagaOrchestrator:
                pub fn new() -> Self
                pub fn add_step(&mut self, step: SagaStep)
                pub fn run(&mut self) -> SagaOutcome
                  → executes steps in order; on Err: calls compensate in reverse, returns RolledBack
                pub fn history(&self) -> &[String]
            """),
        "test_cases": [
            "test_saga_all_steps_succeed",
            "test_saga_rollback_on_step_failure",
            "test_saga_compensation_order_reversed",
            "test_saga_history_records_executed_steps",
            "test_saga_empty_steps_completes",
            "test_saga_first_step_failure_no_compensation",
        ],
        "synergies": ["PhaseRegistry: each phase wrapped in SagaStep for safe rollback"],
        "vgp_structs": [],
    },
    {
        "id": "INS-7",
        "title": "Parallel Generator Engine",
        "crate": "touring-learning",
        "module": "src/aco/graph.rs",
        "action": "MODIFY",
        "priority": "Q2",
        "effort": "3 days",
        "roi": 1.33,
        "phase": 4,
        "description": (
            "Add parallel execution engine using Rayon for parallelizable node "
            "groups in MutableGeneratorGraph. GeneratorGraphModel.parallelizable "
            "already identifies parallel groups — this adds execution using "
            "rayon::par_iter for those groups."
        ),
        "implementation": dedent("""\
            Modify src/aco/graph.rs:
              Add rayon = \"1\" to touring-learning Cargo.toml
              Add pub fn execute_parallel_nodes<F>(
                  &self,
                  parallel_groups: &[Vec<String>],
                  execute_fn: F,
              ) -> Vec<(String, Result<(), String>)>
              where F: Fn(&str) -> Result<(), String> + Send + Sync
                → uses rayon::par_iter on each group
                → collects (node_id, result) for all nodes in group
                → sequential between groups (group N+1 only after group N completes)
              Add pub fn parallelizable_groups(&self) -> Vec<Vec<String>>
                → detects nodes with no mutual dependency via adjacency check
                → returns groups as Vec<Vec<String>> in topological order
            """),
        "test_cases": [
            "test_parallel_execute_independent_nodes",
            "test_parallel_groups_sequential_constraint",
            "test_parallelizable_groups_detection",
            "test_parallel_execute_collects_errors",
        ],
        "synergies": ["TOPO-1: uses topological_sort_deterministic for group ordering"],
        "vgp_structs": ["MutableGeneratorGraph", "GeneratorGraphModel"],
    },
    {
        "id": "INS-8",
        "title": "ESAA Complete 24 Subsystems",
        "crate": "touring-learning",
        "module": "src/aco/esaa.rs",
        "action": "MODIFY",
        "priority": "Q2",
        "effort": "1.5 weeks",
        "roi": 1.13,
        "phase": 4,
        "description": (
            "Expand ESAA from current 2 subsystems (QueryCache + EventBuffer) to "
            "the full 24 subsystems identified in the analise project: router, "
            "planner, executor, validator, monitor, coordinator, scheduler, "
            "notifier, aggregator, transformer, filter, dispatcher, collector, "
            "analyzer, reporter, auditor, archiver, indexer, searcher, learner, "
            "predictor, optimizer, balancer, observer."
        ),
        "implementation": dedent("""\
            Modify src/aco/esaa.rs:
              Add trait EsaaSubsystem: Send + Sync {
                  fn subsystem_id(&self) -> &str;
                  fn process(&self, input: &EsaaInput) -> EsaaOutput;
                  fn health_check(&self) -> bool;
              }
              Add struct EsaaInput { pub event_type: String, pub payload: Vec<u8>, pub metadata: HashMap<String,String> }
              Add struct EsaaOutput { pub success: bool, pub result: Vec<u8>, pub latency_us: u64 }
              Add struct EsaaCoordinator {
                  subsystems: IndexMap<String, Box<dyn EsaaSubsystem>>,
                  routing_table: HashMap<String, Vec<String>>,  // event_type -> [subsystem_ids]
              }
              impl EsaaCoordinator:
                pub fn new() -> Self
                pub fn register(&mut self, subsystem: Box<dyn EsaaSubsystem>)
                pub fn route(&self, input: EsaaInput) -> Vec<EsaaOutput>
                pub fn health_report(&self) -> HashMap<String, bool>
              Implement 24 subsystem structs (each: minimal impl satisfying trait):
                Router, Planner, Executor, Validator, Monitor, Coordinator,
                Scheduler, Notifier, Aggregator, Transformer, Filter, Dispatcher,
                Collector, Analyzer, Reporter, Auditor, Archiver, Indexer,
                Searcher, Learner, Predictor, Optimizer, Balancer, Observer
            """),
        "test_cases": [
            "test_esaa_coordinator_register_all_24",
            "test_esaa_router_routes_correctly",
            "test_esaa_planner_processes_input",
            "test_esaa_executor_runs",
            "test_esaa_validator_validates",
            "test_esaa_monitor_health_check",
            "test_esaa_coordinator_route_dispatches",
            "test_esaa_health_report_all_healthy",
            "test_esaa_scheduler_schedule",
            "test_esaa_notifier_notify",
            "test_esaa_aggregator_aggregate",
            "test_esaa_transformer_transform",
            "test_esaa_filter_filter",
            "test_esaa_dispatcher_dispatch",
            "test_esaa_collector_collect",
            "test_esaa_analyzer_analyze",
            "test_esaa_reporter_report",
            "test_esaa_auditor_audit",
            "test_esaa_archiver_archive",
            "test_esaa_indexer_index",
            "test_esaa_searcher_search",
            "test_esaa_learner_learn",
            "test_esaa_predictor_predict",
            "test_esaa_balancer_balance",
        ],
        "synergies": ["PhaseRegistry: each subsystem registered as PhaseHandler"],
        "vgp_structs": [],
    },
    {
        "id": "INS-9",
        "title": "Time-Travel Debugging",
        "crate": "touring-learning",
        "module": "src/aco/time_travel.rs",
        "action": "NEW",
        "priority": "Q2",
        "effort": "3 days",
        "roi": 1.17,
        "phase": 5,
        "description": (
            "Implement time-travel debugging for ACO execution: capture state "
            "snapshots at each epoch, query state_at_epoch, diff two epochs, "
            "replay execution from any checkpoint. Enables post-mortem analysis "
            "of orchestration failures."
        ),
        "implementation": dedent("""\
            New file src/aco/time_travel.rs:
              pub struct StateSnapshot {
                  pub epoch: u64,
                  pub node_statuses: HashMap<String, ExecutionStatus>,
                  pub graph_metrics: GraphMetrics,
                  pub timestamp_ms: u64,
              }
              pub struct EpochDiff {
                  pub from_epoch: u64,
                  pub to_epoch: u64,
                  pub changed_nodes: Vec<(String, ExecutionStatus, ExecutionStatus)>,
                  pub new_nodes: Vec<String>,
                  pub removed_nodes: Vec<String>,
              }
              pub struct TimeTravelDebugger {
                  snapshots: Vec<StateSnapshot>,
                  max_snapshots: usize,
              }
              impl TimeTravelDebugger:
                pub fn new(max_snapshots: usize) -> Self
                pub fn capture_state(&mut self, graph: &MutableGeneratorGraph) -> u64
                  → creates StateSnapshot, appends, returns epoch number
                  → if at capacity: evict oldest snapshot
                pub fn state_at_epoch(&self, epoch: u64) -> Option<&StateSnapshot>
                pub fn diff_epochs(&self, from: u64, to: u64) -> Option<EpochDiff>
                pub fn replay_from(&self, epoch: u64) -> Option<Vec<StateSnapshot>>
                  → returns all snapshots from epoch onward
                pub fn epoch_count(&self) -> usize
            """),
        "test_cases": [
            "test_capture_state_returns_epoch",
            "test_state_at_epoch_found",
            "test_state_at_epoch_not_found",
            "test_diff_epochs_detects_changes",
            "test_diff_epochs_new_nodes",
            "test_replay_from_returns_subsequence",
        ],
        "synergies": ["AcoReadModel: snapshots read model at each epoch"],
        "vgp_structs": ["MutableGeneratorGraph"],
    },
    {
        "id": "INS-10",
        "title": "Agent State Machine Complete",
        "crate": "touring-cognitive",
        "module": "src/agent_state_machine.rs",
        "action": "NEW",
        "priority": "Q2",
        "effort": "2 days",
        "roi": 1.20,
        "phase": 5,
        "description": (
            "Implement formal agent state machine for touring-cognitive. "
            "AgentState enum with 7 states: Idle, Perceiving, Planning, "
            "Executing, Evaluating, Refining, Halted. Transition table enforces "
            "valid state changes. History tracks full state sequence."
        ),
        "implementation": dedent("""\
            New file src/agent_state_machine.rs:
              pub enum AgentState { Idle, Perceiving, Planning, Executing, Evaluating, Refining, Halted }
              pub enum TransitionError { InvalidTransition(AgentState, AgentState), AlreadyHalted }
              pub struct AgentStateMachine {
                  current: AgentState,
                  history: Vec<AgentState>,
                  transition_count: u64,
              }
              const VALID_TRANSITIONS: &[(AgentState, AgentState)] = &[
                (Idle, Perceiving), (Perceiving, Planning), (Planning, Executing),
                (Executing, Evaluating), (Evaluating, Refining), (Refining, Executing),
                (Evaluating, Idle), (Refining, Idle),
                (*, Halted),  // any state -> Halted
              ];
              impl AgentStateMachine:
                pub fn new() -> Self            → starts in Idle
                pub fn current(&self) -> &AgentState
                pub fn transition(&mut self, next: AgentState) -> Result<(), TransitionError>
                  → validates against VALID_TRANSITIONS, appends to history, increments count
                pub fn can_transition(&self, next: &AgentState) -> bool
                pub fn history(&self) -> &[AgentState]
                pub fn transition_count(&self) -> u64
                pub fn reset(&mut self)         → returns to Idle, clears history
            Register in src/lib.rs as pub mod agent_state_machine
            """),
        "test_cases": [
            "test_state_machine_starts_idle",
            "test_valid_transition_sequence",
            "test_invalid_transition_rejected",
            "test_any_state_can_halt",
            "test_history_records_transitions",
            "test_reset_clears_history",
        ],
        "synergies": ["RefinementCycle: drives state transitions during cognitive refinement"],
        "vgp_structs": [],
    },
]

# ══════════════════════════════════════════════════════════════════════════════
# PHASES DEFINITION
# ══════════════════════════════════════════════════════════════════════════════

PHASES: list[dict[str, Any]] = [
    {
        "phase": 0,
        "title": "Foundation: DISCOVER, VGP Cache, Checkpoints",
        "horizon": "H0",
        "insights_covered": [],
        "depends_on_phases": [],
        "estimated_effort": "2h",
        "status": "planned",
        "objective": (
            "Establish pre-conditions for all implementation phases: verify daemon "
            "health (P50=1ms), cache VGP schemas for 10 verified structs, setup "
            ".toon checkpoint format using msgpack, and validate test baseline "
            "(2,152 passed, 0 failed)."
        ),
        "final_result": (
            "VGP schemas cached for 10 structs. Daemon warm. Checkpoint directory "
            "ready with .toon format. Baseline verified: 2,152 tests green."
        ),
        "impacts": [
            ("performance", "No runtime impact (setup only)."),
            ("maintainability", "VGP cache prevents field hallucination in all subsequent phases."),
            ("reliability", "Baseline test count verified before any changes."),
            ("daemon_latency", "Daemon pre-warmed — subsequent hook calls at P50=1ms."),
        ],
        "subtasks": [
            ("S0.1", "Verify daemon: ls /tmp/touring-daemon-$(id -u).sock", []),
            ("S0.2", "Run baseline: cargo test --workspace --exclude touring-python → 2152 passed", []),
            ("S0.3", "Cache VGP schemas via touring_ast_find for all 10 structs", []),
            ("S0.4", "Install msgpack-python: pip install msgpack; verify .toon roundtrip", []),
            ("S0.5", "Create checkpoints/ directory with .gitkeep", []),
            ("S0.6", "Run cargo clippy --workspace -- -D warnings → 0 warnings", []),
        ],
        "discover_protocol": [
            "touring_ast_find(symbol_name='LearnedPattern', definitions_only=true)",
            "touring_ast_find(symbol_name='MutableGeneratorGraph', definitions_only=true)",
            "touring_ast_find(symbol_name='TrackerReport', definitions_only=true)",
            "touring_memory_recall(query='touring-learning aco tracker template', top_k=10)",
            "cargo test --workspace --exclude touring-python 2>&1 | tail -3",
        ],
        "tdd_plan": "No new code. Validation + setup only.",
        "validation_criteria": [
            "cargo test → 2152 passed, 0 failed",
            "cargo clippy → 0 warnings",
            "Daemon socket exists: /tmp/touring-daemon-$(id -u).sock",
            "VGP cache entries > 0 in memory store",
            "checkpoints/ directory exists",
        ],
        "checkpoint": {
            "file": "checkpoints/phase-00-foundation.toon",
            "data": {"baseline_tests": 2152, "daemon_warm": True, "vgp_cached": 10},
        },
    },
    {
        "phase": 1,
        "title": "Quick Wins: TemplateLibrary + GoalTracker 9×9",
        "horizon": "H1",
        "insights_covered": ["INS-1", "INS-2"],
        "depends_on_phases": [0],
        "estimated_effort": "1 week",
        "status": "planned",
        "objective": (
            "Implement INS-1 (TemplateLibrary with Learning, ROI=2.00) and INS-2 "
            "(GoalTracker 9×9 Computational, ROI=1.60). Highest ROI insights — "
            "ship first for early value. Both are self-contained in touring-learning."
        ),
        "final_result": (
            "New template_library.rs with TemplateLibrary. Modified tracker.rs "
            "with 81-check computational verification. 14 new tests."
        ),
        "impacts": [
            ("performance", "TemplateLibrary adds O(1) lookup amortized. GoalTracker adds 81 checks but batched."),
            ("maintainability", "Templates persist cross-session — reduces repeated pattern extraction."),
            ("reliability", "GoalTracker now has real evidence for scores, not math approximations."),
            ("daemon_latency", "No daemon changes. Caller-side only."),
        ],
        "subtasks": [
            ("S1.1", "[INS-1] Write 8 tests for TemplateLibrary (TDD first)", []),
            ("S1.2", "[INS-1] Create src/aco/template_library.rs with TemplateLibrary struct", ["S1.1"]),
            ("S1.3", "[INS-1] Integrate with evolution.rs: call library.record_template()", ["S1.2"]),
            ("S1.4", "[INS-1] Register in src/aco/mod.rs: pub mod template_library", ["S1.2"]),
            ("S1.5", "[INS-2] Write 6 tests for GoalTracker 9×9 (TDD first)", []),
            ("S1.6", "[INS-2] Implement DIM_CHECKS const and verify_check() in tracker.rs", ["S1.5"]),
            ("S1.7", "[INS-2] Implement compute_dimensional_score_real() in tracker.rs", ["S1.6"]),
            ("S1.8", "[INS-2] Implement verify_9x9_matrix() in tracker.rs", ["S1.7"]),
            ("S1.9", "cargo clippy -p touring-learning -- -D warnings", ["S1.4", "S1.8"]),
            ("S1.10", "cargo test -p touring-learning → expect 2152+14 = 2166 passed", ["S1.9"]),
        ],
        "discover_protocol": [
            "touring_ast_find(symbol_name='LearnedPattern', definitions_only=true) → verify fields",
            "touring_ast_find(symbol_name='EvolutionPackage', definitions_only=true) → fields",
            "touring_ast_find(symbol_name='TrackerReport', definitions_only=true) → fields",
            "touring_ast_overview(file_path='crates/touring-learning/src/aco/tracker.rs')",
            "touring_graph(action='blast_radius', symbol='DimResult') → impact scope",
            "touring_memory_recall(query='goaltracker 9x9 computational template library', top_k=5)",
        ],
        "tdd_plan": dedent("""\
            Tests First (14 total):
              - template_library.rs: 8 tests before implementation
              - tracker.rs: 6 tests before compute_dimensional_score_real()
            Implementation:
              1. template_library.rs (INS-1) — new file
              2. tracker.rs modifications (INS-2)
              3. mod.rs registration
              4. evolution.rs integration
            E2E:
              Create EvolutionPackage → extract_learned_patterns → TemplateLibrary.record_template
              → find_similar → top_k → persist → load → assert roundtrip
            """),
        "validation_criteria": [
            "src/aco/template_library.rs exists",
            "TemplateLibrary struct has record_template, find_similar, top_k, persist, load",
            "Evolution.rs calls library.record_template() in extract_learned_patterns",
            "tracker.rs has compute_dimensional_score_real() and verify_9x9_matrix()",
            "cargo clippy -p touring-learning → 0 warnings",
            "cargo test -p touring-learning → 14+ new tests passing",
        ],
        "checkpoint": {
            "file": "checkpoints/phase-01-quick-wins.toon",
            "data": {"insights": ["INS-1", "INS-2"], "new_tests": 14, "new_files": ["template_library.rs"]},
        },
    },
    {
        "phase": 2,
        "title": "Graph Analytics + Phase Registry",
        "horizon": "H1",
        "insights_covered": ["INS-3", "INS-4"],
        "depends_on_phases": [0],
        "estimated_effort": "1 week",
        "status": "planned",
        "objective": (
            "Implement INS-4 (Deterministic Topological Sort + GraphMetrics, ROI=1.67) "
            "and INS-3 (Plugin/Phase Registry, ROI=1.50). Graph analytics first "
            "as PhaseRegistry depends on stable MutableGeneratorGraph API."
        ),
        "final_result": (
            "MutableGeneratorGraph extended with topological_sort_deterministic, "
            "compute_graph_metrics, execute_parallel_nodes. New registry.rs with "
            "PhaseRegistry. 11 new tests."
        ),
        "impacts": [
            ("performance", "Deterministic sort eliminates non-deterministic iteration. BTreeMap = O(n log n)."),
            ("maintainability", "GraphMetrics makes graph health observable. Registry decouples phases from core."),
            ("reliability", "Stable sort prevents flaky tests that depend on HashMap ordering."),
            ("daemon_latency", "No daemon changes."),
        ],
        "subtasks": [
            ("S2.1", "[INS-4] Write 6 tests for graph analytics (TDD first)", []),
            ("S2.2", "[INS-4] Add GraphMetrics struct to graph.rs", ["S2.1"]),
            ("S2.3", "[INS-4] Implement topological_sort_deterministic() in graph.rs", ["S2.2"]),
            ("S2.4", "[INS-4] Implement compute_graph_metrics() in graph.rs", ["S2.3"]),
            ("S2.5", "[INS-3] Write 5 tests for PhaseRegistry (TDD first)", []),
            ("S2.6", "[INS-3] Add indexmap = '2' to touring-learning Cargo.toml", ["S2.5"]),
            ("S2.7", "[INS-3] Create src/aco/registry.rs with PhaseHandler trait + PhaseRegistry", ["S2.6"]),
            ("S2.8", "[INS-3] Register in src/aco/mod.rs: pub mod registry", ["S2.7"]),
            ("S2.9", "cargo clippy -p touring-learning -- -D warnings", ["S2.4", "S2.8"]),
            ("S2.10", "cargo test -p touring-learning → expect +11 more tests", ["S2.9"]),
        ],
        "discover_protocol": [
            "touring_ast_find(symbol_name='MutableGeneratorGraph', definitions_only=true)",
            "touring_ast_overview(file_path='crates/touring-learning/src/aco/graph.rs')",
            "touring_graph(action='blast_radius', symbol='MutableGeneratorGraph') → impact",
            "touring_graph(action='blast_radius', symbol='topological_sort') → dependents",
            "touring_memory_recall(query='topological sort deterministic graph analytics', top_k=5)",
        ],
        "tdd_plan": dedent("""\
            Tests First (11 total):
              - graph.rs: 6 tests for sort + metrics before implementation
              - registry.rs: 5 tests before PhaseRegistry
            Implementation:
              1. graph.rs: GraphMetrics struct + topological_sort_deterministic + compute_graph_metrics
              2. Cargo.toml: indexmap dependency
              3. registry.rs: PhaseHandler trait + PhaseRegistry
              4. mod.rs registration
            E2E:
              Build DAG → topological_sort_deterministic twice → assert same order
              Register 3 phases → execute_phase in order → verify side effects
            """),
        "validation_criteria": [
            "GraphMetrics struct exists in graph.rs with node_count, edge_count, density, critical_path_length, max_fan_out, max_fan_in",
            "topological_sort_deterministic() returns same order for same DAG always",
            "src/aco/registry.rs exists with PhaseHandler trait + PhaseRegistry",
            "cargo check --workspace → 0 errors",
            "cargo clippy -p touring-learning → 0 warnings",
            "cargo test -p touring-learning → 11+ new tests",
        ],
        "checkpoint": {
            "file": "checkpoints/phase-02-graph-registry.toon",
            "data": {"insights": ["INS-4", "INS-3"], "new_tests": 11, "new_files": ["registry.rs"]},
        },
    },
    {
        "phase": 3,
        "title": "Persistence Patterns: CQRS + Saga",
        "horizon": "H2",
        "insights_covered": ["INS-5", "INS-6"],
        "depends_on_phases": [0, 1, 2],
        "estimated_effort": "1 week",
        "status": "planned",
        "objective": (
            "Implement INS-5 (CQRS Read Model, ROI=1.40) and INS-6 "
            "(Saga Pattern, ROI=1.40). CQRS first as Saga depends on clean "
            "read/write separation. Both are independent of phase 1-2 outputs "
            "but benefit from GraphMetrics (phase 2)."
        ),
        "final_result": (
            "New read_model.rs with AcoReadModel. New saga.rs with SagaOrchestrator. "
            "11 new tests."
        ),
        "impacts": [
            ("performance", "CQRS cache avoids repeated graph traversal on reads. Amortized O(1) status lookup."),
            ("maintainability", "Saga makes multi-step rollback declarative and testable."),
            ("reliability", "CQRS version prevents stale reads. Saga ensures compensation on failure."),
            ("daemon_latency", "No daemon changes. Both are caller-side patterns."),
        ],
        "subtasks": [
            ("S3.1", "[INS-5] Write 5 tests for AcoReadModel (TDD first)", []),
            ("S3.2", "[INS-5] Create src/aco/read_model.rs with AcoSnapshot + AcoReadModel", ["S3.1"]),
            ("S3.3", "[INS-5] Register in mod.rs: pub mod read_model", ["S3.2"]),
            ("S3.4", "[INS-6] Write 6 tests for SagaOrchestrator (TDD first)", []),
            ("S3.5", "[INS-6] Create src/aco/saga.rs with SagaStep + SagaOutcome + SagaOrchestrator", ["S3.4"]),
            ("S3.6", "[INS-6] Register in mod.rs: pub mod saga", ["S3.5"]),
            ("S3.7", "cargo clippy -p touring-learning -- -D warnings", ["S3.3", "S3.6"]),
            ("S3.8", "cargo test -p touring-learning → expect +11 more tests", ["S3.7"]),
        ],
        "discover_protocol": [
            "touring_ast_find(symbol_name='MutableGeneratorGraph', definitions_only=true) → iter() method",
            "touring_ast_find(symbol_name='ExecutionStatus', definitions_only=true) → variants",
            "touring_graph(action='blast_radius', symbol='ExecutionStatus') → impact",
            "touring_memory_recall(query='CQRS read model saga compensating transaction rust', top_k=5)",
        ],
        "tdd_plan": dedent("""\
            Tests First (11 total):
              - read_model.rs: 5 tests before AcoReadModel
              - saga.rs: 6 tests before SagaOrchestrator
            Implementation:
              1. read_model.rs (INS-5)
              2. saga.rs (INS-6)
              3. mod.rs registrations
            E2E:
              CQRS: execute 3 nodes → read model → invalidate → rebuild → compare
              Saga: 3-step saga with step 2 failing → verify compensation called for step 1 in reverse
            """),
        "validation_criteria": [
            "src/aco/read_model.rs exists with AcoReadModel (apply_execution, invalidate, get_status, rebuild_from_graph)",
            "src/aco/saga.rs exists with SagaOrchestrator (add_step, run, history)",
            "SagaOutcome::RolledBack includes failed_step and reason",
            "cargo clippy -p touring-learning → 0 warnings",
            "cargo test -p touring-learning → 11+ new tests",
        ],
        "checkpoint": {
            "file": "checkpoints/phase-03-persistence-patterns.toon",
            "data": {"insights": ["INS-5", "INS-6"], "new_tests": 11, "new_files": ["read_model.rs", "saga.rs"]},
        },
    },
    {
        "phase": 4,
        "title": "Performance: Parallel Engine + ESAA 24",
        "horizon": "H2",
        "insights_covered": ["INS-7", "INS-8"],
        "depends_on_phases": [0, 2],
        "estimated_effort": "2.5 weeks",
        "status": "planned",
        "objective": (
            "Implement INS-7 (Parallel Generator Engine, ROI=1.33) and INS-8 "
            "(ESAA 24 subsystems, ROI=1.13). Parallel engine needs INS-4 "
            "(topological_sort_deterministic). ESAA needs PhaseRegistry from INS-3."
        ),
        "final_result": (
            "MutableGeneratorGraph with parallel execution. ESAA with 24 "
            "subsystems + EsaaCoordinator + routing. 28 new tests."
        ),
        "impacts": [
            ("performance", "Rayon parallelism reduces execution time for independent nodes. Speedup = parallelizable_groups.len()."),
            ("maintainability", "ESAA 24 subsystems makes the event-driven architecture extensible via plugin pattern."),
            ("reliability", "Parallel errors collected cleanly — no partial state corruption."),
            ("daemon_latency", "No daemon changes."),
        ],
        "subtasks": [
            ("S4.1", "[INS-7] Write 4 tests for parallel execution (TDD first)", []),
            ("S4.2", "[INS-7] Add rayon = '1' to touring-learning Cargo.toml", ["S4.1"]),
            ("S4.3", "[INS-7] Implement execute_parallel_nodes() in graph.rs", ["S4.2"]),
            ("S4.4", "[INS-7] Implement parallelizable_groups() in graph.rs", ["S4.3"]),
            ("S4.5", "[INS-8] Write 24 tests for ESAA subsystems (TDD first — 1 per subsystem)", []),
            ("S4.6", "[INS-8] Add EsaaSubsystem trait + EsaaInput/EsaaOutput + EsaaCoordinator", ["S4.5"]),
            ("S4.7", "[INS-8] Implement all 24 subsystem structs", ["S4.6"]),
            ("S4.8", "[INS-8] Implement routing_table and EsaaCoordinator.route()", ["S4.7"]),
            ("S4.9", "cargo clippy -p touring-learning -- -D warnings", ["S4.4", "S4.8"]),
            ("S4.10", "cargo test -p touring-learning → expect +28 more tests", ["S4.9"]),
        ],
        "discover_protocol": [
            "touring_ast_find(symbol_name='GeneratorGraphModel', definitions_only=true) → parallelizable field",
            "touring_ast_overview(file_path='crates/touring-learning/src/aco/esaa.rs')",
            "touring_graph(action='blast_radius', symbol='MutableGeneratorGraph') → rayon impact",
            "touring_memory_recall(query='rayon parallel esaa subsystem coordinator', top_k=5)",
        ],
        "tdd_plan": dedent("""\
            Tests First (28 total):
              - graph.rs: 4 tests for parallel execution
              - esaa.rs: 24 tests (1 per subsystem + coordinator)
            Implementation:
              1. Cargo.toml: rayon dependency
              2. graph.rs: execute_parallel_nodes + parallelizable_groups (INS-7)
              3. esaa.rs: EsaaSubsystem trait + 24 impls + EsaaCoordinator (INS-8)
            E2E:
              Build 6-node DAG with 2 parallelizable groups → execute_parallel_nodes → verify all succeed
              Register all 24 subsystems → route event → health_report → all healthy
            """),
        "validation_criteria": [
            "execute_parallel_nodes exists in graph.rs using rayon::par_iter",
            "parallelizable_groups() correctly identifies independent node groups",
            "EsaaSubsystem trait exists with process() and health_check()",
            "EsaaCoordinator has all 24 subsystems registered",
            "cargo clippy -p touring-learning → 0 warnings",
            "cargo test -p touring-learning → 28+ new tests",
        ],
        "checkpoint": {
            "file": "checkpoints/phase-04-performance-esaa.toon",
            "data": {"insights": ["INS-7", "INS-8"], "new_tests": 28, "subsystems_implemented": 24},
        },
    },
    {
        "phase": 5,
        "title": "Cognitive Patterns: Time-Travel + State Machine",
        "horizon": "H2",
        "insights_covered": ["INS-9", "INS-10"],
        "depends_on_phases": [0, 3],
        "estimated_effort": "1 week",
        "status": "planned",
        "objective": (
            "Implement INS-9 (Time-Travel Debugging, ROI=1.17) in touring-learning "
            "and INS-10 (Agent State Machine Complete, ROI=1.20) in touring-cognitive. "
            "INS-9 benefits from CQRS snapshot concept (phase 3). INS-10 is independent."
        ),
        "final_result": (
            "New time_travel.rs with TimeTravelDebugger. New agent_state_machine.rs "
            "in touring-cognitive. 12 new tests."
        ),
        "impacts": [
            ("performance", "TimeTravelDebugger adds O(snapshots) memory. Bounded by max_snapshots."),
            ("maintainability", "Time-travel enables reproducible post-mortem. State machine makes agent flow explicit."),
            ("reliability", "State machine prevents invalid state transitions in cognitive layer."),
            ("daemon_latency", "No daemon changes."),
        ],
        "subtasks": [
            ("S5.1", "[INS-9] Write 6 tests for TimeTravelDebugger (TDD first)", []),
            ("S5.2", "[INS-9] Create src/aco/time_travel.rs with StateSnapshot, EpochDiff, TimeTravelDebugger", ["S5.1"]),
            ("S5.3", "[INS-9] Register in aco/mod.rs: pub mod time_travel", ["S5.2"]),
            ("S5.4", "[INS-10] Write 6 tests for AgentStateMachine (TDD first)", []),
            ("S5.5", "[INS-10] Create src/agent_state_machine.rs in touring-cognitive with AgentState enum", ["S5.4"]),
            ("S5.6", "[INS-10] Implement VALID_TRANSITIONS const and transition() method", ["S5.5"]),
            ("S5.7", "[INS-10] Register in touring-cognitive/src/lib.rs: pub mod agent_state_machine", ["S5.6"]),
            ("S5.8", "cargo clippy --workspace -- -D warnings", ["S5.3", "S5.7"]),
            ("S5.9", "cargo test --workspace --exclude touring-python → expect +12 more tests", ["S5.8"]),
        ],
        "discover_protocol": [
            "touring_ast_find(symbol_name='MutableGeneratorGraph', definitions_only=true) → iter method",
            "touring_ast_overview(file_path='crates/touring-cognitive/src/lib.rs')",
            "touring_ast_find(symbol_name='RefinementCycle', definitions_only=true) → integration point",
            "touring_memory_recall(query='time travel debug state machine cognitive', top_k=5)",
        ],
        "tdd_plan": dedent("""\
            Tests First (12 total):
              - time_travel.rs: 6 tests for TimeTravelDebugger
              - agent_state_machine.rs: 6 tests for AgentStateMachine
            Implementation:
              1. time_travel.rs in touring-learning (INS-9)
              2. agent_state_machine.rs in touring-cognitive (INS-10)
              3. mod.rs/lib.rs registrations
            E2E:
              Capture 5 epochs → state_at_epoch(3) → diff_epochs(1,4) → replay_from(2)
              AgentStateMachine: full cycle Idle→Perceiving→Planning→Executing→Evaluating→Idle
            """),
        "validation_criteria": [
            "src/aco/time_travel.rs exists with TimeTravelDebugger (capture_state, state_at_epoch, diff_epochs, replay_from)",
            "src/agent_state_machine.rs exists in touring-cognitive",
            "AgentState enum has all 7 states",
            "VALID_TRANSITIONS enforced — invalid transitions return Err",
            "cargo clippy --workspace → 0 warnings",
            "cargo test --workspace --exclude touring-python → 12+ new tests",
        ],
        "checkpoint": {
            "file": "checkpoints/phase-05-cognitive-patterns.toon",
            "data": {"insights": ["INS-9", "INS-10"], "new_tests": 12, "new_files": ["time_travel.rs", "agent_state_machine.rs"]},
        },
    },
    {
        "phase": 6,
        "title": "Cross-Crate Synergies Integration",
        "horizon": "H3",
        "insights_covered": [],
        "depends_on_phases": [1, 2, 3, 4, 5],
        "estimated_effort": "1 week",
        "status": "planned",
        "objective": (
            "Wire up 4 key synergies: (1) TemplateLibrary ↔ CognitiveMCTS shared patterns, "
            "(2) GoalTracker ↔ AST FileHeat heat-informed dimensional scoring, "
            "(3) TimeTravelDebugger ↔ AcoReadModel snapshot at epoch, "
            "(4) ESAA subsystems ↔ PhaseRegistry plugin-as-phase."
        ),
        "final_result": (
            "Integration bridges connecting touring-learning ↔ touring-cognitive ↔ touring-ast. "
            "8 integration tests. No circular dependencies."
        ),
        "impacts": [
            ("performance", "Cross-crate calls add indirection. Benchmark before optimizing."),
            ("maintainability", "Clean trait boundaries prevent tight coupling."),
            ("reliability", "Integration tests catch cross-crate regressions."),
            ("daemon_latency", "Possible increase if synergies add to hook processing. Benchmark."),
        ],
        "subtasks": [
            ("S6.1", "Synergy 1: Export TemplateLibrary from touring-learning; use in CognitiveMCTS as expansion prior", []),
            ("S6.2", "Synergy 2: GoalTracker passes HeatMap from touring-ast as dimensional check context", []),
            ("S6.3", "Synergy 3: TimeTravelDebugger.capture_state uses AcoReadModel snapshot", []),
            ("S6.4", "Synergy 4: Register ESAA subsystems via PhaseRegistry.register()", []),
            ("S6.5", "Write 2 integration tests per synergy (8 total)", []),
            ("S6.6", "cargo check --workspace → 0 errors (verify no circular deps)", []),
            ("S6.7", "cargo clippy --workspace -- -D warnings", []),
            ("S6.8", "cargo test --workspace --exclude touring-python", []),
        ],
        "discover_protocol": [
            "Check Cargo.toml deps: touring-learning → touring-ast? touring-cognitive → touring-learning?",
            "touring_graph(action='blast_radius', symbol='TemplateLibrary') → who imports",
            "touring_ast_find(symbol_name='CognitiveMCTS', definitions_only=true) → expand_fn signature",
            "touring_memory_recall(query='cross crate synergy integration circular dependency', top_k=5)",
        ],
        "tdd_plan": dedent("""\
            Tests First (8 integration tests):
              - Synergy 1: 2 tests for TemplateLibrary ↔ CognitiveMCTS
              - Synergy 2: 2 tests for GoalTracker ↔ FileHeat
              - Synergy 3: 2 tests for TimeTravelDebugger ↔ AcoReadModel
              - Synergy 4: 2 tests for ESAA ↔ PhaseRegistry
            Implementation:
              1. Dependency additions in Cargo.toml (verify no cycles: cargo check)
              2. Trait definitions for cross-crate interfaces
              3. Bridge implementations
            """),
        "validation_criteria": [
            "TemplateLibrary importable from touring-cognitive",
            "No circular dependency (cargo check --workspace exits 0)",
            "All 4 synergy integration tests pass",
            "cargo clippy --workspace → 0 warnings",
            "cargo test --workspace --exclude touring-python → 8+ new integration tests",
        ],
        "checkpoint": {
            "file": "checkpoints/phase-06-synergies.toon",
            "data": {"synergies": 4, "new_tests": 8, "modified_crates": ["touring-learning", "touring-cognitive", "touring-ast"]},
        },
    },
    {
        "phase": 7,
        "title": "Final Audit and Cross-Validation",
        "horizon": "H3",
        "insights_covered": [],
        "depends_on_phases": [6],
        "estimated_effort": "3 days",
        "status": "planned",
        "objective": (
            "Run complete audit verifying all 10 insights implemented. Validate test "
            "count reached baseline+84=2236. Clippy clean. Generate cross-audit report. "
            "Verify each insight fulfills its stated ROI contribution."
        ),
        "final_result": (
            "audit_v19.py reports 10/10 insights PASS. Test count 2,236. "
            "Cross-audit report with ROI evidence per insight."
        ),
        "impacts": [
            ("performance", "No code changes (audit only)."),
            ("maintainability", "Documentation synchronized prevents drift."),
            ("reliability", "Full workspace test suite green."),
            ("daemon_latency", "Verified unchanged from baseline."),
        ],
        "subtasks": [
            ("S7.1", "python audit_v19.py --verbose → all 10 insights PASS", []),
            ("S7.2", "Fix any FAIL findings from audit", ["S7.1"]),
            ("S7.3", "cargo clippy --workspace -- -D warnings", []),
            ("S7.4", "cargo test --workspace --exclude touring-python → 2236 passed, 0 failed", []),
            ("S7.5", "Verify test delta: 2236 - 2152 = 84", ["S7.4"]),
            ("S7.6", "Update module-level doc comments in all modified files", []),
            ("S7.7", "Create final checkpoint with all metadata", []),
            ("S7.8", "Review README.md for updates", []),
        ],
        "discover_protocol": [
            "python audit_v19.py --verbose",
            "cargo test --workspace --exclude touring-python 2>&1 | tail -5",
            "grep -r TODO crates/touring-learning/src/aco/ crates/touring-cognitive/src/ | grep -v test",
        ],
        "tdd_plan": "No new code. Validation + audit only.",
        "validation_criteria": [
            "audit_v19.py reports 10/10 PASS",
            "cargo clippy --workspace → 0 warnings",
            "cargo test --workspace → 2236 passed, 0 failed",
            "No stale TODO comments in modified files",
            "Final checkpoint file exists",
        ],
        "checkpoint": {
            "file": "checkpoints/phase-07-final-audit.toon",
            "data": {"all_insights_verified": True, "total_tests_added": 84, "final_test_count": 2236, "audit_score": "10/10"},
        },
    },
]

# ══════════════════════════════════════════════════════════════════════════════
# GENERATORS
# ══════════════════════════════════════════════════════════════════════════════

def _insight_by_id(insight_id: str) -> dict[str, Any]:
    for ins in INSIGHTS:
        if ins["id"] == insight_id:
            return ins
    raise KeyError(f"Insight not found: {insight_id}")


def _phase_frontmatter(phase: dict[str, Any], ins_covered: list[str], depends: list[int]) -> list[str]:
    """Emit YAML frontmatter block for a phase."""
    p = phase["phase"]
    validates_with = f"validate_phase{p:02d}.py"
    lines = ["---"]
    lines += [
        f"plan_id: {PLAN_METADATA['plan_id']}",
        f"phase: {p}",
        f"title: '{phase['title']}'",
        f"status: {phase['status']}",
        f"created: '{TODAY}'",
        f"horizon: {phase['horizon']}",
        f"insights_covered: [{', '.join(ins_covered)}]",
        f"depends_on_phases:",
    ]
    for d in depends:
        lines.append(f"- {d}")
    lines += [
        f"validates_with: {validates_with}",
        f"estimated_effort: {phase['estimated_effort']}",
        f"vgp_verified_structs: [{', '.join(sorted(set(s for ins_id in ins_covered for s in _insight_by_id(ins_id).get('vgp_structs', []))))}]",
    ]
    lines.append("---")
    lines.append("")
    return lines


def _phase_cross_refs(phase: dict[str, Any], ins_covered: list[str], depends: list[int]) -> list[str]:
    """Emit title + cross-reference callouts for a phase."""
    lines = [f"# {phase['title']}", ""]
    if depends:
        lines.append(f"> **Depends on**: Phase {', '.join(str(d) for d in depends)}")
    if ins_covered:
        roi_parts = [f"{ins_id} (ROI={_insight_by_id(ins_id)['roi']:.2f})" for ins_id in ins_covered]
        lines.append(f"> **Insights**: {', '.join(roi_parts)}")
    lines.append("")
    return lines


def _phase_insight_sections(ins_covered: list[str]) -> list[str]:
    """Emit per-insight sections (description, impl, VGP, tests, synergies)."""
    lines = []
    for ins_id in ins_covered:
        ins = _insight_by_id(ins_id)
        lines.append(f"## Insight {ins_id}: {ins['title']}")
        lines.append("")
        lines.append(f"**Crate**: `{ins['crate']}` | **Module**: `{ins['module']}`")
        lines.append(f"**Action**: {ins['action']} | **Priority**: {ins['priority']} | **Effort**: {ins['effort']} | **ROI**: {ins['roi']:.2f}")
        lines.append("")
        lines.append("### Description")
        lines.append("")
        lines.append(ins["description"])
        lines.append("")
        lines.append("### Implementation")
        lines.append("")
        lines.append("```")
        lines.append(ins["implementation"])
        lines.append("```")
        lines.append("")
        lines.append("### VGP-Verified Structs")
        lines.append("")
        if ins["vgp_structs"]:
            for struct_name in ins["vgp_structs"]:
                schema = VGP_SCHEMAS.get(struct_name, {})
                lines.append(f"**{struct_name}** ({schema.get('file', 'unknown')} line {schema.get('line', '?')}):")
                for fname, ftype in schema.get("fields", []):
                    lines.append(f"- `{fname}: {ftype}`")
        else:
            lines.append("No existing structs referenced — all new types.")
        lines.append("")
        lines.append("### Test Cases")
        lines.append("")
        for tc in ins["test_cases"]:
            lines.append(f"- `{tc}`")
        lines.append("")
        if ins["synergies"]:
            lines.append("### Synergies")
            lines.append("")
            for syn in ins["synergies"]:
                lines.append(f"- {syn}")
            lines.append("")
    return lines


def _phase_subtasks_and_protocols(phase: dict[str, Any]) -> list[str]:
    """Emit subtasks + DISCOVER protocol + TDD plan + checkpoint blocks."""
    lines = [
        "## Subtasks",
        "",
    ]
    for stask_id, desc, deps in phase["subtasks"]:
        dep_str = f" (after {', '.join(deps)})" if deps else ""
        lines.append(f"- [ ]{stask_id}: {desc}{dep_str}")
    lines.append("")
    lines.append("## DISCOVER Protocol")
    lines.append("")
    lines.append("```")
    for step in phase["discover_protocol"]:
        lines.append(step)
    lines.append("```")
    lines.append("")
    lines.append("## TDD Plan")
    lines.append("")
    lines.append(phase["tdd_plan"])
    lines.append("")
    chk = phase["checkpoint"]
    lines.append("## Checkpoint (.toon)")
    lines.append("")
    lines.append("```python")
    lines.append(f"# Save: {chk['data']}")
    lines.append(f"# File: {chk['file']}")
    lines.append("import msgpack, pathlib")
    lines.append(f"pathlib.Path('{chk['file']}').write_bytes(msgpack.packb({chk['data']!r}))")
    lines.append("```")
    lines.append("")
    return lines


def _phase_validation_footer(phase: dict[str, Any], depends: list[int]) -> list[str]:
    """Emit validation criteria list + dependencies footer."""
    lines = [
        "## Validation Criteria",
        "",
    ]
    for crit in phase["validation_criteria"]:
        lines.append(f"- [ ]{crit}")
    lines.append("")
    if depends:
        lines.append(f"## Dependencies: phase-{', phase-'.join(f'{d:02d}' for d in depends)}")
        lines.append("")
    return lines


def generate_phase_md(phase: dict[str, Any]) -> str:
    """Generate markdown content for a phase file."""
    ins_covered = phase["insights_covered"]
    depends = phase["depends_on_phases"]

    lines = _phase_frontmatter(phase, ins_covered, depends)
    lines += _phase_cross_refs(phase, ins_covered, depends)

    lines.append("## Objective")
    lines.append("")
    lines.append(phase["objective"])
    lines.append("")

    lines.append("## Final Result")
    lines.append("")
    lines.append(phase["final_result"])
    lines.append("")

    lines.append("## Impacts")
    lines.append("")
    lines.append("| Dimension | Assessment |")
    lines.append("|-----------|------------|")
    for dim, assessment in phase["impacts"]:
        lines.append(f"| {dim} | {assessment} |")
    lines.append("")

    lines += _phase_insight_sections(ins_covered)
    lines += _phase_subtasks_and_protocols(phase)
    lines += _phase_validation_footer(phase, depends)

    return "\n".join(lines)


def generate_validate_py(phase: dict[str, Any]) -> str:
    """Generate a validation script for a phase."""
    p = phase["phase"]
    ins_covered = phase["insights_covered"]

    header = f'''\
#!/usr/bin/env python3
"""
validate_phase{p:02d}.py — Validation script for Phase {p}: {phase["title"]}
Plan: {PLAN_METADATA["plan_id"]}

Runs all validation checks for phase {p} and exits 1 if any FAIL.
Usage: python validate_phase{p:02d}.py [--verbose]
"""
import argparse
import subprocess
import sys
from pathlib import Path

WORKSPACE = Path("{str(WORKSPACE)}")
PLAN_DIR = Path(__file__).parent
VERBOSE = False

PASS = "\\033[92mPASS\\033[0m"
FAIL = "\\033[91mFAIL\\033[0m"
SKIP = "\\033[93mSKIP\\033[0m"


def log(msg: str) -> None:
    if VERBOSE:
        print(msg)


def check(name: str, fn) -> bool:
    try:
        result = fn()
        status = PASS if result else FAIL
        print(f"  {{status}}: {{name}}")
        return bool(result)
    except Exception as exc:
        print(f"  {{FAIL}}: {{name}} — {{exc}}")
        return False


def run_cmd(cmd: str, cwd=None) -> tuple[int, str]:
    """Run a shell command and return (returncode, stdout+stderr)."""
    r = subprocess.run(shlex.split(cmd), shell=False, capture_output=True, text=True, cwd=cwd or WORKSPACE)
    return r.returncode, r.stdout + r.stderr


def main() -> int:
    global VERBOSE
    parser = argparse.ArgumentParser(description="Validate Phase {p}")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()
    VERBOSE = args.verbose

    print(f"=== Phase {p} Validation: {phase["title"]} ===")
    results = []

'''

    checks = []

    # Always check: cargo check
    checks.append(f'''\
    results.append(check(
        "cargo check --workspace exits 0",
        lambda: run_cmd("cargo check --workspace")[0] == 0
    ))
''')

    # Check per insight
    for ins_id in ins_covered:
        ins = _insight_by_id(ins_id)
        mod_path = ins["module"]
        file_path = WORKSPACE / "crates" / ins["crate"] / mod_path
        checks.append(f'''\
    # {ins_id}: {ins["title"]}
    results.append(check(
        "{ins_id}: {mod_path} exists",
        lambda: Path("{file_path}").exists()
    ))
''')
        for struct_or_fn in ins.get("vgp_structs", []):
            checks.append(f'''\
    results.append(check(
        "{ins_id}: {struct_or_fn} struct verified",
        lambda: run_cmd(
            "grep -r 'pub struct {struct_or_fn}\\\\|pub enum {struct_or_fn}' crates/", cwd=WORKSPACE
        )[0] == 0
    ))
''')
        for tc in ins["test_cases"][:3]:  # spot-check first 3
            checks.append(f'''\
    results.append(check(
        "{ins_id}: test {tc} exists",
        lambda tc="{tc}": tc in run_cmd(
            "grep -r '{tc}' crates/", cwd=WORKSPACE
        )[1]
    ))
''')

    # Always check: clippy
    checks.append(f'''\
    results.append(check(
        "cargo clippy --workspace -D warnings exits 0",
        lambda: run_cmd("cargo clippy --workspace -- -D warnings")[0] == 0
    ))
''')

    # Always check: tests
    checks.append(f'''\
    code, out = run_cmd("cargo test --workspace --exclude touring-python 2>&1 | tail -5")
    results.append(check(
        "cargo test --workspace: 0 failed",
        lambda: "0 failed" in out
    ))
    if VERBOSE:
        print(f"  Test output: {{out.strip()}}")
''')

    # Checkpoint exists (if applicable)
    chk_file = phase["checkpoint"]["file"]
    checks.append(f'''\
    results.append(check(
        "Checkpoint {chk_file} exists",
        lambda: (PLAN_DIR / "{chk_file}").exists()
    ))
''')

    footer = '''\

    passed = sum(results)
    total = len(results)
    print(f"\\n{'=' * 40}")
    print(f"Phase {p} Validation: {{passed}}/{{total}} PASS")
    if passed < total:
        print("FAIL — fix issues before proceeding")
        return 1
    print("ALL CHECKS PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
'''
    footer = footer.replace("{p}", str(p))

    return header + "".join(checks) + footer


def generate_readme() -> str:
    """Generate the README.md."""
    plan = PLAN_METADATA
    lines = [
        f"# {plan['plan_id']} — Touring v19 Improvements",
        "",
        f"**Plan ID**: {plan['plan_id']}",
        f"**Created**: {plan['created']}",
        f"**Version**: {plan['version']}",
        f"**Total Insights**: {plan['total_insights']}",
        f"**Baseline Tests**: {plan['baseline_tests']}",
        f"**Expected New Tests**: {plan['expected_new_tests']}",
        f"**Expected Final Tests**: {plan['expected_final_tests']}",
        "",
        "## Objective",
        "",
        plan["objective"],
        "",
        "## Horizons",
        "",
        "| Horizon | Description |",
        "|---------|-------------|",
    ]
    for h, desc in plan["horizons"].items():
        lines.append(f"| {h} | {desc} |")

    lines += [
        "",
        "## Insights by ROI",
        "",
        "| ID | Title | Crate | Phase | ROI | Effort |",
        "|----|-------|-------|-------|-----|--------|",
    ]
    sorted_insights = sorted(INSIGHTS, key=lambda x: -x["roi"])
    for ins in sorted_insights:
        lines.append(
            f"| {ins['id']} | {ins['title']} | {ins['crate']} | {ins['phase']} | {ins['roi']:.2f} | {ins['effort']} |"
        )

    lines += [
        "",
        "## Phases",
        "",
        "| Phase | Title | Horizon | Insights | Status |",
        "|-------|-------|---------|----------|--------|",
    ]
    for ph in PHASES:
        ins_list = ", ".join(ph["insights_covered"]) or "—"
        filename = _phase_filename(ph)
        lines.append(
            f"| {ph['phase']} | [{ph['title']}]({filename}) | {ph['horizon']} | {ins_list} | {ph['status']} |"
        )

    lines += [
        "",
        "## VGP-Verified Structs",
        "",
        "| Struct | File | Line | Kind |",
        "|--------|------|------|------|",
    ]
    for name, schema in VGP_SCHEMAS.items():
        lines.append(f"| {name} | {schema['file']} | {schema['line']} | {schema['kind']} |")

    lines += [
        "",
        "## Usage",
        "",
        "```bash",
        "# Regenerate all plan files",
        "python generate_plan.py",
        "",
        "# Validate idempotency",
        "python generate_plan.py --validate",
        "",
        "# Validate a specific phase",
        "python validate_phase01.py --verbose",
        "",
        "# Cross-audit all 10 insights",
        "python audit_v19.py --verbose",
        "```",
        "",
    ]
    return "\n".join(lines)


def generate_audit_script() -> str:
    """Generate the cross-audit script audit_v19.py."""
    insight_checks = []
    for ins in INSIGHTS:
        file_path = WORKSPACE / "crates" / ins["crate"] / ins["module"]
        test_check = ins["test_cases"][0] if ins["test_cases"] else ""
        insight_checks.append(f"""\
    # {ins['id']}: {ins['title']} (ROI={ins['roi']:.2f})
    results["{ins['id']}"] = {{
        "title": "{ins['title']}",
        "crate": "{ins['crate']}",
        "module": "{ins['module']}",
        "file_exists": Path("{file_path}").exists(),
        "test_present": "{test_check}" in cargo_test_output if "{test_check}" else True,
        "roi": {ins['roi']},
    }}
    results["{ins['id']}"]["pass"] = (
        results["{ins['id']}"]["file_exists"] and
        results["{ins['id']}"]["test_present"]
    )
""")

    return f'''\
#!/usr/bin/env python3
"""
audit_v19.py — Cross-audit script for Touring v19 Improvements.

Verifies all 10 insights are implemented. Reports ROI evidence.
Usage: python audit_v19.py [--verbose]
"""
import argparse
import subprocess
import sys
from pathlib import Path

WORKSPACE = Path("{str(WORKSPACE)}")
PLAN_DIR = Path(__file__).parent

PASS = "\\033[92mPASS\\033[0m"
FAIL = "\\033[91mFAIL\\033[0m"


# NOTE: `run_cmd` is defined once above (line 1406) and reused here. Do not
# redefine — Python would shadow the import-free helper with a duplicate
# that only accepts positional args, breaking earlier call sites.


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    print("=== Touring v19 Cross-Audit ===")
    print(f"Workspace: {{WORKSPACE}}")
    print()

    # Run cargo test once to get test output
    print("Running cargo test (this may take ~30s)...")
    _, cargo_test_output = run_cmd(
        "cargo test --workspace --exclude touring-python 2>&1"
    )

    results: dict[str, dict] = {{}}

{chr(10).join(insight_checks)}

    # Print results
    print("\\n--- Insight Audit ---")
    all_pass = True
    for ins_id, r in results.items():
        status = PASS if r["pass"] else FAIL
        print(f"  {{status}}: {{ins_id}} — {{r['title']}} (ROI={{r['roi']:.2f}})")
        if not r["pass"] and args.verbose:
            print(f"    file_exists: {{r['file_exists']}}")
            print(f"    test_present: {{r['test_present']}}")
        if not r["pass"]:
            all_pass = False

    # Test count check
    print("\\n--- Test Suite ---")
    test_line = [l for l in cargo_test_output.splitlines() if "test result" in l]
    if test_line:
        print(f"  {{test_line[-1].strip()}}")
        if "0 failed" in test_line[-1]:
            print(f"  {{PASS}}: 0 failed")
        else:
            print(f"  {{FAIL}}: failures detected")
            all_pass = False
    else:
        print(f"  {{FAIL}}: Could not parse test output")
        all_pass = False

    # Clippy check
    print("\\n--- Clippy ---")
    code, _ = run_cmd("cargo clippy --workspace -- -D warnings 2>&1")
    clippy_status = PASS if code == 0 else FAIL
    print(f"  {{clippy_status}}: cargo clippy --workspace -D warnings")
    if code != 0:
        all_pass = False

    # Summary
    passed = sum(1 for r in results.values() if r["pass"])
    total = len(results)
    print(f"\\n{'=' * 50}")
    print(f"INSIGHTS: {{passed}}/{{total}} PASS")
    print(f"OVERALL: {{'ALL PASS' if all_pass else 'FAILURES DETECTED'}}")
    return 0 if all_pass else 1


if __name__ == "__main__":
    sys.exit(main())
'''


def _phase_filename(phase: dict[str, Any]) -> str:
    p = phase["phase"]
    slug = phase["title"].lower().replace(" ", "-").replace(":", "").replace("+", "plus")
    slug = "".join(c if c.isalnum() or c == "-" else "" for c in slug)
    slug = slug[:50].rstrip("-")
    return f"phase-{p:02d}-{slug}.md"


def generate_all() -> None:
    """Generate all plan files."""
    generated: list[str] = []

    for phase in PHASES:
        # Phase markdown
        md_file = OUTPUT_DIR / _phase_filename(phase)
        md_content = generate_phase_md(phase)
        md_file.write_text(md_content, encoding="utf-8")
        generated.append(str(md_file.name))

        # Validation script
        val_file = OUTPUT_DIR / f"validate_phase{phase['phase']:02d}.py"
        val_content = generate_validate_py(phase)
        val_file.write_text(val_content, encoding="utf-8")
        generated.append(str(val_file.name))

    # README
    readme = OUTPUT_DIR / "README.md"
    readme.write_text(generate_readme(), encoding="utf-8")
    generated.append("README.md")

    # Audit script
    audit = OUTPUT_DIR / "audit_v19.py"
    audit.write_text(generate_audit_script(), encoding="utf-8")
    generated.append("audit_v19.py")

    # Checkpoints dir
    (OUTPUT_DIR / "checkpoints").mkdir(exist_ok=True)
    (OUTPUT_DIR / "checkpoints" / ".gitkeep").touch()

    print(f"Generated {len(generated)} files:")
    for f in generated:
        print(f"  {f}")


def validate_idempotency() -> bool:
    """Verify that regenerating produces identical files."""
    import hashlib

    snapshots: dict[str, str] = {}
    for phase in PHASES:
        md_file = OUTPUT_DIR / _phase_filename(phase)
        if md_file.exists():
            snapshots[md_file.name] = hashlib.md5(md_file.read_bytes()).hexdigest()

    generate_all()

    all_same = True
    for phase in PHASES:
        md_file = OUTPUT_DIR / _phase_filename(phase)
        new_hash = hashlib.md5(md_file.read_bytes()).hexdigest()
        old_hash = snapshots.get(md_file.name, "")
        if old_hash and old_hash != new_hash:
            print(f"  CHANGED: {md_file.name}")
            all_same = False
        else:
            print(f"  OK: {md_file.name}")
    return all_same


def main() -> None:
    parser = argparse.ArgumentParser(description="Touring v19 Plan Generator")
    parser.add_argument("--validate", action="store_true", help="Validate idempotency")
    parser.add_argument("--list", action="store_true", help="List phases")
    parser.add_argument("--phase", type=int, help="Generate only phase N")
    parser.add_argument("--check", action="store_true", help="Verify all 10 insights covered")
    args = parser.parse_args()

    if args.check:
        covered = set()
        for ph in PHASES:
            covered.update(ph["insights_covered"])
        all_ids = {ins["id"] for ins in INSIGHTS}
        missing = all_ids - covered
        print(f"Insights covered: {len(covered)}/{len(all_ids)}")
        if missing:
            print(f"MISSING: {missing}")
            sys.exit(1)
        print("All 10 insights covered across phases.")
        return

    if args.list:
        for ph in PHASES:
            ins = ", ".join(ph["insights_covered"]) or "—"
            print(f"  Phase {ph['phase']}: {ph['title']} [{ph['horizon']}] — {ins}")
        return

    if args.phase is not None:
        phase = next((p for p in PHASES if p["phase"] == args.phase), None)
        if not phase:
            print(f"Phase {args.phase} not found.")
            sys.exit(1)
        md_file = OUTPUT_DIR / _phase_filename(phase)
        md_file.write_text(generate_phase_md(phase), encoding="utf-8")
        print(f"Generated: {md_file.name}")
        return

    if args.validate:
        ok = validate_idempotency()
        sys.exit(0 if ok else 1)

    generate_all()


if __name__ == "__main__":
    main()
