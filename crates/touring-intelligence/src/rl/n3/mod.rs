//! N3 — Meta-Generation Layer.
//!
//! Generates specialized N1 generators from domain specifications.
//! Part of the N0→N1→N2→N3 hierarchy in Touring.
//!
//! # Architecture
//!
//! ```text
//! DomainSpec ──N3──► GeneratorSpec ──N3──► GeneratorSpec
//!                                               │
//!                                               ▼
//! ObjectiveSpec ──N1──► GeneratedSequence ◄──N3──┘
//! ```

pub mod aco_delegating_generator;
pub mod cortex_handler_gen;
pub mod domain_spec;
#[cfg(test)]
pub mod e2e_audit;
pub mod generator_spec;
pub mod meta_generator;
pub mod n3_pipeline;
pub mod rust_meta_generator;
pub mod wiring_config_gen;

// Re-exports
pub use aco_delegating_generator::{
    AcoDelegatingGenerator, DelegationResult, is_aco_available, test_delegation,
};
pub use cortex_handler_gen::{
    GeneratedHandler, HandlerGenConfig, generate_handler, generate_handlers,
};
pub use domain_spec::{DomainId, DomainSpec, ToolPattern};
pub use generator_spec::{
    GeneratorConfig, GeneratorId, GeneratorSpec, SequencePattern, ToolCallSpec,
};
pub use meta_generator::MetaGenerator;
pub use n3_pipeline::{
    ConfiguredGenerator, DomainBuilder, N3Pipeline, PipelineConfig, PipelinePrepareResult, quick,
};
pub use rust_meta_generator::RustMetaGenerator;
pub use wiring_config_gen::{
    WiringConfig, generate_wiring_config, merge_configs, to_json, to_toml, to_yaml, validate_config,
};

// Re-export QualityGate from n1 for convenience
pub use crate::rl::n1::QualityGate;
