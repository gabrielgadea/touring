//! B3 (01/11/2026) — what the harness ANNOUNCES to the model against what was USED.
//!
//! Every session pays, in its window, for a list of the skills it may invoke, the agent types it
//! may spawn, the tools whose schemas load on demand and the instructions of each MCP server. An
//! offline replay over 250 transcripts found 327 skills announced and 7 used, 28 agents and 9, 265
//! deferred tools and 23 (`docs/plans/2026-11-01-tokeneconomizer-harness2/`). This section keeps
//! that measurement alive: per skill, agent type and deferred tool, announced × used, with the
//! announcement's bytes and its estimated tokens, and the unused items ranked by what they cost.
//!
//! # What counts as USED
//!
//! * the model called it: `Skill.input.skill`, `Agent.input.subagent_type` (absent → the
//!   general-purpose agent), or a tool by its own name;
//! * the person TYPED it: `<command-name>/name</command-name>` in a user record — the signal the
//!   offline replay could not see;
//! * the `invoked_skills` attachment lists it. That one also re-lists what a compaction carried
//!   over, so it marks a skill used but is never counted as another use.
//!
//! Subagent records inside the main transcript are another window and are skipped.
//!
//! # Report only
//!
//! The section proposes nothing and writes nothing: switching a skill or plugin off is a
//! decision of the person, item by item. The estimated tokens are `bytes × TOKENS_PER_BYTE` of
//! ONE announcement; what the announcement costs while it is carried in the window, call after
//! call, is `context_cost.attribution.announcement`.
//!
//! The payload carries names, counts and numbers — never the prose of an announcement or a
//! prompt.

use crate::cli::context_cost::{TOKENS_PER_BYTE, round_to};
use crate::cli::transcript_scan::{
    AnnKind, Announcement, is_sidechain, parse_announcement, tool_calls, typed_commands,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Unused items listed, biggest announcement first.
const TOP_UNUSED: usize = 15;
/// Used items listed, most used first.
const TOP_USED: usize = 10;

/// One surface (skills, agents or tools) as one session saw it.
#[derive(Debug, Default)]
struct Surface {
    /// Bytes announced per item, over every announcement of the session.
    announced: BTreeMap<String, u64>,
    /// Calls the model made, per item.
    used_tool: BTreeMap<String, u64>,
    /// Times the person typed the item.
    used_typed: BTreeMap<String, u64>,
    /// Items an `invoked_skills` attachment listed.
    used_attachment: BTreeSet<String>,
}

impl Surface {
    fn announce(&mut self, a: Announcement) {
        for (name, bytes) in a.items {
            *self.announced.entry(name).or_default() += bytes;
        }
    }

    /// Every item the session used by any signal.
    fn used_names(&self) -> BTreeSet<&str> {
        self.used_tool
            .keys()
            .chain(self.used_typed.keys())
            .map(String::as_str)
            .chain(self.used_attachment.iter().map(String::as_str))
            .collect()
    }
}

/// Walks one session's records, keeping the announced and the used.
#[derive(Debug, Default)]
pub struct SurfaceTracker {
    skills: Surface,
    agents: Surface,
    tools: Surface,
    mcp_bytes: BTreeMap<String, u64>,
    /// Tool-use ids already counted: a block is counted once however many records repeat it.
    seen_tool_use: BTreeSet<String>,
}

/// The texts of a user record: its string content, or its text blocks.
fn user_texts(rec: &Value) -> Vec<&str> {
    match rec.pointer("/message/content") {
        Some(Value::String(s)) => vec![s.as_str()],
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect(),
        _ => Vec::new(),
    }
}

impl SurfaceTracker {
    /// Fold one transcript record into the session's surface.
    pub fn observe(&mut self, rec: &Value) {
        if is_sidechain(rec) {
            return;
        }
        match rec.get("type").and_then(Value::as_str) {
            Some("attachment") => self.observe_attachment(rec),
            Some("user") => self.observe_user(rec),
            Some("assistant") => self.observe_assistant(rec),
            _ => {}
        }
    }

    /// [`observe`](Self::observe) over raw JSONL lines; a line that is not JSON is skipped.
    #[cfg(test)]
    pub fn observe_lines<'a>(&mut self, lines: impl Iterator<Item = &'a str>) {
        for line in lines {
            if let Ok(rec) = serde_json::from_str::<Value>(line) {
                self.observe(&rec);
            }
        }
    }

    fn observe_attachment(&mut self, rec: &Value) {
        let Some(att) = rec.get("attachment") else {
            return;
        };
        if let Some(a) = parse_announcement(att) {
            match a.kind {
                AnnKind::Skill => self.skills.announce(a),
                AnnKind::Agent => self.agents.announce(a),
                AnnKind::Tool => self.tools.announce(a),
                AnnKind::Mcp => {
                    for (server, bytes) in a.items {
                        *self.mcp_bytes.entry(server).or_default() += bytes;
                    }
                }
            }
        } else if att.get("type").and_then(Value::as_str) == Some("invoked_skills") {
            let names = att
                .get("skills")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|s| s.get("name").and_then(Value::as_str));
            self.skills
                .used_attachment
                .extend(names.map(str::to_string));
        }
    }

    fn observe_user(&mut self, rec: &Value) {
        for text in user_texts(rec) {
            // A typed command opens the record; a prompt that merely quotes the tag does not.
            if !text.trim_start().starts_with("<command-") {
                continue;
            }
            for name in typed_commands(text) {
                *self.skills.used_typed.entry(name).or_default() += 1;
            }
        }
    }

    fn observe_assistant(&mut self, rec: &Value) {
        for call in tool_calls(rec) {
            let fresh = call
                .id
                .is_none_or(|id| self.seen_tool_use.insert(id.to_string()));
            if fresh {
                self.count_call(call.name, call.input);
            }
        }
    }

    fn count_call(&mut self, name: &str, input: &Value) {
        *self.tools.used_tool.entry(name.to_string()).or_default() += 1;
        let arg = |key: &str| input.get(key).and_then(Value::as_str).map(str::to_string);
        match name {
            "Skill" => {
                if let Some(skill) = arg("skill") {
                    *self.skills.used_tool.entry(skill).or_default() += 1;
                }
            }
            "Agent" => {
                let kind = arg("subagent_type").unwrap_or_else(|| "general-purpose".to_string());
                *self.agents.used_tool.entry(kind).or_default() += 1;
            }
            _ => {}
        }
    }
}

/// One item across sessions.
#[derive(Debug, Default)]
struct ItemAgg {
    sessions_announced: u64,
    announce_bytes: u64,
    uses_tool: u64,
    uses_typed: u64,
    sessions_used: u64,
    attachment_sessions: u64,
}

impl ItemAgg {
    fn uses(&self) -> u64 {
        self.uses_tool + self.uses_typed
    }

    fn is_used(&self) -> bool {
        self.sessions_used > 0
    }

    fn tokens_est(&self) -> f64 {
        self.announce_bytes as f64 * TOKENS_PER_BYTE
    }
}

fn aggregate(
    sessions: &[&SurfaceTracker],
    pick: fn(&SurfaceTracker) -> &Surface,
) -> BTreeMap<String, ItemAgg> {
    let mut items: BTreeMap<String, ItemAgg> = BTreeMap::new();
    for s in sessions {
        let surface = pick(s);
        for (name, bytes) in &surface.announced {
            let e = items.entry(name.clone()).or_default();
            e.sessions_announced += 1;
            e.announce_bytes += bytes;
        }
        for (name, n) in &surface.used_tool {
            items.entry(name.clone()).or_default().uses_tool += n;
        }
        for (name, n) in &surface.used_typed {
            items.entry(name.clone()).or_default().uses_typed += n;
        }
        for name in &surface.used_attachment {
            items.entry(name.clone()).or_default().attachment_sessions += 1;
        }
        for name in surface.used_names() {
            items.entry(name.to_string()).or_default().sessions_used += 1;
        }
    }
    items
}

fn share(num: f64, den: f64) -> Value {
    if den > 0.0 {
        json!(round_to(num / den, 4))
    } else {
        Value::Null
    }
}

fn top_unused(items: &BTreeMap<String, ItemAgg>) -> Vec<Value> {
    let mut unused: Vec<(&String, &ItemAgg)> = items
        .iter()
        .filter(|(_, i)| i.sessions_announced > 0 && !i.is_used())
        .collect();
    unused.sort_by(|a, b| b.1.announce_bytes.cmp(&a.1.announce_bytes).then(a.0.cmp(b.0)));
    unused
        .into_iter()
        .take(TOP_UNUSED)
        .map(|(name, i)| {
            json!({
                "name": name,
                "sessions_announced": i.sessions_announced,
                "announce_bytes": i.announce_bytes,
                "announce_tokens_est": i.tokens_est(),
            })
        })
        .collect()
}

fn top_used(items: &BTreeMap<String, ItemAgg>) -> Vec<Value> {
    let mut used: Vec<(&String, &ItemAgg)> = items
        .iter()
        .filter(|(_, i)| i.sessions_announced > 0 && i.is_used())
        .collect();
    used.sort_by(|a, b| b.1.uses().cmp(&a.1.uses()).then(a.0.cmp(b.0)));
    used.into_iter()
        .take(TOP_USED)
        .map(|(name, i)| json!({"name": name, "uses": i.uses(), "sessions_used": i.sessions_used}))
        .collect()
}

/// The report of one surface across `sessions` sessions.
fn kind_report(items: &BTreeMap<String, ItemAgg>, sessions: usize) -> Value {
    let announced: Vec<&ItemAgg> = items.values().filter(|i| i.sessions_announced > 0).collect();
    let (used, unused): (Vec<&ItemAgg>, Vec<&ItemAgg>) =
        announced.iter().copied().partition(|i| i.is_used());
    let not_announced = items
        .values()
        .filter(|i| i.sessions_announced == 0 && i.is_used())
        .count();
    let bytes: u64 = announced.iter().map(|i| i.announce_bytes).sum();
    let unused_bytes: u64 = unused.iter().map(|i| i.announce_bytes).sum();
    let tokens = bytes as f64 * TOKENS_PER_BYTE;
    json!({
        "announced": announced.len(),
        "used": used.len(),
        "unused": unused.len(),
        "used_not_announced": not_announced,
        "unused_share": share(unused.len() as f64, announced.len() as f64),
        "announce_bytes": bytes,
        "announce_tokens_est": tokens,
        "announce_tokens_est_per_session": if sessions > 0 { json!(round_to(tokens / sessions as f64, 4)) } else { Value::Null },
        "unused_announce_bytes": unused_bytes,
        "unused_announce_tokens_est": unused_bytes as f64 * TOKENS_PER_BYTE,
        "unused_bytes_share": share(unused_bytes as f64, bytes as f64),
        "uses": {
            "tool": items.values().map(|i| i.uses_tool).sum::<u64>(),
            "typed": items.values().map(|i| i.uses_typed).sum::<u64>(),
        },
        "used_via_attachment": items.values().filter(|i| i.attachment_sessions > 0).count(),
        "top_unused": top_unused(items),
        "top_used": top_used(items),
    })
}

/// The `surface_usage` report over `sessions` (names, counts and numbers only).
#[must_use]
pub fn surface_report(sessions: &[&SurfaceTracker]) -> Value {
    let mcp: BTreeMap<&String, u64> = sessions.iter().fold(BTreeMap::new(), |mut acc, s| {
        for (server, bytes) in &s.mcp_bytes {
            *acc.entry(server).or_default() += bytes;
        }
        acc
    });
    let mcp_total: u64 = mcp.values().sum();
    let n = sessions.len();
    json!({
        "sessions": n,
        "skills": kind_report(&aggregate(sessions, |s| &s.skills), n),
        "agents": kind_report(&aggregate(sessions, |s| &s.agents), n),
        "tools": kind_report(&aggregate(sessions, |s| &s.tools), n),
        "mcp_instructions": {
            "servers": mcp.len(),
            "bytes": mcp_total,
            "tokens_est": mcp_total as f64 * TOKENS_PER_BYTE,
        },
        "constants": {"tokens_per_byte": TOKENS_PER_BYTE},
        "method": "announced: attachments skill_listing / agent_listing_delta / deferred_tools_delta (bytes per item, newline included); used: Skill / Agent / tool_use by name, a typed `/command`, or an `invoked_skills` attachment; tokens are ESTIMATED as bytes × tokens_per_byte of ONE announcement — the carried cost is context_cost.attribution.announcement",
    })
}

#[cfg(test)]
#[path = "surface_usage_tests.rs"]
mod tests;
