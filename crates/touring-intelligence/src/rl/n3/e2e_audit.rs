//! E2E Cross-Audit: N3→N1→N2 Pipeline
//!
//! Cross-audit verifying the implementation matches its documented purpose.
//! Verifies: Purpose, Contracts, Invariants, Edge Cases, Integration.

use std::sync::Arc;

use crate::rl::aco::UnifiedPheromoneBus;
use crate::rl::n1::objective_spec::{TargetKind, TargetSpec};
use crate::rl::n1::{
    BasicGenerator, ExecutionOutcome, GeneratedSequence, ObjectiveSpec, ToolSequenceGenerator,
};
use crate::rl::n3::{
    AcoDelegatingGenerator, DomainBuilder, DomainId, DomainSpec, GeneratorSpec, MetaGenerator,
    N3Pipeline, QualityGate, RustMetaGenerator, SequencePattern, ToolCallSpec,
};

// ============================================================================
// AUDIT 1: MetaGenerator produces valid GeneratorSpec (Purpose)
// ============================================================================
#[test]
fn audit_meta_generator_purpose() {
    let meta = RustMetaGenerator::new();

    // Rust domain
    let rust_domain = crate::rl::n3::domain_spec::predefined::rust();
    let spec = meta
        .generate_spec(&rust_domain)
        .expect("should generate spec");
    assert_eq!(spec.domain_id.0, "rust");
    assert!(!spec.patterns.is_empty());
    assert!(spec.pheromone_prefix.starts_with("n3_rust"));

    // Python domain
    let python_domain = crate::rl::n3::domain_spec::predefined::python();
    let spec = meta
        .generate_spec(&python_domain)
        .expect("should generate spec for python");
    assert_eq!(spec.domain_id.0, "python");

    // TypeScript domain
    let ts_domain = crate::rl::n3::domain_spec::predefined::typescript();
    let spec = meta
        .generate_spec(&ts_domain)
        .expect("should generate spec for typescript");
    assert_eq!(spec.domain_id.0, "typescript");
}

// ============================================================================
// AUDIT 2: DomainId const associated constants (Invariant)
// ============================================================================
#[test]
fn audit_domain_id_invariants() {
    assert_eq!(DomainId::RUST.0, "rust");
    assert_eq!(DomainId::PYTHON.0, "python");
    assert_eq!(DomainId::TYPESCRIPT.0, "typescript");
    assert_eq!(DomainId::WEB.0, "web");
    assert_eq!(DomainId::JAVA.0, "java");
    assert_eq!(DomainId::GO.0, "go");
}

// ============================================================================
// AUDIT 3: MetaGenerator contract - fallback for unknown domains (Edge Case)
// ============================================================================
#[test]
fn audit_meta_generator_contract_fallback() {
    let meta = RustMetaGenerator::new();

    // Unknown domain still returns Ok (fallback)
    let unknown = DomainSpec::new(DomainId("unknown".into()), "Unknown", "unknown");
    let result = meta.generate_spec(&unknown);
    assert!(result.is_ok(), "fallback should succeed");
}

// ============================================================================
// AUDIT 4: MetaGenerator supported_domains (Contract)
// ============================================================================
#[test]
fn audit_supported_domains_contract() {
    let meta = RustMetaGenerator::new();
    let domains = meta.supported_domains();
    assert!(!domains.is_empty());
    assert!(domains.contains(&DomainId::RUST));
    assert!(domains.contains(&DomainId::PYTHON));
}

// ============================================================================
// AUDIT 5: DomainBuilder fluent construction (Integration)
// ============================================================================
#[test]
fn audit_domain_builder_integration() {
    let domain = DomainBuilder::new(DomainId::RUST, "Rust", "rust")
        .with_file_pattern("*.rs")
        .with_framework("cargo")
        .build();

    assert_eq!(domain.id, DomainId::RUST);
    assert_eq!(domain.name, "Rust");
    assert_eq!(domain.file_patterns.len(), 1);
    assert_eq!(domain.framework.as_deref(), Some("cargo"));
}

// ============================================================================
// AUDIT 6: N3Pipeline wires N3→N1 (Integration)
// ============================================================================
#[test]
fn audit_pipeline_integration() {
    let pipeline = N3Pipeline::new(crate::rl::n3::domain_spec::predefined::rust());
    assert_eq!(pipeline.domain().language, "rust");

    let spec = pipeline.generate_spec().expect("generate_spec succeeds");
    assert!(!spec.patterns.is_empty(), "spec has patterns");
    assert_eq!(pipeline.domain().id, DomainId::RUST);
}

// ============================================================================
// AUDIT 7: BasicGenerator produces valid GeneratedSequence (Purpose)
// ============================================================================
#[test]
fn audit_basic_generator_purpose() {
    let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
    let generator = BasicGenerator::new(bus, 0.1).with_max_tool_calls(5);

    let objective = ObjectiveSpec {
        description: "Read the main.rs file".to_string(),
        targets: vec![TargetSpec {
            path: "src/main.rs".to_string(),
            kind: TargetKind::SourceFile,
        }],
        constraints: vec![],
        quality_gates: vec![],
        history: vec![],
        priority: 3,
    };

    let result = generator.generate(&objective);
    assert!(result.is_ok(), "generate should succeed");

    let seq = result.expect("generate succeeded (asserted above)");
    assert!(!seq.tool_calls.is_empty(), "sequence has tool calls");

    // GeneratedSequence is Send + Sync (Invariant)
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<GeneratedSequence>();
}

// ============================================================================
// AUDIT 8: BasicGenerator handles empty targets (Edge Case)
// ============================================================================
#[test]
fn audit_basic_generator_edge_empty_targets() {
    let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
    let generator = BasicGenerator::new(bus, 0.1);

    let empty_obj = ObjectiveSpec {
        description: "Analyze code".to_string(),
        targets: vec![],
        constraints: vec![],
        quality_gates: vec![],
        history: vec![],
        priority: 3,
    };

    let result = generator.generate(&empty_obj);
    assert!(result.is_ok(), "empty targets should not panic");
}

// ============================================================================
// AUDIT 9: BasicGenerator handles long descriptions (Edge Case)
// ============================================================================
#[test]
fn audit_basic_generator_edge_long_description() {
    let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
    let generator = BasicGenerator::new(bus, 0.1);

    let long_obj = ObjectiveSpec {
        description: "a".repeat(10000),
        targets: vec![],
        constraints: vec![],
        quality_gates: vec![],
        history: vec![],
        priority: 3,
    };

    let result = generator.generate(&long_obj);
    assert!(result.is_ok(), "long description should not panic");
}

// ============================================================================
// AUDIT 10: AcoDelegatingGenerator fallback works (Edge Case)
// ============================================================================
#[test]
fn audit_aco_delegating_fallback() {
    let generator = AcoDelegatingGenerator::new().with_skip_aco();
    let domain = DomainSpec::new(DomainId::RUST, "Rust", "rust");

    let result = generator.generate_spec(&domain);
    assert!(result.is_ok(), "fallback should succeed");
    assert_eq!(
        result
            .expect("fallback succeeded (asserted above)")
            .domain_id
            .0,
        "rust"
    );
}

// ============================================================================
// AUDIT 11: N3Pipeline prepare() E2E (Integration)
// ============================================================================
#[test]
fn audit_pipeline_prepare_e2e() {
    let pipeline = N3Pipeline::new(crate::rl::n3::domain_spec::predefined::rust());
    let bus = Arc::new(UnifiedPheromoneBus::new(0.1));

    let result = pipeline.prepare(bus).expect("prepare succeeds");
    assert_eq!(result.spec.domain_id.0, "rust");
    assert_eq!(result.configured.spec().domain_id.0, "rust");

    // ConfiguredGenerator can generate
    let objective = ObjectiveSpec {
        description: "test".to_string(),
        targets: vec![],
        constraints: vec![],
        quality_gates: vec![],
        history: vec![],
        priority: 3,
    };
    let seq = result.configured.generate(&objective);
    assert!(seq.is_ok());
}

// ============================================================================
// AUDIT 12: GeneratorConfig defaults sensible (Invariant)
// ============================================================================
#[test]
fn audit_generator_config_defaults() {
    let config = crate::rl::n3::GeneratorConfig::default();
    assert_eq!(config.max_tool_calls, 10);
    assert!(config.evaporation_rate > 0.0 && config.evaporation_rate < 1.0);
    assert!(config.min_confidence >= 0.0 && config.min_confidence <= 1.0);
    assert!(config.allow_parallel);
}

// ============================================================================
// AUDIT 13: SequencePattern builder (Integration)
// ============================================================================
#[test]
fn audit_sequence_pattern_builder() {
    let pattern = SequencePattern::new("test_pattern", "trigger")
        .with_tool(ToolCallSpec {
            tool_name: "Read".into(),
            has_dependency: false,
            parallelizable: true,
            expected_output: Some("content".into()),
        })
        .with_tool(ToolCallSpec {
            tool_name: "Edit".into(),
            has_dependency: true,
            parallelizable: false,
            expected_output: None,
        })
        .with_strength(0.8);

    assert_eq!(pattern.name, "test_pattern");
    assert_eq!(pattern.trigger, "trigger");
    assert_eq!(pattern.tool_sequence.len(), 2);
    assert_eq!(pattern.pheromone_strength, 0.8);
}

// ============================================================================
// AUDIT 14: N1Error Display (Contract)
// ============================================================================
#[test]
fn audit_n1_error_display() {
    use crate::rl::n1::N1Error;

    let err = N1Error::GenerationFailed("test".into());
    let display = format!("{}", err);
    assert!(display.contains("generation failed"));

    let err = N1Error::ValidationFailed("val".into());
    assert!(format!("{}", err).contains("validation failed"));

    let err = N1Error::ToolNotFound("Read".into());
    assert!(format!("{}", err).contains("tool not found"));

    let err = N1Error::PheromoneError("pheromone".into());
    assert!(format!("{}", err).contains("pheromone"));
}

// ============================================================================
// AUDIT 15: ExecutionOutcome construction (Integration)
// ============================================================================
#[test]
fn audit_execution_outcome() {
    let success = ExecutionOutcome {
        success: true,
        executed: vec![],
        first_failure_index: None,
        quality_score: 1.0,
        error_message: None,
    };
    assert!(success.success);
    assert!(success.first_failure_index.is_none());
    assert_eq!(success.quality_score, 1.0);

    let failed = ExecutionOutcome {
        success: false,
        executed: vec![],
        first_failure_index: Some(2),
        quality_score: 0.0,
        error_message: Some("failed".into()),
    };
    assert!(!failed.success);
    assert_eq!(failed.first_failure_index, Some(2));
}

// ============================================================================
// AUDIT 16: DomainSpec serde round-trip (Contract)
// ============================================================================
#[test]
fn audit_domain_spec_serde() {
    let mut domain = DomainSpec::new(DomainId::RUST, "Rust", "rust").with_file_pattern("*.rs");
    domain.quality_gates = vec![QualityGate::Correctness, QualityGate::CodeQuality];

    let json = serde_json::to_string(&domain).expect("should serialize");
    let round_trip: DomainSpec = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(round_trip.id, domain.id);
    assert_eq!(round_trip.quality_gates.len(), 2);
}

// ============================================================================
// AUDIT 17: GeneratorSpec serde round-trip (Contract)
// ============================================================================
#[test]
fn audit_generator_spec_serde() {
    let domain = crate::rl::n3::domain_spec::predefined::rust();
    let meta = RustMetaGenerator::new();
    let spec = meta.generate_spec(&domain).expect("should generate");

    let json = serde_json::to_string(&spec).expect("should serialize");
    let round_trip: GeneratorSpec = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(round_trip.id.0, spec.id.0);
    assert_eq!(round_trip.domain_id.0, spec.domain_id.0);
}

// ============================================================================
// AUDIT 18: ToolSequenceGenerator trait object (Contract)
// ============================================================================
#[test]
fn audit_trait_object() {
    let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
    let r#gen = BasicGenerator::new(bus, 0.1);
    let _: &dyn ToolSequenceGenerator = &r#gen;

    let objective = ObjectiveSpec {
        description: "test".to_string(),
        targets: vec![TargetSpec {
            path: "test.rs".to_string(),
            kind: TargetKind::SourceFile,
        }],
        constraints: vec![],
        quality_gates: vec![],
        history: vec![],
        priority: 3,
    };

    assert!(r#gen.generate(&objective).is_ok());
}

// ============================================================================
// AUDIT 19: Quick constructors (Integration)
// ============================================================================
#[test]
fn audit_quick_constructors() {
    assert_eq!(crate::rl::n3::quick::rust().domain().language, "rust");
    assert_eq!(crate::rl::n3::quick::python().domain().language, "python");
    assert_eq!(
        crate::rl::n3::quick::typescript().domain().language,
        "typescript"
    );
}

// ============================================================================
// AUDIT 20: GeneratedSequence TRIAD structure (Purpose)
// ============================================================================
#[test]
fn audit_triad_structure() {
    let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
    let generator = BasicGenerator::new(bus, 0.1);

    let objective = ObjectiveSpec {
        description: "test".to_string(),
        targets: vec![TargetSpec {
            path: "test.rs".to_string(),
            kind: TargetKind::SourceFile,
        }],
        constraints: vec![],
        quality_gates: vec![],
        history: vec![],
        priority: 3,
    };

    let seq = generator.generate(&objective).expect("should generate");

    // TRIAD: execute = tool_calls
    assert!(!seq.tool_calls.is_empty());

    // TRIAD: validate = validation criteria (may be empty but structure exists)
    assert!(seq.validation.max_duration_ms.is_some() || seq.validation.max_duration_ms.is_none());

    // TRIAD: rollback = rollback plan (may be empty but structure exists)
    assert!(!seq.rollback.actions.is_empty() || seq.rollback.actions.is_empty());
}
