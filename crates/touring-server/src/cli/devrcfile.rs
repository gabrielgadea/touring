//! `touring devrcfile import|export` — Devrcfile YAML management.
//!
//! Imports a Devrcfile YAML (converted to Tasksfile format) into a decompose DAG,
//! and exports a decompose task back to Devrcfile format.
//!
//! Wave P3-1.3 W6a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The `run` signature is unchanged — receives full
//! argv slice; clap parses `args[1..]`.

use super::daemon_query;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "touring devrcfile",
    bin_name = "touring devrcfile",
    about = "Devrcfile YAML management (import, export)",
    disable_help_subcommand = true
)]
struct DevrcfileCli {
    #[command(subcommand)]
    cmd: Option<DevrcfileCmd>,
}

#[derive(Subcommand, Debug)]
enum DevrcfileCmd {
    /// Import a Devrcfile YAML and convert it into a decompose DAG.
    ///
    /// Reads the YAML, converts to Tasksfile format, and sends it to the daemon
    /// via the `cli-devrcfile-import` hook.
    Import {
        /// Path to the Devrcfile YAML (supports `~/` expansion).
        file: String,
    },
    /// Export a decompose task back to Devrcfile YAML format.
    Export {
        /// The task ID to export.
        task_id: String,
        /// Write output to this file instead of stdout (`--file=<path>`).
        #[arg(long = "file")]
        output_file: Option<String>,
    },
}
/// Expand `~/` prefix to home directory.
fn expand_path(s: &str) -> PathBuf {
    if s.starts_with("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(s.trim_start_matches("~/"));
    }
    PathBuf::from(s)
}
/// Entry point for the `touring devrcfile` CLI handler — parses the argv slice
/// into a `DevrcfileCmd` and dispatches `import` (reads a Devrcfile YAML,
/// expands `~/`, and sends it to the `cli-devrcfile-import` hook) or `export`
/// (fetches a task via `cli-devrcfile-export` and writes it to a file or
/// stdout). Prints `--help` when no subcommand is given.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match DevrcfileCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match cli.cmd {
        Some(DevrcfileCmd::Import { file }) => {
            #[allow(clippy::needless_borrow)]
            let yaml_content = std::fs::read_to_string(expand_path(&file))
                .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", file, e))?;
            let payload = serde_json::json!(
                { "hook" : "cli-devrcfile-import", "devrcfile_yaml" : yaml_content,
                "file_path" : file, }
            );
            let output = daemon_query("cli-devrcfile-import", payload)?;
            println!("{output}");
        }
        Some(DevrcfileCmd::Export {
            task_id,
            output_file,
        }) => {
            let payload = serde_json::json!(
                { "hook" : "cli-devrcfile-export", "task_id" : task_id, }
            );
            let output = daemon_query("cli-devrcfile-export", payload)?;
            if let Some(path) = output_file {
                // Same envelope-vs-document fix as `tasksfile export`. Note the
                // field really is `tasksfile_yaml`: `cli-devrcfile-export`
                // currently emits a Tasksfile, because no Devrcfile serializer
                // exists (`touring_orchestration::devrc` is import-only —
                // `parse_devrcfile` + `devrcfile_to_tasksfile`, no inverse).
                super::common::write_yaml_export(&output, "tasksfile_yaml", &expand_path(&path))?;
                println!("Exported to {} (Tasksfile format)", path);
            } else {
                println!("{output}");
            }
        }
        None => {
            DevrcfileCli::try_parse_from(["devrcfile", "--help"]).unwrap_or_else(|e| e.exit());
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    DevrcfileCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }
    #[test]
    fn import_requires_file_arg() {
        let result = DevrcfileCli::try_parse_from(s(&["devrcfile", "import"]).iter());
        assert!(result.is_err());
    }
    #[test]
    fn import_parses_file() {
        let cli =
            DevrcfileCli::try_parse_from(s(&["devrcfile", "import", "./devrcfile.yml"]).iter())
                .unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), DevrcfileCmd::Import { file } if file ==
            "./devrcfile.yml")
        );
    }
    #[test]
    fn export_requires_task_id() {
        let result = DevrcfileCli::try_parse_from(s(&["devrcfile", "export"]).iter());
        assert!(result.is_err());
    }
    #[test]
    fn export_parses_task_id() {
        let cli =
            DevrcfileCli::try_parse_from(s(&["devrcfile", "export", "task_12345"]).iter()).unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), DevrcfileCmd::Export { task_id, output_file : None
            } if task_id == "task_12345")
        );
    }
    #[test]
    fn export_with_output_file() {
        let cli = DevrcfileCli::try_parse_from(
            s(&["devrcfile", "export", "task_12345", "--file=./out.yml"]).iter(),
        )
        .unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), DevrcfileCmd::Export { output_file : Some(f), .. }
            if f == "./out.yml")
        );
    }
    #[test]
    fn expand_path_home() {
        let home = std::env::var("HOME").unwrap_or_default();
        let expanded = expand_path("~/foo.yml");
        assert!(expanded.to_string_lossy().starts_with(&home));
    }
    #[test]
    fn unknown_subcommand_is_rejected() {
        let result = DevrcfileCli::try_parse_from(s(&["devrcfile", "bogus"]).iter());
        assert!(result.is_err());
    }
}
