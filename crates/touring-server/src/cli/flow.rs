//! `touring flow` — Flow pipeline builder CLI.
//!
//! Provides a command-line interface to the [`TouringFlowBuilder`] API,
//! allowing users to list available stages, run pipelines from YAML config,
//! and validate pipeline configurations.
//!
//! ## Subcommands
//!
//! - `list` — list all available pipeline stage kinds
//! - `run <yaml_file>` — build and run a pipeline from a YAML config file
//! - `validate <yaml_file>` — validate a pipeline config without running it

use std::path::PathBuf;
use std::str::FromStr;

use touring_orchestration::flow::stages::{Filter, Transform};
use touring_orchestration::flow::{Item, TouringFlowBuilder};

/// Stage kinds available for pipeline construction.
#[derive(Debug, serde::Serialize)]
struct StageInfo {
    kind: &'static str,
    description: &'static str,
    example: &'static str,
}

/// List all stage kinds available in the flow pipeline system.
fn list_stages() -> anyhow::Result<()> {
    let stages = vec![
        StageInfo {
            kind: "filter",
            description: "Pass items that satisfy a predicate",
            example: "filter: { name: 'even', predicate: 'item.id.starts_with(\"even\")' }",
        },
        StageInfo {
            kind: "transform",
            description: "Map items through a transformation closure",
            example: "transform: { name: 'upper', fn: 'item.label.to_uppercase()' }",
        },
        StageInfo {
            kind: "inspect",
            description: "Side-effect only; passes item through unchanged",
            example: "inspect: { name: 'log', side_effect: 'println!(..)' }",
        },
        StageInfo {
            kind: "fan_out",
            description: "Dispatch each item to multiple sub-stages",
            example: "fan_out: { branches: [{ name: 'a', stages: [...] }, { name: 'b', stages: [...] }] }",
        },
        StageInfo {
            kind: "fan_in",
            description: "Collect outputs from multiple sub-stages into one",
            example: "fan_in: { sources: ['source_a', 'source_b'], merge: 'concat' }",
        },
    ];

    println!("Available pipeline stages:\n");
    for s in &stages {
        println!(
            "  {:12} — {}\n    Example: {}",
            s.kind, s.description, s.example
        );
    }
    println!("\nUse `touring flow run <config.yaml>` to execute a pipeline.");
    Ok(())
}

/// Parse a minimal YAML config into a TouringFlowBuilder pipeline.
/// This is a simplified parser for demonstration — full YAML parsing
/// would require a crate like `serde_yaml` or `tyaml`.
fn parse_pipeline_config(yaml_content: &str) -> anyhow::Result<TouringFlowBuilder> {
    let mut builder = TouringFlowBuilder::new();

    for line in yaml_content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(stage_line) = line.strip_prefix("filter:") {
            let name = stage_line
                .trim()
                .strip_prefix("name:")
                .or_else(|| stage_line.trim().split(',').find(|p| p.contains("name:")))
                .map(|p| p.split(':').nth(1).unwrap_or("filter").trim())
                .unwrap_or("filter");
            let _filter = Filter::new(name, |_item: &Item| true);
            // For CLI demo, add a pass-through filter stage
            builder = builder.add_stage(name.to_string(), Filter::new(name, |_item: &Item| true));
        } else if let Some(transform_line) = line.strip_prefix("transform:") {
            let name = transform_line
                .trim()
                .split(',')
                .find(|p| p.contains("name:"))
                .map(|p| p.split(':').nth(1).unwrap_or("transform").trim())
                .unwrap_or("transform");
            builder = builder.add_stage(
                name.to_string(),
                Transform::new(name, |item: Item| {
                    Ok(Item::new(item.id.clone(), item.label.clone()))
                }),
            );
        }
    }

    Ok(builder)
}

/// Validate a pipeline configuration file without running it.
fn validate_config(config_path: &str) -> anyhow::Result<()> {
    let path = PathBuf::from_str(config_path)
        .map_err(|e| anyhow::anyhow!("invalid path '{}': {}", config_path, e))?;

    if !path.exists() {
        anyhow::bail!("config file not found: {}", config_path);
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {}", config_path, e))?;

    let builder = parse_pipeline_config(&content)?;

    println!("valid: {}", config_path);
    println!("  stages: {}", builder.build().stages().len());
    Ok(())
}

/// Run a pipeline from a YAML configuration file.
fn run_pipeline(config_path: &str) -> anyhow::Result<()> {
    let path = PathBuf::from_str(config_path)
        .map_err(|_| anyhow::anyhow!("invalid path: {}", config_path))?;

    if !path.exists() {
        anyhow::bail!("config file not found: {}", config_path);
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {}", config_path, e))?;

    let builder = parse_pipeline_config(&content)?;
    let pipeline = builder.build();

    println!("Running pipeline from: {}", config_path);
    println!("  stages: {}", pipeline.stages().len());

    let item = Item::new("cli-run-1", "hello from touring flow");
    let result = pipeline.run(item);
    if result.is_ok() {
        println!(
            "  result: id={}, label={}",
            result.item.id, result.item.label
        );
    } else {
        anyhow::bail!("pipeline error: {:?}", result.stage_outcomes);
    }

    Ok(())
}

/// Run the `touring flow` subcommand dispatcher.
///
/// Receives the full args slice: `["touring", "flow", <subcommand>, ...]`
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let subcommand = args.get(2).map(|s| s.as_str()).unwrap_or("list");

    match subcommand {
        "list" => list_stages(),
        "validate" => {
            let config_path = args
                .get(3)
                .ok_or_else(|| anyhow::anyhow!("usage: touring flow validate <config.yaml>"))?;
            validate_config(config_path)
        }
        "run" => {
            let config_path = args
                .get(3)
                .ok_or_else(|| anyhow::anyhow!("usage: touring flow run <config.yaml>"))?;
            run_pipeline(config_path)
        }
        "--help" | "-h" | "help" => {
            println!("touring flow — Flow pipeline builder CLI");
            println!();
            println!("Usage: touring flow <subcommand> [args]");
            println!();
            println!("Subcommands:");
            println!("  list                List all available pipeline stage kinds");
            println!("  run <config.yaml>  Build and run a pipeline from a YAML config");
            println!("  validate <config.yaml>  Validate a pipeline config without running");
            println!();
            println!("Examples:");
            println!("  touring flow list");
            println!("  touring flow run pipeline.yaml");
            println!("  touring flow validate pipeline.yaml");
            Ok(())
        }
        _ => {
            eprintln!(
                "unknown flow subcommand: '{}'. Use 'touring flow --help' for usage.",
                subcommand
            );
            Err(anyhow::anyhow!("unknown subcommand: {}", subcommand))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_stages() {
        let result = list_stages();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_nonexistent() {
        let result = validate_config("/nonexistent/path.yaml");
        assert!(result.is_err());
    }

    #[test]
    fn test_run_no_args() {
        // When no subcommand, defaults to "list"
        let args = vec!["touring".to_string(), "flow".to_string()];
        let result = run(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_empty_config() {
        let builder = parse_pipeline_config("").expect("empty config must parse");
        assert_eq!(builder.build().stages().len(), 0);
    }
}
