//! `TemplateEngine` — `OnceLock<Tera>` pre-compiled at first use.
//!
//! Fix B7: eliminates per-call `Tera::one_off()` compilation overhead (~90% reduction).
//! All 32 templates are embedded at compile time via `include_str!`.

use crate::core::context::TelemetrySink;
use crate::error::{GenerateError, RenderEngine};
use crate::generator::kinds::GeneratorKind;
use crate::template::allowlist::VariableAllowlist;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tera::{Context, Tera};

// ── Embedded templates (compile-time) ────────────────────────────────────────

static TEMPLATES: OnceLock<Tera> = OnceLock::new();

#[allow(clippy::too_many_lines)]
fn templates() -> &'static Tera {
    TEMPLATES.get_or_init(|| {
        let mut tera = Tera::default();
        // Disable HTML auto-escaping — we generate source code, not HTML.
        tera.autoescape_on(vec![]);

        // SAFETY: all template strings are embedded at compile time via `include_str!`.
        // A parse failure here means the template file contains a Tera syntax error,
        // which is a programmer error — correct the template source and recompile.
        #[allow(clippy::expect_used)]
        tera.add_raw_templates(vec![
            (
                "rust_module.tera",
                include_str!("../../templates/rust_module.tera"),
            ),
            (
                "cli_handler.tera",
                include_str!("../../templates/cli_handler.tera"),
            ),
            (
                "mcp_tool.tera",
                include_str!("../../templates/mcp_tool.tera"),
            ),
            (
                "hook_handler.tera",
                include_str!("../../templates/hook_handler.tera"),
            ),
            ("plan.md.tera", include_str!("../../templates/plan.md.tera")),
            ("test.tera", include_str!("../../templates/test.tera")),
            (
                "python_script.tera",
                include_str!("../../templates/python_script.tera"),
            ),
            (
                "typescript_module.tera",
                include_str!("../../templates/typescript_module.tera"),
            ),
            ("schema.tera", include_str!("../../templates/schema.tera")),
            (
                "benchmark.tera",
                include_str!("../../templates/benchmark.tera"),
            ),
            (
                "fuzz_target.tera",
                include_str!("../../templates/fuzz_target.tera"),
            ),
            (
                "derive_macro.tera",
                include_str!("../../templates/derive_macro.tera"),
            ),
            (
                "migration.tera",
                include_str!("../../templates/migration.tera"),
            ),
            (
                "ffi_binding.tera",
                include_str!("../../templates/ffi_binding.tera"),
            ),
            (
                "protobuf_schema.tera",
                include_str!("../../templates/protobuf_schema.tera"),
            ),
            (
                "openapi_spec.tera",
                include_str!("../../templates/openapi_spec.tera"),
            ),
            (
                "shell_completion.tera",
                include_str!("../../templates/shell_completion.tera"),
            ),
            (
                "man_page.tera",
                include_str!("../../templates/man_page.tera"),
            ),
            (
                "error_catalog.tera",
                include_str!("../../templates/error_catalog.tera"),
            ),
            (
                "incremental_patch.tera",
                include_str!("../../templates/incremental_patch.tera"),
            ),
            (
                "skill_document.tera",
                include_str!("../../templates/skill_document.tera"),
            ),
            (
                "diary_entry.tera",
                include_str!("../../templates/diary_entry.tera"),
            ),
            (
                "dockerfile.tera",
                include_str!("../../templates/dockerfile.tera"),
            ),
            (
                "k8s_manifest.tera",
                include_str!("../../templates/k8s_manifest.tera"),
            ),
            (
                "terraform_module.tera",
                include_str!("../../templates/terraform_module.tera"),
            ),
            (
                "ci_workflow.tera",
                include_str!("../../templates/ci_workflow.tera"),
            ),
            (
                "changelog_entry.tera",
                include_str!("../../templates/changelog_entry.tera"),
            ),
            ("adr.tera", include_str!("../../templates/adr.tera")),
            (
                "asyncapi_spec.tera",
                include_str!("../../templates/asyncapi_spec.tera"),
            ),
            (
                "consumer_generator.tera",
                include_str!("../../templates/consumer_generator.tera"),
            ),
            (
                "task_scaffold.tera",
                include_str!("../../templates/task_scaffold.tera"),
            ),
            (
                "rust_foundational_crate_cargo_toml.tera",
                include_str!("../../templates/rust_foundational_crate_cargo_toml.tera"),
            ),
            (
                "rust_crate_lib_rs.tera",
                include_str!("../../templates/rust_crate_lib_rs.tera"),
            ),
            (
                "systemd_user_service.tera",
                include_str!("../../templates/systemd_user_service.tera"),
            ),
        ])
        .expect("embedded templates must parse at compile time — fix template syntax");

        tera
    })
}

// ── TemplateEngine ────────────────────────────────────────────────────────────

/// Thread-safe template rendering engine backed by a pre-compiled `OnceLock<Tera>`.
pub struct TemplateEngine {
    variable_validator: VariableAllowlist,
    metrics: Arc<dyn TelemetrySink>,
}

impl TemplateEngine {
    /// Construct the engine with a telemetry sink.
    pub fn new(metrics: Arc<dyn TelemetrySink>) -> Self {
        // Eagerly initialize the OnceLock so the first render call has no cold start.
        let _ = templates();
        Self {
            variable_validator: VariableAllowlist::new(),
            metrics,
        }
    }

    /// Render a template by its file name.
    ///
    /// `template_id` must be one of the 29 registered names (e.g. `"rust_module.tera"`).
    /// All `variables` keys are validated against the allowlist before rendering.
    ///
    /// # Errors
    ///
    /// Returns [`GenerateError::TemplateVariableRejected`] if any variable key fails
    /// allowlist validation, or [`GenerateError::TemplateError`] if rendering fails.
    pub fn render(
        &self,
        template_id: &str,
        variables: &HashMap<String, serde_json::Value>,
    ) -> Result<String, GenerateError> {
        self.variable_validator.validate(variables)?;

        let start = std::time::Instant::now();

        let context =
            Context::from_serialize(variables).map_err(|e| GenerateError::TemplateError {
                engine: RenderEngine::Tera,
                message: e.to_string(),
            })?;

        let output = templates().render(template_id, &context).map_err(|e| {
            GenerateError::TemplateError {
                engine: RenderEngine::Tera,
                message: e.to_string(),
            }
        })?;

        let elapsed_us = start.elapsed().as_secs_f64() * 1_000_000.0;
        self.metrics
            .record_histogram("template.render.latency_us", elapsed_us);

        Ok(output)
    }

    /// Render the default template for a [`GeneratorKind`].
    ///
    /// # Errors
    ///
    /// Returns [`GenerateError::TemplateVariableRejected`] or [`GenerateError::TemplateError`]
    /// — same conditions as [`TemplateEngine::render`].
    pub fn render_for_kind(
        &self,
        kind: &GeneratorKind,
        variables: &HashMap<String, serde_json::Value>,
    ) -> Result<String, GenerateError> {
        self.render(kind.template_name(), variables)
    }

    /// Returns the list of all registered template names.
    #[must_use]
    pub fn template_names() -> &'static [&'static str] {
        &[
            "rust_module.tera",
            "cli_handler.tera",
            "mcp_tool.tera",
            "hook_handler.tera",
            "plan.md.tera",
            "test.tera",
            "python_script.tera",
            "typescript_module.tera",
            "schema.tera",
            "benchmark.tera",
            "fuzz_target.tera",
            "derive_macro.tera",
            "migration.tera",
            "ffi_binding.tera",
            "protobuf_schema.tera",
            "openapi_spec.tera",
            "shell_completion.tera",
            "man_page.tera",
            "error_catalog.tera",
            "incremental_patch.tera",
            "skill_document.tera",
            "diary_entry.tera",
            "dockerfile.tera",
            "k8s_manifest.tera",
            "terraform_module.tera",
            "ci_workflow.tera",
            "changelog_entry.tera",
            "adr.tera",
            "asyncapi_spec.tera",
            "consumer_generator.tera",
            "task_scaffold.tera",
            "rust_foundational_crate_cargo_toml.tera",
            "rust_crate_lib_rs.tera",
            "systemd_user_service.tera",
        ]
    }
}
