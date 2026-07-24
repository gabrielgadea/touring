//! Pure path-pattern hint helpers for the `file-changed` lifecycle hook.
//!
//! Each function maps a relative file path to an optional advisory hint string.
//! All helpers are stateless pure functions — no `HookRuntime` dependency.
//! Visibility is `pub(super)` so only `file_changed::mod.rs` can call them.
//!
//! # Catalog (30 helpers + 6 shared path helpers + 2 aggregators)
//!
//! The 30 `maybe_*_hint_on_file_changed` helpers correspond 1:1 to the 30
//! `GeneratorKind` variants defined in `touring-generator`. Each fires when the
//! changed file path matches markers associated with that generator kind.
//!
//! Additional helpers:
//! - `maybe_new_file_hint` — untracked .rs discovery hint
//! - `maybe_plan_json_hint` — generator plan JSON validation hint
//! - `maybe_tera_template_hint` — Tera template change hint
//! - `maybe_index_stale_hint` — VGP index rebuild hint for .rs with dependents
//! - `maybe_wiring_chains_hint_for_handler_file` — wiring chains hint for hook files
//! - `maybe_test_file_hint` — test generator + cargo test hint
//! - `maybe_cargo_toml_hint` — feature gate + wiring audit hint
//! - `collect_path_pattern_warnings` — aggregates all 30 `_on_file_changed` hints

// ── Imports ──────────────────────────────────────────────────────────────────

// Access `file_stem` and `maybe_generator_kind_hint` re-exported from lifecycle.rs
// via the grandparent (`super::super::`). Rust resolves this through the re-export
// chain: lifecycle.rs → pub(crate) use shared::{file_stem, ...}.
use super::super::file_stem;

// ── Shared path helpers ───────────────────────────────────────────────────────

/// R10-A: For new/untracked .rs files, surface Tantivy discovery + plan-recall hints.
pub(crate) fn maybe_new_file_hint(
    warnings_empty: bool,
    has_dependents: bool,
    rel_path: &str,
) -> Option<String> {
    if warnings_empty && !has_dependents && rel_path.ends_with(".rs") {
        let stem = file_stem(rel_path);
        Some(format!(
            "discovery: run `touring tantivy search \"{stem}\"` to find related symbols | \
            run `touring generate plan-recall --query \"{stem}\"` to resume past plans for this module"
        ))
    } else {
        None
    }
}

/// R14-S3: Detect generator plan JSON changes and surface pipeline validation hints.
pub(crate) fn maybe_plan_json_hint(rel_path: &str) -> Option<String> {
    if rel_path.ends_with(".json") && (rel_path.contains("plan") || rel_path.contains("generate")) {
        Some(format!(
            "generator: plan file changed — run `touring generate plan-validate --plan-file {rel_path}` \
            then `touring generate plan-status --plan-file {rel_path}` to verify pipeline state"
        ))
    } else {
        None
    }
}

/// R26-S1: Detect Tera template file changes and surface validate + test commands (CC=2).
///
/// When a `.tera` template is edited, this injects the full generator pipeline:
/// validate syntax first, then template-test to confirm rendering with sample vars.
/// Closes the generator-hooks loop for template authoring workflows.
pub(crate) fn maybe_tera_template_hint(rel_path: &str) -> Option<String> {
    if !rel_path.ends_with(".tera") {
        return None;
    }
    // Extract template name from path (e.g. "templates/rust_module.tera" → "rust_module.tera")
    let template_name = rel_path.rsplit('/').next().unwrap_or(rel_path);
    Some(format!(
        "template-changed: run `touring generate template-validate --template-file {rel_path}` \
        to verify syntax | run `touring generate template-test --template \"{template_name}\"` \
        to confirm rendering | run `touring generate template-list` to see all 29 templates"
    ))
}

/// R41-S1: When a .rs file with dependents changes, emit targeted index rebuild hint (CC≤3).
///
/// A changed source file with dependent consumers means the symbol graph may be stale.
/// VGP `generate verify --symbol` may miss newly-added or renamed exports until the
/// crate index is refreshed. Emits a `touring index rebuild --dir <crate_dir>` hint
/// scoped to the owning crate directory to minimize rebuild scope.
pub(crate) fn maybe_index_stale_hint(rel_path: &str, has_dependents: bool) -> Option<String> {
    if !rel_path.ends_with(".rs") || !has_dependents {
        return None;
    }
    // Derive crate dir: first two slash-separated components (e.g. "crates/touring-hooks").
    let crate_dir = rel_path
        .splitn(3, '/')
        .take(2)
        .collect::<Vec<_>>()
        .join("/");
    let dir_arg = if crate_dir.is_empty() {
        ".".to_string()
    } else {
        crate_dir
    };
    Some(format!(
        "index-stale: {rel_path} changed with dependents — run \
        `touring index rebuild --dir {dir_arg}` to refresh VGP symbol graph"
    ))
}

/// R44-S1: Emit `touring wiring chains` hint when a hook handler file changes (CC≤3).
///
/// Hook handler files (lifecycle.rs, hook_registry.rs, cli_handlers*.rs) participate in
/// functional chains between Claude Code events and Touring intelligence. When any of these
/// change, the functional chains they anchor may shift. This helper emits a `touring wiring
/// chains <rel_path>` command so the engineer can inspect chain membership before and after
/// the edit. Returns `None` for files that are not handler/lifecycle/registry sources.
pub(crate) fn maybe_wiring_chains_hint_for_handler_file(rel_path: &str) -> Option<String> {
    let lower = rel_path.to_lowercase();
    let is_handler = lower.ends_with(".rs")
        && (lower.contains("lifecycle")
            || lower.contains("hook_registry")
            || lower.contains("cli_handler"));
    if !is_handler {
        return None;
    }
    Some(format!(
        "chains: run `touring wiring chains {rel_path}` to inspect \
        functional chain membership after this handler change"
    ))
}

/// R70: Group the 3 file-path-pattern matchers (asyncapi/error-catalog/adr) into one iterator
/// so `handle_file_changed` pays zero CC for them (CC=0 at call site — single `extend` call).
pub(crate) fn collect_path_pattern_warnings(
    rel_path: &str,
) -> impl Iterator<Item = String> + use<> {
    [
        maybe_asyncapi_hint_on_file_changed(rel_path),
        maybe_error_catalog_hint_on_file_changed(rel_path),
        maybe_adr_hint_on_file_changed(rel_path),
        // R77: terraform, ci_workflow, k8s_manifest — infra files need scaffold hints on change
        maybe_terraform_hint_on_file_changed(rel_path),
        maybe_ci_workflow_hint_on_file_changed(rel_path),
        maybe_k8s_hint_on_file_changed(rel_path),
        // R80: rust_module, test, schema — source/test/schema files surface generator hints
        maybe_rust_module_hint_on_file_changed(rel_path),
        maybe_test_hint_on_file_changed(rel_path),
        maybe_schema_hint_on_file_changed(rel_path),
        // R95: migration, protobuf, dockerfile — db/proto/container files surface scaffold hints
        maybe_migration_hint_on_file_changed(rel_path),
        maybe_protobuf_hint_on_file_changed(rel_path),
        maybe_dockerfile_hint_on_file_changed(rel_path),
        // R101: openapi, shell_completion, changelog — API/shell/release files surface scaffold hints
        maybe_openapi_hint_on_file_changed(rel_path),
        maybe_shell_completion_hint_on_file_changed(rel_path),
        maybe_changelog_hint_on_file_changed(rel_path),
        // R104: ffi_binding, python_script, benchmark — native/scripting/perf files surface scaffold hints
        maybe_ffi_binding_hint_on_file_changed(rel_path),
        maybe_python_script_hint_on_file_changed(rel_path),
        maybe_benchmark_hint_on_file_changed(rel_path),
        // R107: fuzz_target, derive_macro, incremental_patch — fuzzing/macro/patch files surface scaffold hints
        maybe_fuzz_target_hint_on_file_changed(rel_path),
        maybe_derive_macro_hint_on_file_changed(rel_path),
        maybe_incremental_patch_hint_on_file_changed(rel_path),
        // R110: cli_handler, mcp_tool, hook_handler — CLI/MCP/hook files surface scaffold hints
        maybe_cli_handler_hint_on_file_changed(rel_path),
        maybe_mcp_tool_hint_on_file_changed(rel_path),
        maybe_hook_handler_hint_on_file_changed(rel_path),
        // R113: plan_md, man_page, skill_document — planning/docs/skill files surface scaffold hints. FileChanged 27/30.
        maybe_plan_md_hint_on_file_changed(rel_path),
        maybe_man_page_hint_on_file_changed(rel_path),
        maybe_skill_document_hint_on_file_changed(rel_path),
        // R114: diary_entry, consumer_generator, task_scaffold — agent memory/consumer/task files. FileChanged 30/30 COMPLETE.
        maybe_diary_entry_hint_on_file_changed(rel_path),
        maybe_consumer_generator_hint_on_file_changed(rel_path),
        maybe_task_scaffold_hint_on_file_changed(rel_path),
    ]
    .into_iter()
    .flatten()
}

/// R29-S2: Detect test file changes and surface Test generator + cargo test reminder (CC≤3).
///
/// Matches paths that indicate a Rust test file:
/// - Contains `/tests/` or `/test/` directory component (integration test directories)
/// - Ends with `_test.rs` or `_tests.rs` (Rust inline test naming convention)
///
/// When matched, emits a `touring generate render Test` command with the file stem as
/// `module_name` var, plus a `cargo test <stem>` reminder. This closes the test-authoring
/// loop: Claude Code edits a test → hook immediately surfaces how to expand coverage.
/// Returns `None` for production source files so there's no noise on regular edits.
pub(crate) fn maybe_test_file_hint(rel_path: &str) -> Option<String> {
    let is_test = rel_path.contains("/tests/")
        || rel_path.contains("/test/")
        || rel_path.ends_with("_test.rs")
        || rel_path.ends_with("_tests.rs");
    if !is_test {
        return None;
    }
    let stem = file_stem(rel_path);
    Some(format!(
        "test-changed: run `touring generate render Test --vars '{{\"module_name\":\"{stem}\"}}' ` \
        to scaffold more tests | run `cargo test {stem}` to verify"
    ))
}

/// R33-S1: Detect Cargo.toml changes and surface feature-gate + wiring audit commands (CC=2).
///
/// When `Cargo.toml` is modified, feature-gated adapters and crate dependency graphs may shift.
/// This hook fires immediately after the edit so the engineer can verify:
/// 1. Feature gate correctness via `cargo check --all-features`
/// 2. Wiring integration via `touring wiring audit` (orphan + module score deltas)
/// 3. Generator adapter status via `touring generate capacity`
///
/// Returns `None` for non-Cargo.toml paths to avoid noise on routine edits.
pub(crate) fn maybe_cargo_toml_hint(rel_path: &str) -> Option<String> {
    if !rel_path.ends_with("Cargo.toml") {
        return None;
    }
    Some(
        "cargo-toml-changed: run `cargo check --all-features` to verify feature gates | \
        run `touring wiring audit -j | head -5` to detect new orphans | \
        run `touring generate capacity -j` to confirm all 10 generator adapters active"
            .to_string(),
    )
}

// ── 30 GeneratorKind path-pattern hints ──────────────────────────────────────

/// R70-S1: Emit AsyncApiSpec hint when a file path suggests event-driven API patterns (CC=2).
///
/// Matches `/events/`, `/channels/`, `/brokers/`, or `asyncapi` in the path so that editing
/// an event-schema file immediately surfaces `touring generate render AsyncApiSpec`.
pub(crate) fn maybe_asyncapi_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const ASYNCAPI_MARKERS: &[&str] = &[
        "/events/",
        "/channels/",
        "/brokers/",
        "asyncapi",
        "event-driven",
        "pubsub",
        "message-broker",
    ];
    let lower = rel_path.to_lowercase();
    let matches = ASYNCAPI_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "asyncapi: {rel_path} suggests event-driven API pattern — run \
        `touring generate render AsyncApiSpec --vars '{{\"title\":\"events\"}}'` \
        to scaffold an AsyncAPI spec via touring-generator"
    ))
}

/// R70-S2: Emit ErrorCatalog hint when a file path suggests error type definitions (CC=2).
///
/// Matches `/errors/`, `error_types`, `errors.rs`, `error_catalog`, or `error_codes` in the
/// path so that editing an error module immediately surfaces `touring generate render ErrorCatalog`.
pub(crate) fn maybe_error_catalog_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const ERROR_MARKERS: &[&str] = &[
        "/errors/",
        "error_types",
        "errors.rs",
        "error_catalog",
        "error_codes",
        "error_variants",
    ];
    let lower = rel_path.to_lowercase();
    let matches = ERROR_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "error-catalog: {rel_path} suggests error type definitions — run \
        `touring generate render ErrorCatalog --vars '{{\"crate_name\":\"errors\"}}'` \
        to scaffold an error catalog via touring-generator"
    ))
}

/// R70-S3: Emit Adr hint when a file path suggests an architectural decision record (CC=2).
///
/// Matches `/decisions/`, `/adr/`, `decision-record`, or `architecture-decision` in the path
/// so that editing a decision file immediately surfaces `touring generate render Adr`.
pub(crate) fn maybe_adr_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const ADR_MARKERS: &[&str] = &[
        "/decisions/",
        "/adr/",
        "decision-record",
        "architecture-decision",
        "adr-",
    ];
    let lower = rel_path.to_lowercase();
    let matches = ADR_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "adr: {rel_path} suggests an architectural decision record — run \
        `touring generate render Adr --vars '{{\"title\":\"decision\"}}'` \
        to scaffold an ADR via touring-generator"
    ))
}

/// R77-S1: Emit TerraformModule hint when a file path suggests infrastructure-as-code (CC=2).
///
/// Matches `.tf`, `/terraform/`, `/infra/`, `main.tf`, or `variables.tf` in the path so that
/// editing a Terraform file immediately surfaces `touring generate render TerraformModule`.
pub(crate) fn maybe_terraform_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const TF_MARKERS: &[&str] = &[
        ".tf",
        "/terraform/",
        "/infra/",
        "main.tf",
        "variables.tf",
        "outputs.tf",
        "provider.tf",
    ];
    let lower = rel_path.to_lowercase();
    let matches = TF_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "terraform: {rel_path} suggests infrastructure-as-code — run \
        `touring generate render TerraformModule --vars '{{\"module_name\":\"infra\"}}'` \
        to scaffold a Terraform module via touring-generator"
    ))
}

/// R77-S2: Emit CiWorkflow hint when a file path suggests a CI/CD pipeline definition (CC=2).
///
/// Matches `.github/workflows/`, `.gitlab-ci`, `Jenkinsfile`, or `ci.yml` in the path so that
/// editing a CI file immediately surfaces `touring generate render CiWorkflow`.
pub(crate) fn maybe_ci_workflow_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const CI_MARKERS: &[&str] = &[
        ".github/workflows/",
        ".gitlab-ci",
        "jenkinsfile",
        "ci.yml",
        "ci.yaml",
        "pipeline.yml",
        "pipeline.yaml",
    ];
    let lower = rel_path.to_lowercase();
    let matches = CI_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "ci-workflow: {rel_path} suggests a CI/CD pipeline — run \
        `touring generate render CiWorkflow --vars '{{\"pipeline_name\":\"ci\"}}'` \
        to scaffold a CI workflow via touring-generator"
    ))
}

/// R77-S3: Emit K8sManifest hint when a file path suggests Kubernetes resource definitions (CC=2).
///
/// Matches `/k8s/`, `/kubernetes/`, `/manifests/`, `deployment.yaml`, or `service.yaml` in the
/// path so that editing a manifest file immediately surfaces `touring generate render K8sManifest`.
pub(crate) fn maybe_k8s_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const K8S_MARKERS: &[&str] = &[
        "/k8s/",
        "/kubernetes/",
        "/manifests/",
        "deployment.yaml",
        "service.yaml",
        "ingress.yaml",
        "configmap.yaml",
    ];
    let lower = rel_path.to_lowercase();
    let matches = K8S_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "k8s-manifest: {rel_path} suggests Kubernetes resource definitions — run \
        `touring generate render K8sManifest --vars '{{\"resource_name\":\"service\"}}'` \
        to scaffold a K8s manifest via touring-generator"
    ))
}

/// R80-S1: Emit RustModule hint when a file path suggests a Rust module entry point (CC=2).
///
/// Matches `mod.rs`, `lib.rs`, `main.rs`, or `/src/` Rust files so that editing a module
/// root immediately surfaces `touring generate render RustModule`.
pub(crate) fn maybe_rust_module_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const RUST_MARKERS: &[&str] = &["mod.rs", "lib.rs", "main.rs", "/src/", "crate/src", ".rs"];
    let lower = rel_path.to_lowercase();
    // Only match if it's actually a Rust file (ends in .rs or is a known entry point)
    if !lower.ends_with(".rs") {
        return None;
    }
    let matches = RUST_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "rust-module: {rel_path} is a Rust source file — run \
        `touring generate render RustModule --vars '{{\"module_name\":\"module\"}}'` \
        to scaffold a new Rust module via touring-generator"
    ))
}

/// R80-S2: Emit Test hint when a file path suggests a test module (CC=2).
///
/// Matches `_test.rs`, `test_`, `tests/`, or `#[cfg(test)]` patterns so that editing
/// a test file immediately surfaces `touring generate render Test`.
pub(crate) fn maybe_test_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const TEST_MARKERS: &[&str] = &[
        "_test.rs",
        "test_",
        "/tests/",
        "tests.rs",
        "test_helpers",
        "test_utils",
    ];
    let lower = rel_path.to_lowercase();
    let matches = TEST_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "test: {rel_path} is a test file — run \
        `touring generate render Test --vars '{{\"module_name\":\"module\"}}'` \
        to scaffold additional test coverage via touring-generator"
    ))
}

/// R80-S3: Emit Schema hint when a file path suggests a schema definition file (CC=2).
///
/// Matches `schema`, `openrpc`, `jsonschema`, `.schema.json` patterns so that editing
/// a schema file immediately surfaces `touring generate render Schema`.
pub(crate) fn maybe_schema_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const SCHEMA_MARKERS: &[&str] = &[
        "schema.rs",
        "schema.json",
        "schema.yaml",
        "/schema/",
        "schemas/",
        "openrpc",
        ".schema.",
    ];
    let lower = rel_path.to_lowercase();
    let matches = SCHEMA_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "schema: {rel_path} suggests a schema definition — run \
        `touring generate render Schema --vars '{{\"schema_name\":\"schema\"}}'` \
        to scaffold a schema definition via touring-generator"
    ))
}

/// R95-S1: Emit Migration hint when a file path suggests database migration files (CC=2).
///
/// Matches `.sql`, `/migrations/`, `migrate_`, or `migration.rs` so that editing a migration
/// file immediately surfaces `touring generate render Migration`.
pub(crate) fn maybe_migration_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const MIGRATION_MARKERS: &[&str] = &[
        ".sql",
        "/migrations/",
        "migrate_",
        "migration.rs",
        "/migrate/",
        "db_migration",
        "schema_migration",
    ];
    let lower = rel_path.to_lowercase();
    let matches = MIGRATION_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "migration: {rel_path} suggests a database migration — run \
        `touring generate render Migration --vars '{{\"migration_name\":\"migration\"}}'` \
        to scaffold a migration via touring-generator"
    ))
}

/// R95-S2: Emit ProtobufSchema hint when a file path suggests Protocol Buffer definitions (CC=2).
///
/// Matches `.proto`, `/proto/`, `grpc`, or `protobuf` in the path so that editing a proto
/// file immediately surfaces `touring generate render ProtobufSchema`.
pub(crate) fn maybe_protobuf_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const PROTO_MARKERS: &[&str] = &[
        ".proto",
        "/proto/",
        "grpc",
        "protobuf",
        "/protos/",
        "proto_",
        "protocol_buffer",
    ];
    let lower = rel_path.to_lowercase();
    let matches = PROTO_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "protobuf-schema: {rel_path} suggests Protocol Buffer definitions — run \
        `touring generate render ProtobufSchema --vars '{{\"service_name\":\"service\"}}'` \
        to scaffold a protobuf schema via touring-generator"
    ))
}

/// R95-S3: Emit Dockerfile hint when a file path suggests container build definitions (CC=2).
///
/// Matches `Dockerfile`, `/docker/`, `docker-compose`, or `.dockerfile` in the path so that
/// editing a container file immediately surfaces `touring generate render Dockerfile`.
pub(crate) fn maybe_dockerfile_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const DOCKER_MARKERS: &[&str] = &[
        "dockerfile",
        "/docker/",
        "docker-compose",
        ".dockerfile",
        "containerfile",
        "docker_build",
    ];
    let lower = rel_path.to_lowercase();
    let matches = DOCKER_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "dockerfile: {rel_path} suggests container build definitions — run \
        `touring generate render Dockerfile --vars '{{\"app_name\":\"app\"}}'` \
        to scaffold a Dockerfile via touring-generator"
    ))
}

/// R104-S1: Emit FfiBinding hint when a file path suggests FFI/native binding files (CC=2).
pub(crate) fn maybe_ffi_binding_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const FFI_MARKERS: &[&str] = &[
        "/ffi/",
        "bindings.rs",
        "_bindings.rs",
        "sys.rs",
        "/sys/",
        "native/",
        "extern_",
        "ffi_",
        "libffi",
    ];
    let lower = rel_path.to_lowercase();
    let matches = FFI_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "ffi-binding: {rel_path} suggests FFI/native bindings — run \
        `touring generate render FfiBinding` to scaffold FFI bindings via touring-generator"
    ))
}

/// R104-S2: Emit PythonScript hint when a file path suggests Python scripting files (CC=2).
pub(crate) fn maybe_python_script_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const PY_MARKERS: &[&str] = &[
        ".py",
        "scripts/",
        "automation/",
        "python/",
        "pyscript",
        ".pyw",
        "pyproject",
    ];
    let lower = rel_path.to_lowercase();
    let matches = PY_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "python-script: {rel_path} suggests Python scripts — run \
        `touring generate render PythonScript` to scaffold a Python script via touring-generator"
    ))
}

/// R104-S3: Emit Benchmark hint when a file path suggests criterion benchmark files (CC=2).
pub(crate) fn maybe_benchmark_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const BENCH_MARKERS: &[&str] = &[
        "benches/",
        "benchmark",
        "bench_",
        "_bench.rs",
        "criterion",
        "perf_test",
        "microbench",
    ];
    let lower = rel_path.to_lowercase();
    let matches = BENCH_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "benchmark: {rel_path} suggests criterion benchmarks — run \
        `touring generate render Benchmark` to scaffold a benchmark via touring-generator"
    ))
}

/// R110-S1: Emit CliHandler hint when a file path suggests CLI handler/command files (CC=2).
pub(crate) fn maybe_cli_handler_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const CLI_MARKERS: &[&str] = &[
        "cli/",
        "handlers/cli",
        "command_table",
        "cli_handler",
        "cli_handlers",
        "clap_app",
        "subcommand",
        "daemon_query",
    ];
    let lower = rel_path.to_lowercase();
    let matches = CLI_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "cli-handler: {rel_path} suggests CLI handler code — run \
        `touring generate render CliHandler` to scaffold a CLI handler via touring-generator"
    ))
}

/// R110-S2: Emit McpTool hint when a file path suggests MCP tool/server files (CC=2).
pub(crate) fn maybe_mcp_tool_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const MCP_MARKERS: &[&str] = &[
        "mcp_tool",
        "mcp/tools",
        "tools_mcp",
        "rmcp",
        "model_context",
        "mcp_server",
        "mcp_tools",
        "#[tool]",
    ];
    let lower = rel_path.to_lowercase();
    let matches = MCP_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "mcp-tool: {rel_path} suggests MCP tool code — run \
        `touring generate render McpTool` to scaffold an MCP tool via touring-generator"
    ))
}

/// R110-S3: Emit HookHandler hint when a file path suggests lifecycle hook handler files (CC=2).
pub(crate) fn maybe_hook_handler_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const HOOK_MARKERS: &[&str] = &[
        "hook_handler",
        "lifecycle.rs",
        "hook_registry",
        "cli_handlers",
        "post_read",
        "pre_edit",
        "post_edit",
        "session_start",
    ];
    let lower = rel_path.to_lowercase();
    let matches = HOOK_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "hook-handler: {rel_path} suggests hook handler code — run \
        `touring generate render HookHandler` to scaffold a hook handler via touring-generator"
    ))
}

/// R113-S1: Emit PlanMd hint when a file path suggests project plan/roadmap markdown files (CC=2).
pub(crate) fn maybe_plan_md_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const PLAN_MARKERS: &[&str] = &[
        "plan.md",
        "plans/",
        "roadmap.md",
        "todo.md",
        "planning/",
        "sprint_plan",
        "project_plan",
        "milestone",
    ];
    let lower = rel_path.to_lowercase();
    let matches = PLAN_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "plan-md: {rel_path} suggests project plan/roadmap — run \
        `touring generate render PlanMd` to scaffold a plan document via touring-generator"
    ))
}

/// R113-S2: Emit ManPage hint when a file path suggests man page documentation files (CC=2).
pub(crate) fn maybe_man_page_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const MAN_MARKERS: &[&str] = &[
        "man/", "manpage", "man_page", ".1.md", ".8.md", "docs/man", "ronn", "troff", "groff",
    ];
    let lower = rel_path.to_lowercase();
    let matches = MAN_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "man-page: {rel_path} suggests man page documentation — run \
        `touring generate render ManPage` to scaffold a man page via touring-generator"
    ))
}

/// R113-S3: Emit SkillDocument hint when a file path suggests skill/guide documentation (CC=2).
pub(crate) fn maybe_skill_document_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const SKILL_MARKERS: &[&str] = &[
        "skills/",
        "skill.md",
        "skill_document",
        "SKILL.md",
        "guides/",
        "playbook",
        "runbook",
        "tutorial",
    ];
    let lower = rel_path.to_lowercase();
    let matches = SKILL_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "skill-document: {rel_path} suggests skill/guide documentation — run \
        `touring generate render SkillDocument` to scaffold a skill document via touring-generator"
    ))
}

/// R114-S1: Emit DiaryEntry hint when a file path suggests touring diary/agent memory files (CC=2).
pub(crate) fn maybe_diary_entry_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const DIARY_MARKERS: &[&str] = &[
        "diary/",
        "diary_entry",
        "agent_memory",
        "aaak",
        "lessons/",
        "lessons_learned",
        "session_diary",
        "retrospective",
    ];
    let lower = rel_path.to_lowercase();
    let matches = DIARY_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "diary-entry: {rel_path} suggests agent diary/memory — run \
        `touring generate render DiaryEntry` to scaffold a diary entry via touring-generator"
    ))
}

/// R114-S2: Emit ConsumerGenerator hint when a file path suggests event consumer code (CC=2).
pub(crate) fn maybe_consumer_generator_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const CONSUMER_MARKERS: &[&str] = &[
        "consumer/",
        "consumers/",
        "consumer_",
        "_consumer.rs",
        "event_consumer",
        "message_consumer",
        "subscriber",
        "handler_consumer",
    ];
    let lower = rel_path.to_lowercase();
    let matches = CONSUMER_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "consumer-generator: {rel_path} suggests event consumer code — run \
        `touring generate render ConsumerGenerator` to scaffold a consumer via touring-generator"
    ))
}

/// R114-S3: Emit TaskScaffold hint when a file path suggests TACO task/DAG scaffold files (CC=2).
pub(crate) fn maybe_task_scaffold_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const TASK_MARKERS: &[&str] = &[
        "task_scaffold",
        "taco_task",
        "decompose",
        "dag_task",
        "tasks/",
        "subtask",
        "task_dag",
        "touring_task",
    ];
    let lower = rel_path.to_lowercase();
    let matches = TASK_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "task-scaffold: {rel_path} suggests TACO task/DAG scaffold — run \
        `touring generate render TaskScaffold` to scaffold a task DAG via touring-generator"
    ))
}

/// R107-S1: Emit FuzzTarget hint when a file path suggests cargo-fuzz target files (CC=2).
pub(crate) fn maybe_fuzz_target_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const FUZZ_MARKERS: &[&str] = &[
        "fuzz_targets/",
        "fuzz/targets/",
        "fuzz_target",
        "_fuzz.rs",
        "afl_target",
        "cargo-fuzz",
        "libfuzzer",
    ];
    let lower = rel_path.to_lowercase();
    let matches = FUZZ_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "fuzz-target: {rel_path} suggests fuzz test targets — run \
        `touring generate render FuzzTarget` to scaffold a fuzz target via touring-generator"
    ))
}

/// R107-S2: Emit DeriveMacro hint when a file path suggests proc-macro/derive crates (CC=2).
pub(crate) fn maybe_derive_macro_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const MACRO_MARKERS: &[&str] = &[
        "derive_",
        "proc_macro",
        "proc-macro",
        "custom_derive",
        "macros/",
        "macro_rules",
        "_derive.rs",
        "derive.rs",
    ];
    let lower = rel_path.to_lowercase();
    let matches = MACRO_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "derive-macro: {rel_path} suggests proc-macro/derive code — run \
        `touring generate render DeriveMacro` to scaffold a derive macro via touring-generator"
    ))
}

/// R107-S3: Emit IncrementalPatch hint when a file path suggests patch/diff files (CC=2).
pub(crate) fn maybe_incremental_patch_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const PATCH_MARKERS: &[&str] = &[
        ".patch",
        ".diff",
        "patches/",
        "patch_",
        "incremental_",
        "hotfix",
        "bugfix_patch",
        "apply_patch",
    ];
    let lower = rel_path.to_lowercase();
    let matches = PATCH_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "incremental-patch: {rel_path} suggests patch/diff files — run \
        `touring generate render IncrementalPatch` to scaffold an incremental patch via touring-generator"
    ))
}

/// R101-S1: Emit OpenApiSpec hint when a file path suggests OpenAPI/REST API patterns (CC=2).
pub(crate) fn maybe_openapi_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const OAS_MARKERS: &[&str] = &[
        "openapi",
        "swagger",
        "/api/spec",
        "oas3",
        "api-spec",
        "rest-spec",
        "api_spec",
        "api-schema",
    ];
    let lower = rel_path.to_lowercase();
    let matches = OAS_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "openapi-spec: {rel_path} suggests OpenAPI/REST API spec — run \
        `touring generate render OpenApiSpec` to scaffold an OpenAPI spec via touring-generator"
    ))
}

/// R101-S2: Emit ShellCompletion hint when a file path suggests shell completion scripts (CC=2).
pub(crate) fn maybe_shell_completion_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const COMPLETION_MARKERS: &[&str] = &[
        "completion",
        "_completion",
        "bash_completion",
        "zsh_completion",
        "fish_completion",
        "completions/",
        ".bash_profile",
        ".zshrc",
    ];
    let lower = rel_path.to_lowercase();
    let matches = COMPLETION_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "shell-completion: {rel_path} suggests shell completion scripts — run \
        `touring generate render ShellCompletion` to scaffold completions via touring-generator"
    ))
}

/// R101-S3: Emit ChangelogEntry hint when a file path suggests changelog/release notes (CC=2).
pub(crate) fn maybe_changelog_hint_on_file_changed(rel_path: &str) -> Option<String> {
    const CHANGELOG_MARKERS: &[&str] = &[
        "changelog",
        "changes",
        "release_notes",
        "release-notes",
        "history.md",
        "news.md",
        "releases/",
    ];
    let lower = rel_path.to_lowercase();
    let matches = CHANGELOG_MARKERS.iter().any(|m| lower.contains(m));
    if !matches {
        return None;
    }
    Some(format!(
        "changelog-entry: {rel_path} suggests release notes — run \
        `touring generate render ChangelogEntry` to scaffold a changelog entry via touring-generator"
    ))
}
