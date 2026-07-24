//! `touring suggest next|skill|action|list|stats|gc|consumed` — RL-guided and Pln3 bidirectional
//! action suggestions.
//!
//! Subcommands:
//! - `next [query]`  — RL engine next-action recommendation
//! - `skill [query]` — RL engine skill recommendation
//! - `action <action_type> <task_id> <reason> [--subtask=<id>]` — insert Pln3 suggestion
//! - `list [action_type]` — list pending suggestions
//! - `stats` — deactivation + acceptance stats per action_type
//! - `gc [--stale-days=N]` — garbage-collect consumed suggestions
//! - `consumed <suggestion_id> [consumed_action]` — mark suggestion consumed
//!
//! Wave P3-1.3 W3 (2026-06-11): migrated from manual positional parsing
//! (`arg_or`) to clap derive. All payload keys and conditions preserved (G6).

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring suggest",
    bin_name = "touring suggest",
    about = "RL-guided action and skill suggestions (Pln3 bidirectional flow)",
    disable_help_subcommand = true
)]
struct SuggestCli {
    #[command(subcommand)]
    cmd: Option<SuggestCmd>,
}

#[derive(Subcommand, Debug)]
enum SuggestCmd {
    /// RL engine next-action recommendation (default).
    Next {
        /// Optional query context (remaining positional args, joined).
        query: Vec<String>,
    },
    /// RL engine skill recommendation.
    Skill {
        /// Optional query context (remaining positional args, joined).
        query: Vec<String>,
    },
    /// Insert a Pln3 suggestion into the bidirectional flow.
    Action {
        /// Suggestion action type (e.g. `implement`, `refactor`).
        action_type: String,
        /// Target task id.
        task_id: String,
        /// Human-readable reason for the suggestion.
        reason: String,
        /// Optional target subtask id.
        #[arg(long)]
        subtask: Option<String>,
    },
    /// List pending suggestions, optionally filtered by action type.
    List {
        /// Optional action type filter (positional, not a flag).
        #[arg(default_value = "")]
        action_type: String,
        /// Maximum number of results to return.
        #[arg(long, default_value_t = 10)]
        limit: i64,
    },
    /// Deactivation + acceptance statistics per action type.
    Stats,
    /// Garbage-collect consumed suggestions older than N days.
    Gc {
        /// Retention window in days.
        #[arg(long = "stale-days", default_value_t = 30)]
        stale_days: i64,
    },
    /// Mark a suggestion as consumed.
    Consumed {
        /// Suggestion identifier.
        suggestion_id: String,
        /// The action that consumed the suggestion (default: `manual`).
        #[arg(default_value = "manual")]
        consumed_action: String,
    },
}

/// Entry point for the `touring suggest` CLI handler — dispatches the `next`
/// (default), `skill`, `action`, `list`, `stats`, `gc`, and `consumed`
/// subcommands of the RL-guided and Pln3 bidirectional suggestion flow to their
/// respective `cli-suggest-*`/`cli-suggestion-*` daemon queries and prints each
/// response. For `list`, an empty action-type filter is sent as JSON null.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match SuggestCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(SuggestCmd::Next { query: Vec::new() }) {
        SuggestCmd::Next { query } => {
            let output = daemon_query(
                "cli-suggest-next",
                serde_json::json!({ "query": query.join(" ") }),
            )?;
            println!("{output}");
        }
        SuggestCmd::Skill { query } => {
            let output = daemon_query(
                "cli-suggest-skill",
                serde_json::json!({ "query": query.join(" ") }),
            )?;
            println!("{output}");
        }
        SuggestCmd::Action {
            action_type,
            task_id,
            reason,
            subtask,
        } => {
            let payload = serde_json::json!({
                "action_type": action_type,
                "target_task_id": task_id,
                "reason": reason,
                "target_subtask_id": subtask,
            });
            let output = daemon_query("cli-suggest-action", payload)?;
            println!("{output}");
        }
        SuggestCmd::List { action_type, limit } => {
            // Preserve original G6 contract: action_type is null when empty string.
            let action_type_val: serde_json::Value = if action_type.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::json!(action_type)
            };
            let payload = serde_json::json!({
                "action_type": action_type_val,
                "limit": limit,
            });
            let output = daemon_query("cli-suggestion-list", payload)?;
            println!("{output}");
        }
        SuggestCmd::Stats => {
            let output = daemon_query("cli-suggestion-stats", serde_json::json!({}))?;
            println!("{output}");
        }
        SuggestCmd::Gc { stale_days } => {
            let output = daemon_query(
                "cli-suggestion-gc",
                serde_json::json!({ "retention_days": stale_days }),
            )?;
            println!("{output}");
        }
        SuggestCmd::Consumed {
            suggestion_id,
            consumed_action,
        } => {
            let output = daemon_query(
                "cli-suggestion-consumed",
                serde_json::json!({
                    "suggestion_id": suggestion_id,
                    "consumed_action": consumed_action,
                }),
            )?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    SuggestCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> SuggestCli {
        SuggestCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_suggest_defaults_to_next() {
        let cli = parse(&["suggest"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Next { query: [] }
    }

    #[test]
    fn next_with_query() {
        let cli = parse(&["suggest", "next", "what", "should", "I", "do"]);
        let Some(SuggestCmd::Next { query }) = cli.cmd else {
            panic!("expected Next");
        };
        assert_eq!(query.join(" "), "what should I do");
    }

    #[test]
    fn skill_with_query() {
        let cli = parse(&["suggest", "skill", "refactor"]);
        let Some(SuggestCmd::Skill { query }) = cli.cmd else {
            panic!("expected Skill");
        };
        assert_eq!(query.join(" "), "refactor");
    }

    #[test]
    fn action_parses_all_positionals() {
        let cli = parse(&[
            "suggest",
            "action",
            "implement",
            "task_123",
            "missing coverage",
        ]);
        let Some(SuggestCmd::Action {
            action_type,
            task_id,
            reason,
            subtask,
        }) = cli.cmd
        else {
            panic!("expected Action");
        };
        assert_eq!(action_type, "implement");
        assert_eq!(task_id, "task_123");
        assert_eq!(reason, "missing coverage");
        assert!(subtask.is_none());
    }

    #[test]
    fn action_parses_subtask_flag() {
        let cli = parse(&[
            "suggest",
            "action",
            "implement",
            "task_123",
            "reason",
            "--subtask=S-01",
        ]);
        let Some(SuggestCmd::Action { subtask, .. }) = cli.cmd else {
            panic!("expected Action");
        };
        assert_eq!(subtask.as_deref(), Some("S-01"));
    }

    #[test]
    fn list_defaults() {
        let cli = parse(&["suggest", "list"]);
        let Some(SuggestCmd::List { action_type, limit }) = cli.cmd else {
            panic!("expected List");
        };
        assert_eq!(action_type, "");
        assert_eq!(limit, 10);
    }

    #[test]
    fn list_with_action_type_and_limit() {
        let cli = parse(&["suggest", "list", "implement", "--limit", "25"]);
        let Some(SuggestCmd::List { action_type, limit }) = cli.cmd else {
            panic!("expected List");
        };
        assert_eq!(action_type, "implement");
        assert_eq!(limit, 25);
    }

    #[test]
    fn gc_default_stale_days() {
        let cli = parse(&["suggest", "gc"]);
        let Some(SuggestCmd::Gc { stale_days }) = cli.cmd else {
            panic!("expected Gc");
        };
        assert_eq!(stale_days, 30);
    }

    #[test]
    fn gc_custom_stale_days() {
        let cli = parse(&["suggest", "gc", "--stale-days=7"]);
        let Some(SuggestCmd::Gc { stale_days }) = cli.cmd else {
            panic!("expected Gc");
        };
        assert_eq!(stale_days, 7);
    }

    #[test]
    fn consumed_defaults_consumed_action() {
        let cli = parse(&["suggest", "consumed", "sugg_42"]);
        let Some(SuggestCmd::Consumed {
            suggestion_id,
            consumed_action,
        }) = cli.cmd
        else {
            panic!("expected Consumed");
        };
        assert_eq!(suggestion_id, "sugg_42");
        assert_eq!(consumed_action, "manual");
    }

    #[test]
    fn consumed_custom_action() {
        let cli = parse(&["suggest", "consumed", "sugg_42", "auto"]);
        let Some(SuggestCmd::Consumed {
            consumed_action, ..
        }) = cli.cmd
        else {
            panic!("expected Consumed");
        };
        assert_eq!(consumed_action, "auto");
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(SuggestCli::try_parse_from(["suggest", "frobnicate"]).is_err());
    }
}
