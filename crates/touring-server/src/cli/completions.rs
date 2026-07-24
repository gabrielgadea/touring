//! `touring completions <shell|man>` — shell completions and man page generation.
//!
//! Assembles the full top-level `touring` Command from the 47 clap-migrated
//! handlers and uses `clap_complete` / `clap_mangen` to emit completions or
//! a man page to stdout (W7; closes Master Plan B-W1.T5).
//!
//! Wave P3-1.3 W7 FINAL (2026-06-11).

use clap::{Parser, ValueEnum};

// ── CLI struct ───────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "touring completions",
    bin_name = "touring completions",
    about = "Generate shell completions or a man page for the touring CLI",
    long_about = "Generate shell tab-completions (bash/zsh/fish/powershell/elvish) \
                  or a man page for all touring subcommands.\n\n\
                  Examples:\n  \
                  touring completions bash >> ~/.bash_completion\n  \
                  touring completions zsh  > ~/.zfunc/_touring\n  \
                  touring completions man  > touring.1"
)]
struct CompletionsCli {
    /// Output target: shell name or 'man'.
    #[arg(value_enum)]
    target: Target,
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
enum Target {
    Bash,
    Zsh,
    Fish,
    Powershell,
    Elvish,
    Man,
}

// ── Top-level command assembly ───────────────────────────────────────────────

/// Assemble the full top-level `touring` Command from the 47 clap-migrated
/// handlers.  Each `.subcommand()` call uses the canonical name that appears
/// in `common::command_table()` so completions match what users actually type.
pub(super) fn assemble_top_level() -> clap::Command {
    clap::Command::new("touring")
        .about("Touring — code-native, agent-first infrastructure for code-generating agents")
        .subcommand(super::assist::command().name("assist"))
        .subcommand(super::ast::command().name("ast"))
        .subcommand(super::cascade::command().name("cascade"))
        .subcommand(super::clones::command().name("clones"))
        .subcommand(command().name("completions"))
        .subcommand(super::cognitive::command().name("cognitive"))
        .subcommand(super::conflict::command().name("conflict"))
        .subcommand(super::daemon_ctl::command().name("daemon-ctl"))
        .subcommand(super::decompose::command().name("decompose"))
        .subcommand(super::definitions::command().name("definitions"))
        .subcommand(super::devrcfile::command().name("devrcfile"))
        .subcommand(super::diagnostics::command().name("diagnostics"))
        .subcommand(super::diary::command().name("diary"))
        .subcommand(super::e2e::command().name("e2e"))
        .subcommand(super::evolution::command().name("evolution"))
        .subcommand(super::file_knowledge::command().name("file-knowledge"))
        .subcommand(super::find_references::command().name("find-references"))
        .subcommand(super::fix::command().name("fix"))
        .subcommand(super::flywheel::command().name("flywheel"))
        .subcommand(super::gate_metrics::command().name("gate-metrics"))
        .subcommand(super::generate::command().name("generate"))
        .subcommand(super::gotcha::command().name("gotcha"))
        .subcommand(super::granularity::command().name("granularity"))
        .subcommand(super::graph::command().name("graph"))
        .subcommand(super::health_delta::command().name("health-delta"))
        .subcommand(super::highlight::command().name("highlight"))
        .subcommand(super::init::command().name("init"))
        .subcommand(super::incremental::command().name("incremental"))
        .subcommand(super::index::command().name("index"))
        .subcommand(super::inferlets::command().name("inferlets"))
        .subcommand(super::jobs::command().name("jobs"))
        .subcommand(super::learning::command().name("learning"))
        .subcommand(super::mcts::command().name("mcts"))
        .subcommand(super::memory::command().name("memory"))
        .subcommand(super::overlay::command().name("overlay"))
        .subcommand(super::plugin::command().name("plugin"))
        .subcommand(super::saga::command().name("saga"))
        .subcommand(super::search_unified::command().name("search"))
        .subcommand(super::session::command().name("session"))
        .subcommand(super::shadow::command().name("shadow"))
        .subcommand(super::snapshot::command().name("snapshot"))
        .subcommand(super::source_change::command().name("source-change"))
        .subcommand(super::suggest::command().name("suggest"))
        .subcommand(super::synergy::command().name("synergy"))
        .subcommand(super::tantivy::command().name("tantivy"))
        .subcommand(super::tasksfile::command().name("tasksfile"))
        .subcommand(super::viz::command().name("viz"))
        .subcommand(super::wiring::command().name("wiring"))
        .subcommand(super::workflow::command().name("workflow"))
}

// ── Handler ──────────────────────────────────────────────────────────────────

/// Run `touring completions <target>`.
///
/// Parses `args[1..]` (so "completions" acts as the bin-name) and writes
/// the requested output to stdout.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = CompletionsCli::try_parse_from(args.iter().skip(1)).unwrap_or_else(|e| e.exit());

    let mut top = assemble_top_level();

    match cli.target {
        Target::Man => {
            let man = clap_mangen::Man::new(top);
            man.render(&mut std::io::stdout())?;
        }
        shell => {
            let shell_id = match shell {
                Target::Bash => clap_complete::Shell::Bash,
                Target::Zsh => clap_complete::Shell::Zsh,
                Target::Fish => clap_complete::Shell::Fish,
                Target::Powershell => clap_complete::Shell::PowerShell,
                Target::Elvish => clap_complete::Shell::Elvish,
                Target::Man => unreachable!("handled above"),
            };
            clap_complete::generate(shell_id, &mut top, "touring", &mut std::io::stdout());
        }
    }

    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    CompletionsCli::command()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_top_level_has_at_least_45_subcommands() {
        let cmd = assemble_top_level();
        let count = cmd.get_subcommands().count();
        assert!(
            count >= 45,
            "expected >= 45 subcommands in assemble_top_level, got {count}"
        );
    }

    #[test]
    fn all_target_variants_parse() {
        for target in ["bash", "zsh", "fish", "powershell", "elvish", "man"] {
            let result = CompletionsCli::try_parse_from(["completions", target]);
            assert!(
                result.is_ok(),
                "failed to parse target '{target}': {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn bash_completions_generate_nonempty_output() {
        use clap_complete::Shell;
        let mut top = assemble_top_level();
        let mut buf: Vec<u8> = Vec::new();
        clap_complete::generate(Shell::Bash, &mut top, "touring", &mut buf);
        assert!(
            !buf.is_empty(),
            "bash completions output should not be empty"
        );
        // Basic sanity: bash completions start with the shebang or function def
        let output = String::from_utf8_lossy(&buf);
        assert!(
            output.contains("touring") || output.contains("_touring"),
            "bash completions should reference 'touring'"
        );
    }

    #[test]
    fn zsh_completions_generate_nonempty_output() {
        use clap_complete::Shell;
        let mut top = assemble_top_level();
        let mut buf: Vec<u8> = Vec::new();
        clap_complete::generate(Shell::Zsh, &mut top, "touring", &mut buf);
        assert!(
            !buf.is_empty(),
            "zsh completions output should not be empty"
        );
    }

    #[test]
    fn man_page_generates_nonempty_output() {
        let top = assemble_top_level();
        let man = clap_mangen::Man::new(top);
        let mut buf: Vec<u8> = Vec::new();
        man.render(&mut buf).expect("man render should succeed");
        assert!(!buf.is_empty(), "man page output should not be empty");
        let output = String::from_utf8_lossy(&buf);
        assert!(
            output.contains("touring"),
            "man page should contain 'touring'"
        );
    }

    #[test]
    fn unknown_target_is_rejected() {
        let result = CompletionsCli::try_parse_from(["completions", "tcsh"]);
        assert!(result.is_err(), "unknown shell 'tcsh' should fail to parse");
    }
}
