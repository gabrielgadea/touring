//! `touring generate` — Artifact generator CLI commands.
//!
//! Provides code/artifact generation operations backed by `touring-generator`.
//!
//! ## Subcommands (27 total)
//!
//! ### Original (4)
//! - `list-kinds [--json]`        — List all GeneratorKind variants
//! - `render <kind> [--vars '{}'] [--json]` — Render a template for a kind
//! - `plan <kind> [--json]`        — Show plan info for a kind
//! - `verify --symbol <name> [--json]` — Verify symbol exists via VGP
//!
//! ### Plan lifecycle (14)
//! - `plan-submit --plan-file <path> [--json]`
//! - `plan-validate --plan-file <path> [--json]`
//! - `plan-verify --plan-file <path> [--json]`
//! - `plan-render --plan-file <path> [--json]`
//! - `plan-speculate --plan-file <path> [--json]`
//! - `plan-commit --plan-file <path> [--json]`   (alias for plan-submit)
//! - `plan-status --plan-file <path> [--json]`
//! - `plan-export --plan-file <path> --format json|toml [--json]`
//! - `plan-diff --plan-file <path> --other <path> [--json]`
//! - `plan-critique --plan-file <path> [--json]`
//! - `plan-suggest --intent "<text>" [--kind <kind>] [--json]`
//! - `plan-recall --query "<text>" [--limit N] [--json]`
//! - `plan-history --plan-file <path> [--json]`
//! - `plan-replay --plan-file <path> [--json]`
//! - `plan-rollback --plan-file <path> [--json]`
//!
//! ### Template commands (3)
//! - `template-list [--json]`
//! - `template-validate --template-file <path> [--json]`
//! - `template-test --template <name> [--vars '{}'] [--json]`
//!
//! ### Info commands (4)
//! - `kinds-list [--json]`
//! - `schema-dump [--json]`
//! - `capacity [--json]`
//! - `plan-rollback --plan-file <path> [--json]`
//! - `consumer-wiring [--limit N] [--json]`
//! - `autonomous --intent "<text>" [--json]`
//!
//! This handler uses **direct instantiation** (no daemon required) — all operations
//! are CPU-bound local work using the `touring-generator` crate directly.
//!
//! ## Wave P3-1.3 clap-derive migration (2026-06-11)
//!
//! Migrated from manual positional parsing (`arg_or`/`extract_flag`) to clap
//! derive. The `run()` public contract is unchanged — it still receives the full
//! argv slice; clap parses `args[1..]` so "generate" acts as the program name.

use std::collections::HashMap;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use touring_generator::{GeneratorKind, NoopTelemetry, TelemetrySink, TemplateEngine};

// ── CLI types ─────────────────────────────────────────────────────────────────

/// `touring generate` — artifact generation pipeline backed by touring-generator.
#[derive(Parser, Debug)]
#[command(
    name = "touring generate",
    bin_name = "touring generate",
    about = "Artifact generation pipeline (touring-generator typestate)",
    disable_help_subcommand = true
)]
struct GenerateCli {
    #[command(subcommand)]
    cmd: Option<GenerateCmd>,
}

#[derive(Subcommand, Debug)]
enum GenerateCmd {
    // ── Original subcommands ──────────────────────────────────────────────
    /// List all GeneratorKind variants.
    #[command(name = "list-kinds")]
    ListKinds {
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Alias for list-kinds.
    #[command(name = "kinds-list")]
    KindsList {
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Render a template for a GeneratorKind.
    Render {
        /// GeneratorKind name (case-insensitive, hyphens/underscores ignored).
        kind: String,
        /// Template variables as a JSON object string (e.g. `'{"name":"MyMod"}'`).
        #[arg(long)]
        vars: Option<String>,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Show plan info for a GeneratorKind.
    Plan {
        /// GeneratorKind name.
        kind: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Verify a symbol exists in the Touring index (VGP gate).
    Verify {
        /// Symbol name to look up.
        #[arg(long)]
        symbol: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    // ── Plan lifecycle ────────────────────────────────────────────────────
    /// Run the full typestate pipeline (verify→render→speculate→commit).
    #[command(name = "plan-submit")]
    PlanSubmit {
        /// Path to a GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Alias for plan-submit.
    #[command(name = "plan-commit")]
    PlanCommit {
        /// Path to a GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Validate a GeneratorPlan file (schema + structural checks).
    #[command(name = "plan-validate")]
    PlanValidate {
        /// Path to a GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Run VGP verification on a GeneratorPlan.
    #[command(name = "plan-verify")]
    PlanVerify {
        /// Path to a GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Render all artifacts described by a GeneratorPlan.
    #[command(name = "plan-render")]
    PlanRender {
        /// Path to a GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Run speculative validation on a GeneratorPlan.
    #[command(name = "plan-speculate")]
    PlanSpeculate {
        /// Path to a GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Show status of a GeneratorPlan (stage, CILA, trace length).
    #[command(name = "plan-status")]
    PlanStatus {
        /// Path to a GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Export a GeneratorPlan in json or toml format.
    #[command(name = "plan-export")]
    PlanExport {
        /// Path to a GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Output format: json or toml.
        #[arg(long, default_value = "json")]
        format: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Show field-level diff between two GeneratorPlan files.
    #[command(name = "plan-diff")]
    PlanDiff {
        /// First GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Second GeneratorPlan JSON file.
        #[arg(long)]
        other: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Critique a GeneratorPlan (errors + warnings).
    #[command(name = "plan-critique")]
    PlanCritique {
        /// Path to a GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Suggest a GeneratorPlan from a free-form intent string.
    #[command(name = "plan-suggest")]
    PlanSuggest {
        /// Free-form intent description.
        #[arg(long)]
        intent: String,
        /// Optional GeneratorKind hint.
        #[arg(long)]
        kind: Option<String>,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Recall similar GeneratorPlans from memory.
    #[command(name = "plan-recall")]
    PlanRecall {
        /// Search query string.
        #[arg(long)]
        query: String,
        /// Maximum number of results.
        #[arg(long, default_value = "10")]
        limit: i64,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Show the execution trace history of a GeneratorPlan.
    #[command(name = "plan-history")]
    PlanHistory {
        /// Path to a GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Replay (re-commit) a GeneratorPlan through the full pipeline.
    #[command(name = "plan-replay")]
    PlanReplay {
        /// Path to a GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Show rollback information for a GeneratorPlan.
    #[command(name = "plan-rollback")]
    PlanRollback {
        /// Path to a GeneratorPlan JSON file.
        #[arg(long)]
        plan_file: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    // ── Template commands ─────────────────────────────────────────────────
    /// List all built-in templates.
    #[command(name = "template-list")]
    TemplateList {
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Validate a template file.
    #[command(name = "template-validate")]
    TemplateValidate {
        /// Path to the template file to validate.
        #[arg(long)]
        template_file: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Test a template with optional variables.
    #[command(name = "template-test")]
    TemplateTest {
        /// Template name.
        #[arg(long)]
        template: String,
        /// Template variables as a JSON object string (e.g. `'{"key":"val"}'`).
        #[arg(long)]
        vars: Option<String>,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    // ── Info commands ─────────────────────────────────────────────────────
    /// Dump the GeneratorPlan JSON schema.
    #[command(name = "schema-dump")]
    SchemaDump {
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Show generator capacity metrics.
    Capacity {
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Query orphan pub symbols and build ConsumerGenerator plans.
    #[command(name = "consumer-wiring")]
    ConsumerWiring {
        /// Maximum number of plans to generate.
        #[arg(long, default_value = "10")]
        limit: usize,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Run the full LLM-as-Planner → VGP pipeline in memory (no temp file).
    Autonomous {
        /// Free-form generation intent.
        #[arg(long)]
        intent: String,
        /// Emit machine-readable JSON.
        #[arg(short = 'j', long)]
        json: bool,
    },
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run an async future blocking, safely whether or not we are already inside a tokio runtime.
///
/// `main.rs` builds a multi-thread runtime explicitly, so CLI handlers reached via dispatch are
/// invoked from **inside** that runtime. A fresh `Runtime::new()` + `block_on` then panics with
/// "Cannot start a runtime from within a runtime". This helper reuses the current handle via
/// `block_in_place` (multi-thread runtime requirement is satisfied by `main.rs`), and falls back
/// to a fresh runtime only when no current runtime exists (unit-test contexts).
fn block_on_async<F: std::future::Future>(future: F) -> F::Output {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(future)),
        _ => tokio::runtime::Runtime::new()
            .expect("create tokio runtime for non-runtime context")
            .block_on(future),
    }
}

/// CLI entry point for `touring generate` subcommand.
///
/// Receives the full argv slice (`args[0]` = binary name, `args[1]` = "generate").
/// Clap parses `args[1..]` so "generate" acts as the program name and `args[2]`
/// becomes the subcommand. When no subcommand is given, defaults to `list-kinds`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match GenerateCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(GenerateCmd::ListKinds { json: false }) {
        // ── Original subcommands ──────────────────────────────────────────
        GenerateCmd::ListKinds { json } | GenerateCmd::KindsList { json } => run_list_kinds(json),
        GenerateCmd::Render { kind, vars, json } => run_render(&kind, vars.as_deref(), json),
        GenerateCmd::Plan { kind, json } => run_plan(&kind, json),
        GenerateCmd::Verify { symbol, json } => run_verify(&symbol, json),
        // ── Plan lifecycle ────────────────────────────────────────────────
        GenerateCmd::PlanSubmit { plan_file, json }
        | GenerateCmd::PlanCommit { plan_file, json } => run_plan_submit(&plan_file, json),
        GenerateCmd::PlanValidate { plan_file, json } => run_plan_validate(&plan_file, json),
        GenerateCmd::PlanVerify { plan_file, json } => run_plan_verify(&plan_file, json),
        GenerateCmd::PlanRender { plan_file, json } => run_plan_render(&plan_file, json),
        GenerateCmd::PlanSpeculate { plan_file, json } => run_plan_speculate(&plan_file, json),
        GenerateCmd::PlanStatus { plan_file, json } => run_plan_status(&plan_file, json),
        GenerateCmd::PlanExport {
            plan_file,
            format,
            json,
        } => run_plan_export(&plan_file, &format, json),
        GenerateCmd::PlanDiff {
            plan_file,
            other,
            json,
        } => run_plan_diff(&plan_file, &other, json),
        GenerateCmd::PlanCritique { plan_file, json } => run_plan_critique(&plan_file, json),
        GenerateCmd::PlanSuggest { intent, kind, json } => {
            run_plan_suggest(&intent, kind.as_deref(), json)
        }
        GenerateCmd::PlanRecall { query, limit, json } => run_plan_recall(&query, limit, json),
        GenerateCmd::PlanHistory { plan_file, json } => run_plan_history(&plan_file, json),
        GenerateCmd::PlanReplay { plan_file, json } => run_plan_replay(&plan_file, json),
        GenerateCmd::PlanRollback { plan_file, json } => run_plan_rollback(&plan_file, json),
        // ── Template commands ─────────────────────────────────────────────
        GenerateCmd::TemplateList { json } => run_template_list(json),
        GenerateCmd::TemplateValidate {
            template_file,
            json,
        } => run_template_validate(&template_file, json),
        GenerateCmd::TemplateTest {
            template,
            vars,
            json,
        } => run_template_test(&template, vars.as_deref(), json),
        // ── Info commands ─────────────────────────────────────────────────
        GenerateCmd::SchemaDump { json } => run_schema_dump(json),
        GenerateCmd::Capacity { json } => run_capacity(json),
        GenerateCmd::ConsumerWiring { limit, json } => run_consumer_wiring(limit, json),
        GenerateCmd::Autonomous { intent, json } => run_autonomous(&intent, json),
    }
}

// ── list-kinds / kinds-list ───────────────────────────────────────────────────

/// `touring generate list-kinds [--json]`
fn run_list_kinds(is_json: bool) -> anyhow::Result<()> {
    let kinds = all_kinds();
    if is_json {
        let entries: Vec<serde_json::Value> = kinds
            .iter()
            .map(|k| serde_json::json!({"kind": format!("{k:?}"), "label": k.label(), "template": k.template_name()}))
            .collect();
        println!(
            "{}",
            serde_json::json!({"count": entries.len(), "kinds": entries})
        );
    } else {
        println!("{} GeneratorKind variants:", kinds.len());
        for k in &kinds {
            println!("  {:30}  template: {}", k.label(), k.template_name());
        }
    }
    Ok(())
}

// ── render ────────────────────────────────────────────────────────────────────

/// `touring generate render <kind> [--vars '{"key":"value"}'] [--json]`
fn run_render(kind_str: &str, vars_json: Option<&str>, is_json: bool) -> anyhow::Result<()> {
    let kind = parse_kind(kind_str)?;
    let vars = parse_vars(vars_json)?;
    let metrics: Arc<dyn TelemetrySink> = Arc::new(NoopTelemetry);
    let engine = TemplateEngine::new(Arc::clone(&metrics));
    match engine.render_for_kind(&kind, &vars) {
        Ok(output) => {
            if is_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "kind": format!("{kind:?}"), "template": kind.template_name(),
                        "output": output, "vars_used": vars.keys().collect::<Vec<_>>(),
                    })
                );
            } else {
                print!("{output}");
            }
        }
        Err(e) => {
            if is_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "kind": format!("{kind:?}"), "error": e.to_string(),
                        "hint": "Provide required template variables with --vars '{\"key\":\"value\"}'",
                    })
                );
            } else {
                eprintln!("Render error for {}: {}", kind.label(), e);
                eprintln!(
                    "Hint: use --vars '{{\"name\":\"MyModule\"}}' to supply template variables."
                );
            }
        }
    }
    Ok(())
}

// ── plan ──────────────────────────────────────────────────────────────────────

/// `touring generate plan <kind> [--json]`
fn run_plan(kind_str: &str, is_json: bool) -> anyhow::Result<()> {
    let kind = parse_kind(kind_str)?;
    if is_json {
        println!(
            "{}",
            serde_json::json!({
                "kind": format!("{kind:?}"), "label": kind.label(), "template": kind.template_name(),
                "note": "Use 'touring generate render <kind> --vars <json>' to render with variables.",
            })
        );
    } else {
        println!("Kind:     {:?}", kind);
        println!("Label:    {}", kind.label());
        println!("Template: {}", kind.template_name());
        println!();
        println!(
            "Use 'touring generate render {:?} --vars {{\"key\":\"value\"}}' to render.",
            kind
        );
    }
    Ok(())
}

// ── verify ────────────────────────────────────────────────────────────────────

/// Result of a symbol lookup via touring index.
struct SymbolLookup {
    found: bool,
    file_path: Option<String>,
    line: Option<u64>,
}

/// Query `touring index find <symbol> -j` and parse the result.
fn lookup_symbol(symbol: &str) -> SymbolLookup {
    let out = std::process::Command::new("touring")
        .args(["index", "find", symbol, "-j"])
        .output();
    let Ok(out) = out else {
        return SymbolLookup {
            found: false,
            file_path: None,
            line: None,
        };
    };
    if !out.status.success() {
        return SymbolLookup {
            found: false,
            file_path: None,
            line: None,
        };
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return SymbolLookup {
            found: false,
            file_path: None,
            line: None,
        };
    };
    let count = v.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
    let defs = v.get("definitions").and_then(|d| d.get(0));
    let file_path = defs
        .and_then(|d| d.get("file_path"))
        .and_then(|p| p.as_str())
        .map(|s| s.to_owned());
    let line = defs.and_then(|d| d.get("line")).and_then(|l| l.as_u64());
    SymbolLookup {
        found: count > 0,
        file_path,
        line,
    }
}

/// Format a human-readable location string.
fn format_location(file_path: Option<&str>, line: Option<u64>) -> String {
    match (file_path, line) {
        (Some(fp), Some(ln)) => format!("{fp}:{ln}"),
        (Some(fp), None) => fp.to_owned(),
        _ => "(unknown location)".to_owned(),
    }
}

/// `touring generate verify --symbol <name> [--json]`
fn run_verify(symbol: &str, is_json: bool) -> anyhow::Result<()> {
    let result = lookup_symbol(symbol);
    if is_json {
        println!(
            "{}",
            serde_json::json!({
                "symbol": symbol, "found": result.found,
                "file_path": result.file_path, "line": result.line,
            })
        );
    } else if result.found {
        let loc = format_location(result.file_path.as_deref(), result.line);
        println!("FOUND: {symbol} at {loc}");
    } else {
        println!("NOT FOUND: {symbol}");
    }
    Ok(())
}

// ── Plan file helpers ─────────────────────────────────────────────────────────

/// Read a plan file and deserialize it as `GeneratorPlan`.
fn read_plan_file(path: &str) -> anyhow::Result<(touring_generator::GeneratorPlan, String)> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read '{}': {e}", path))?;
    let plan: touring_generator::GeneratorPlan = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("invalid plan JSON in '{}': {e}", path))?;
    Ok((plan, content))
}

/// Print a serde_json::Value as pretty JSON or fall back to human text.
fn print_result(value: &serde_json::Value, is_json: bool, human_fn: impl FnOnce()) {
    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        );
    } else {
        human_fn();
    }
}

// ── plan-submit / plan-commit ─────────────────────────────────────────────────

/// `touring generate plan-submit --plan-file <path> [--json]`
fn run_plan_submit(path: &str, is_json: bool) -> anyhow::Result<()> {
    let (plan, _) = read_plan_file(path)?;
    let plan_json = serde_json::to_string(&plan)?;
    let result = block_on_async(crate::tools::generator_tools::submit_plan(
        &plan_json, false,
    ));
    print_result(&result, is_json, || {
        if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            println!(
                "Committed plan: {}",
                result
                    .get("plan_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
            );
        } else {
            eprintln!(
                "Pipeline failed at stage '{}': {}",
                result.get("stage").and_then(|v| v.as_str()).unwrap_or("?"),
                result.get("error").and_then(|v| v.as_str()).unwrap_or("?")
            );
        }
    });
    Ok(())
}

// ── plan-validate ─────────────────────────────────────────────────────────────

/// `touring generate plan-validate --plan-file <path> [--json]`
fn run_plan_validate(path: &str, is_json: bool) -> anyhow::Result<()> {
    let (_, content) = read_plan_file(path)?;
    let result = crate::tools::generator_tools::validate_plan(&content);
    print_result(&result, is_json, || {
        if result
            .get("valid")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            println!("Plan is valid.");
        } else {
            eprintln!("Validation errors:");
            if let Some(errors) = result.get("errors").and_then(|v| v.as_array()) {
                for e in errors {
                    eprintln!("  - {}", e.as_str().unwrap_or("unknown error"));
                }
            }
        }
    });
    Ok(())
}

// ── plan-verify ───────────────────────────────────────────────────────────────

/// `touring generate plan-verify --plan-file <path> [--json]`
fn run_plan_verify(path: &str, is_json: bool) -> anyhow::Result<()> {
    let (plan, _) = read_plan_file(path)?;
    let plan_json = serde_json::to_string(&plan)?;
    let result = block_on_async(crate::tools::generator_tools::verify_plan(&plan_json));
    print_result(&result, is_json, || {
        if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            println!("VGP verification passed.");
        } else {
            eprintln!(
                "VGP verification failed: {}",
                result.get("error").and_then(|v| v.as_str()).unwrap_or("?")
            );
        }
    });
    Ok(())
}

// ── plan-render ───────────────────────────────────────────────────────────────

/// `touring generate plan-render --plan-file <path> [--json]`
fn run_plan_render(path: &str, is_json: bool) -> anyhow::Result<()> {
    let (plan, _) = read_plan_file(path)?;
    let plan_json = serde_json::to_string(&plan)?;
    let result = block_on_async(crate::tools::generator_tools::render_plan(&plan_json));
    print_result(&result, is_json, || {
        if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            println!("Render succeeded (stage: rendered).");
        } else {
            eprintln!(
                "Render failed at '{}': {}",
                result.get("stage").and_then(|v| v.as_str()).unwrap_or("?"),
                result.get("error").and_then(|v| v.as_str()).unwrap_or("?")
            );
        }
    });
    Ok(())
}

// ── plan-speculate ────────────────────────────────────────────────────────────

/// `touring generate plan-speculate --plan-file <path> [--json]`
fn run_plan_speculate(path: &str, is_json: bool) -> anyhow::Result<()> {
    let (plan, _) = read_plan_file(path)?;
    let plan_json = serde_json::to_string(&plan)?;
    let result = block_on_async(crate::tools::generator_tools::speculate_plan(&plan_json));
    print_result(&result, is_json, || {
        if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            println!("Speculation passed (stage: speculated).");
        } else {
            eprintln!(
                "Speculation failed at '{}': {}",
                result.get("stage").and_then(|v| v.as_str()).unwrap_or("?"),
                result.get("error").and_then(|v| v.as_str()).unwrap_or("?")
            );
        }
    });
    Ok(())
}

// ── plan-status ───────────────────────────────────────────────────────────────

/// `touring generate plan-status --plan-file <path> [--json]`
fn run_plan_status(path: &str, is_json: bool) -> anyhow::Result<()> {
    let (_, content) = read_plan_file(path)?;
    let result = crate::tools::generator_tools::plan_status(&content);
    print_result(&result, is_json, || {
        println!(
            "Plan ID:    {}",
            result
                .get("plan_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        );
        println!(
            "Kind:       {}",
            result.get("kind").and_then(|v| v.as_str()).unwrap_or("?")
        );
        println!(
            "Intent:     {}",
            result.get("intent").and_then(|v| v.as_str()).unwrap_or("?")
        );
        println!(
            "Target:     {}",
            result.get("target").and_then(|v| v.as_str()).unwrap_or("?")
        );
        println!(
            "CILA:       {}",
            result
                .get("cila_level")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        );
        println!(
            "Trace:      {} entries",
            result
                .get("trace_entries")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        );
        println!(
            "Valid:      {}",
            result
                .get("validation")
                .and_then(|v| v.get("valid"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        );
    });
    Ok(())
}

// ── plan-export ───────────────────────────────────────────────────────────────

/// `touring generate plan-export --plan-file <path> --format json|toml [--json]`
fn run_plan_export(path: &str, format: &str, is_json: bool) -> anyhow::Result<()> {
    let (plan, _) = read_plan_file(path)?;
    let output = match format {
        "json" | "JSON" => serde_json::to_string_pretty(&plan)?,
        "toml" | "TOML" => toml_from_plan(&plan),
        other => anyhow::bail!("Unsupported format '{}'. Use: json, toml", other),
    };
    if is_json {
        println!(
            "{}",
            serde_json::json!({"ok": true, "format": format, "content": output})
        );
    } else {
        println!("{output}");
    }
    Ok(())
}

/// Serialize a plan to a TOML-like representation (via JSON round-trip).
fn toml_from_plan(plan: &touring_generator::GeneratorPlan) -> String {
    // TOML serialization requires the `toml` crate which is not in scope here.
    // We produce a JSON-in-TOML comment wrapper as a safe fallback.
    let json = serde_json::to_string_pretty(plan).unwrap_or_default();
    format!(
        "# GeneratorPlan (TOML format not available — JSON follows)\n# {}",
        json.replace('\n', "\n# ")
    )
}

// ── plan-diff ─────────────────────────────────────────────────────────────────

/// `touring generate plan-diff --plan-file <path> --other <path> [--json]`
fn run_plan_diff(path_a: &str, path_b: &str, is_json: bool) -> anyhow::Result<()> {
    let content_a = std::fs::read_to_string(path_a)
        .map_err(|e| anyhow::anyhow!("cannot read '{}': {e}", path_a))?;
    let content_b = std::fs::read_to_string(path_b)
        .map_err(|e| anyhow::anyhow!("cannot read '{}': {e}", path_b))?;
    let result = crate::tools::generator_tools_introspect::diff_plans(&content_a, &content_b);
    print_result(&result, is_json, || {
        if result
            .get("identical")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            println!("Plans are identical.");
        } else {
            println!(
                "{} difference(s):",
                result
                    .get("diff_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
            );
            if let Some(diffs) = result.get("diffs").and_then(|v| v.as_array()) {
                for d in diffs {
                    println!(
                        "  {}: {:?} → {:?}",
                        d.get("field").unwrap_or(&serde_json::Value::Null),
                        d.get("a").unwrap_or(&serde_json::Value::Null),
                        d.get("b").unwrap_or(&serde_json::Value::Null)
                    );
                }
            }
        }
    });
    Ok(())
}

// ── plan-critique ─────────────────────────────────────────────────────────────

/// `touring generate plan-critique --plan-file <path> [--json]`
fn run_plan_critique(path: &str, is_json: bool) -> anyhow::Result<()> {
    let (_, content) = read_plan_file(path)?;
    let result = crate::tools::generator_tools_introspect::critique_plan(&content);
    print_result(&result, is_json, || {
        let errors = result
            .get("error_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let warnings = result
            .get("warning_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        println!("Critique: {} error(s), {} warning(s)", errors, warnings);
        if let Some(issues) = result.get("issues").and_then(|v| v.as_array()) {
            for issue in issues {
                println!(
                    "  [{:8}] {}: {}",
                    issue
                        .get("severity")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?"),
                    issue.get("field").and_then(|v| v.as_str()).unwrap_or("?"),
                    issue.get("message").and_then(|v| v.as_str()).unwrap_or("?"),
                );
            }
        }
    });
    Ok(())
}

// ── plan-suggest ──────────────────────────────────────────────────────────────

/// `touring generate plan-suggest --intent "<text>" [--kind <kind>] [--json]`
fn run_plan_suggest(intent: &str, kind: Option<&str>, is_json: bool) -> anyhow::Result<()> {
    let result = crate::tools::generator_tools::suggest_plan(intent, kind);
    print_result(&result, is_json, || {
        if let Some(suggestion) = result.get("suggestion") {
            println!(
                "{}",
                serde_json::to_string_pretty(suggestion).unwrap_or_default()
            );
        }
        println!(
            "\n{}",
            result.get("note").and_then(|v| v.as_str()).unwrap_or("")
        );
    });
    Ok(())
}

// ── plan-recall ───────────────────────────────────────────────────────────────

/// `touring generate plan-recall --query "<text>" [--limit N] [--json]`
fn run_plan_recall(query: &str, limit: i64, is_json: bool) -> anyhow::Result<()> {
    let result = crate::tools::generator_tools::recall_similar(query, limit);
    print_result(&result, is_json, || {
        if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            println!("Memory recall for '{}' (limit {}):", query, limit);
            if let Some(results) = result
                .get("results")
                .and_then(|v| v.get("matches"))
                .and_then(|v| v.as_array())
            {
                for m in results {
                    println!(
                        "  [{}] {}",
                        m.get("score").unwrap_or(&serde_json::Value::Null),
                        m.get("key").and_then(|v| v.as_str()).unwrap_or("?")
                    );
                }
            }
        } else {
            eprintln!(
                "Recall failed: {}",
                result.get("error").and_then(|v| v.as_str()).unwrap_or("?")
            );
        }
    });
    Ok(())
}

// ── plan-history ──────────────────────────────────────────────────────────────

/// `touring generate plan-history --plan-file <path> [--json]`
fn run_plan_history(path: &str, is_json: bool) -> anyhow::Result<()> {
    let (_, content) = read_plan_file(path)?;
    let result = crate::tools::generator_tools_introspect::plan_history(&content);
    print_result(&result, is_json, || {
        let count = result
            .get("trace_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        println!("Execution trace: {} entries", count);
        if let Some(trace) = result.get("trace").and_then(|v| v.as_array()) {
            for (i, entry) in trace.iter().enumerate() {
                println!(
                    "  [{}] {}",
                    i,
                    serde_json::to_string(entry).unwrap_or_default()
                );
            }
        }
    });
    Ok(())
}

// ── plan-replay ───────────────────────────────────────────────────────────────

/// `touring generate plan-replay --plan-file <path> [--json]`
fn run_plan_replay(path: &str, is_json: bool) -> anyhow::Result<()> {
    let (plan, _) = read_plan_file(path)?;
    let plan_json = serde_json::to_string(&plan)?;
    let result = block_on_async(crate::tools::generator_tools::replay_plan(&plan_json));
    print_result(&result, is_json, || {
        if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            println!(
                "Replay committed (iteration {}): {}",
                result
                    .get("iteration")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                result
                    .get("plan_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?"),
            );
        } else {
            eprintln!(
                "Replay failed at '{}': {}",
                result.get("stage").and_then(|v| v.as_str()).unwrap_or("?"),
                result.get("error").and_then(|v| v.as_str()).unwrap_or("?")
            );
        }
    });
    Ok(())
}

// ── plan-rollback ─────────────────────────────────────────────────────────────

/// `touring generate plan-rollback --plan-file <path> [--json]`
fn run_plan_rollback(path: &str, is_json: bool) -> anyhow::Result<()> {
    let (_, content) = read_plan_file(path)?;
    let result = crate::tools::generator_tools::rollback_plan(&content);
    print_result(&result, is_json, || {
        if result
            .get("rollback_available")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            println!(
                "Rollback available: {} → {}",
                result.get("backup").and_then(|v| v.as_str()).unwrap_or("?"),
                result.get("target").and_then(|v| v.as_str()).unwrap_or("?"),
            );
            println!(
                "{}",
                result.get("note").and_then(|v| v.as_str()).unwrap_or("")
            );
        } else {
            eprintln!(
                "No rollback available: {}",
                result
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
            );
        }
    });
    Ok(())
}

// ── template-list ─────────────────────────────────────────────────────────────

/// `touring generate template-list [--json]`
fn run_template_list(is_json: bool) -> anyhow::Result<()> {
    let result = crate::tools::generator_tools::template_list();
    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).unwrap_or_default()
        );
    } else {
        println!(
            "{} built-in templates:",
            result.get("count").and_then(|v| v.as_u64()).unwrap_or(0)
        );
        if let Some(templates) = result.get("templates").and_then(|v| v.as_array()) {
            for t in templates {
                println!(
                    "  {:30}  template: {}",
                    t.get("label").and_then(|v| v.as_str()).unwrap_or("?"),
                    t.get("template").and_then(|v| v.as_str()).unwrap_or("?")
                );
            }
        }
    }
    Ok(())
}

// ── template-validate ─────────────────────────────────────────────────────────

/// `touring generate template-validate --template-file <path> [--json]`
fn run_template_validate(template_file: &str, is_json: bool) -> anyhow::Result<()> {
    let result = crate::tools::generator_tools::template_validate(template_file);
    print_result(&result, is_json, || {
        if result
            .get("valid")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            println!("Template is valid: {}", template_file);
        } else {
            eprintln!(
                "Template invalid: {}",
                result
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error")
            );
        }
    });
    Ok(())
}

// ── template-test ─────────────────────────────────────────────────────────────

/// `touring generate template-test --template <name> [--vars '{}'] [--json]`
fn run_template_test(
    template_name: &str,
    vars_json: Option<&str>,
    is_json: bool,
) -> anyhow::Result<()> {
    let result = crate::tools::generator_tools::template_test(template_name, vars_json);
    print_result(&result, is_json, || {
        if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            println!(
                "{}",
                result.get("output").and_then(|v| v.as_str()).unwrap_or("")
            );
        } else {
            eprintln!(
                "Template test failed: {}",
                result
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
            );
            if let Some(hint) = result.get("hint").and_then(|v| v.as_str()) {
                eprintln!("Hint: {hint}");
            }
        }
    });
    Ok(())
}

// ── schema-dump ───────────────────────────────────────────────────────────────

/// `touring generate schema-dump [--json]`
fn run_schema_dump(is_json: bool) -> anyhow::Result<()> {
    let result = crate::tools::generator_tools::schema_dump();
    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).unwrap_or_default()
        );
    } else if let Some(schema) = result.get("schema") {
        println!(
            "{}",
            serde_json::to_string_pretty(schema).unwrap_or_default()
        );
    } else {
        eprintln!(
            "schema-dump failed: {}",
            result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
        );
    }
    Ok(())
}

// ── capacity ──────────────────────────────────────────────────────────────────

/// `touring generate capacity [--json]`
fn run_capacity(is_json: bool) -> anyhow::Result<()> {
    let result = crate::tools::generator_tools::capacity();
    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).unwrap_or_default()
        );
    } else if let Some(cap) = result.get("capacity") {
        println!("{}", serde_json::to_string_pretty(cap).unwrap_or_default());
    } else {
        eprintln!(
            "capacity failed: {}",
            result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
        );
    }
    Ok(())
}

// ── consumer-wiring ───────────────────────────────────────────────────────────

/// `touring generate consumer-wiring [--limit N] [--json]`
///
/// Queries orphan pub symbols from the touring wiring system and builds
/// `ConsumerGenerator` plans for each wiring opportunity. Plans are returned
/// as JSON — submit individually via `touring generate plan-submit`.
fn run_consumer_wiring(limit: usize, is_json: bool) -> anyhow::Result<()> {
    let result = crate::tools::generator_tools::build_consumer_generator_plans(limit);
    print_result(&result, is_json, || {
        let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = result
            .get("total_orphans")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        println!("Consumer-wiring plans: {count} generated (from {total} total orphans)");
        if let Some(plans) = result.get("plans").and_then(|v| v.as_array()) {
            for (i, plan) in plans.iter().enumerate() {
                let intent = plan.get("intent").and_then(|v| v.as_str()).unwrap_or("");
                println!("  [{:2}] {intent}", i + 1);
            }
        }
        if let Some(note) = result.get("note").and_then(|v| v.as_str()) {
            println!("Note: {note}");
        }
    });
    Ok(())
}

// ── autonomous ────────────────────────────────────────────────────────────────

/// `touring generate autonomous --intent "<text>" [--json]`
///
/// Facade command that runs the full LLM-as-Planner → VGP pipeline in memory:
/// suggest_plan → auto-populate contracts → submit_plan (verify+render+speculate+commit).
/// No temp file is written — the typestate transitions happen entirely in memory.
///
/// On success: injects RL reward +1.0 and returns committed file list.
/// On failure: injects RL reward -0.3 and returns error details.
fn run_autonomous(intent: &str, is_json: bool) -> anyhow::Result<()> {
    // Step 1: suggest plan in memory (no disk I/O)
    let suggestion = crate::tools::generator_tools::suggest_plan(intent, None);

    // Extract skeleton plan; merge suggested_contracts if present
    let mut plan_value = suggestion
        .get("suggestion")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("suggest_plan returned no suggestion"))?;

    if let (Some(contracts), Some(plan_obj)) = (
        suggestion.get("suggested_contracts"),
        plan_value.as_object_mut(),
    ) {
        if contracts.is_object() {
            plan_obj.insert("contracts".to_string(), contracts.clone());
        }
    }

    let plan_json = serde_json::to_string(&plan_value)?;

    // Step 2: submit plan through full typestate pipeline (verify→render→speculate→commit)
    let result = block_on_async(crate::tools::generator_tools::submit_plan(
        &plan_json, false,
    ));

    // Step 3: inject RL reward via background subprocess (fire-and-forget, non-blocking)
    let succeeded = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    {
        let reward_str = if succeeded { "1.0" } else { "-0.3" };
        let ctx = if succeeded {
            intent.to_string()
        } else {
            "commit_failed".to_string()
        };
        let reward_s = reward_str.to_string();
        std::thread::spawn(move || {
            let _ = std::process::Command::new("touring")
                .args(["learning", "reward", "generate-autonomous", &reward_s, &ctx])
                .output();
        });
    }

    if !succeeded {
        let msg = result
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("pipeline failed");
        anyhow::bail!("autonomous generation failed: {msg}");
    }

    print_result(&result, is_json, || {
        if let Some(files) = result.get("committed_files").and_then(|v| v.as_array()) {
            println!("Generated {} file(s):", files.len());
            for f in files {
                println!("  {}", f.as_str().unwrap_or("?"));
            }
        }
        println!("Intent: {intent}");
    });
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// All GeneratorKind variants in declaration order.
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
        // ── Missing pre-2026-05-02 (added Wave TRM + drift fix) ─────────
        GeneratorKind::ConsumerGenerator,
        GeneratorKind::TaskScaffold,
        GeneratorKind::RustFoundationalCrateCargoToml,
        GeneratorKind::RustCrateLibRs,
        GeneratorKind::SystemdUserService,
    ]
}

/// Parse a string into a `GeneratorKind`, case-insensitive.
fn parse_kind(s: &str) -> anyhow::Result<GeneratorKind> {
    let normalized = s.to_lowercase().replace(['-', '_', ' '], "");
    let candidates: &[(&str, GeneratorKind)] = &[
        ("rustmodule", GeneratorKind::RustModule),
        ("clihandler", GeneratorKind::CliHandler),
        ("mcptool", GeneratorKind::McpTool),
        ("hookhandler", GeneratorKind::HookHandler),
        ("test", GeneratorKind::Test),
        ("benchmarksuite", GeneratorKind::BenchmarkSuite),
        ("fuzztarget", GeneratorKind::FuzzTarget),
        ("derivemacro", GeneratorKind::DeriveMacro),
        ("attributemacro", GeneratorKind::AttributeMacro),
        ("functionmacro", GeneratorKind::FunctionMacro),
        ("errorcatalog", GeneratorKind::ErrorCatalog),
        ("incrementalpatch", GeneratorKind::IncrementalPatch),
        ("ffibinding", GeneratorKind::FfiBinding),
        ("schema", GeneratorKind::Schema),
        ("migrationscript", GeneratorKind::MigrationScript),
        ("protobufschema", GeneratorKind::ProtoBufSchema),
        ("openapispec", GeneratorKind::OpenApiSpec),
        ("asyncapispec", GeneratorKind::AsyncApiSpec),
        ("planmarkdown", GeneratorKind::PlanMarkdown),
        ("skilldocument", GeneratorKind::SkillDocument),
        ("diaryentry", GeneratorKind::DiaryEntry),
        ("changelogentry", GeneratorKind::ChangelogEntry),
        ("adr", GeneratorKind::Adr),
        ("shellcompletion", GeneratorKind::ShellCompletion),
        ("manpage", GeneratorKind::ManPage),
        ("pythonscript", GeneratorKind::PythonScript),
        // Aliases for the intuitive Python names. `PythonModule` never existed
        // as a kind; the real Python generator is `PythonScript`. Mapping the
        // intuitive names here turns a hard "Unknown GeneratorKind" failure
        // (touring-native tooling STAGE 6 → exit 4) into a successful render. (A11, 2026-06-27)
        ("pythonmodule", GeneratorKind::PythonScript),
        ("python", GeneratorKind::PythonScript),
        ("typescriptmodule", GeneratorKind::TypeScriptModule),
        ("typescript", GeneratorKind::TypeScriptModule),
        ("dockerimage", GeneratorKind::DockerImage),
        ("kubernetesmanifest", GeneratorKind::KubernetesManifest),
        ("terraformmodule", GeneratorKind::TerraformModule),
        ("ciworkflow", GeneratorKind::CiWorkflow),
        // ── Missing pre-2026-05-02 (drift fix) ───────────────────────────
        ("consumergenerator", GeneratorKind::ConsumerGenerator),
        ("taskscaffold", GeneratorKind::TaskScaffold),
        // ── Wave TRM 2026-05-02 — crate scaffolding kinds ────────────────
        (
            "rustfoundationalcratecargotoml",
            GeneratorKind::RustFoundationalCrateCargoToml,
        ),
        ("rustcratelibrs", GeneratorKind::RustCrateLibRs),
        ("systemduserservice", GeneratorKind::SystemdUserService),
    ];
    for (key, kind) in candidates {
        if normalized == *key {
            return Ok(kind.clone());
        }
    }
    anyhow::bail!(
        "Unknown GeneratorKind: '{}'. Run 'touring generate list-kinds' to see valid values.",
        s
    )
}

/// Parse an optional `--vars '<json>'` string into a variable map.
fn parse_vars(vars_json: Option<&str>) -> anyhow::Result<HashMap<String, serde_json::Value>> {
    match vars_json {
        None => Ok(HashMap::new()),
        Some(raw) => {
            let v: serde_json::Value = serde_json::from_str(raw)
                .map_err(|e| anyhow::anyhow!("--vars is not valid JSON: {e}."))?;
            match v {
                serde_json::Value::Object(map) => Ok(map.into_iter().collect()),
                _ => anyhow::bail!("--vars must be a JSON object"),
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    GenerateCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> GenerateCli {
        GenerateCli::try_parse_from(args).expect("args should parse")
    }

    fn parse_err(args: &[&str]) -> clap::Error {
        GenerateCli::try_parse_from(args).expect_err("expected parse error")
    }

    // ── Default subcommand ────────────────────────────────────────────────

    #[test]
    fn bare_generate_defaults_to_list_kinds() {
        let cli = parse(&["generate"]);
        assert!(cli.cmd.is_none()); // run() maps None → ListKinds
    }

    // ── list-kinds / kinds-list ───────────────────────────────────────────

    #[test]
    fn list_kinds_parses_json_flag() {
        let cli = parse(&["generate", "list-kinds", "--json"]);
        assert!(matches!(
            cli.cmd,
            Some(GenerateCmd::ListKinds { json: true })
        ));
    }

    #[test]
    fn list_kinds_short_j_flag() {
        let cli = parse(&["generate", "list-kinds", "-j"]);
        assert!(matches!(
            cli.cmd,
            Some(GenerateCmd::ListKinds { json: true })
        ));
    }

    #[test]
    fn kinds_list_alias_parses() {
        let cli = parse(&["generate", "kinds-list"]);
        assert!(matches!(
            cli.cmd,
            Some(GenerateCmd::KindsList { json: false })
        ));
    }

    // ── render ────────────────────────────────────────────────────────────

    #[test]
    fn render_parses_kind_positional() {
        let cli = parse(&["generate", "render", "RustModule"]);
        let Some(GenerateCmd::Render { kind, vars, json }) = cli.cmd else {
            panic!("expected Render");
        };
        assert_eq!(kind, "RustModule");
        assert!(vars.is_none());
        assert!(!json);
    }

    #[test]
    fn render_parses_vars_with_json_object() {
        let cli = parse(&[
            "generate",
            "render",
            "RustModule",
            "--vars",
            r#"{"name":"MyMod"}"#,
        ]);
        let Some(GenerateCmd::Render { vars, .. }) = cli.cmd else {
            panic!("expected Render");
        };
        assert_eq!(vars.as_deref(), Some(r#"{"name":"MyMod"}"#));
    }

    #[test]
    fn render_parses_vars_with_braces_and_quotes() {
        // Explicit test for G6/gotcha (e): JSON inline in --vars
        let raw = r#"{"k":"v","nested":{"x":1}}"#;
        let cli = parse(&["generate", "render", "Test", "--vars", raw]);
        let Some(GenerateCmd::Render { vars, .. }) = cli.cmd else {
            panic!("expected Render");
        };
        assert_eq!(vars.as_deref(), Some(raw));
    }

    #[test]
    fn render_with_json_flag() {
        let cli = parse(&["generate", "render", "Test", "--json"]);
        let Some(GenerateCmd::Render { json, .. }) = cli.cmd else {
            panic!("expected Render");
        };
        assert!(json);
    }

    #[test]
    fn render_missing_kind_is_parse_error() {
        let err = parse_err(&["generate", "render"]);
        // clap should require the positional <kind>
        assert!(
            err.to_string().contains("kind") || err.kind() != clap::error::ErrorKind::DisplayHelp
        );
    }

    // ── plan ─────────────────────────────────────────────────────────────

    #[test]
    fn plan_parses_kind() {
        let cli = parse(&["generate", "plan", "McpTool"]);
        let Some(GenerateCmd::Plan { kind, json }) = cli.cmd else {
            panic!("expected Plan");
        };
        assert_eq!(kind, "McpTool");
        assert!(!json);
    }

    // ── verify ────────────────────────────────────────────────────────────

    #[test]
    fn verify_parses_symbol_flag() {
        let cli = parse(&["generate", "verify", "--symbol", "MyStruct"]);
        let Some(GenerateCmd::Verify { symbol, json }) = cli.cmd else {
            panic!("expected Verify");
        };
        assert_eq!(symbol, "MyStruct");
        assert!(!json);
    }

    #[test]
    fn verify_missing_symbol_is_parse_error() {
        let err = parse_err(&["generate", "verify"]);
        assert_ne!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    // ── plan-submit / plan-commit ─────────────────────────────────────────

    #[test]
    fn plan_submit_parses_plan_file() {
        let cli = parse(&["generate", "plan-submit", "--plan-file", "/tmp/p.json"]);
        let Some(GenerateCmd::PlanSubmit { plan_file, json }) = cli.cmd else {
            panic!("expected PlanSubmit");
        };
        assert_eq!(plan_file, "/tmp/p.json");
        assert!(!json);
    }

    #[test]
    fn plan_commit_alias_parses() {
        let cli = parse(&["generate", "plan-commit", "--plan-file", "/tmp/p.json"]);
        assert!(matches!(cli.cmd, Some(GenerateCmd::PlanCommit { .. })));
    }

    #[test]
    fn plan_submit_missing_plan_file_is_parse_error() {
        let err = parse_err(&["generate", "plan-submit"]);
        assert_ne!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    // ── plan-validate ─────────────────────────────────────────────────────

    #[test]
    fn plan_validate_parses() {
        let cli = parse(&["generate", "plan-validate", "--plan-file", "/a.json"]);
        assert!(matches!(cli.cmd, Some(GenerateCmd::PlanValidate { .. })));
    }

    // ── plan-verify ───────────────────────────────────────────────────────

    #[test]
    fn plan_verify_parses() {
        let cli = parse(&["generate", "plan-verify", "--plan-file", "/a.json"]);
        assert!(matches!(cli.cmd, Some(GenerateCmd::PlanVerify { .. })));
    }

    // ── plan-render ───────────────────────────────────────────────────────

    #[test]
    fn plan_render_parses() {
        let cli = parse(&["generate", "plan-render", "--plan-file", "/a.json"]);
        assert!(matches!(cli.cmd, Some(GenerateCmd::PlanRender { .. })));
    }

    // ── plan-speculate ────────────────────────────────────────────────────

    #[test]
    fn plan_speculate_parses() {
        let cli = parse(&["generate", "plan-speculate", "--plan-file", "/a.json"]);
        assert!(matches!(cli.cmd, Some(GenerateCmd::PlanSpeculate { .. })));
    }

    // ── plan-status ───────────────────────────────────────────────────────

    #[test]
    fn plan_status_parses() {
        let cli = parse(&["generate", "plan-status", "--plan-file", "/a.json"]);
        assert!(matches!(cli.cmd, Some(GenerateCmd::PlanStatus { .. })));
    }

    // ── plan-export ───────────────────────────────────────────────────────

    #[test]
    fn plan_export_default_format_is_json() {
        let cli = parse(&["generate", "plan-export", "--plan-file", "/a.json"]);
        let Some(GenerateCmd::PlanExport { format, .. }) = cli.cmd else {
            panic!("expected PlanExport");
        };
        assert_eq!(format, "json");
    }

    #[test]
    fn plan_export_toml_format() {
        let cli = parse(&[
            "generate",
            "plan-export",
            "--plan-file",
            "/a.json",
            "--format",
            "toml",
        ]);
        let Some(GenerateCmd::PlanExport { format, .. }) = cli.cmd else {
            panic!("expected PlanExport");
        };
        assert_eq!(format, "toml");
    }

    // ── plan-diff ─────────────────────────────────────────────────────────

    #[test]
    fn plan_diff_parses_both_files() {
        let cli = parse(&[
            "generate",
            "plan-diff",
            "--plan-file",
            "/a.json",
            "--other",
            "/b.json",
        ]);
        let Some(GenerateCmd::PlanDiff {
            plan_file, other, ..
        }) = cli.cmd
        else {
            panic!("expected PlanDiff");
        };
        assert_eq!(plan_file, "/a.json");
        assert_eq!(other, "/b.json");
    }

    #[test]
    fn plan_diff_missing_other_is_parse_error() {
        let err = parse_err(&["generate", "plan-diff", "--plan-file", "/a.json"]);
        assert_ne!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    // ── plan-critique ─────────────────────────────────────────────────────

    #[test]
    fn plan_critique_parses() {
        let cli = parse(&["generate", "plan-critique", "--plan-file", "/a.json"]);
        assert!(matches!(cli.cmd, Some(GenerateCmd::PlanCritique { .. })));
    }

    // ── plan-suggest ──────────────────────────────────────────────────────

    #[test]
    fn plan_suggest_parses_intent() {
        let cli = parse(&["generate", "plan-suggest", "--intent", "build auth module"]);
        let Some(GenerateCmd::PlanSuggest { intent, kind, json }) = cli.cmd else {
            panic!("expected PlanSuggest");
        };
        assert_eq!(intent, "build auth module");
        assert!(kind.is_none());
        assert!(!json);
    }

    #[test]
    fn plan_suggest_parses_kind_flag() {
        let cli = parse(&[
            "generate",
            "plan-suggest",
            "--intent",
            "foo",
            "--kind",
            "RustModule",
        ]);
        let Some(GenerateCmd::PlanSuggest { kind, .. }) = cli.cmd else {
            panic!("expected PlanSuggest");
        };
        assert_eq!(kind.as_deref(), Some("RustModule"));
    }

    // ── plan-recall ───────────────────────────────────────────────────────

    #[test]
    fn plan_recall_parses_query_and_default_limit() {
        let cli = parse(&["generate", "plan-recall", "--query", "auth"]);
        let Some(GenerateCmd::PlanRecall { query, limit, .. }) = cli.cmd else {
            panic!("expected PlanRecall");
        };
        assert_eq!(query, "auth");
        assert_eq!(limit, 10);
    }

    #[test]
    fn plan_recall_custom_limit() {
        let cli = parse(&["generate", "plan-recall", "--query", "foo", "--limit", "5"]);
        let Some(GenerateCmd::PlanRecall { limit, .. }) = cli.cmd else {
            panic!("expected PlanRecall");
        };
        assert_eq!(limit, 5);
    }

    // ── plan-history ──────────────────────────────────────────────────────

    #[test]
    fn plan_history_parses() {
        let cli = parse(&["generate", "plan-history", "--plan-file", "/a.json"]);
        assert!(matches!(cli.cmd, Some(GenerateCmd::PlanHistory { .. })));
    }

    // ── plan-replay ───────────────────────────────────────────────────────

    #[test]
    fn plan_replay_parses() {
        let cli = parse(&["generate", "plan-replay", "--plan-file", "/a.json"]);
        assert!(matches!(cli.cmd, Some(GenerateCmd::PlanReplay { .. })));
    }

    // ── plan-rollback ─────────────────────────────────────────────────────

    #[test]
    fn plan_rollback_parses() {
        let cli = parse(&["generate", "plan-rollback", "--plan-file", "/a.json"]);
        assert!(matches!(cli.cmd, Some(GenerateCmd::PlanRollback { .. })));
    }

    // ── template-list ─────────────────────────────────────────────────────

    #[test]
    fn template_list_parses() {
        let cli = parse(&["generate", "template-list"]);
        assert!(matches!(
            cli.cmd,
            Some(GenerateCmd::TemplateList { json: false })
        ));
    }

    // ── template-validate ─────────────────────────────────────────────────

    #[test]
    fn template_validate_parses_file() {
        let cli = parse(&["generate", "template-validate", "--template-file", "/t.hbs"]);
        let Some(GenerateCmd::TemplateValidate { template_file, .. }) = cli.cmd else {
            panic!("expected TemplateValidate");
        };
        assert_eq!(template_file, "/t.hbs");
    }

    // ── template-test ─────────────────────────────────────────────────────

    #[test]
    fn template_test_parses_template_and_vars() {
        let cli = parse(&[
            "generate",
            "template-test",
            "--template",
            "rust_module",
            "--vars",
            r#"{"name":"Foo"}"#,
        ]);
        let Some(GenerateCmd::TemplateTest {
            template,
            vars,
            json,
        }) = cli.cmd
        else {
            panic!("expected TemplateTest");
        };
        assert_eq!(template, "rust_module");
        assert_eq!(vars.as_deref(), Some(r#"{"name":"Foo"}"#));
        assert!(!json);
    }

    // ── schema-dump ───────────────────────────────────────────────────────

    #[test]
    fn schema_dump_parses() {
        let cli = parse(&["generate", "schema-dump"]);
        assert!(matches!(
            cli.cmd,
            Some(GenerateCmd::SchemaDump { json: false })
        ));
    }

    // ── capacity ──────────────────────────────────────────────────────────

    #[test]
    fn capacity_parses() {
        let cli = parse(&["generate", "capacity"]);
        assert!(matches!(
            cli.cmd,
            Some(GenerateCmd::Capacity { json: false })
        ));
    }

    // ── consumer-wiring ───────────────────────────────────────────────────

    #[test]
    fn consumer_wiring_default_limit() {
        let cli = parse(&["generate", "consumer-wiring"]);
        let Some(GenerateCmd::ConsumerWiring { limit, .. }) = cli.cmd else {
            panic!("expected ConsumerWiring");
        };
        assert_eq!(limit, 10);
    }

    #[test]
    fn consumer_wiring_custom_limit() {
        let cli = parse(&["generate", "consumer-wiring", "--limit", "5"]);
        let Some(GenerateCmd::ConsumerWiring { limit, .. }) = cli.cmd else {
            panic!("expected ConsumerWiring");
        };
        assert_eq!(limit, 5);
    }

    // ── autonomous ────────────────────────────────────────────────────────

    #[test]
    fn autonomous_parses_intent() {
        let cli = parse(&["generate", "autonomous", "--intent", "create auth service"]);
        let Some(GenerateCmd::Autonomous { intent, json }) = cli.cmd else {
            panic!("expected Autonomous");
        };
        assert_eq!(intent, "create auth service");
        assert!(!json);
    }

    #[test]
    fn autonomous_missing_intent_is_parse_error() {
        let err = parse_err(&["generate", "autonomous"]);
        assert_ne!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    // ── parse_vars helper ─────────────────────────────────────────────────

    #[test]
    fn parse_vars_none_returns_empty_map() {
        let m = parse_vars(None).unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn parse_vars_valid_object() {
        let m = parse_vars(Some(r#"{"key":"value","n":42}"#)).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m["key"], serde_json::json!("value"));
        assert_eq!(m["n"], serde_json::json!(42));
    }

    #[test]
    fn parse_vars_invalid_json_errors() {
        assert!(parse_vars(Some("not-json")).is_err());
    }

    #[test]
    fn parse_vars_non_object_errors() {
        assert!(parse_vars(Some("[1,2,3]")).is_err());
    }

    // ── parse_kind helper ─────────────────────────────────────────────────

    #[test]
    fn parse_kind_case_insensitive() {
        assert!(matches!(
            parse_kind("rustmodule"),
            Ok(GeneratorKind::RustModule)
        ));
        assert!(matches!(
            parse_kind("RustModule"),
            Ok(GeneratorKind::RustModule)
        ));
        assert!(matches!(
            parse_kind("rust-module"),
            Ok(GeneratorKind::RustModule)
        ));
        assert!(matches!(
            parse_kind("rust_module"),
            Ok(GeneratorKind::RustModule)
        ));
    }

    #[test]
    fn parse_kind_typescript_alias() {
        assert!(matches!(
            parse_kind("typescript"),
            Ok(GeneratorKind::TypeScriptModule)
        ));
    }

    #[test]
    fn parse_kind_python_alias() {
        // The intuitive Python names all resolve to the real `PythonScript`
        // kind (A11) — `PythonModule` never existed as a variant.
        for name in [
            "pythonmodule",
            "PythonModule",
            "python",
            "Python",
            "python-module",
        ] {
            assert!(
                matches!(parse_kind(name), Ok(GeneratorKind::PythonScript)),
                "alias {name:?} should resolve to PythonScript"
            );
        }
        // The canonical name still works unchanged.
        assert!(matches!(
            parse_kind("pythonscript"),
            Ok(GeneratorKind::PythonScript)
        ));
    }

    #[test]
    fn parse_kind_unknown_errors() {
        assert!(parse_kind("unknown-kind-xyz").is_err());
    }
}
