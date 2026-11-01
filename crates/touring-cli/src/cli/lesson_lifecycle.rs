//! B1 (01/11/2026) — how many of the lessons the transcript miner stores are confirmed facts,
//! in SHADOW.
//!
//! The miner (`touring-server`'s `ingest::transcript_miner`) turns ONE witnessed
//! error-then-resolution into a lesson; its key is `(tool, 200 chars of the error)`; a recurrence
//! in another session is dropped as a duplicate, so the store cannot say how many sessions saw the
//! same fact (ADR-001). TokenEconomizer keeps a micro-skill only after K=3 confirmations
//! (`lifecycle.py:15-21`). This module measures what that rule would do to OUR stock, and writes
//! nothing: no state, no tag, no retirement.
//!
//! # The extractor is a port
//!
//! `touring-server` sits above `touring-cli` in the dependency graph, so its
//! `extract_error_resolution_pairs` cannot be called from here. [`LessonTracker`] is the same
//! algorithm, line by line: per tool, a contiguous run of failed calls closed by a success within
//! [`RESOLUTION_SCAN_WINDOW`] emits ONE pair from the FIRST failure; a call without a result breaks
//! the run; the first result of an id wins; a tool result is a string, or its text blocks joined
//! with nothing, or its JSON; the error is cut at [`ERROR_TEXT_MAX`] CHARS. The unit tests are the
//! original's own, case by case. The reference of the measurement is `medicao/f4a_lessons.py`,
//! which differs from the miner in small ways (it joins text blocks with a newline and lets the
//! LAST result of an id win); the probe reports both on the same corpus.
//!
//! # The key
//!
//! Normalised BEFORE the cut: the error text (already cut at 500 chars) has paths replaced by
//! `<path>`, long hex by `<hex>`, numbers by `<n>`, whitespace collapsed and the ends trimmed; the
//! key is the tool and the first [`KEY_PREFIX_CHARS`] chars of that. Two sessions that hit the same
//! failure at different line numbers, commits or temp paths are then ONE fact.
//!
//! # What is reported
//!
//! Keys, their distribution by the number of DISTINCT sessions that saw them (1, 2, 3, 4, 5+), how
//! many would be `active` (at least [`K_PROMOTE`] sessions) and how many `candidate`, and the
//! overlap with the stock in the project's `memory.db` (opened READ-ONLY): by the exact hash the
//! miner wrote in each row key, and by the normalised key of the stored error text.
//!
//! The payload carries counts and numbers — never an error text.

use crate::cli::context_budget::transcript_dir_for;
use crate::cli::context_cost::{merge, round_to, scan_meta};
use crate::cli::transcript_scan::{SHADOW_CAPS, ScanCaps, scan_dir};
use regex::Regex;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::OnceLock;

/// Characters of an error kept as the lesson's text — the miner's `ERROR_TEXT_MAX`.
pub const ERROR_TEXT_MAX: usize = 500;

/// How many same-tool failures a run may hold and still count as resolved — the miner's
/// `RESOLUTION_SCAN_WINDOW`.
pub const RESOLUTION_SCAN_WINDOW: usize = 3;

/// Characters of the normalised error that form the key (the reference's `PREFIX`).
pub const KEY_PREFIX_CHARS: usize = 200;

/// Distinct sessions a key needs to be `active` — TokenEconomizer's `K_PROMOTE`
/// (`microskills/lifecycle.py:15`), the number ADR-001 adopts.
pub const K_PROMOTE: u64 = 3;

/// A lesson the miner would store: the tool and the error of the FIRST failure of a resolved run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPair {
    /// The tool that failed and was then resolved.
    pub tool: String,
    /// Its error, cut at [`ERROR_TEXT_MAX`] chars.
    pub error_text: String,
}

/// The lessons one session yields.
#[derive(Debug, Clone, Default)]
pub struct SessionLessons {
    /// One per resolved failure run.
    pub pairs: Vec<RawPair>,
}

/// `s` cut at `max` chars, never inside one.
fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// A tool result's content as the miner flattens it: a string; else the `text` blocks joined with
/// nothing; else the JSON of the whole.
fn flatten_result(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(items) = v.as_array() {
        let parts: Vec<&str> = items
            .iter()
            .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .collect();
        if !parts.is_empty() {
            return parts.join("");
        }
    }
    v.to_string()
}

/// Walks one session's records and collects the resolved failure runs.
#[derive(Debug, Default)]
pub struct LessonTracker {
    /// First stream position of each tool-use id (`entry().or_insert`).
    uses: HashMap<String, usize>,
    /// Tool-use ids per tool, one per occurrence, in stream order.
    by_tool: BTreeMap<String, Vec<String>>,
    /// First result of each id: `(is_error, text)`. The text is kept only for a failure — it is
    /// all a lesson ever reads.
    results: HashMap<String, (bool, String)>,
    pos: usize,
}

impl LessonTracker {
    /// Fold one transcript record in. Only user and assistant lines with a block list are read,
    /// as the miner's `parse_transcript_line` reads them.
    pub fn observe(&mut self, rec: &Value) {
        if !matches!(
            rec.get("type").and_then(Value::as_str),
            Some("user" | "assistant")
        ) {
            return;
        }
        let Some(blocks) = rec.pointer("/message/content").and_then(Value::as_array) else {
            return;
        };
        self.pos += 1;
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => self.note_use(block),
                Some("tool_result") => self.note_result(block),
                _ => {}
            }
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

    fn note_use(&mut self, block: &Value) {
        let text = |k: &str| block.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let (id, name) = (text("id"), text("name"));
        self.uses.entry(id.clone()).or_insert(self.pos);
        self.by_tool.entry(name).or_default().push(id);
    }

    fn note_result(&mut self, block: &Value) {
        let id = block
            .get("tool_use_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let is_error = block
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let text = if is_error {
            truncate_chars(
                &block.get("content").map(flatten_result).unwrap_or_default(),
                ERROR_TEXT_MAX,
            )
        } else {
            String::new()
        };
        self.results.entry(id).or_insert((is_error, text));
    }

    /// The session's lessons: the chain scan of the miner over every tool.
    #[must_use]
    pub fn finish(self) -> SessionLessons {
        let mut pairs = Vec::new();
        for (tool, mut ids) in self.by_tool {
            ids.sort_by_key(|id| self.uses.get(id).copied().unwrap_or(0));
            let mut chain_start: Option<&str> = None;
            let mut chain_len = 0usize;
            for id in &ids {
                let Some((is_error, text)) = self.results.get(id) else {
                    chain_start = None;
                    chain_len = 0;
                    continue;
                };
                if *is_error {
                    chain_start.get_or_insert(text.as_str());
                    chain_len += 1;
                    continue;
                }
                if let Some(first) = chain_start
                    && chain_len <= RESOLUTION_SCAN_WINDOW
                {
                    pairs.push(RawPair {
                        tool: tool.clone(),
                        error_text: first.to_string(),
                    });
                }
                chain_start = None;
                chain_len = 0;
            }
        }
        SessionLessons { pairs }
    }
}

/// The normalisation, in the reference's order.
fn normalization_rules() -> &'static [(Regex, &'static str)] {
    static RULES: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    RULES.get_or_init(|| {
        [
            (r"/[\w.\-/]+", "<path>"),
            (r"0x[0-9a-f]+|\b[0-9a-f]{8,}\b", "<hex>"),
            (r"\d+", "<n>"),
            (r"\s+", " "),
        ]
        .into_iter()
        .filter_map(|(pattern, with)| Regex::new(pattern).ok().map(|re| (re, with)))
        .collect()
    })
}

/// `text` with paths, long hex and numbers replaced, whitespace collapsed and the ends trimmed.
#[must_use]
pub fn normalize_key(text: &str) -> String {
    let mut t = text.to_string();
    for (re, with) in normalization_rules() {
        t = re.replace_all(&t, *with).into_owned();
    }
    t.trim().to_string()
}

/// The key of an error: cut at 500 chars, normalised, then the first [`KEY_PREFIX_CHARS`] chars.
#[must_use]
pub fn lesson_key(text: &str) -> String {
    truncate_chars(
        &normalize_key(&truncate_chars(text, ERROR_TEXT_MAX)),
        KEY_PREFIX_CHARS,
    )
}

/// The 8 hex digits the miner puts in a lesson's row key: `DefaultHasher` over the tool and the
/// first 200 chars of the RAW error, as `dedup_key` does.
#[must_use]
pub fn raw_hash8(tool: &str, error_text: &str) -> String {
    let prefix: String = error_text.chars().take(200).collect();
    let mut hasher = DefaultHasher::new();
    tool.hash(&mut hasher);
    prefix.hash(&mut hasher);
    format!("{:08x}", hasher.finish() as u32)
}

/// The transcript-mined lessons already stored in `memory.db`.
#[derive(Debug, Clone, Default)]
pub struct StoredLessons {
    /// Rows keyed `outcome:<class>:transcript-<hash8>:failure`.
    pub lessons: u64,
    hash8: BTreeSet<String>,
    keys: BTreeSet<(String, String)>,
    unparsable: u64,
}

/// Read the stock from `path`, READ-ONLY: the connection cannot write, so nothing in the file —
/// nor beside it — changes.
///
/// # Errors
///
/// The file cannot be opened, or has no readable `memory_entries` table.
pub fn read_stored_lessons(path: &Path) -> Result<StoredLessons, String> {
    let con = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("cannot open {} read-only: {e}", path.display()))?;
    let mut stmt = con
        .prepare(
            "SELECT key, value FROM memory_entries WHERE key LIKE 'outcome:%:transcript-%:failure'",
        )
        .map_err(|e| format!("memory_entries is not readable: {e}"))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| format!("memory_entries is not readable: {e}"))?;
    let mut stored = StoredLessons::default();
    for (key, value) in rows.flatten() {
        stored.absorb_row(&key, &value);
    }
    Ok(stored)
}

/// The `(tool, error)` a stored lesson's JSON value carries.
fn parse_lesson(value: &str) -> Option<(String, String)> {
    let v: Value = serde_json::from_str(value).ok()?;
    Some((
        v.get("tool")?.as_str()?.to_string(),
        v.get("error")?.as_str()?.to_string(),
    ))
}

impl StoredLessons {
    fn absorb_row(&mut self, key: &str, value: &str) {
        self.lessons += 1;
        if let Some(h) = key.split(':').find_map(|p| p.strip_prefix("transcript-")) {
            self.hash8.insert(h.to_string());
        }
        match parse_lesson(value) {
            Some((tool, error)) => {
                self.keys.insert((tool, lesson_key(&error)));
            }
            None => self.unparsable += 1,
        }
    }
}

fn ratio(num: u64, den: u64) -> Value {
    if den > 0 {
        json!(round_to(num as f64 / den as f64, 6))
    } else {
        Value::Null
    }
}

/// `mcp__*` tools are one class; every other tool is its own.
fn tool_class(tool: &str) -> &str {
    if tool.starts_with("mcp__") { "mcp" } else { tool }
}

type KeyIndex<'a> = BTreeMap<(String, String), BTreeSet<&'a str>>;

fn bucket(sessions: usize) -> &'static str {
    match sessions {
        0 | 1 => "1",
        2 => "2",
        3 => "3",
        4 => "4",
        _ => "5+",
    }
}

fn distribution_json(keys: &KeyIndex<'_>) -> Value {
    let mut dist: BTreeMap<&str, u64> = ["1", "2", "3", "4", "5+"].into_iter().map(|b| (b, 0)).collect();
    for sessions in keys.values() {
        *dist.entry(bucket(sessions.len())).or_default() += 1;
    }
    json!(dist)
}

fn by_tool_json(keys: &KeyIndex<'_>) -> Value {
    let mut classes: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    for ((tool, _), sessions) in keys {
        let e = classes.entry(tool_class(tool)).or_default();
        e.0 += 1;
        e.1 += u64::from(sessions.len() as u64 >= K_PROMOTE);
    }
    let mut ranked: Vec<_> = classes.into_iter().collect();
    ranked.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(b.0)));
    Value::Object(
        ranked
            .into_iter()
            .take(8)
            .map(|(class, (n, active))| (class.to_string(), json!({"keys": n, "active": active})))
            .collect(),
    )
}

fn memory_db_json(
    stored: &Result<StoredLessons, String>,
    keys: &KeyIndex<'_>,
    raw: &BTreeSet<String>,
) -> Value {
    let stored = match stored {
        Ok(s) => s,
        Err(reason) => return json!({"available": false, "reason": reason}),
    };
    let active = |s: &BTreeSet<&str>| s.len() as u64 >= K_PROMOTE;
    let in_db = |k: &(String, String)| stored.keys.contains(k);
    let corpus_in_db = keys.keys().filter(|k| in_db(k)).count() as u64;
    let active_total = keys.values().filter(|s| active(s)).count() as u64;
    let active_in_db = keys.iter().filter(|(k, s)| active(s) && in_db(k)).count() as u64;
    let raw_in_db = raw.intersection(&stored.hash8).count() as u64;
    json!({
        "available": true,
        "source": "the project's memory.db, opened read-only",
        "lessons": stored.lessons,
        "normalized_keys": stored.keys.len(),
        "lessons_per_key": ratio(stored.lessons, stored.keys.len() as u64),
        "unparsable_rows": stored.unparsable,
        "raw_overlap": {
            "corpus_raw_keys": raw.len(),
            "in_db": raw_in_db,
            "share": ratio(raw_in_db, raw.len() as u64),
        },
        "key_overlap": {
            "corpus_keys": keys.len(),
            "in_db": corpus_in_db,
            "share": ratio(corpus_in_db, keys.len() as u64),
            "active_total": active_total,
            "active_in_db": active_in_db,
            "candidate_in_db": corpus_in_db - active_in_db,
        },
        "method": "raw_overlap: the corpus pair's dedup hash against the hash in the stored row key (exact); key_overlap: the normalized key of the corpus pair against the normalized key of the stored error text (redaction can shift a key, so it undercounts)",
    })
}

/// The `lesson_lifecycle` payload over `sessions` (id, lessons) and the stock in `stored`.
#[must_use]
pub fn lesson_report(
    sessions: &[(String, SessionLessons)],
    stored: &Result<StoredLessons, String>,
) -> Value {
    let mut keys: KeyIndex<'_> = BTreeMap::new();
    let mut raw: BTreeSet<String> = BTreeSet::new();
    let mut pairs_total = 0u64;
    for (id, s) in sessions {
        for p in &s.pairs {
            pairs_total += 1;
            keys.entry((p.tool.clone(), lesson_key(&p.error_text)))
                .or_default()
                .insert(id.as_str());
            raw.insert(raw_hash8(&p.tool, &p.error_text));
        }
    }
    let total = keys.len() as u64;
    let active = keys.values().filter(|s| s.len() as u64 >= K_PROMOTE).count() as u64;
    let single = keys.values().filter(|s| s.len() == 1).count() as u64;
    json!({
        "sessions": sessions.len(),
        "pairs_total": pairs_total,
        "distinct_keys": total,
        "keys_by_distinct_sessions": distribution_json(&keys),
        "lifecycle": {
            "k_promote": K_PROMOTE,
            "active": active,
            "candidate": total - active,
            "single_session": single,
            "share_active": ratio(active, total),
            "share_single_session": ratio(single, total),
        },
        "by_tool": by_tool_json(&keys),
        "memory_db": memory_db_json(stored, &keys, &raw),
        "key": {
            "normalization": ["path", "hex", "number", "whitespace"],
            "prefix_chars": KEY_PREFIX_CHARS,
            "error_text_max": ERROR_TEXT_MAX,
        },
        "method": "pairs: a port of the transcript miner's error->resolution extractor; key: tool + the first 200 chars of the normalized error; a key's sessions are the DISTINCT transcripts it appears in; mode: shadow, nothing is written",
    })
}

/// The `lesson_lifecycle` section for `project_root`, under the shadow caps.
#[must_use]
pub fn lesson_lifecycle(project_root: &Path) -> Value {
    let Some(home) = std::env::var_os("HOME") else {
        return json!({"available": false, "reason": "HOME unset"});
    };
    lesson_lifecycle_in(Path::new(&home), project_root, &SHADOW_CAPS)
}

/// [`lesson_lifecycle`] against an explicit `home` and `caps` — the testable body.
#[must_use]
pub fn lesson_lifecycle_in(home: &Path, project_root: &Path, caps: &ScanCaps) -> Value {
    let dir = transcript_dir_for(home, project_root);
    let scan = match scan_dir::<LessonTracker>(&dir, caps, &mut |t, rec| t.observe(rec)) {
        Ok(scan) => scan,
        Err(reason) => return json!({"available": false, "reason": reason}),
    };
    let malformed: u64 = scan.sessions.iter().map(|s| s.malformed_lines).sum();
    let meta = scan_meta(
        scan.sessions.len(),
        scan.bytes_scanned,
        scan.capped,
        scan.skipped_by_cap,
        malformed,
    );
    let sessions: Vec<(String, SessionLessons)> = scan
        .sessions
        .into_iter()
        .map(|s| (s.id, s.state.finish()))
        .collect();
    let stored = read_stored_lessons(&project_root.join(".claude/touring/memory.db"));
    let mut out = lesson_report(&sessions, &stored);
    merge(&mut out, meta);
    out
}

#[cfg(test)]
#[path = "lesson_lifecycle_tests.rs"]
mod tests;
