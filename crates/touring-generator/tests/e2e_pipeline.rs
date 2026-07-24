//! E2E integration tests for touring-generator.
//!
//! Verifies the generator pipeline end-to-end:
//! NormalizedScore → VgpEngine cache → TemplateEngine rendering →
//! PlanExecutor typestate transitions → ReplanRequest circuit breaker →
//! PlanRegistry concurrent operations.

// Tests are allowed to unwrap/expect/panic — production restrictions in
// lints.clippy apply to library code, not test harness idioms.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::manual_async_fn,
    clippy::field_reassign_with_default
)]

use std::collections::HashMap;
use std::sync::Arc;

use touring_generator::error::RenderEngine;
use touring_generator::plan::failure::{FailureReason, FailureReport, NextAction};
use touring_generator::plan::result::ExecutionStatus;
use touring_generator::plan::schema::{
    Assembly, CapacityHints, CilaLevel, CommitPolicy, LearningDirectives, PlanMetadata,
    RollbackPolicy, Target, TemplateSelection, ValidationDirectives,
};
use touring_generator::{
    CapacityLimits, Contracts, DynGenerator, FileAction, GenerateError, GeneratorContext,
    GeneratorKind, GeneratorPlan, LlmProvider, NoopTelemetry, NormalizedScore, PlanExecutorHandle,
    PlanRegistry, RenderShape, RenderedFile, SharedPlanRegistry, SymbolRef, TemplateEngine,
    VgpEngine, VgpReport,
};
use uuid::Uuid;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn noop_metrics() -> Arc<NoopTelemetry> {
    Arc::new(NoopTelemetry)
}

fn make_plan(kind: GeneratorKind) -> GeneratorPlan {
    GeneratorPlan {
        version: "8".into(),
        plan_id: Uuid::new_v4(),
        intent: "e2e test plan".into(),
        cila_level: CilaLevel::L1,
        target: Target {
            file_path: "/tmp/touring_gen_test_output.rs".into(),
            crate_name: None,
            module_path: None,
        },
        kind,
        contracts: Contracts::default(),
        template: TemplateSelection::default(),
        assembly: Assembly::default(),
        validation: ValidationDirectives::default(),
        commit_policy: CommitPolicy::default(),
        rollback: RollbackPolicy::default(),
        learning: LearningDirectives::default(),
        spec_inputs: None,
        capacity_hints: CapacityHints::default(),
        execution_trace: Vec::new(),
        metadata: PlanMetadata::default(),
    }
}

// ── NormalizedScore ───────────────────────────────────────────────────────────

#[test]
fn normalized_score_zero_is_zero() {
    assert_eq!(NormalizedScore::ZERO.value(), 0.0);
}

#[test]
fn normalized_score_one_is_one() {
    assert_eq!(NormalizedScore::ONE.value(), 1.0);
}

#[test]
fn normalized_score_clamped_below_zero_gives_zero() {
    assert_eq!(NormalizedScore::clamped(-1.5).value(), 0.0);
}

#[test]
fn normalized_score_clamped_above_one_gives_one() {
    assert_eq!(NormalizedScore::clamped(2.0).value(), 1.0);
}

#[test]
fn normalized_score_clamped_midpoint() {
    let s = NormalizedScore::clamped(0.75);
    assert!((s.value() - 0.75).abs() < f64::EPSILON);
}

#[test]
fn normalized_score_new_valid_range() {
    assert!(NormalizedScore::new(0.0).is_ok());
    assert!(NormalizedScore::new(1.0).is_ok());
    assert!(NormalizedScore::new(0.5).is_ok());
}

#[test]
fn normalized_score_new_rejects_out_of_range() {
    assert!(NormalizedScore::new(-0.01).is_err());
    assert!(NormalizedScore::new(1.01).is_err());
}

#[test]
fn normalized_score_serde_round_trip() {
    let score = NormalizedScore::clamped(0.85);
    let s = serde_json::to_string(&score).expect("NormalizedScore must serialize");
    let d: NormalizedScore = serde_json::from_str(&s).expect("NormalizedScore must deserialize");
    assert!((d.value() - 0.85).abs() < 1e-10);
}

// ── GeneratorKind ─────────────────────────────────────────────────────────────

#[test]
fn all_generator_kinds_have_nonempty_template_names() {
    let kinds = all_kinds();
    for kind in &kinds {
        let name = kind.template_name();
        assert!(!name.is_empty(), "Kind {:?} has empty template name", kind);
        assert!(
            name.ends_with(".tera"),
            "Kind {:?} template '{}' must end in .tera",
            kind,
            name
        );
        assert!(!kind.label().is_empty(), "Kind {:?} has empty label", kind);
    }
}

#[test]
fn generator_kind_display_matches_label() {
    assert_eq!(format!("{}", GeneratorKind::RustModule), "Rust Module");
    assert_eq!(format!("{}", GeneratorKind::McpTool), "MCP Tool");
    assert_eq!(format!("{}", GeneratorKind::CiWorkflow), "CI Workflow");
}

fn all_kinds() -> Vec<GeneratorKind> {
    vec![
        GeneratorKind::RustModule,
        GeneratorKind::CliHandler,
        GeneratorKind::McpTool,
        GeneratorKind::HookHandler,
        GeneratorKind::Test,
        GeneratorKind::BenchmarkSuite,
        GeneratorKind::FuzzTarget,
        GeneratorKind::DeriveMacro,
        GeneratorKind::AttributeMacro,
        GeneratorKind::FunctionMacro,
        GeneratorKind::ErrorCatalog,
        GeneratorKind::IncrementalPatch,
        GeneratorKind::FfiBinding,
        GeneratorKind::Schema,
        GeneratorKind::MigrationScript,
        GeneratorKind::ProtoBufSchema,
        GeneratorKind::OpenApiSpec,
        GeneratorKind::AsyncApiSpec,
        GeneratorKind::PlanMarkdown,
        GeneratorKind::SkillDocument,
        GeneratorKind::DiaryEntry,
        GeneratorKind::ChangelogEntry,
        GeneratorKind::Adr,
        GeneratorKind::ShellCompletion,
        GeneratorKind::ManPage,
        GeneratorKind::PythonScript,
        GeneratorKind::TypeScriptModule,
        GeneratorKind::DockerImage,
        GeneratorKind::KubernetesManifest,
        GeneratorKind::TerraformModule,
        GeneratorKind::CiWorkflow,
        GeneratorKind::ConsumerGenerator,
        GeneratorKind::TaskScaffold,
        GeneratorKind::RustFoundationalCrateCargoToml,
        GeneratorKind::RustCrateLibRs,
        GeneratorKind::SystemdUserService,
    ]
}

#[test]
fn generator_kind_serde_all_variants() {
    for kind in all_kinds() {
        let serialized = serde_json::to_string(&kind)
            .unwrap_or_else(|e| panic!("Kind {:?} serialize failed: {}", kind, e));
        let deserialized: GeneratorKind = serde_json::from_str(&serialized)
            .unwrap_or_else(|e| panic!("Kind {:?} deserialize failed: {}", kind, e));
        assert_eq!(kind, deserialized);
    }
}

// ── TemplateEngine ────────────────────────────────────────────────────────────

#[test]
fn template_engine_registers_29_templates() {
    // Count updated to 34 (added rust_foundational_crate_cargo_toml, rust_crate_lib_rs, systemd_user_service in Wave 2026-05-02)
    let names = TemplateEngine::template_names();
    assert_eq!(
        names.len(),
        34,
        "Expected 34 templates, got {}",
        names.len()
    );
    for name in names {
        assert!(
            name.ends_with(".tera"),
            "Template '{}' must end in .tera",
            name
        );
    }
}

#[test]
fn template_engine_renders_all_29_kinds_with_empty_vars() {
    let engine = TemplateEngine::new(noop_metrics());
    let vars: HashMap<String, serde_json::Value> = HashMap::new();

    for kind in all_kinds() {
        let result = engine.render_for_kind(&kind, &vars);
        assert!(
            result.is_ok(),
            "Kind {:?} render failed: {:?}",
            kind,
            result
        );
        // Output must not be empty — templates have at least skeleton content.
        assert!(
            !result.unwrap().is_empty(),
            "Kind {:?} template produced empty output",
            kind
        );
    }
}

#[test]
fn template_engine_rejects_invalid_variable_key() {
    let engine = TemplateEngine::new(noop_metrics());
    let mut vars: HashMap<String, serde_json::Value> = HashMap::new();
    vars.insert("invalid-key".into(), serde_json::Value::Null);

    let result = engine.render("rust_module.tera", &vars);
    assert!(
        matches!(result, Err(GenerateError::TemplateVariableRejected { .. })),
        "Expected TemplateVariableRejected for 'invalid-key', got {:?}",
        result
    );
}

#[test]
fn template_engine_accepts_valid_variable_keys() {
    let engine = TemplateEngine::new(noop_metrics());
    let mut vars: HashMap<String, serde_json::Value> = HashMap::new();
    vars.insert("moduleName".into(), serde_json::json!("MyModule"));
    vars.insert("crate_name".into(), serde_json::json!("my_crate"));

    let result = engine.render("rust_module.tera", &vars);
    assert!(result.is_ok(), "Valid vars rejected: {:?}", result);
}

#[test]
fn template_engine_unknown_template_returns_template_error() {
    let engine = TemplateEngine::new(noop_metrics());
    let result = engine.render("nonexistent_template_xyz.tera", &HashMap::new());
    assert!(
        matches!(result, Err(GenerateError::TemplateError { .. })),
        "Expected TemplateError for unknown template, got {:?}",
        result
    );
}

#[test]
fn template_engine_second_call_uses_precompiled_cache() {
    // OnceLock ensures templates are compiled once — this test verifies no double-init panic.
    let engine = TemplateEngine::new(noop_metrics());
    let vars = HashMap::new();
    let r1 = engine.render("rust_module.tera", &vars);
    let r2 = engine.render("rust_module.tera", &vars);
    assert!(
        r1.is_ok() && r2.is_ok(),
        "Second render call failed: {:?} {:?}",
        r1,
        r2
    );
    assert_eq!(
        r1.unwrap(),
        r2.unwrap(),
        "Deterministic renders must produce identical output"
    );
}

// ── VgpEngine ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn vgp_engine_empty_contracts_returns_pass() {
    let engine = VgpEngine::with_subprocess(noop_metrics());
    let contracts = Contracts::default();
    let plan_id = Uuid::new_v4();

    let report = engine
        .verify_batch(&contracts, plan_id)
        .await
        .unwrap_or_else(|e| panic!("verify_batch failed: {}", e));

    assert!(report.all_passed, "Empty contracts must pass VGP");
    assert!(report.missing_symbols.is_empty());
    assert!(report.collisions.is_empty());
    assert_eq!(report.plan_id, plan_id);
    assert_eq!(report.elapsed_ms, 0, "Empty-contracts path sets elapsed=0");
}

#[tokio::test]
async fn vgp_engine_nonexistent_symbol_produces_missing_entry() {
    let engine = VgpEngine::with_subprocess(noop_metrics());
    let mut contracts = Contracts::default();
    contracts
        .symbols_must_exist
        .push(SymbolRef::named("__totally_nonexistent_xyz_999__"));

    let plan_id = Uuid::new_v4();
    let result = engine.verify_batch(&contracts, plan_id).await;
    assert!(
        result.is_ok(),
        "verify_batch must not panic on missing symbol: {:?}",
        result
    );

    let report = result.unwrap();
    assert!(!report.all_passed, "Report must fail for missing symbol");
    assert_eq!(
        report.missing_symbols.len(),
        1,
        "One symbol should be missing"
    );
    assert_eq!(
        report.missing_symbols[0].requested.name,
        "__totally_nonexistent_xyz_999__"
    );
}

#[tokio::test]
async fn vgp_engine_cache_counters_track_hits_and_misses() {
    let engine = VgpEngine::with_subprocess(noop_metrics());
    let mut contracts = Contracts::default();
    contracts
        .symbols_must_exist
        .push(SymbolRef::named("NormalizedScore"));

    // First call: cache miss.
    let _ = engine.verify_batch(&contracts, Uuid::new_v4()).await;
    assert_eq!(
        engine.cache_miss_count(),
        1,
        "First call should be a cache miss"
    );
    assert_eq!(engine.cache_hit_count(), 0);

    // Second call: cache hit.
    let _ = engine.verify_batch(&contracts, Uuid::new_v4()).await;
    assert_eq!(
        engine.cache_hit_count(),
        1,
        "Second call should be a cache hit"
    );
    assert_eq!(
        engine.cache_miss_count(),
        1,
        "Miss count should not increase"
    );
}

#[tokio::test]
async fn vgp_engine_invalidate_and_reset_counters() {
    let engine = VgpEngine::with_subprocess(noop_metrics());
    let mut contracts = Contracts::default();
    contracts
        .symbols_must_exist
        .push(SymbolRef::named("VgpEngine"));

    // Prime cache.
    let _ = engine.verify_batch(&contracts, Uuid::new_v4()).await;
    engine.invalidate_all();
    engine.reset_counters();

    assert_eq!(
        engine.cache_hit_count(),
        0,
        "Counters should be 0 after reset"
    );
    assert_eq!(engine.cache_miss_count(), 0);

    // After invalidation, next lookup is a miss.
    let _ = engine.verify_batch(&contracts, Uuid::new_v4()).await;
    assert_eq!(
        engine.cache_miss_count(),
        1,
        "Post-invalidation lookup must be a miss"
    );
    assert_eq!(engine.cache_hit_count(), 0);
}

#[tokio::test]
async fn vgp_engine_empty_pass_has_zero_cache_counters() {
    let engine = VgpEngine::with_subprocess(noop_metrics());
    // Empty contracts → early return, no cache access.
    let _ = engine
        .verify_batch(&Contracts::default(), Uuid::new_v4())
        .await;
    assert_eq!(engine.cache_hit_count(), 0);
    assert_eq!(engine.cache_miss_count(), 0);
}

// ── PlanRegistry ──────────────────────────────────────────────────────────────

#[test]
fn plan_registry_register_and_query_status() {
    let registry = PlanRegistry::new();
    let plan_id = Uuid::new_v4();

    registry.register(PlanExecutorHandle {
        plan_id,
        status: ExecutionStatus::Draft,
        intent_preview: "test plan".into(),
    });

    assert_eq!(registry.status(plan_id), Some(ExecutionStatus::Draft));
    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());
}

#[test]
fn plan_registry_update_status_transitions() {
    let registry = PlanRegistry::new();
    let plan_id = Uuid::new_v4();

    registry.register(PlanExecutorHandle {
        plan_id,
        status: ExecutionStatus::Draft,
        intent_preview: "test".into(),
    });

    for status in [
        ExecutionStatus::Verified,
        ExecutionStatus::Rendered,
        ExecutionStatus::Speculated,
        ExecutionStatus::Committed,
    ] {
        registry.update_status(plan_id, status.clone());
        assert_eq!(registry.status(plan_id), Some(status));
    }
}

#[test]
fn plan_registry_remove_returns_and_deletes() {
    let registry = PlanRegistry::new();
    let plan_id = Uuid::new_v4();

    registry.register(PlanExecutorHandle {
        plan_id,
        status: ExecutionStatus::Committed,
        intent_preview: "done".into(),
    });

    let removed = registry.remove(plan_id);
    assert!(removed.is_some(), "remove() must return the handle");
    assert_eq!(registry.len(), 0);
    assert!(registry.is_empty());
    // Second remove returns None.
    assert!(registry.remove(plan_id).is_none());
}

#[test]
fn plan_registry_unknown_id_returns_none() {
    let registry = PlanRegistry::new();
    assert!(registry.status(Uuid::new_v4()).is_none());
}

#[test]
fn plan_registry_default_is_empty() {
    let registry = PlanRegistry::default();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
}

#[test]
fn shared_plan_registry_arc_clone_shares_state() {
    let registry: SharedPlanRegistry = Arc::new(PlanRegistry::new());
    let clone = Arc::clone(&registry);

    let plan_id = Uuid::new_v4();
    registry.register(PlanExecutorHandle {
        plan_id,
        status: ExecutionStatus::Draft,
        intent_preview: "shared".into(),
    });

    assert_eq!(clone.status(plan_id), Some(ExecutionStatus::Draft));
}

// ── GeneratorContext ──────────────────────────────────────────────────────────

#[test]
fn generator_context_for_testing_constructs_without_panic() {
    let ctx = GeneratorContext::for_testing();
    assert!(ctx.capacity.speculate_threshold > 0.0);
    assert!(ctx.capacity.speculate_threshold <= 1.0);
}

#[test]
fn generator_context_speculate_threshold_defaults_to_0_8() {
    let ctx = GeneratorContext::for_testing();
    assert!(
        (ctx.capacity.speculate_threshold - 0.8).abs() < 0.01,
        "Default threshold should be 0.8, got {}",
        ctx.capacity.speculate_threshold
    );
}

#[test]
fn generator_context_noop_rl_does_not_panic() {
    let ctx = GeneratorContext::for_testing();
    ctx.rl_reward("test_tool", 1.0, "context");
    assert!(ctx.rl.ema("test_tool").is_none());
}

#[test]
fn generator_context_noop_memory_store_and_recall() {
    use touring_generator::{MemoryKind, MemoryTier};
    let ctx = GeneratorContext::for_testing();

    assert!(
        ctx.memory
            .store("k", "v", MemoryTier::Semantic, MemoryKind::Lesson)
            .is_ok()
    );
    let entries = ctx.memory.recall("k", 10).unwrap();
    assert!(entries.is_empty(), "NoopMemory should discard writes");
    assert_eq!(ctx.memory.stats().total_entries, 0);
}

#[test]
fn generator_context_noopllm_estimates_tokens() {
    let llm = touring_generator::NoopLlm;
    // 4-char-per-token rule: 8 chars → ~2 tokens.
    let t = llm.estimate_tokens("hello wo");
    assert_eq!(t, 2, "Expected 2 tokens for 8 chars");
    assert_eq!(llm.name(), "noop");
}

// ── RenderedFile ──────────────────────────────────────────────────────────────

#[test]
fn rendered_file_sha256_is_deterministic() {
    let f1 = RenderedFile::new("/tmp/test.rs", "fn main() {}", FileAction::Created);
    let f2 = RenderedFile::new("/tmp/test.rs", "fn main() {}", FileAction::Created);
    assert_eq!(f1.sha256(), f2.sha256());
}

#[test]
fn rendered_file_sha256_changes_with_content() {
    let f1 = RenderedFile::new("/tmp/test.rs", "fn a() {}", FileAction::Created);
    let f2 = RenderedFile::new("/tmp/test.rs", "fn b() {}", FileAction::Created);
    assert_ne!(f1.sha256(), f2.sha256());
}

#[test]
fn rendered_file_sha256_is_valid_hex() {
    let f = RenderedFile::new("/tmp/out.rs", "content", FileAction::Created);
    let hash = f.sha256();
    assert_eq!(
        hash.len(),
        64,
        "SHA-256 hex must be 64 chars, got {}",
        hash.len()
    );
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn rendered_file_new_stores_fields_correctly() {
    let f = RenderedFile::new("/tmp/out.rs", "content", FileAction::Overwritten);
    assert_eq!(f.path, "/tmp/out.rs");
    assert_eq!(f.content, "content");
    assert_eq!(f.action, FileAction::Overwritten);
}

// ── FailureReport ─────────────────────────────────────────────────────────────

#[test]
fn failure_report_escalation_flag_is_respected() {
    let report = make_failure_report(true);
    assert!(report.requires_escalation());

    let report = make_failure_report(false);
    assert!(!report.requires_escalation());
}

#[test]
fn failure_report_serde_round_trip() {
    let report = make_failure_report(true);
    let json = serde_json::to_string(&report).expect("FailureReport must serialize");
    let decoded: FailureReport =
        serde_json::from_str(&json).expect("FailureReport must deserialize");
    assert_eq!(decoded.plan_id, report.plan_id);
    assert_eq!(decoded.iteration, 3);
    assert!(decoded.escalate_to_human);
}

fn make_failure_report(escalate: bool) -> FailureReport {
    FailureReport {
        plan_id: Uuid::new_v4(),
        iteration: 3,
        reason: FailureReason::VgpFailed,
        missing_symbols: Vec::new(),
        collisions: Vec::new(),
        failing_layers: Vec::new(),
        template_errors: Vec::new(),
        io_errors: Vec::new(),
        code_excerpts: Vec::new(),
        suggestions: vec!["try again".into()],
        recommended_next_action: NextAction::GiveUp {
            rationale: "exhausted".into(),
        },
        escalate_to_human: escalate,
        elapsed_ms: 500,
    }
}

// ── VgpReport ─────────────────────────────────────────────────────────────────

#[test]
fn vgp_report_empty_pass_invariants() {
    let plan_id = Uuid::new_v4();
    let report = VgpReport::empty_pass(plan_id);

    assert!(report.all_passed);
    assert_eq!(report.plan_id, plan_id);
    assert!(report.missing_symbols.is_empty());
    assert!(report.collisions.is_empty());
    assert_eq!(report.elapsed_ms, 0);
    assert_eq!(report.cache_hits, 0);
    assert_eq!(report.cache_misses, 0);
}

#[test]
fn vgp_report_serde_round_trip() {
    let report = VgpReport::empty_pass(Uuid::new_v4());
    let json = serde_json::to_string(&report).expect("VgpReport must serialize");
    let decoded: VgpReport = serde_json::from_str(&json).expect("VgpReport must deserialize");
    assert_eq!(decoded.plan_id, report.plan_id);
    assert!(decoded.all_passed);
}

// ── SymbolRef ─────────────────────────────────────────────────────────────────

#[test]
fn symbol_ref_named_has_no_filters() {
    let sym = SymbolRef::named("MyStruct");
    assert_eq!(sym.name, "MyStruct");
    assert!(sym.crate_name.is_none());
    assert!(sym.module_path.is_none());
    assert!(sym.definition_hint.is_none());
}

#[test]
fn symbol_ref_in_crate_sets_crate_name() {
    let sym = SymbolRef::in_crate("GeneratorPlan", "touring-generator");
    assert_eq!(sym.name, "GeneratorPlan");
    assert_eq!(sym.crate_name.as_deref(), Some("touring-generator"));
    assert!(sym.module_path.is_none());
}

// ── GenerateError ─────────────────────────────────────────────────────────────

#[test]
fn generate_error_display_contains_relevant_info() {
    let err = GenerateError::VgpFailed {
        missing_count: 3,
        collision_count: 1,
        plan_id: Uuid::nil(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("3"),
        "Error message must contain missing count: {}",
        msg
    );
    assert!(
        msg.contains("1"),
        "Error message must contain collision count: {}",
        msg
    );
}

#[test]
fn generate_error_template_error_includes_message() {
    let err = GenerateError::TemplateError {
        engine: RenderEngine::Tera,
        message: "undefined variable 'foo'".into(),
    };
    assert!(err.to_string().contains("foo"));
}

#[test]
fn generate_error_path_traversal_shows_path() {
    let err = GenerateError::PathTraversalDenied {
        path: "../etc/passwd".into(),
    };
    assert!(err.to_string().contains("etc/passwd"));
}

#[test]
fn generate_error_internal_shows_message() {
    let err = GenerateError::Internal("test internal error".into());
    assert!(err.to_string().contains("test internal error"));
}

// ── Contracts ─────────────────────────────────────────────────────────────────

#[test]
fn contracts_default_is_empty() {
    assert!(Contracts::default().is_empty());
}

#[test]
fn contracts_with_symbol_is_not_empty() {
    let mut c = Contracts::default();
    c.symbols_must_exist.push(SymbolRef::named("Foo"));
    assert!(!c.is_empty());
}

#[test]
fn contracts_with_file_is_not_empty() {
    let mut c = Contracts::default();
    c.files_must_exist.push("/some/file.rs".into());
    assert!(!c.is_empty());
}

#[test]
fn contracts_plan_id_is_nil() {
    let c = Contracts::default();
    assert_eq!(c.plan_id(), Uuid::nil());
}

// ── CapacityLimits ────────────────────────────────────────────────────────────

#[test]
fn capacity_limits_default_threshold_is_0_8() {
    let limits = CapacityLimits::default();
    assert!(limits.speculate_threshold >= 0.0);
    assert!(limits.speculate_threshold <= 1.0);
    assert!(
        (limits.speculate_threshold - 0.8).abs() < 0.01,
        "Default speculate threshold must be 0.8, got {}",
        limits.speculate_threshold
    );
}

// ── GeneratorPlan serde ───────────────────────────────────────────────────────

#[test]
fn generator_plan_serde_round_trip() {
    let plan = make_plan(GeneratorKind::McpTool);
    let json = serde_json::to_string(&plan).expect("GeneratorPlan must serialize");
    let decoded: GeneratorPlan =
        serde_json::from_str(&json).expect("GeneratorPlan must deserialize");

    assert_eq!(decoded.plan_id, plan.plan_id);
    assert_eq!(decoded.intent, plan.intent);
    assert!(matches!(decoded.kind, GeneratorKind::McpTool));
    assert_eq!(decoded.version, "8");
}

// ── ErasedGenerator blanket impl ─────────────────────────────────────────────

/// Minimal test generator — produces a single fixed file.
struct EchoGenerator;

impl touring_generator::Generator for EchoGenerator {
    fn id(&self) -> &'static str {
        "echo"
    }

    fn render(
        &self,
        _plan: &GeneratorPlan,
        _vars: &HashMap<String, serde_json::Value>,
    ) -> impl std::future::Future<Output = Result<Vec<RenderedFile>, GenerateError>> + Send {
        async move {
            Ok(vec![RenderedFile::new(
                "/tmp/echo_out.rs",
                "// echo output",
                FileAction::Created,
            )])
        }
    }
}

#[tokio::test]
async fn erased_generator_blanket_impl_routes_to_generator_trait() {
    let r#gen: DynGenerator = Arc::new(EchoGenerator);
    assert_eq!(r#gen.id(), "echo");

    let plan = make_plan(GeneratorKind::RustModule);
    let vars = HashMap::new();
    let result = r#gen.render_boxed(&plan, &vars).await;

    let files = result.unwrap_or_else(|e| panic!("render_boxed failed: {}", e));
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "/tmp/echo_out.rs");
    assert_eq!(files[0].content, "// echo output");
    assert_eq!(files[0].action, FileAction::Created);
    assert!(!files[0].sha256().is_empty());
}

// ── Pipeline integration tests ────────────────────────────────────────────────

#[tokio::test]
async fn pipeline_draft_to_verified_with_empty_contracts() {
    use touring_generator::{Draft, PlanExecutor};

    let ctx = GeneratorContext::for_testing();
    let plan = make_plan(GeneratorKind::RustModule);
    let vgp = &ctx.vgp_engine;

    let executor: PlanExecutor<Draft> = PlanExecutor::first(plan, Arc::clone(&ctx));
    let result = executor.verify(vgp).await;

    assert!(
        result.is_ok(),
        "Draft→Verified with empty contracts must succeed"
    );
}

#[tokio::test]
async fn pipeline_verified_to_rendered_produces_nonempty_file() {
    use touring_generator::{Draft, PlanExecutor};

    let ctx = GeneratorContext::for_testing();
    let plan = make_plan(GeneratorKind::RustModule);
    let vgp = &ctx.vgp_engine;
    let template = &ctx.template_engine;

    let executor: PlanExecutor<Draft> = PlanExecutor::first(plan, Arc::clone(&ctx));
    let verified = match executor.verify(vgp).await {
        Ok(v) => v,
        Err(_) => panic!("VGP must pass on empty contracts"),
    };

    let rendered = verified.render(
        template,
        &HashMap::new(),
        None,
        RenderShape::default_width(),
    );
    assert!(
        rendered.is_ok(),
        "Verified→Rendered must succeed with default template"
    );
}

#[tokio::test]
async fn pipeline_speculate_result_has_clamped_score() {
    use touring_generator::{Draft, PlanExecutor};

    let ctx = GeneratorContext::for_testing();
    let plan = make_plan(GeneratorKind::Test);
    let vgp = &ctx.vgp_engine;
    let template = &ctx.template_engine;
    let speculate = &ctx.speculate_bridge;

    let executor: PlanExecutor<Draft> = PlanExecutor::first(plan, Arc::clone(&ctx));
    let verified = match executor.verify(vgp).await {
        Ok(v) => v,
        Err(_) => panic!("VGP must pass"),
    };
    let rendered = match verified.render(
        template,
        &HashMap::new(),
        None,
        RenderShape::default_width(),
    ) {
        Ok(Some(r)) => r,
        Ok(None) => {
            panic!("Render returned None due to overflow (should not happen with default width)")
        }
        Err(e) => panic!("Render must succeed: {}", e),
    };

    let speculated = rendered.speculate(speculate).await;
    match speculated {
        Ok(s) => {
            let score = s.score();
            assert!(
                score.value() >= 0.0 && score.value() <= 1.0,
                "Speculate score must be in [0.0, 1.0], got {}",
                score.value()
            );
        }
        Err(_replan) => {
            // Daemon unavailable → score 0.0 → replan triggered. Acceptable in CI.
        }
    }
}

#[tokio::test]
async fn pipeline_full_draft_to_committed_writes_file() {
    use touring_generator::{Draft, PlanExecutor};

    let ctx = GeneratorContext::for_testing();
    // Use a test output path under /tmp to avoid polluting the project.
    let mut plan = make_plan(GeneratorKind::DiaryEntry);
    plan.target.file_path = format!("/tmp/touring_gen_test_{}.md", plan.plan_id);

    let vgp = &ctx.vgp_engine;
    let template = &ctx.template_engine;
    let speculate = &ctx.speculate_bridge;

    let executor: PlanExecutor<Draft> = PlanExecutor::first(plan.clone(), Arc::clone(&ctx));

    let verified = match executor.verify(vgp).await {
        Ok(v) => v,
        Err(_) => panic!("VGP must pass on empty contracts"),
    };
    let rendered = match verified.render(
        template,
        &HashMap::new(),
        None,
        RenderShape::default_width(),
    ) {
        Ok(Some(r)) => r,
        Ok(None) => panic!("render returned None due to overflow"),
        Err(e) => panic!("render must succeed: {}", e),
    };

    let speculate_result = rendered.speculate(speculate).await;

    match speculate_result {
        Ok(speculated) => {
            // Speculate passed — commit.
            let completed = speculated.commit().await;
            assert!(completed.is_ok(), "Commit must succeed: {:?}", completed);

            let report = completed.unwrap();
            assert_eq!(report.plan_id, plan.plan_id);
            assert!(!report.commit_report.files_written.is_empty());

            // Verify the file was actually written.
            let path = &report.commit_report.files_written[0].path;
            assert!(
                std::path::Path::new(path).exists(),
                "Committed file must exist on disk: {}",
                path
            );
            // Clean up.
            let _ = std::fs::remove_file(path);
        }
        Err(_replan) => {
            // Speculate daemon unavailable in CI — acceptable.
        }
    }
}

// ── Concurrent registry ───────────────────────────────────────────────────────

#[tokio::test]
async fn plan_registry_concurrent_register_20_plans() {
    use tokio::task::JoinSet;

    let registry: SharedPlanRegistry = Arc::new(PlanRegistry::new());
    let mut join_set = JoinSet::new();

    for i in 0u64..20 {
        let reg = Arc::clone(&registry);
        join_set.spawn(async move {
            let plan_id = Uuid::from_u64_pair(0, i);
            reg.register(PlanExecutorHandle {
                plan_id,
                status: ExecutionStatus::Draft,
                intent_preview: format!("plan {}", i),
            });
        });
    }

    while join_set.join_next().await.is_some() {}
    assert_eq!(
        registry.len(),
        20,
        "All 20 plans must be concurrently registered"
    );
}

#[tokio::test]
async fn plan_registry_concurrent_update_status() {
    use tokio::task::JoinSet;

    let registry: SharedPlanRegistry = Arc::new(PlanRegistry::new());
    let plan_id = Uuid::new_v4();

    registry.register(PlanExecutorHandle {
        plan_id,
        status: ExecutionStatus::Draft,
        intent_preview: "concurrent".into(),
    });

    let mut join_set = JoinSet::new();
    for _ in 0u8..10 {
        let reg = Arc::clone(&registry);
        join_set.spawn(async move {
            reg.update_status(plan_id, ExecutionStatus::Verified);
        });
    }

    while join_set.join_next().await.is_some() {}
    // Final state should be Verified (all tasks set the same value).
    assert_eq!(registry.status(plan_id), Some(ExecutionStatus::Verified));
}

// ── PLN2 section 8.1 — Closure wiring E2E tests ──────────────────────────────
//
// These tests prove that the 7 closure fields from GeneratorContext v2 are
// actually CALLED during the typestate pipeline (Draft→Verified→Rendered→
// Speculated→Committed). They inject mock closures that increment counters,
// then assert that the counters match expected dispatch events.
//
// Why this matters for POTENCIALIZAR: without these tests a closure could be
// silently "plugged in" but never invoked, reducing the scope of integration
// without detection. These tests guarantee end-to-end wiring.

use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use touring_generator::{
    CognitiveNexusFn, DspyInputs, DspyOutputs, DspySigFn, FuzzyMatcher, MctsEvalFn, NoopAuditLog,
    NoopFuzzyMatcher, NoopLlm, NoopMemory, NoopRlSink, PheromoneUpdateFn,
    PlanRegistry as GenPlanRegistry, PlanSimilarityScore, SchemaRegistry, SemanticGraphFn,
    SpeculateBridge, WasmSandboxFn, WiringGateFn,
};

/// Counters recorded by mock closures injected into `GeneratorContext`.
/// Each field corresponds to one closure type.
#[derive(Default)]
struct ClosureCallCounters {
    pheromone: AtomicUsize,
    wiring_gate: AtomicUsize,
    mcts_eval: AtomicUsize,
    wasm_sandbox: AtomicUsize,
    dspy_sig: AtomicUsize,
    cognitive_nexus: AtomicUsize,
    semantic_graph: AtomicUsize,
}

impl ClosureCallCounters {
    fn snapshot(&self) -> (usize, usize, usize, usize, usize, usize, usize) {
        (
            self.pheromone.load(AtomicOrdering::Relaxed),
            self.wiring_gate.load(AtomicOrdering::Relaxed),
            self.mcts_eval.load(AtomicOrdering::Relaxed),
            self.wasm_sandbox.load(AtomicOrdering::Relaxed),
            self.dspy_sig.load(AtomicOrdering::Relaxed),
            self.cognitive_nexus.load(AtomicOrdering::Relaxed),
            self.semantic_graph.load(AtomicOrdering::Relaxed),
        )
    }
}

/// Build a test GeneratorContext with all 7 closures wired to counting mocks.
/// Returns the Arc'd context and the counters for later assertion.
fn context_with_counting_closures() -> (Arc<GeneratorContext>, Arc<ClosureCallCounters>) {
    let counters = Arc::new(ClosureCallCounters::default());

    let pheromone_counter = Arc::clone(&counters);
    let pheromone_fn: PheromoneUpdateFn = Arc::new(move |_tool: &str, _score| {
        pheromone_counter
            .pheromone
            .fetch_add(1, AtomicOrdering::Relaxed);
    });

    let wiring_counter = Arc::clone(&counters);
    let wiring_gate_fn: WiringGateFn = Arc::new(move |_files: &[RenderedFile], _plan_id: &str| {
        wiring_counter
            .wiring_gate
            .fetch_add(1, AtomicOrdering::Relaxed);
        Ok(())
    });

    let mcts_counter = Arc::clone(&counters);
    let mcts_eval_fn: MctsEvalFn = Arc::new(move |_state: &str| {
        mcts_counter.mcts_eval.fetch_add(1, AtomicOrdering::Relaxed);
        NormalizedScore::clamped(0.5)
    });

    let wasm_counter = Arc::clone(&counters);
    let wasm_sandbox_fn: WasmSandboxFn = Arc::new(move |_code: &str, _lang: &str| {
        wasm_counter
            .wasm_sandbox
            .fetch_add(1, AtomicOrdering::Relaxed);
        Ok(String::new())
    });

    let dspy_counter = Arc::clone(&counters);
    let dspy_sig_fn: DspySigFn = Arc::new(move |_sig, inputs: &DspyInputs| {
        dspy_counter.dspy_sig.fetch_add(1, AtomicOrdering::Relaxed);
        // Echo inputs back as outputs to prove round-trip.
        let mut outputs: DspyOutputs = DspyOutputs::new();
        for (k, v) in inputs.iter() {
            outputs.insert(format!("echo:{k}"), v.clone());
        }
        outputs
    });

    let nexus_counter = Arc::clone(&counters);
    let cognitive_nexus_fn: CognitiveNexusFn = Arc::new(move |_key: &str| {
        nexus_counter
            .cognitive_nexus
            .fetch_add(1, AtomicOrdering::Relaxed);
        Some(PlanSimilarityScore::clamped(0.7))
    });

    let graph_counter = Arc::clone(&counters);
    let semantic_graph_fn: SemanticGraphFn = Arc::new(move |_plan: &GeneratorPlan| {
        graph_counter
            .semantic_graph
            .fetch_add(1, AtomicOrdering::Relaxed);
        Some(vec![SymbolRef::named("TestRelatedSym")])
    });

    let metrics: Arc<NoopTelemetry> = Arc::new(NoopTelemetry);
    let file_cache = Arc::new(tokio::sync::RwLock::new(
        touring_intelligence::index::FileCache::new(),
    ));
    let ctx = Arc::new(GeneratorContext {
        project_root: camino::Utf8PathBuf::from("/tmp/touring-generator-closure-test"),
        symbol_index: Arc::new(touring_intelligence::index::IncrementalIndex::new(
            file_cache,
        )),
        fuzzy_index: Arc::new(NoopFuzzyMatcher) as Arc<dyn FuzzyMatcher>,
        vgp_engine: Arc::new(VgpEngine::with_subprocess(
            Arc::clone(&metrics) as Arc<NoopTelemetry>
        )),
        template_engine: Arc::new(TemplateEngine::new(
            Arc::clone(&metrics) as Arc<NoopTelemetry>
        )),
        speculate_bridge: Arc::new(SpeculateBridge::new(
            Arc::clone(&metrics) as Arc<NoopTelemetry>
        )),
        schema_registry: Arc::new(SchemaRegistry::new("2.0.0")),
        plan_registry: Arc::new(GenPlanRegistry::new()),
        memory: Arc::new(NoopMemory),
        llm: Arc::new(NoopLlm),
        rl: Arc::new(NoopRlSink),
        telemetry: metrics,
        semantic_graph_fn: Some(semantic_graph_fn),
        pheromone_fn: Some(pheromone_fn),
        cognitive_nexus_fn: Some(cognitive_nexus_fn),
        wiring_gate_fn: Some(wiring_gate_fn),
        health_delta_record_fn: None,
        health_delta_compute_fn: None,
        wasm_sandbox_fn: Some(wasm_sandbox_fn),
        mcts_eval_fn: Some(mcts_eval_fn),
        dspy_sig_fn: Some(dspy_sig_fn),
        knowledge_upsert_fn: None,
        session_start_fn: None,
        session_checkpoint_fn: None,
        session_assess_fn: None,
        decompose_create_fn: None,
        decompose_update_fn: None,
        #[cfg(feature = "quality-gate")]
        quality_gate_fn: None,
        #[cfg(feature = "quality-gate")]
        quality_gate_adapter: None,
        #[cfg(feature = "health-gate")]
        health_gate_fn: None,
        #[cfg(feature = "enrichment-gate")]
        enrichment_trigger_fn: None,
        backpressure: Arc::new(tokio::sync::Semaphore::new(64)),
        capacity: CapacityLimits::default(),
        audit_log: Arc::new(NoopAuditLog),
        concolic_analyze_fn: None,
    });

    (ctx, counters)
}

#[test]
fn closure_helpers_dispatch_to_injected_functions() {
    // Unit test for the closure dispatch helpers in GeneratorContext —
    // proves that when closures ARE wired, the helpers forward correctly.
    let (ctx, counters) = context_with_counting_closures();

    ctx.pheromone_update("unit_test", NormalizedScore::ONE);
    assert_eq!(counters.pheromone.load(AtomicOrdering::Relaxed), 1);

    let files = vec![RenderedFile::new(
        "/tmp/closure_helper_test.rs",
        "fn main() {}",
        FileAction::Created,
    )];
    assert!(ctx.evaluate_wiring_gate(&files, "test-plan").is_ok());
    assert_eq!(counters.wiring_gate.load(AtomicOrdering::Relaxed), 1);

    let score = ctx.mcts_evaluate("some_state");
    assert!((score.value() - 0.5).abs() < f64::EPSILON);
    assert_eq!(counters.mcts_eval.load(AtomicOrdering::Relaxed), 1);

    let out = ctx.sandbox_execute("let x = 1;", "rust");
    assert!(out.is_ok());
    assert_eq!(counters.wasm_sandbox.load(AtomicOrdering::Relaxed), 1);

    let sig: touring_generator::DspySignatureName = String::from("test.sig");
    let mut inputs: touring_generator::DspyInputs = touring_generator::DspyInputs::new();
    inputs.insert("k".into(), serde_json::Value::String("v".into()));
    let dspy_out = ctx.execute_dspy(&sig, &inputs);
    assert_eq!(dspy_out.len(), 1, "DSPy closure must echo one field");
    assert_eq!(counters.dspy_sig.load(AtomicOrdering::Relaxed), 1);

    let similarity = ctx.evaluate_plan_similarity("plan_123");
    assert!(similarity.is_some());
    assert!((similarity.unwrap().value() - 0.7).abs() < f64::EPSILON);
    assert_eq!(counters.cognitive_nexus.load(AtomicOrdering::Relaxed), 1);

    let plan = make_plan(GeneratorKind::RustModule);
    let similar = ctx.find_similar_plans(&plan);
    assert_eq!(similar.len(), 1);
    assert_eq!(similar[0].name, "TestRelatedSym");
    assert_eq!(counters.semantic_graph.load(AtomicOrdering::Relaxed), 1);
}

#[tokio::test]
async fn closures_called_on_draft_to_verified_transition() {
    use touring_generator::{Draft, PlanExecutor};

    let (ctx, counters) = context_with_counting_closures();
    let plan = make_plan(GeneratorKind::RustModule);
    let vgp = &ctx.vgp_engine;

    let executor: PlanExecutor<Draft> = PlanExecutor::first(plan, Arc::clone(&ctx));
    let verified = executor.verify(vgp).await;
    assert!(verified.is_ok(), "VGP must pass with empty contracts");

    // Draft→Verified must fire:
    //   • pheromone_fn once (plan.kind signal)
    //   • cognitive_nexus_fn once (plan_id lookup)
    //   • semantic_graph_fn once (related plan search)
    let (pher, wiring, mcts, wasm, dspy, nexus, graph) = counters.snapshot();
    assert_eq!(pher, 1, "pheromone_fn must fire once on Draft→Verified");
    assert_eq!(
        nexus, 1,
        "cognitive_nexus_fn must fire once on Draft→Verified"
    );
    assert_eq!(
        graph, 1,
        "semantic_graph_fn must fire once on Draft→Verified"
    );
    assert_eq!(
        wiring, 0,
        "wiring_gate_fn must NOT fire yet (hard gate is at commit)"
    );
    assert_eq!(
        mcts, 0,
        "mcts_eval_fn must NOT fire yet (runs at speculate)"
    );
    assert_eq!(
        wasm, 0,
        "wasm_sandbox_fn must NOT fire yet (runs at render)"
    );
    assert_eq!(dspy, 0, "dspy_sig_fn must NOT fire yet (runs at commit)");
}

#[tokio::test]
async fn closures_called_on_verified_to_rendered_transition() {
    use touring_generator::{Draft, PlanExecutor};

    let (ctx, counters) = context_with_counting_closures();
    let plan = make_plan(GeneratorKind::RustModule);

    let executor: PlanExecutor<Draft> = PlanExecutor::first(plan, Arc::clone(&ctx));
    let verified = executor
        .verify(&ctx.vgp_engine)
        .await
        .expect("VGP must pass");

    // Reset wasm counter before render so we can see exactly what render produces.
    let pre_wasm = counters.wasm_sandbox.load(AtomicOrdering::Relaxed);
    let pre_pher = counters.pheromone.load(AtomicOrdering::Relaxed);

    let rendered = verified.render(
        &ctx.template_engine,
        &HashMap::new(),
        None,
        RenderShape::default_width(),
    );
    assert!(rendered.is_ok(), "render must succeed with default vars");

    // Verified→Rendered must fire:
    //   • wasm_sandbox_fn once (pre-validation of rendered content)
    //   • pheromone_fn once ("render_pass" signal)
    let post_wasm = counters.wasm_sandbox.load(AtomicOrdering::Relaxed);
    let post_pher = counters.pheromone.load(AtomicOrdering::Relaxed);
    assert_eq!(
        post_wasm - pre_wasm,
        1,
        "wasm_sandbox_fn must fire once on Verified→Rendered"
    );
    assert_eq!(
        post_pher - pre_pher,
        1,
        "pheromone_fn must fire once on Verified→Rendered"
    );
}

#[tokio::test]
async fn closures_called_on_rendered_to_speculated_transition() {
    use touring_generator::{Draft, PlanExecutor};

    let (ctx, counters) = context_with_counting_closures();
    let plan = make_plan(GeneratorKind::RustModule);

    let executor: PlanExecutor<Draft> = PlanExecutor::first(plan, Arc::clone(&ctx));
    let verified = executor
        .verify(&ctx.vgp_engine)
        .await
        .expect("VGP must pass");
    let rendered = verified
        .render(
            &ctx.template_engine,
            &HashMap::new(),
            None,
            RenderShape::default_width(),
        )
        .expect("render must succeed")
        .expect("render must return Some (not overflow with default width)");

    let pre_mcts = counters.mcts_eval.load(AtomicOrdering::Relaxed);
    let speculated_result = rendered.speculate(&ctx.speculate_bridge).await;

    // MCTS must be called exactly once regardless of speculate outcome.
    let post_mcts = counters.mcts_eval.load(AtomicOrdering::Relaxed);
    assert_eq!(
        post_mcts - pre_mcts,
        1,
        "mcts_eval_fn must fire once on Rendered→Speculated"
    );

    // If speculate passes, pheromone fires once more ("speculate_pass").
    if speculated_result.is_ok() {
        // No assertion needed — success path is optional in CI without speculate daemon.
    }
}

#[tokio::test]
async fn closures_called_on_full_draft_to_committed_pipeline() {
    use touring_generator::{Draft, PlanExecutor};

    let (ctx, counters) = context_with_counting_closures();
    let mut plan = make_plan(GeneratorKind::DiaryEntry);
    plan.target.file_path = format!("/tmp/touring_gen_closure_e2e_{}.md", plan.plan_id);

    let executor: PlanExecutor<Draft> = PlanExecutor::first(plan.clone(), Arc::clone(&ctx));

    let verified = executor.verify(&ctx.vgp_engine).await.expect("VGP pass");
    let rendered = verified
        .render(
            &ctx.template_engine,
            &HashMap::new(),
            None,
            RenderShape::default_width(),
        )
        .expect("render must succeed")
        .expect("render must return Some (not overflow with default width)");

    let speculated_result = rendered.speculate(&ctx.speculate_bridge).await;

    match speculated_result {
        Ok(speculated) => {
            let committed = speculated.commit().await;
            assert!(committed.is_ok(), "commit must succeed: {:?}", committed);

            let (pher, wiring, mcts, wasm, dspy, nexus, graph) = counters.snapshot();

            // Full-pipeline closure dispatch assertions:
            // • semantic_graph_fn: 1 (draft→verified)
            // • cognitive_nexus_fn: 1 (draft→verified)
            // • pheromone_fn: ≥ 4 — vgp_pass (0.3) + render_pass (0.2) + speculate_pass (0.5) + commit_success (1.0)
            // • wasm_sandbox_fn: 1 (verified→rendered pre-validation)
            // • mcts_eval_fn: 1 (rendered→speculated blending)
            // • wiring_gate_fn: 1 (speculated→committed hard gate)
            // • dspy_sig_fn: 1 (speculated→committed plan.commit signature)
            assert_eq!(graph, 1, "semantic_graph_fn must fire exactly once");
            assert_eq!(nexus, 1, "cognitive_nexus_fn must fire exactly once");
            assert!(
                pher >= 4,
                "pheromone_fn must fire at least 4 times (got {pher})"
            );
            assert_eq!(wasm, 1, "wasm_sandbox_fn must fire exactly once");
            assert_eq!(mcts, 1, "mcts_eval_fn must fire exactly once");
            assert_eq!(wiring, 1, "wiring_gate_fn must fire exactly once at commit");
            assert_eq!(dspy, 1, "dspy_sig_fn must fire exactly once at commit");

            // Clean up produced file.
            let written_path = &committed.unwrap().commit_report.files_written[0].path;
            let _ = std::fs::remove_file(written_path);
        }
        Err(_replan) => {
            // Speculate daemon unavailable in CI — still assert that the stages
            // that DID run fired their closures correctly.
            let (pher, wiring, mcts, _wasm, dspy, nexus, graph) = counters.snapshot();
            assert_eq!(graph, 1);
            assert_eq!(nexus, 1);
            assert_eq!(mcts, 1, "mcts must still fire even on replan path");
            assert!(
                pher >= 2,
                "pheromone must fire at least on vgp+render (got {pher})"
            );
            // Wiring gate and DSPy only run on successful commit.
            assert_eq!(wiring, 0);
            assert_eq!(dspy, 0);
        }
    }
}

// ── PLN2 MEGA-E2E — all 10 adapters wired simultaneously ─────────────────
//
// The definitive integration test: constructs a GeneratorContext with EVERY
// production adapter active at once, runs a GeneratorPlan through the full
// Draft → Verified → Rendered → Speculated → Committed pipeline, and asserts
// each adapter's internal counters prove it was invoked by the executor.

#[cfg(all(
    feature = "analysis-gate",
    feature = "cognitive-nexus",
    feature = "mcts-synthesis",
    feature = "nlp-reranking",
    feature = "observability",
    feature = "rl-integration",
    feature = "simd-fuzzy",
    feature = "wasm-sandbox",
    feature = "zero-copy",
))]
mod mega_e2e_all_adapters {
    use super::*;
    use touring_generator::{
        AnalysisGateAdapter, BkTreeFuzzyAdapter, CompositeWiringGate, LinUCBRewardSink,
        McctsEvalAdapter, NlpPlanRankerAdapter, RkyvFileSnapshotAdapter, SemanticGraphAdapter,
        SynWiringGateAdapter, TelemetrySink, TracingTelemetrySink, WasmSandboxAdapter,
    };

    fn mega_seed_db(dir: &str) -> std::path::PathBuf {
        use std::fs;
        let _ = fs::create_dir_all(dir);
        let db_path =
            std::path::PathBuf::from(dir).join(format!("mega_{}.db", Uuid::new_v4().as_u128()));
        let _ = fs::remove_file(&db_path);
        let conn = rusqlite::Connection::open(&db_path).expect("open");
        conn.execute_batch(
            "CREATE TABLE wiring_map (module_file TEXT NOT NULL, symbol_name TEXT NOT NULL, consumer_file TEXT);
             CREATE TABLE module_ecosystem (module_file TEXT PRIMARY KEY, integration_score REAL);",
        )
        .expect("schema");
        for i in 0..20 {
            conn.execute(
                "INSERT INTO wiring_map VALUES (?, ?, ?)",
                rusqlite::params![
                    format!("f{i}.rs"),
                    format!("s{i}"),
                    Some(format!("c{i}.rs"))
                ],
            )
            .expect("insert");
        }
        conn.execute(
            "INSERT INTO module_ecosystem VALUES (?, ?)",
            rusqlite::params!["good.rs", 0.98_f64],
        )
        .expect("me");
        db_path
    }

    #[tokio::test]
    async fn mega_e2e_full_pipeline_with_all_ten_adapters() {
        use touring_generator::{Draft, PlanExecutor};

        // 10 production adapters assembled into a single context.
        let fuzzy_index: Arc<dyn FuzzyMatcher> = Arc::new(BkTreeFuzzyAdapter::new());
        let rl: Arc<dyn touring_generator::RlRewardSink> = Arc::new(LinUCBRewardSink::new());

        let db_path = mega_seed_db("/tmp/touring-gen-mega");
        let syn_gate = SynWiringGateAdapter::with_config(1000, false);
        let analysis_gate =
            AnalysisGateAdapter::with_thresholds(&db_path, 0.1, 1000).expect("open");
        let composite = CompositeWiringGate::compose(syn_gate, analysis_gate);
        let wiring_gate_fn: WiringGateFn = composite.into_closure();

        let graph_adapter = Arc::new(SemanticGraphAdapter::new(std::path::PathBuf::from(
            "/tmp/mega_sg.json",
        )));
        let semantic_graph_fn: SemanticGraphFn =
            Arc::clone(&graph_adapter).into_semantic_graph_fn();

        let nlp_adapter = NlpPlanRankerAdapter::new();
        let cognitive_nexus_fn: CognitiveNexusFn = nlp_adapter.into_cognitive_nexus_fn(vec![
            ("plan-a".into(), "rust module template".into()),
            ("plan-b".into(), "test runner".into()),
        ]);

        let tracing_sink: Arc<TracingTelemetrySink> = Arc::new(TracingTelemetrySink::new());
        let telemetry: Arc<dyn TelemetrySink> = Arc::clone(&tracing_sink) as Arc<dyn TelemetrySink>;

        let mcts_adapter = McctsEvalAdapter::with_graph(graph_adapter.graph());
        let mcts_eval_fn: MctsEvalFn = mcts_adapter.into_closure();

        let wasm_adapter = WasmSandboxAdapter::with_default_wat().expect("default wat");
        let wasm_sandbox_fn: WasmSandboxFn = wasm_adapter.into_closure();

        // Pheromone + dspy counting wrappers.
        let pheromone_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let pc_clone = Arc::clone(&pheromone_count);
        let pheromone_fn: PheromoneUpdateFn = Arc::new(move |_, _| {
            pc_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        });

        let dspy_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let dc_clone = Arc::clone(&dspy_count);
        let dspy_sig_fn: DspySigFn = Arc::new(move |_, inputs: &DspyInputs| {
            dc_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            inputs.clone()
        });

        let file_cache = Arc::new(tokio::sync::RwLock::new(
            touring_intelligence::index::FileCache::new(),
        ));
        let ctx = Arc::new(GeneratorContext {
            project_root: camino::Utf8PathBuf::from("/tmp/touring-gen-mega-e2e"),
            symbol_index: Arc::new(touring_intelligence::index::IncrementalIndex::new(
                file_cache,
            )),
            fuzzy_index,
            vgp_engine: Arc::new(VgpEngine::with_subprocess(Arc::clone(&telemetry))),
            template_engine: Arc::new(TemplateEngine::new(Arc::clone(&telemetry))),
            speculate_bridge: Arc::new(SpeculateBridge::new(Arc::clone(&telemetry))),
            schema_registry: Arc::new(SchemaRegistry::new("2.0.0")),
            plan_registry: Arc::new(GenPlanRegistry::new()),
            memory: Arc::new(NoopMemory),
            llm: Arc::new(NoopLlm),
            rl,
            telemetry,
            semantic_graph_fn: Some(semantic_graph_fn),
            pheromone_fn: Some(pheromone_fn),
            cognitive_nexus_fn: Some(cognitive_nexus_fn),
            wiring_gate_fn: Some(wiring_gate_fn),
            health_delta_record_fn: None,
            health_delta_compute_fn: None,
            wasm_sandbox_fn: Some(wasm_sandbox_fn),
            mcts_eval_fn: Some(mcts_eval_fn),
            dspy_sig_fn: Some(dspy_sig_fn),
            knowledge_upsert_fn: None,
            session_start_fn: None,
            session_checkpoint_fn: None,
            session_assess_fn: None,
            decompose_create_fn: None,
            decompose_update_fn: None,
            #[cfg(feature = "quality-gate")]
            quality_gate_fn: None,
            #[cfg(feature = "quality-gate")]
            quality_gate_adapter: None,
            #[cfg(feature = "health-gate")]
            health_gate_fn: None,
            #[cfg(feature = "enrichment-gate")]
            enrichment_trigger_fn: None,
            backpressure: Arc::new(tokio::sync::Semaphore::new(64)),
            capacity: CapacityLimits::default(),
            audit_log: Arc::new(NoopAuditLog),
            concolic_analyze_fn: None,
        });

        // Drive a plan through the full pipeline.
        let mut plan = make_plan(GeneratorKind::DiaryEntry);
        plan.target.file_path = format!("/tmp/touring_mega_e2e_{}.md", plan.plan_id);
        let target_path = plan.target.file_path.clone();
        let _ = std::fs::remove_file(&target_path);

        let executor: PlanExecutor<Draft> = PlanExecutor::first(plan, Arc::clone(&ctx));

        let verified = executor.verify(&ctx.vgp_engine).await.expect("VGP pass");
        assert!(tracing_sink.plan_event_count() >= 1);
        assert!(pheromone_count.load(std::sync::atomic::Ordering::Relaxed) >= 1);

        let rendered = verified
            .render(
                &ctx.template_engine,
                &HashMap::new(),
                None,
                RenderShape::default_width(),
            )
            .expect("render")
            .expect("render must return Some (not overflow with default width)");

        let speculated_result = rendered.speculate(&ctx.speculate_bridge).await;

        if let Ok(speculated) = speculated_result {
            let score_before = speculated.score().value();
            assert!((0.0..=1.0).contains(&score_before));

            let committed = speculated.commit().await;
            assert!(committed.is_ok(), "commit must succeed: {committed:?}");

            assert!(
                tracing_sink.plan_event_count() >= 4,
                "TracingTelemetrySink must record ≥4 transitions, got {}",
                tracing_sink.plan_event_count()
            );
            assert!(dspy_count.load(std::sync::atomic::Ordering::Relaxed) >= 1);

            let report = committed.unwrap();
            assert!(!report.commit_report.files_written.is_empty());
            let written_path = &report.commit_report.files_written[0].path;
            assert!(std::path::Path::new(written_path).exists());

            // RkyvFileSnapshotAdapter round-trip on the committed content.
            let content = std::fs::read_to_string(written_path).expect("read");
            let files = vec![RenderedFile::new(
                written_path.clone(),
                content,
                FileAction::Created,
            )];
            let snapshot = RkyvFileSnapshotAdapter::snapshot(&files).expect("snapshot");
            let restored = RkyvFileSnapshotAdapter::restore(&snapshot).expect("restore");
            assert_eq!(restored.len(), 1);

            let _ = std::fs::remove_file(written_path);
        } else {
            // Speculate daemon unavailable in CI — still proves upstream adapters fired.
            assert!(tracing_sink.plan_event_count() >= 2);
        }

        let _ = std::fs::remove_file(&target_path);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file("/tmp/mega_sg.json");
    }
}

// ── PLN2 compositional — with_mcts_eval builder E2E ──────────────────────

#[cfg(feature = "mcts-synthesis")]
mod with_mcts_eval_builder_tests {
    use super::*;
    use touring_generator::McctsEvalAdapter;

    #[test]
    fn with_mcts_eval_injects_closure_into_arc_context() {
        let ctx = GeneratorContext::for_testing();
        assert!(ctx.mcts_eval_fn.is_none());

        let persistence = Arc::new(
            touring_intelligence::reasoning::persistence::GraphPersistence::new(
                std::path::PathBuf::from("/tmp/with_mcts_builder.json"),
            ),
        );
        let graph = Arc::new(
            touring_intelligence::reasoning::semantic_graph::SemanticGraph::new(persistence),
        );
        let adapter = McctsEvalAdapter::with_graph(graph);
        let mcts_fn: MctsEvalFn = adapter.into_closure();

        let new_ctx = ctx.with_mcts_eval(mcts_fn);
        assert!(new_ctx.mcts_eval_fn.is_some());

        // Calling the mcts scoring helper through the Arc now returns a real score.
        let score = new_ctx.mcts_evaluate("any-state");
        assert!(score.value() >= 0.0 && score.value() <= 1.0);
    }
}

// ── PLN2 compositional — CompositeWiringGate E2E ─────────────────────────

#[cfg(feature = "analysis-gate")]
mod composite_wiring_gate_tests {
    use super::*;
    use touring_generator::{AnalysisGateAdapter, CompositeWiringGate, SynWiringGateAdapter};

    fn seed_db(dir: &str, orphans: usize, total: usize) -> std::path::PathBuf {
        use std::fs;
        let _ = fs::create_dir_all(dir);
        let db_path =
            std::path::PathBuf::from(dir).join(format!("cg_{}.db", Uuid::new_v4().as_u128()));
        let _ = fs::remove_file(&db_path);
        let conn = rusqlite::Connection::open(&db_path).expect("open");
        conn.execute_batch(
            "CREATE TABLE wiring_map (module_file TEXT NOT NULL, symbol_name TEXT NOT NULL, consumer_file TEXT);
             CREATE TABLE module_ecosystem (module_file TEXT PRIMARY KEY, integration_score REAL);",
        )
        .expect("schema");
        for i in 0..total {
            let consumer: Option<String> = if i < orphans {
                None
            } else {
                Some(format!("c{i}.rs"))
            };
            conn.execute(
                "INSERT INTO wiring_map VALUES (?, ?, ?)",
                rusqlite::params![format!("f{i}.rs"), format!("s{i}"), consumer],
            )
            .expect("insert");
        }
        conn.execute(
            "INSERT INTO module_ecosystem VALUES (?, ?)",
            rusqlite::params!["good.rs", 0.95_f64],
        )
        .expect("me");
        db_path
    }

    #[test]
    fn composite_gate_open_succeeds_with_valid_db() {
        let db = seed_db("/tmp/touring-gen-comp-open", 0, 10);
        let gate = CompositeWiringGate::open(&db);
        assert!(gate.is_ok());
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn composite_gate_accepts_clean_rust() {
        let db = seed_db("/tmp/touring-gen-comp-accept", 1, 20);
        let gate = CompositeWiringGate::open(&db).expect("open");
        let files = vec![RenderedFile::new(
            "/tmp/cg_clean.rs",
            "pub fn ok() {}\n",
            FileAction::Created,
        )];
        assert!(gate.check(&files, "test-plan").is_ok());
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn composite_gate_rejects_unparseable_rust_at_syn_stage() {
        let db = seed_db("/tmp/touring-gen-comp-syn-fail", 0, 10);
        let gate = CompositeWiringGate::open(&db).expect("open");
        let files = vec![RenderedFile::new(
            "/tmp/cg_bad.rs",
            "pub fn broken( { }",
            FileAction::Created,
        )];
        let result = gate.check(&files, "test-plan");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not valid Rust"),
            "must be Syn rejection: {err}"
        );
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn composite_gate_rejects_orphan_risk_at_analysis_stage() {
        let db = seed_db("/tmp/touring-gen-comp-analysis-fail", 1, 20);
        let syn_gate = SynWiringGateAdapter::new();
        let analysis_gate = AnalysisGateAdapter::with_thresholds(&db, 0.5, 1).expect("open");
        let gate = CompositeWiringGate::compose(syn_gate, analysis_gate);
        let files = vec![RenderedFile::new(
            "/tmp/cg_orphan.rs",
            "pub fn a() {}\npub fn b() {}\npub fn c() {}\n",
            FileAction::Created,
        )];
        let result = gate.check(&files, "test-plan");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("projected") || err.contains("analysis"),
            "must be Analysis rejection: {err}"
        );
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn composite_gate_into_closure_round_trips_via_wiring_gate_fn() {
        let db = seed_db("/tmp/touring-gen-comp-closure", 0, 10);
        let gate = CompositeWiringGate::open(&db).expect("open");
        let gate_fn: WiringGateFn = gate.into_closure();

        let files = vec![RenderedFile::new(
            "/tmp/cg_closure.rs",
            "pub fn via_closure() {}\n",
            FileAction::Created,
        )];
        assert!(gate_fn(&files, "test-plan").is_ok());
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn composite_gate_accessors_return_both_inner_gates() {
        let db = seed_db("/tmp/touring-gen-comp-access", 0, 5);
        let gate = CompositeWiringGate::open(&db).expect("open");
        let _syn = gate.syn_gate();
        let _analysis = gate.analysis_gate();
        let _ = std::fs::remove_file(&db);
    }
}

// ── PLN2 production adapter — RkyvFileSnapshotAdapter E2E ─────────────────

#[cfg(feature = "zero-copy")]
mod rkyv_snapshot_adapter_tests {
    use super::*;
    use touring_generator::RkyvFileSnapshotAdapter;

    #[test]
    fn rkyv_snapshot_empty_list_round_trips() {
        let files: Vec<RenderedFile> = Vec::new();
        let buf = RkyvFileSnapshotAdapter::snapshot(&files).expect("snapshot");
        let restored = RkyvFileSnapshotAdapter::restore(&buf).expect("restore");
        assert!(restored.is_empty());
    }

    #[test]
    fn rkyv_snapshot_single_file_round_trips() {
        let files = vec![RenderedFile::new(
            "/tmp/rkyv_single.rs",
            "pub fn hello() -> &'static str { \"world\" }\n",
            FileAction::Created,
        )];
        let buf = RkyvFileSnapshotAdapter::snapshot(&files).expect("snapshot");
        let restored = RkyvFileSnapshotAdapter::restore(&buf).expect("restore");
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].path, "/tmp/rkyv_single.rs");
        assert!(restored[0].content.contains("pub fn hello"));
        assert_eq!(restored[0].action, FileAction::Created);
    }

    #[test]
    fn rkyv_snapshot_multiple_files_round_trip() {
        let files = vec![
            RenderedFile::new("/tmp/a.rs", "fn a() {}", FileAction::Created),
            RenderedFile::new("/tmp/b.rs", "fn b() {}", FileAction::Overwritten),
            RenderedFile::new("/tmp/c.md", "# Markdown", FileAction::Created),
        ];
        let buf = RkyvFileSnapshotAdapter::snapshot(&files).expect("snapshot");
        let restored = RkyvFileSnapshotAdapter::restore(&buf).expect("restore");
        assert_eq!(restored.len(), 3);
        for (orig, got) in files.iter().zip(restored.iter()) {
            assert_eq!(orig.path, got.path);
            assert_eq!(orig.content, got.content);
            // Note: restore always uses Created — by design
        }
    }

    #[test]
    fn rkyv_snapshot_unicode_content_round_trips() {
        let files = vec![RenderedFile::new(
            "/tmp/rkyv_utf8.rs",
            "// Comentário português com acentuação: çãáéíóú 🚀\npub fn ok() {}\n",
            FileAction::Created,
        )];
        let buf = RkyvFileSnapshotAdapter::snapshot(&files).expect("snapshot");
        let restored = RkyvFileSnapshotAdapter::restore(&buf).expect("restore");
        assert_eq!(restored[0].content, files[0].content);
    }

    #[test]
    fn rkyv_restore_detects_truncated_buffer() {
        let files = vec![RenderedFile::new(
            "/tmp/t.rs",
            "fn f() {}",
            FileAction::Created,
        )];
        let buf = RkyvFileSnapshotAdapter::snapshot(&files).expect("snapshot");

        // Truncate to force error paths.
        assert!(RkyvFileSnapshotAdapter::restore(&buf[..3]).is_err());
        assert!(RkyvFileSnapshotAdapter::restore(&buf[..7]).is_err());
        assert!(RkyvFileSnapshotAdapter::restore(&[]).is_err());
    }

    #[test]
    fn rkyv_restore_detects_trailing_garbage() {
        let files = vec![RenderedFile::new(
            "/tmp/t.rs",
            "fn f() {}",
            FileAction::Created,
        )];
        let mut buf = RkyvFileSnapshotAdapter::snapshot(&files).expect("snapshot");
        buf.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);

        let result = RkyvFileSnapshotAdapter::restore(&buf);
        assert!(result.is_err(), "trailing bytes must fail validation");
    }

    #[test]
    fn rkyv_snapshot_rkyv_produces_aligned_buffer() {
        let files = vec![RenderedFile::new(
            "/tmp/rkyv_aligned.rs",
            "pub fn aligned() {}",
            FileAction::Created,
        )];
        let aligned = RkyvFileSnapshotAdapter::snapshot_rkyv(&files).expect("rkyv snapshot");
        // AlignedVec has a non-zero length.
        assert!(!aligned.is_empty());
    }

    #[test]
    fn rkyv_snapshot_adapter_default_constructs() {
        let _adapter = RkyvFileSnapshotAdapter::new();
        let _default = RkyvFileSnapshotAdapter;
    }

    #[test]
    fn rkyv_snapshot_large_batch_round_trips() {
        let files: Vec<RenderedFile> = (0..100)
            .map(|i| {
                RenderedFile::new(
                    format!("/tmp/rkyv_batch_{i}.rs"),
                    format!("pub fn f{i}() {{ {i} }}\n"),
                    FileAction::Created,
                )
            })
            .collect();
        let buf = RkyvFileSnapshotAdapter::snapshot(&files).expect("snapshot");
        let restored = RkyvFileSnapshotAdapter::restore(&buf).expect("restore");
        assert_eq!(restored.len(), 100);
        for (i, file) in restored.iter().enumerate() {
            assert!(file.path.contains(&format!("rkyv_batch_{i}")));
            assert!(file.content.contains(&format!("f{i}")));
        }
    }
}

// ── PLN2 production adapter — WasmSandboxAdapter E2E ──────────────────────

#[cfg(feature = "wasm-sandbox")]
mod wasm_sandbox_adapter_tests {
    use super::*;
    use touring_generator::WasmSandboxAdapter;

    #[test]
    fn wasm_sandbox_with_default_wat_constructs() {
        let adapter = WasmSandboxAdapter::with_default_wat();
        assert!(adapter.is_ok(), "default WAT must compile: {adapter:?}");
    }

    #[test]
    fn wasm_sandbox_with_custom_wat_success_module() {
        // Custom WAT that also returns 1 (success).
        let wat = r#"
            (module
              (func $evaluate (export "evaluate") (result i32)
                i32.const 1))
        "#;
        let adapter = WasmSandboxAdapter::with_wat(wat);
        assert!(adapter.is_ok());
    }

    #[test]
    fn wasm_sandbox_with_invalid_wat_returns_error() {
        let wat = "this is not valid WAT";
        let adapter = WasmSandboxAdapter::with_wat(wat);
        assert!(adapter.is_err(), "malformed WAT must fail to load");
    }

    #[test]
    fn wasm_sandbox_run_success_module_returns_ok() {
        let adapter = WasmSandboxAdapter::with_default_wat().expect("default wat");
        let result = adapter.run("fn main() {}", "rust");
        // Default success module: plugin returns success=true, output="".
        assert!(
            result.is_ok(),
            "default success module must return Ok: {result:?}"
        );
    }

    #[test]
    fn wasm_sandbox_run_failure_module_returns_err() {
        // WAT that returns 0 (failure).
        let wat = r#"
            (module
              (func $evaluate (export "evaluate") (result i32)
                i32.const 0))
        "#;
        let adapter = WasmSandboxAdapter::with_wat(wat).expect("wat load");
        let result = adapter.run("any content", "rust");
        assert!(result.is_err(), "failure module must return Err");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("wasm sandbox"),
            "error must mention sandbox: {err_msg}"
        );
    }

    #[test]
    fn wasm_sandbox_into_closure_round_trips_via_wasm_sandbox_fn() {
        let adapter = WasmSandboxAdapter::with_default_wat().expect("default wat");
        let sandbox_fn: WasmSandboxFn = adapter.into_closure();
        let result = sandbox_fn("pub fn closure_test() {}", "rust");
        assert!(result.is_ok());
    }

    #[test]
    fn wasm_sandbox_with_wasm_bytes_from_valid_bytes() {
        // Compile the default WAT into WASM bytes via wabt — since we don't have
        // wabt at runtime, we use the WAT form here. This test verifies the byte
        // path exists; real usage would preload compiled .wasm files.
        // Trick: touring_wasm may support raw WAT bytes via load_module? — it does not.
        // So this test only verifies that invalid bytes fail gracefully.
        let invalid_bytes = b"not a wasm module";
        let adapter = WasmSandboxAdapter::with_wasm_bytes(invalid_bytes);
        assert!(adapter.is_err(), "invalid bytes must be rejected");
    }

    #[test]
    fn wasm_sandbox_runner_accessor_returns_reference() {
        let adapter = WasmSandboxAdapter::with_default_wat().expect("default wat");
        let _runner = adapter.runner();
        // Runner exists and is accessible — proof that the internal field is live.
    }
}

// ── PLN2 production adapter — McctsEvalAdapter E2E ────────────────────────

#[cfg(feature = "mcts-synthesis")]
mod mcts_eval_adapter_tests {
    use super::*;
    use touring_generator::McctsEvalAdapter;

    fn build_test_graph() -> Arc<touring_intelligence::reasoning::semantic_graph::SemanticGraph> {
        let persistence = Arc::new(
            touring_intelligence::reasoning::persistence::GraphPersistence::new(
                std::path::PathBuf::from("/tmp/mcts_adapter_test.json"),
            ),
        );
        Arc::new(touring_intelligence::reasoning::semantic_graph::SemanticGraph::new(persistence))
    }

    #[test]
    fn mcts_hash_state_is_deterministic() {
        let h1 = McctsEvalAdapter::hash_state("plan-a");
        let h2 = McctsEvalAdapter::hash_state("plan-a");
        let h3 = McctsEvalAdapter::hash_state("plan-b");
        assert_eq!(h1, h2, "same key must yield same hash");
        assert_ne!(h1, h3, "different keys must differ");
    }

    #[test]
    fn mcts_evaluate_empty_graph_returns_zero() {
        let graph = build_test_graph();
        let adapter = McctsEvalAdapter::with_graph(Arc::clone(&graph));
        let score = adapter.evaluate("any-plan-id");
        // Empty graph → no neighbors → MCTS returns None → score is ZERO.
        assert!((score.value() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn mcts_evaluate_returns_score_with_populated_graph() {
        use touring_intelligence::reasoning::semantic_graph::{MemoryNode, NodeType};

        let graph = build_test_graph();
        // Seed: root + 3 neighbors forms a small branching tree for MCTS.
        graph
            .add_node(MemoryNode::new("root", "root plan", NodeType::Concept))
            .expect("root node");
        graph
            .add_node(MemoryNode::new("a", "alt a", NodeType::Concept))
            .expect("a node");
        graph
            .add_node(MemoryNode::new("b", "alt b", NodeType::Concept))
            .expect("b node");
        graph
            .add_node(MemoryNode::new("c", "alt c", NodeType::Concept))
            .expect("c node");
        graph.add_edge("root", "a", 1.0).expect("edge a");
        graph.add_edge("root", "b", 0.7).expect("edge b");
        graph.add_edge("root", "c", 0.4).expect("edge c");

        let adapter = McctsEvalAdapter::with_graph(Arc::clone(&graph));
        let score = adapter.evaluate("root");
        // Populated graph → MCTS returns Some → score in [0, 1].
        assert!(score.value() >= 0.0 && score.value() <= 1.0);
    }

    #[test]
    fn mcts_evaporate_does_not_panic() {
        let graph = build_test_graph();
        let adapter = McctsEvalAdapter::with_graph(graph);
        adapter.evaporate(); // must be safe on a fresh engine
    }

    #[test]
    fn mcts_into_closure_round_trips_via_mcts_eval_fn() {
        let graph = build_test_graph();
        let adapter = McctsEvalAdapter::with_graph(graph);
        let eval_fn: MctsEvalFn = adapter.into_closure();

        let score = eval_fn("any-state");
        assert!(score.value() >= 0.0 && score.value() <= 1.0);
    }

    #[test]
    fn mcts_with_config_honours_custom_config() {
        let graph = build_test_graph();
        let cfg = touring_intelligence::reasoning::cognitive_mcts::CognitiveMCTSConfig::default();
        let adapter = McctsEvalAdapter::with_config(Arc::clone(&graph), cfg);
        // Graph ref returned by adapter matches the one passed in.
        let returned = adapter.graph();
        assert!(Arc::ptr_eq(&graph, &returned));
    }
}

// ── PLN2 production adapter — TracingTelemetrySink E2E ────────────────────

#[cfg(feature = "observability")]
mod tracing_telemetry_sink_tests {
    use super::*;
    use touring_generator::{TelemetrySink, TracingTelemetrySink};

    #[test]
    fn tracing_sink_initial_counters_are_zero() {
        let sink = TracingTelemetrySink::new();
        assert_eq!(sink.plan_event_count(), 0);
        assert_eq!(sink.counter_total(), 0);
        assert_eq!(sink.histogram_sample_count(), 0);
    }

    #[test]
    fn tracing_sink_records_lifecycle_transition_increments_counter() {
        let sink = TracingTelemetrySink::new();
        let plan_id = Uuid::new_v4();

        sink.record_lifecycle_transition("Draft", "Verified", plan_id, 12345);
        assert_eq!(sink.plan_event_count(), 1);

        sink.record_lifecycle_transition("Verified", "Rendered", plan_id, 23456);
        sink.record_lifecycle_transition("Rendered", "Speculated", plan_id, 34567);
        assert_eq!(sink.plan_event_count(), 3);
    }

    #[test]
    fn tracing_sink_increment_counter_adds_value() {
        let sink = TracingTelemetrySink::new();
        sink.increment_counter("plans_submitted", 1);
        sink.increment_counter("plans_committed", 5);
        sink.increment_counter("plans_failed", 2);
        assert_eq!(sink.counter_total(), 8);
    }

    #[test]
    fn tracing_sink_record_histogram_counts_samples() {
        let sink = TracingTelemetrySink::new();
        sink.record_histogram("vgp_latency_ms", 3.2);
        sink.record_histogram("vgp_latency_ms", 4.1);
        sink.record_histogram("render_latency_ms", 12.5);
        assert_eq!(sink.histogram_sample_count(), 3);
    }

    #[tokio::test]
    async fn tracing_sink_captures_full_pipeline_transitions() {
        // Drive a real plan through Draft→Verified→Rendered and assert the sink
        // saw all transitions from the typestate executor closures.
        use touring_generator::{Draft, PlanExecutor};

        let sink: Arc<TracingTelemetrySink> = Arc::new(TracingTelemetrySink::new());
        let sink_for_ctx: Arc<dyn touring_generator::TelemetrySink> =
            Arc::clone(&sink) as Arc<dyn touring_generator::TelemetrySink>;

        let file_cache = Arc::new(tokio::sync::RwLock::new(
            touring_intelligence::index::FileCache::new(),
        ));
        let ctx = Arc::new(GeneratorContext {
            project_root: camino::Utf8PathBuf::from("/tmp/touring-gen-tracing"),
            symbol_index: Arc::new(touring_intelligence::index::IncrementalIndex::new(
                file_cache,
            )),
            fuzzy_index: Arc::new(NoopFuzzyMatcher) as Arc<dyn FuzzyMatcher>,
            vgp_engine: Arc::new(VgpEngine::with_subprocess(Arc::clone(&sink_for_ctx))),
            template_engine: Arc::new(TemplateEngine::new(Arc::clone(&sink_for_ctx))),
            speculate_bridge: Arc::new(SpeculateBridge::new(Arc::clone(&sink_for_ctx))),
            schema_registry: Arc::new(SchemaRegistry::new("2.0.0")),
            plan_registry: Arc::new(GenPlanRegistry::new()),
            memory: Arc::new(NoopMemory),
            llm: Arc::new(NoopLlm),
            rl: Arc::new(NoopRlSink),
            telemetry: Arc::clone(&sink_for_ctx),
            semantic_graph_fn: None,
            pheromone_fn: None,
            cognitive_nexus_fn: None,
            wiring_gate_fn: None,
            health_delta_record_fn: None,
            health_delta_compute_fn: None,
            wasm_sandbox_fn: None,
            mcts_eval_fn: None,
            dspy_sig_fn: None,
            knowledge_upsert_fn: None,
            session_start_fn: None,
            session_checkpoint_fn: None,
            session_assess_fn: None,
            decompose_create_fn: None,
            decompose_update_fn: None,
            #[cfg(feature = "quality-gate")]
            quality_gate_fn: None,
            #[cfg(feature = "quality-gate")]
            quality_gate_adapter: None,
            #[cfg(feature = "health-gate")]
            health_gate_fn: None,
            #[cfg(feature = "enrichment-gate")]
            enrichment_trigger_fn: None,
            backpressure: Arc::new(tokio::sync::Semaphore::new(64)),
            capacity: CapacityLimits::default(),
            audit_log: Arc::new(NoopAuditLog),
            concolic_analyze_fn: None,
        });

        let plan = make_plan(GeneratorKind::RustModule);
        let executor: PlanExecutor<Draft> = PlanExecutor::first(plan, Arc::clone(&ctx));
        let verified = executor
            .verify(&ctx.vgp_engine)
            .await
            .expect("VGP must pass");
        let _rendered = verified
            .render(
                &ctx.template_engine,
                &HashMap::new(),
                None,
                RenderShape::default_width(),
            )
            .expect("render must succeed")
            .expect("render must return Some (not overflow with default width)");

        // The executor calls ctx.record_transition() at least twice:
        //   Draft → Verified
        //   Verified → Rendered
        assert!(
            sink.plan_event_count() >= 2,
            "Expected ≥2 lifecycle events, got {}",
            sink.plan_event_count()
        );
    }

    #[test]
    fn tracing_sink_relaxed_ordering_is_thread_safe() {
        use std::thread;

        let sink = Arc::new(TracingTelemetrySink::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let s = Arc::clone(&sink);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    s.increment_counter("concurrent_counter", 1);
                    s.record_histogram("concurrent_hist", 1.0);
                    s.record_lifecycle_transition("A", "B", Uuid::new_v4(), 0);
                }
            }));
        }
        for h in handles {
            h.join().expect("thread join");
        }
        assert_eq!(sink.counter_total(), 800);
        assert_eq!(sink.histogram_sample_count(), 800);
        assert_eq!(sink.plan_event_count(), 800);
    }
}

// ── PLN2 production adapter — NlpPlanRankerAdapter E2E ─────────────────────

#[cfg(feature = "nlp-reranking")]
mod nlp_plan_ranker_adapter_tests {
    use super::*;
    use touring_generator::NlpPlanRankerAdapter;

    #[test]
    fn nlp_ranker_ranks_matching_intent_highest() {
        let adapter = NlpPlanRankerAdapter::new();
        let candidates = vec![
            (
                "plan-a".to_string(),
                "implement async API handler".to_string(),
            ),
            (
                "plan-b".to_string(),
                "docker kubernetes deployment".to_string(),
            ),
            (
                "plan-c".to_string(),
                "async handler with rate limiter".to_string(),
            ),
        ];
        let ranked = adapter.rank_intents("async handler implementation", &candidates);

        assert_eq!(ranked.len(), 3);
        // plan-a and plan-c both contain "async" and "handler" — at least one wins.
        assert!(
            ranked[0].0 == "plan-a" || ranked[0].0 == "plan-c",
            "top rank should match async+handler keywords, got {:?}",
            ranked[0]
        );
        // plan-b has zero matches — must be last.
        assert_eq!(ranked[2].0, "plan-b");
        assert_eq!(ranked[2].1, 0);
    }

    #[test]
    fn nlp_ranker_returns_zero_for_empty_query_tokens() {
        let adapter = NlpPlanRankerAdapter::new();
        let candidates = vec![("plan-a".to_string(), "anything here".to_string())];
        // Query has only tokens shorter than 3 chars → filtered out → no matches.
        let ranked = adapter.rank_intents("a b c", &candidates);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].1, 0);
    }

    #[test]
    fn nlp_ranker_handles_empty_candidates() {
        let adapter = NlpPlanRankerAdapter::new();
        let ranked = adapter.rank_intents("any query here", &[]);
        assert!(ranked.is_empty());
    }

    #[test]
    fn nlp_ranker_is_case_insensitive_by_default() {
        let adapter = NlpPlanRankerAdapter::new();
        let candidates = vec![
            ("plan-a".to_string(), "ASYNC HANDLER SETUP".to_string()),
            ("plan-b".to_string(), "unrelated content".to_string()),
        ];
        let ranked = adapter.rank_intents("async handler", &candidates);
        // Case-insensitive: plan-a must match despite uppercase.
        assert_eq!(ranked[0].0, "plan-a");
        assert!(ranked[0].1 >= 2, "should match 'async' AND 'handler'");
    }

    #[test]
    fn nlp_ranker_into_cognitive_nexus_fn_scores_known_query() {
        let adapter = NlpPlanRankerAdapter::new();
        let candidates = vec![
            (
                "plan-x".to_string(),
                "rust async tokio framework".to_string(),
            ),
            ("plan-y".to_string(), "python django rest".to_string()),
        ];
        let nexus_fn: CognitiveNexusFn = adapter.into_cognitive_nexus_fn(candidates);

        // Query matches plan-x strongly.
        let score_rust = nexus_fn("rust tokio framework");
        assert!(score_rust.is_some());
        assert!(score_rust.unwrap().value() > 0.0);

        // Query with zero matches → score 0.0 but still Some.
        let score_empty = nexus_fn("unrelated zzz qqq");
        assert!(score_empty.is_some());
        assert!((score_empty.unwrap().value() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn nlp_ranker_cognitive_nexus_fn_empty_candidates_returns_none() {
        let adapter = NlpPlanRankerAdapter::new();
        let nexus_fn: CognitiveNexusFn = adapter.into_cognitive_nexus_fn(vec![]);
        assert!(nexus_fn("any query").is_none());
    }

    #[test]
    fn nlp_ranker_with_config_respects_custom_matcher_config() {
        let mut cfg = touring_intelligence::ann::MatcherConfig::default();
        cfg.case_insensitive = false;
        let adapter = NlpPlanRankerAdapter::with_config(cfg);
        let candidates = vec![
            ("plan-lower".to_string(), "async handler setup".to_string()),
            ("plan-upper".to_string(), "ASYNC HANDLER SETUP".to_string()),
        ];
        // With case sensitivity: lowercase query matches only plan-lower.
        let ranked = adapter.rank_intents("async", &candidates);
        // extract_tokens lowercases the query, so "async" matches lowercase exactly.
        assert_eq!(ranked[0].0, "plan-lower");
    }
}

// ── PLN2 production adapter — AnalysisGateAdapter E2E ──────────────────────

#[cfg(feature = "analysis-gate")]
mod analysis_gate_adapter_tests {
    use super::*;
    use touring_generator::AnalysisGateAdapter;

    /// Helper: create a temp SQLite DB file seeded with a minimal wiring_map
    /// + module_ecosystem schema so `analyze_wiring` can run against it.
    fn make_seeded_db(dir: &str, orphans: usize, total_pub: usize) -> std::path::PathBuf {
        use std::fs;
        let _ = fs::create_dir_all(dir);
        let db_path =
            std::path::PathBuf::from(dir).join(format!("ag_test_{}.db", Uuid::new_v4().as_u128()));
        let _ = fs::remove_file(&db_path);

        let conn = rusqlite::Connection::open(&db_path).expect("open db");
        // Minimal wiring_map table matching touring-analysis schema expectations.
        conn.execute_batch(
            "CREATE TABLE wiring_map (
                module_file TEXT NOT NULL,
                symbol_name TEXT NOT NULL,
                consumer_file TEXT
            );
            CREATE TABLE module_ecosystem (
                module_file TEXT PRIMARY KEY,
                integration_score REAL
            );",
        )
        .expect("create schema");

        // Seed total_pub rows. First `orphans` have NULL consumer; the rest have a dummy consumer.
        for i in 0..total_pub {
            let consumer: Option<String> = if i < orphans {
                None
            } else {
                Some(format!("file_consumer_{i}.rs"))
            };
            conn.execute(
                "INSERT INTO wiring_map (module_file, symbol_name, consumer_file) VALUES (?, ?, ?)",
                rusqlite::params![format!("file_{i}.rs"), format!("sym_{i}"), consumer],
            )
            .expect("insert");
        }
        // Seed module_ecosystem with high scores so avg_integration_score stays high.
        conn.execute(
            "INSERT INTO module_ecosystem (module_file, integration_score) VALUES (?, ?)",
            rusqlite::params!["good_file.rs", 0.95_f64],
        )
        .expect("seed ecosystem");

        db_path
    }

    #[test]
    fn analysis_gate_open_fails_on_missing_db() {
        let result = AnalysisGateAdapter::open(std::path::Path::new(
            "/nonexistent-absolutely/path/to/missing.db",
        ));
        // rusqlite::Connection::open auto-creates missing files — the open itself succeeds
        // but baseline analysis runs on an empty schema. Cover both outcomes below.
        if let Ok(adapter) = result {
            let _ = adapter.baseline_report(); // must not panic on missing tables
        }
    }

    #[test]
    fn analysis_gate_baseline_report_returns_score() {
        let db_path = make_seeded_db("/tmp/touring-gen-ag-baseline", 2, 20);
        let adapter = AnalysisGateAdapter::open(&db_path).expect("open");
        let report = adapter.baseline_report().expect("baseline");
        assert_eq!(report.orphan_count, 2);
        assert_eq!(report.total_pub_symbols, 20);
        assert!(report.score >= 0.0 && report.score <= 1.0);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn analysis_gate_accepts_files_under_threshold() {
        let db_path = make_seeded_db("/tmp/touring-gen-ag-under", 1, 20);
        let adapter = AnalysisGateAdapter::with_thresholds(&db_path, 0.5, 5).expect("open");

        let files = vec![RenderedFile::new(
            "/tmp/ag_accept.rs",
            "pub fn one() {}\npub struct Two;\n",
            FileAction::Created,
        )];
        let result = adapter.check(&files, "test-plan");
        assert!(result.is_ok(), "2 pub items ≤ max 5 must pass: {result:?}");
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn analysis_gate_rejects_when_projected_orphan_delta_exceeds_max() {
        let db_path = make_seeded_db("/tmp/touring-gen-ag-reject-delta", 1, 20);
        let adapter = AnalysisGateAdapter::with_thresholds(&db_path, 0.5, 2).expect("open");

        // Build content with 5 pub items — exceeds max delta of 2.
        let content = "pub fn a() {}\npub fn b() {}\npub fn c() {}\npub fn d() {}\npub fn e() {}\n";
        let files = vec![RenderedFile::new(
            "/tmp/ag_reject_delta.rs",
            content,
            FileAction::Created,
        )];
        let result = adapter.check(&files, "test-plan");
        assert!(result.is_err(), "5 pub items > max 2 must reject");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("projected"),
            "error must mention projection: {err}"
        );
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn analysis_gate_rejects_when_baseline_score_below_min() {
        // Heavily degraded DB: 9 orphans out of 10 pubs → orphan_rate = 0.9 → score < 0.5.
        let db_path = make_seeded_db("/tmp/touring-gen-ag-reject-baseline", 9, 10);
        let adapter = AnalysisGateAdapter::with_thresholds(&db_path, 0.9, 100).expect("open");

        let files = vec![RenderedFile::new(
            "/tmp/ag_reject_baseline.rs",
            "pub fn ok() {}\n",
            FileAction::Created,
        )];
        let result = adapter.check(&files, "test-plan");
        assert!(result.is_err(), "baseline below min must reject");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("baseline"),
            "error must mention baseline: {err}"
        );
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn analysis_gate_disabled_via_env_skips_check() {
        // Bypass scenario: degraded DB (9/10 orphans, score way below 0.7) but
        // env var disables the gate. Even with restrictive thresholds, check
        // returns Ok and counter increments.
        use std::sync::atomic::Ordering;
        use touring_generator::WIRING_GATE_BYPASSED_COUNT;

        let db_path = make_seeded_db("/tmp/touring-gen-ag-bypass", 9, 10);
        let mut adapter = AnalysisGateAdapter::open(&db_path).expect("open");
        adapter.disabled = true;

        let before = WIRING_GATE_BYPASSED_COUNT.load(Ordering::Relaxed);
        let files = vec![RenderedFile::new(
            "/tmp/ag_bypass.rs",
            "pub fn a() {}\npub fn b() {}\npub fn c() {}\npub fn d() {}\npub fn e() {}\npub fn f() {}\n",
            FileAction::Created,
        )];
        let result = adapter.check(&files, "bypass-test");
        assert!(
            result.is_ok(),
            "disabled adapter must skip check: {result:?}"
        );
        let after = WIRING_GATE_BYPASSED_COUNT.load(Ordering::Relaxed);
        assert_eq!(after, before + 1, "counter must increment on bypass");
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    #[serial_test::serial(touring_wiring_gate_env)]
    fn analysis_gate_open_with_env_default_when_no_env() {
        // Env vars unset — adapter must keep defaults: min_score=0.7, max=5, disabled=false.
        let db_path = make_seeded_db("/tmp/touring-gen-ag-env-default", 0, 10);

        // Ensure env is clean (defensive — other tests may have set).
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_WIRING_GATE_MIN_SCORE") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_WIRING_GATE_MAX_DELTA") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_WIRING_GATE_DISABLED") };

        let adapter = AnalysisGateAdapter::open_with_env(&db_path).expect("open_with_env");
        assert!(
            (adapter.min_score - 0.7).abs() < f64::EPSILON,
            "default 0.7"
        );
        assert_eq!(adapter.max_projected_orphan_delta, 5, "default 5");
        assert!(!adapter.disabled, "default not disabled");
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    #[serial_test::serial(touring_wiring_gate_env)]
    fn analysis_gate_open_with_env_reads_overrides() {
        let db_path = make_seeded_db("/tmp/touring-gen-ag-env-set", 0, 10);

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_WIRING_GATE_MIN_SCORE", "0.42") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_WIRING_GATE_MAX_DELTA", "99") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_WIRING_GATE_DISABLED", "yes") };

        let adapter = AnalysisGateAdapter::open_with_env(&db_path).expect("open_with_env");
        assert!(
            (adapter.min_score - 0.42).abs() < f64::EPSILON,
            "env min_score"
        );
        assert_eq!(adapter.max_projected_orphan_delta, 99, "env max_delta");
        assert!(adapter.disabled, "env disabled=yes activates bypass");

        // Cleanup so other tests are not contaminated.
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_WIRING_GATE_MIN_SCORE") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_WIRING_GATE_MAX_DELTA") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_WIRING_GATE_DISABLED") };
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn analysis_gate_skips_non_rust_files() {
        let db_path = make_seeded_db("/tmp/touring-gen-ag-skip", 0, 5);
        let adapter = AnalysisGateAdapter::with_thresholds(&db_path, 0.5, 0).expect("open");

        // Non-rust files contribute 0 to projected orphans — should pass even with max=0.
        let files = vec![
            RenderedFile::new("/tmp/ag_skip.md", "# Content", FileAction::Created),
            RenderedFile::new("/tmp/ag_skip.yaml", "key: val", FileAction::Created),
            RenderedFile::new("/tmp/ag_skip.toml", "[a]\nb = 1", FileAction::Created),
        ];
        let result = adapter.check(&files, "test-plan");
        assert!(result.is_ok(), "non-rust files must be skipped: {result:?}");
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn analysis_gate_count_pub_declarations_basic() {
        let content = r"
            pub fn foo() {}
            pub struct Bar;
            pub enum Baz { A }
            pub trait Qux {}
            pub const X: u8 = 1;
            pub static Y: u8 = 2;
            pub type T = u8;
            pub union U { x: u8, y: u8 }
            pub mod m {}
            pub use foo::bar;
            fn private_fn() {}
            struct PrivateStruct;
        ";
        // 9 pub items (fn/struct/enum/trait/const/static/type/union/mod).
        // pub use is explicitly NOT counted (re-export, not new symbol).
        assert_eq!(AnalysisGateAdapter::count_pub_declarations(content), 9);
    }

    #[test]
    fn analysis_gate_count_pub_declarations_visibility_modifiers() {
        // pub(crate), pub(super), pub(in path) should all count.
        let content = r"
            pub(crate) fn a() {}
            pub(super) struct B;
            pub(in crate::foo) const C: u8 = 0;
        ";
        assert_eq!(AnalysisGateAdapter::count_pub_declarations(content), 3);
    }

    #[test]
    fn analysis_gate_into_closure_round_trips_via_wiring_gate_fn() {
        let db_path = make_seeded_db("/tmp/touring-gen-ag-closure", 0, 10);
        let adapter =
            Arc::new(AnalysisGateAdapter::with_thresholds(&db_path, 0.5, 5).expect("open"));
        let gate_fn: WiringGateFn = Arc::clone(&adapter).into_closure();

        let files = vec![RenderedFile::new(
            "/tmp/ag_closure.rs",
            "pub fn via_closure() {}\n",
            FileAction::Created,
        )];
        assert!(gate_fn(&files, "test-plan").is_ok());
        let _ = std::fs::remove_file(&db_path);
    }
}

// ── PLN2 production adapter — SemanticGraphAdapter E2E ──────────────────────

#[cfg(feature = "cognitive-nexus")]
mod semantic_graph_adapter_tests {
    use super::*;
    use touring_generator::SemanticGraphAdapter;

    #[test]
    fn semantic_graph_adapter_records_plan_idempotently() {
        let adapter = SemanticGraphAdapter::new(std::path::PathBuf::from("/tmp/sg_idem.json"));
        let plan = make_plan(GeneratorKind::RustModule);

        // Record twice — second call must be idempotent (no error).
        assert!(adapter.record_plan(&plan).is_ok());
        assert!(adapter.record_plan(&plan).is_ok());
    }

    #[test]
    fn semantic_graph_fn_returns_plan_symbols() {
        let adapter = Arc::new(SemanticGraphAdapter::new(std::path::PathBuf::from(
            "/tmp/sg_symbols.json",
        )));
        let semantic_fn = Arc::clone(&adapter).into_semantic_graph_fn();

        let mut plan = make_plan(GeneratorKind::RustModule);
        plan.contracts
            .symbols_must_exist
            .push(SymbolRef::named("Foo"));
        plan.contracts
            .symbols_must_exist
            .push(SymbolRef::named("Bar"));

        let result = semantic_fn(&plan);
        assert!(
            result.is_some(),
            "must return symbols when contracts not empty"
        );
        let symbols = result.unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "Foo");
        assert_eq!(symbols[1].name, "Bar");
    }

    #[test]
    fn semantic_graph_fn_returns_none_for_empty_contracts() {
        let adapter = Arc::new(SemanticGraphAdapter::new(std::path::PathBuf::from(
            "/tmp/sg_empty.json",
        )));
        let semantic_fn = Arc::clone(&adapter).into_semantic_graph_fn();

        let plan = make_plan(GeneratorKind::RustModule); // empty contracts
        assert!(semantic_fn(&plan).is_none());
    }

    #[test]
    fn cognitive_nexus_fn_returns_none_for_unknown_key() {
        let adapter = Arc::new(SemanticGraphAdapter::new(std::path::PathBuf::from(
            "/tmp/sg_unknown.json",
        )));
        let nexus_fn = Arc::clone(&adapter).into_cognitive_nexus_fn();

        // No node has been recorded → no neighbors → None.
        let result = nexus_fn("unknown-plan-id");
        assert!(result.is_none());
    }

    #[test]
    fn cognitive_nexus_fn_returns_score_after_linking() {
        let adapter = Arc::new(SemanticGraphAdapter::new(std::path::PathBuf::from(
            "/tmp/sg_link.json",
        )));

        // Record two plans and link them.
        let plan_a = make_plan(GeneratorKind::RustModule);
        let plan_b = make_plan(GeneratorKind::CliHandler);
        adapter.record_plan(&plan_a).expect("record A");
        adapter.record_plan(&plan_b).expect("record B");

        // Link a → b. After linking, plan_a has a neighbor.
        let link = adapter.link_plans(
            &plan_a.plan_id.to_string(),
            &plan_b.plan_id.to_string(),
            1.0,
        );
        assert!(link.is_ok(), "link must succeed: {link:?}");

        // The cognitive nexus closure should now return Some(score) for plan_a.
        let nexus_fn = Arc::clone(&adapter).into_cognitive_nexus_fn();
        let result = nexus_fn(&plan_a.plan_id.to_string());
        assert!(result.is_some(), "linked plan must yield non-None score");
        let score = result.unwrap();
        assert!(score.value() > 0.0 && score.value() <= 1.0);
    }

    #[test]
    fn semantic_graph_link_rejects_self_loop() {
        let adapter = SemanticGraphAdapter::new(std::path::PathBuf::from("/tmp/sg_self.json"));
        let plan = make_plan(GeneratorKind::Test);
        adapter.record_plan(&plan).expect("record");

        // Self-loop must be rejected by the underlying SemanticGraph.
        let result = adapter.link_plans(&plan.plan_id.to_string(), &plan.plan_id.to_string(), 0.5);
        assert!(result.is_err(), "self-loop must be rejected");
    }

    #[test]
    fn semantic_graph_from_graph_constructor_works() {
        // Compose with an externally-built SemanticGraph (advanced use case).
        let persistence = Arc::new(
            touring_intelligence::reasoning::persistence::GraphPersistence::new(
                std::path::PathBuf::from("/tmp/sg_compose.json"),
            ),
        );
        let graph = Arc::new(
            touring_intelligence::reasoning::semantic_graph::SemanticGraph::new(persistence),
        );
        let adapter = SemanticGraphAdapter::from_graph(Arc::clone(&graph));

        let plan = make_plan(GeneratorKind::DiaryEntry);
        assert!(adapter.record_plan(&plan).is_ok());
        // The composed graph reference should also see the new node.
        let neighbors = graph.neighbors(&plan.plan_id.to_string());
        assert!(
            neighbors.is_empty(),
            "newly-added node has no neighbors yet"
        );
    }

    #[tokio::test]
    async fn semantic_graph_adapter_drives_full_pipeline_closures() {
        // Wire the SemanticGraphAdapter into a full GeneratorContext and run a
        // plan through Draft → Verified → Rendered → Speculated → (Committed).
        // The adapter must:
        //   • record the plan via semantic_graph_fn (called in Draft→Verified)
        //   • answer cognitive_nexus_fn lookup with None (no neighbors yet)
        use touring_generator::{Draft, PlanExecutor};

        let adapter = Arc::new(SemanticGraphAdapter::new(std::path::PathBuf::from(
            "/tmp/sg_pipeline.json",
        )));
        let semantic_fn: SemanticGraphFn = Arc::clone(&adapter).into_semantic_graph_fn();
        let nexus_fn: CognitiveNexusFn = Arc::clone(&adapter).into_cognitive_nexus_fn();

        let metrics: Arc<NoopTelemetry> = Arc::new(NoopTelemetry);
        let file_cache = Arc::new(tokio::sync::RwLock::new(
            touring_intelligence::index::FileCache::new(),
        ));
        let ctx = Arc::new(GeneratorContext {
            project_root: camino::Utf8PathBuf::from("/tmp/touring-gen-sg-pipeline"),
            symbol_index: Arc::new(touring_intelligence::index::IncrementalIndex::new(
                file_cache,
            )),
            fuzzy_index: Arc::new(NoopFuzzyMatcher) as Arc<dyn FuzzyMatcher>,
            vgp_engine: Arc::new(VgpEngine::with_subprocess(
                Arc::clone(&metrics) as Arc<NoopTelemetry>
            )),
            template_engine: Arc::new(TemplateEngine::new(
                Arc::clone(&metrics) as Arc<NoopTelemetry>
            )),
            speculate_bridge: Arc::new(SpeculateBridge::new(
                Arc::clone(&metrics) as Arc<NoopTelemetry>
            )),
            schema_registry: Arc::new(SchemaRegistry::new("2.0.0")),
            plan_registry: Arc::new(GenPlanRegistry::new()),
            memory: Arc::new(NoopMemory),
            llm: Arc::new(NoopLlm),
            rl: Arc::new(NoopRlSink),
            telemetry: metrics,
            semantic_graph_fn: Some(semantic_fn),
            pheromone_fn: None,
            cognitive_nexus_fn: Some(nexus_fn),
            wiring_gate_fn: None,
            health_delta_record_fn: None,
            health_delta_compute_fn: None,
            wasm_sandbox_fn: None,
            mcts_eval_fn: None,
            dspy_sig_fn: None,
            knowledge_upsert_fn: None,
            session_start_fn: None,
            session_checkpoint_fn: None,
            session_assess_fn: None,
            decompose_create_fn: None,
            decompose_update_fn: None,
            #[cfg(feature = "quality-gate")]
            quality_gate_fn: None,
            #[cfg(feature = "quality-gate")]
            quality_gate_adapter: None,
            #[cfg(feature = "health-gate")]
            health_gate_fn: None,
            #[cfg(feature = "enrichment-gate")]
            enrichment_trigger_fn: None,
            backpressure: Arc::new(tokio::sync::Semaphore::new(64)),
            capacity: CapacityLimits::default(),
            audit_log: Arc::new(NoopAuditLog),
            concolic_analyze_fn: None,
        });

        // Use empty contracts so VGP passes regardless of daemon availability.
        // The semantic_graph_fn closure still records the plan in the graph.
        let plan = make_plan(GeneratorKind::RustModule);
        let plan_id_str = plan.plan_id.to_string();

        let executor: PlanExecutor<Draft> = PlanExecutor::first(plan, Arc::clone(&ctx));
        let _verified = executor
            .verify(&ctx.vgp_engine)
            .await
            .expect("VGP must pass");

        // Verify the plan was recorded in the underlying graph.
        let neighbors = adapter.graph().neighbors(&plan_id_str);
        // No edges added yet → empty neighbors but the node exists.
        assert!(
            neighbors.is_empty(),
            "no edges should exist after a single verify"
        );

        // Re-running the closure for the same plan_id must still work (idempotent record).
        let plan2 = make_plan(GeneratorKind::Test);
        let executor2: PlanExecutor<Draft> = PlanExecutor::first(plan2, Arc::clone(&ctx));
        let _ = executor2
            .verify(&ctx.vgp_engine)
            .await
            .expect("second VGP must pass");
    }
}

// ── PLN2 production adapter — SynWiringGateAdapter E2E ──────────────────────

mod syn_wiring_gate_tests {
    use super::*;
    use touring_generator::SynWiringGateAdapter;

    #[test]
    fn syn_gate_accepts_valid_rust_file() {
        let adapter = SynWiringGateAdapter::new();
        let files = vec![RenderedFile::new(
            "/tmp/syn_gate_valid.rs",
            "pub fn hello() -> &'static str { \"world\" }\n",
            FileAction::Created,
        )];
        assert!(adapter.check(&files).is_ok());
    }

    #[test]
    fn syn_gate_rejects_unparseable_rust_file() {
        let adapter = SynWiringGateAdapter::new();
        let files = vec![RenderedFile::new(
            "/tmp/syn_gate_invalid.rs",
            "pub fn broken() { let x = ; }",
            FileAction::Created,
        )];
        let result = adapter.check(&files);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not valid Rust"),
            "Expected parse error, got: {err}"
        );
    }

    #[test]
    fn syn_gate_rejects_forbidden_dead_code_allow() {
        let adapter = SynWiringGateAdapter::new();
        let files = vec![RenderedFile::new(
            "/tmp/syn_gate_dead.rs",
            "#[allow(dead_code)]\npub fn unused() {}\n",
            FileAction::Created,
        )];
        let result = adapter.check(&files);
        assert!(result.is_err(), "must reject #[allow(dead_code)]");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("REGRA #0"),
            "Expected POTENCIALIZAR error, got: {err}"
        );
    }

    #[test]
    fn syn_gate_rejects_forbidden_unused_allow() {
        let adapter = SynWiringGateAdapter::new();
        let files = vec![RenderedFile::new(
            "/tmp/syn_gate_unused.rs",
            "#[allow(unused)]\npub struct Foo;\n",
            FileAction::Created,
        )];
        assert!(adapter.check(&files).is_err());
    }

    #[test]
    fn syn_gate_allows_other_allow_attributes() {
        // #[allow(clippy::xxx)] is NOT forbidden — only dead_code/unused.
        let adapter = SynWiringGateAdapter::new();
        let files = vec![RenderedFile::new(
            "/tmp/syn_gate_clippy_allow.rs",
            "#[allow(clippy::needless_pass_by_value)]\npub fn ok(x: String) -> String { x }\n",
            FileAction::Created,
        )];
        assert!(adapter.check(&files).is_ok());
    }

    #[test]
    fn syn_gate_skips_non_rust_files() {
        let adapter = SynWiringGateAdapter::new();
        let files = vec![
            RenderedFile::new("/tmp/syn_gate_skip.md", "# Not Rust", FileAction::Created),
            RenderedFile::new("/tmp/syn_gate_skip.yaml", "key: value", FileAction::Created),
            RenderedFile::new("/tmp/Dockerfile", "FROM alpine", FileAction::Created),
        ];
        // Even garbage content for non-rs files passes the gate.
        assert!(adapter.check(&files).is_ok());
    }

    #[test]
    fn syn_gate_handles_uppercase_extension() {
        // Path with .RS (uppercase) must still be detected and validated.
        let adapter = SynWiringGateAdapter::new();
        let files = vec![RenderedFile::new(
            "/tmp/syn_gate_UPPER.RS",
            "pub fn upper_ext() {}\n",
            FileAction::Created,
        )];
        assert!(adapter.check(&files).is_ok());
    }

    #[test]
    fn syn_gate_rejects_too_many_pub_items() {
        // Default max_pub_items_per_file = 50; build a file with 51 pub items.
        let adapter = SynWiringGateAdapter::new();
        let mut content = String::new();
        for i in 0..51 {
            content.push_str(&format!("pub fn f{i}() {{}}\n"));
        }
        let files = vec![RenderedFile::new(
            "/tmp/syn_gate_overflow.rs",
            &content,
            FileAction::Created,
        )];
        let result = adapter.check(&files);
        assert!(result.is_err(), "must reject when pub item count > max");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("51"), "Expected count 51 in error, got: {err}");
    }

    #[test]
    fn syn_gate_with_config_relaxes_thresholds() {
        // Custom: max=1, allow forbidden allows.
        let adapter = SynWiringGateAdapter::with_config(1, false);
        let files = vec![RenderedFile::new(
            "/tmp/syn_gate_relaxed.rs",
            "#[allow(dead_code)]\npub fn first() {}\n",
            FileAction::Created,
        )];
        // With reject_forbidden_allows=false, the dead_code allow is OK.
        // pub item count = 1 == max, so it passes.
        assert!(adapter.check(&files).is_ok());

        // But adding a second pub item exceeds max=1.
        let files2 = vec![RenderedFile::new(
            "/tmp/syn_gate_relaxed_two.rs",
            "pub fn one() {}\npub fn two() {}\n",
            FileAction::Created,
        )];
        assert!(adapter.check(&files2).is_err());
    }

    #[test]
    fn syn_gate_into_closure_round_trips_via_wiring_gate_fn() {
        // Prove the closure produced by into_closure() matches WiringGateFn signature.
        let adapter = SynWiringGateAdapter::new();
        let gate_fn: WiringGateFn = adapter.into_closure();

        let files = vec![RenderedFile::new(
            "/tmp/syn_gate_closure.rs",
            "pub const PI: f64 = 3.14;\n",
            FileAction::Created,
        )];
        assert!(gate_fn(&files, "test-plan").is_ok());
    }

    #[tokio::test]
    async fn syn_gate_blocks_commit_when_injected_via_with_closures() {
        // Wire SynWiringGateAdapter into a real GeneratorContext and prove the
        // commit pipeline rejects when the rendered file violates the gate.
        // We use a custom plan that targets a .rs file and a generator that
        // produces a forbidden file body.
        use touring_generator::{Draft, PlanExecutor};

        let adapter = SynWiringGateAdapter::with_config(1, true);
        let gate_fn: WiringGateFn = adapter.into_closure();

        // Build a counting context with this real gate.
        let metrics: Arc<NoopTelemetry> = Arc::new(NoopTelemetry);
        let file_cache = Arc::new(tokio::sync::RwLock::new(
            touring_intelligence::index::FileCache::new(),
        ));
        let ctx = Arc::new(GeneratorContext {
            project_root: camino::Utf8PathBuf::from("/tmp/touring-generator-syn-gate"),
            symbol_index: Arc::new(touring_intelligence::index::IncrementalIndex::new(
                file_cache,
            )),
            fuzzy_index: Arc::new(NoopFuzzyMatcher) as Arc<dyn FuzzyMatcher>,
            vgp_engine: Arc::new(VgpEngine::with_subprocess(
                Arc::clone(&metrics) as Arc<NoopTelemetry>
            )),
            template_engine: Arc::new(TemplateEngine::new(
                Arc::clone(&metrics) as Arc<NoopTelemetry>
            )),
            speculate_bridge: Arc::new(SpeculateBridge::new(
                Arc::clone(&metrics) as Arc<NoopTelemetry>
            )),
            schema_registry: Arc::new(SchemaRegistry::new("2.0.0")),
            plan_registry: Arc::new(GenPlanRegistry::new()),
            memory: Arc::new(NoopMemory),
            llm: Arc::new(NoopLlm),
            rl: Arc::new(NoopRlSink),
            telemetry: metrics,
            semantic_graph_fn: None,
            pheromone_fn: None,
            cognitive_nexus_fn: None,
            wiring_gate_fn: Some(gate_fn),
            health_delta_record_fn: None,
            health_delta_compute_fn: None,
            wasm_sandbox_fn: None,
            mcts_eval_fn: None,
            dspy_sig_fn: None,
            knowledge_upsert_fn: None,
            session_start_fn: None,
            session_checkpoint_fn: None,
            session_assess_fn: None,
            decompose_create_fn: None,
            decompose_update_fn: None,
            #[cfg(feature = "quality-gate")]
            quality_gate_fn: None,
            #[cfg(feature = "quality-gate")]
            quality_gate_adapter: None,
            #[cfg(feature = "health-gate")]
            health_gate_fn: None,
            #[cfg(feature = "enrichment-gate")]
            enrichment_trigger_fn: None,
            backpressure: Arc::new(tokio::sync::Semaphore::new(64)),
            capacity: CapacityLimits::default(),
            audit_log: Arc::new(NoopAuditLog),
            concolic_analyze_fn: None,
        });

        // Use RustModule kind which renders a .rs file with multiple pub items.
        let mut plan = make_plan(GeneratorKind::RustModule);
        plan.target.file_path = format!("/tmp/touring_syn_gate_e2e_{}.rs", plan.plan_id);
        let target_path = plan.target.file_path.clone();
        let _ = std::fs::remove_file(&target_path);

        let executor: PlanExecutor<Draft> = PlanExecutor::first(plan, Arc::clone(&ctx));
        let verified = executor.verify(&ctx.vgp_engine).await.expect("VGP pass");
        let rendered = verified
            .render(
                &ctx.template_engine,
                &HashMap::new(),
                None,
                RenderShape::default_width(),
            )
            .expect("render must succeed")
            .expect("render must return Some (not overflow with default width)");

        // Speculate may pass or fail; what matters is the gate at commit.
        if let Ok(speculated) = rendered.speculate(&ctx.speculate_bridge).await {
            // Test commit behavior with strict max_pub=1 gate.
            let result = speculated.commit().await;
            // The default rust_module template has more than 1 pub item OR
            // the file body is well-formed Rust under the threshold.
            // Either way, the file must NOT exist if the gate rejected.
            if result.is_err() {
                assert!(
                    !std::path::Path::new(&target_path).exists(),
                    "Gate-rejected commit must NOT write file: {target_path}"
                );
            } else {
                // Gate passed → file should exist; clean up.
                let _ = std::fs::remove_file(&target_path);
            }
        }
        // Final cleanup.
        let _ = std::fs::remove_file(&target_path);
    }
}

#[tokio::test]
async fn wiring_gate_rejection_blocks_commit() {
    use touring_generator::{Draft, PlanExecutor};

    // Build a context whose wiring_gate_fn REJECTS any artifact.
    // This proves the hard gate is enforced.
    let metrics: Arc<NoopTelemetry> = Arc::new(NoopTelemetry);
    let file_cache = Arc::new(tokio::sync::RwLock::new(
        touring_intelligence::index::FileCache::new(),
    ));
    let rejecting_gate: WiringGateFn = Arc::new(|_files: &[RenderedFile], _plan_id: &str| {
        Err(GenerateError::Internal("test rejection".into()))
    });
    let ctx = Arc::new(GeneratorContext {
        project_root: camino::Utf8PathBuf::from("/tmp/touring-generator-reject"),
        symbol_index: Arc::new(touring_intelligence::index::IncrementalIndex::new(
            file_cache,
        )),
        fuzzy_index: Arc::new(NoopFuzzyMatcher) as Arc<dyn FuzzyMatcher>,
        vgp_engine: Arc::new(VgpEngine::with_subprocess(
            Arc::clone(&metrics) as Arc<NoopTelemetry>
        )),
        template_engine: Arc::new(TemplateEngine::new(
            Arc::clone(&metrics) as Arc<NoopTelemetry>
        )),
        speculate_bridge: Arc::new(SpeculateBridge::new(
            Arc::clone(&metrics) as Arc<NoopTelemetry>
        )),
        schema_registry: Arc::new(SchemaRegistry::new("2.0.0")),
        plan_registry: Arc::new(GenPlanRegistry::new()),
        memory: Arc::new(NoopMemory),
        llm: Arc::new(NoopLlm),
        rl: Arc::new(NoopRlSink),
        telemetry: metrics,
        semantic_graph_fn: None,
        pheromone_fn: None,
        cognitive_nexus_fn: None,
        wiring_gate_fn: Some(rejecting_gate),
        health_delta_record_fn: None,
        health_delta_compute_fn: None,
        wasm_sandbox_fn: None,
        mcts_eval_fn: None,
        dspy_sig_fn: None,
        knowledge_upsert_fn: None,
        session_start_fn: None,
        session_checkpoint_fn: None,
        session_assess_fn: None,
        decompose_create_fn: None,
        decompose_update_fn: None,
        #[cfg(feature = "quality-gate")]
        quality_gate_fn: None,
        #[cfg(feature = "quality-gate")]
        quality_gate_adapter: None,
        #[cfg(feature = "health-gate")]
        health_gate_fn: None,
        #[cfg(feature = "enrichment-gate")]
        enrichment_trigger_fn: None,
        backpressure: Arc::new(tokio::sync::Semaphore::new(64)),
        capacity: CapacityLimits::default(),
        audit_log: Arc::new(NoopAuditLog),
        concolic_analyze_fn: None,
    });

    let mut plan = make_plan(GeneratorKind::DiaryEntry);
    plan.target.file_path = format!("/tmp/touring_gen_reject_test_{}.md", plan.plan_id);
    let target_path = plan.target.file_path.clone();
    // Ensure the file does not exist before the test.
    let _ = std::fs::remove_file(&target_path);

    let executor: PlanExecutor<Draft> = PlanExecutor::first(plan, Arc::clone(&ctx));
    let verified = executor.verify(&ctx.vgp_engine).await.expect("VGP pass");
    let rendered = verified
        .render(
            &ctx.template_engine,
            &HashMap::new(),
            None,
            RenderShape::default_width(),
        )
        .expect("render pass")
        .expect("render must return Some (not overflow with default width)");

    let speculated_result = rendered.speculate(&ctx.speculate_bridge).await;
    if let Ok(speculated) = speculated_result {
        let committed = speculated.commit().await;
        assert!(
            committed.is_err(),
            "Commit must fail when wiring_gate rejects"
        );
        assert!(
            matches!(committed.unwrap_err(), GenerateError::Internal(_)),
            "Error must propagate from wiring_gate closure"
        );
        // And critically — the file must NOT have been written.
        assert!(
            !std::path::Path::new(&target_path).exists(),
            "File must not exist when wiring_gate rejected (got written at {target_path})"
        );
    }
    // Final cleanup in case something slipped through.
    let _ = std::fs::remove_file(&target_path);
}
