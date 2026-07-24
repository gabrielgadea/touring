//! `touring projects` — Multi-project registry CLI.
//!
//! Provides commands to list, add, remove, and switch between touring workspaces.
//! Uses the [`ProjectRegistry`] from `touring-server::projects`.

use crate::projects::{ProjectEntry, ProjectRegistry};
use anyhow::{Context, Result};
use std::path::PathBuf;

const USAGE: &str = "\
Usage:
  touring projects [list]              List all registered projects (default)
  touring projects add <alias> <path>  Register a new project workspace
  touring projects remove <alias>      Unregister a project
  touring projects switch <alias>      Set the active project
  touring projects info <alias>        Show details of a specific project
  touring projects -h|--help           Show this help and exit 0
";

/// Print usage to stdout and return `Ok(())` (exit 0 semantics for `-h`/`--help`).
fn run_help() -> Result<()> {
    print!("{USAGE}");
    Ok(())
}

/// Run the projects subcommand dispatcher.
pub fn run(args: &[String]) -> Result<()> {
    let subcmd = args.get(2).map(|s| s.as_str()).unwrap_or("list");

    match subcmd {
        "-h" | "--help" => run_help(),
        "list" => run_list(args),
        "add" => run_add(args),
        "remove" => run_remove(args),
        "switch" => run_switch(args),
        "info" => run_info(args),
        _ => {
            eprintln!(
                "Unknown projects subcommand: {}. Use: list, add, remove, switch, info",
                subcmd
            );
            eprint!("{USAGE}");
            std::process::exit(1);
        }
    }
}

/// `touring projects list` — List all registered projects.
fn run_list(_args: &[String]) -> Result<()> {
    let mut registry = ProjectRegistry::with_default_path();
    registry
        .load()
        .context("Failed to load projects registry")?;

    let entries: Vec<_> = registry.entries().collect();

    if entries.is_empty() {
        println!("{{\"projects\": [], \"count\": 0, \"current\": null}}");
        return Ok(());
    }

    let current = registry.current_project().map(|e| e.alias.clone());

    let projects: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "alias": e.alias,
                "path": e.path.display().to_string(),
                "last_used": e.last_used.to_rfc3339(),
                "is_default": e.is_default,
                "daemon_socket": e.daemon_socket.as_ref().map(|p| p.display().to_string()),
            })
        })
        .collect();

    println!(
        "{}",
        serde_json::json!({
            "projects": projects,
            "count": projects.len(),
            "current": current,
        })
    );

    Ok(())
}

/// `touring projects add <alias> <path>` — Register a new project.
fn run_add(args: &[String]) -> Result<()> {
    let alias = args
        .get(3)
        .context("Usage: touring projects add <alias> <path>")?;
    let path_str = args
        .get(4)
        .context("Usage: touring projects add <alias> <path>")?;
    let path = PathBuf::from(path_str);

    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }

    let mut registry = ProjectRegistry::with_default_path();
    registry
        .load()
        .context("Failed to load projects registry")?;

    // Check if alias already exists
    if registry.find_by_alias(alias).is_some() {
        anyhow::bail!(
            "Project with alias '{}' already exists. Use 'touring projects remove {}' first.",
            alias,
            alias
        );
    }

    let entry = ProjectEntry::new(alias, &path);
    registry.add(entry).context("Failed to add project")?;
    registry
        .save()
        .context("Failed to save projects registry")?;

    println!(
        "{}",
        serde_json::json!({
            "success": true,
            "alias": alias,
            "path": path_str,
            "message": format!("Project '{}' registered at {}", alias, path.display()),
        })
    );

    Ok(())
}

/// `touring projects remove <alias>` — Unregister a project.
fn run_remove(args: &[String]) -> Result<()> {
    let alias = args
        .get(3)
        .context("Usage: touring projects remove <alias>")?;

    let mut registry = ProjectRegistry::with_default_path();
    registry
        .load()
        .context("Failed to load projects registry")?;

    if registry.remove(alias).is_some() {
        registry
            .save()
            .context("Failed to save projects registry")?;
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "alias": alias,
                "message": format!("Project '{}' removed from registry.", alias),
            })
        );
    } else {
        println!(
            "{}",
            serde_json::json!({
                "success": false,
                "alias": alias,
                "message": format!("Project '{}' not found in registry.", alias),
            })
        );
    }

    Ok(())
}

/// `touring projects switch <alias>` — Set the active project.
fn run_switch(args: &[String]) -> Result<()> {
    let alias = args
        .get(3)
        .context("Usage: touring projects switch <alias>")?;

    let mut registry = ProjectRegistry::with_default_path();
    registry
        .load()
        .context("Failed to load projects registry")?;

    // Verify alias exists first
    if registry.find_by_alias(alias).is_none() {
        anyhow::bail!(
            "Project '{}' not found. Use 'touring projects list' to see available projects.",
            alias
        );
    }

    registry.set_current(Some(alias.to_string()));
    registry
        .save()
        .context("Failed to save projects registry")?;

    println!(
        "{}",
        serde_json::json!({
            "success": true,
            "alias": alias,
            "message": format!("Switched to project '{}'.", alias),
        })
    );

    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────────

// `run_info` (a production fn) is defined after this module — a pre-existing layout.
// Allow the lint here rather than reorder unrelated code in an R4-scoped change.
#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod tests {
    use super::*;

    fn sv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    // ── help flag (A2: -h/--help must exit 0 via Ok) ─────────────────────

    #[test]
    fn help_short_returns_ok() {
        let args = sv(&["touring", "projects", "-h"]);
        assert!(run(&args).is_ok());
    }

    #[test]
    fn help_long_returns_ok() {
        let args = sv(&["touring", "projects", "--help"]);
        assert!(run(&args).is_ok());
    }

    #[test]
    fn run_help_fn_returns_ok() {
        assert!(run_help().is_ok());
    }

    // ── usage constant sanity ─────────────────────────────────────────────

    #[test]
    fn usage_contains_all_subcommands() {
        for sub in &["list", "add", "remove", "switch", "info"] {
            assert!(USAGE.contains(sub), "USAGE missing subcommand: {sub}");
        }
    }
}

/// `touring projects info <alias>` — Show details of a specific project.
fn run_info(args: &[String]) -> Result<()> {
    let alias = args
        .get(3)
        .context("Usage: touring projects info <alias>")?;

    let mut registry = ProjectRegistry::with_default_path();
    registry
        .load()
        .context("Failed to load projects registry")?;

    match registry.find_by_alias(alias) {
        Some(entry) => {
            println!(
                "{}",
                serde_json::json!({
                    "alias": entry.alias,
                    "path": entry.path.display().to_string(),
                    "last_used": entry.last_used.to_rfc3339(),
                    "is_default": entry.is_default,
                    "daemon_socket": entry.daemon_socket.as_ref().map(|p| p.display().to_string()),
                    "exists": entry.path.exists(),
                })
            );
        }
        None => {
            println!(
                "{}",
                serde_json::json!({
                    "error": format!("Project '{}' not found", alias),
                })
            );
        }
    }

    Ok(())
}
