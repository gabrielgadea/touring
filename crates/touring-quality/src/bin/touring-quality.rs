//! `touring-quality` CLI — 50-dim quality scoring engine.
//!
//! ## Usage
//!
//! ```text
//! touring-quality score <target>                    # single file (auto-detect)
//! touring-quality score --workspace                 # entire project
//! touring-quality score <crate-dir> --scope crate   # one crate
//! touring-quality score <mod.rs|foo.rs|foo/> --scope module  # one Rust module
//! touring-quality score <dir> --scope path          # an arbitrary subtree
//! touring-quality score <target> --format json      # JSON output
//! touring-quality score <target> --format html -o report.html
//! touring-quality score <target> --format badge -o badge.svg
//! touring-quality check --gate F1.1 --target <path> # single dim
//! touring-quality list                             # list all 50 dims
//! ```

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use touring_quality::{
    DimId, OutputFormat, Scope, ScopeKind, report_to_string, scope_report, score_scope,
    score_target,
};

/// Touring Quality — 50-dim elite scoring engine
#[derive(Parser, Debug)]
#[command(name = "touring-quality")]
#[command(version, about = "50-dim quality scoring engine for elite code", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Score a target (file or workspace) on the 50 quality dimensions.
    Score {
        /// Target file or directory to score.
        target: PathBuf,

        /// Score the entire workspace (alias for `--scope workspace`).
        #[arg(long)]
        workspace: bool,

        /// Scope granularity: file|module|path|feature|crate|repo|project|system|workspace
        /// (auto-detected from the target when omitted; `module` is opt-in only —
        /// it resolves a Rust module-root + its submodule directory).
        #[arg(long)]
        scope: Option<String>,

        /// Include only files matching these globs (relative, `**`/`*`); makes a
        /// feature slice (the git-free "new code" / Clean as You Code analog).
        #[arg(long, value_delimiter = ',')]
        include: Vec<String>,

        /// Exclude files matching these globs.
        #[arg(long, value_delimiter = ',')]
        exclude: Vec<String>,

        /// Output format.
        #[arg(long, default_value = "compact")]
        format: String,

        /// Output to file instead of stdout.
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Score only specific dims (comma-separated, e.g. "F1.1,F2.5").
        #[arg(long, value_delimiter = ',')]
        dims: Vec<String>,

        /// Fail (exit 1) if composite score is below this threshold.
        #[arg(long)]
        fail_below: Option<f32>,
    },

    /// Check a single dim.
    Check {
        /// Dim to check (e.g. F1.1).
        #[arg(long)]
        gate: String,

        /// Target file or directory.
        #[arg(long)]
        target: PathBuf,

        /// Scope granularity (auto-detected from the target when omitted).
        #[arg(long)]
        scope: Option<String>,

        /// Output format.
        #[arg(long, default_value = "compact")]
        format: String,
    },

    /// List all 50 quality dimensions.
    List {
        /// Output format.
        #[arg(long, default_value = "compact")]
        format: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    match cli.command {
        Commands::Score {
            target,
            workspace,
            scope,
            include,
            exclude,
            format,
            output,
            dims,
            fail_below,
        } => cmd_score(ScoreArgs {
            target,
            workspace,
            scope,
            include,
            exclude,
            format,
            output,
            dims,
            fail_below,
        }),
        Commands::Check {
            gate,
            target,
            scope,
            format,
        } => cmd_check(gate, target, scope, format),
        Commands::List { format } => cmd_list(format),
    }
}

/// Arguments for the `score` subcommand (grouped to keep arity + CC low).
struct ScoreArgs {
    target: PathBuf,
    workspace: bool,
    scope: Option<String>,
    include: Vec<String>,
    exclude: Vec<String>,
    format: String,
    output: Option<PathBuf>,
    dims: Vec<String>,
    fail_below: Option<f32>,
}

fn cmd_score(args: ScoreArgs) -> Result<()> {
    let fmt = OutputFormat::from_str(&args.format)
        .map_err(|e| anyhow::anyhow!("invalid format '{}': {}", args.format, e))?;
    let target_path = if args.workspace {
        std::env::current_dir().context("get current dir")?
    } else {
        args.target.clone()
    };
    let dim_filter = parse_dims(&args.dims)?;
    let scope_kind = resolve_scope_kind(args.scope.as_deref(), args.workspace)?;
    let (rendered, composite) = score_to_string(
        &target_path,
        scope_kind,
        &args.include,
        &args.exclude,
        &dim_filter,
        fmt,
    )?;
    emit(&rendered, args.output, fmt)?;
    fail_if_below(composite, args.fail_below);
    Ok(())
}

/// Score a target on a dim filter, returning `(rendered, composite)`. Routes a
/// bare single file to the back-compat single-target path; anything with an
/// explicit scope/feature filter or a directory goes through the multi-scope
/// engine (`score_scope`).
fn score_to_string(
    target: &Path,
    scope_kind: Option<ScopeKind>,
    include: &[String],
    exclude: &[String],
    dim_filter: &[DimId],
    fmt: OutputFormat,
) -> Result<(String, f32)> {
    let use_scope =
        scope_kind.is_some() || !include.is_empty() || !exclude.is_empty() || target.is_dir();
    if use_scope {
        let resolved = Scope::resolve(target, scope_kind, include, exclude)?;
        let report = score_scope(&resolved, dim_filter)?;
        Ok((render_scope(&report, fmt)?, report.composite))
    } else {
        let report = score_target(target, dim_filter, fmt)
            .with_context(|| format!("failed to score {}", target.display()))?;
        Ok((report_to_string(&report, fmt)?, report.composite))
    }
}

/// Render a scope report (JSON or compact; HTML/badge fall back to compact).
fn render_scope(report: &scope_report::ScopeReport, fmt: OutputFormat) -> Result<String> {
    match fmt {
        OutputFormat::Json => scope_report::render_json(report),
        _ => Ok(scope_report::render_compact(report)),
    }
}

/// Write the rendered report to a file or stdout.
fn emit(rendered: &str, output: Option<PathBuf>, fmt: OutputFormat) -> Result<()> {
    if let Some(out_path) = output {
        std::fs::write(&out_path, rendered)
            .with_context(|| format!("failed to write to {}", out_path.display()))?;
        eprintln!("Wrote {} to {}", fmt, out_path.display());
    } else {
        print!("{}", rendered);
    }
    Ok(())
}

/// Exit 1 if the composite is below the optional `--fail-below` threshold.
fn fail_if_below(composite: f32, threshold: Option<f32>) {
    if let Some(t) = threshold
        && composite < t
    {
        eprintln!(
            "❌ Composite {:.3} < threshold {:.3} — exiting 1",
            composite, t
        );
        std::process::exit(1);
    }
}

/// Resolve the scope kind from `--scope`, `--workspace`, or auto-detect (`None`).
fn resolve_scope_kind(scope: Option<&str>, workspace: bool) -> Result<Option<ScopeKind>> {
    if let Some(s) = scope {
        Ok(Some(
            ScopeKind::from_str(s).map_err(|e| anyhow::anyhow!(e))?,
        ))
    } else if workspace {
        Ok(Some(ScopeKind::Workspace))
    } else {
        Ok(None)
    }
}

/// Parse a comma-separated dim filter (empty = all 50).
fn parse_dims(dims: &[String]) -> Result<Vec<DimId>> {
    let mut out = Vec::with_capacity(dims.len());
    for s in dims {
        out.push(parse_dim_id(s)?);
    }
    Ok(out)
}

fn cmd_check(gate: String, target: PathBuf, scope: Option<String>, format: String) -> Result<()> {
    let fmt = OutputFormat::from_str(&format)
        .map_err(|e| anyhow::anyhow!("invalid format '{}': {}", format, e))?;
    let dim = parse_dim_id(&gate)?;
    let scope_kind = resolve_scope_kind(scope.as_deref(), false)?;
    let (rendered, _) = score_to_string(&target, scope_kind, &[], &[], &[dim], fmt)?;
    print!("{}", rendered);
    Ok(())
}

fn cmd_list(format: String) -> Result<()> {
    let fmt = OutputFormat::from_str(&format)
        .map_err(|e| anyhow::anyhow!("invalid format '{}': {}", format, e))?;

    if fmt == OutputFormat::Json {
        let json: Vec<_> = DimId::ALL
            .iter()
            .map(|d| {
                serde_json::json!({
                    "id": d.as_str(),
                    "name": d.name(),
                    "short_name": d.short_name(),
                    "phase": d.phase(),
                    "enforcement": format!("{:?}", d.enforcement()),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(());
    }

    println!("Touring Quality — 50 dimensions:");
    println!();
    for dim in DimId::ALL {
        let marker = match dim.enforcement() {
            touring_quality::Enforcement::Block => "⛔",
            touring_quality::Enforcement::Warn => "⚠ ",
            touring_quality::Enforcement::Advisory => "  ",
        };
        println!("  {} {} — {}", marker, dim.as_str(), dim.name());
    }
    println!();
    println!("Legend: ⛔ BLOCK (fail-closed) | ⚠ WARN (advisory) | ADVISORY (log only)");
    Ok(())
}

fn parse_dim_id(s: &str) -> Result<DimId> {
    let s = s.trim();
    for dim in DimId::ALL {
        if dim.as_str().eq_ignore_ascii_case(s) {
            return Ok(*dim);
        }
        if dim.short_name().eq_ignore_ascii_case(s) {
            return Ok(*dim);
        }
    }
    anyhow::bail!(
        "unknown dim: '{}'. Use `touring-quality list` to see all 50.",
        s
    )
}
