//! Subject-keyword GeneratorKind hint matchers (Wave C2 inversion, 2026-06-10).
//!
//! Moved from touring-dispatch `lifecycle/task_create.rs` so both the dispatch
//! layer (hook_registry task-created events) and the cli layer (touring-cli
//! `cli/decompose.rs`) can consume them without an upward edge. All matchers are
//! pure `&str -> Option<String>` — zero engine state.

/// R49-S1: Detect API/endpoint/REST keywords in task subject and suggest `openapi_spec` generator (CC=2).
///
/// When TaskCreate has a subject mentioning API, endpoint, REST, HTTP, or routes,
/// surfaces `touring generate render openapi_spec` so the API contract is scaffolded
/// at task-creation time — before implementation begins.
/// Returns `None` when subject is empty or contains no API-related keywords.
pub fn maybe_openapi_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const API_KEYWORDS: &[&str] = &[
        "api", "endpoint", "rest", "http", "route", "openapi", "swagger", "graphql", "grpc",
    ];
    let lower = task_subject.to_lowercase();
    if !API_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let title = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "openapi-spec: API task detected — run `touring generate render openapi_spec \
        --vars '{{\"title\":\"{title}\",\"version\":\"1.0.0\"}}' -j` \
        to scaffold contract via touring-generator"
    ))
}

/// R50-S1: Detect proto/gRPC/RPC keywords in task subject and suggest `protobuf_schema` generator (CC=2).
///
/// When TaskCreate has a subject mentioning protobuf, proto, grpc, rpc, or buf,
/// surfaces `touring generate render protobuf_schema` so the service contract is
/// scaffolded at task-creation time — before any implementation.
/// Returns `None` when subject is empty or contains no proto-related keywords.
pub fn maybe_protobuf_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const PROTO_KEYWORDS: &[&str] = &[
        "protobuf", "proto", "grpc", "rpc", "buf", "thrift", "flatbuf",
    ];
    let lower = task_subject.to_lowercase();
    if !PROTO_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let service = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "protobuf-schema: gRPC/proto task detected — run `touring generate render protobuf_schema \
        --vars '{{\"service_name\":\"{service}\"}}' -j` \
        to scaffold service contract via touring-generator"
    ))
}

/// R51-S1: Detect fuzz/security/vulnerability keywords in task subject and suggest `fuzz_target` generator (CC=2).
///
/// When TaskCreate has a subject mentioning fuzz, fuzzing, afl, libfuzzer, security,
/// or vulnerability, surfaces `touring generate render fuzz_target` so Claude Code
/// scaffolds a fuzz harness at task-creation time — before writing any logic.
/// Returns `None` when subject is empty or contains no fuzzing-related keywords.
pub fn maybe_fuzz_target_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const FUZZ_KEYWORDS: &[&str] = &[
        "fuzz",
        "fuzzing",
        "afl",
        "libfuzzer",
        "sanitizer",
        "security audit",
        "vulnerability",
        "exploit",
        "harness",
    ];
    let lower = task_subject.to_lowercase();
    if !FUZZ_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let target = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "fuzz-target: security/fuzz task detected — run `touring generate render fuzz_target \
        --vars '{{\"target_name\":\"{target}\"}}' -j` \
        to scaffold fuzz harness via touring-generator"
    ))
}

/// R56-S1: Detect derive-macro keywords in task subject and suggest `derive_macro` generator (CC=2).
///
/// When TaskCreate has a subject mentioning derive, proc macro, proc-macro, attribute macro,
/// custom derive, or derive macro keywords, surfaces `touring generate render derive_macro` so
/// Claude Code scaffolds a procedural macro crate at task-creation time.
/// Closes the loop: TaskCreate(derive macro) → derive_macro.tera → touring-generator.
/// Returns `None` when subject is empty or contains no derive/proc-macro keywords.
pub fn maybe_derive_macro_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const MACRO_KEYWORDS: &[&str] = &[
        "derive macro",
        "proc macro",
        "proc-macro",
        "attribute macro",
        "custom derive",
        "procedural macro",
        "#[derive",
    ];
    let lower = task_subject.to_lowercase();
    if !MACRO_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "derive-macro: proc-macro task detected — run `touring generate render derive_macro \
        --vars '{{\"macro_name\":\"{name}\"}}' -j` \
        to scaffold procedural macro crate via touring-generator"
    ))
}

/// R55-S1: Detect CLI-handler keywords in task subject and suggest `cli_handler` generator (CC=2).
///
/// When TaskCreate has a subject mentioning cli command, cli handler, subcommand, clap, argparse,
/// command parser, or cli arg keywords, surfaces `touring generate render cli_handler` so Claude
/// Code scaffolds a Touring CLI command handler at task-creation time.
/// Closes the loop: TaskCreate(CLI command) → cli_handler.tera → touring-generator handler scaffold.
/// Returns `None` when subject is empty or contains no CLI-handler keywords.
pub fn maybe_cli_handler_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const CLI_HANDLER_KEYWORDS: &[&str] = &[
        "cli command",
        "cli handler",
        "clap",
        "argparse",
        "command parser",
        "cli arg",
        "subcommand",
        "command handler",
    ];
    let lower = task_subject.to_lowercase();
    if !CLI_HANDLER_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let cmd = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "cli-handler: CLI command task detected — run `touring generate render cli_handler \
        --vars '{{\"command_name\":\"{cmd}\"}}' -j` \
        to scaffold Touring CLI handler via touring-generator"
    ))
}

/// R54-S1: Detect MCP-tool keywords in task subject and suggest `mcp_tool` generator (CC=2).
///
/// When TaskCreate has a subject mentioning mcp, model context protocol, tool server, claude tool,
/// or tool endpoint keywords, surfaces `touring generate render mcp_tool` so Claude Code
/// scaffolds an MCP tool scaffold at task-creation time.
/// Closes the loop: TaskCreate(MCP tool) → mcp_tool.tera → touring-generator.
/// Returns `None` when subject is empty or contains no MCP-related keywords.
pub fn maybe_mcp_tool_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const MCP_KEYWORDS: &[&str] = &[
        "mcp",
        "model context protocol",
        "tool server",
        "claude tool",
        "tool endpoint",
        "mcp tool",
        "mcp server",
    ];
    let lower = task_subject.to_lowercase();
    if !MCP_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "mcp-tool: MCP tool task detected — run `touring generate render mcp_tool \
        --vars '{{\"tool_name\":\"{name}\"}}' -j` \
        to scaffold MCP tool handler via touring-generator"
    ))
}

/// R53-S1: Detect architecture-decision keywords in task subject and suggest `adr` generator (CC=2).
///
/// When TaskCreate has a subject mentioning architecture, architectural decision, design decision,
/// adr, decision record, system design, trade-off, or tradeoff keywords, surfaces
/// `touring generate render adr` so Claude Code scaffolds an ADR immediately at task-creation time.
/// Closes the loop: TaskCreate(architecture) → adr.tera → touring-generator decision record.
/// Returns `None` when subject is empty or contains no architecture-decision keywords.
pub fn maybe_adr_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const ADR_KEYWORDS: &[&str] = &[
        "architecture",
        "architectural",
        "adr",
        "design decision",
        "decision record",
        "system design",
        "trade-off",
        "tradeoff",
        "architectural decision",
    ];
    let lower = task_subject.to_lowercase();
    if !ADR_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let title = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "adr: architecture task detected — run `touring generate render adr \
        --vars '{{\"title\":\"{title}\",\"status\":\"proposed\"}}' -j` \
        to scaffold Architecture Decision Record via touring-generator"
    ))
}

/// R52-S1: Detect benchmark/performance keywords in task subject and suggest `benchmark` generator (CC=2).
///
/// When TaskCreate has a subject mentioning bench, benchmark, criterion, performance,
/// perf, latency, or throughput, surfaces `touring generate render benchmark` so Claude Code
/// scaffolds a Criterion benchmark target at task-creation time.
/// Returns `None` when subject is empty or contains no performance-related keywords.
pub fn maybe_benchmark_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const BENCH_KEYWORDS: &[&str] = &[
        "bench",
        "benchmark",
        "criterion",
        "perf",
        "performance",
        "latency",
        "throughput",
        "profile",
        "optimize",
    ];
    let lower = task_subject.to_lowercase();
    if !BENCH_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let target = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "benchmark: perf task detected — run `touring generate render benchmark \
        --vars '{{\"bench_name\":\"{target}\"}}' -j` \
        to scaffold Criterion target via touring-generator"
    ))
}

/// R60-S1: Suggest `incremental_patch` generator when TaskCreate subject involves patch/diff (CC=2).
///
/// When a TaskCreate subject contains incremental patch, apply patch, diff patch, patch file,
/// code patch, incremental change, or rkyv snapshot keywords, surfaces
/// `touring generate render incremental_patch` so Claude Code scaffolds a patch artifact.
/// Closes the loop: TaskCreate(patch subject) → incremental_patch.tera → touring-generator.
/// Returns `None` when subject is empty or contains no patch keywords.
pub fn maybe_incremental_patch_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const PATCH_KEYWORDS: &[&str] = &[
        "incremental patch",
        "apply patch",
        "diff patch",
        "patch file",
        "code patch",
        "incremental change",
        "patch set",
        "delta patch",
    ];
    let lower = task_subject.to_lowercase();
    if !PATCH_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "incremental-patch: patch task detected — run `touring generate render incremental_patch \
        --vars '{{\"patch_name\":\"{name}\"}}' -j` \
        to scaffold incremental patch artifact via touring-generator"
    ))
}

/// R62-S1: Suggest `task_scaffold` generator when TaskCreate subject involves task planning (CC=2).
///
/// When a TaskCreate subject contains "scaffold task", "taco", "decompose", "dag task", or
/// "task plan" keywords, surfaces `touring generate render task_scaffold` so Claude Code
/// scaffolds a TACO-compatible task DAG at creation time.
/// Closes the loop: TaskCreate(planning subject) → task_scaffold.tera → touring-generator.
/// Returns `None` when subject is empty or contains no task-planning keywords.
pub fn maybe_task_scaffold_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const TASK_KEYWORDS: &[&str] = &[
        "scaffold task",
        "taco task",
        "decompose task",
        "dag task",
        "task plan",
        "task scaffold",
        "create dag",
        "decompose dag",
        "plan subtasks",
        "taco phase",
    ];
    let lower = task_subject.to_lowercase();
    if !TASK_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "task-scaffold: DAG task detected — run `touring generate render task_scaffold \
        --vars '{{\"task_id\":\"{name}\"}}' -j` \
        to scaffold TACO task DAG via touring-generator"
    ))
}

/// R62-S2: Suggest `diary_entry` generator when TaskCreate subject involves retrospective work (CC=2).
///
/// When a TaskCreate subject contains "diary", "lesson learned", "retrospective", "postmortem",
/// "after-action", or "learnings" keywords, surfaces `touring generate render diary_entry`
/// so Claude Code scaffolds an AAAK-format diary entry at task creation time.
/// Closes the loop: TaskCreate(retrospective subject) → diary_entry.tera → touring-generator.
/// Returns `None` when subject is empty or contains no retrospective keywords.
pub fn maybe_diary_entry_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const DIARY_KEYWORDS: &[&str] = &[
        "diary entry",
        "lesson learned",
        "retrospective",
        "postmortem",
        "after-action",
        "learnings",
        "debrief",
        "incident review",
        "write diary",
        "aaak entry",
    ];
    let lower = task_subject.to_lowercase();
    if !DIARY_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "diary-entry: retrospective task detected — run `touring generate render diary_entry \
        --vars '{{\"agent\":\"claude_code\",\"topic\":\"{name}\"}}' -j` \
        to scaffold AAAK diary entry via touring-generator"
    ))
}

/// R62-S3: Suggest `skill_document` generator when TaskCreate subject involves skill authoring (CC=2).
///
/// When a TaskCreate subject contains "skill", "skill document", "claude skill", "touring skill",
/// or "agent skill" keywords, surfaces `touring generate render skill_document` so Claude Code
/// scaffolds a skill SKILL.md at task creation time.
/// Closes the loop: TaskCreate(skill subject) → skill_document.tera → touring-generator.
/// Returns `None` when subject is empty or contains no skill-authoring keywords.
pub fn maybe_skill_document_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const SKILL_KEYWORDS: &[&str] = &[
        "skill document",
        "claude skill",
        "touring skill",
        "agent skill",
        "create skill",
        "new skill",
        "write skill",
        "skill.md",
        "skill template",
        "skill scaffold",
    ];
    let lower = task_subject.to_lowercase();
    if !SKILL_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "skill-document: skill authoring task detected — run `touring generate render skill_document \
        --vars '{{\"skill_name\":\"{name}\"}}' -j` \
        to scaffold SKILL.md via touring-generator"
    ))
}

/// R59-S3: Suggest `k8s_manifest` generator when TaskCreate subject involves Kubernetes work (CC=2).
///
/// When a TaskCreate subject contains kubernetes, k8s, helm chart, deploy to cluster, pod spec,
/// ingress, namespace, or kustomize keywords, surfaces `touring generate render k8s_manifest`
/// so Claude Code scaffolds a Kubernetes manifest at task creation time.
/// Closes the loop: TaskCreate(k8s subject) → k8s_manifest.tera → touring-generator.
/// Returns `None` when subject is empty or contains no Kubernetes keywords.
pub fn maybe_k8s_manifest_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const K8S_KEYWORDS: &[&str] = &[
        "kubernetes",
        "k8s",
        "helm chart",
        "deploy to cluster",
        "pod spec",
        "ingress",
        "kustomize",
        "kubectl",
        "deployment yaml",
    ];
    let lower = task_subject.to_lowercase();
    if !K8S_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "k8s-manifest: Kubernetes task detected — run `touring generate render k8s_manifest \
        --vars '{{\"app_name\":\"{name}\"}}' -j` \
        to scaffold Kubernetes manifest via touring-generator"
    ))
}

/// R59-S2: Suggest `ci_workflow` generator when TaskCreate subject involves CI/CD pipeline (CC=2).
///
/// When a TaskCreate subject contains github actions, ci/cd, ci pipeline, github workflow,
/// continuous integration, build pipeline, or release workflow keywords, surfaces
/// `touring generate render ci_workflow` so Claude Code scaffolds a CI workflow at task creation.
/// Closes the loop: TaskCreate(CI/CD subject) → ci_workflow.tera → touring-generator.
/// Returns `None` when subject is empty or contains no CI/CD keywords.
pub fn maybe_ci_workflow_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const CI_KEYWORDS: &[&str] = &[
        "github actions",
        "ci/cd",
        "ci pipeline",
        "github workflow",
        "continuous integration",
        "build pipeline",
        "release workflow",
        "workflow yaml",
        "gitlab ci",
    ];
    let lower = task_subject.to_lowercase();
    if !CI_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "ci-workflow: CI/CD task detected — run `touring generate render ci_workflow \
        --vars '{{\"workflow_name\":\"{name}\"}}' -j` \
        to scaffold CI workflow via touring-generator"
    ))
}

/// R59-S1: Suggest `terraform_module` generator when TaskCreate subject involves IaC (CC=2).
///
/// When a TaskCreate subject contains terraform, opentofu, infrastructure as code, iac, aws vpc,
/// aws iam, provision infrastructure, or tf module keywords, surfaces
/// `touring generate render terraform_module` so Claude Code scaffolds a Terraform module.
/// Closes the loop: TaskCreate(IaC subject) → terraform_module.tera → touring-generator.
/// Returns `None` when subject is empty or contains no IaC keywords.
pub fn maybe_terraform_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const TERRAFORM_KEYWORDS: &[&str] = &[
        "terraform",
        "opentofu",
        "infrastructure as code",
        "iac",
        "aws vpc",
        "aws iam",
        "provision infrastructure",
        "tf module",
        "hcl module",
    ];
    let lower = task_subject.to_lowercase();
    if !TERRAFORM_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "terraform-module: IaC task detected — run `touring generate render terraform_module \
        --vars '{{\"module_name\":\"{name}\"}}' -j` \
        to scaffold Terraform module via touring-generator"
    ))
}

/// R58-S2: Suggest `rust_module` generator when TaskCreate subject involves new Rust module work (CC=2).
///
/// When a TaskCreate subject contains new module, implement struct, define trait, new crate,
/// rust module, add impl, or pub mod keywords, surfaces `touring generate render rust_module`
/// so Claude Code scaffolds a Rust module skeleton at task creation time.
/// Closes the loop: TaskCreate(Rust module subject) → rust_module.tera → touring-generator.
/// Returns `None` when subject is empty or contains no Rust module keywords.
pub fn maybe_rust_module_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const MODULE_KEYWORDS: &[&str] = &[
        "new module",
        "new crate",
        "rust module",
        "add module",
        "implement trait",
        "define trait",
        "new trait",
        "pub mod",
        "create struct",
        "new struct",
        "implement struct",
    ];
    let lower = task_subject.to_lowercase();
    if !MODULE_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "rust-module: Rust module task detected — run `touring generate render rust_module \
        --vars '{{\"module_name\":\"{name}\"}}' -j` \
        to scaffold Rust module skeleton via touring-generator"
    ))
}

/// R58-S1: Suggest `consumer_generator` when TaskCreate subject involves wiring/consumer work (CC=2).
///
/// When a TaskCreate subject contains wire consumer, connect module, wiring consumer, orphan symbol,
/// or wire into keywords, surfaces `touring generate render consumer_generator` so Claude Code
/// scaffolds a consumer wiring module at task creation time.
/// Closes the loop: TaskCreate(wiring subject) → consumer_generator.tera → touring-generator.
/// Returns `None` when subject is empty or contains no consumer wiring keywords.
pub fn maybe_consumer_generator_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const CONSUMER_KEYWORDS: &[&str] = &[
        "wire consumer",
        "connect module",
        "wiring consumer",
        "orphan symbol",
        "wire into",
        "consumer wiring",
        "wire orphan",
        "touring consumer",
    ];
    let lower = task_subject.to_lowercase();
    if !CONSUMER_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "consumer-generator: wiring task detected — run `touring generate render consumer_generator \
        --vars '{{\"module_name\":\"{name}\"}}' -j` \
        to scaffold consumer wiring module via touring-generator"
    ))
}

/// R57-S1: Suggest `schema` generator when TaskCreate subject involves JSON/schema definition (CC=2).
///
/// When a TaskCreate subject contains schema, json schema, openapi schema, avro, jsonschema,
/// data model, or schema definition keywords, surfaces `touring generate render schema`
/// so Claude Code scaffolds a schema definition artifact at task creation time.
/// Closes the loop: TaskCreate(schema subject) → schema.tera → touring-generator.
/// Returns `None` when subject is empty or contains no schema keywords.
pub fn maybe_schema_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const SCHEMA_KEYWORDS: &[&str] = &[
        "json schema",
        "jsonschema",
        "avro schema",
        "data model",
        "schema definition",
        "schema validation",
        "schema registry",
        "openapi schema",
        "graphql schema",
    ];
    let lower = task_subject.to_lowercase();
    if !SCHEMA_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "schema: schema definition task detected — run `touring generate render schema \
        --vars '{{\"schema_name\":\"{name}\"}}' -j` \
        to scaffold schema definition via touring-generator"
    ))
}

/// R67-S1: When task subject contains AsyncAPI/event-driven keywords, suggest asyncapi_spec (CC≤2).
///
/// Keywords: asyncapi, event-driven api, async api, message broker, amqp, event streaming, kafka api.
/// Returns a hint to run `touring generate render asyncapi_spec` via touring-generator.
/// Bridges TaskCreate(event-driven API tasks) → asyncapi_spec.tera → AsyncAPI specification scaffold.
pub fn maybe_asyncapi_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const ASYNCAPI_KEYWORDS: &[&str] = &[
        "asyncapi",
        "async api",
        "event-driven api",
        "event driven api",
        "message broker",
        "amqp",
        "event streaming",
        "kafka api",
        "pubsub api",
    ];
    let lower = task_subject.to_lowercase();
    if !ASYNCAPI_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "asyncapi: event-driven API task detected — run `touring generate render asyncapi_spec \
        --vars '{{\"title\":\"{name}\"}}' -j` \
        to scaffold AsyncAPI specification via touring-generator"
    ))
}

/// R67-S2: When task subject contains man-page/documentation keywords, suggest man_page (CC≤2).
///
/// Keywords: man page, manual page, unix docs, binary documentation, help text, --help output.
/// Returns a hint to run `touring generate render man_page` via touring-generator.
/// Bridges TaskCreate(documentation tasks) → man_page.tera → Unix man page scaffold.
pub fn maybe_man_page_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const MAN_PAGE_KEYWORDS: &[&str] = &[
        "man page",
        "manual page",
        "man section",
        "unix docs",
        "binary documentation",
        "help text",
        "groff",
        "troff",
        "man format",
    ];
    let lower = task_subject.to_lowercase();
    if !MAN_PAGE_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "man-page: documentation task detected — run `touring generate render man_page \
        --vars '{{\"command_name\":\"{name}\"}}' -j` \
        to scaffold a Unix man page via touring-generator"
    ))
}

/// R67-S3: When task subject contains error-catalog keywords, suggest error_catalog (CC≤2).
///
/// Keywords: error catalog, error codes, error registry, error enum, custom errors, thiserror.
/// Returns a hint to run `touring generate render error_catalog` via touring-generator.
/// Bridges TaskCreate(error-handling tasks) → error_catalog.tera → error catalog scaffold.
pub fn maybe_error_catalog_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const ERROR_CATALOG_KEYWORDS: &[&str] = &[
        "error catalog",
        "error codes",
        "error registry",
        "error enum",
        "custom errors",
        "thiserror",
        "error variants",
        "error types",
    ];
    let lower = task_subject.to_lowercase();
    if !ERROR_CATALOG_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "error-catalog: error-type task detected — run `touring generate render error_catalog \
        --vars '{{\"module_name\":\"{name}\"}}' -j` \
        to scaffold an error catalog via touring-generator"
    ))
}

/// R73-S1: Suggest `changelog_entry` generator when task subject mentions release/semver patterns (CC=2).
///
/// Bridges TaskCreate(release intent) → changelog_entry.tera → touring-generator ChangelogEntry scaffold.
pub fn maybe_changelog_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const CHANGELOG_KEYWORDS: &[&str] = &[
        "changelog",
        "release notes",
        "version bump",
        "semver",
        "breaking change",
        "release entry",
        "release candidate",
    ];
    let lower = task_subject.to_lowercase();
    if !CHANGELOG_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "changelog: release task detected — run `touring generate render ChangelogEntry \
        --vars '{{\"version\":\"{name}\"}}' -j` \
        to scaffold a changelog entry via touring-generator"
    ))
}

/// R73-S2: Suggest `dockerfile` generator when task subject mentions container/docker patterns (CC=2).
///
/// Bridges TaskCreate(container intent) → dockerfile.tera → touring-generator Dockerfile scaffold.
pub fn maybe_dockerfile_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const DOCKERFILE_KEYWORDS: &[&str] = &[
        "dockerfile",
        "docker build",
        "containerize",
        "docker image",
        "container setup",
        "docker compose",
        "docker layer",
    ];
    let lower = task_subject.to_lowercase();
    if !DOCKERFILE_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "dockerfile: container task detected — run `touring generate render Dockerfile \
        --vars '{{\"service_name\":\"{name}\"}}' -j` \
        to scaffold a Dockerfile via touring-generator"
    ))
}

/// R73-S3: Suggest `migration` generator when task subject mentions DB migration patterns (CC=2).
///
/// Bridges TaskCreate(db migration intent) → migration.tera → touring-generator Migration scaffold.
pub fn maybe_migration_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const MIGRATION_KEYWORDS: &[&str] = &[
        "migration",
        "db migration",
        "database migration",
        "schema change",
        "alter table",
        "create table",
        "database schema",
    ];
    let lower = task_subject.to_lowercase();
    if !MIGRATION_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "migration: DB migration task detected — run `touring generate render migration \
        --vars '{{\"migration_name\":\"{name}\"}}' -j` \
        to scaffold a SQL migration via touring-generator"
    ))
}

/// R74-S1: Suggest `python_script` generator when task subject mentions Python patterns (CC=2).
///
/// Bridges TaskCreate(python intent) → python_script.tera → touring-generator PythonScript scaffold.
pub fn maybe_python_script_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const PYTHON_KEYWORDS: &[&str] = &[
        "python script",
        "python module",
        "py script",
        "fastapi route",
        "django view",
        "pydantic model",
        "asyncio task",
        "python tool",
    ];
    let lower = task_subject.to_lowercase();
    if !PYTHON_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "python: Python task detected — run `touring generate render PythonScript \
        --vars '{{\"script_name\":\"{name}\"}}' -j` \
        to scaffold a Python script via touring-generator"
    ))
}

/// R74-S2: Suggest `test` generator when task subject mentions testing patterns (CC=2).
///
/// Bridges TaskCreate(test intent) → test.tera → touring-generator Test scaffold.
pub fn maybe_test_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const TEST_KEYWORDS: &[&str] = &[
        "unit test",
        "integration test",
        "test suite",
        "test coverage",
        "write tests",
        "add tests",
        "cargo test",
        "test module",
    ];
    let lower = task_subject.to_lowercase();
    if !TEST_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "test: testing task detected — run `touring generate render Test \
        --vars '{{\"module_name\":\"{name}\"}}' -j` \
        to scaffold a test module via touring-generator"
    ))
}

/// R74-S3: Suggest `shell_completion` generator when task subject mentions CLI completion (CC=2).
///
/// Bridges TaskCreate(completion intent) → shell_completion.tera → touring-generator ShellCompletion scaffold.
pub fn maybe_shell_completion_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const COMPLETION_KEYWORDS: &[&str] = &[
        "shell completion",
        "bash completion",
        "zsh completion",
        "fish completion",
        "cli completion",
        "tab completion",
        "completions script",
    ];
    let lower = task_subject.to_lowercase();
    if !COMPLETION_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "shell-completion: CLI completion task detected — run `touring generate render ShellCompletion \
        --vars '{{\"cli_name\":\"{name}\"}}' -j` \
        to scaffold shell completions via touring-generator"
    ))
}

/// R75-S1: hint for HookHandler generator (CC=2).
///
/// Fires when the task subject mentions Claude Code lifecycle hooks, event handlers,
/// or hook registration — suggesting `touring generate render HookHandler`.
pub fn maybe_hook_handler_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const HOOK_KEYWORDS: &[&str] = &[
        "hook handler",
        "lifecycle hook",
        "pre-edit hook",
        "post-edit hook",
        "pre-bash hook",
        "post-bash hook",
        "hook event",
        "claude hook",
        "hook registration",
        "hook registry",
    ];
    let lower = task_subject.to_lowercase();
    if !HOOK_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "hook-handler: hook task detected — run `touring generate render HookHandler \
        --vars '{{\"hook_name\":\"{name}\"}}' -j` \
        to scaffold a Claude Code hook handler via touring-generator"
    ))
}

/// R75-S2: hint for plan.md generator (CC=2).
///
/// Fires when the task subject mentions implementation plans, architecture plans,
/// or design documents — suggesting `touring generate render PlanMd`.
pub fn maybe_plan_md_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const PLAN_KEYWORDS: &[&str] = &[
        "implementation plan",
        "architecture plan",
        "design document",
        "technical spec",
        "planning document",
        "plan.md",
        "plan document",
        "roadmap document",
        "feature plan",
    ];
    let lower = task_subject.to_lowercase();
    if !PLAN_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "plan-md: planning task detected — run `touring generate render PlanMd \
        --vars '{{\"plan_title\":\"{name}\"}}' -j` \
        to scaffold a structured implementation plan via touring-generator"
    ))
}

/// R75-S3: hint for FfiBinding generator (CC=2).
///
/// Fires when the task subject mentions FFI, C bindings, foreign functions,
/// or interop — suggesting `touring generate render FfiBinding`.
pub fn maybe_ffi_binding_hint_on_task_create(task_subject: &str) -> Option<String> {
    if task_subject.is_empty() {
        return None;
    }
    const FFI_KEYWORDS: &[&str] = &[
        "ffi binding",
        "c binding",
        "foreign function",
        "extern c",
        "interop layer",
        "native binding",
        "ffi wrapper",
        "cffi",
        "c interop",
        "unsafe extern",
    ];
    let lower = task_subject.to_lowercase();
    if !FFI_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let name = &task_subject[..task_subject.len().min(40)];
    Some(format!(
        "ffi-binding: FFI task detected — run `touring generate render FfiBinding \
        --vars '{{\"lib_name\":\"{name}\"}}' -j` \
        to scaffold an FFI binding module via touring-generator"
    ))
}

/// R49-R52-S1 dispatcher: collect all subject-keyword GeneratorKind hints (CC=2).
///
/// Calls all `maybe_*_hint_on_task_create` helpers in a single pass and returns
/// matched hints as a `Vec<String>`. The caller uses `parts.extend(...)` — zero
/// extra CC branches in `handle_task_sync_post_create`.
///
/// `pub(crate)` so `hook_registry.rs` can use this in the `task-created` event
/// handler (R122-S1) to surface ALL matching GeneratorKind hints, not just first-match.
pub fn collect_subject_generator_hints(task_subject: &str) -> Vec<String> {
    type HintFn = fn(&str) -> Option<String>;
    const MATCHERS: &[HintFn] = &[
        maybe_openapi_hint_on_task_create,
        maybe_protobuf_hint_on_task_create,
        maybe_fuzz_target_hint_on_task_create,
        maybe_benchmark_hint_on_task_create,
        maybe_adr_hint_on_task_create,
        maybe_mcp_tool_hint_on_task_create,
        maybe_cli_handler_hint_on_task_create,
        maybe_derive_macro_hint_on_task_create,
        maybe_schema_hint_on_task_create,
        maybe_rust_module_hint_on_task_create,
        maybe_consumer_generator_hint_on_task_create,
        maybe_terraform_hint_on_task_create,
        maybe_ci_workflow_hint_on_task_create,
        maybe_k8s_manifest_hint_on_task_create,
        maybe_incremental_patch_hint_on_task_create,
        // R62: task lifecycle templates — task_scaffold, diary_entry, skill_document
        maybe_task_scaffold_hint_on_task_create,
        maybe_diary_entry_hint_on_task_create,
        maybe_skill_document_hint_on_task_create,
        // R67: remaining templates — asyncapi_spec, man_page, error_catalog
        maybe_asyncapi_hint_on_task_create,
        maybe_man_page_hint_on_task_create,
        maybe_error_catalog_hint_on_task_create,
        // R73: changelog_entry, dockerfile, migration
        maybe_changelog_hint_on_task_create,
        maybe_dockerfile_hint_on_task_create,
        maybe_migration_hint_on_task_create,
        // R74: python_script, test, shell_completion
        maybe_python_script_hint_on_task_create,
        maybe_test_hint_on_task_create,
        maybe_shell_completion_hint_on_task_create,
        // R75: hook_handler, plan_md, ffi_binding — completes 30/30 GeneratorKind coverage
        maybe_hook_handler_hint_on_task_create,
        maybe_plan_md_hint_on_task_create,
        maybe_ffi_binding_hint_on_task_create,
    ];
    MATCHERS.iter().filter_map(|f| f(task_subject)).collect()
}
