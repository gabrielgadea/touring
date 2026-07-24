//! `GeneratorKind` — 35 artifact kinds supported by touring-generator.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// All artifact kinds that the generator can produce.
///
/// Each kind maps to one `.tera` template and one set of
/// default variable bindings.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[non_exhaustive]
pub enum GeneratorKind {
    // ── Rust source ──────────────────────────────────────────────────────────
    /// A Rust module (`mod.rs` or named module file).
    RustModule,
    /// A CLI handler registered in `command_table()`.
    CliHandler,
    /// An MCP tool method with `#[tool]` attribute.
    McpTool,
    /// A hook handler function for the hook registry.
    HookHandler,
    /// A unit or integration test module.
    Test,
    /// A criterion benchmark suite.
    BenchmarkSuite,
    /// A cargo-fuzz fuzz target.
    FuzzTarget,
    /// A proc-macro derive implementation.
    DeriveMacro,
    /// A proc-macro attribute implementation.
    AttributeMacro,
    /// A proc-macro function-like implementation.
    FunctionMacro,
    /// An error catalog from thiserror variants.
    ErrorCatalog,
    /// A unified diff / incremental patch (does not replace full file).
    IncrementalPatch,
    /// An FFI binding (C/C++ header via cbindgen).
    FfiBinding,

    // ── Data / schema ────────────────────────────────────────────────────────
    /// A JSON Schema document.
    Schema,
    /// A SQL DDL migration script.
    MigrationScript,
    /// A Protocol Buffers `.proto` file.
    ProtoBufSchema,
    /// An `OpenAPI` 3.1 YAML specification.
    OpenApiSpec,
    /// An `AsyncAPI` YAML specification.
    AsyncApiSpec,

    // ── Documentation ────────────────────────────────────────────────────────
    /// A TACO phase plan in Markdown.
    PlanMarkdown,
    /// A SKILL.md workspace skill document.
    SkillDocument,
    /// A diary entry in AAAK format.
    DiaryEntry,
    /// A keep-a-changelog entry.
    ChangelogEntry,
    /// An Architecture Decision Record (ADR).
    Adr,
    /// A shell completion script (bash/zsh/fish).
    ShellCompletion,
    /// A roff man page.
    ManPage,

    // ── Infrastructure ───────────────────────────────────────────────────────
    /// A Python script (Python 3.12+ with type hints).
    PythonScript,
    /// A TypeScript module (`.ts` file with strict type annotations).
    ///
    /// Sprint 8 P3: target supports `target: { file_path: "src/<name>.ts" }`
    /// + `extra_vars` `{name, description, exported_fn}`. Renders ESM
    ///   `export function <name>` boilerplate with strict TS types.
    TypeScriptModule,
    /// A Dockerfile (multi-stage build).
    DockerImage,
    /// A Kubernetes manifest (Deployment/Service/Ingress).
    KubernetesManifest,
    /// A Terraform module (`.tf` files).
    TerraformModule,
    /// A CI workflow (GitHub Actions / GitLab CI).
    CiWorkflow,

    // ── Wiring ───────────────────────────────────────────────────────────────
    /// A consumer call-site that wires an orphan pub symbol into active use.
    ///
    /// Generated from wiring suggestions produced by `touring wiring orphans`.
    /// The template receives `orphan_symbol`, `orphan_crate`, `consumer_fn_name`,
    /// and an optional `wiring_suggestion_id`.
    ConsumerGenerator,

    // ── Task orchestration ───────────────────────────────────────────────────
    /// A 3-subtask scaffold (scout → implement → validate) for a Claude Code task.
    ///
    /// Generated when a `TaskCreate` event fires. Template receives `task_id`,
    /// `intent`, and an optional `phase`. Produces YAML-like DAG definition
    /// and ready-to-paste `touring decompose add` CLI commands.
    TaskScaffold,

    // ── Crate scaffolding ────────────────────────────────────────────────────
    /// A `Cargo.toml` for a new foundational Rust crate (no touring-* deps).
    ///
    /// Template receives `crate_name`, `description`, optional `edition`,
    /// `rust_version`, `with_bin`, `bin_name`, `bin_path`, `with_linux_deps`,
    /// `optional_features`, `dev_deps`, and `extra_deps`.
    RustFoundationalCrateCargoToml,

    /// A `src/lib.rs` skeleton for a new Rust crate.
    ///
    /// Template receives `crate_name`, `description`, `modules`,
    /// `linux_only_modules`, and `re_exports`.
    RustCrateLibRs,

    /// A systemd user-service unit file (`.config/systemd/user/<name>.service`).
    ///
    /// Template receives `service_name`, `description`, `binary_path`,
    /// `after_services`, `wants_services`, `rust_log`, `tokio_workers`,
    /// `memory_max`, `tasks_max`, and `read_write_paths`.
    SystemdUserService,
}

impl GeneratorKind {
    /// Returns the canonical template file name for this kind.
    #[must_use]
    pub fn template_name(&self) -> &'static str {
        match self {
            Self::RustModule => "rust_module.tera",
            Self::CliHandler => "cli_handler.tera",
            Self::McpTool => "mcp_tool.tera",
            Self::HookHandler => "hook_handler.tera",
            Self::Test => "test.tera",
            Self::BenchmarkSuite => "benchmark.tera",
            Self::FuzzTarget => "fuzz_target.tera",
            Self::DeriveMacro | Self::AttributeMacro | Self::FunctionMacro => "derive_macro.tera",
            Self::ErrorCatalog => "error_catalog.tera",
            Self::IncrementalPatch => "incremental_patch.tera",
            Self::FfiBinding => "ffi_binding.tera",
            Self::Schema => "schema.tera",
            Self::MigrationScript => "migration.tera",
            Self::ProtoBufSchema => "protobuf_schema.tera",
            Self::OpenApiSpec => "openapi_spec.tera",
            Self::AsyncApiSpec => "asyncapi_spec.tera",
            Self::PlanMarkdown => "plan.md.tera",
            Self::SkillDocument => "skill_document.tera",
            Self::DiaryEntry => "diary_entry.tera",
            Self::ChangelogEntry => "changelog_entry.tera",
            Self::Adr => "adr.tera",
            Self::ShellCompletion => "shell_completion.tera",
            Self::ManPage => "man_page.tera",
            Self::PythonScript => "python_script.tera",
            Self::TypeScriptModule => "typescript_module.tera",
            Self::DockerImage => "dockerfile.tera",
            Self::KubernetesManifest => "k8s_manifest.tera",
            Self::TerraformModule => "terraform_module.tera",
            Self::CiWorkflow => "ci_workflow.tera",
            Self::ConsumerGenerator => "consumer_generator.tera",
            Self::TaskScaffold => "task_scaffold.tera",
            Self::RustFoundationalCrateCargoToml => "rust_foundational_crate_cargo_toml.tera",
            Self::RustCrateLibRs => "rust_crate_lib_rs.tera",
            Self::SystemdUserService => "systemd_user_service.tera",
        }
    }

    /// Returns a human-readable label for this kind.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::RustModule => "Rust Module",
            Self::CliHandler => "CLI Handler",
            Self::McpTool => "MCP Tool",
            Self::HookHandler => "Hook Handler",
            Self::Test => "Test",
            Self::BenchmarkSuite => "Benchmark Suite",
            Self::FuzzTarget => "Fuzz Target",
            Self::DeriveMacro => "Derive Macro",
            Self::AttributeMacro => "Attribute Macro",
            Self::FunctionMacro => "Function Macro",
            Self::ErrorCatalog => "Error Catalog",
            Self::IncrementalPatch => "Incremental Patch",
            Self::FfiBinding => "FFI Binding",
            Self::Schema => "JSON Schema",
            Self::MigrationScript => "Migration Script",
            Self::ProtoBufSchema => "ProtoBuf Schema",
            Self::OpenApiSpec => "OpenAPI Spec",
            Self::AsyncApiSpec => "AsyncAPI Spec",
            Self::PlanMarkdown => "Plan (Markdown)",
            Self::SkillDocument => "Skill Document",
            Self::DiaryEntry => "Diary Entry",
            Self::ChangelogEntry => "Changelog Entry",
            Self::Adr => "Architecture Decision Record",
            Self::ShellCompletion => "Shell Completion",
            Self::ManPage => "Man Page",
            Self::PythonScript => "Python Script",
            Self::TypeScriptModule => "TypeScript Module",
            Self::DockerImage => "Dockerfile",
            Self::KubernetesManifest => "Kubernetes Manifest",
            Self::TerraformModule => "Terraform Module",
            Self::CiWorkflow => "CI Workflow",
            Self::ConsumerGenerator => "Consumer Generator",
            Self::TaskScaffold => "Task Scaffold",
            Self::RustFoundationalCrateCargoToml => "Rust Foundational Crate Cargo.toml",
            Self::RustCrateLibRs => "Rust Crate lib.rs",
            Self::SystemdUserService => "Systemd User Service",
        }
    }
}

impl std::fmt::Display for GeneratorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}
