//! `touring decompose create|add|get|update|validate|status|finalize|ready|templates|template`
//! — DAG task decomposition.
//!
//! Manages hierarchical task decomposition for multi-step workflows.
//!
//! Wave P3-1.3 pilot (2026-06-11): migrated from manual positional parsing
//! (`arg_or`) to clap derive. The dispatch contract is unchanged — `run`
//! still receives the full argv slice; clap parses `args[1..]` so the
//! subcommand name ("decompose") acts as the program name and `--help` is
//! auto-generated per variant.

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring decompose",
    bin_name = "touring decompose",
    about = "DAG task decomposition for multi-step workflows",
    disable_help_subcommand = true
)]
struct DecomposeCli {
    #[command(subcommand)]
    cmd: Option<DecomposeCmd>,
}

#[derive(Subcommand, Debug)]
enum DecomposeCmd {
    /// Create a new task DAG container.
    Create {
        /// Task type (general, refactor, feature, plan, ...).
        task_type: Option<String>,
        /// Free-form description (remaining positional args, joined).
        description: Vec<String>,
        /// Origin marker for bidirectional flow (entries with origin !=
        /// claude-code surface in the digest for CC to adopt).
        #[arg(long, default_value = "touring-cli")]
        origin: String,
        /// Priority bucket (high|normal|low).
        #[arg(long, default_value = "normal")]
        priority: String,
        /// CILA complexity level 0-4. When omitted the daemon defaults to 3
        /// and derives the task split factor itself (key presence in the
        /// payload toggles explicit-vs-derived on the daemon side).
        #[arg(long)]
        cila_level: Option<i64>,
    },
    /// Add a subtask to an existing task DAG.
    Add {
        task_id: String,
        subtask_id: String,
        /// Subtask description (remaining positional args, joined).
        description: Vec<String>,
        /// Priority bucket.
        #[arg(long, default_value = "normal")]
        priority: String,
        /// Optional deadline (ISO date).
        #[arg(long)]
        deadline: Option<String>,
        /// Parallel execution group id.
        #[arg(long)]
        parallel_group: Option<String>,
        /// Comma-separated dependency list (e.g. --depends-on=S-01,S-02).
        #[arg(long, value_delimiter = ',')]
        depends_on: Option<Vec<String>>,
    },
    /// Fetch one task DAG by id.
    Get { task_id: String },
    /// Update a subtask (status, deps, priority, quality score).
    Update {
        task_id: String,
        subtask_id: String,
        /// Legacy positional status (deprecated; prefer `--status=<value>`).
        #[arg(hide = true)]
        legacy_status: Option<String>,
        /// New status (pending|in_progress|completed|failed).
        #[arg(long)]
        status: Option<String>,
        /// Comma-separated dependency list.
        #[arg(long, value_delimiter = ',')]
        depends_on: Option<Vec<String>>,
        /// Numeric priority.
        #[arg(long)]
        priority: Option<i64>,
        /// Quality score 0.0-1.0.
        #[arg(long)]
        quality_score: Option<f64>,
    },
    /// Validate a task DAG (cycles, dangling deps).
    Validate { task_id: String },
    /// Global decompose status (default when no subcommand is given).
    Status,
    /// Archive a completed task.
    Finalize {
        task_id: String,
        /// Optional minimum quality threshold.
        quality_threshold: Option<f64>,
    },
    /// List subtasks ready to execute (all deps satisfied).
    Ready {
        /// Optional task id filter.
        task_id: Option<String>,
        /// Sort output by priority (high → low).
        #[arg(long)]
        by_priority: bool,
    },
    /// List the 10 reusable workflow templates W1-W10 (TR-5). Emits JSON
    /// `{count, templates: [...]}` — consumed by touring-web `/plans`.
    Templates,
    /// Show one workflow template in detail (canonical step sequence). Emits JSON.
    Template { id: String },
}

/// Parse a comma-separated dependency list, trimming whitespace and filtering
/// empty tokens. Still used by the legacy comma-deps path; the flag path goes
/// through clap `value_delimiter` + [`normalize_deps`].
fn parse_csv_deps(csv: &str) -> Vec<String> {
    csv.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Trim + drop empty tokens from a clap-parsed `--depends-on` list so that
/// trailing commas (`"S-01,"`) and spaced CSVs (`"S-01, S-02"`) normalize the
/// same way the legacy parser did.
fn normalize_deps(raw: Vec<String>) -> Vec<String> {
    raw.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Backwards-compat (deprecated since 2026-05-02): when no `--depends-on` was
/// given AND the last positional token is purely a deps-shaped CSV
/// (`S-NN`/`sub_*`/`task_*`), treat it as the dependency list, pop it from the
/// description, and emit a deprecation warning.
fn take_legacy_comma_deps(desc_parts: &mut Vec<String>) -> Vec<String> {
    let Some(last) = desc_parts.last() else {
        return Vec::new();
    };
    let looks_like_deps = last.contains(',')
        && last.split(',').all(|t| {
            let t = t.trim();
            !t.is_empty()
                && (t.starts_with("S-") || t.starts_with("sub_") || t.starts_with("task_"))
        });
    if !looks_like_deps {
        return Vec::new();
    }
    eprintln!("warning: legacy comma-deps detected; use --depends-on=<csv> instead");
    let deps = parse_csv_deps(last);
    desc_parts.pop();
    deps
}

/// Entry point for the `touring decompose` CLI handler — parses the argv slice
/// into a `DecomposeCmd` and dispatches each subcommand
/// (`create`/`add`/`get`/`update`/`validate`/`status`/`finalize`/`ready`/`templates`/`template`)
/// to the corresponding `cli-decompose-*` daemon hook, printing the JSON
/// response. Defaults to `status` when no subcommand is given.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    // args = [<binary>, "decompose", ...] — skip the binary name so clap sees
    // "decompose" as the program name and args[2] as its subcommand. Passing
    // the full slice would make clap try to match "decompose" itself as a
    // DecomposeCmd variant (spec Q7 resolution, 2026-06-11).
    let cli = match DecomposeCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        // --help/--version arrive as clap "errors"; e.exit() prints to the
        // right stream and exits 0 (help) or 2 (real parse error) — matching
        // clap's canonical CLI behavior.
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(DecomposeCmd::Status) {
        DecomposeCmd::Create {
            task_type,
            description,
            origin,
            priority,
            cila_level,
        } => {
            let mut payload = serde_json::json!({
                "task_type": task_type.unwrap_or_else(|| "general".to_string()),
                "description": description.join(" "),
                "origin": origin,
                "priority": priority,
            });
            // Only include cila_level when explicitly given: the daemon checks
            // key presence to distinguish explicit level from derived default.
            if let Some(c) = cila_level {
                payload["cila_level"] = serde_json::json!(c);
            }
            let output = daemon_query("cli-decompose-create", payload)?;
            println!("{output}");
        }
        DecomposeCmd::Add {
            task_id,
            subtask_id,
            mut description,
            priority,
            deadline,
            parallel_group,
            depends_on,
        } => {
            let mut depends_on = depends_on.map(normalize_deps).unwrap_or_default();
            if depends_on.is_empty() {
                depends_on = take_legacy_comma_deps(&mut description);
            }
            let mut payload = serde_json::json!({
                "task_id": task_id,
                "subtask_id": subtask_id,
                "description": description.join(" "),
                "depends_on": depends_on,
                "priority": priority,
            });
            if let Some(dl) = deadline {
                payload["deadline"] = serde_json::json!(dl);
            }
            if let Some(pg) = parallel_group {
                payload["parallel_group"] = serde_json::json!(pg);
            }
            let output = daemon_query("cli-decompose-add", payload)?;
            println!("{output}");
        }
        DecomposeCmd::Get { task_id } => {
            let payload = serde_json::json!({ "task_id": task_id });
            let output = daemon_query("cli-decompose-get", payload)?;
            println!("{output}");
        }
        DecomposeCmd::Update {
            task_id,
            subtask_id,
            legacy_status,
            status,
            depends_on,
            priority,
            quality_score,
        } => {
            let mut payload = serde_json::json!({
                "task_id": task_id,
                "subtask_id": subtask_id,
            });
            if let Some(s) = status {
                payload["status"] = serde_json::json!(s);
            } else if let Some(s) = legacy_status {
                eprintln!("warning: positional <status> is deprecated; use --status=<value>");
                payload["status"] = serde_json::json!(s);
            } else {
                // Legacy contract: the payload always carries a status key.
                payload["status"] = serde_json::json!("");
            }
            if let Some(deps) = depends_on {
                payload["depends_on"] = serde_json::json!(normalize_deps(deps));
            }
            if let Some(p) = priority {
                payload["priority"] = serde_json::json!(p);
            }
            if let Some(q) = quality_score {
                payload["quality_score"] = serde_json::json!(q);
            }
            let output = daemon_query("cli-decompose-update", payload)?;
            println!("{output}");
        }
        DecomposeCmd::Validate { task_id } => {
            let payload = serde_json::json!({ "task_id": task_id });
            let output = daemon_query("cli-decompose-validate", payload)?;
            println!("{output}");
        }
        DecomposeCmd::Status => {
            let output = daemon_query("cli-decompose-status", serde_json::json!({}))?;
            println!("{output}");
        }
        DecomposeCmd::Finalize {
            task_id,
            quality_threshold,
        } => {
            let payload = serde_json::json!({
                "task_id": task_id,
                "quality_threshold": quality_threshold,
            });
            let output = daemon_query("cli-decompose-finalize", payload)?;
            println!("{output}");
        }
        DecomposeCmd::Ready {
            task_id,
            by_priority,
        } => {
            let payload = serde_json::json!({
                "task_id": task_id.unwrap_or_default(),
                "by_priority": by_priority,
            });
            let output = daemon_query("cli-decompose-ready", payload)?;
            println!("{output}");
        }
        DecomposeCmd::Templates => {
            // TR-5 (Rn2 §6/§11): list the 10 reusable workflow templates W1-W10.
            // Wires the `touring_hooks::workflow_templates` catalog to a CLI consumer.
            // JSON output (CLI-wide default contract) — consumed by touring-web /plans.
            let payload = templates_payload();
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        DecomposeCmd::Template { id } => {
            // TR-5: show one workflow template in detail (canonical step sequence).
            match touring_hooks::workflow_templates::template(&id) {
                Some(t) => {
                    println!("{}", serde_json::to_string_pretty(t)?);
                }
                None => {
                    let ids: Vec<&str> = touring_hooks::workflow_templates::all_templates()
                        .iter()
                        .map(|t| t.id)
                        .collect();
                    anyhow::bail!(
                        "Unknown workflow template: '{}'. Available: {}",
                        id,
                        ids.join(", ")
                    );
                }
            }
        }
    }
    Ok(())
}

/// Build the JSON payload for `decompose templates`: the full W1-W10
/// workflow-template catalog wrapped as `{count, templates: [...]}` so both
/// array-shaped and object-shaped consumers (`extract_template_array` in
/// touring-web) can parse it.
fn templates_payload() -> serde_json::Value {
    let templates = touring_hooks::workflow_templates::all_templates();
    serde_json::json!({
        "count": templates.len(),
        "templates": templates,
    })
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    DecomposeCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> DecomposeCli {
        DecomposeCli::try_parse_from(args).expect("args should parse")
    }

    // ── Pattern A — parsing-only (fast, no daemon) ─────────────────────────

    #[test]
    fn templates_payload_is_json_with_full_w1_w10_catalog() {
        let payload = templates_payload();
        assert_eq!(payload["count"], 10);
        let templates = payload["templates"]
            .as_array()
            .expect("templates must be a JSON array");
        assert_eq!(templates.len(), 10);
        // Every entry carries the fields the touring-web /plans page renders.
        for t in templates {
            assert!(t["id"].as_str().is_some_and(|s| s.starts_with('W')));
            assert!(t["name"].as_str().is_some_and(|s| !s.is_empty()));
            assert!(t["description"].as_str().is_some());
            assert!(t["patterns"].is_array());
            assert!(!t["steps"].as_array().expect("steps array").is_empty());
            assert!(t["pitfall"].as_str().is_some());
        }
        // Steps serialize with their full shape (order/action/tool_or_cmd/note).
        let step = &templates[0]["steps"][0];
        assert!(step["order"].as_u64().is_some());
        assert!(step["action"].as_str().is_some());
        assert!(step["tool_or_cmd"].as_str().is_some());
        assert!(step["note"].is_string());
    }

    #[test]
    fn single_template_serializes_to_json_object() {
        let t = touring_hooks::workflow_templates::template("W1").expect("W1 exists");
        let json = serde_json::to_value(t).expect("WorkflowTemplate serializes");
        assert_eq!(json["id"], "W1");
        assert!(json["steps"].is_array());
    }

    #[test]
    fn create_parses_origin_priority_cila_and_description() {
        let cli = parse(&[
            "decompose",
            "create",
            "refactor",
            "Implement",
            "foo",
            "--origin=cc",
            "--priority=high",
            "--cila-level=4",
        ]);
        let Some(DecomposeCmd::Create {
            task_type,
            description,
            origin,
            priority,
            cila_level,
        }) = cli.cmd
        else {
            panic!("expected Create");
        };
        assert_eq!(task_type.as_deref(), Some("refactor"));
        assert_eq!(description.join(" "), "Implement foo");
        assert_eq!(origin, "cc");
        assert_eq!(priority, "high");
        assert_eq!(cila_level, Some(4));
    }

    #[test]
    fn create_defaults_match_legacy_behavior() {
        let cli = parse(&["decompose", "create"]);
        let Some(DecomposeCmd::Create {
            task_type,
            description,
            origin,
            priority,
            cila_level,
        }) = cli.cmd
        else {
            panic!("expected Create");
        };
        assert_eq!(task_type, None); // run() maps None -> "general"
        assert!(description.is_empty());
        assert_eq!(origin, "touring-cli");
        assert_eq!(priority, "normal");
        assert_eq!(cila_level, None); // key omitted from payload -> daemon derives
    }

    #[test]
    fn bare_decompose_defaults_to_status() {
        let cli = parse(&["decompose"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Status
    }

    // ── Pattern C — regressions against the 4 root-cause bugs (2026-05-02) ─

    /// ISSUE-2: description commas must not leak into deps.
    #[test]
    fn add_description_with_comma_does_not_leak_into_deps() {
        let cli = parse(&[
            "decompose",
            "add",
            "task_1",
            "S-03",
            "Implement",
            "foo,",
            "bar",
        ]);
        let Some(DecomposeCmd::Add {
            mut description,
            depends_on,
            ..
        }) = cli.cmd
        else {
            panic!("expected Add");
        };
        assert!(depends_on.is_none());
        // "bar" is the last positional and is not deps-shaped — heuristic must not fire.
        let legacy = take_legacy_comma_deps(&mut description);
        assert!(legacy.is_empty());
        assert_eq!(description.join(" "), "Implement foo, bar");
    }

    /// ISSUE-3: a single dependency must survive (legacy predicate required a comma).
    #[test]
    fn add_single_dep_is_preserved() {
        let cli = parse(&[
            "decompose",
            "add",
            "t",
            "S-02",
            "title",
            "--depends-on=S-01",
        ]);
        let Some(DecomposeCmd::Add { depends_on, .. }) = cli.cmd else {
            panic!("expected Add");
        };
        assert_eq!(
            normalize_deps(depends_on.expect("flag given")),
            vec!["S-01"]
        );
    }

    /// ISSUE-5: trailing commas and spaces are normalized away.
    #[test]
    fn add_trailing_comma_and_spaces_filtered() {
        let cli = parse(&[
            "decompose",
            "add",
            "t",
            "S-02",
            "title",
            "--depends-on=S-01, S-02,",
        ]);
        let Some(DecomposeCmd::Add { depends_on, .. }) = cli.cmd else {
            panic!("expected Add");
        };
        assert_eq!(
            normalize_deps(depends_on.expect("flag given")),
            vec!["S-01", "S-02"]
        );
    }

    /// ISSUE-4: `update --depends-on` must be parsed (was a silent no-op).
    #[test]
    fn update_depends_on_flag_is_parsed() {
        let cli = parse(&[
            "decompose",
            "update",
            "task_1",
            "S-01",
            "--depends-on=S-00,S-A",
        ]);
        let Some(DecomposeCmd::Update { depends_on, .. }) = cli.cmd else {
            panic!("expected Update");
        };
        assert_eq!(
            normalize_deps(depends_on.expect("flag given")),
            vec!["S-00", "S-A"]
        );
    }

    // ── Pattern B — backwards-compat ───────────────────────────────────────

    #[test]
    fn update_accepts_legacy_positional_status() {
        let cli = parse(&["decompose", "update", "task_1", "S-01", "completed"]);
        let Some(DecomposeCmd::Update {
            legacy_status,
            status,
            ..
        }) = cli.cmd
        else {
            panic!("expected Update");
        };
        assert_eq!(legacy_status.as_deref(), Some("completed"));
        assert_eq!(status, None);
    }

    #[test]
    fn add_depends_on_space_separated_value_form() {
        let cli = parse(&[
            "decompose",
            "add",
            "t",
            "S-02",
            "title",
            "--depends-on",
            "S-01,S-02",
        ]);
        let Some(DecomposeCmd::Add { depends_on, .. }) = cli.cmd else {
            panic!("expected Add");
        };
        assert_eq!(
            normalize_deps(depends_on.expect("flag given")),
            vec!["S-01", "S-02"]
        );
    }

    #[test]
    fn add_legacy_comma_deps_in_last_positional_with_warning() {
        let mut desc = vec![
            "Implement".to_string(),
            "foo".to_string(),
            "S-01,S-02".to_string(),
        ];
        let deps = take_legacy_comma_deps(&mut desc);
        assert_eq!(deps, vec!["S-01", "S-02"]);
        assert_eq!(desc.join(" "), "Implement foo");
    }

    #[test]
    fn ready_accepts_task_id_and_by_priority_in_any_order() {
        for args in [
            &["decompose", "ready", "task_9", "--by-priority"][..],
            &["decompose", "ready", "--by-priority", "task_9"][..],
        ] {
            let cli = parse(args);
            let Some(DecomposeCmd::Ready {
                task_id,
                by_priority,
            }) = cli.cmd
            else {
                panic!("expected Ready");
            };
            assert_eq!(task_id.as_deref(), Some("task_9"));
            assert!(by_priority);
        }
    }

    #[test]
    fn finalize_parses_optional_quality_threshold() {
        let cli = parse(&["decompose", "finalize", "task_1", "0.85"]);
        let Some(DecomposeCmd::Finalize {
            task_id,
            quality_threshold,
        }) = cli.cmd
        else {
            panic!("expected Finalize");
        };
        assert_eq!(task_id, "task_1");
        assert_eq!(quality_threshold, Some(0.85));
    }

    // ── New failure modes the migration makes LOUD ─────────────────────────

    /// Gotcha 2026-06-11: `update <task>::<sub> --status X` used to bind
    /// subtask_id="--status" and reach the daemon as a silent no-op
    /// (updated:false). With clap, the missing positional is a parse error.
    #[test]
    fn update_missing_subtask_id_is_a_parse_error_not_silent_noop() {
        let res = DecomposeCli::try_parse_from([
            "decompose",
            "update",
            "task_1::E-W1",
            "--status",
            "completed",
        ]);
        assert!(res.is_err());
    }

    /// Typoed flags used to fall silently into the description bucket.
    #[test]
    fn typo_flag_is_a_parse_error_not_description_text() {
        let res =
            DecomposeCli::try_parse_from(["decompose", "update", "t", "s", "--statsu=completed"]);
        assert!(res.is_err());
    }

    /// `--cila-level` used to be swallowed into the description; now it is a
    /// real typed flag wired to the daemon's explicit-vs-derived logic.
    #[test]
    fn create_cila_level_is_typed_not_description_text() {
        let res = DecomposeCli::try_parse_from(["decompose", "create", "--cila-level=abc"]);
        assert!(res.is_err()); // non-numeric is rejected at parse time
    }
}
