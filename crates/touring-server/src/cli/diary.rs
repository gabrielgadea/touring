//! `touring diary write|read|list|meta|projects` — Agent Diary System CLI.
//!
//! Provides diary operations for agent memory persistence:
//! - `list`     — List all agents with diaries (default)
//! - `write`    — Write a diary entry for an agent
//! - `read`     — Read diary entries for an agent
//! - `meta`     — Show agent diary metadata
//! - `projects` — List projects with diary entries for an agent (W6)
//!
//! Wave P3-1.3 W3 (2026-06-11): migrated from manual positional / flag-loop
//! parsing to clap derive. All internal AgentDiary/MemoryStore logic unchanged.
//! WriteArgs parsing is replaced by clap fields; extract_flag is preserved for
//! backwards-compat with existing tests.
//!
//! # Palace Hierarchy Integration
//!
//! Entries are stored under the palace hierarchy:
//! - `wing_{agent}/diary/meta` — AgentMeta (JSON)
//! - `wing_{agent}/diary/entries/{timestamp}` — StructuredEntry (JSON)
//! - `wing_{agent}/diary/topics/{topic}` — Topic index
//! - `wing_{agent}/diary/projects/{project}/entries/{timestamp}` — Project-scoped entry (W6)
//! - `wing_{agent}/diary/projects/_index` — List of known projects (W6)

use crate::agent_diary::AgentDiary;
use crate::memory_store::MemoryStore;
use clap::{Parser, Subcommand};
use std::path::Path;

#[derive(Parser, Debug)]
#[command(
    name = "touring diary",
    bin_name = "touring diary",
    about = "Agent Diary System: write, read, list, meta, projects",
    disable_help_subcommand = true
)]
struct DiaryCli {
    #[command(subcommand)]
    cmd: Option<DiaryCmd>,
}

#[derive(Subcommand, Debug)]
enum DiaryCmd {
    /// List all agents with diaries (default).
    List,
    /// Write a diary entry for an agent.
    Write {
        /// Agent name.
        agent_name: String,
        /// Entry text words (remaining positional args, joined).
        entry_text: Vec<String>,
        /// Topic tag for the entry.
        #[arg(long)]
        topic: Option<String>,
        /// Project scope for the entry (W6).
        #[arg(long)]
        project: Option<String>,
        /// Task link (W6).
        #[arg(long)]
        task: Option<String>,
        /// Subtask link (W6).
        #[arg(long)]
        subtask: Option<String>,
        /// Mark as Actionable Architecture Knowledge (AAK) — prepends "#AAK".
        #[arg(long)]
        aaak: bool,
    },
    /// Read diary entries for an agent.
    Read {
        /// Agent name.
        agent_name: String,
        /// Number of most-recent entries to return.
        #[arg(long)]
        last: Option<usize>,
        /// Filter by topic.
        #[arg(long)]
        topic: Option<String>,
        /// Filter by project (W6).
        #[arg(long)]
        project: Option<String>,
        /// Filter by task id (W6).
        #[arg(long)]
        task: Option<String>,
    },
    /// Show metadata for an agent's diary.
    Meta {
        /// Agent name.
        agent_name: String,
    },
    /// List all projects with diary entries for an agent (W6).
    Projects {
        /// Agent name.
        agent_name: String,
    },
}

/// CLI entry point for `touring diary` subcommand.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match DiaryCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(DiaryCmd::List) {
        DiaryCmd::List => {
            let project_root = std::env::current_dir()?;
            let memory_store = init_memory_store(&project_root)?;

            let known_agents = [
                "claude",
                "scout",
                "orchestrator",
                "auditor",
                "architect",
                "engineer",
                "validator",
                "planner",
                "executor",
                "alice",
                "jordan",
                "gabriel",
                "kazuba",
                "zero",
                "kimi",
            ];

            let mut agents_with_diaries: Vec<String> = Vec::new();
            for agent in known_agents {
                let test_key = format!("wing_{}/diary/meta", agent);
                if memory_store.get(&test_key, "working")?.is_some() {
                    agents_with_diaries.push(agent.to_string());
                }
            }

            println!(
                "{}",
                serde_json::json!({
                    "count": agents_with_diaries.len(),
                    "agents": agents_with_diaries,
                    "note": "Only probes known patterns. Use 'touring diary meta <agent>' to check a specific agent."
                })
            );
        }
        DiaryCmd::Write {
            agent_name,
            entry_text,
            topic,
            project,
            task,
            subtask,
            aaak,
        } => {
            if agent_name.is_empty() {
                anyhow::bail!(
                    "Usage: touring diary write <agent> <entry> \
                     [--project <p>] [--task <id>] [--subtask <id>] [--topic <topic>] [--aaak]"
                );
            }
            let raw_entry = entry_text.join(" ");
            let entry_with_prefix = if aaak {
                format!("#AAK {}", raw_entry)
            } else {
                raw_entry.clone()
            };

            let project_root = std::env::current_dir()?;
            let memory_store = init_memory_store(&project_root)?;
            let mut diary = AgentDiary::new(&agent_name);
            diary.ensure_meta(&memory_store)?;

            let (timestamp, scoped) = if let Some(ref proj) = project {
                let ts = diary.diary_write_project(
                    &memory_store,
                    &entry_with_prefix,
                    proj,
                    task.as_deref(),
                    subtask.as_deref(),
                    topic.as_deref(),
                )?;
                (ts, true)
            } else {
                let ts = diary.diary_write(&memory_store, &entry_with_prefix, topic.as_deref())?;
                (ts, false)
            };

            println!(
                "{}",
                serde_json::json!({
                    "status": "written",
                    "agent": agent_name,
                    "timestamp": timestamp.to_rfc3339(),
                    "topic": topic,
                    "is_aaak": aaak,
                    "project": project,
                    "task_id": task,
                    "subtask_id": subtask,
                    "project_scoped": scoped,
                    "entry_len": raw_entry.len()
                })
            );
        }
        DiaryCmd::Read {
            agent_name,
            last,
            topic,
            project,
            task,
        } => {
            if agent_name.is_empty() {
                anyhow::bail!(
                    "Usage: touring diary read <agent> [--last N] [--topic <topic>] [--project <p>] [--task <id>]"
                );
            }
            let project_root = std::env::current_dir()?;
            let memory_store = init_memory_store(&project_root)?;
            let diary = AgentDiary::new(&agent_name);

            let entries = fetch_entries(
                &diary,
                &memory_store,
                project.as_deref(),
                task.as_deref(),
                topic.as_deref(),
                last,
            )?;

            let formatted: Vec<serde_json::Value> = entries
                .iter()
                .map(|e| {
                    let markers: Vec<(&str, &str)> = e
                        .markers
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect();

                    serde_json::json!({
                        "timestamp": e.timestamp.to_rfc3339(),
                        "content": e.content,
                        "is_aaak": e.is_aaak,
                        "markers": markers,
                        "phase": e.markers.get("P"),
                        "result": e.markers.get("R"),
                        "lesson": e.markers.get("L"),
                        "warning": e.markers.get("W"),
                        "error": e.markers.get("E"),
                    })
                })
                .collect();

            println!(
                "{}",
                serde_json::json!({
                    "agent": agent_name,
                    "count": entries.len(),
                    "topic_filter": topic,
                    "last_n": last,
                    "entries": formatted
                })
            );
        }
        DiaryCmd::Meta { agent_name } => {
            if agent_name.is_empty() {
                anyhow::bail!("Usage: touring diary meta <agent>");
            }
            let project_root = std::env::current_dir()?;
            let memory_store = init_memory_store(&project_root)?;
            let mut diary = AgentDiary::new(&agent_name);
            diary.load_meta(&memory_store)?;

            if let Some(meta) = diary.get_meta() {
                println!(
                    "{}",
                    serde_json::json!({
                        "agent": meta.agent_name,
                        "created_at": meta.created_at.to_rfc3339(),
                        "entry_count": meta.entry_count,
                        "last_entry_at": meta.last_entry_at.map(|dt| dt.to_rfc3339()),
                        "status": "ok",
                    })
                );
            } else {
                let entries = diary.diary_read(&memory_store, Some(1)).unwrap_or_default();
                if entries.is_empty() {
                    println!(
                        "{}",
                        serde_json::json!({
                            "agent": agent_name,
                            "status": "no_diary",
                            "message": "No diary found for this agent. Write the first entry with 'touring diary write'"
                        })
                    );
                } else {
                    println!(
                        "{}",
                        serde_json::json!({
                            "agent": agent_name,
                            "status": "ok",
                            "note": "Diary exists but meta not yet persisted",
                            "entry_count": entries.len(),
                        })
                    );
                }
            }
        }
        DiaryCmd::Projects { agent_name } => {
            if agent_name.is_empty() {
                anyhow::bail!("Usage: touring diary projects <agent>");
            }
            let project_root = std::env::current_dir()?;
            let memory_store = init_memory_store(&project_root)?;
            let diary = AgentDiary::new(&agent_name);
            let projects = diary
                .diary_list_projects(&memory_store)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!(
                "{}",
                serde_json::json!({
                    "agent": agent_name,
                    "count": projects.len(),
                    "projects": projects,
                })
            );
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Private helpers (unchanged from original)
// ─────────────────────────────────────────────────────────────────────────────

/// Select and fetch entries according to the active filter combination.
///
/// Priority: (project + task) > project-only > topic > flat (all).
fn fetch_entries(
    diary: &AgentDiary,
    memory_store: &crate::memory_store::MemoryStore,
    project: Option<&str>,
    task: Option<&str>,
    topic: Option<&str>,
    last_n: Option<usize>,
) -> anyhow::Result<Vec<crate::agent_diary::StructuredEntry>> {
    use crate::agent_diary::DiaryError;
    let map_err = |e: DiaryError| anyhow::anyhow!("{e}");

    if let (Some(proj), Some(tid)) = (project, task) {
        return diary
            .diary_read_by_task(memory_store, proj, tid)
            .map_err(map_err);
    }
    if let Some(proj) = project {
        return diary
            .diary_read_by_project(memory_store, proj, last_n)
            .map_err(map_err);
    }
    if let Some(t) = topic {
        return diary.diary_read_by_topic(memory_store, t).map_err(map_err);
    }
    diary.diary_read(memory_store, last_n).map_err(map_err)
}

/// Initialize MemoryStore for CLI use (direct file access, no daemon required).
fn init_memory_store(project_root: &Path) -> anyhow::Result<MemoryStore> {
    let memory_db = project_root
        .join(".claude")
        .join("touring")
        .join("memory.db");
    let semantic_db = project_root
        .join(".claude")
        .join("touring")
        .join("semantic.db");

    if !memory_db.exists() || !semantic_db.exists() {
        if let Some(parent) = memory_db.parent() {
            std::fs::create_dir_all(parent).ok();
        }
    }

    MemoryStore::new(&memory_db, &semantic_db)
        .map_err(|e| anyhow::anyhow!("MemoryStore init failed: {}. Note: Run 'touring session-start' first to initialize the memory database.", e))
}

/// Extract a flag value from args (e.g., `--topic <value>`).
/// Preserved for backwards-compat with existing tests.
/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    DiaryCli::command()
}

#[cfg(test)]
fn extract_flag(args: &[String], flag: &str) -> Option<String> {
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == flag {
            if let Some(value) = iter.peek() {
                if !value.starts_with("--") {
                    return Some((**value).clone());
                }
            }
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    fn parse(args: &[&str]) -> DiaryCli {
        DiaryCli::try_parse_from(args).expect("args should parse")
    }

    // ── Subcommand routing ──────────────────────────────────────────────

    #[test]
    fn bare_diary_defaults_to_list() {
        let cli = parse(&["diary"]);
        assert!(cli.cmd.is_none()); // run() maps None -> List
    }

    #[test]
    fn explicit_list_parses() {
        let cli = parse(&["diary", "list"]);
        assert!(matches!(cli.cmd, Some(DiaryCmd::List)));
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(DiaryCli::try_parse_from(["diary", "frobnicate"]).is_err());
    }

    // ── write ───────────────────────────────────────────────────────────

    #[test]
    fn write_parses_agent_and_entry() {
        let cli = parse(&["diary", "write", "claude", "some", "entry", "text"]);
        let Some(DiaryCmd::Write {
            agent_name,
            entry_text,
            aaak,
            ..
        }) = cli.cmd
        else {
            panic!("expected Write");
        };
        assert_eq!(agent_name, "claude");
        assert_eq!(entry_text.join(" "), "some entry text");
        assert!(!aaak);
    }

    #[test]
    fn write_aaak_flag() {
        let cli = parse(&["diary", "write", "claude", "entry", "--aaak"]);
        let Some(DiaryCmd::Write { aaak, .. }) = cli.cmd else {
            panic!("expected Write");
        };
        assert!(aaak);
    }

    #[test]
    fn write_with_topic() {
        let cli = parse(&["diary", "write", "claude", "text", "--topic", "refactor"]);
        let Some(DiaryCmd::Write { topic, .. }) = cli.cmd else {
            panic!("expected Write");
        };
        assert_eq!(topic.as_deref(), Some("refactor"));
    }

    #[test]
    fn write_with_project() {
        let cli = parse(&[
            "diary",
            "write",
            "claude",
            "entry",
            "--project",
            "konverter",
        ]);
        let Some(DiaryCmd::Write { project, .. }) = cli.cmd else {
            panic!("expected Write");
        };
        assert_eq!(project.as_deref(), Some("konverter"));
    }

    #[test]
    fn write_with_task_and_subtask() {
        let cli = parse(&[
            "diary",
            "write",
            "claude",
            "e",
            "--task",
            "T-01",
            "--subtask",
            "S-02",
        ]);
        let Some(DiaryCmd::Write { task, subtask, .. }) = cli.cmd else {
            panic!("expected Write");
        };
        assert_eq!(task.as_deref(), Some("T-01"));
        assert_eq!(subtask.as_deref(), Some("S-02"));
    }

    #[test]
    fn write_missing_agent_is_parse_error() {
        assert!(DiaryCli::try_parse_from(["diary", "write"]).is_err());
    }

    // ── read ────────────────────────────────────────────────────────────

    #[test]
    fn read_parses_agent() {
        let cli = parse(&["diary", "read", "claude"]);
        let Some(DiaryCmd::Read { agent_name, .. }) = cli.cmd else {
            panic!("expected Read");
        };
        assert_eq!(agent_name, "claude");
    }

    #[test]
    fn read_with_last() {
        let cli = parse(&["diary", "read", "claude", "--last", "10"]);
        let Some(DiaryCmd::Read { last, .. }) = cli.cmd else {
            panic!("expected Read");
        };
        assert_eq!(last, Some(10));
    }

    #[test]
    fn read_with_topic_filter() {
        let cli = parse(&["diary", "read", "claude", "--topic", "implement"]);
        let Some(DiaryCmd::Read { topic, .. }) = cli.cmd else {
            panic!("expected Read");
        };
        assert_eq!(topic.as_deref(), Some("implement"));
    }

    #[test]
    fn read_with_project_filter() {
        let cli = parse(&["diary", "read", "claude", "--project", "konverter"]);
        let Some(DiaryCmd::Read { project, .. }) = cli.cmd else {
            panic!("expected Read");
        };
        assert_eq!(project.as_deref(), Some("konverter"));
    }

    #[test]
    fn read_missing_agent_is_parse_error() {
        assert!(DiaryCli::try_parse_from(["diary", "read"]).is_err());
    }

    // ── meta ────────────────────────────────────────────────────────────

    #[test]
    fn meta_parses_agent() {
        let cli = parse(&["diary", "meta", "claude"]);
        let Some(DiaryCmd::Meta { agent_name }) = cli.cmd else {
            panic!("expected Meta");
        };
        assert_eq!(agent_name, "claude");
    }

    #[test]
    fn meta_missing_agent_is_parse_error() {
        assert!(DiaryCli::try_parse_from(["diary", "meta"]).is_err());
    }

    // ── projects ─────────────────────────────────────────────────────────

    #[test]
    fn projects_parses_agent() {
        let cli = parse(&["diary", "projects", "claude"]);
        let Some(DiaryCmd::Projects { agent_name }) = cli.cmd else {
            panic!("expected Projects");
        };
        assert_eq!(agent_name, "claude");
    }

    #[test]
    fn projects_missing_agent_is_parse_error() {
        assert!(DiaryCli::try_parse_from(["diary", "projects"]).is_err());
    }

    // ── extract_flag (legacy helper) ────────────────────────────────────

    #[test]
    fn extract_flag_finds_value() {
        let args = s(&[
            "touring", "diary", "write", "claude", "text", "--topic", "refactor",
        ]);
        assert_eq!(extract_flag(&args, "--topic"), Some("refactor".to_string()));
    }

    #[test]
    fn extract_flag_returns_none_when_absent() {
        let args = s(&["touring", "diary", "write", "claude", "text"]);
        assert_eq!(extract_flag(&args, "--topic"), None);
    }

    #[test]
    fn extract_flag_skips_flag_immediately_followed_by_flag() {
        // --topic followed by another flag (no value) should return None
        let args = s(&["touring", "diary", "write", "claude", "--topic", "--aaak"]);
        assert_eq!(extract_flag(&args, "--topic"), None);
    }
}
