//! Shared hint helpers for Enter/ExitPlanMode handlers (D9).
//!
//! All functions are `pub(super)` — only `enter.rs` and `exit.rs` call them.
//! Naming convention: `maybe_*_on_enter_plan` / `maybe_*_on_exit_plan`.

/// Upsert a plan session document into Tantivy (R18-S2).
///
/// Records the plan session as a searchable symbol with kind="plan_session",
/// enabling `touring tantivy search "plan_session"` and intent-keyword lookup.
/// No-op when the `tantivy-fts` feature is disabled.
/// A raiz do projeto acompanha a operação: o store de decompose já é
/// per-project (`locate_task_store`), então o espelho no Tantivy segue a
/// fonte da verdade em vez de cair no índice legado compartilhado.
pub(crate) fn upsert_plan_session_to_tantivy(
    project_root: &std::path::Path,
    plan_task_id: &str,
    intent: &str,
) {
    #[cfg(feature = "tantivy-fts")]
    {
        if let Some(idx) = crate::tantivy_index::tantivy_for(Some(project_root)) {
            let doc = crate::tantivy_index::SymbolDoc {
                symbol_name: plan_task_id.to_string(),
                file_path: format!("plan_session:{plan_task_id}"),
                symbol_kind: "plan_session".to_string(),
                module_path: Some("plan_sessions".to_string()),
                docstring: Some(intent[..intent.len().min(300)].to_string()),
                line_number: 0,
                language: "plan".to_string(),
                visibility: None,
                crate_name: None,
                blake3_hash: None,
                import_count: None,
                export_count: None,
                cognitive_score: None,
                functional_signature: None,
                community_id: None,
            };
            let _ = idx.upsert_symbol(&doc);
            let _ = idx.commit();
            tracing::debug!(
                plan_task_id = plan_task_id,
                "plan session upserted to Tantivy"
            );
        }
    }
    #[cfg(not(feature = "tantivy-fts"))]
    let _ = (plan_task_id, intent);
}

// ── Enter-plan hints ──────────────────────────────────────────────────────────

/// R48-S2: Suggest `adr` generator when planning intent involves architectural decisions (CC=2).
///
/// When EnterPlanMode carries intent containing architectural keywords (architecture, design,
/// decision, pattern, refactor, migrate, integration, tradeoff), surfaces
/// `touring generate render adr` so Claude Code captures the decision rationale as an ADR
/// artifact. Closes the loop: planning intent → adr.tera → touring-generator registry.
/// Returns `None` when intent is empty or contains no ADR-worthy keywords.
pub(crate) fn maybe_adr_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const ADR_KEYWORDS: &[&str] = &[
        "architect",
        "design",
        "decision",
        "pattern",
        "refactor",
        "migrate",
        "integration",
        "tradeoff",
        "trade-off",
        "approach",
        "adr",
    ];
    let lower = intent.to_lowercase();
    if !ADR_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let title = &intent[..intent.len().min(40)];
    Some(format!(
        "adr: architectural decision — run `touring generate render adr \
        --vars '{{\"title\":\"{title}\",\"status\":\"proposed\"}}' -j` \
        to capture decision rationale via touring-generator"
    ))
}

/// R49-S3: Suggest `asyncapi_spec` generator when planning intent involves async/event patterns (CC=2).
///
/// When EnterPlanMode has an intent containing async, event, message, queue, broker,
/// stream, websocket, kafka, or rabbitmq keywords, surfaces `touring generate render asyncapi_spec`
/// so Claude Code scaffolds the async API contract before implementation.
/// Closes the loop: EnterPlanMode(async intent) → asyncapi_spec.tera → touring-generator.
/// Returns `None` when intent is empty or contains no async messaging keywords.
pub(crate) fn maybe_asyncapi_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const ASYNC_KEYWORDS: &[&str] = &[
        "async",
        "event",
        "message",
        "queue",
        "broker",
        "stream",
        "websocket",
        "kafka",
        "rabbitmq",
        "pubsub",
        "pub/sub",
        "nats",
    ];
    let lower = intent.to_lowercase();
    if !ASYNC_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let title = &intent[..intent.len().min(40)];
    Some(format!(
        "asyncapi: async messaging intent — run `touring generate render asyncapi_spec \
        --vars '{{\"title\":\"{title}\",\"version\":\"1.0.0\"}}' -j` \
        to scaffold event contract via touring-generator"
    ))
}

/// R51-S3: Suggest `error_catalog` generator when EnterPlanMode intent involves error/exception handling (CC=2).
///
/// When EnterPlanMode has an intent containing error, exception, fault, failure, catalog,
/// error codes, or error handling keywords, surfaces `touring generate render error_catalog`
/// so Claude Code scaffolds a structured error catalog before implementing error paths.
/// Closes the loop: EnterPlanMode(error intent) → error_catalog.tera → touring-generator.
/// Returns `None` when intent is empty or contains no error-related keywords.
pub(crate) fn maybe_error_catalog_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const ERROR_KEYWORDS: &[&str] = &[
        "error",
        "exception",
        "fault",
        "failure",
        "error catalog",
        "error code",
        "error handling",
        "status code",
        "http status",
    ];
    let lower = intent.to_lowercase();
    if !ERROR_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let title = &intent[..intent.len().min(40)];
    Some(format!(
        "error-catalog: error handling intent — run `touring generate render error_catalog \
        --vars '{{\"domain\":\"{title}\"}}' -j` \
        to scaffold structured error catalog via touring-generator"
    ))
}

/// R54-S3: Suggest `task_scaffold` generator when EnterPlanMode intent involves DAG/decompose (CC=2).
///
/// When EnterPlanMode has an intent containing dag, decompose, taco, scaffold, or task_scaffold
/// keywords, surfaces `touring generate render task_scaffold` so Claude Code generates the
/// task decomposition YAML scaffold at planning time — directly wiring Claude Code task planning
/// to the Touring decompose DAG.
/// Closes the loop: EnterPlanMode(DAG planning) → task_scaffold.tera → touring-generator.
/// Returns `None` when intent is empty or contains no DAG-planning keywords.
pub(crate) fn maybe_task_scaffold_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const DAG_KEYWORDS: &[&str] = &[
        "dag",
        "decompose",
        "taco",
        "scaffold",
        "task_scaffold",
        "task scaffold",
        "subtask",
        "phase plan",
    ];
    let lower = intent.to_lowercase();
    if !DAG_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let task_id = &intent[..intent.len().min(30)];
    Some(format!(
        "task-scaffold: DAG planning detected — run `touring generate render task_scaffold \
        --vars '{{\"task_id\":\"{task_id}\",\"intent\":\"plan\"}}' -j` \
        to scaffold Touring decompose DAG via touring-generator"
    ))
}

/// R63-S1: Suggest `ci_workflow` generator when EnterPlanMode intent involves CI/CD pipelines (CC=2).
///
/// When EnterPlanMode has an intent containing ci/cd, github actions, pipeline, continuous
/// integration, build pipeline, or release workflow keywords, surfaces
/// `touring generate render ci_workflow` so Claude Code scaffolds the CI workflow before
/// implementing the automation.
/// Closes the loop: EnterPlanMode(CI/CD intent) → ci_workflow.tera → touring-generator.
/// Returns `None` when intent is empty or contains no CI/CD-related keywords.
pub(crate) fn maybe_ci_workflow_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const CI_KEYWORDS: &[&str] = &[
        "ci/cd",
        "github actions",
        "ci pipeline",
        "cd pipeline",
        "continuous integration",
        "build pipeline",
        "release workflow",
        "github workflow",
        "deploy pipeline",
        "release pipeline",
    ];
    let lower = intent.to_lowercase();
    if !CI_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "ci-workflow: CI/CD intent — run `touring generate render ci_workflow \
        --vars '{{\"workflow_name\":\"{name}\",\"trigger\":\"push\"}}' -j` \
        to scaffold CI pipeline via touring-generator"
    ))
}

/// R63-S2: Suggest `dockerfile` generator when EnterPlanMode intent involves containers (CC=2).
///
/// When EnterPlanMode has an intent containing docker, container, dockerfile, containerize,
/// docker image, or docker compose keywords, surfaces `touring generate render dockerfile`
/// so Claude Code scaffolds the container build before implementation.
/// Closes the loop: EnterPlanMode(Docker intent) → dockerfile.tera → touring-generator.
/// Returns `None` when intent is empty or contains no container-related keywords.
pub(crate) fn maybe_dockerfile_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const DOCKER_KEYWORDS: &[&str] = &[
        "dockerfile",
        "docker image",
        "docker compose",
        "container image",
        "containerize",
        "docker build",
        "docker registry",
        "container build",
    ];
    let lower = intent.to_lowercase();
    if !DOCKER_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "dockerfile: container intent — run `touring generate render dockerfile \
        --vars '{{\"service_name\":\"{name}\",\"base_image\":\"rust:1.77\"}}' -j` \
        to scaffold Dockerfile via touring-generator"
    ))
}

/// R63-S3: Suggest `terraform_module` generator when EnterPlanMode intent involves IaC (CC=2).
///
/// When EnterPlanMode has an intent containing terraform, infrastructure, infra module,
/// iac, cloud infra, or provision keywords, surfaces `touring generate render terraform_module`
/// so Claude Code scaffolds the Terraform module before implementation.
/// Closes the loop: EnterPlanMode(IaC intent) → terraform_module.tera → touring-generator.
/// Returns `None` when intent is empty or contains no IaC-related keywords.
pub(crate) fn maybe_terraform_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const TERRAFORM_KEYWORDS: &[&str] = &[
        "terraform",
        "infra module",
        "infrastructure as code",
        "iac",
        "cloud infra",
        "provision cloud",
        "terraform module",
        "opentofu",
        "pulumi",
        "cloud formation",
        "cloudformation",
    ];
    let lower = intent.to_lowercase();
    if !TERRAFORM_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "terraform: IaC intent — run `touring generate render terraform_module \
        --vars '{{\"module_name\":\"{name}\"}}' -j` \
        to scaffold Terraform module via touring-generator"
    ))
}

/// R96-S1: Suggest `rust_module` generator when EnterPlanMode intent involves Rust module creation (CC=2).
///
/// Closes the loop: EnterPlanMode(rust module intent) → rust_module.tera → touring-generator.
pub(crate) fn maybe_rust_module_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const RUST_KEYWORDS: &[&str] = &[
        "rust module",
        "new module",
        "create module",
        "rust crate",
        "new crate",
        "rust library",
        "rust struct",
        "implement trait",
    ];
    let lower = intent.to_lowercase();
    if !RUST_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "rust-module: Rust module intent — run `touring generate render RustModule \
        --vars '{{\"module_name\":\"{name}\"}}' -j` \
        to scaffold a Rust module via touring-generator"
    ))
}

/// R96-S2: Suggest `migration` generator when EnterPlanMode intent involves database migrations (CC=2).
///
/// Closes the loop: EnterPlanMode(migration intent) → migration.tera → touring-generator.
pub(crate) fn maybe_migration_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const MIGRATION_KEYWORDS: &[&str] = &[
        "migration",
        "database migration",
        "schema change",
        "db migration",
        "alter table",
        "add column",
        "sql migration",
        "database schema",
    ];
    let lower = intent.to_lowercase();
    if !MIGRATION_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "migration: database migration intent — run `touring generate render Migration \
        --vars '{{\"migration_name\":\"{name}\"}}' -j` \
        to scaffold a migration via touring-generator"
    ))
}

/// R96-S3: Suggest `protobuf_schema` generator when EnterPlanMode intent involves protobuf/gRPC (CC=2).
///
/// Closes the loop: EnterPlanMode(gRPC intent) → protobuf_schema.tera → touring-generator.
pub(crate) fn maybe_protobuf_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const PROTO_KEYWORDS: &[&str] = &[
        "protobuf",
        "grpc",
        "proto schema",
        "protocol buffer",
        "grpc service",
        "proto message",
        "proto definition",
        "rpc service",
    ];
    let lower = intent.to_lowercase();
    if !PROTO_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "protobuf-schema: gRPC/protobuf intent — run `touring generate render ProtobufSchema \
        --vars '{{\"service_name\":\"{name}\"}}' -j` \
        to scaffold a protobuf schema via touring-generator"
    ))
}

/// R102-S1: Suggest `k8s_manifest` generator when EnterPlanMode intent involves Kubernetes (CC=2).
pub(crate) fn maybe_k8s_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const K8S_KEYWORDS: &[&str] = &[
        "kubernetes",
        "k8s",
        "kubectl",
        "helm chart",
        "pod deployment",
        "k8s manifest",
        "deployment yaml",
        "service mesh",
        "kustomize",
    ];
    let lower = intent.to_lowercase();
    if !K8S_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "k8s-manifest: Kubernetes intent — run `touring generate render K8sManifest \
        --vars '{{\"app_name\":\"{name}\"}}' -j` \
        to scaffold a K8s manifest via touring-generator"
    ))
}

/// R102-S2: Suggest `openapi_spec` generator when EnterPlanMode intent involves REST APIs (CC=2).
pub(crate) fn maybe_openapi_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const OAS_KEYWORDS: &[&str] = &[
        "openapi",
        "swagger",
        "rest api spec",
        "oas3",
        "api contract",
        "api specification",
        "http api",
        "rest specification",
    ];
    let lower = intent.to_lowercase();
    if !OAS_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "openapi-spec: REST/OpenAPI intent — run `touring generate render OpenApiSpec \
        --vars '{{\"api_name\":\"{name}\"}}' -j` \
        to scaffold an OpenAPI spec via touring-generator"
    ))
}

/// R102-S3: Suggest `shell_completion` generator when EnterPlanMode intent involves CLI completions (CC=2).
pub(crate) fn maybe_shell_completion_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const COMPLETION_KEYWORDS: &[&str] = &[
        "shell completion",
        "bash completion",
        "zsh completion",
        "fish completion",
        "tab completion",
        "autocomplete",
        "completions script",
        "cli completion",
    ];
    let lower = intent.to_lowercase();
    if !COMPLETION_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "shell-completion: tab completion intent — run `touring generate render ShellCompletion \
        --vars '{{\"tool_name\":\"{name}\"}}' -j` \
        to scaffold shell completions via touring-generator"
    ))
}

/// R105-S1: Suggest `man_page` generator when EnterPlanMode intent involves Unix man pages (CC=2).
pub(crate) fn maybe_man_page_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const MAN_KEYWORDS: &[&str] = &[
        "man page",
        "manpage",
        "linux man",
        "unix man",
        "manual page",
        "man section",
        "groff manual",
        "troff document",
    ];
    let lower = intent.to_lowercase();
    if !MAN_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "man-page: Unix man page intent — run `touring generate render ManPage \
        --vars '{{\"command_name\":\"{name}\"}}' -j` \
        to scaffold a man page via touring-generator"
    ))
}

/// R105-S2: Suggest `changelog_entry` generator when EnterPlanMode intent involves releases (CC=2).
pub(crate) fn maybe_changelog_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const CHANGELOG_KEYWORDS: &[&str] = &[
        "changelog",
        "release notes",
        "release entry",
        "version bump",
        "semantic version",
        "release log",
        "what's new",
        "breaking change",
    ];
    let lower = intent.to_lowercase();
    if !CHANGELOG_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "changelog-entry: release/changelog intent — run `touring generate render ChangelogEntry \
        --vars '{{\"version\":\"{name}\"}}' -j` \
        to scaffold a changelog entry via touring-generator"
    ))
}

/// R105-S3: Suggest `skill_document` generator when EnterPlanMode intent involves skills/docs (CC=2).
pub(crate) fn maybe_skill_document_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const SKILL_KEYWORDS: &[&str] = &[
        "skill document",
        "skill.md",
        "claude skill",
        "skill definition",
        "agent skill",
        "skill scaffold",
        "guide document",
        "playbook",
    ];
    let lower = intent.to_lowercase();
    if !SKILL_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "skill-document: skill/guide intent — run `touring generate render SkillDocument \
        --vars '{{\"skill_name\":\"{name}\"}}' -j` \
        to scaffold a skill document via touring-generator"
    ))
}

/// R108-S1: Suggest `ffi_binding` generator when EnterPlanMode intent involves FFI/native (CC=2).
pub(crate) fn maybe_ffi_binding_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const FFI_KEYWORDS: &[&str] = &[
        "ffi binding",
        "native binding",
        "c binding",
        "extern ffi",
        "bindgen",
        "unsafe extern",
        "ffi wrapper",
        "native library",
    ];
    let lower = intent.to_lowercase();
    if !FFI_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "ffi-binding: FFI/native intent — run `touring generate render FfiBinding \
        --vars '{{\"lib_name\":\"{name}\"}}' -j` \
        to scaffold FFI bindings via touring-generator"
    ))
}

/// R108-S2: Suggest `python_script` generator when EnterPlanMode intent involves Python (CC=2).
pub(crate) fn maybe_python_script_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const PY_KEYWORDS: &[&str] = &[
        "python script",
        "python automation",
        "pyscript",
        "python tool",
        "python module",
        "python utility",
        "py script",
        "python cli",
    ];
    let lower = intent.to_lowercase();
    if !PY_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "python-script: Python intent — run `touring generate render PythonScript \
        --vars '{{\"script_name\":\"{name}\"}}' -j` \
        to scaffold a Python script via touring-generator"
    ))
}

/// R108-S3: Suggest `benchmark` generator when EnterPlanMode intent involves perf benchmarks (CC=2).
pub(crate) fn maybe_benchmark_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const BENCH_KEYWORDS: &[&str] = &[
        "benchmark",
        "criterion",
        "performance test",
        "perf bench",
        "latency measure",
        "throughput test",
        "microbenchmark",
        "cargo bench",
    ];
    let lower = intent.to_lowercase();
    if !BENCH_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "benchmark: performance benchmark intent — run `touring generate render Benchmark \
        --vars '{{\"bench_name\":\"{name}\"}}' -j` \
        to scaffold a criterion benchmark via touring-generator"
    ))
}

/// R111-S1: Suggest `fuzz_target` generator when EnterPlanMode intent involves fuzzing (CC=2).
pub(crate) fn maybe_fuzz_target_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const FUZZ_KEYWORDS: &[&str] = &[
        "fuzz target",
        "cargo fuzz",
        "libfuzzer",
        "afl fuzz",
        "fuzz test",
        "fuzzing",
        "fuzz corpus",
        "fuzz harness",
    ];
    let lower = intent.to_lowercase();
    if !FUZZ_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "fuzz-target: fuzz test intent — run `touring generate render FuzzTarget \
        --vars '{{\"target_name\":\"{name}\"}}' -j` \
        to scaffold a libfuzzer fuzz target via touring-generator"
    ))
}

/// R111-S2: Suggest `derive_macro` generator when EnterPlanMode intent involves proc-macros (CC=2).
pub(crate) fn maybe_derive_macro_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const MACRO_KEYWORDS: &[&str] = &[
        "derive macro",
        "proc macro",
        "proc-macro",
        "procedural macro",
        "custom derive",
        "attribute macro",
        "macro crate",
        "derive trait",
    ];
    let lower = intent.to_lowercase();
    if !MACRO_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "derive-macro: proc-macro intent — run `touring generate render DeriveMacro \
        --vars '{{\"macro_name\":\"{name}\"}}' -j` \
        to scaffold a custom derive macro via touring-generator"
    ))
}

/// R111-S3: Suggest `diary_entry` generator when EnterPlanMode intent involves lessons/diary (CC=2).
pub(crate) fn maybe_diary_entry_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const DIARY_KEYWORDS: &[&str] = &[
        "diary entry",
        "lesson learned",
        "session note",
        "retrospective",
        "post-mortem",
        "aaak entry",
        "agent diary",
        "memory entry",
    ];
    let lower = intent.to_lowercase();
    if !DIARY_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "diary-entry: lesson/diary intent — run `touring generate render DiaryEntry \
        --vars '{{\"agent_name\":\"{name}\"}}' -j` \
        to scaffold a diary entry via touring-generator"
    ))
}

/// R115-S1: Suggest `cli_handler` generator when EnterPlanMode intent involves CLI commands (CC=2).
pub(crate) fn maybe_cli_handler_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const CLI_KEYWORDS: &[&str] = &[
        "cli command",
        "cli handler",
        "command handler",
        "clap command",
        "subcommand",
        "arg parse",
        "cli tool",
        "cli subcommand",
    ];
    let lower = intent.to_lowercase();
    if !CLI_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "cli-handler: CLI command planning intent — run `touring generate render CliHandler \
        --vars '{{\"command_name\":\"{name}\"}}' -j` \
        to scaffold a CLI handler via touring-generator"
    ))
}

/// R115-S2: Suggest `mcp_tool` generator when EnterPlanMode intent involves MCP tool design (CC=2).
pub(crate) fn maybe_mcp_tool_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const MCP_KEYWORDS: &[&str] = &[
        "mcp tool",
        "mcp server",
        "model context",
        "tool definition",
        "rmcp tool",
        "mcp endpoint",
        "tool schema",
        "mcp plugin",
    ];
    let lower = intent.to_lowercase();
    if !MCP_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "mcp-tool: MCP tool planning intent — run `touring generate render McpTool \
        --vars '{{\"tool_name\":\"{name}\"}}' -j` \
        to scaffold an MCP tool via touring-generator"
    ))
}

/// R115-S3: Suggest `hook_handler` generator when EnterPlanMode intent involves hook handlers (CC=2).
pub(crate) fn maybe_hook_handler_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const HOOK_KEYWORDS: &[&str] = &[
        "hook handler",
        "lifecycle hook",
        "pre-edit hook",
        "post-edit hook",
        "session hook",
        "hook registry",
        "claude code hook",
        "hook implementation",
    ];
    let lower = intent.to_lowercase();
    if !HOOK_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "hook-handler: hook handler planning intent — run `touring generate render HookHandler \
        --vars '{{\"hook_name\":\"{name}\"}}' -j` \
        to scaffold a hook handler via touring-generator"
    ))
}

/// R116-S1: Suggest `plan_md` generator when EnterPlanMode intent involves project planning (CC=2).
pub(crate) fn maybe_plan_md_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const PLAN_KEYWORDS: &[&str] = &[
        "project plan",
        "roadmap plan",
        "sprint plan",
        "milestone plan",
        "planning document",
        "plan document",
        "plan markdown",
        "planning doc",
    ];
    let lower = intent.to_lowercase();
    if !PLAN_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "plan-md: project planning intent — run `touring generate render PlanMd \
        --vars '{{\"project_name\":\"{name}\"}}' -j` \
        to scaffold a plan document via touring-generator"
    ))
}

/// R116-S2: Suggest `schema` generator when EnterPlanMode intent involves data schema design (CC=2).
pub(crate) fn maybe_schema_hint_on_enter_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const SCHEMA_KEYWORDS: &[&str] = &[
        "data schema",
        "json schema",
        "schema design",
        "schema definition",
        "schema validator",
        "schema migration",
        "type schema",
        "validate schema",
    ];
    let lower = intent.to_lowercase();
    if !SCHEMA_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        "schema: data schema planning intent — run `touring generate render Schema \
        --vars '{{\"schema_name\":\"{name}\"}}' -j` \
        to scaffold a schema definition via touring-generator"
    ))
}

// ── Exit-plan hints ───────────────────────────────────────────────────────────

/// R60-S3: Suggest `error_catalog` generator when ExitPlanMode intent involves error design (CC=2).
///
/// When ExitPlanMode has an intent containing error catalog, error codes, error handling design,
/// error taxonomy, error registry, or structured errors keywords, surfaces
/// `touring generate render error_catalog` so Claude Code scaffolds an error catalog at exit.
/// Closes the loop: ExitPlanMode(error design intent) → error_catalog.tera → touring-generator.
/// Returns `None` when intent is empty or contains no error catalog keywords.
pub(crate) fn maybe_error_catalog_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const ERROR_KEYWORDS: &[&str] = &[
        "error catalog",
        "error codes",
        "error handling design",
        "error taxonomy",
        "error registry",
        "structured errors",
        "error definitions",
        "error domain",
    ];
    let lower = intent.to_lowercase();
    if !ERROR_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | error-catalog: error design intent detected — run `touring generate render error_catalog \
        --vars '{{\"domain\":\"{name}\"}}' -j` \
        to scaffold structured error catalog via touring-generator"
    ))
}

/// R58-S3: Suggest `plan.md` generator when ExitPlanMode intent involves project planning (CC=2).
///
/// When ExitPlanMode has an intent containing project plan, roadmap, planning doc, architecture doc,
/// planning document, or system design keywords, surfaces `touring generate render plan.md` so
/// Claude Code scaffolds a structured planning document at exit time.
/// Closes the loop: ExitPlanMode(project plan intent) → plan.md.tera → touring-generator.
/// Returns `None` when intent is empty or contains no planning document keywords.
pub(crate) fn maybe_plan_md_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const PLAN_KEYWORDS: &[&str] = &[
        "project plan",
        "roadmap",
        "planning doc",
        "architecture doc",
        "planning document",
        "system design",
        "design document",
        "project roadmap",
    ];
    let lower = intent.to_lowercase();
    if !PLAN_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | plan-md: project planning intent detected — run `touring generate render plan.md \
        --vars '{{\"title\":\"{name}\"}}' -j` \
        to scaffold structured planning document via touring-generator"
    ))
}

/// R57-S3: Suggest `asyncapi_spec` generator when ExitPlanMode intent involves async/event APIs (CC=2).
///
/// When ExitPlanMode has an intent containing asyncapi, async api, event-driven, message broker,
/// kafka, rabbitmq, mqtt, websocket spec, or pub/sub keywords, surfaces `touring generate render
/// asyncapi_spec` so Claude Code scaffolds an AsyncAPI specification at planning exit time.
/// Closes the loop: ExitPlanMode(async API intent) → asyncapi_spec.tera → touring-generator.
/// Returns `None` when intent is empty or contains no async API keywords.
pub(crate) fn maybe_asyncapi_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const ASYNC_KEYWORDS: &[&str] = &[
        "asyncapi",
        "async api",
        "event-driven",
        "message broker",
        "kafka spec",
        "rabbitmq",
        "mqtt spec",
        "websocket spec",
        "pub/sub spec",
    ];
    let lower = intent.to_lowercase();
    if !ASYNC_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | asyncapi-spec: async API intent detected — run `touring generate render asyncapi_spec \
        --vars '{{\"title\":\"{name}\"}}' -j` \
        to scaffold AsyncAPI specification via touring-generator"
    ))
}

/// R64-S1: Suggest `ci_workflow` generator when ExitPlanMode intent involves CI/CD automation (CC=2).
///
/// When ExitPlanMode has an intent containing ci/cd, github actions, ci pipeline, release pipeline,
/// continuous integration, or build automation keywords, surfaces `touring generate render ci_workflow`
/// so Claude Code scaffolds the CI workflow immediately after completing the planning phase.
/// Closes the loop: ExitPlanMode(CI/CD intent) → ci_workflow.tera → touring-generator.
/// Returns `None` when intent is empty or contains no CI/CD keywords.
pub(crate) fn maybe_ci_workflow_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const CI_KEYWORDS: &[&str] = &[
        "ci/cd",
        "github actions",
        "ci pipeline",
        "release pipeline",
        "continuous integration",
        "build automation",
        "github workflow",
        "deploy automation",
        "release automation",
    ];
    let lower = intent.to_lowercase();
    if !CI_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | ci-workflow: CI/CD intent detected — run `touring generate render ci_workflow \
        --vars '{{\"workflow_name\":\"{name}\",\"trigger\":\"push\"}}' -j` \
        to scaffold CI pipeline via touring-generator"
    ))
}

/// R64-S2: Suggest `dockerfile` generator when ExitPlanMode intent involves container builds (CC=2).
///
/// When ExitPlanMode has an intent containing dockerfile, docker image, container image,
/// containerize, docker build, or container registry keywords, surfaces
/// `touring generate render dockerfile` so Claude Code scaffolds the Dockerfile at planning exit.
/// Closes the loop: ExitPlanMode(Docker intent) → dockerfile.tera → touring-generator.
/// Returns `None` when intent is empty or contains no Docker/container keywords.
pub(crate) fn maybe_dockerfile_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const DOCKER_KEYWORDS: &[&str] = &[
        "dockerfile",
        "docker image",
        "container image",
        "containerize",
        "docker build",
        "container registry",
        "docker compose",
        "container build",
    ];
    let lower = intent.to_lowercase();
    if !DOCKER_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | dockerfile: container intent detected — run `touring generate render dockerfile \
        --vars '{{\"service_name\":\"{name}\",\"base_image\":\"rust:1.77\"}}' -j` \
        to scaffold Dockerfile via touring-generator"
    ))
}

/// R64-S3: Suggest `k8s_manifest` generator when ExitPlanMode intent involves Kubernetes (CC=2).
///
/// When ExitPlanMode has an intent containing kubernetes, k8s, helm chart, pod spec, ingress,
/// deploy to cluster, kubectl, or kustomize keywords, surfaces `touring generate render k8s_manifest`
/// so Claude Code scaffolds the Kubernetes manifest immediately after planning.
/// Closes the loop: ExitPlanMode(K8s intent) → k8s_manifest.tera → touring-generator.
/// Returns `None` when intent is empty or contains no Kubernetes keywords.
pub(crate) fn maybe_k8s_manifest_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const K8S_KEYWORDS: &[&str] = &[
        "kubernetes",
        "k8s",
        "helm chart",
        "pod spec",
        "ingress",
        "deploy to cluster",
        "kubectl",
        "kustomize",
        "deployment yaml",
        "namespace",
        "container orchestration",
    ];
    let lower = intent.to_lowercase();
    if !K8S_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | k8s-manifest: Kubernetes intent detected — run `touring generate render k8s_manifest \
        --vars '{{\"app_name\":\"{name}\",\"replicas\":1}}' -j` \
        to scaffold Kubernetes manifest via touring-generator"
    ))
}

/// R56-S3: Suggest `man_page` generator when ExitPlanMode intent involves Unix man page docs (CC=2).
///
/// When ExitPlanMode has an intent containing man page, manpage, unix manual, cli documentation,
/// man 1, or man 3 keywords, surfaces `touring generate render man_page` so Claude Code
/// scaffolds a Unix man page at planning exit time.
/// Closes the loop: ExitPlanMode(man page intent) → man_page.tera → touring-generator.
/// Returns `None` when intent is empty or contains no man-page keywords.
pub(crate) fn maybe_man_page_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const MAN_KEYWORDS: &[&str] = &[
        "man page",
        "manpage",
        "man section",
        "unix manual",
        "cli documentation",
        "man 1",
        "man 3",
    ];
    let lower = intent.to_lowercase();
    if !MAN_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | man-page: man page intent detected — run `touring generate render man_page \
        --vars '{{\"command_name\":\"{name}\"}}' -j` \
        to scaffold Unix man page via touring-generator"
    ))
}

/// R55-S3: Suggest `hook_handler` generator when ExitPlanMode intent involves Touring hooks (CC=2).
///
/// When ExitPlanMode has an intent containing hook, hook handler, lifecycle hook, pre-read,
/// post-edit, claude code hook, or hook event keywords, surfaces `touring generate render
/// hook_handler` so Claude Code scaffolds a new hook handler at planning exit time.
/// Closes the loop: ExitPlanMode(hook intent) → hook_handler.tera → touring-generator.
/// Returns `None` when intent is empty or contains no hook-related keywords.
pub(crate) fn maybe_hook_handler_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const HOOK_KEYWORDS: &[&str] = &[
        "hook handler",
        "lifecycle hook",
        "pre-read",
        "post-edit",
        "claude code hook",
        "hook event",
        "pre_read",
        "post_edit",
        "hook registry",
        "new hook",
    ];
    let lower = intent.to_lowercase();
    if !HOOK_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | hook-handler: Touring hook planning detected — run `touring generate render hook_handler \
        --vars '{{\"hook_name\":\"{name}\"}}' -j` \
        to scaffold hook handler via touring-generator"
    ))
}

/// R97-S1: Suggest `rust_module` generator when ExitPlanMode intent involves Rust modules (CC=2).
///
/// Closes the loop: ExitPlanMode(rust module intent) → rust_module.tera → touring-generator.
pub(crate) fn maybe_rust_module_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const RUST_KEYWORDS: &[&str] = &[
        "rust module",
        "new module",
        "create module",
        "rust crate",
        "new crate",
        "rust library",
        "implement trait",
        "rust struct",
    ];
    let lower = intent.to_lowercase();
    if !RUST_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | rust-module: Rust module planning — run `touring generate render RustModule \
        --vars '{{\"module_name\":\"{name}\"}}' -j` \
        to scaffold a Rust module via touring-generator"
    ))
}

/// R97-S2: Suggest `migration` generator when ExitPlanMode intent involves database migrations (CC=2).
///
/// Closes the loop: ExitPlanMode(migration intent) → migration.tera → touring-generator.
pub(crate) fn maybe_migration_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const MIGRATION_KEYWORDS: &[&str] = &[
        "migration",
        "database migration",
        "schema change",
        "db migration",
        "alter table",
        "add column",
        "sql migration",
        "database schema",
    ];
    let lower = intent.to_lowercase();
    if !MIGRATION_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | migration: database migration planning — run `touring generate render Migration \
        --vars '{{\"migration_name\":\"{name}\"}}' -j` \
        to scaffold a migration via touring-generator"
    ))
}

/// R97-S3: Suggest `protobuf_schema` generator when ExitPlanMode intent involves gRPC/protobuf (CC=2).
///
/// Closes the loop: ExitPlanMode(gRPC intent) → protobuf_schema.tera → touring-generator.
pub(crate) fn maybe_protobuf_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const PROTO_KEYWORDS: &[&str] = &[
        "protobuf",
        "grpc",
        "proto schema",
        "protocol buffer",
        "grpc service",
        "proto message",
        "proto definition",
        "rpc service",
    ];
    let lower = intent.to_lowercase();
    if !PROTO_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | protobuf-schema: gRPC/protobuf planning — run `touring generate render ProtobufSchema \
        --vars '{{\"service_name\":\"{name}\"}}' -j` \
        to scaffold a protobuf schema via touring-generator"
    ))
}

/// R109-S1: Suggest `python_script` generator when ExitPlanMode intent involves Python (CC=2).
pub(crate) fn maybe_python_script_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const PY_KEYWORDS: &[&str] = &[
        "python script",
        "python automation",
        "pyscript",
        "python tool",
        "python module",
        "python utility",
        "py script",
        "python cli",
    ];
    let lower = intent.to_lowercase();
    if !PY_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | python-script: Python intent — run `touring generate render PythonScript \
        --vars '{{\"script_name\":\"{name}\"}}' -j` \
        to scaffold a Python script via touring-generator"
    ))
}

/// R109-S2: Suggest `benchmark` generator when ExitPlanMode intent involves perf benchmarks (CC=2).
pub(crate) fn maybe_benchmark_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const BENCH_KEYWORDS: &[&str] = &[
        "benchmark",
        "criterion",
        "performance test",
        "perf bench",
        "latency measure",
        "throughput test",
        "microbenchmark",
        "cargo bench",
    ];
    let lower = intent.to_lowercase();
    if !BENCH_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | benchmark: perf benchmark intent — run `touring generate render Benchmark \
        --vars '{{\"bench_name\":\"{name}\"}}' -j` \
        to scaffold a criterion benchmark via touring-generator"
    ))
}

/// R109-S3: Suggest `incremental_patch` generator when ExitPlanMode intent involves patches (CC=2).
pub(crate) fn maybe_incremental_patch_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const PATCH_KEYWORDS: &[&str] = &[
        "patch",
        "incremental patch",
        "hotfix",
        "bugfix patch",
        "diff apply",
        "apply patch",
        "incremental update",
        "delta patch",
    ];
    let lower = intent.to_lowercase();
    if !PATCH_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | incremental-patch: patch intent — run `touring generate render IncrementalPatch \
        --vars '{{\"patch_name\":\"{name}\"}}' -j` \
        to scaffold an incremental patch via touring-generator"
    ))
}

/// R112-S1: Suggest `cli_handler` generator when ExitPlanMode intent involves CLI commands (CC=2).
pub(crate) fn maybe_cli_handler_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const CLI_KEYWORDS: &[&str] = &[
        "cli command",
        "cli handler",
        "command handler",
        "clap command",
        "subcommand",
        "arg parse",
        "cli tool",
        "cli subcommand",
    ];
    let lower = intent.to_lowercase();
    if !CLI_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | cli-handler: CLI command intent — run `touring generate render CliHandler \
        --vars '{{\"command_name\":\"{name}\"}}' -j` \
        to scaffold a CLI handler via touring-generator"
    ))
}

/// R112-S2: Suggest `mcp_tool` generator when ExitPlanMode intent involves MCP/tool definitions (CC=2).
pub(crate) fn maybe_mcp_tool_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const MCP_KEYWORDS: &[&str] = &[
        "mcp tool",
        "mcp server",
        "model context",
        "tool definition",
        "rmcp tool",
        "mcp endpoint",
        "tool schema",
        "mcp plugin",
    ];
    let lower = intent.to_lowercase();
    if !MCP_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | mcp-tool: MCP tool intent — run `touring generate render McpTool \
        --vars '{{\"tool_name\":\"{name}\"}}' -j` \
        to scaffold an MCP tool via touring-generator"
    ))
}

/// R112-S3: Suggest `schema` generator when ExitPlanMode intent involves data schema design (CC=2).
pub(crate) fn maybe_schema_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const SCHEMA_KEYWORDS: &[&str] = &[
        "data schema",
        "json schema",
        "schema design",
        "schema definition",
        "schema validator",
        "schema migration",
        "type schema",
        "validate schema",
    ];
    let lower = intent.to_lowercase();
    if !SCHEMA_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | schema: data schema intent — run `touring generate render Schema \
        --vars '{{\"schema_name\":\"{name}\"}}' -j` \
        to scaffold a schema definition via touring-generator"
    ))
}

/// R53-S3: Suggest `skill_document` generator when ExitPlanMode intent involves documentation (CC=2).
///
/// When ExitPlanMode has an intent containing skill, document, documentation, guide, tutorial,
/// playbook, runbook, or how-to keywords, surfaces `touring generate render skill_document` so
/// Claude Code scaffolds a skill/guide artifact immediately after planning.
/// Closes the loop: ExitPlanMode(doc intent) → skill_document.tera → touring-generator skill doc.
/// Returns `None` when intent is empty or contains no documentation-related keywords.
pub(crate) fn maybe_skill_document_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const DOC_KEYWORDS: &[&str] = &[
        "skill",
        "document",
        "documentation",
        "guide",
        "tutorial",
        "playbook",
        "runbook",
        "how-to",
        "howto",
        "handbook",
    ];
    let lower = intent.to_lowercase();
    if !DOC_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let title = &intent[..intent.len().min(40)];
    Some(format!(
        " | skill-document: documentation intent detected — run `touring generate render skill_document \
        --vars '{{\"title\":\"{title}\"}}' -j` \
        to scaffold skill doc via touring-generator"
    ))
}

/// R52-S3: Suggest `terraform_module` generator when ExitPlanMode intent involves IaC/cloud (CC=2).
///
/// When ExitPlanMode has an intent containing terraform, infra, infrastructure, cloud, aws, gcp,
/// azure, iac, or provisioning keywords, surfaces `touring generate render terraform_module` so
/// Claude Code scaffolds IaC modules immediately after planning.
/// Closes the loop: ExitPlanMode(IaC intent) → terraform_module.tera → touring-generator.
/// Returns `None` when intent is empty or contains no IaC-related keywords.
pub(crate) fn maybe_terraform_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const IAC_KEYWORDS: &[&str] = &[
        "terraform",
        "infra",
        "infrastructure",
        "cloud",
        "aws",
        "gcp",
        "azure",
        "iac",
        "provisioning",
    ];
    let lower = intent.to_lowercase();
    if !IAC_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | terraform-module: IaC planning detected — run `touring generate render terraform_module \
        --vars '{{\"module_name\":\"{name}\"}}' -j` \
        to scaffold Terraform module via touring-generator"
    ))
}

/// R50-S3: Suggest `shell_completion` generator when ExitPlanMode intent involves CLI tools (CC=2).
///
/// When ExitPlanMode has an intent containing cli, shell, completion, terminal, command-line,
/// bash, zsh, or fish keywords, surfaces `touring generate render shell_completion` so Claude Code
/// scaffolds shell completion scripts immediately after planning.
/// Closes the loop: ExitPlanMode(CLI intent) → shell_completion.tera → touring-generator.
/// Returns `None` when intent is empty or contains no CLI-related keywords.
pub(crate) fn maybe_shell_completion_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const CLI_KEYWORDS: &[&str] = &[
        "cli",
        "shell",
        "completion",
        "terminal",
        "command-line",
        "commandline",
        "bash",
        "zsh",
        "fish",
        "autocomplete",
        "subcommand",
    ];
    let lower = intent.to_lowercase();
    if !CLI_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(30)];
    Some(format!(
        " | shell-completion: CLI planning detected — run `touring generate render shell_completion \
        --vars '{{\"program_name\":\"{name}\"}}' -j` \
        to scaffold shell completions via touring-generator"
    ))
}

/// R48-S3: Suggest `changelog_entry` generator on ExitPlanMode when intent is present (CC=2).
///
/// When ExitPlanMode has a non-trivial intent/description, surfaces
/// `touring generate render changelog_entry` so Claude Code documents what was planned/built
/// in the changelog artifact. Closes the loop: ExitPlanMode → changelog_entry.tera → generator.
/// Returns `None` when intent is empty or too short to produce a useful entry.
pub(crate) fn maybe_changelog_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.len() < 3 {
        return None;
    }
    let summary = &intent[..intent.len().min(50)];
    Some(format!(
        " | changelog-entry: run `touring generate render changelog_entry \
        --vars '{{\"version\":\"next\",\"summary\":\"{summary}\"}}' -j` \
        to document planned changes via touring-generator"
    ))
}

/// R103-S1: Suggest `adr` generator when ExitPlanMode intent involves architecture decisions (CC=2).
pub(crate) fn maybe_adr_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const ADR_KEYWORDS: &[&str] = &[
        "architecture decision",
        "adr",
        "design decision",
        "technical decision",
        "design record",
        "decision record",
        "architectural record",
        "arch decision",
    ];
    let lower = intent.to_lowercase();
    if !ADR_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | adr: architecture decision intent — run `touring generate render Adr \
        --vars '{{\"title\":\"{name}\"}}' -j` \
        to scaffold an ADR via touring-generator"
    ))
}

/// R103-S2: Suggest `task_scaffold` generator when ExitPlanMode intent involves decompose/DAG (CC=2).
pub(crate) fn maybe_task_scaffold_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const TASK_KEYWORDS: &[&str] = &[
        "task scaffold",
        "dag scaffold",
        "subtask",
        "decompose",
        "task dag",
        "touring decompose",
        "task plan",
        "task breakdown",
        "work breakdown",
    ];
    let lower = intent.to_lowercase();
    if !TASK_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | task-scaffold: task decomposition intent — run `touring generate render TaskScaffold \
        --vars '{{\"task_id\":\"{name}\"}}' -j` \
        to scaffold a DAG task via touring-generator"
    ))
}

/// R103-S3: Suggest `test` generator when ExitPlanMode intent involves test suites (CC=2).
pub(crate) fn maybe_test_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const TEST_KEYWORDS: &[&str] = &[
        "test suite",
        "unit test",
        "integration test",
        "e2e test",
        "test coverage",
        "test scaffold",
        "cargo test",
        "#[test]",
        "test module",
    ];
    let lower = intent.to_lowercase();
    if !TEST_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | test: test intent detected — run `touring generate render Test \
        --vars '{{\"module_name\":\"{name}\"}}' -j` \
        to scaffold a test module via touring-generator"
    ))
}

/// R106-S1: Suggest `openapi_spec` generator when ExitPlanMode intent involves REST APIs (CC=2).
pub(crate) fn maybe_openapi_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const OAS_KEYWORDS: &[&str] = &[
        "openapi",
        "swagger",
        "rest api spec",
        "oas3",
        "api contract",
        "http api schema",
        "api specification",
        "rest specification",
    ];
    let lower = intent.to_lowercase();
    if !OAS_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | openapi-spec: REST/OpenAPI exit intent — run `touring generate render OpenApiSpec \
        --vars '{{\"api_name\":\"{name}\"}}' -j` \
        to scaffold an OpenAPI spec via touring-generator"
    ))
}

/// R106-S2: Suggest `consumer_generator` when ExitPlanMode intent involves event consumers (CC=2).
pub(crate) fn maybe_consumer_generator_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const CONSUMER_KEYWORDS: &[&str] = &[
        "consumer",
        "event consumer",
        "kafka consumer",
        "message consumer",
        "event handler",
        "consume events",
        "async consumer",
        "stream consumer",
    ];
    let lower = intent.to_lowercase();
    if !CONSUMER_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | consumer-generator: consumer intent detected — run `touring generate render ConsumerGenerator \
        --vars '{{\"consumer_name\":\"{name}\"}}' -j` \
        to scaffold an event consumer via touring-generator"
    ))
}

/// R106-S3: Suggest `ffi_binding` generator when ExitPlanMode intent involves FFI/native libs (CC=2).
pub(crate) fn maybe_ffi_binding_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const FFI_KEYWORDS: &[&str] = &[
        "ffi binding",
        "native binding",
        "c binding",
        "extern ffi",
        "unsafe extern",
        "bindgen",
        "ffi wrapper",
        "native library",
    ];
    let lower = intent.to_lowercase();
    if !FFI_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | ffi-binding: FFI intent detected — run `touring generate render FfiBinding \
        --vars '{{\"lib_name\":\"{name}\"}}' -j` \
        to scaffold FFI bindings via touring-generator"
    ))
}

/// R117-S1: Suggest `diary_entry` generator when ExitPlanMode intent involves lessons/diary (CC=2).
pub(crate) fn maybe_diary_entry_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const DIARY_KEYWORDS: &[&str] = &[
        "diary entry",
        "lesson learned",
        "session note",
        "retrospective",
        "post-mortem",
        "aaak entry",
        "agent diary",
        "memory entry",
    ];
    let lower = intent.to_lowercase();
    if !DIARY_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | diary-entry: lesson/diary intent — run `touring generate render DiaryEntry \
        --vars '{{\"agent_name\":\"{name}\"}}' -j` \
        to scaffold a diary entry via touring-generator"
    ))
}

/// R117-S2: Suggest `fuzz_target` generator when ExitPlanMode intent involves fuzzing (CC=2).
pub(crate) fn maybe_fuzz_target_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const FUZZ_KEYWORDS: &[&str] = &[
        "fuzz target",
        "cargo fuzz",
        "libfuzzer",
        "afl fuzz",
        "fuzz test",
        "fuzzing",
        "fuzz corpus",
        "fuzz harness",
    ];
    let lower = intent.to_lowercase();
    if !FUZZ_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | fuzz-target: fuzz test intent — run `touring generate render FuzzTarget \
        --vars '{{\"target_name\":\"{name}\"}}' -j` \
        to scaffold a libfuzzer fuzz target via touring-generator"
    ))
}

/// R117-S3: Suggest `derive_macro` generator when ExitPlanMode intent involves proc-macros (CC=2).
pub(crate) fn maybe_derive_macro_hint_on_exit_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    const MACRO_KEYWORDS: &[&str] = &[
        "derive macro",
        "proc macro",
        "proc-macro",
        "procedural macro",
        "custom derive",
        "attribute macro",
        "macro crate",
        "derive trait",
    ];
    let lower = intent.to_lowercase();
    if !MACRO_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &intent[..intent.len().min(40)];
    Some(format!(
        " | derive-macro: proc-macro intent — run `touring generate render DeriveMacro \
        --vars '{{\"macro_name\":\"{name}\"}}' -j` \
        to scaffold a custom derive macro via touring-generator"
    ))
}
