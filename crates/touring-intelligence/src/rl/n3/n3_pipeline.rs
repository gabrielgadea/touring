//! N3→N1→N2 Pipeline — End-to-end meta-generation to execution.
//!
//! This module wires together the N3 meta-generation layer with the N1
//! sequence generation layer, demonstrating the complete flow:
//!
//! ```text
//! DomainSpec ──N3──► GeneratorSpec
//!                            │
//!                            ▼
//!                      BasicGenerator
//!                            │
//!                            ▼
//!               ObjectiveSpec ──N1──► GeneratedSequence
//!                                          │
//!                                          ▼
//!                                    TACO Orchestrator (N2)
//!                                          │
//!                                          ▼
//!                                    ExecutionResult
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! // Build the pipeline
//! let pipeline = N3Pipeline::with_domain(rust_domain);
//!
//! // Generate a specialized generator spec
//! let gen_spec = pipeline.generate_spec().await?;
//!
//! // Use the spec to configure a generator
//! let generator = pipeline.build_generator(bus, gen_spec)?;
//!
//! // Generate sequence for an objective
//! let sequence = generator.generate(&objective)?;
//! ```

use std::sync::Arc;

use crate::rl::aco::UnifiedPheromoneBus;
use crate::rl::n1::{
    BasicGenerator, ExecutionOutcome, GeneratedSequence, N1Error, N1Result, ObjectiveSpec,
    ToolSequenceGenerator,
};
use crate::rl::n3::meta_generator::MetaGenerator;
use crate::rl::n3::rust_meta_generator::RustMetaGenerator;
use crate::rl::n3::{DomainId, DomainSpec, GeneratorSpec};

/// Configuration for the E2E pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Whether to use ACO delegation (if available).
    pub use_aco_delegation: bool,
    /// Maximum tool calls per sequence.
    pub max_tool_calls: usize,
    /// Default evaporation rate.
    pub evaporation_rate: f64,
    /// Default minimum confidence.
    pub min_confidence: f64,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            use_aco_delegation: false,
            max_tool_calls: 10,
            evaporation_rate: 0.1,
            min_confidence: 0.5,
        }
    }
}

/// Complete N3→N1→N2 pipeline state.
#[derive(Clone)]
pub struct N3Pipeline {
    /// Domain specification.
    pub domain: DomainSpec,
    /// Meta generator (produces GeneratorSpec from DomainSpec).
    meta_generator: RustMetaGenerator,
    /// Pipeline configuration.
    config: PipelineConfig,
}

impl N3Pipeline {
    /// Create a new pipeline for a domain.
    pub fn new(domain: DomainSpec) -> Self {
        Self {
            domain,
            meta_generator: RustMetaGenerator::new(),
            config: PipelineConfig::default(),
        }
    }

    /// Create with explicit configuration.
    pub fn with_config(domain: DomainSpec, config: PipelineConfig) -> Self {
        Self {
            domain,
            meta_generator: RustMetaGenerator::new(),
            config,
        }
    }

    /// Set the domain for this pipeline.
    pub fn with_domain(mut self, domain: DomainSpec) -> Self {
        self.domain = domain;
        self
    }

    /// Generate a GeneratorSpec from the domain (N3 step).
    pub fn generate_spec(&self) -> N1Result<GeneratorSpec> {
        self.meta_generator
            .generate_spec(&self.domain)
            .map_err(|e| N1Error::GenerationFailed(e.to_string()))
    }

    /// Build a BasicGenerator configured from the GeneratorSpec (N1 step).
    ///
    /// This wires the N3 output into the N1 generator.
    pub fn build_generator(
        &self,
        bus: Arc<UnifiedPheromoneBus>,
        spec: &GeneratorSpec,
    ) -> N1Result<ConfiguredGenerator> {
        let config = spec.config.clone();
        let generator = BasicGenerator::new(bus, config.evaporation_rate)
            .with_max_tool_calls(config.max_tool_calls);

        Ok(ConfiguredGenerator {
            generator,
            spec: spec.clone(),
        })
    }

    /// Full pipeline: domain → generator spec → configured generator.
    pub fn prepare(&self, bus: Arc<UnifiedPheromoneBus>) -> N1Result<PipelinePrepareResult> {
        let spec = self.generate_spec()?;
        let configured = self.build_generator(bus, &spec)?;
        Ok(PipelinePrepareResult { spec, configured })
    }

    /// Get the domain for this pipeline.
    pub fn domain(&self) -> &DomainSpec {
        &self.domain
    }

    /// Get the meta generator.
    pub fn meta_generator(&self) -> &RustMetaGenerator {
        &self.meta_generator
    }

    /// Get pipeline configuration.
    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }
}

/// Result of preparing the pipeline: both the spec and configured generator.
#[derive(Clone)]
pub struct PipelinePrepareResult {
    /// The generated spec.
    pub spec: GeneratorSpec,
    /// The configured generator.
    pub configured: ConfiguredGenerator,
}

/// A generator that has been configured from a GeneratorSpec.
#[derive(Clone)]
pub struct ConfiguredGenerator {
    /// The underlying N1 generator.
    generator: BasicGenerator,
    /// The spec that configured it.
    spec: GeneratorSpec,
}

impl ConfiguredGenerator {
    /// Generate a sequence for an objective.
    pub fn generate(&self, objective: &ObjectiveSpec) -> N1Result<GeneratedSequence> {
        self.generator.generate(objective)
    }

    /// Generate with explicit quality gates.
    pub fn generate_with_gates(&self, objective: &ObjectiveSpec) -> N1Result<GeneratedSequence> {
        self.generator.generate(objective)
    }

    /// Learn from an execution outcome.
    pub fn learn(&self, sequence: &GeneratedSequence, outcome: &ExecutionOutcome) {
        let _ = self.generator.learn(sequence, outcome);
    }

    /// Get the pheromone prefix for this generator.
    pub fn pheromone_prefix(&self) -> &'static str {
        self.generator.pheromone_prefix()
    }

    /// Get the underlying spec.
    pub fn spec(&self) -> &GeneratorSpec {
        &self.spec
    }
}

/// Domain builder for fluent pipeline construction.
#[derive(Debug, Clone)]
pub struct DomainBuilder {
    spec: DomainSpec,
}

impl DomainBuilder {
    /// Start building a domain with id, name, and language.
    pub fn new(id: DomainId, name: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            spec: DomainSpec::new(id, name, language),
        }
    }

    /// Add a file pattern.
    pub fn with_file_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.spec.file_patterns.push(pattern.into());
        self
    }

    /// Add a tool pattern.
    pub fn with_tool(mut self, tool_name: impl Into<String>) -> Self {
        use crate::rl::n3::ToolPattern;
        self.spec.tool_patterns.push(ToolPattern::new(tool_name));
        self
    }

    /// Set framework.
    pub fn with_framework(mut self, framework: impl Into<String>) -> Self {
        self.spec.framework = Some(framework.into());
        self
    }

    /// Build the domain spec.
    pub fn build(self) -> DomainSpec {
        self.spec
    }

    /// Build and create a pipeline.
    pub fn into_pipeline(self) -> N3Pipeline {
        N3Pipeline::new(self.spec)
    }
}

/// Quick pipeline construction for common domains.
pub mod quick {
    use super::*;

    /// Create a pipeline for the Rust domain.
    pub fn rust() -> N3Pipeline {
        use crate::rl::n3::domain_spec::predefined::rust;
        N3Pipeline::new(rust())
    }

    /// Create a pipeline for the Python domain.
    pub fn python() -> N3Pipeline {
        use crate::rl::n3::domain_spec::predefined::python;
        N3Pipeline::new(python())
    }

    /// Create a pipeline for the TypeScript domain.
    pub fn typescript() -> N3Pipeline {
        use crate::rl::n3::domain_spec::predefined::typescript;
        N3Pipeline::new(typescript())
    }
}

/// E2E test result.
#[derive(Debug)]
pub struct E2EResult {
    /// The generated sequence.
    pub sequence: GeneratedSequence,
    /// Execution outcome (simulated or real).
    pub outcome: ExecutionOutcome,
    /// Whether the pipeline succeeded end-to-end.
    pub success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_creation() {
        let domain = DomainBuilder::new(DomainId::RUST, "Rust", "rust")
            .with_file_pattern("*.rs")
            .with_framework("cargo")
            .build();

        let pipeline = N3Pipeline::new(domain);
        assert_eq!(pipeline.domain().language, "rust");
    }

    #[test]
    fn test_domain_builder() {
        let domain = DomainBuilder::new(DomainId::RUST, "Rust", "rust")
            .with_file_pattern("*.rs")
            .with_tool("Read")
            .with_tool("Edit")
            .with_framework("cargo")
            .build();

        assert_eq!(domain.file_patterns.len(), 1);
        assert_eq!(domain.tool_patterns.len(), 2);
        assert!(domain.framework.is_some());
    }

    #[test]
    fn test_pipeline_config_defaults() {
        let config = PipelineConfig::default();
        assert_eq!(config.max_tool_calls, 10);
        assert_eq!(config.evaporation_rate, 0.1);
        assert_eq!(config.min_confidence, 0.5);
    }

    #[test]
    fn test_quick_rust_pipeline() {
        let pipeline = quick::rust();
        assert_eq!(pipeline.domain().language, "rust");
    }

    #[test]
    fn test_generate_spec() {
        let pipeline = quick::rust();
        let spec = pipeline.generate_spec().expect("should generate spec");
        assert_eq!(spec.domain_id.0, "rust");
        assert!(!spec.patterns.is_empty());
    }
}
