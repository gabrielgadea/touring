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
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use touring_hooks::action_signature::classify_tool_class;
use touring_hooks::gateway::sandbox_executor::redact_secrets;

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
    let mut pairs = Vec::new();

    for (tool_name, ids) in &uses_by_tool {
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
                if let Some((_, first_use, first_result)) = chain_start {
                    if chain_len <= RESOLUTION_SCAN_WINDOW {
                        pairs.push(ErrorResolutionPair {
                            tool_name: tool_name.clone(),
                            failed_input: first_use.input.clone(),
                            error_text: truncate_chars(&first_result.content_text, ERROR_TEXT_MAX),
                            resolution_input: use_entry.input.clone(),
                            session_id: first_result.session_id.clone(),
                            timestamp: first_result.timestamp.clone(),
                        });
                    }
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
#[derive(Debug, Clone, Serialize)]
pub struct MinerSweepStats {
    /// Number of transcript files opened during this sweep.
    pub files_scanned: usize,
    /// Total new lines read across all files.
    pub lines_read: usize,
    /// Error→resolution pairs extracted from new lines.
    pub pairs_mined: usize,
    /// Pairs actually written to [`MemoryStore`] (new, not deduped).
    pub pairs_persisted: usize,
    /// Pairs skipped because the memory key already existed.
    pub pairs_deduped: usize,
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
///    error→resolution pairs via [`extract_error_resolution_pairs`].
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
}

impl TranscriptMiner {
    /// Construct a new miner, loading persisted offset state from
    /// `state_path` (or starting fresh if the file is absent/invalid).
    pub fn new(state_path: PathBuf) -> Self {
        let state = MinerState::load(&state_path);
        Self { state, state_path }
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
    pub fn sweep(&mut self, projects_root: &Path, store: &MemoryStore) -> MinerSweepStats {
        let paths = discover_transcript_paths(projects_root);

        let mut stats = MinerSweepStats {
            files_scanned: 0,
            lines_read: 0,
            pairs_mined: 0,
            pairs_persisted: 0,
            pairs_deduped: 0,
        };

        for path in &paths {
            stats.files_scanned += 1;

            let (lines_read, pairs_mined, pairs_persisted, pairs_deduped) =
                match self.sweep_file(path, store) {
                    Ok(counts) => counts,
                    Err(e) => {
                        tracing::debug!(
                            "TranscriptMiner: skipping {:?}: {}",
                            path.file_name().unwrap_or_default(),
                            e
                        );
                        continue;
                    }
                };

            stats.lines_read += lines_read;
            stats.pairs_mined += pairs_mined;
            stats.pairs_persisted += pairs_persisted;
            stats.pairs_deduped += pairs_deduped;
        }

        // Persist updated offsets; log on failure but do not abort.
        if let Err(e) = self.state.save(&self.state_path) {
            tracing::debug!("TranscriptMiner: failed to save state: {}", e);
        }

        stats
    }

    /// Sweep a single file, returning (lines_read, pairs_mined, pairs_persisted, pairs_deduped).
    fn sweep_file(
        &mut self,
        path: &Path,
        store: &MemoryStore,
    ) -> Result<(usize, usize, usize, usize), String> {
        let path_key = path.to_string_lossy().to_string();
        let last_offset = self.state.offsets.get(&path_key).copied().unwrap_or(0);

        let file = std::fs::File::open(path)
            .map_err(|e| format!("open {:?}: {}", path.file_name().unwrap_or_default(), e))?;

        let file_len = file.metadata().map_err(|e| format!("stat: {}", e))?.len();

        if file_len <= last_offset {
            // No new data.
            self.state.offsets.insert(path_key, last_offset);
            return Ok((0, 0, 0, 0));
        }

        let mut reader = BufReader::new(file);
        reader
            .seek(SeekFrom::Start(last_offset))
            .map_err(|e| format!("seek: {}", e))?;

        let mut line_buf = String::new();
        let mut parsed_lines: Vec<ParsedTranscriptLine> = Vec::new();
        let mut lines_read = 0usize;

        while reader
            .read_line(&mut line_buf)
            .map_err(|e| format!("read_line: {}", e))?
            > 0
        {
            let trimmed = line_buf.trim();
            if !trimmed.is_empty() {
                lines_read += 1;
                if let Some(parsed) = parse_transcript_line(trimmed) {
                    parsed_lines.push(parsed);
                }
            }
            line_buf.clear();
        }

        // Update offset to current stream position.
        let new_offset = reader
            .stream_position()
            .map_err(|e| format!("stream_position: {}", e))?;
        self.state.offsets.insert(path_key, new_offset);

        let pairs = extract_error_resolution_pairs(&parsed_lines);
        let pairs_mined = pairs.len();
        let mut pairs_persisted = 0usize;
        let mut pairs_deduped = 0usize;

        for pair in &pairs {
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

        Ok((lines_read, pairs_mined, pairs_persisted, pairs_deduped))
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
        let pair = ErrorResolutionPair {
            tool_name: "Bash".to_string(),
            failed_input: json!({}),
            error_text: "fatal: GH_TOKEN=ghp_realsecret123 rejected".to_string(),
            resolution_input: json!({ "command": "export AWS_SECRET_ACCESS_KEY=abcdef123secret" }),
            session_id: "s1".to_string(),
            timestamp: "t1".to_string(),
        };
        let s = redacted_lesson_value(&pair).to_string();
        assert!(
            !s.contains("ghp_realsecret123"),
            "GH_TOKEN value must be redacted: {s}"
        );
        assert!(
            !s.contains("abcdef123secret"),
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
}
