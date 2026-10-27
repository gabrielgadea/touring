//! Tool-channel trio measured AT EMISSION (S-11 / D1 of the janela-eficacia plan).
//!
//! The trio — efficacy (`erro`/`vazio`/`negado` per tool class), efficiency
//! (byte-identical `retry` plus the result-size distribution) and effectiveness
//! (does a gate denial lead to a recovered next call?) — used to exist only as an
//! offline transcript replay (`scripts/measure_tool_trio.py`). This module brings
//! the SAME predicates into the hook path so the numbers exist live, per session,
//! in `touring gate-metrics -j` under the `tool_trio` key.
//!
//! # One source of predicates (rule D8)
//!
//! [`classify_bash`], [`tool_class`], [`outcome`] and the retry window are a
//! line-by-line port of the Python script (same regexes, same verb tables). Parity
//! is proven by the 9 ported fixtures plus a shared replay fixture
//! (`scripts/fixtures/tool_trio_parity.jsonl`) that BOTH the Python test and the Rust
//! test check against one `expected` file.
//!
//! # Where the events come from
//!
//! * `PostToolUse` (`post-tool-rl`, matcher `*`) and `PostToolUseFailure`
//!   (`post-tool-failure`) → [`record_completed_call`].
//! * A Touring gate denial in the `cli-suggest` PreToolUse path →
//!   [`observe_pre_call`], which also classifies the call that FOLLOWS a denial.
//!
//! # State and bounds
//!
//! One process-global ledger, like the other gate-metrics counters (the hooks run
//! inside the daemon; the counters reset with it and are captured by the periodic
//! KPI snapshot). Memory is bounded: at most 256 tool classes (the rest fold into
//! `(outras)`), 32 tracked sessions (oldest evicted), 32 gate rows, and per session
//! an 8-call window, a 16-id dedupe ring and one pending denial. Hot-path cost is
//! one mutex acquisition plus the per-call classification; no I/O.
//!
//! # Honest limits
//!
//! * Denied calls never reach `PostToolUse`, so `negado` is counted when the denial
//!   is emitted, and only for Touring `cli-suggest` gates (G1..G11); denials issued
//!   by other PreToolUse hooks are invisible here.
//! * `erro` needs the `PostToolUseFailure` hook: tools outside its registered
//!   matcher have failures that are ABSENT, not zero (see [`SCOPE_NOTES`]).
//! * `tool_response` is the structured form, not the text the model sees; sizes are
//!   exact for Bash/MCP text, `content + 7 B/line` for Read, and a synthetic
//!   one-line message for Edit/Write (the model sees that line, not `originalFile`).

use regex::Regex;
use rustc_hash::FxHasher;
use serde::Serialize;
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::hash::Hasher;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

/// Retry window: a retry is a byte-identical call within the last `WINDOW` calls of
/// the session — the predicate of gate G6 (`measure_tool_trio.py::WINDOW`).
const WINDOW: usize = 8;
/// A result shorter than this many bytes (after trimming) is `vazio`.
const EMPTY_BYTES: usize = 50;
/// Distinct tool classes kept; the rest fold into [`OVERFLOW_CLASS`].
const MAX_CLASSES: usize = 256;
/// Sessions tracked at once (windows + pending denial); the oldest is evicted.
const MAX_SESSIONS: usize = 32;
/// Gate rows kept; the rest fold into `other`.
const MAX_GATES: usize = 32;
/// Class labels are cut to this many characters to bound memory.
const MAX_LABEL_CHARS: usize = 64;
/// Recent `tool_use_id` hashes kept per session to drop duplicate deliveries.
const ID_RING: usize = 16;
/// Row that absorbs classes past [`MAX_CLASSES`].
const OVERFLOW_CLASS: &str = "(outras)";
/// Bytes of the `cat -n` prefix (`{:>6}\t`) the model sees on every Read line.
const READ_LINE_PREFIX_BYTES: usize = 7;
/// "Big" result threshold for the `gt_10kb_frac` column: 10 KB = 10 000 bytes, the
/// same cut as `size_summary` in `scripts/measure_tool_trio.py` (`x > 10_000`).
const BIG_RESULT_BYTES: usize = 10_000;
/// Upper edges (bytes, inclusive) of the fixed result-size buckets; one open bucket
/// follows. The 7th edge IS [`BIG_RESULT_BYTES`], so "> 10 KB" is an exact bucket sum.
const SIZE_EDGES: [usize; 13] = [
    64, 256, 1_024, 2_048, 4_096, 8_192, 10_000, 16_384, 32_768, 65_536, 131_072, 262_144,
    1_048_576,
];
/// Buckets = the 13 closed ones plus the open tail.
const N_BUCKETS: usize = SIZE_EDGES.len() + 1;
/// First bucket whose sizes are strictly above [`BIG_RESULT_BYTES`].
const BIG_FROM_BUCKET: usize = 7;
const _: () = assert!(SIZE_EDGES[BIG_FROM_BUCKET - 1] == BIG_RESULT_BYTES);

/// Verbs that only set the stage for the real command (`cd x && cargo test`).
const PREFIX_VERBS: [&str; 10] = [
    "cd", "export", "set", "source", ".", "pushd", "popd", "true", "unset", "shopt",
];
/// Wrappers peeled off the front of a segment.
const WRAPPERS: [&str; 6] = ["env", "time", "nice", "nohup", "command", "exec"];
/// Section-header verbs: the verb only when nothing else runs.
const DECORATIVE_VERBS: [&str; 2] = ["echo", "printf"];
/// Verbs whose `exit 1` with no output means "no match", not failure.
const NO_MATCH_VERBS: [&str; 5] = ["grep", "egrep", "fgrep", "rg", "ugrep"];
/// Verbs whose first non-flag argument is part of the class label.
const SUBCOMMAND_VERBS: [&str; 4] = ["touring", "git", "cargo", "scripts/hw-slot"];

/// Caveats shipped with every snapshot so a reader cannot mistake absence for zero.
const SCOPE_NOTES: [&str; 4] = [
    "calls = PostToolUse/PostToolUseFailure events seen by this daemon plus Touring cli-suggest gate denials; standalone hook runs (daemon down) are not counted",
    "erro is observed only for tools whose PostToolUseFailure hook is registered (settings.json matcher): other tools' failures are absent, not zero",
    "negado counts Touring cli-suggest gate denials (G1..G11) only; denials by other PreToolUse hooks are not visible here",
    "result sizes: Bash/MCP text exact; Read = content + 7 B/line; Edit/Write = synthetic one-line message; other objects = JSON length",
];

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static tool_trio pattern is valid")
}

static SEGMENT_SPLIT: LazyLock<Regex> = LazyLock::new(|| re(r"&&|\|\||;|\n"));
static ASSIGN: LazyLock<Regex> = LazyLock::new(|| re(r"^[A-Za-z_][A-Za-z0-9_]*="));
static DURATION: LazyLock<Regex> = LazyLock::new(|| re(r"^\d+[smh]?$"));
static CAT_WRITE: LazyLock<Regex> = LazyLock::new(|| re(r"^cat\b[^|]*(?:>|<<)"));
static HOOK_DENIAL: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)hook error|PreToolUse:\w+ hook|blocked by .*hook"));
static EXIT_ONE_ONLY: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*(?:Exit code 1)?\s*$"));

static NULL: Value = Value::Null;

// ─── Predicates (port of scripts/measure_tool_trio.py) ───────────────────────

/// Tokens of one shell segment with assignments, wrappers and `timeout N` peeled.
fn first_tokens(segment: &str) -> Vec<&str> {
    let toks: Vec<&str> = segment.split_whitespace().collect();
    let mut at = 0usize;
    while let Some(&head) = toks.get(at) {
        if ASSIGN.is_match(head) || WRAPPERS.contains(&head) {
            at += 1;
        } else if head == "timeout" {
            at += 1;
            while toks
                .get(at)
                .is_some_and(|t| t.starts_with('-') || DURATION.is_match(t))
            {
                at += 1;
            }
        } else {
            break;
        }
    }
    toks[at..].to_vec()
}

/// Effective verb of a Bash command (the action, not its prefix).
///
/// `echo`/`printf` lead many multi-step scripts as section headers; they are the
/// verb only when nothing else runs.
fn classify_bash(command: &str) -> String {
    let segments: Vec<Vec<&str>> = SEGMENT_SPLIT
        .split(command)
        .map(first_tokens)
        .filter(|t| {
            t.first()
                .is_some_and(|h| !PREFIX_VERBS.contains(h) && !h.starts_with('#'))
        })
        .collect();
    let chosen = segments
        .iter()
        .find(|t| !DECORATIVE_VERBS.contains(&t[0]))
        .or_else(|| segments.first());
    chosen.map_or_else(|| "(prefixo-só)".to_string(), |toks| verb_of(toks))
}

/// Class label of one peeled segment: `cat` split by direction, subcommand kept.
fn verb_of(toks: &[&str]) -> String {
    let head = toks[0];
    let verb = if head.contains('/') && head != "scripts/hw-slot" {
        head.rsplit('/').next().unwrap_or(head)
    } else {
        head
    };
    if verb == "cat" {
        return if CAT_WRITE.is_match(&toks.join(" ")) {
            "cat-write"
        } else {
            "cat-read"
        }
        .to_string();
    }
    if SUBCOMMAND_VERBS.contains(&verb) && toks.len() > 1 && !toks[1].starts_with('-') {
        return format!("{verb} {}", toks[1]);
    }
    verb.to_string()
}

/// Class of one tool call: `Bash:<effective verb>` for Bash, the tool name otherwise.
fn tool_class(name: &str, input: &Value) -> String {
    if name == "Bash" {
        let command = input.get("command").and_then(Value::as_str).unwrap_or("");
        format!("Bash:{}", classify_bash(command))
    } else {
        name.to_string()
    }
}

/// Outcome of one call, in the vocabulary of the offline script.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    /// A hook denied the call: not a tool failure.
    Negado,
    /// The tool failed.
    Erro,
    /// The tool succeeded with (almost) no content.
    Vazio,
    /// The tool succeeded with content.
    Ok,
}

/// Classify one result. `extra_bytes` are model-visible bytes not present in `text`
/// (the Read line-number prefixes, an image payload); they count towards emptiness.
fn outcome(class: &str, text: &str, extra_bytes: usize, is_error: bool) -> Outcome {
    let stripped = text.trim();
    if is_error && HOOK_DENIAL.is_match(text) {
        return Outcome::Negado;
    }
    let verb = class.strip_prefix("Bash:").unwrap_or("");
    if is_error && NO_MATCH_VERBS.contains(&verb) && EXIT_ONE_ONLY.is_match(stripped) {
        return Outcome::Vazio;
    }
    if is_error {
        return Outcome::Erro;
    }
    if class == "Bash:cat-write" {
        return Outcome::Ok; // a heredoc write prints nothing on success
    }
    let sentinel = extra_bytes == 0 && matches!(stripped, "[]" | "{}" | "null" | "(No content)");
    if stripped.len() + extra_bytes < EMPTY_BYTES || sentinel {
        return Outcome::Vazio;
    }
    Outcome::Ok
}

/// Join the blocks of a content array the way `result_text` does in Python: text
/// blocks verbatim, every other block (tool_reference, image) as its JSON.
fn blocks_text(blocks: &[Value]) -> String {
    let mut out = String::new();
    for block in blocks {
        if !block.is_object() {
            continue;
        }
        match block.get("text").and_then(Value::as_str) {
            Some(text) => out.push_str(text),
            None => out.push_str(&block.to_string()),
        }
    }
    out
}

/// Result text plus the model-visible bytes that are not in it.
struct ResultView<'a> {
    text: Cow<'a, str>,
    extra_bytes: usize,
}

impl<'a> ResultView<'a> {
    fn borrowed(text: &'a str) -> Self {
        Self {
            text: Cow::Borrowed(text),
            extra_bytes: 0,
        }
    }

    fn owned(text: String) -> Self {
        Self {
            text: Cow::Owned(text),
            extra_bytes: 0,
        }
    }

    fn opaque(bytes: usize) -> Self {
        Self {
            text: Cow::Borrowed(""),
            extra_bytes: bytes,
        }
    }

    fn bytes(&self) -> usize {
        self.text.len() + self.extra_bytes
    }
}

/// `io::Write` sink that only counts bytes (sizes an object without allocating it).
struct CountingWriter(usize);

impl std::io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn json_len(value: &impl Serialize) -> usize {
    let mut sink = CountingWriter(0);
    // A counting sink cannot fail; on the impossible error the size is just 0.
    let _ = serde_json::to_writer(&mut sink, value);
    sink.0
}

fn str_field<'a>(map: &'a Map<String, Value>, key: &str) -> &'a str {
    map.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Bash: stdout and stderr, joined the way `post_bash` reads them. A command moved to the
/// background has empty output but the model is told where it went (never `vazio`).
fn bash_view(map: &Map<String, Value>) -> Option<ResultView<'_>> {
    if !map.contains_key("stdout") && !map.contains_key("stderr") {
        return None;
    }
    let (out, err) = (str_field(map, "stdout"), str_field(map, "stderr"));
    if let Some(task) = map.get("backgroundTaskId").and_then(Value::as_str)
        && out.is_empty()
        && err.is_empty()
    {
        return Some(ResultView::owned(format!(
            "Command running in background with ID: {task}. Output is being written to: {task}.output"
        )));
    }
    Some(match (out.is_empty(), err.is_empty()) {
        (_, true) => ResultView::borrowed(out),
        (true, false) => ResultView::borrowed(err),
        (false, false) => ResultView::owned(format!("{out}\n{err}")),
    })
}

/// Read: raw content plus the `cat -n` prefix the model sees; an image counts as payload.
fn read_view(map: &Map<String, Value>) -> Option<ResultView<'_>> {
    let file = map.get("file")?.as_object()?;
    if let Some(content) = file.get("content").and_then(Value::as_str) {
        let lines = file
            .get("numLines")
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or_else(|| content.lines().count());
        return Some(ResultView {
            text: Cow::Borrowed(content),
            extra_bytes: lines * READ_LINE_PREFIX_BYTES,
        });
    }
    let payload = file.get("base64").and_then(Value::as_str)?;
    Some(ResultView::opaque(payload.len()))
}

/// Edit/Write echo the whole file in `originalFile`; the model sees one line.
fn mutation_view(map: &Map<String, Value>) -> Option<ResultView<'_>> {
    let path = map.get("filePath")?.as_str()?;
    let created = map.get("type").and_then(Value::as_str) == Some("create");
    Some(ResultView::owned(if created {
        format!("File created successfully at: {path}")
    } else {
        format!("The file {path} has been updated successfully.")
    }))
}

/// Wrappers that carry the text under `content` (a string or a block array), or under
/// `listing` (ListAgents repeats the same listing again as `sections`).
fn content_view(map: &Map<String, Value>) -> Option<ResultView<'_>> {
    if let Some(Value::String(text)) = map.get("listing") {
        return Some(ResultView::borrowed(text));
    }
    match map.get("content")? {
        Value::String(text) => Some(ResultView::borrowed(text)),
        Value::Array(blocks) => Some(ResultView::owned(blocks_text(blocks))),
        _ => None,
    }
}

/// JSON length of an unknown response shape, minus the fields that merely echo the
/// call's own input (an Agent launch repeats its whole `prompt`, AskUserQuestion its
/// `questions`): the model already holds those bytes, the result does not add them.
fn opaque_len(map: &Map<String, Value>, input: &Value) -> usize {
    let echoed: usize = map
        .iter()
        .filter(|(key, value)| input.get(key.as_str()) == Some(*value))
        .map(|(_, value)| json_len(value))
        .sum();
    json_len(map).saturating_sub(echoed)
}

/// The model-visible shape of a structured `tool_response` object; an unknown shape is
/// sized by its JSON length minus the input echo (counted, never allocated).
fn object_view<'a>(map: &'a Map<String, Value>, input: &Value) -> ResultView<'a> {
    bash_view(map)
        .or_else(|| read_view(map))
        .or_else(|| mutation_view(map))
        .or_else(|| content_view(map))
        .unwrap_or_else(|| ResultView::opaque(opaque_len(map, input)))
}

/// The model-visible result of a `PostToolUse` `tool_response` of a call with `input`.
fn response_view<'a>(response: Option<&'a Value>, input: &Value) -> ResultView<'a> {
    match response {
        None | Some(Value::Null) => ResultView::borrowed(""),
        Some(Value::String(text)) => ResultView::borrowed(text),
        Some(Value::Array(blocks)) => ResultView::owned(blocks_text(blocks)),
        Some(Value::Object(map)) => object_view(map, input),
        Some(other) => ResultView::owned(other.to_string()),
    }
}

// ─── Signatures and keys ─────────────────────────────────────────────────────

fn hash_value(h: &mut FxHasher, value: &Value) {
    match value {
        Value::Null => h.write_u8(0),
        Value::Bool(b) => {
            h.write_u8(1);
            h.write_u8(u8::from(*b));
        }
        Value::Number(n) => {
            h.write_u8(2);
            if let Some(u) = n.as_u64() {
                h.write_u64(u);
            } else if let Some(i) = n.as_i64() {
                h.write_i64(i);
            } else {
                h.write_u64(n.as_f64().unwrap_or(0.0).to_bits());
            }
        }
        Value::String(s) => {
            h.write_u8(3);
            h.write_usize(s.len());
            h.write(s.as_bytes());
        }
        Value::Array(items) => {
            h.write_u8(4);
            h.write_usize(items.len());
            for item in items {
                hash_value(h, item);
            }
        }
        Value::Object(map) => {
            h.write_u8(5);
            h.write_usize(map.len());
            // Sorted keys: the canonical form, whatever order the payload arrived in.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            for key in keys {
                h.write_usize(key.len());
                h.write(key.as_bytes());
                if let Some(v) = map.get(key) {
                    hash_value(h, v);
                }
            }
        }
    }
}

/// 64-bit signature of one call: the tool plus its canonical input. Two calls are a
/// retry exactly when their signatures are equal (byte-identical canonical JSON).
fn call_signature(tool: &str, input: &Value) -> u64 {
    let mut h = FxHasher::default();
    h.write(tool.as_bytes());
    h.write_u8(0);
    hash_value(&mut h, input);
    h.finish()
}

fn hash_str(s: &str) -> u64 {
    let mut h = FxHasher::default();
    h.write(s.as_bytes());
    h.finish()
}

/// Session key of a hook payload: the session id, plus the agent id for subagents
/// (a subagent must not share its parent's windows). Same rule as the cli-suggest
/// gates, so a denial and the call that follows it land in the same session.
pub fn session_key(payload: &Value) -> String {
    let session = payload
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("anon");
    match payload
        .get("agent_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(agent) => format!("{session}\u{2}agent:{agent}"),
        None => session.to_string(),
    }
}

/// Stable tag of a gate from its denial reason: `[G10 exec-burst] …` → `G10`,
/// anything without the `[G<n> …]` prefix → `other`.
fn gate_tag(reason: &str) -> String {
    reason
        .strip_prefix("[G")
        .and_then(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            (!digits.is_empty()).then(|| format!("G{digits}"))
        })
        .unwrap_or_else(|| "other".to_string())
}

/// Gate tag (`G1`, `G10`, `G11`, …) when `response` — a PreToolUse hook output — is a
/// denial; `None` for allows, rewrites (`updatedInput`) and advisories.
pub fn deny_gate_of(response: &str) -> Option<String> {
    // Cheap reject before the parse: every denial spells the literal `"deny"` or `"block"`.
    if !response.contains("\"deny\"") && !response.contains("\"block\"") {
        return None;
    }
    let value: Value = serde_json::from_str(response).ok()?;
    if let Some(out) = value.get("hookSpecificOutput")
        && out.get("permissionDecision").and_then(Value::as_str) == Some("deny")
    {
        let reason = out
            .get("permissionDecisionReason")
            .and_then(Value::as_str)
            .unwrap_or("");
        return Some(gate_tag(reason));
    }
    if value.get("decision").and_then(Value::as_str) == Some("block") {
        return Some(gate_tag(
            value.get("reason").and_then(Value::as_str).unwrap_or(""),
        ));
    }
    None
}

// ─── Ledger ──────────────────────────────────────────────────────────────────

/// Where a gate denial went on the next call of the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Recovery {
    /// A different call that was not denied.
    Recovered,
    /// The same call again.
    Repeated,
    /// A different call, denied again.
    DeniedAgain,
}

#[derive(Default)]
struct ClassStats {
    calls: u64,
    retry: u64,
    erro: u64,
    vazio: u64,
    negado: u64,
    ok: u64,
    result_bytes: u64,
    max_bytes: u64,
    sizes: [u64; N_BUCKETS],
}

impl ClassStats {
    fn record(&mut self, outcome: Outcome, retry: bool, bytes: Option<usize>) {
        self.calls += 1;
        if retry {
            self.retry += 1;
        }
        match outcome {
            Outcome::Negado => self.negado += 1,
            Outcome::Erro => self.erro += 1,
            Outcome::Vazio => self.vazio += 1,
            Outcome::Ok => self.ok += 1,
        }
        if let Some(bytes) = bytes {
            let bytes = bytes as u64;
            self.result_bytes += bytes;
            self.max_bytes = self.max_bytes.max(bytes);
            self.sizes[bucket_of(bytes)] += 1;
        }
    }
}

fn bucket_of(bytes: u64) -> usize {
    SIZE_EDGES
        .iter()
        .position(|edge| bytes <= *edge as u64)
        .unwrap_or(N_BUCKETS - 1)
}

#[derive(Default)]
struct GateStats {
    denied: u64,
    recovered: u64,
    repeated: u64,
    denied_again: u64,
}

impl GateStats {
    fn bump(&mut self, recovery: Recovery) {
        match recovery {
            Recovery::Recovered => self.recovered += 1,
            Recovery::Repeated => self.repeated += 1,
            Recovery::DeniedAgain => self.denied_again += 1,
        }
    }
}

struct PendingDeny {
    gate: String,
    sig: u64,
}

#[derive(Default)]
struct SessionState {
    window: VecDeque<u64>,
    ids: VecDeque<u64>,
    pending: Option<PendingDeny>,
    last_seen: u64,
}

fn push_capped(queue: &mut VecDeque<u64>, value: u64, cap: usize) {
    if queue.len() >= cap {
        queue.pop_front();
    }
    queue.push_back(value);
}

fn session_entry<'a>(
    sessions: &'a mut HashMap<String, SessionState>,
    key: &str,
    tick: u64,
) -> &'a mut SessionState {
    if sessions.len() >= MAX_SESSIONS && !sessions.contains_key(key) {
        let oldest = sessions
            .iter()
            .min_by_key(|(_, s)| s.last_seen)
            .map(|(k, _)| k.clone());
        if let Some(oldest) = oldest {
            sessions.remove(&oldest);
        }
    }
    let state = sessions.entry(key.to_string()).or_default();
    state.last_seen = tick;
    state
}

fn truncate_label(label: String) -> String {
    if label.chars().count() <= MAX_LABEL_CHARS {
        label
    } else {
        label.chars().take(MAX_LABEL_CHARS).collect()
    }
}

/// The bounded in-memory state behind the `tool_trio` key.
#[derive(Default)]
struct TrioLedger {
    classes: HashMap<String, ClassStats>,
    gates: HashMap<String, GateStats>,
    sessions: HashMap<String, SessionState>,
    tick: u64,
    total_calls: u64,
}

impl TrioLedger {
    fn class_entry(&mut self, class: String) -> &mut ClassStats {
        let label = truncate_label(class);
        let key = if self.classes.contains_key(&label) || self.classes.len() < MAX_CLASSES {
            label
        } else {
            OVERFLOW_CLASS.to_string()
        };
        self.classes.entry(key).or_default()
    }

    fn gate_entry(&mut self, gate: String) -> &mut GateStats {
        let key = if self.gates.contains_key(&gate) || self.gates.len() < MAX_GATES {
            gate
        } else {
            "other".to_string()
        };
        self.gates.entry(key).or_default()
    }

    fn has_pending(&self, session: &str) -> bool {
        self.sessions
            .get(session)
            .is_some_and(|s| s.pending.is_some())
    }

    /// One call finished (success or failure). `id` drops duplicate deliveries of the
    /// same `tool_use_id`; a pending denial of the session is resolved here when no
    /// PreToolUse event saw the call (tools outside the `cli-suggest` matcher).
    fn on_completed(
        &mut self,
        session: &str,
        class: String,
        sig: u64,
        id: Option<u64>,
        outcome: Outcome,
        bytes: usize,
    ) {
        self.tick += 1;
        let tick = self.tick;
        let sess = session_entry(&mut self.sessions, session, tick);
        if let Some(id) = id {
            if sess.ids.contains(&id) {
                return;
            }
            push_capped(&mut sess.ids, id, ID_RING);
        }
        let retry = sess.window.contains(&sig);
        push_capped(&mut sess.window, sig, WINDOW);
        let resolved = sess.pending.take().map(|p| {
            let how = if p.sig == sig {
                Recovery::Repeated
            } else {
                Recovery::Recovered
            };
            (p.gate, how)
        });
        if let Some((gate, how)) = resolved {
            self.gate_entry(gate).bump(how);
        }
        self.class_entry(class).record(outcome, retry, Some(bytes));
        self.total_calls += 1;
    }

    /// One PreToolUse event of the `cli-suggest` path. Classifies the call that
    /// follows a pending denial and, when this call was itself denied by `gate`,
    /// arms the next classification and counts the denied call as `negado`.
    fn on_pre(&mut self, session: &str, sig: u64, denied: Option<(String, String)>) {
        self.tick += 1;
        let tick = self.tick;
        let sess = session_entry(&mut self.sessions, session, tick);
        let prior = sess.pending.take();
        let denied_now = denied.is_some();
        let mut deny_row = None;
        if let Some((gate, class)) = denied {
            let retry = sess.window.contains(&sig);
            push_capped(&mut sess.window, sig, WINDOW);
            sess.pending = Some(PendingDeny {
                gate: gate.clone(),
                sig,
            });
            deny_row = Some((gate, class, retry));
        }
        if let Some(prior) = prior {
            let how = if prior.sig == sig {
                Recovery::Repeated
            } else if denied_now {
                Recovery::DeniedAgain
            } else {
                Recovery::Recovered
            };
            self.gate_entry(prior.gate).bump(how);
        }
        if let Some((gate, class, retry)) = deny_row {
            self.gate_entry(gate).denied += 1;
            self.class_entry(class).record(Outcome::Negado, retry, None);
            self.total_calls += 1;
        }
    }

    fn snapshot(&self) -> Snapshot {
        let classes = self
            .classes
            .iter()
            .map(|(name, s)| (name.clone(), ClassRow::from_stats(s)))
            .collect();
        let deny_recovery = self
            .gates
            .iter()
            .map(|(name, g)| (name.clone(), GateRow::from_stats(g)))
            .collect();
        Snapshot {
            window: WINDOW,
            empty_bytes: EMPTY_BYTES,
            big_result_bytes: BIG_RESULT_BYTES,
            total_calls: self.total_calls,
            sessions_tracked: self.sessions.len(),
            classes_tracked: self.classes.len(),
            classes,
            deny_recovery,
            scope: SCOPE_NOTES.to_vec(),
        }
    }
}

// ─── Snapshot (serialization) ────────────────────────────────────────────────

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn pct(part: u64, whole: u64) -> f64 {
    round1(100.0 * part as f64 / whole.max(1) as f64)
}

/// Upper bound of the bucket holding quantile `q`, clamped to the largest result
/// seen; `None` when no result was measured (absence of signal is not zero).
fn quantile_le(buckets: &[u64; N_BUCKETS], max_bytes: u64, q: f64) -> Option<u64> {
    let n: u64 = buckets.iter().sum();
    if n == 0 {
        return None;
    }
    let target = ((q * n as f64).ceil() as u64).max(1);
    let mut seen = 0u64;
    for (i, count) in buckets.iter().enumerate() {
        seen += count;
        if seen >= target {
            return Some(
                SIZE_EDGES
                    .get(i)
                    .map_or(max_bytes, |e| (*e as u64).min(max_bytes)),
            );
        }
    }
    Some(max_bytes)
}

#[derive(Serialize)]
struct SizeRow {
    samples: u64,
    p50_le_bytes: Option<u64>,
    p95_le_bytes: Option<u64>,
    max_bytes: u64,
    gt_10kb_frac: Option<f64>,
}

#[derive(Serialize)]
struct ClassRow {
    calls: u64,
    erro: u64,
    vazio: u64,
    negado: u64,
    ok: u64,
    retry: u64,
    erro_pct: f64,
    vazio_pct: f64,
    negado_pct: f64,
    retry_pct: f64,
    eficacia_pct: f64,
    result_bytes: u64,
    size: SizeRow,
}

impl ClassRow {
    fn from_stats(s: &ClassStats) -> Self {
        let (erro_pct, vazio_pct, negado_pct) = (
            pct(s.erro, s.calls),
            pct(s.vazio, s.calls),
            pct(s.negado, s.calls),
        );
        let samples: u64 = s.sizes.iter().sum();
        let big: u64 = s.sizes[BIG_FROM_BUCKET..].iter().sum();
        Self {
            calls: s.calls,
            erro: s.erro,
            vazio: s.vazio,
            negado: s.negado,
            ok: s.ok,
            retry: s.retry,
            erro_pct,
            vazio_pct,
            negado_pct,
            retry_pct: pct(s.retry, s.calls),
            eficacia_pct: round1(100.0 - erro_pct - vazio_pct - negado_pct),
            result_bytes: s.result_bytes,
            size: SizeRow {
                samples,
                p50_le_bytes: quantile_le(&s.sizes, s.max_bytes, 0.50),
                p95_le_bytes: quantile_le(&s.sizes, s.max_bytes, 0.95),
                max_bytes: s.max_bytes,
                gt_10kb_frac: (samples > 0)
                    .then(|| (big as f64 / samples as f64 * 1000.0).round() / 1000.0),
            },
        }
    }
}

#[derive(Serialize)]
struct GateRow {
    denied: u64,
    recovered: u64,
    repeated: u64,
    denied_again: u64,
    resolved: u64,
    unresolved: u64,
    recovery_rate: Option<f64>,
}

impl GateRow {
    fn from_stats(g: &GateStats) -> Self {
        let resolved = g.recovered + g.repeated + g.denied_again;
        Self {
            denied: g.denied,
            recovered: g.recovered,
            repeated: g.repeated,
            denied_again: g.denied_again,
            resolved,
            unresolved: g.denied.saturating_sub(resolved),
            recovery_rate: (resolved > 0)
                .then(|| (g.recovered as f64 / resolved as f64 * 1000.0).round() / 1000.0),
        }
    }
}

#[derive(Serialize)]
struct Snapshot {
    window: usize,
    empty_bytes: usize,
    big_result_bytes: usize,
    total_calls: u64,
    sessions_tracked: usize,
    classes_tracked: usize,
    classes: BTreeMap<String, ClassRow>,
    deny_recovery: BTreeMap<String, GateRow>,
    scope: Vec<&'static str>,
}

// ─── Process-global entry points ─────────────────────────────────────────────

static LEDGER: LazyLock<Mutex<TrioLedger>> = LazyLock::new(|| Mutex::new(TrioLedger::default()));

fn ledger() -> MutexGuard<'static, TrioLedger> {
    LEDGER.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Record one finished tool call from a `PostToolUse` (`failed = false`) or
/// `PostToolUseFailure` (`failed = true`) payload. Fail-open and best-effort: a
/// payload without a tool name is ignored, a duplicate `tool_use_id` is dropped.
pub fn record_completed_call(payload: &Value, failed: bool) {
    let Some(tool) = payload
        .get("tool_name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let input = payload.get("tool_input").unwrap_or(&NULL);
    let class = tool_class(tool, input);
    let sig = call_signature(tool, input);
    let id = payload
        .get("tool_use_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(hash_str);
    let view = if failed {
        ResultView::borrowed(payload.get("error").and_then(Value::as_str).unwrap_or(""))
    } else {
        response_view(payload.get("tool_response"), input)
    };
    let result = outcome(&class, &view.text, view.extra_bytes, failed);
    let bytes = view.bytes();
    let session = session_key(payload);
    ledger().on_completed(&session, class, sig, id, result, bytes);
}

/// Observe one PreToolUse event of the `cli-suggest` path. `response` is what the
/// gates decided for this call (`None` = they stayed silent). Counts a denial
/// (`negado` for the call's class, `denied` for the gate) and classifies the call that
/// follows an earlier denial of the session as `recovered`, `repeated` or
/// `denied_again`. Without a pending denial and without a denial now it costs one
/// lock and one map lookup.
pub fn observe_pre_call(session: &str, tool: &str, input: &Value, response: Option<&str>) {
    let gate = response.and_then(deny_gate_of);
    let pending = ledger().has_pending(session);
    if gate.is_none() && !pending {
        return;
    }
    let sig = call_signature(tool, input);
    let denied = gate.map(|g| (g, tool_class(tool, input)));
    ledger().on_pre(session, sig, denied);
}

/// The `tool_trio` object of `touring gate-metrics -j`: per-class rows and the
/// per-gate `deny_recovery` table of this daemon's lifetime.
pub fn snapshot_json() -> Value {
    serde_json::to_value(ledger().snapshot()).unwrap_or(Value::Null)
}

#[cfg(test)]
#[path = "tool_trio_tests.rs"]
mod tests;
