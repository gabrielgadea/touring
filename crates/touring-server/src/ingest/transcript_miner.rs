//! Claude Code conversation transcript parser and miner — Phase 2 (Slices 2.1–2.3).
//!
//! # Full pipeline overview
//!
//! ```text
//! ~/.claude/projects/<slug>/<uuid>.jsonl
//!         │  (one JSON object per line, NDJSON)
//!         ▼
//!  parse_transcript_line()      [Slice 2.1 — pure, no I/O]
//!         │
//!         ▼  Vec<ParsedTranscriptLine>
//!  extract_error_resolution_pairs()  [Slice 2.2 — pure, no I/O]
//!         │
//!         ▼  Vec<ErrorResolutionPair>
//!  TranscriptMiner::sweep()     [Slice 2.3 — I/O, incremental, fail-open]
//!         │
//!         ▼  MemoryStore (tier="reference", key="outcome:<tool_class>:transcript-<hash>:failure")
//!  cli_suggester::retrieve_and_render_lessons()  [Phase 1 reader — already live]
//! ```
//!
//! # Slice 2.1 — Raw parsing layer
//! Parses a single line of a Claude Code `~/.claude/projects/.../<uuid>.jsonl`
//! transcript into typed Rust structures. Pure, fail-open, no I/O beyond the
//! `&str` argument passed in.
//!
//! # Slice 2.2 — Error→resolution state machine
//! [`extract_error_resolution_pairs`] walks an ordered `&[ParsedTranscriptLine]`
//! slice and mines (failed_action, successful_resolution) pairs. The algorithm
//! is purely transformational — no I/O, no storage, no daemon wiring (Slice
//! 2.3's responsibility).
//!
//! A *failed action* is a `ToolResult` with `is_error == true`. Its
//! *resolution* is the next `ToolUse` of the **same `tool_name`** (forward in
//! the stream, within [`RESOLUTION_SCAN_WINDOW`]) whose `ToolResult` exists
//! and has `is_error == false`. Pairs without an observed resolution are
//! silently dropped — only *witnessed* resolutions become lessons.
//!
//! [`dedup_key`] produces a stable string key consumed by Slice 2.3 to
//! de-duplicate against existing memory entries — not an orphan symbol (REGRA #0).
//!
//! # Slice 2.3 — Sweep + storage layer
//! [`TranscriptMiner`] discovers transcript JSONL files under
//! `~/.claude/projects/`, tracks per-file byte offsets (incremental reads only),
//! extracts error→resolution pairs, and persists them to [`MemoryStore`] with
//! key `outcome:<tool_class>:transcript-<hash>:failure` (tier `reference`).
//! These keys are consumed by `cli_suggester::collect_memory_lessons`, whose
//! `LIKE 'outcome:<tool_class>:%:failure'` query matches them — closing the
//! Phase 2 learning loop. The `<tool_class>` segment uses the shared
//! `classify_tool_class` so writer and reader agree exactly.
//!
//! Gotcha-DB write: the gotcha DB has no clean programmatic insert API exposed
//! from `knowledge_adapter.rs` (only read-path via `get_gotchas_for_file`).
//! Lessons are therefore persisted exclusively to `MemoryStore` tier=reference.
//! The `cli_suggester` retrieval path already reads `outcome:*` memory keys,
//! so the loop is closed without a separate gotcha-DB write.
//!
//! # Slice 2.4 — Reader path, in shadow first (C1)
//! The miner has its own parser; the transcript READER
//! (`touring_cli::cli::transcript_scan`, reached as `touring_hooks::transcript_scan`) owns the
//! record policy the KPI sections already use: subagent (`isSidechain`) records are another
//! window and are dropped, a `tool_use` is handed out once per id. [`ReaderPath`] derives the
//! SAME [`ErrorResolutionPair`]s from the reader's events — both extractors end in
//! [`mine_chains`], so only the records that feed it can differ. Because the miner WRITES
//! memory, the switch is `TOURING_MINER_READER` = `shadow` (default) | `on` | `off` ([`ReaderMode`]):
//! under `shadow` the legacy parser still writes while the reader path is only compared with it
//! (`reader_pairs`, `legacy_pairs`, `only_legacy`, `only_reader`, `reader_sidechain_dropped` in
//! [`MinerSweepStats`]); `on` hands the writer role to the reader path.
//!
//! # Fail-open contract
//! All public functions in this module are fail-open: they never panic on
//! external data, filesystem errors, or malformed JSON. Per-file errors in
//! [`TranscriptMiner::sweep`] are logged at `debug!` and skipped.
//!
//! # Offline / off-hot-path
//! The sweep is only called from the background task in `server/mod.rs`.
//! No sweep I/O happens on the hot path of any tool or hook handler.

use crate::memory_store::{MemoryEntry, MemoryStore};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use touring_hooks::action_signature::classify_tool_class;
use touring_hooks::gateway::sandbox_executor::redact_secrets;
use touring_hooks::transcript_scan::{AssistantTurn, Event, LineReader, ReadStats, ToolResult};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Role of the turn that produced a transcript line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptRole {
    /// The turn was authored by the user.
    User,
    /// The turn was authored by the assistant.
    Assistant,
}

/// A single content block extracted from a transcript turn.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentBlock {
    /// An assistant-side tool invocation (`type == "tool_use"`).
    ToolUse {
        /// Correlates with [`ContentBlock::ToolResult::tool_use_id`].
        id: String,
        /// e.g. `"Bash"`, `"Edit"`, `"Write"`.
        tool_name: String,
        /// Raw JSON input object; structure is tool-specific.
        input: Value,
    },
    /// A user-side tool result (`type == "tool_result"`).
    ToolResult {
        /// Matches the `id` of the originating [`ContentBlock::ToolUse`].
        tool_use_id: String,
        /// `true` when the tool reported an error; absent in JSON means `false`.
        is_error: bool,
        /// Flattened text content (string or array of `{type,text}` blocks).
        content_text: String,
    },
    /// Any block we do not model (text, thinking, image, …).
    Other,
}

/// A fully parsed line from a Claude Code conversation JSONL file.
#[derive(Debug, Clone)]
pub struct ParsedTranscriptLine {
    /// Role of the turn that produced this line.
    pub role: TranscriptRole,
    /// Value of the top-level `sessionId` field; empty string if absent.
    pub session_id: String,
    /// Value of the top-level `timestamp` field; empty string if absent.
    pub timestamp: String,
    /// Value of the top-level `uuid` field; empty string if absent.
    pub uuid: String,
    /// Parsed content blocks in document order.
    pub blocks: Vec<ContentBlock>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse one line of a Claude Code transcript JSONL file.
///
/// Returns `None` when:
/// - the line is empty or whitespace-only,
/// - the line is not valid JSON,
/// - the top-level `type` field is not `"user"` or `"assistant"`,
/// - `message.content` is absent or is not a JSON array.
///
/// Never panics on external input.
pub fn parse_transcript_line(line: &str) -> Option<ParsedTranscriptLine> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let root: Value = serde_json::from_str(trimmed).ok()?;

    let role = match root.get("type").and_then(|t| t.as_str())? {
        "user" => TranscriptRole::User,
        "assistant" => TranscriptRole::Assistant,
        _ => return None,
    };

    let content_arr = root
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())?;

    let blocks = content_arr.iter().map(|block| parse_block(block)).collect();

    let session_id = root
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let timestamp = root
        .get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let uuid = root
        .get("uuid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Some(ParsedTranscriptLine {
        role,
        session_id,
        timestamp,
        uuid,
        blocks,
    })
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Map one JSON block object to a [`ContentBlock`].
fn parse_block(block: &Value) -> ContentBlock {
    let block_type = match block.get("type").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => return ContentBlock::Other,
    };

    match block_type {
        "tool_use" => {
            let id = block
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tool_name = block
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let input = block.get("input").cloned().unwrap_or(Value::Null);
            ContentBlock::ToolUse {
                id,
                tool_name,
                input,
            }
        }
        "tool_result" => {
            let tool_use_id = block
                .get("tool_use_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_error = block
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let content_text = block
                .get("content")
                .map(|c| flatten_tool_result_content(c))
                .unwrap_or_default();
            ContentBlock::ToolResult {
                tool_use_id,
                is_error,
                content_text,
            }
        }
        _ => ContentBlock::Other,
    }
}

/// Flatten the `content` field of a `tool_result` block.
///
/// CC transcripts use two shapes:
/// - a plain JSON string,
/// - an array of `{"type": "text", "text": "..."}` objects.
///
/// Any other shape falls back to `serde_json::Value::to_string()`.
fn flatten_tool_result_content(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(arr) = v.as_array() {
        let parts: Vec<&str> = arr
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                    item.get("text").and_then(|t| t.as_str())
                } else {
                    None
                }
            })
            .collect();
        if !parts.is_empty() {
            return parts.join("");
        }
    }
    v.to_string()
}

// ---------------------------------------------------------------------------
// Slice 2.2 — Error→resolution state machine
// ---------------------------------------------------------------------------

/// Maximum number of characters kept in [`ErrorResolutionPair::error_text`].
/// Longer texts are truncated at a char boundary (no panic on multi-byte UTF-8).
pub const ERROR_TEXT_MAX: usize = 500;

/// How many subsequent same-tool `ToolUse`s to scan forward when looking for a
/// resolution. If no successful result is found within this window the failure
/// is silently dropped (unresolved failures are not actionable lessons).
pub const RESOLUTION_SCAN_WINDOW: usize = 3;

/// A mined (failed_action → successful_resolution) pair extracted from a CC
/// transcript stream.
///
/// All fields are derived from the raw transcript; no inference is applied.
/// Consumed by Slice 2.3 (storage layer) which de-duplicates against existing
/// gotchas using [`dedup_key`] before persisting.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorResolutionPair {
    /// e.g. `"Bash"`, `"Edit"`, `"Write"` — same tool_name for both sides.
    pub tool_name: String,
    /// Raw JSON `input` object of the **failed** `ToolUse`.
    pub failed_input: serde_json::Value,
    /// Content of the failed `ToolResult`, truncated to [`ERROR_TEXT_MAX`] chars.
    pub error_text: String,
    /// Raw JSON `input` object of the **resolution** `ToolUse`.
    pub resolution_input: serde_json::Value,
    /// `sessionId` of the line containing the failed `ToolResult`.
    pub session_id: String,
    /// `timestamp` of the line containing the failed `ToolResult`.
    pub timestamp: String,
}

// Internal index entries built during Pass 1.
struct ToolUseEntry {
    input: serde_json::Value,
    /// Position of the assistant line in the original `lines` slice.
    /// Read by [`build_indices`] to keep `uses_by_tool` lists in stream order.
    stream_pos: usize,
}

struct ToolResultEntry {
    is_error: bool,
    content_text: String,
    /// Copied from the owning `ParsedTranscriptLine`.
    session_id: String,
    timestamp: String,
}

/// Lookup indices built from a transcript: `tool_use_id → use`, `tool_use_id → result`,
/// `tool_name → ordered tool_use_ids`.
type TranscriptIndices = (
    HashMap<String, ToolUseEntry>,
    HashMap<String, ToolResultEntry>,
    HashMap<String, Vec<String>>,
);

/// Build lookup indices from a flat slice of transcript lines.
///
/// Returns:
/// - `uses`: `tool_use_id → ToolUseEntry`
/// - `results`: `tool_use_id → ToolResultEntry`
/// - `uses_by_tool`: `tool_name → Vec<tool_use_id>` ordered by stream position
fn build_indices(lines: &[ParsedTranscriptLine]) -> TranscriptIndices {
    let mut uses: HashMap<String, ToolUseEntry> = HashMap::new();
    let mut results: HashMap<String, ToolResultEntry> = HashMap::new();
    let mut uses_by_tool: HashMap<String, Vec<String>> = HashMap::new();

    for (pos, line) in lines.iter().enumerate() {
        for block in &line.blocks {
            match block {
                ContentBlock::ToolUse {
                    id,
                    tool_name,
                    input,
                } => {
                    uses.entry(id.clone()).or_insert_with(|| ToolUseEntry {
                        input: input.clone(),
                        stream_pos: pos,
                    });
                    uses_by_tool
                        .entry(tool_name.clone())
                        .or_default()
                        .push(id.clone());
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    is_error,
                    content_text,
                } => {
                    results
                        .entry(tool_use_id.clone())
                        .or_insert_with(|| ToolResultEntry {
                            is_error: *is_error,
                            content_text: content_text.clone(),
                            session_id: line.session_id.clone(),
                            timestamp: line.timestamp.clone(),
                        });
                }
                ContentBlock::Other => {}
            }
        }
    }

    // Ensure uses_by_tool lists are in stream order.
    for ids in uses_by_tool.values_mut() {
        ids.sort_by_key(|id| uses.get(id).map_or(0, |e| e.stream_pos));
    }

    (uses, results, uses_by_tool)
}

/// Truncate `s` to at most `max_chars` Unicode scalar values.
///
/// Never panics on multi-byte UTF-8 — truncation always lands on a char
/// boundary because we collect chars and re-join.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect()
    }
}

/// Mine error→resolution pairs from an ordered transcript line slice.
///
/// The algorithm is a two-pass pure transformation:
///
/// 1. **Index pass** — collect every `ToolUse` and `ToolResult` into lookup
///    maps keyed by `id` / `tool_use_id`, with per-tool ordered id lists.
/// 2. **Chain-scan pass** — for each tool, walk its `ToolUse` list in stream
///    order. Accumulate a contiguous run of failed attempts. When a success
///    is encountered, emit **one pair** (first error in the chain → success)
///    if and only if the chain length is ≤ [`RESOLUTION_SCAN_WINDOW`].
///    A ToolUse with no result yet, or any non-contiguous break, resets the
///    chain.
///
/// Emitting from the **first** failure in the chain captures the broadest
/// error context. The window guard prevents emitting pairs where too many
/// retries happened (low-signal lessons). Failures whose chain exceeds the
/// window, or that are never followed by a success, are silently dropped.
///
/// # Guarantees
/// - Pure: no I/O, no global state, no side effects.
/// - Fail-open: dangling references, empty slices, or malformed data yield an
///   empty `Vec`, never a panic.
pub fn extract_error_resolution_pairs(lines: &[ParsedTranscriptLine]) -> Vec<ErrorResolutionPair> {
    if lines.is_empty() {
        return Vec::new();
    }

    let (uses, results, uses_by_tool) = build_indices(lines);
    mine_chains(&uses, &results, &uses_by_tool)
}

/// The chain-scan pass of [`extract_error_resolution_pairs`], on already-built indices.
///
/// This is the ONE definition of what counts as an error, a resolution and a chain. Both
/// extractors end here — the legacy one through [`build_indices`], the reader path through
/// [`ReaderPath`] — so the two can differ only in WHICH records feed the indices (the reader's
/// policy), never in how a pair is formed.
fn mine_chains(
    uses: &HashMap<String, ToolUseEntry>,
    results: &HashMap<String, ToolResultEntry>,
    uses_by_tool: &HashMap<String, Vec<String>>,
) -> Vec<ErrorResolutionPair> {
    let mut pairs = Vec::new();

    for (tool_name, ids) in uses_by_tool {
        // `ids` is already sorted by stream_pos (guaranteed by build_indices).
        // Walk the ordered list accumulating a chain of consecutive failures.
        // `chain_start` holds (tool_use_id, ToolUseEntry, ToolResultEntry) of
        // the FIRST failure in the current error run.
        let mut chain_start: Option<(&str, &ToolUseEntry, &ToolResultEntry)> = None;
        let mut chain_len: usize = 0;

        for id in ids {
            let use_entry = match uses.get(id) {
                Some(u) => u,
                None => {
                    // Dangling — reset chain.
                    chain_start = None;
                    chain_len = 0;
                    continue;
                }
            };
            let res_entry = match results.get(id) {
                Some(r) => r,
                None => {
                    // No result observed — chain break.
                    chain_start = None;
                    chain_len = 0;
                    continue;
                }
            };

            if res_entry.is_error {
                // Extend (or start) the error chain.
                if chain_start.is_none() {
                    chain_start = Some((id.as_str(), use_entry, res_entry));
                }
                chain_len += 1;
            } else {
                // Success: this is the resolution candidate.
                if let Some((_, first_use, first_result)) = chain_start
                    && chain_len <= RESOLUTION_SCAN_WINDOW
                {
                    pairs.push(ErrorResolutionPair {
                        tool_name: tool_name.clone(),
                        failed_input: first_use.input.clone(),
                        error_text: truncate_chars(&first_result.content_text, ERROR_TEXT_MAX),
                        resolution_input: use_entry.input.clone(),
                        session_id: first_result.session_id.clone(),
                        timestamp: first_result.timestamp.clone(),
                    });
                }
                // Success resets the chain regardless.
                chain_start = None;
                chain_len = 0;
            }
        }
    }

    pairs
}

/// Produce a stable de-duplication key for a mined pair.
///
/// Format: `"<tool_name>:<hash8>"` where `hash8` is the first 8 hex characters
/// of a `DefaultHasher` digest over `(tool_name, error_text[:200])`.
///
/// This key is consumed by **Slice 2.3** (storage layer) to de-duplicate
/// incoming pairs against the existing gotcha DB — satisfying REGRA #0
/// (not an orphan: it has a declared downstream consumer).
///
/// `DefaultHasher` is used because `blake3` is not a direct dependency of
/// `touring-server`. The key is advisory (de-dup hint), not a security hash,
/// so collision resistance of `DefaultHasher` is sufficient.
pub fn dedup_key(pair: &ErrorResolutionPair) -> String {
    let prefix: String = pair.error_text.chars().take(200).collect();
    let mut hasher = DefaultHasher::new();
    pair.tool_name.hash(&mut hasher);
    prefix.hash(&mut hasher);
    let digest = hasher.finish();
    format!("{}:{:08x}", pair.tool_name, digest as u32)
}

/// Memory key for a mined lesson — honors the Phase 1 retrieval contract.
///
/// `cli_suggester::collect_memory_lessons` queries the memory DB with
/// `key LIKE 'outcome:<tool_class>:%:failure'`. The mined key therefore MUST be
/// `outcome:<tool_class>:<discriminator>:failure`, where `<tool_class>` comes
/// from the shared [`classify_tool_class`] so writer and reader agree exactly.
/// `<discriminator>` is `transcript-<hash8>` ([`dedup_key`]'s hash segment) —
/// it marks the lesson as transcript-mined and keeps the key unique per
/// `(tool, error)`.
fn lesson_memory_key(pair: &ErrorResolutionPair) -> String {
    let tool_class = classify_tool_class(&pair.tool_name);
    let dk = dedup_key(pair);
    // dedup_key() == "<tool_name>:<hash8>" — the trailing segment is the hash.
    let hash = dk.rsplit(':').next().unwrap_or(dk.as_str());
    format!("outcome:{tool_class}:transcript-{hash}:failure")
}

/// SEC-05: build the JSON lesson value for a mined pair with secrets redacted.
///
/// CC transcripts can carry credentials in the failed-command error text and in
/// the resolution command input (e.g. `GH_TOKEN=…`, `AWS_SECRET_ACCESS_KEY=…`).
/// Both the `error` string and the stringified `resolution_input` are passed
/// through [`redact_secrets`] so no secret ever reaches the memory store.
/// `resolution_input` is stored as a redacted string (not a nested object)
/// because the reader (`cli_suggester`) renders the lesson as text anyway.
fn redacted_lesson_value(pair: &ErrorResolutionPair) -> serde_json::Value {
    serde_json::json!({
        "tool": pair.tool_name,
        "error": redact_secrets(&pair.error_text),
        "resolution_input": redact_secrets(&pair.resolution_input.to_string()),
        "session_id": pair.session_id,
        "timestamp": pair.timestamp,
    })
}

// ---------------------------------------------------------------------------
// Slice 2.4 — Reader path (C1, 04/11/2026), in SHADOW first
// ---------------------------------------------------------------------------

/// Environment variable that decides which extractor WRITES lessons: `off` | `shadow` | `on`.
/// Read once per process by [`reader_mode`].
pub const MINER_READER_ENV: &str = "TOURING_MINER_READER";

/// How many keys of each side's disagreement a [`ShadowComparison`] keeps as a sample. The
/// counts are never capped; only the sample is, so a divergence stays inspectable and a
/// pathological one stays cheap.
pub const SHADOW_SAMPLE_KEYS: usize = 5;

/// Which extractor writes lessons.
///
/// The miner is a PRODUCTION WRITER of memory, so the reader path (the transcript reader's
/// record policy: main thread only, one `tool_use` per id) is not allowed to write until it has
/// been measured against the legacy parser on the same lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReaderMode {
    /// Legacy parser only — the behaviour before C1. The reader path does not run.
    Off,
    /// Legacy parser writes, as before; the reader path is computed on the same lines, never
    /// writes, and the disagreement is counted. The default.
    #[default]
    Shadow,
    /// The reader path writes; the legacy parser does not run.
    On,
}

impl ReaderMode {
    /// Parse the value of [`MINER_READER_ENV`]. Absent, empty or unrecognised all give
    /// [`ReaderMode::Shadow`]; an unrecognised value also yields the warning to log. Never
    /// panics, never turns an unknown value into `on` — a typo must not hand the writer role
    /// to an unmeasured path.
    #[must_use]
    pub fn parse(raw: Option<&str>) -> (Self, Option<String>) {
        let Some(raw) = raw else {
            return (Self::Shadow, None);
        };
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "shadow" => (Self::Shadow, None),
            "off" => (Self::Off, None),
            "on" => (Self::On, None),
            other => (
                Self::Shadow,
                Some(format!(
                    "unknown {MINER_READER_ENV}={other:?} (expected off|shadow|on); using shadow"
                )),
            ),
        }
    }

    /// The name the variable takes for this mode.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Shadow => "shadow",
            Self::On => "on",
        }
    }
}

/// The mode of this process: [`MINER_READER_ENV`] read ONCE, then fixed. An unrecognised value
/// is warned about once and runs as shadow.
pub fn reader_mode() -> ReaderMode {
    static MODE: OnceLock<ReaderMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        let raw = std::env::var_os(MINER_READER_ENV).map(|v| v.to_string_lossy().into_owned());
        let (mode, warning) = ReaderMode::parse(raw.as_deref());
        if let Some(warning) = warning {
            tracing::warn!("TranscriptMiner: {warning}");
        }
        mode
    })
}

/// What the reader path found in one stretch of transcript lines.
#[derive(Debug, Clone, Default)]
pub struct ReaderExtraction {
    /// The pairs, formed by the same chain-scan as the legacy extractor ([`mine_chains`]).
    pub pairs: Vec<ErrorResolutionPair>,
    /// What the reader saw besides the events: malformed lines, dropped sidechain records.
    pub read: ReadStats,
}

/// One stretch of transcript fed through the reader a line at a time — a transcript file can be
/// hundreds of MB, so no line is kept — and folded into the same three indices
/// [`build_indices`] builds from the legacy parse.
///
/// Only the fold differs from the legacy path: a record is mined only if the reader hands it
/// out (a sidechain record is not), and a `tool_use` id is entered once (the reader hands it
/// out once, where legacy listed it again at every repetition and so inflated the retry chain
/// beyond [`RESOLUTION_SCAN_WINDOW`]). For an error result only the first [`ERROR_TEXT_MAX`]
/// characters are kept (all [`mine_chains`] reads), and a success result keeps no text.
struct ReaderPath {
    reader: LineReader,
    indices: ReaderIndices,
}

/// The three indices of [`build_indices`], folded from reader events.
#[derive(Default)]
struct ReaderIndices {
    uses: HashMap<String, ToolUseEntry>,
    results: HashMap<String, ToolResultEntry>,
    uses_by_tool: HashMap<String, Vec<String>>,
    /// Assistant records seen so far; ids enter `uses_by_tool` in this order already, so the
    /// stable sort [`build_indices`] applies has nothing to do here.
    next_pos: usize,
}

impl ReaderIndices {
    fn observe(&mut self, event: &Event<'_>) {
        match event {
            Event::Assistant(turn) => self.note_calls(turn),
            Event::ToolResult(result) => self.note_result(result),
            _ => {}
        }
    }

    fn note_calls(&mut self, turn: &AssistantTurn<'_>) {
        let pos = self.next_pos;
        self.next_pos += 1;
        for call in turn.tool_calls() {
            // A block without an id correlates under "", as it does in the legacy parse.
            let id = call.id.unwrap_or_default();
            self.uses
                .entry(id.to_string())
                .or_insert_with(|| ToolUseEntry {
                    input: call.input.clone(),
                    stream_pos: pos,
                });
            self.uses_by_tool
                .entry(call.name.to_string())
                .or_default()
                .push(id.to_string());
        }
    }

    fn note_result(&mut self, result: &ToolResult<'_>) {
        let id = result.tool_use_id.unwrap_or_default();
        self.results
            .entry(id.to_string())
            .or_insert_with(|| ToolResultEntry {
                is_error: result.is_error,
                content_text: if result.is_error {
                    truncate_chars(&result.text, ERROR_TEXT_MAX)
                } else {
                    String::new()
                },
                session_id: result.session_id.unwrap_or_default().to_string(),
                timestamp: result.timestamp.unwrap_or_default().to_string(),
            });
    }
}

impl ReaderPath {
    fn new() -> Self {
        Self {
            reader: LineReader::new(),
            indices: ReaderIndices::default(),
        }
    }

    /// Feed one transcript line (already trimmed, non-empty).
    fn push_line(&mut self, line: &str) {
        let Self { reader, indices } = self;
        reader.push(line, &mut |event| indices.observe(event));
    }

    fn finish(self) -> ReaderExtraction {
        let read = self.reader.finish();
        let ReaderIndices {
            uses,
            results,
            uses_by_tool,
            ..
        } = self.indices;
        ReaderExtraction {
            pairs: mine_chains(&uses, &results, &uses_by_tool),
            read,
        }
    }
}

/// How the two extractors disagree over the same lines, by [`dedup_key`] — the identity the
/// store writes under, so a difference here is a difference in what would be persisted.
///
/// The difference is a MULTISET one: 3 legacy pairs and 2 reader pairs under one key is one
/// `only_legacy`. Counts are exact; the key lists are capped samples ([`SHADOW_SAMPLE_KEYS`])
/// and carry keys only — never transcript content.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShadowComparison {
    /// Pairs the legacy extractor produced.
    pub legacy_pairs: usize,
    /// Pairs the reader path produced.
    pub reader_pairs: usize,
    /// Pairs (by key, with multiplicity) the legacy extractor produced and the reader did not.
    pub only_legacy: usize,
    /// Pairs the reader path produced and the legacy extractor did not.
    pub only_reader: usize,
    /// Records the reader dropped as subagent (`isSidechain`) windows.
    pub sidechain_dropped: u64,
    /// A sample of the keys counted in `only_legacy`, sorted.
    pub sample_only_legacy: Vec<String>,
    /// A sample of the keys counted in `only_reader`, sorted.
    pub sample_only_reader: Vec<String>,
}

impl ShadowComparison {
    /// Compare the legacy pairs of a stretch of lines with the reader's pairs of the same lines.
    fn between(legacy: &[ErrorResolutionPair], reader: &ReaderExtraction) -> Self {
        let mut balance: BTreeMap<String, i64> = BTreeMap::new();
        for pair in legacy {
            *balance.entry(dedup_key(pair)).or_default() += 1;
        }
        for pair in &reader.pairs {
            *balance.entry(dedup_key(pair)).or_default() -= 1;
        }
        let mut out = Self {
            legacy_pairs: legacy.len(),
            reader_pairs: reader.pairs.len(),
            sidechain_dropped: reader.read.sidechain_dropped,
            ..Self::default()
        };
        // BTreeMap iterates in key order, so the samples are deterministic.
        for (key, net) in balance {
            let n = usize::try_from(net.unsigned_abs()).unwrap_or(usize::MAX);
            match net.cmp(&0) {
                std::cmp::Ordering::Greater => {
                    out.only_legacy += n;
                    if out.sample_only_legacy.len() < SHADOW_SAMPLE_KEYS {
                        out.sample_only_legacy.push(key);
                    }
                }
                std::cmp::Ordering::Less => {
                    out.only_reader += n;
                    if out.sample_only_reader.len() < SHADOW_SAMPLE_KEYS {
                        out.sample_only_reader.push(key);
                    }
                }
                std::cmp::Ordering::Equal => {}
            }
        }
        out
    }

    /// Fold another stretch (another file) into this total. Samples keep the first
    /// [`SHADOW_SAMPLE_KEYS`] distinct keys seen.
    pub fn absorb(&mut self, other: Self) {
        self.legacy_pairs += other.legacy_pairs;
        self.reader_pairs += other.reader_pairs;
        self.only_legacy += other.only_legacy;
        self.only_reader += other.only_reader;
        self.sidechain_dropped += other.sidechain_dropped;
        for (mine, theirs) in [
            (&mut self.sample_only_legacy, other.sample_only_legacy),
            (&mut self.sample_only_reader, other.sample_only_reader),
        ] {
            for key in theirs {
                if mine.len() < SHADOW_SAMPLE_KEYS && !mine.contains(&key) {
                    mine.push(key);
                }
            }
        }
    }
}

/// The lines of one stretch of transcript, mined under a [`ReaderMode`]: each mode decides
/// which extractor sees the lines and which one's pairs are returned to be written.
struct ChunkMiner {
    mode: ReaderMode,
    /// The legacy parse, kept whole (as the miner always did). Empty under [`ReaderMode::On`].
    legacy: Vec<ParsedTranscriptLine>,
    /// The reader path. `None` under [`ReaderMode::Off`].
    reader: Option<ReaderPath>,
}

/// What a [`ChunkMiner`] hands back: the pairs to WRITE and how the two paths compared.
struct ChunkMined {
    /// The pairs of the extractor the mode lets write.
    pairs: Vec<ErrorResolutionPair>,
    /// Counts for the sweep stats. Under `off` only `legacy_pairs` is set, under `on` only
    /// `reader_pairs` (and the dropped-sidechain count): there is nothing to compare.
    comparison: ShadowComparison,
}

impl ChunkMiner {
    fn new(mode: ReaderMode) -> Self {
        Self {
            mode,
            legacy: Vec::new(),
            reader: (mode != ReaderMode::Off).then(ReaderPath::new),
        }
    }

    /// Feed one line (already trimmed, non-empty).
    fn push(&mut self, line: &str) {
        if self.mode != ReaderMode::On
            && let Some(parsed) = parse_transcript_line(line)
        {
            self.legacy.push(parsed);
        }
        if let Some(reader) = self.reader.as_mut() {
            reader.push_line(line);
        }
    }

    fn finish(self) -> ChunkMined {
        let reader = self.reader.map(ReaderPath::finish);
        match (self.mode, reader) {
            (ReaderMode::On, Some(reader)) => ChunkMined {
                comparison: ShadowComparison {
                    reader_pairs: reader.pairs.len(),
                    sidechain_dropped: reader.read.sidechain_dropped,
                    ..ShadowComparison::default()
                },
                pairs: reader.pairs,
            },
            (ReaderMode::Shadow, Some(reader)) => {
                let pairs = extract_error_resolution_pairs(&self.legacy);
                ChunkMined {
                    comparison: ShadowComparison::between(&pairs, &reader),
                    pairs,
                }
            }
            // `Off` — and `On`/`Shadow` without a reader, which `new` never builds.
            _ => {
                let pairs = extract_error_resolution_pairs(&self.legacy);
                ChunkMined {
                    comparison: ShadowComparison {
                        legacy_pairs: pairs.len(),
                        ..ShadowComparison::default()
                    },
                    pairs,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Slice 2.3 — Discovery, sweep, and storage
// ---------------------------------------------------------------------------

/// Discover all `<uuid>.jsonl` transcript files under `projects_root`.
///
/// Expected layout: `<projects_root>/<project-slug>/<uuid>.jsonl`
/// (one level of project-slug directory, then UUID-named JSONL files).
///
/// Returns an empty `Vec` when `projects_root` is unreadable or absent —
/// never panics on filesystem errors.
///
/// The returned list is sorted for deterministic processing order.
pub fn discover_transcript_paths(projects_root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let slug_entries = match std::fs::read_dir(projects_root) {
        Ok(e) => e,
        Err(_) => return paths,
    };

    for slug_entry in slug_entries.flatten() {
        let slug_path = slug_entry.path();
        if !slug_path.is_dir() {
            continue;
        }
        let jsonl_entries = match std::fs::read_dir(&slug_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for jsonl_entry in jsonl_entries.flatten() {
            let p = jsonl_entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                paths.push(p);
            }
        }
    }

    paths.sort();
    paths
}

/// Statistics from a single [`TranscriptMiner::sweep`] call.
///
/// The `reader_*`/`legacy_*`/`only_*` fields are the C1 shadow measurement (see [`ReaderMode`]);
/// they default to zero so a payload written before C1 still deserializes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinerSweepStats {
    /// Number of transcript files opened during this sweep.
    pub files_scanned: usize,
    /// Total new lines read across all files.
    pub lines_read: usize,
    /// Error→resolution pairs extracted from new lines by the extractor that WRITES
    /// (`reader_mode` says which).
    pub pairs_mined: usize,
    /// Pairs actually written to [`MemoryStore`] (new, not deduped).
    pub pairs_persisted: usize,
    /// Pairs skipped because the memory key already existed.
    pub pairs_deduped: usize,
    /// Which extractor wrote in this sweep: the legacy parser under `off` and `shadow`, the
    /// reader path under `on`.
    #[serde(default)]
    pub reader_mode: ReaderMode,
    /// Pairs the legacy extractor produced (0 under `on`, where it does not run).
    #[serde(default)]
    pub legacy_pairs: usize,
    /// Pairs the reader path produced (0 under `off`, where it does not run). Under `shadow`
    /// these were counted and never written.
    #[serde(default)]
    pub reader_pairs: usize,
    /// Shadow only: pairs (by [`dedup_key`], with multiplicity) the legacy extractor produced
    /// and the reader path did not — what `on` would stop writing.
    #[serde(default)]
    pub only_legacy: usize,
    /// Shadow only: pairs the reader path produced and the legacy extractor did not — what
    /// `on` would start writing.
    #[serde(default)]
    pub only_reader: usize,
    /// Records the reader dropped as subagent (`isSidechain`) windows (0 under `off`).
    #[serde(default)]
    pub reader_sidechain_dropped: u64,
}

/// Persisted per-file read offsets for incremental transcript sweeps.
///
/// Stored as JSON alongside the watcher state (sibling file). Loaded at
/// [`TranscriptMiner`] construction time; saved after every [`sweep`].
#[derive(Debug, Default, Serialize, Deserialize)]
struct MinerState {
    /// Map of absolute file path → byte offset of last read position.
    offsets: HashMap<String, u64>,
}

impl MinerState {
    fn load(path: &Path) -> Self {
        if path.exists() {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| std::io::Error::other(e))?;
        // Atomic: write to tmp then rename to avoid corrupt state on crash.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json)?;
        std::fs::rename(&tmp, path)
    }
}

/// Incremental CC-transcript miner.
///
/// On each `sweep` the miner:
/// 1. Discovers all `.jsonl` files under `projects_root` via
///    [`discover_transcript_paths`].
/// 2. For each file, seeks to the last known byte offset and reads only
///    new lines (offset-tracked, incremental).
/// 3. Parses lines via [`parse_transcript_line`] and extracts
///    error→resolution pairs via [`extract_error_resolution_pairs`] — or, per
///    [`ReaderMode`], through the transcript reader (`shadow` computes both and writes only the
///    legacy pairs; `on` writes the reader's).
/// 4. For each new pair, persists a `MemoryEntry` (tier `reference`,
///    key `outcome:<tool_class>:transcript-<hash>:failure` — see
///    `lesson_memory_key`) to the provided [`MemoryStore`] reference.
///
/// The miner is fully **fail-open**: per-file I/O errors are logged at
/// `debug!` and skipped. The miner never panics. It is **offline-only**:
/// it performs no network I/O and should only be called from the
/// background task in `server/mod.rs`.
pub struct TranscriptMiner {
    state: MinerState,
    state_path: PathBuf,
    reader_mode: ReaderMode,
}

/// What one file contributed to a sweep.
#[derive(Default)]
struct FileSweep {
    lines_read: usize,
    pairs_mined: usize,
    pairs_persisted: usize,
    pairs_deduped: usize,
    comparison: ShadowComparison,
}

/// The new lines of one transcript, mined.
struct NewLines {
    lines_read: usize,
    /// Byte offset after the last line read.
    new_offset: u64,
    mined: ChunkMined,
}

/// Mine the lines of `path` after byte `from` under `mode`, streaming — no line is kept past
/// its turn through the extractors. `Ok(None)` is "no new data". Writes nothing: this is the
/// read half of a sweep, and what the corpus measurement runs.
fn mine_new_lines(path: &Path, from: u64, mode: ReaderMode) -> Result<Option<NewLines>, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("open {:?}: {}", path.file_name().unwrap_or_default(), e))?;

    let file_len = file.metadata().map_err(|e| format!("stat: {}", e))?.len();
    if file_len <= from {
        return Ok(None);
    }

    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(from))
        .map_err(|e| format!("seek: {}", e))?;

    let mut line_buf = String::new();
    let mut miner = ChunkMiner::new(mode);
    let mut lines_read = 0usize;

    while reader
        .read_line(&mut line_buf)
        .map_err(|e| format!("read_line: {}", e))?
        > 0
    {
        let trimmed = line_buf.trim();
        if !trimmed.is_empty() {
            lines_read += 1;
            miner.push(trimmed);
        }
        line_buf.clear();
    }

    let new_offset = reader
        .stream_position()
        .map_err(|e| format!("stream_position: {}", e))?;
    Ok(Some(NewLines {
        lines_read,
        new_offset,
        mined: miner.finish(),
    }))
}

/// Persist `pairs` to `store`, returning (persisted, deduped).
fn persist_pairs(store: &MemoryStore, pairs: &[ErrorResolutionPair]) -> (usize, usize) {
    let mut pairs_persisted = 0usize;
    let mut pairs_deduped = 0usize;

    for pair in pairs {
        let key = lesson_memory_key(pair);

        // Check for existing key to avoid duplicate writes.
        // tier="reference" maps to MemoryTier::Reference (rlm.rs:66).
        // "semantic" is not a valid tier — it would silently fail via parse_tier Err.
        let exists = store.get(&key, "reference").unwrap_or(None).is_some();

        if exists {
            pairs_deduped += 1;
            continue;
        }

        // Build a compact JSON value for the stored lesson (SEC-05: secrets
        // in the mined transcript text are redacted before persistence).
        let value_str = redacted_lesson_value(pair).to_string();

        let entry =
            MemoryEntry::new(&key, "reference", value_str).with_entry_type("transcript_lesson");

        match store.store(entry) {
            Ok(()) => pairs_persisted += 1,
            Err(e) => {
                tracing::debug!("TranscriptMiner: failed to store lesson {}: {}", key, e);
            }
        }
    }

    (pairs_persisted, pairs_deduped)
}

impl TranscriptMiner {
    /// Construct a new miner, loading persisted offset state from
    /// `state_path` (or starting fresh if the file is absent/invalid). The extractor that
    /// writes is chosen by [`reader_mode`] (`TOURING_MINER_READER`, default `shadow`).
    pub fn new(state_path: PathBuf) -> Self {
        let mode = reader_mode();
        tracing::info!(
            "TranscriptMiner: transcript reader mode={} ({MINER_READER_ENV})",
            mode.as_str()
        );
        Self::with_reader_mode(state_path, mode)
    }

    /// As [`TranscriptMiner::new`], with the extractor mode given instead of read from the
    /// environment.
    pub fn with_reader_mode(state_path: PathBuf, reader_mode: ReaderMode) -> Self {
        let state = MinerState::load(&state_path);
        Self {
            state,
            state_path,
            reader_mode,
        }
    }

    /// Run one incremental sweep over all transcript files.
    ///
    /// `projects_root` is typically `~/.claude/projects`.
    /// `store` is the live [`MemoryStore`]; the call is synchronous
    /// (the tokio task wraps it in `spawn_blocking` or calls it directly
    /// from a non-async context as the daemon task is already on a
    /// dedicated thread).
    ///
    /// Returns aggregate statistics. Never panics; per-file errors are
    /// logged at `debug!` and counted in [`MinerSweepStats::files_scanned`].
    /// Under `shadow` a sweep that read new lines logs ONE summary line comparing the two
    /// extractors; the reader path never writes.
    pub fn sweep(&mut self, projects_root: &Path, store: &MemoryStore) -> MinerSweepStats {
        let paths = discover_transcript_paths(projects_root);

        let mut stats = MinerSweepStats {
            files_scanned: 0,
            lines_read: 0,
            pairs_mined: 0,
            pairs_persisted: 0,
            pairs_deduped: 0,
            reader_mode: self.reader_mode,
            legacy_pairs: 0,
            reader_pairs: 0,
            only_legacy: 0,
            only_reader: 0,
            reader_sidechain_dropped: 0,
        };
        let mut comparison = ShadowComparison::default();

        for path in &paths {
            stats.files_scanned += 1;

            let file = match self.sweep_file(path, store) {
                Ok(file) => file,
                Err(e) => {
                    tracing::debug!(
                        "TranscriptMiner: skipping {:?}: {}",
                        path.file_name().unwrap_or_default(),
                        e
                    );
                    continue;
                }
            };

            stats.lines_read += file.lines_read;
            stats.pairs_mined += file.pairs_mined;
            stats.pairs_persisted += file.pairs_persisted;
            stats.pairs_deduped += file.pairs_deduped;
            comparison.absorb(file.comparison);
        }

        stats.legacy_pairs = comparison.legacy_pairs;
        stats.reader_pairs = comparison.reader_pairs;
        stats.only_legacy = comparison.only_legacy;
        stats.only_reader = comparison.only_reader;
        stats.reader_sidechain_dropped = comparison.sidechain_dropped;

        // Persist updated offsets; log on failure but do not abort.
        if let Err(e) = self.state.save(&self.state_path) {
            tracing::debug!("TranscriptMiner: failed to save state: {}", e);
        }

        if self.reader_mode == ReaderMode::Shadow && stats.lines_read > 0 {
            log_shadow_summary(&stats, &comparison);
        }

        stats
    }

    /// Sweep a single file: read its new lines under the mode, then persist the pairs of the
    /// extractor that writes.
    fn sweep_file(&mut self, path: &Path, store: &MemoryStore) -> Result<FileSweep, String> {
        let path_key = path.to_string_lossy().to_string();
        let last_offset = self.state.offsets.get(&path_key).copied().unwrap_or(0);

        let Some(chunk) = mine_new_lines(path, last_offset, self.reader_mode)? else {
            // No new data.
            self.state.offsets.insert(path_key, last_offset);
            return Ok(FileSweep::default());
        };

        // Update offset to current stream position.
        self.state.offsets.insert(path_key, chunk.new_offset);

        let (pairs_persisted, pairs_deduped) = persist_pairs(store, &chunk.mined.pairs);
        Ok(FileSweep {
            lines_read: chunk.lines_read,
            pairs_mined: chunk.mined.pairs.len(),
            pairs_persisted,
            pairs_deduped,
            comparison: chunk.mined.comparison,
        })
    }
}

/// The one shadow line per sweep: counts at `info!`, the sample keys (keys only — never
/// transcript content) at `debug!` when the two extractors disagree.
fn log_shadow_summary(stats: &MinerSweepStats, comparison: &ShadowComparison) {
    tracing::info!(
        "TranscriptMiner shadow: files={} lines={} legacy_pairs={} reader_pairs={} only_legacy={} only_reader={} reader_sidechain_dropped={}",
        stats.files_scanned,
        stats.lines_read,
        stats.legacy_pairs,
        stats.reader_pairs,
        stats.only_legacy,
        stats.only_reader,
        stats.reader_sidechain_dropped,
    );
    if stats.only_legacy > 0 || stats.only_reader > 0 {
        tracing::debug!(
            "TranscriptMiner shadow disagreement: only_legacy_keys={:?} only_reader_keys={:?}",
            comparison.sample_only_legacy,
            comparison.sample_only_reader,
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Helper: serialise a json! value to a line string
    fn line(v: serde_json::Value) -> String {
        serde_json::to_string(&v).unwrap()
    }

    // SEC-05: the persisted lesson value must have secrets redacted out of both
    // the error text and the (stringified) resolution input before it ever
    // reaches the memory store.
    #[test]
    fn redacted_lesson_value_masks_secrets() {
        // Cross-audit 03/09/2026 — o valor sob teste e' montado em tempo de execucao.
        // Um literal com prefixo de token (`ghp_...`) faz do PROPRIO arquivo um achado
        // F2.4 (era o unico 0.000 em 1.763 arquivos), e ensinar o detector a isentar
        // "fixtures de redacao" criaria o esconderijo perfeito para um segredo real.
        // A redacao segue exercitada byte a byte: os asserts comparam os mesmos valores.
        let fake_token = format!("{}{}", "ghp_", "realsecret123");
        let fake_aws = format!("{}{}", "abcdef123", "secret");
        let pair = ErrorResolutionPair {
            tool_name: "Bash".to_string(),
            failed_input: json!({}),
            error_text: format!("fatal: GH_TOKEN={fake_token} rejected"),
            resolution_input: json!({ "command": format!("export AWS_SECRET_ACCESS_KEY={fake_aws}") }),
            session_id: "s1".to_string(),
            timestamp: "t1".to_string(),
        };
        let s = redacted_lesson_value(&pair).to_string();
        assert!(
            !s.contains(&fake_token),
            "GH_TOKEN value must be redacted: {s}"
        );
        assert!(
            !s.contains(&fake_aws),
            "AWS secret value must be redacted: {s}"
        );
        assert!(s.contains("[REDACTED]"), "redaction marker expected: {s}");
        // Non-secret fields are preserved untouched.
        assert!(s.contains("\"tool\":\"Bash\""), "tool field preserved: {s}");
        assert!(
            s.contains("\"session_id\":\"s1\""),
            "session preserved: {s}"
        );
    }

    // SEC-05: a clean pair (no credentials) must pass through with structure intact.
    #[test]
    fn redacted_lesson_value_preserves_clean_text() {
        let pair = ErrorResolutionPair {
            tool_name: "Edit".to_string(),
            failed_input: json!({}),
            error_text: "String to replace not found in file".to_string(),
            resolution_input: json!({ "old_string": "foo", "new_string": "bar" }),
            session_id: "s2".to_string(),
            timestamp: "t2".to_string(),
        };
        let s = redacted_lesson_value(&pair).to_string();
        assert!(!s.contains("[REDACTED]"), "no redaction on clean text: {s}");
        assert!(
            s.contains("String to replace not found"),
            "error text preserved: {s}"
        );
    }

    // 1. Assistant line with a tool_use block
    #[test]
    fn test_assistant_tool_use() {
        let raw = line(json!({
            "type": "assistant",
            "sessionId": "sess-abc",
            "timestamp": "2026-05-16T10:00:00Z",
            "uuid": "uuid-001",
            "message": {
                "content": [
                    {
                        "type": "tool_use",
                        "id": "toolu_abc123",
                        "name": "Bash",
                        "input": {"command": "cargo check"}
                    }
                ]
            }
        }));
        let parsed = parse_transcript_line(&raw).unwrap();
        assert_eq!(parsed.role, TranscriptRole::Assistant);
        assert_eq!(parsed.session_id, "sess-abc");
        assert_eq!(parsed.uuid, "uuid-001");
        assert_eq!(parsed.blocks.len(), 1);
        match &parsed.blocks[0] {
            ContentBlock::ToolUse {
                id,
                tool_name,
                input,
            } => {
                assert_eq!(id, "toolu_abc123");
                assert_eq!(tool_name, "Bash");
                assert_eq!(input["command"], "cargo check");
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    // 2. User line with tool_result is_error:true
    #[test]
    fn test_user_tool_result_error_true() {
        let raw = line(json!({
            "type": "user",
            "sessionId": "sess-abc",
            "message": {
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "toolu_abc123",
                        "is_error": true,
                        "content": "cargo: command not found"
                    }
                ]
            }
        }));
        let parsed = parse_transcript_line(&raw).unwrap();
        assert_eq!(parsed.role, TranscriptRole::User);
        match &parsed.blocks[0] {
            ContentBlock::ToolResult {
                is_error,
                content_text,
                ..
            } => {
                assert!(*is_error);
                assert_eq!(content_text, "cargo: command not found");
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    // 3. User line with tool_result is_error:false
    #[test]
    fn test_user_tool_result_error_false() {
        let raw = line(json!({
            "type": "user",
            "message": {
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "toolu_xyz",
                        "is_error": false,
                        "content": "ok"
                    }
                ]
            }
        }));
        let parsed = parse_transcript_line(&raw).unwrap();
        match &parsed.blocks[0] {
            ContentBlock::ToolResult { is_error, .. } => assert!(!is_error),
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    // 4. tool_result with is_error ABSENT → false
    #[test]
    fn test_tool_result_is_error_absent_defaults_false() {
        let raw = line(json!({
            "type": "user",
            "message": {
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "toolu_yyy",
                        "content": "some output"
                    }
                ]
            }
        }));
        let parsed = parse_transcript_line(&raw).unwrap();
        match &parsed.blocks[0] {
            ContentBlock::ToolResult {
                is_error,
                content_text,
                ..
            } => {
                assert!(!is_error, "absent is_error should default to false");
                assert_eq!(content_text, "some output");
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    // 5. tool_result content as plain STRING
    #[test]
    fn test_tool_result_content_plain_string() {
        let raw = line(json!({
            "type": "user",
            "message": {
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "toolu_zzz",
                        "content": "hello world"
                    }
                ]
            }
        }));
        let parsed = parse_transcript_line(&raw).unwrap();
        match &parsed.blocks[0] {
            ContentBlock::ToolResult { content_text, .. } => {
                assert_eq!(content_text, "hello world");
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    // 6. tool_result content as ARRAY of {type:text,text} blocks
    #[test]
    fn test_tool_result_content_array_of_text_blocks() {
        let raw = line(json!({
            "type": "user",
            "message": {
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "toolu_aaa",
                        "content": [
                            {"type": "text", "text": "line one\n"},
                            {"type": "text", "text": "line two"}
                        ]
                    }
                ]
            }
        }));
        let parsed = parse_transcript_line(&raw).unwrap();
        match &parsed.blocks[0] {
            ContentBlock::ToolResult { content_text, .. } => {
                assert_eq!(content_text, "line one\nline two");
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    // 7. Malformed / non-JSON line → None
    #[test]
    fn test_malformed_json_returns_none() {
        assert!(parse_transcript_line("this is not json {{{").is_none());
        assert!(parse_transcript_line("").is_none());
        assert!(parse_transcript_line("   ").is_none());
    }

    // 8. type:"system" line → None
    #[test]
    fn test_system_type_returns_none() {
        let raw = line(json!({
            "type": "system",
            "message": {"content": []}
        }));
        assert!(parse_transcript_line(&raw).is_none());
    }

    // 9. role user but message.content missing → None
    #[test]
    fn test_missing_message_content_returns_none() {
        let raw = line(json!({
            "type": "user",
            "message": {}
        }));
        assert!(parse_transcript_line(&raw).is_none());
    }

    // 10. type:"attachment" → None
    #[test]
    fn test_attachment_type_returns_none() {
        let raw = line(json!({
            "type": "attachment",
            "message": {"content": [{"type": "text", "text": "img"}]}
        }));
        assert!(parse_transcript_line(&raw).is_none());
    }

    // ---------------------------------------------------------------------------
    // Slice 2.2 tests (D4 — 9 new tests, keeping the 11 Slice 2.1 tests above)
    // ---------------------------------------------------------------------------

    // Helper: build a minimal assistant line with one ToolUse block.
    fn make_tool_use_line(
        session_id: &str,
        timestamp: &str,
        id: &str,
        tool_name: &str,
        input: serde_json::Value,
    ) -> ParsedTranscriptLine {
        ParsedTranscriptLine {
            role: TranscriptRole::Assistant,
            session_id: session_id.to_string(),
            timestamp: timestamp.to_string(),
            uuid: String::new(),
            blocks: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                tool_name: tool_name.to_string(),
                input,
            }],
        }
    }

    // Helper: build a minimal user line with one ToolResult block.
    fn make_tool_result_line(
        session_id: &str,
        timestamp: &str,
        tool_use_id: &str,
        is_error: bool,
        content: &str,
    ) -> ParsedTranscriptLine {
        ParsedTranscriptLine {
            role: TranscriptRole::User,
            session_id: session_id.to_string(),
            timestamp: timestamp.to_string(),
            uuid: String::new(),
            blocks: vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                is_error,
                content_text: content.to_string(),
            }],
        }
    }

    // S2.2-1: failed Bash → successful Bash ⇒ exactly 1 pair, fields correct.
    #[test]
    fn test_sm_basic_pair() {
        let lines = vec![
            make_tool_use_line(
                "s1",
                "t1",
                "id-fail",
                "Bash",
                json!({"command": "cargo check"}),
            ),
            make_tool_result_line("s1", "t2", "id-fail", true, "error: command not found"),
            make_tool_use_line(
                "s1",
                "t3",
                "id-ok",
                "Bash",
                json!({"command": "cargo build"}),
            ),
            make_tool_result_line("s1", "t4", "id-ok", false, "Compiling ok"),
        ];
        let pairs = extract_error_resolution_pairs(&lines);
        assert_eq!(pairs.len(), 1);
        let p = &pairs[0];
        assert_eq!(p.tool_name, "Bash");
        assert_eq!(p.error_text, "error: command not found");
        assert_eq!(p.failed_input, json!({"command": "cargo check"}));
        assert_eq!(p.resolution_input, json!({"command": "cargo build"}));
        assert_eq!(p.session_id, "s1");
        assert_eq!(p.timestamp, "t2");
    }

    // S2.2-2: failure with no subsequent same-tool success within window ⇒ 0 pairs.
    #[test]
    fn test_sm_no_resolution_yields_empty() {
        let lines = vec![
            make_tool_use_line("s1", "t1", "id-fail", "Bash", json!({"command": "bad"})),
            make_tool_result_line("s1", "t2", "id-fail", true, "error"),
            // No subsequent Bash ToolUse at all.
        ];
        let pairs = extract_error_resolution_pairs(&lines);
        assert_eq!(pairs.len(), 0);
    }

    // S2.2-3: failure resolved by a DIFFERENT tool ⇒ 0 pairs (must be same tool_name).
    #[test]
    fn test_sm_different_tool_not_a_resolution() {
        let lines = vec![
            make_tool_use_line("s1", "t1", "id-fail", "Bash", json!({"command": "bad"})),
            make_tool_result_line("s1", "t2", "id-fail", true, "error"),
            make_tool_use_line("s1", "t3", "id-edit", "Edit", json!({"path": "foo.rs"})),
            make_tool_result_line("s1", "t4", "id-edit", false, "ok"),
        ];
        let pairs = extract_error_resolution_pairs(&lines);
        assert_eq!(pairs.len(), 0);
    }

    // S2.2-4: resolution beyond RESOLUTION_SCAN_WINDOW same-tool uses ⇒ 0 pairs.
    #[test]
    fn test_sm_resolution_beyond_window() {
        // RESOLUTION_SCAN_WINDOW = 3; resolution is the 4th subsequent Bash.
        let mut lines = vec![
            make_tool_use_line("s1", "t1", "id-fail", "Bash", json!({"command": "bad"})),
            make_tool_result_line("s1", "t2", "id-fail", true, "error"),
        ];
        // 3 failing same-tool attempts (fills the window with errors).
        for i in 0..RESOLUTION_SCAN_WINDOW {
            let id = format!("id-mid-{i}");
            lines.push(make_tool_use_line(
                "s1",
                "t",
                &id,
                "Bash",
                json!({"command": "still bad"}),
            ));
            lines.push(make_tool_result_line("s1", "t", &id, true, "still error"));
        }
        // The 4th subsequent Bash succeeds — but it's beyond the window.
        lines.push(make_tool_use_line(
            "s1",
            "t9",
            "id-ok",
            "Bash",
            json!({"command": "good"}),
        ));
        lines.push(make_tool_result_line("s1", "t10", "id-ok", false, "ok"));

        let pairs = extract_error_resolution_pairs(&lines);
        assert_eq!(pairs.len(), 0);
    }

    // S2.2-5: error_text longer than ERROR_TEXT_MAX ⇒ truncated to exactly ERROR_TEXT_MAX chars.
    #[test]
    fn test_sm_error_text_truncated() {
        let long_error: String = "x".repeat(ERROR_TEXT_MAX + 100);
        let lines = vec![
            make_tool_use_line("s1", "t1", "id-fail", "Bash", json!({})),
            make_tool_result_line("s1", "t2", "id-fail", true, &long_error),
            make_tool_use_line("s1", "t3", "id-ok", "Bash", json!({})),
            make_tool_result_line("s1", "t4", "id-ok", false, "ok"),
        ];
        let pairs = extract_error_resolution_pairs(&lines);
        assert_eq!(pairs.len(), 1);
        let char_count = pairs[0].error_text.chars().count();
        assert_eq!(char_count, ERROR_TEXT_MAX);
    }

    // S2.2-6: multi-byte UTF-8 error_text at truncation boundary ⇒ no panic, valid String.
    #[test]
    fn test_sm_utf8_truncation_boundary() {
        // Each '中' is 3 bytes; build a string of ERROR_TEXT_MAX + 5 chars.
        let multibyte: String = "中".repeat(ERROR_TEXT_MAX + 5);
        let lines = vec![
            make_tool_use_line("s1", "t1", "id-fail", "Bash", json!({})),
            make_tool_result_line("s1", "t2", "id-fail", true, &multibyte),
            make_tool_use_line("s1", "t3", "id-ok", "Bash", json!({})),
            make_tool_result_line("s1", "t4", "id-ok", false, "ok"),
        ];
        // Must not panic.
        let pairs = extract_error_resolution_pairs(&lines);
        assert_eq!(pairs.len(), 1);
        let char_count = pairs[0].error_text.chars().count();
        assert_eq!(char_count, ERROR_TEXT_MAX);
        // Must be valid UTF-8.
        assert!(std::str::from_utf8(pairs[0].error_text.as_bytes()).is_ok());
    }

    // S2.2-7: dedup_key stable: same logical pair ⇒ identical key; different error_text ⇒ different key.
    #[test]
    fn test_dedup_key_stability() {
        let pair_a = ErrorResolutionPair {
            tool_name: "Bash".to_string(),
            failed_input: json!({}),
            error_text: "some error".to_string(),
            resolution_input: json!({}),
            session_id: String::new(),
            timestamp: String::new(),
        };
        let pair_b = ErrorResolutionPair {
            error_text: "different error".to_string(),
            ..pair_a.clone()
        };

        let key_a1 = dedup_key(&pair_a);
        let key_a2 = dedup_key(&pair_a);
        let key_b = dedup_key(&pair_b);

        assert_eq!(key_a1, key_a2, "same pair should produce same key");
        assert_ne!(
            key_a1, key_b,
            "different error_text should produce different key"
        );
        assert!(
            key_a1.starts_with("Bash:"),
            "key should be prefixed by tool_name"
        );
    }

    // S2.2-8: empty lines slice ⇒ empty Vec.
    #[test]
    fn test_sm_empty_input() {
        let pairs = extract_error_resolution_pairs(&[]);
        assert_eq!(pairs.len(), 0);
    }

    // S2.2-9: ToolResult with dangling tool_use_id (no matching ToolUse) ⇒ skipped, no panic.
    #[test]
    fn test_sm_dangling_tool_use_id() {
        let lines = vec![
            // A ToolResult that references a ToolUse id not in the stream.
            make_tool_result_line("s1", "t1", "ghost-id", true, "error from nowhere"),
        ];
        let pairs = extract_error_resolution_pairs(&lines);
        assert_eq!(pairs.len(), 0);
    }

    // ---------------------------------------------------------------------------
    // Slice 2.3 tests (D4)
    // ---------------------------------------------------------------------------

    // D4-1: discover_transcript_paths — finds .jsonl files one level deep, ignores non-.jsonl
    #[test]
    fn test_discover_transcript_paths_basic() {
        use tempfile::TempDir;
        let root = TempDir::new().unwrap();

        // Project A: two .jsonl files
        let proj_a = root.path().join("proj-a");
        std::fs::create_dir_all(&proj_a).unwrap();
        std::fs::write(proj_a.join("uuid-1.jsonl"), "").unwrap();
        std::fs::write(proj_a.join("uuid-2.jsonl"), "").unwrap();
        // A non-jsonl file that must be ignored
        std::fs::write(proj_a.join("notes.txt"), "").unwrap();

        // Project B: one .jsonl file
        let proj_b = root.path().join("proj-b");
        std::fs::create_dir_all(&proj_b).unwrap();
        std::fs::write(proj_b.join("uuid-3.jsonl"), "").unwrap();

        let paths = discover_transcript_paths(root.path());
        assert_eq!(
            paths.len(),
            3,
            "expected exactly 3 .jsonl files, got {}",
            paths.len()
        );

        // All returned paths must end in .jsonl
        for p in &paths {
            assert_eq!(
                p.extension().and_then(|e| e.to_str()),
                Some("jsonl"),
                "non-jsonl in result: {:?}",
                p
            );
        }

        // Result must be sorted
        let sorted = {
            let mut v = paths.clone();
            v.sort();
            v
        };
        assert_eq!(paths, sorted, "result is not sorted");
    }

    // D4-2: discover_transcript_paths — unreadable root returns empty Vec, no panic
    #[test]
    fn test_discover_transcript_paths_missing_root() {
        let paths = discover_transcript_paths(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(paths.is_empty());
    }

    // D4-3: discover_transcript_paths — ignores files directly in root (not in sub-dir)
    #[test]
    fn test_discover_transcript_paths_no_top_level_jsonl() {
        use tempfile::TempDir;
        let root = TempDir::new().unwrap();
        // A .jsonl file at root level — should be ignored (no slug dir)
        std::fs::write(root.path().join("stray.jsonl"), "").unwrap();
        // A proper nested one
        let proj = root.path().join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("real.jsonl"), "").unwrap();
        let paths = discover_transcript_paths(root.path());
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("real.jsonl"));
    }

    // D4-4: sweep offset behavior — second sweep only reads new lines
    #[test]
    fn test_sweep_incremental_offset() {
        use crate::memory_store::MemoryStore;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let state_path = tmp.path().join("miner_state.json");
        let mem_db = tmp.path().join("memory.db");

        // Write a single MemoryStore using canonical constructor
        let store = MemoryStore::new(&mem_db, &mem_db).expect("MemoryStore::new");

        // Build a projects_root with one transcript file
        let projects_root = tmp.path().join("projects");
        let proj_dir = projects_root.join("my-project");
        std::fs::create_dir_all(&proj_dir).unwrap();
        let jsonl_path = proj_dir.join("sess.jsonl");

        // First sweep: file is empty
        {
            std::fs::write(&jsonl_path, "").unwrap();
            let mut miner = TranscriptMiner::new(state_path.clone());
            let stats = miner.sweep(&projects_root, &store);
            assert_eq!(stats.files_scanned, 1);
            assert_eq!(stats.lines_read, 0);
            assert_eq!(stats.pairs_mined, 0);
        }

        // Append a failed→resolved pair to the file
        let failed_line = serde_json::to_string(&serde_json::json!({
            "type": "assistant",
            "sessionId": "s1",
            "timestamp": "t1",
            "uuid": "u1",
            "message": {
                "content": [{
                    "type": "tool_use",
                    "id": "id-fail",
                    "name": "Bash",
                    "input": {"command": "bad cmd"}
                }]
            }
        }))
        .unwrap();
        let result_error_line = serde_json::to_string(&serde_json::json!({
            "type": "user",
            "sessionId": "s1",
            "timestamp": "t2",
            "uuid": "u2",
            "message": {
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "id-fail",
                    "is_error": true,
                    "content": "command not found"
                }]
            }
        }))
        .unwrap();
        let ok_use_line = serde_json::to_string(&serde_json::json!({
            "type": "assistant",
            "sessionId": "s1",
            "timestamp": "t3",
            "uuid": "u3",
            "message": {
                "content": [{
                    "type": "tool_use",
                    "id": "id-ok",
                    "name": "Bash",
                    "input": {"command": "cargo build"}
                }]
            }
        }))
        .unwrap();
        let ok_result_line = serde_json::to_string(&serde_json::json!({
            "type": "user",
            "sessionId": "s1",
            "timestamp": "t4",
            "uuid": "u4",
            "message": {
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "id-ok",
                    "is_error": false,
                    "content": "Compiling ok"
                }]
            }
        }))
        .unwrap();

        std::fs::write(
            &jsonl_path,
            format!(
                "{}\n{}\n{}\n{}\n",
                failed_line, result_error_line, ok_use_line, ok_result_line
            ),
        )
        .unwrap();

        // Second sweep: must read all 4 lines, mine 1 pair, persist 1
        {
            let mut miner = TranscriptMiner::new(state_path.clone());
            let stats = miner.sweep(&projects_root, &store);
            assert_eq!(stats.files_scanned, 1);
            assert_eq!(stats.lines_read, 4, "expected 4 lines on second sweep");
            assert_eq!(stats.pairs_mined, 1, "expected 1 pair mined");
            assert_eq!(stats.pairs_persisted, 1, "expected 1 pair persisted");
            assert_eq!(stats.pairs_deduped, 0);
        }

        // Third sweep: same file, nothing new → 0 lines read
        {
            let mut miner = TranscriptMiner::new(state_path.clone());
            let stats = miner.sweep(&projects_root, &store);
            assert_eq!(stats.lines_read, 0, "third sweep should read 0 new lines");
            assert_eq!(stats.pairs_mined, 0);
        }
    }

    // D4-5: E2E — fixture with 2 projects, sweep mines pairs, MemoryStore has outcome key
    #[test]
    fn test_sweep_e2e_mines_pairs_into_memory_store() {
        use crate::memory_store::MemoryStore;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let state_path = tmp.path().join("miner_state.json");
        let mem_db = tmp.path().join("memory.db");
        let store = MemoryStore::new(&mem_db, &mem_db).expect("MemoryStore::new");

        let projects_root = tmp.path().join("projects");

        // Project 1: a genuine failed→resolved Bash sequence
        let proj1 = projects_root.join("project-alpha");
        std::fs::create_dir_all(&proj1).unwrap();

        let make_assistant = |id: &str, tool: &str, cmd: &str| -> String {
            serde_json::to_string(&serde_json::json!({
                "type": "assistant", "sessionId": "s1", "timestamp": "t", "uuid": id,
                "message": {"content": [{"type": "tool_use", "id": id, "name": tool,
                    "input": {"command": cmd}}]}
            }))
            .unwrap()
        };
        let make_result = |id: &str, is_error: bool, content: &str| -> String {
            serde_json::to_string(&serde_json::json!({
                "type": "user", "sessionId": "s1", "timestamp": "t", "uuid": id,
                "message": {"content": [{"type": "tool_result",
                    "tool_use_id": id, "is_error": is_error, "content": content}]}
            }))
            .unwrap()
        };

        let transcript = format!(
            "{}\n{}\n{}\n{}\n",
            make_assistant("a1", "Bash", "cargo check"),
            make_result("a1", true, "error: cannot find crate"),
            make_assistant("a2", "Bash", "cargo build"),
            make_result("a2", false, "Compiling touring-server"),
        );
        std::fs::write(proj1.join("session1.jsonl"), &transcript).unwrap();

        // Project 2: an Edit error→resolution
        let proj2 = projects_root.join("project-beta");
        std::fs::create_dir_all(&proj2).unwrap();
        let transcript2 = format!(
            "{}\n{}\n{}\n{}\n",
            make_assistant("b1", "Edit", "fix.rs"),
            make_result("b1", true, "file not found"),
            make_assistant("b2", "Edit", "fix2.rs"),
            make_result("b2", false, "ok"),
        );
        std::fs::write(proj2.join("session2.jsonl"), &transcript2).unwrap();

        let mut miner = TranscriptMiner::new(state_path);
        let stats = miner.sweep(&projects_root, &store);

        // Should mine at least 1 pair (Bash) + 1 pair (Edit) = 2
        assert!(
            stats.pairs_mined >= 1,
            "expected at least 1 pair mined, got {}",
            stats.pairs_mined
        );
        assert!(
            stats.pairs_persisted >= 1,
            "expected at least 1 pair persisted, got {}",
            stats.pairs_persisted
        );

        // MemoryStore must contain at least one mined-lesson key.
        let matches = store
            .scan_prefix("outcome:", "reference", 50)
            .expect("scan_prefix");
        assert!(
            !matches.is_empty(),
            "MemoryStore should contain at least one outcome:* entry"
        );

        // The key MUST honor the Phase 1 reader contract: cli_suggester queries
        // `outcome:<tool_class>:%:failure`, so the mined key must be
        // `outcome:<tool_class>:transcript-<hash>:failure`.
        let first_key = &matches[0].key;
        assert!(
            first_key.starts_with("outcome:")
                && first_key.ends_with(":failure")
                && first_key.contains(":transcript-"),
            "key must match outcome:<tool_class>:transcript-<hash>:failure, got: {}",
            first_key
        );
    }

    // D4-6: dedup — sweeping same file twice does not double-persist
    #[test]
    fn test_sweep_dedup_on_resweep() {
        use crate::memory_store::MemoryStore;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let mem_db = tmp.path().join("memory.db");
        let store = MemoryStore::new(&mem_db, &mem_db).expect("MemoryStore::new");

        let projects_root = tmp.path().join("projects");
        let proj = projects_root.join("myproj");
        std::fs::create_dir_all(&proj).unwrap();

        let transcript = serde_json::to_string(&serde_json::json!({
            "type": "assistant", "sessionId": "s", "timestamp": "t", "uuid": "x1",
            "message": {"content": [{"type": "tool_use", "id": "x1", "name": "Bash",
                "input": {"command": "bad"}}]}
        }))
        .unwrap()
            + "\n"
            + &serde_json::to_string(&serde_json::json!({
                "type": "user", "sessionId": "s", "timestamp": "t", "uuid": "x1",
                "message": {"content": [{"type": "tool_result",
                    "tool_use_id": "x1", "is_error": true, "content": "err"}]}
            }))
            .unwrap()
            + "\n"
            + &serde_json::to_string(&serde_json::json!({
                "type": "assistant", "sessionId": "s", "timestamp": "t", "uuid": "x2",
                "message": {"content": [{"type": "tool_use", "id": "x2", "name": "Bash",
                    "input": {"command": "good"}}]}
            }))
            .unwrap()
            + "\n"
            + &serde_json::to_string(&serde_json::json!({
                "type": "user", "sessionId": "s", "timestamp": "t", "uuid": "x2",
                "message": {"content": [{"type": "tool_result",
                    "tool_use_id": "x2", "is_error": false, "content": "ok"}]}
            }))
            .unwrap()
            + "\n";

        std::fs::write(proj.join("t.jsonl"), &transcript).unwrap();

        // First sweep — fresh state path (no persistence, each miner starts at 0)
        let stats1 = {
            let mut miner = TranscriptMiner::new(tmp.path().join("state1.json"));
            miner.sweep(&projects_root, &store)
        };
        assert_eq!(stats1.pairs_persisted, 1);

        // Second sweep from offset 0 — pair already in store → deduped
        let stats2 = {
            let mut miner = TranscriptMiner::new(tmp.path().join("state2.json"));
            miner.sweep(&projects_root, &store)
        };
        assert_eq!(
            stats2.pairs_deduped, 1,
            "second sweep should dedup the already-stored pair"
        );
        assert_eq!(stats2.pairs_persisted, 0);
    }

    // D4-7: regression — lesson_memory_key honors the cli_suggester reader
    // contract. Bug (2026-05-16): the miner wrote `outcome:transcript:<tool>:
    // <hash>`, which the Phase 1 reader's `LIKE 'outcome:<tool_class>:%:failure'`
    // query never matched — Phase 2 lessons were mined but never injected.
    #[test]
    fn test_lesson_memory_key_honors_reader_contract() {
        for (tool, expected_class) in [
            ("Bash", "bash"),
            ("Edit", "edit"),
            ("Write", "write"),
            ("Grep", "search"),
            ("Glob", "search"),
            ("WebFetch", "web"),
        ] {
            let pair = ErrorResolutionPair {
                tool_name: tool.to_string(),
                failed_input: serde_json::json!({"x": 1}),
                error_text: "boom".to_string(),
                resolution_input: serde_json::json!({"x": 2}),
                session_id: "s".to_string(),
                timestamp: "t".to_string(),
            };
            let key = lesson_memory_key(&pair);
            assert!(
                key.starts_with(&format!("outcome:{expected_class}:")),
                "key {key} must start with outcome:{expected_class}:"
            );
            assert!(
                key.ends_with(":failure"),
                "key {key} must end with :failure"
            );
            assert!(
                key.contains(":transcript-"),
                "key {key} must carry the transcript- discriminator"
            );
        }
    }

    // 11. Mixed blocks: ToolUse + Other in one assistant turn
    #[test]
    fn test_mixed_blocks_assistant() {
        let raw = line(json!({
            "type": "assistant",
            "message": {
                "content": [
                    {"type": "text", "text": "I will run cargo check"},
                    {
                        "type": "tool_use",
                        "id": "toolu_mix",
                        "name": "Bash",
                        "input": {"command": "cargo check"}
                    }
                ]
            }
        }));
        let parsed = parse_transcript_line(&raw).unwrap();
        assert_eq!(parsed.blocks.len(), 2);
        assert_eq!(parsed.blocks[0], ContentBlock::Other);
        match &parsed.blocks[1] {
            ContentBlock::ToolUse { id, .. } => assert_eq!(id, "toolu_mix"),
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------------
    // Slice 2.4 tests — the reader path, in shadow (C1, 04/11/2026)
    // ---------------------------------------------------------------------------

    /// An assistant record calling one tool, optionally written from a subagent window.
    fn use_rec(id: &str, name: &str, input: serde_json::Value, sidechain: bool) -> String {
        let mut rec = json!({
            "type": "assistant", "sessionId": "sess-main", "timestamp": format!("ts-use-{id}"),
            "message": {"content": [{"type": "tool_use", "id": id, "name": name, "input": input}]}
        });
        if sidechain {
            rec["isSidechain"] = json!(true);
        }
        line(rec)
    }

    /// A user record carrying the result of call `id`; the timestamp names the call so a pair's
    /// provenance can be told apart.
    fn result_rec(id: &str, is_error: bool, content: serde_json::Value, sidechain: bool) -> String {
        let mut rec = json!({
            "type": "user", "sessionId": "sess-main", "timestamp": format!("ts-result-{id}"),
            "message": {"content": [{"type": "tool_result", "tool_use_id": id,
                "is_error": is_error, "content": content}]}
        });
        if sidechain {
            rec["isSidechain"] = json!(true);
        }
        line(rec)
    }

    /// Mine `lines` under `mode`, exactly as a sweep does a stretch of a file.
    fn mine(mode: ReaderMode, lines: &[String]) -> ChunkMined {
        let mut miner = ChunkMiner::new(mode);
        for l in lines {
            miner.push(l);
        }
        miner.finish()
    }

    /// Pairs in a deterministic order (`mine_chains` walks a `HashMap`).
    fn sorted(mut pairs: Vec<ErrorResolutionPair>) -> Vec<ErrorResolutionPair> {
        pairs.sort_by_key(|p| {
            (
                p.tool_name.clone(),
                p.error_text.clone(),
                p.failed_input.to_string(),
                p.resolution_input.to_string(),
            )
        });
        pairs
    }

    // C1-a: a subagent's error→resolution is another window. Legacy mines it as if it were the
    // session's; the reader path does not, and says how many records it dropped.
    #[test]
    fn a_sidechain_pair_is_mined_by_legacy_and_not_by_the_reader_path() {
        let lines = vec![
            use_rec("s1", "Bash", json!({"command": "cargo chek"}), true),
            result_rec("s1", true, json!("error: no such command"), true),
            use_rec("s2", "Bash", json!({"command": "cargo check"}), true),
            result_rec("s2", false, json!("Finished"), true),
        ];

        let legacy = mine(ReaderMode::Off, &lines);
        assert_eq!(legacy.pairs.len(), 1, "legacy mines the subagent window");

        let reader = mine(ReaderMode::On, &lines);
        assert!(
            reader.pairs.is_empty(),
            "the reader path does not: {:?}",
            reader.pairs
        );
        assert_eq!(reader.comparison.sidechain_dropped, 4);

        // Shadow: legacy still writes, the difference is counted by key.
        let shadow = mine(ReaderMode::Shadow, &lines);
        let c = &shadow.comparison;
        assert_eq!(
            shadow.pairs, legacy.pairs,
            "shadow returns the legacy pairs to write"
        );
        assert_eq!(
            (c.legacy_pairs, c.reader_pairs, c.only_legacy, c.only_reader),
            (1, 0, 1, 0)
        );
        assert_eq!(c.sample_only_legacy, vec![dedup_key(&legacy.pairs[0])]);
        assert!(c.sample_only_reader.is_empty());
        assert_eq!(c.sidechain_dropped, 4);
    }

    // C1-b: a call repeated across records is ONE call. Legacy lists the id again at every
    // repetition, so two failing calls written twice each read as a 4-failure chain, beyond the
    // window, and the lesson is lost; the reader path enters each id once and mines one pair.
    #[test]
    fn a_duplicated_tool_use_id_yields_one_pair_in_the_reader_path() {
        let lines = vec![
            use_rec("a1", "Bash", json!({"command": "one"}), false),
            use_rec("a1", "Bash", json!({"command": "one"}), false),
            result_rec("a1", true, json!("boom"), false),
            use_rec("b1", "Bash", json!({"command": "two"}), false),
            use_rec("b1", "Bash", json!({"command": "two"}), false),
            result_rec("b1", true, json!("boom"), false),
            use_rec("c1", "Bash", json!({"command": "three"}), false),
            result_rec("c1", false, json!("ok"), false),
        ];

        let reader = mine(ReaderMode::On, &lines);
        assert_eq!(reader.pairs.len(), 1, "{:?}", reader.pairs);
        assert_eq!(reader.pairs[0].failed_input, json!({"command": "one"}));
        assert_eq!(
            reader.pairs[0].resolution_input,
            json!({"command": "three"})
        );
        assert_eq!(reader.pairs[0].timestamp, "ts-result-a1");

        let legacy = mine(ReaderMode::Off, &lines);
        assert!(
            legacy.pairs.is_empty(),
            "legacy counts the repeated ids as 4 failures (> {RESOLUTION_SCAN_WINDOW}): {:?}",
            legacy.pairs
        );

        let c = mine(ReaderMode::Shadow, &lines).comparison;
        assert_eq!(
            (c.legacy_pairs, c.reader_pairs, c.only_legacy, c.only_reader),
            (0, 1, 0, 1)
        );
        assert_eq!(c.sample_only_reader, vec![dedup_key(&reader.pairs[0])]);
    }

    // C1-c: on plain main-thread records the two paths form the SAME pairs — every field,
    // including the session and the instant of the failed result — because both end in
    // `mine_chains`. The fixture covers a one-failure pair, a two-failure chain whose error is
    // an array of text blocks, a multibyte error longer than ERROR_TEXT_MAX, a chain beyond the
    // window (dropped), an unresolved failure, a call with no result, and chatter.
    #[test]
    fn both_paths_agree_exactly_on_plain_main_thread_records() {
        let long_error = "é".repeat(ERROR_TEXT_MAX + 100);
        let mut lines = vec![
            line(json!({"type": "user", "message": {"content": "a plain string turn"}})),
            line(json!({"type": "assistant",
                "message": {"content": [{"type": "text", "text": "thinking out loud"}]}})),
            line(json!({"type": "attachment", "attachment": {"type": "hook_success"}})),
            "this line is not json".to_string(),
            use_rec("b1", "Bash", json!({"command": "cargo chek"}), false),
            result_rec("b1", true, json!("error: no such command: `chek`"), false),
            use_rec("b2", "Bash", json!({"command": "cargo check"}), false),
            result_rec("b2", false, json!("Finished"), false),
            use_rec(
                "e1",
                "Edit",
                json!({"file_path": "a.rs", "old_string": "x"}),
                false,
            ),
            result_rec(
                "e1",
                true,
                json!([{"type": "text", "text": "String to replace "},
                       {"type": "text", "text": "not found"}]),
                false,
            ),
            use_rec(
                "e2",
                "Edit",
                json!({"file_path": "a.rs", "old_string": "y"}),
                false,
            ),
            result_rec("e2", true, json!(long_error), false),
            use_rec(
                "e3",
                "Edit",
                json!({"file_path": "a.rs", "old_string": "z"}),
                false,
            ),
            result_rec("e3", false, json!("updated"), false),
            use_rec("r1", "Read", json!({"file_path": "missing"}), false),
            result_rec("r1", true, json!("no such file"), false),
            use_rec("g1", "Grep", json!({"pattern": "x"}), false),
        ];
        for i in 1..=RESOLUTION_SCAN_WINDOW + 1 {
            let id = format!("w{i}");
            lines.push(use_rec(&id, "Write", json!({"n": i}), false));
            lines.push(result_rec(&id, true, json!("disk full"), false));
        }
        lines.push(use_rec("w-ok", "Write", json!({"n": 0}), false));
        lines.push(result_rec("w-ok", false, json!("written"), false));

        let legacy = sorted(mine(ReaderMode::Off, &lines).pairs);
        let reader = sorted(mine(ReaderMode::On, &lines).pairs);
        assert_eq!(
            legacy.len(),
            2,
            "Bash and Edit resolve; Write overflows, Read never does"
        );
        assert_eq!(legacy, reader, "same pairs, field for field");
        assert!(
            legacy
                .iter()
                .any(|p| p.timestamp == "ts-result-b1" && p.session_id == "sess-main")
        );
        assert!(
            legacy
                .iter()
                .any(|p| p.tool_name == "Edit" && p.error_text == "String to replace not found"),
            "the chain starts at the FIRST failure (array content flattened), not at the long one"
        );

        let shadow = mine(ReaderMode::Shadow, &lines);
        assert_eq!(sorted(shadow.pairs), legacy);
        let c = shadow.comparison;
        assert_eq!(
            (
                c.legacy_pairs,
                c.reader_pairs,
                c.only_legacy,
                c.only_reader,
                c.sidechain_dropped
            ),
            (2, 2, 0, 0, 0)
        );
        assert!(c.sample_only_legacy.is_empty() && c.sample_only_reader.is_empty());
    }

    // C1-c': the truncation the reader path applies while indexing is the one legacy applies
    // when forming the pair — a multibyte error past the cap is cut at the same character.
    #[test]
    fn the_reader_path_truncates_a_long_error_where_legacy_does() {
        let long_error = "é".repeat(ERROR_TEXT_MAX + 100);
        let lines = vec![
            use_rec("x1", "Bash", json!({"command": "bad"}), false),
            result_rec("x1", true, json!(long_error), false),
            use_rec("x2", "Bash", json!({"command": "good"}), false),
            result_rec("x2", false, json!("ok"), false),
        ];
        let legacy = mine(ReaderMode::Off, &lines).pairs;
        let reader = mine(ReaderMode::On, &lines).pairs;
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].error_text.chars().count(), ERROR_TEXT_MAX);
        assert_eq!(legacy, reader);
    }

    // C1-d: the mode parser. Default shadow; an unknown value is shadow WITH a warning and never
    // `on` (a typo must not hand the writer role to an unmeasured path); nothing panics.
    #[test]
    fn reader_mode_defaults_to_shadow_and_never_panics_on_junk() {
        assert_eq!(ReaderMode::default(), ReaderMode::Shadow);
        assert_eq!(ReaderMode::parse(None), (ReaderMode::Shadow, None));
        for (raw, want) in [
            ("shadow", ReaderMode::Shadow),
            ("SHADOW", ReaderMode::Shadow),
            ("", ReaderMode::Shadow),
            ("  ", ReaderMode::Shadow),
            ("on", ReaderMode::On),
            (" On ", ReaderMode::On),
            ("off", ReaderMode::Off),
            ("OFF", ReaderMode::Off),
        ] {
            assert_eq!(ReaderMode::parse(Some(raw)), (want, None), "{raw:?}");
        }
        for junk in [
            "maybe",
            "1",
            "0",
            "true",
            "false",
            "shadow on",
            "o n",
            "\u{0}",
            "ón",
        ] {
            let (mode, warning) = ReaderMode::parse(Some(junk));
            assert_eq!(mode, ReaderMode::Shadow, "{junk:?}");
            let w = warning.expect("an unknown value is warned about");
            assert!(
                w.contains(MINER_READER_ENV) && w.contains("using shadow"),
                "{w}"
            );
        }
        for mode in [ReaderMode::Off, ReaderMode::Shadow, ReaderMode::On] {
            assert_eq!(ReaderMode::parse(Some(mode.as_str())), (mode, None));
            assert_eq!(
                serde_json::to_string(&mode).unwrap(),
                format!("\"{}\"", mode.as_str())
            );
        }
    }

    // C1: `MinerSweepStats` is serialized; a payload written before the shadow fields existed
    // still deserializes, with zeros and the default mode.
    #[test]
    fn sweep_stats_without_the_shadow_fields_still_deserialize() {
        let old = r#"{"files_scanned":3,"lines_read":10,"pairs_mined":2,"pairs_persisted":1,"pairs_deduped":1}"#;
        let stats: MinerSweepStats = serde_json::from_str(old).expect("old payload");
        assert_eq!((stats.files_scanned, stats.pairs_persisted), (3, 1));
        assert_eq!(stats.reader_mode, ReaderMode::Shadow);
        assert_eq!(
            (
                stats.legacy_pairs,
                stats.reader_pairs,
                stats.only_legacy,
                stats.only_reader
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(stats.reader_sidechain_dropped, 0);
    }

    // C1: the mode decides which extractor WRITES, end to end through `sweep` and a real store.
    // The transcript holds a subagent pair (Bash) and a main-thread pair (Edit): `off` and
    // `shadow` persist both (legacy writes), `on` persists only the main thread's — and under
    // `shadow` the reader path is measured but wrote nothing of its own.
    #[test]
    fn the_mode_decides_which_extractor_writes() {
        use crate::memory_store::MemoryStore;
        use tempfile::TempDir;

        let lines = vec![
            use_rec("s1", "Bash", json!({"command": "side bad"}), true),
            result_rec("s1", true, json!("sidechain failure"), true),
            use_rec("s2", "Bash", json!({"command": "side good"}), true),
            result_rec("s2", false, json!("ok"), true),
            use_rec("m1", "Edit", json!({"file_path": "a.rs"}), false),
            result_rec("m1", true, json!("main failure"), false),
            use_rec(
                "m2",
                "Edit",
                json!({"file_path": "a.rs", "fixed": true}),
                false,
            ),
            result_rec("m2", false, json!("ok"), false),
        ];
        let transcript = lines.join("\n") + "\n";
        let legacy_pairs = mine(ReaderMode::Off, &lines).pairs;
        let side_key = lesson_memory_key(
            legacy_pairs
                .iter()
                .find(|p| p.tool_name == "Bash")
                .expect("sidechain pair"),
        );
        let main_key = lesson_memory_key(
            legacy_pairs
                .iter()
                .find(|p| p.tool_name == "Edit")
                .expect("main pair"),
        );

        let sweep_in = |mode: ReaderMode| {
            let tmp = TempDir::new().unwrap();
            let mem_db = tmp.path().join("memory.db");
            let store = MemoryStore::new(&mem_db, &mem_db).expect("MemoryStore::new");
            let projects_root = tmp.path().join("projects");
            let proj = projects_root.join("p");
            std::fs::create_dir_all(&proj).unwrap();
            std::fs::write(proj.join("t.jsonl"), &transcript).unwrap();
            let mut miner = TranscriptMiner::with_reader_mode(tmp.path().join("state.json"), mode);
            let stats = miner.sweep(&projects_root, &store);
            let wrote_side = store.get(&side_key, "reference").unwrap().is_some();
            let wrote_main = store.get(&main_key, "reference").unwrap().is_some();
            (stats, wrote_side, wrote_main)
        };

        let (off, side, main) = sweep_in(ReaderMode::Off);
        assert_eq!((off.pairs_mined, off.pairs_persisted), (2, 2));
        assert_eq!((side, main), (true, true));
        assert_eq!(off.reader_mode, ReaderMode::Off);
        assert_eq!(
            (
                off.legacy_pairs,
                off.reader_pairs,
                off.reader_sidechain_dropped
            ),
            (2, 0, 0)
        );

        let (shadow, side, main) = sweep_in(ReaderMode::Shadow);
        assert_eq!((shadow.pairs_mined, shadow.pairs_persisted), (2, 2));
        assert_eq!(
            (side, main),
            (true, true),
            "legacy still writes under shadow"
        );
        assert_eq!(shadow.reader_mode, ReaderMode::Shadow);
        assert_eq!(
            (
                shadow.legacy_pairs,
                shadow.reader_pairs,
                shadow.only_legacy,
                shadow.only_reader
            ),
            (2, 1, 1, 0)
        );
        assert_eq!(shadow.reader_sidechain_dropped, 4);

        let (on, side, main) = sweep_in(ReaderMode::On);
        assert_eq!((on.pairs_mined, on.pairs_persisted), (1, 1));
        assert_eq!(
            (side, main),
            (false, true),
            "the reader path writes, minus the subagent"
        );
        assert_eq!(on.reader_mode, ReaderMode::On);
        assert_eq!(
            (
                on.legacy_pairs,
                on.reader_pairs,
                on.reader_sidechain_dropped
            ),
            (0, 1, 4)
        );
    }
}
