//! ACO (Agentic Code Orchestrator) — Rust acceleration module (v19.0.0).
//!
//! Provides high-performance implementations of ACO core components:
//! - models: Domain types (6 enums + 14 structs, all with Display impls)
//! - graph: DAG operations (topological sort deterministic, critical path, parallelizable_groups,
//!   execute_parallel_nodes via rayon, GraphMetrics — INS-4 + INS-7)
//! - tracker: 9×9 = 81 dimensional checks, compute_dimensional_score_real, weighted scoring
//!   (TrackerReport::summary()) — INS-2
//! - esaa: Event Sourcing Agent Architecture — EsaaSubsystem trait, EsaaCoordinator,
//!   24 subsystems (Router/Validator/Monitor/Transformer/Filter/Analyzer differentiated) — INS-8
//! - diagnostics: Diagnostic pipeline and DiagnosticResult types
//! - evolution: EvolutionPackage management and pattern learning
//! - read_model: CQRS read-side projection (AcoReadModel) — INS-5
//! - registry: PhaseRegistry dynamic with IndexMap — INS-3
//! - saga: SagaOrchestrator with compensating transactions — INS-6
//! - template_library: TemplateLibrary with similarity search and learning — INS-1
//! - time_travel: TimeTravelDebugger with snapshots and diff — INS-9
//!
//! v19.0.0: 10 insights implemented (INS-1 through INS-9 + INS-10 in touring-cognitive).
//! Baseline: 2,258 tests passing, 0 clippy warnings.
//! PyO3 bindings are NOT included here — they belong in touring-python.

pub mod diagnostics;
pub mod esaa;
pub mod evolution;
pub mod graph;
pub mod models;
pub mod pheromone_bus;
pub mod pheromone_bus_metrics;
pub mod read_model;
pub mod registry;
pub mod saga;
pub mod template_library;
pub mod time_travel;
pub mod tracker;
pub mod tracker_rl_bridge;

pub use esaa::{
    Analyzer, EsaaCoordinator, EsaaInput, EsaaOutput, EsaaReaderError, EsaaRkyvReader,
    EsaaSubsystem, EventBuffer, EventBufferConfig, EventRecord, Filter, Monitor, QueryCache,
    Router, Transformer, Validator,
};
pub use graph::{GraphError, MutableGeneratorGraph};
pub use models::*;
pub use pheromone_bus::{PheroKey, UnifiedPheromoneBus};
pub use read_model::AcoReadModel;
pub use registry::{PhaseHandler, PhaseRegistry};
pub use saga::{GeneratorSagaStep, SagaOrchestrator, SagaState, SagaStep, StepResult};
pub use time_travel::{GraphSnapshot, SnapshotDiff, TimeTravelDebugger};
pub use tracker::{
    CRITICAL_DIMS, CRITICAL_WEIGHT, CheckContext, CheckHandler, CheckRegistry, CheckSpec,
    DimResult, DimensionalFeatures, HALT_ITERATIONS, HALT_THRESHOLD, NORMAL_WEIGHT, TrackerReport,
    TrackerStatus, VETO_THRESHOLD, build_report, compute_composite, determine_status,
    dim_from_checks,
};
pub use tracker_rl_bridge::{MultiObjectiveMapping, TrackerRlBridge};
