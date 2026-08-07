//! `touring tasksfile import|export|validate` — Tasksfile YAML management.
//!
//! Manages Tasksfile YAML import (into decompose DAG), export (from decompose), and validation.
//!
//! Wave P3-1.3 W4 (2026-06-11): migrated from manual `arg_or` to clap derive.
//! `expand_path` helper and `print_help` are preserved verbatim (G6).
//! Payload keys and hook names are unchanged.

use super::daemon_query;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Read a file path argument, expanding `~` to home.
fn expand_path(s: &str) -> PathBuf {
    if s.starts_with("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(s.trim_start_matches("~/"));
    }
    PathBuf::from(s)
}

#[derive(Parser, Debug)]
#[command(
    name = "touring tasksfile",
    bin_name = "touring tasksfile",
    about = "Tasksfile YAML management: import, export, validate, help (default)",
    disable_help_subcommand = true
)]
struct TasksfileCli {
    #[command(subcommand)]
    cmd: Option<TasksfileCmd>,
}

#[derive(Subcommand, Debug)]
enum TasksfileCmd {
    /// Import a Tasksfile YAML and create a decompose DAG from it.
    Import {
        /// Path to the Tasksfile YAML (supports ~/ expansion).
        file: String,
        /// Optional existing task ID to append subtasks to.
        #[arg(long = "task-id")]
        task_id: Option<String>,
        /// Task type (default: "tasksfile").
        #[arg(long = "type", default_value = "tasksfile")]
        task_type: String,
    },
    /// Export a decompose task back to Tasksfile YAML format.
    Export {
        /// The task ID to export.
        task_id: String,
        /// Write to file instead of stdout.
        #[arg(long = "file")]
        output_file: Option<String>,
    },
    /// Validate a Tasksfile YAML file for syntax and schema correctness.
    Validate {
        /// Path to the Tasksfile YAML (supports ~/ expansion).
        file: String,
    },
    /// Show help for a subcommand (default).
    Help,
}

/// Run the `tasksfile` CLI subcommand dispatcher.
///
/// # Errors
///
/// Returns an error if a required file argument is missing, file I/O fails,
/// an unknown subcommand is given, or the daemon is unreachable.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match TasksfileCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(TasksfileCmd::Help) {
        TasksfileCmd::Import {
            file,
            task_id,
            task_type,
        } => {
            #[allow(clippy::needless_borrow)]
            let yaml_content = std::fs::read_to_string(expand_path(&file))
                .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", file, e))?;
            let task_id_val = match task_id {
                Some(id) => serde_json::json!(id),
                None => serde_json::Value::Null,
            };
            let payload = serde_json::json!({
                "task_type": task_type,
                "description": format!("Tasksfile import: {}", file),
                "tasksfile_yaml": yaml_content,
                "task_id": task_id_val,
            });
            let output = daemon_query("cli-decompose-create", payload)?;
            println!("{output}");
        }
        TasksfileCmd::Export {
            task_id,
            output_file,
        } => {
            let payload = serde_json::json!({
                "hook": "cli-tasksfile-export",
                "task_id": task_id,
            });
            let output = daemon_query("cli-tasksfile-export", payload)?;
            if let Some(path) = output_file {
                // Write the YAML document, not the JSON envelope around it —
                // the envelope made `tasksfile validate` reject its own export.
                super::common::write_yaml_export(
                    &output,
                    "tasksfile_yaml",
                    &expand_path(&path),
                )?;
                println!("Exported to {}", path);
            } else {
                println!("{output}");
            }
        }
        TasksfileCmd::Validate { file } => {
            #[allow(clippy::needless_borrow)]
            let yaml_content = std::fs::read_to_string(expand_path(&file))
                .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", file, e))?;
            let payload = serde_json::json!({
                "hook": "cli-tasksfile-validate",
                "yaml": yaml_content,
            });
            let output = daemon_query("cli-tasksfile-validate", payload)?;
            println!("{output}");
        }
        TasksfileCmd::Help => {
            println!(
                r#"touring tasksfile — Tasksfile YAML management

Subcommands:
  import <file>        Import a Tasksfile YAML into a decompose DAG
  export <task_id>    Export a decompose task to Tasksfile YAML
  validate <file>     Validate a Tasksfile YAML for correctness

Use 'touring tasksfile <subcommand> --help' for subcommand-specific help."#
            );
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    TasksfileCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    fn parse(args: &[&str]) -> TasksfileCli {
        TasksfileCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_tasksfile_defaults_to_help() {
        let cli = parse(&["tasksfile"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Help
    }

    #[test]
    fn explicit_help_parses() {
        let cli = parse(&["tasksfile", "help"]);
        assert!(matches!(cli.cmd, Some(TasksfileCmd::Help)));
    }

    #[test]
    fn import_parses_file() {
        let cli = parse(&["tasksfile", "import", "tasks.yml"]);
        let Some(TasksfileCmd::Import {
            file,
            task_id,
            task_type,
        }) = cli.cmd
        else {
            panic!("expected Import");
        };
        assert_eq!(file, "tasks.yml");
        assert!(task_id.is_none());
        assert_eq!(task_type, "tasksfile");
    }

    #[test]
    fn import_parses_task_id_and_type() {
        let cli = parse(&[
            "tasksfile",
            "import",
            "tasks.yml",
            "--task-id",
            "task_123",
            "--type",
            "ci",
        ]);
        let Some(TasksfileCmd::Import {
            file,
            task_id,
            task_type,
        }) = cli.cmd
        else {
            panic!("expected Import");
        };
        assert_eq!(file, "tasks.yml");
        assert_eq!(task_id.as_deref(), Some("task_123"));
        assert_eq!(task_type, "ci");
    }

    #[test]
    fn import_missing_file_is_parse_error() {
        assert!(TasksfileCli::try_parse_from(["tasksfile", "import"]).is_err());
    }

    #[test]
    fn export_parses_task_id() {
        let cli = parse(&["tasksfile", "export", "task_abc"]);
        let Some(TasksfileCmd::Export {
            task_id,
            output_file,
        }) = cli.cmd
        else {
            panic!("expected Export");
        };
        assert_eq!(task_id, "task_abc");
        assert!(output_file.is_none());
    }

    #[test]
    fn export_parses_output_file() {
        let cli = parse(&["tasksfile", "export", "task_abc", "--file", "out.yml"]);
        let Some(TasksfileCmd::Export {
            task_id,
            output_file,
        }) = cli.cmd
        else {
            panic!("expected Export");
        };
        assert_eq!(task_id, "task_abc");
        assert_eq!(output_file.as_deref(), Some("out.yml"));
    }

    #[test]
    fn export_missing_task_id_is_parse_error() {
        assert!(TasksfileCli::try_parse_from(["tasksfile", "export"]).is_err());
    }

    #[test]
    fn validate_parses_file() {
        let cli = parse(&["tasksfile", "validate", "tasks.yml"]);
        let Some(TasksfileCmd::Validate { file }) = cli.cmd else {
            panic!("expected Validate");
        };
        assert_eq!(file, "tasks.yml");
    }

    #[test]
    fn validate_missing_file_is_parse_error() {
        assert!(TasksfileCli::try_parse_from(["tasksfile", "validate"]).is_err());
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(TasksfileCli::try_parse_from(["tasksfile", "frobnicate"]).is_err());
    }

    #[test]
    fn help_subcommand_runs_without_error() {
        let args = s(&["touring", "tasksfile", "help"]);
        let result = run(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn expand_path_expands_tilde() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        let expanded = expand_path("~/foo.yml");
        assert_eq!(expanded, std::path::PathBuf::from(home).join("foo.yml"));
    }

    #[test]
    fn expand_path_leaves_absolute_unchanged() {
        let expanded = expand_path("/tmp/tasks.yml");
        assert_eq!(expanded, std::path::PathBuf::from("/tmp/tasks.yml"));
    }
}
