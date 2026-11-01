//! The shared, read-only walk over Claude Code transcripts behind the `context_cost` (B5) and
//! `surface_usage` (B3) KPI sections (01/11/2026).
//!
//! `context_budget` measures BYTES per hook family; bytes are not cost, because the prefix is
//! cached and a cache read bills a fraction of a fresh token. These two sections read the same
//! transcripts (`~/.claude/projects/<slug>/*.jsonl`) for what bytes cannot say: the tokens the
//! provider billed (`usage`) and what the harness announced against what the model used.
//!
//! One walk serves both, so a `touring kpi` call parses each transcript once. The walk is
//! bounded exactly as `context_budget` bounds its own: newest first, a transcript cap and a byte
//! cap, and BOTH are reported — a partial scan must never read as a complete one.
//!
//! Nothing here keeps transcript text. The parsers return names and byte counts.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// How much of a project's history one scan may read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanCaps {
    /// Transcripts to read, most recent first.
    pub max_transcripts: usize,
    /// Bytes to read across them before stopping and saying so.
    pub max_bytes: u64,
}

/// The caps `touring kpi` uses — the ones `context_budget` uses, so the three sections describe
/// the same window of history.
pub const KPI_CAPS: ScanCaps = ScanCaps {
    max_transcripts: 3,
    max_bytes: 48 * 1024 * 1024,
};

/// The caps of the SHADOW sections (`family_gate`, `lesson_lifecycle`): they count over many
/// sessions — a fact is confirmed by distinct sessions, a follow ratio needs n — so three
/// transcripts say nothing. Wider than [`KPI_CAPS`], still bounded so a `touring kpi` call stays
/// cheap, and still REPORTED (`capped`, `skipped_by_cap`): a partial walk reads as partial. The
/// full-history numbers come from the `shadow_probe` example, which lifts the caps.
pub const SHADOW_CAPS: ScanCaps = ScanCaps {
    max_transcripts: 24,
    max_bytes: 128 * 1024 * 1024,
};

/// One transcript (one Claude Code session — its file name is the session id) after the walk.
#[derive(Debug)]
pub struct SessionScan<T> {
    /// The session id (the transcript's file stem).
    pub id: String,
    /// Bytes of transcript read.
    pub bytes: u64,
    /// What the observer folded out of the records.
    pub state: T,
    /// Lines that were not JSON: counted, never hidden.
    pub malformed_lines: u64,
}

/// The result of one walk.
#[derive(Debug)]
pub struct Scan<T> {
    /// The transcripts read, newest first.
    pub sessions: Vec<SessionScan<T>>,
    /// Bytes read across all of them.
    pub bytes_scanned: u64,
    /// The walk stopped at `max_bytes`: this is a PARTIAL measurement.
    pub capped: bool,
    /// Transcripts that exist but were not read because of `max_transcripts`.
    pub skipped_by_cap: usize,
}

/// Transcript files of `dir`, newest first.
fn transcripts_newest_first(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|_| format!("no transcript directory at {}", dir.display()))?;
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|p| {
            let mtime = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((mtime, p))
        })
        .collect();
    files.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));
    Ok(files.into_iter().map(|(_, p)| p).collect())
}

/// Walk the transcripts of `dir` newest first, handing every parsed record of a session to
/// `observe` together with that session's own state.
///
/// # Errors
///
/// A directory that cannot be listed is an error naming it — absence is displayed, never a
/// silent zero.
pub fn scan_dir<T: Default>(
    dir: &Path,
    caps: &ScanCaps,
    observe: &mut dyn FnMut(&mut T, &Value),
) -> Result<Scan<T>, String> {
    let files = transcripts_newest_first(dir)?;
    let mut scan = Scan {
        sessions: Vec::new(),
        bytes_scanned: 0,
        capped: false,
        skipped_by_cap: files.len().saturating_sub(caps.max_transcripts),
    };
    for path in files.into_iter().take(caps.max_transcripts) {
        if scan.bytes_scanned >= caps.max_bytes {
            scan.capped = true;
            break;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut state = T::default();
        let mut malformed_lines = 0;
        for line in content.lines() {
            match serde_json::from_str::<Value>(line) {
                Ok(rec) => observe(&mut state, &rec),
                Err(_) => malformed_lines += 1,
            }
        }
        scan.bytes_scanned += content.len() as u64;
        scan.sessions.push(SessionScan {
            id: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            bytes: content.len() as u64,
            state,
            malformed_lines,
        });
    }
    Ok(scan)
}

/// Which announcement surface an attachment carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnnKind {
    /// `skill_listing` — the skills the model may invoke.
    Skill,
    /// `agent_listing_delta` — the agent types the model may spawn.
    Agent,
    /// `deferred_tools_delta` — the tools whose schemas load on demand (names only).
    Tool,
    /// `mcp_instructions_delta` — the instructions each MCP server contributes.
    Mcp,
}

impl AnnKind {
    /// The key the payload publishes this surface under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill_listing",
            Self::Agent => "agent_listing",
            Self::Tool => "deferred_tools",
            Self::Mcp => "mcp_instructions",
        }
    }
}

/// One announcement: what it cost in bytes, in total and per announced item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    /// The surface announced.
    pub kind: AnnKind,
    /// Bytes the announcement put in the window (items the parser could not name included).
    pub total_bytes: u64,
    /// Bytes per announced item (skill, agent type, tool name, MCP server).
    pub items: BTreeMap<String, u64>,
}

/// The item a listing line announces: the LONGEST announced name that prefixes the line (after
/// an optional `- `) and is followed by `:` or the end of the line. A plugin skill is
/// `plugin:skill`, so cutting at the first colon would bill the bytes to a name nobody listed.
fn announced_name<'a>(line: &'a str, names: &BTreeSet<String>) -> Option<&'a str> {
    let body = line.strip_prefix("- ").unwrap_or(line);
    body.match_indices(':')
        .map(|(i, _)| i)
        .chain(std::iter::once(body.len()))
        .rev()
        .map(|end| &body[..end])
        .find(|candidate| names.contains(*candidate))
}

/// Bytes per announced name over listing `lines` (each line also costs its newline).
#[must_use]
pub fn attribute_lines<'a>(
    lines: impl Iterator<Item = &'a str>,
    names: &BTreeSet<String>,
) -> BTreeMap<String, u64> {
    let mut out: BTreeMap<String, u64> = BTreeMap::new();
    for line in lines {
        if let Some(name) = announced_name(line, names) {
            *out.entry(name.to_string()).or_default() += line.len() as u64 + 1;
        }
    }
    out
}

fn string_list(att: &Value, key: &str) -> Vec<String> {
    att.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A listing attachment: `names_key` holds the announced names and `lines_key` the lines.
fn listed(att: &Value, kind: AnnKind, names_key: &str, lines_key: &str) -> Announcement {
    let names: BTreeSet<String> = string_list(att, names_key).into_iter().collect();
    let lines = string_list(att, lines_key);
    Announcement {
        kind,
        total_bytes: lines.iter().map(|l| l.len() as u64 + 1).sum(),
        items: attribute_lines(lines.iter().map(String::as_str), &names),
    }
}

fn skill_announcement(att: &Value) -> Announcement {
    let names: BTreeSet<String> = string_list(att, "names").into_iter().collect();
    let content = att.get("content").and_then(Value::as_str).unwrap_or("");
    Announcement {
        kind: AnnKind::Skill,
        total_bytes: content.len() as u64,
        items: attribute_lines(content.lines(), &names),
    }
}

fn mcp_announcement(att: &Value) -> Announcement {
    let names = string_list(att, "addedNames");
    let blocks = string_list(att, "addedBlocks");
    let mut items = BTreeMap::new();
    if names.len() == blocks.len() {
        for (name, block) in names.into_iter().zip(&blocks) {
            *items.entry(name).or_default() += block.len() as u64;
        }
    }
    Announcement {
        kind: AnnKind::Mcp,
        total_bytes: blocks.iter().map(|b| b.len() as u64).sum(),
        items,
    }
}

/// The announcement an `attachment` record carries, or `None` for any other attachment.
#[must_use]
pub fn parse_announcement(att: &Value) -> Option<Announcement> {
    match att.get("type").and_then(Value::as_str)? {
        "skill_listing" => Some(skill_announcement(att)),
        "agent_listing_delta" => Some(listed(att, AnnKind::Agent, "addedTypes", "addedLines")),
        "deferred_tools_delta" => Some(listed(att, AnnKind::Tool, "addedNames", "addedLines")),
        "mcp_instructions_delta" => Some(mcp_announcement(att)),
        _ => None,
    }
}

const COMMAND_OPEN: &str = "<command-name>";
const COMMAND_CLOSE: &str = "</command-name>";

/// The slash commands a user text carries. Claude Code records a typed `/name` as
/// `<command-name>/name</command-name>` inside a user string; the slash is not part of the name.
#[must_use]
pub fn typed_commands(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(COMMAND_OPEN) {
        let after = &rest[start + COMMAND_OPEN.len()..];
        let Some(end) = after.find(COMMAND_CLOSE) else {
            break;
        };
        let name = after[..end].trim().trim_start_matches('/');
        if !name.is_empty() {
            out.push(name.to_string());
        }
        rest = &after[end + COMMAND_CLOSE.len()..];
    }
    out
}

/// One `tool_use` block of an assistant record.
#[derive(Debug, Clone, Copy)]
pub struct ToolCall<'a> {
    /// The block's id (`None` in a record that does not carry one).
    pub id: Option<&'a str>,
    /// The tool's name.
    pub name: &'a str,
    /// The call's arguments (`null` when the block carries none).
    pub input: &'a Value,
}

static NO_INPUT: Value = Value::Null;

/// The tool calls of an assistant record.
pub fn tool_calls(rec: &Value) -> impl Iterator<Item = ToolCall<'_>> {
    rec.pointer("/message/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter_map(|b| {
            Some(ToolCall {
                id: b.get("id").and_then(Value::as_str),
                name: b.get("name")?.as_str()?,
                input: b.get("input").unwrap_or(&NO_INPUT),
            })
        })
}

/// A record written by a subagent in the main transcript: another window, not this one.
#[must_use]
pub fn is_sidechain(rec: &Value) -> bool {
    rec.get("isSidechain").and_then(Value::as_bool) == Some(true)
}

/// Synthetic transcript records shared by the B5/B3 tests.
#[cfg(test)]
pub(crate) mod fixtures {
    use serde_json::{Value, json};

    /// An assistant record with a usage block: `usage = [input, cache_creation, cache_read, output]`.
    pub fn assistant(id: &str, model: &str, usage: [u64; 4], blocks: Value) -> String {
        assistant_cc(id, model, usage, None, blocks)
    }

    /// As [`assistant`], with the `cache_creation` split (`ephemeral_1h` tokens).
    pub fn assistant_cc(
        id: &str,
        model: &str,
        usage: [u64; 4],
        one_hour: Option<u64>,
        blocks: Value,
    ) -> String {
        let mut u = json!({
            "input_tokens": usage[0],
            "cache_creation_input_tokens": usage[1],
            "cache_read_input_tokens": usage[2],
            "output_tokens": usage[3],
        });
        if let Some(h) = one_hour {
            u["cache_creation"] = json!({
                "ephemeral_1h_input_tokens": h,
                "ephemeral_5m_input_tokens": usage[1] - h,
            });
        }
        json!({"type": "assistant", "message": {"id": id, "model": model, "usage": u, "content": blocks}})
            .to_string()
    }

    /// A text block.
    pub fn text(t: &str) -> Value {
        json!([{"type": "text", "text": t}])
    }

    /// A `tool_use` block. The id is derived from the call, as a real one is unique to it: the
    /// same call repeated across the records of one message keeps its id, a different call
    /// never shares one.
    pub fn tool_use(name: &str, input: Value) -> Value {
        json!([{"type": "tool_use", "id": format!("tu-{name}-{input}"), "name": name, "input": input}])
    }

    /// An assistant record that calls one tool under an explicit id (the id a result points at).
    pub fn assistant_call(id: &str, name: &str, input: Value) -> String {
        json!({"type": "assistant", "message": {"content": [
            {"type": "tool_use", "id": id, "name": name, "input": input}
        ]}})
        .to_string()
    }

    /// A user record carrying the result of the call `id`; `content` is a string or a block list.
    pub fn call_result(id: &str, is_error: bool, content: Value) -> String {
        json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": id, "is_error": is_error, "content": content}
        ]}})
        .to_string()
    }

    /// The same record with a top-level `timestamp` (what dates a session).
    pub fn stamped(line: &str, timestamp: &str) -> String {
        let mut rec: Value = serde_json::from_str(line).unwrap_or(Value::Null);
        rec["timestamp"] = json!(timestamp);
        rec.to_string()
    }

    /// A human user turn (string content).
    pub fn user_text(t: &str) -> String {
        json!({"type": "user", "message": {"content": t}}).to_string()
    }

    /// A user record carrying one tool result.
    pub fn user_result(content: &str) -> String {
        json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": "tu", "content": content}
        ]}})
        .to_string()
    }

    /// A hook injection.
    pub fn hook(text: &str) -> String {
        let inner = json!({"hookSpecificOutput": {"additionalContext": text}}).to_string();
        json!({"type": "attachment", "attachment": {"type": "hook_success", "stdout": inner}})
            .to_string()
    }

    /// A `skill_listing` attachment.
    pub fn skill_listing(names: &[&str], content: &str) -> String {
        json!({"type": "attachment", "attachment": {
            "type": "skill_listing", "names": names, "content": content,
            "skillCount": names.len(), "isInitial": true}})
        .to_string()
    }

    /// An `agent_listing_delta` attachment.
    pub fn agent_listing(types: &[&str], lines: &[&str]) -> String {
        json!({"type": "attachment", "attachment": {
            "type": "agent_listing_delta", "addedTypes": types, "addedLines": lines}})
        .to_string()
    }

    /// A `deferred_tools_delta` attachment (each line is the bare tool name).
    pub fn deferred_tools(names: &[&str]) -> String {
        json!({"type": "attachment", "attachment": {
            "type": "deferred_tools_delta", "addedNames": names, "addedLines": names}})
        .to_string()
    }

    /// An `mcp_instructions_delta` attachment.
    pub fn mcp_instructions(names: &[&str], blocks: &[&str]) -> String {
        json!({"type": "attachment", "attachment": {
            "type": "mcp_instructions_delta", "addedNames": names, "addedBlocks": blocks}})
        .to_string()
    }

    /// The system record Claude Code writes when it compacts.
    pub fn compact_boundary() -> String {
        json!({"type": "system", "subtype": "compact_boundary"}).to_string()
    }

    /// A `cost-state` record (Claude Code's own running total).
    pub fn cost_state(total: f64, models: Value) -> String {
        json!({"type": "cost-state", "totalCostUSD": total, "modelUsage": models}).to_string()
    }
}

#[cfg(test)]
#[path = "transcript_scan_tests.rs"]
mod tests;
