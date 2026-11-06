//! The transcript READER behind the `context_budget`, `context_cost` (B5), `surface_usage` (B3),
//! `family_gate` (B2) and `lesson_lifecycle` (B1) KPI sections (01/11/2026).
//!
//! `context_budget` measures BYTES per hook family; bytes are not cost, because the prefix is
//! cached and a cache read bills a fraction of a fresh token. The other sections read the same
//! transcripts (`~/.claude/projects/<slug>/*.jsonl`) for what bytes cannot say: the tokens the
//! provider billed (`usage`), what the harness announced against what the model used, how each
//! hook family was followed, and which failures were resolved.
//!
//! # The reader owns the record-kind policy
//!
//! A transcript record is a loose JSON object; what it MEANS (is this a person typing? is it a
//! subagent's window? is this the same API call repeated?) used to be decided again in every
//! consumer, and the consumers disagreed — five rulers read one corpus under five different
//! definitions of "a turn". Now the policy is decided HERE, once, and consumers receive typed
//! [`Event`]s and never a raw record:
//!
//! * **main thread only** — a record written by a subagent (`isSidechain`) is another window, not
//!   this one: it is dropped before any consumer sees it, and counted in
//!   [`ReadStats::sidechain_dropped`] so the drop is displayed, never hidden;
//! * **a human prompt** is a user record that is not `isMeta`, carries no `tool_result` block and
//!   whose text holds none of [`NON_HUMAN_TURN_MARKERS`]; string content and a text-only block
//!   array are both read (a bare string is the common shape: 893 of the 898 non-meta user
//!   records without a tool result, in 219 real transcripts);
//! * **usage is billed once per API call** — Claude Code writes one assistant record per content
//!   block and every record repeats the call's `usage`, so [`AssistantTurn::usage`] is `Some`
//!   only on the first record of a message id of the session;
//! * **a `tool_use` block counts once per id** — a call repeated across records is one call.
//!
//! The walk is bounded: newest first, a transcript cap and a byte cap, and BOTH are reported — a
//! partial scan must never read as a complete one. One walk serves every section, so a `touring
//! kpi` call parses each transcript once.
//!
//! Nothing here keeps transcript text beyond the event it is handed to. The parsers return names
//! and byte counts.

use serde_json::Value;
use std::borrow::Cow;
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
    /// What the observer folded out of the events.
    pub state: T,
    /// What the reader saw besides the events: malformed lines, dropped records, the start date.
    pub read: ReadStats,
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

/// Walk the transcripts of `dir` newest first, handing every [`Event`] of a session to `observe`
/// together with that session's own state.
///
/// # Errors
///
/// A directory that cannot be listed is an error naming it — absence is displayed, never a
/// silent zero.
pub fn scan_dir<T: Default>(
    dir: &Path,
    caps: &ScanCaps,
    observe: &mut dyn FnMut(&mut T, &Event<'_>),
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
        let read = read_lines(content.lines(), &mut |event| observe(&mut state, event));
        scan.bytes_scanned += content.len() as u64;
        scan.sessions.push(SessionScan {
            id: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            bytes: content.len() as u64,
            state,
            read,
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
fn attribute_lines<'a>(
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

/// Texts the HARNESS files as user records, not the person: the output of a slash command, its
/// caveat banner, and the summary injected when a context is compacted. They carry no
/// `tool_result`, so the obvious predicate ("a user record without tool results is a human turn")
/// counts them — and the human-turn count is the DENOMINATOR of the ruler's headline metric.
///
/// Measured 04/09/2026 over 5 real transcripts: **35 of 172** counted turns (20,3%) were one of
/// these — 13 caveats, 13 command outputs, 9 compaction summaries. The denominator was inflated
/// 25,5%, so `injected_bytes_per_turn` read ~25% LOWER than the truth. This list stops adding fake
/// turns; reading string content as a turn stopped dropping real ones. A denominator has two ways
/// to be wrong and both were live. `isMeta` closes a third: measured 01/11/2026 over 219
/// transcripts, 883 user records are `isMeta` — Claude Code's own expansions (a skill body, a
/// command's caveat) — and 862 of them passed the marker list and still counted as a turn.
///
/// A genuine turn may carry an appended `<system-reminder>`, so that is deliberately NOT a marker:
/// the predicate stays narrow, and a doubtful record counts as human (a larger denominator
/// understates the problem, which is the conservative direction for a cost metric).
///
/// `scripts/test_human_turn_predicate_parity.py` reads this array out of THIS source file with a
/// regex and demands the same list in the Python hook that cuts turn records
/// (`loop_turn_record.py`): keep the declaration in the `const NAME: [&str; N] = [ … ];` shape.
pub(crate) const NON_HUMAN_TURN_MARKERS: [&str; 4] = [
    "This session is being continued from a previous conversation",
    "<local-command-stdout>",
    "<local-command-caveat>",
    "Caveat: The messages below were generated by the user while running local commands",
];

/// Did a person write this text, or the harness? See [`NON_HUMAN_TURN_MARKERS`].
fn is_human_turn_text(text: &str) -> bool {
    !NON_HUMAN_TURN_MARKERS.iter().any(|m| text.contains(m))
}

/// `YYYY-MM-DD` when `ts` starts with one.
fn date_prefix(ts: &str) -> Option<&str> {
    let d = ts.get(..10)?;
    let shaped = d.bytes().enumerate().all(|(i, b)| {
        if i == 4 || i == 7 {
            b == b'-'
        } else {
            b.is_ascii_digit()
        }
    });
    shaped.then_some(d)
}

/// What the reader says about a record, after the policy of the module docs.
#[derive(Debug)]
pub enum Event<'a> {
    /// The text a user record carried (string content or its text blocks).
    User(UserTurn<'a>),
    /// One `tool_result` block of a user record.
    ToolResult(ToolResult<'a>),
    /// One assistant record: its blocks in order, and the call's usage when this record is the
    /// first of its message id.
    Assistant(AssistantTurn<'a>),
    /// The payload of an `attachment` record (the object under `attachment`). What the payload
    /// means — a hook injection, an announcement, an `invoked_skills` list — stays a domain
    /// concern of the consumer ([`parse_announcement`] reads the announcements).
    Attachment(&'a Value),
    /// The system record Claude Code writes when it compacts: the window starts over.
    CompactBoundary,
    /// Claude Code's own running cost total, the whole `cost-state` record.
    CostState(&'a Value),
}

/// The text of one user record.
#[derive(Debug)]
pub struct UserTurn<'a> {
    /// Every text of the record, in order. Bytes of these are window bytes whoever wrote them.
    pub texts: Vec<&'a str>,
    /// A PERSON wrote it: not `isMeta`, no `tool_result` block, no harness marker in any text.
    pub is_prompt: bool,
}

/// One `tool_result` block, flattened.
#[derive(Debug)]
pub struct ToolResult<'a> {
    /// The call this answers (`None` in a block that does not say).
    pub tool_use_id: Option<&'a str>,
    /// The call failed.
    pub is_error: bool,
    /// The result as text: a string as is; else the `text` blocks joined with nothing; else the
    /// JSON of the whole. One flattening for every consumer — it used to be three.
    pub text: Cow<'a, str>,
    /// The `sessionId` of the record that carried the block (`None` when it carries none).
    /// Provenance of the record, not of the result: a consumer that persists what it found
    /// (the miner's lessons) must say which session it came from.
    pub session_id: Option<&'a str>,
    /// The `timestamp` of that record, as written (`None` when it carries none).
    pub timestamp: Option<&'a str>,
}

/// One assistant record.
#[derive(Debug)]
pub struct AssistantTurn<'a> {
    /// The API call's usage, on the FIRST record of its message id in the session and `None` on
    /// every later record of it (each repeats the usage; summing records bills a call once per
    /// block). `None` also for a record with no message id.
    pub usage: Option<Usage<'a>>,
    /// The record's blocks in order. A `tool_use` already seen under its id is not repeated.
    pub blocks: Vec<Block<'a>>,
}

impl<'a> AssistantTurn<'a> {
    /// The tool calls of the record, in order.
    pub fn tool_calls(&self) -> impl Iterator<Item = ToolCall<'a>> + '_ {
        self.blocks.iter().filter_map(|b| match b {
            Block::ToolUse(call) => Some(*call),
            _ => None,
        })
    }
}

/// One content block of an assistant record (a bare-string content is one `Text`).
#[derive(Debug)]
pub enum Block<'a> {
    /// Prose.
    Text(&'a str),
    /// Extended thinking.
    Thinking(&'a str),
    /// A tool call.
    ToolUse(ToolCall<'a>),
}

/// One `tool_use` block.
#[derive(Debug, Clone, Copy)]
pub struct ToolCall<'a> {
    /// The block's id (`None` in a block that does not carry one).
    pub id: Option<&'a str>,
    /// The tool's name.
    pub name: &'a str,
    /// The call's arguments (`null` when the block carries none).
    pub input: &'a Value,
}

/// The `usage` of one API call, as the message states it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage<'a> {
    /// The model that answered.
    pub model: Option<&'a str>,
    /// Uncached input tokens.
    pub input_tokens: u64,
    /// Tokens written to the cache.
    pub cache_creation_tokens: u64,
    /// Tokens read from the cache.
    pub cache_read_tokens: u64,
    /// Output tokens (thinking included).
    pub output_tokens: u64,
    /// Of `cache_creation_tokens`, the part written with the 1-hour TTL.
    pub cache_creation_1h_tokens: u64,
    /// The speed tier (`fast`…) when the message names one.
    pub speed: Option<&'a str>,
}

impl<'a> Usage<'a> {
    fn of(msg: &'a Value) -> Self {
        let u = msg.get("usage").unwrap_or(&Value::Null);
        let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
        Self {
            model: msg.get("model").and_then(Value::as_str),
            input_tokens: n("input_tokens"),
            cache_creation_tokens: n("cache_creation_input_tokens"),
            cache_read_tokens: n("cache_read_input_tokens"),
            output_tokens: n("output_tokens"),
            cache_creation_1h_tokens: u
                .pointer("/cache_creation/ephemeral_1h_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            speed: u.get("speed").and_then(Value::as_str),
        }
    }
}

/// What the reader saw besides the events of one session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadStats {
    /// Lines that were not JSON: counted, never hidden.
    pub malformed_lines: u64,
    /// Subagent records dropped from the main thread: counted, never hidden.
    pub sidechain_dropped: u64,
    /// `YYYY-MM-DD` of the first main-thread record that carries a `timestamp` shaped like one;
    /// `None` when none does.
    pub started_on: Option<String>,
}

static NO_INPUT: Value = Value::Null;

/// A `tool_result` content as text: a string; else the `text` blocks joined with nothing; else the
/// JSON of the whole.
fn flatten_result(content: &Value) -> Cow<'_, str> {
    if let Some(s) = content.as_str() {
        return Cow::Borrowed(s);
    }
    if let Some(items) = content.as_array() {
        let mut parts = items
            .iter()
            .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|i| i.get("text").and_then(Value::as_str));
        if let Some(first) = parts.next() {
            return match parts.next() {
                None => Cow::Borrowed(first),
                Some(second) => Cow::Owned([first, second].into_iter().chain(parts).collect()),
            };
        }
    }
    Cow::Owned(content.to_string())
}

/// The state the policy needs across the records of one session.
#[derive(Default)]
struct Reader {
    stats: ReadStats,
    /// Message ids whose usage was already handed out.
    seen_messages: BTreeSet<String>,
    /// `tool_use` ids already handed out.
    seen_tool_uses: BTreeSet<String>,
}

/// One `tool_result` block, flattened.
fn tool_result_of(b: &Value) -> ToolResult<'_> {
    ToolResult {
        tool_use_id: b.get("tool_use_id").and_then(Value::as_str),
        is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
        text: b.get("content").map_or(Cow::Borrowed(""), flatten_result),
        // The block does not know its record; `read_user` stamps the provenance.
        session_id: None,
        timestamp: None,
    }
}

/// The texts and the tool results of a user record's `content` (a bare string is one text).
fn user_content(content: &Value) -> (Vec<&str>, Vec<ToolResult<'_>>) {
    let mut texts = Vec::new();
    let mut results = Vec::new();
    match content {
        Value::String(s) => texts.push(s.as_str()),
        Value::Array(blocks) => {
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("tool_result") => results.push(tool_result_of(b)),
                    Some("text") => texts.extend(b.get("text").and_then(Value::as_str)),
                    _ => {}
                }
            }
        }
        _ => {}
    }
    (texts, results)
}

impl Reader {
    /// Apply the policy to one record, handing the events it yields to `sink`.
    fn feed(&mut self, rec: &Value, sink: &mut dyn FnMut(&Event<'_>)) {
        if rec.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            self.stats.sidechain_dropped += 1;
            return;
        }
        self.note_start(rec);
        match rec.get("type").and_then(Value::as_str) {
            Some("system") => {
                if rec.get("subtype").and_then(Value::as_str) == Some("compact_boundary") {
                    sink(&Event::CompactBoundary);
                }
            }
            Some("attachment") => {
                if let Some(att) = rec.get("attachment") {
                    sink(&Event::Attachment(att));
                }
            }
            Some("user") => Self::read_user(rec, sink),
            Some("assistant") => self.read_assistant(rec, sink),
            Some("cost-state") => sink(&Event::CostState(rec)),
            _ => {}
        }
    }

    /// The first record that carries a dated `timestamp` dates the session.
    fn note_start(&mut self, rec: &Value) {
        if self.stats.started_on.is_none() {
            self.stats.started_on = rec
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(date_prefix)
                .map(str::to_string);
        }
    }

    fn read_user(rec: &Value, sink: &mut dyn FnMut(&Event<'_>)) {
        let Some(content) = rec.pointer("/message/content") else {
            return;
        };
        let (texts, results) = user_content(content);
        let carries_result = !results.is_empty();
        let session_id = rec.get("sessionId").and_then(Value::as_str);
        let timestamp = rec.get("timestamp").and_then(Value::as_str);
        for result in results {
            sink(&Event::ToolResult(ToolResult {
                session_id,
                timestamp,
                ..result
            }));
        }
        if texts.is_empty() {
            return;
        }
        let is_prompt = !carries_result
            && rec.get("isMeta").and_then(Value::as_bool) != Some(true)
            && texts.iter().all(|t| is_human_turn_text(t));
        sink(&Event::User(UserTurn { texts, is_prompt }));
    }

    fn read_assistant<'a>(&mut self, rec: &'a Value, sink: &mut dyn FnMut(&Event<'_>)) {
        let Some(msg) = rec.get("message") else {
            return;
        };
        let usage = msg
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| self.seen_messages.insert((*id).to_string()))
            .map(|_| Usage::of(msg));
        let mut blocks: Vec<Block<'a>> = Vec::new();
        match msg.get("content") {
            Some(Value::String(s)) => blocks.push(Block::Text(s)),
            Some(Value::Array(items)) => {
                blocks.extend(items.iter().filter_map(|b| self.block_of(b)));
            }
            _ => {}
        }
        sink(&Event::Assistant(AssistantTurn { usage, blocks }));
    }

    fn block_of<'a>(&mut self, b: &'a Value) -> Option<Block<'a>> {
        match b.get("type").and_then(Value::as_str)? {
            "text" => b.get("text").and_then(Value::as_str).map(Block::Text),
            "thinking" => b
                .get("thinking")
                .and_then(Value::as_str)
                .map(Block::Thinking),
            "tool_use" => {
                let name = b.get("name")?.as_str()?;
                let id = b.get("id").and_then(Value::as_str);
                if let Some(id) = id
                    && !self.seen_tool_uses.insert(id.to_string())
                {
                    return None;
                }
                Some(Block::ToolUse(ToolCall {
                    id,
                    name,
                    input: b.get("input").unwrap_or(&NO_INPUT),
                }))
            }
            _ => None,
        }
    }
}

/// The incremental form of [`read_lines`]: one session read a line at a time, so a consumer that
/// streams a very large transcript (single files reach hundreds of MB) never has to hold every
/// line at once. The policy state — message ids seen, `tool_use` ids seen — lives across
/// [`LineReader::push`] calls exactly as it does across the lines of one `read_lines` call.
#[derive(Default)]
pub struct LineReader {
    reader: Reader,
}

impl LineReader {
    /// A reader at the start of a session.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply the policy to one JSONL `line`, handing the events it yields to `sink`. A line that
    /// is not JSON is counted in [`ReadStats::malformed_lines`], never hidden.
    pub fn push(&mut self, line: &str, sink: &mut dyn FnMut(&Event<'_>)) {
        match serde_json::from_str::<Value>(line) {
            Ok(rec) => self.reader.feed(&rec, sink),
            Err(_) => self.reader.stats.malformed_lines += 1,
        }
    }

    /// What the reader saw besides the events, once the session is over.
    #[must_use]
    pub fn finish(self) -> ReadStats {
        self.reader.stats
    }
}

/// Read one session's JSONL `lines`, handing every [`Event`] to `sink` in order. This is the
/// single door from a transcript to a consumer: [`scan_dir`] reads a file through it, and a
/// consumer that already holds the lines does the same ([`LineReader`] is the same door for a
/// consumer that does not).
pub fn read_lines<'a>(
    lines: impl Iterator<Item = &'a str>,
    sink: &mut dyn FnMut(&Event<'_>),
) -> ReadStats {
    let mut reader = LineReader::new();
    for line in lines {
        reader.push(line, sink);
    }
    reader.finish()
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

    /// A user record whose `content` is the block list `blocks`.
    pub fn user_blocks(blocks: Value) -> String {
        json!({"type": "user", "message": {"content": blocks}}).to_string()
    }

    /// The same record flagged `isMeta` (Claude Code's own expansion, not the person's text).
    pub fn meta(line: &str) -> String {
        flagged(line, "isMeta")
    }

    /// The same record flagged `isSidechain` (written by a subagent).
    pub fn sidechain(line: &str) -> String {
        flagged(line, "isSidechain")
    }

    fn flagged(line: &str, flag: &str) -> String {
        let mut rec: Value = serde_json::from_str(line).unwrap_or(Value::Null);
        rec[flag] = json!(true);
        rec.to_string()
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
