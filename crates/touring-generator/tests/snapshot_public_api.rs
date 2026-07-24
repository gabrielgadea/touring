//! Snapshot tests locking the public API shape of touring-generator.
//!
//! Rationale: GeneratorKind is the contract with the LLM planner. A silent
//! variant addition/removal breaks downstream plan-submit consumers without
//! any compile-time signal (the enum is `#[non_exhaustive]`). These snapshots
//! force `cargo insta review` approval on any shape change.
//!
//! Review new snapshots with: `cargo insta review -p touring-generator`.
//!
//! Pairs with `all_kinds()` in `tests/e2e_pipeline.rs` — that test verifies
//! runtime behavior per kind; this file locks the set itself.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::pedantic)]

use schemars::schema_for;
use touring_generator::GeneratorKind;
use touring_generator::plan::failure::{FailureReason, NextAction, SuggestionSource};

fn debug_sorted<T: core::fmt::Debug>(items: impl IntoIterator<Item = T>) -> Vec<String> {
    let mut v: Vec<String> = items.into_iter().map(|x| format!("{x:?}")).collect();
    v.sort();
    v
}

#[test]
fn snapshot_generator_kinds_all_32_variants() {
    // Source of truth: crates/touring-generator/src/generator/kinds.rs
    // When adding a variant:
    //   1. Append to this array
    //   2. Run `cargo test -p touring-generator --test snapshot_public_api`
    //   3. Review with `cargo insta review -p touring-generator`
    let kinds = [
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
        GeneratorKind::DockerImage,
        GeneratorKind::KubernetesManifest,
        GeneratorKind::TerraformModule,
        GeneratorKind::CiWorkflow,
        GeneratorKind::ConsumerGenerator,
        GeneratorKind::TaskScaffold,
    ];
    assert_eq!(kinds.len(), 32, "kinds.rs documents 32 artifact kinds");
    insta::assert_yaml_snapshot!("generator_kinds_all_variants", debug_sorted(kinds));
}

#[test]
fn snapshot_suggestion_source_variants() {
    let sources = [
        SuggestionSource::SimdTopK,
        SuggestionSource::TrigramIndex,
        SuggestionSource::MemoryRecall,
        SuggestionSource::LlmHypothesis,
    ];
    insta::assert_yaml_snapshot!("suggestion_source_variants", debug_sorted(sources));
}

#[test]
fn snapshot_failure_reason_json_schema() {
    // JsonSchema captures payload fields — detects adds/removes/renames
    // that simple variant-list snapshots would miss.
    let schema = schema_for!(FailureReason);
    let json = serde_json::to_string_pretty(&schema).expect("serialize FailureReason schema");
    insta::assert_snapshot!("failure_reason_schema", json);
}

#[test]
fn snapshot_next_action_json_schema() {
    let schema = schema_for!(NextAction);
    let json = serde_json::to_string_pretty(&schema).expect("serialize NextAction schema");
    insta::assert_snapshot!("next_action_schema", json);
}
