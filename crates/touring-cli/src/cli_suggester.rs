//! `cli-suggest` — PreToolUse hook handler that suggests the best Touring CLI
//! command(s) for a proposed Claude Code tool invocation.
//!
//! ## Motivation
//!
//! Claude Code emits a `PreToolUse` event before invoking each tool (Bash, Grep,
//! Glob, Read, Edit, Write, ...). The shell version of this handler
//! (`~/.claude/hooks/touring-cli-suggester.sh`) classifies the operation via
//! regex and prints a fixed table of suggestions. It works, but:
//!
//! 1. **Cold latency** ~12 ms (jq + sha1sum subprocess overhead).
//! 2. **No real-time enrichment** — can't check whether the symbol is in the
//!    index, whether the file is indexed, whether there are gotcha matches.
//! 3. **Regex-fragile classifier** — false positives/negatives on unusual
//!    inputs.
//!
//! This Rust handler runs **in-process** inside the touring daemon actor
//! (registered as the `cli-suggest` hook in `hook_registry`). Latency drops to
//! sub-millisecond because:
//!
//! - The daemon socket is already open and warm.
//! - Symbol and FileKnowledge queries are direct method calls on
//!   `HookRuntime`, not subprocess invocations.
//! - The TTL cache is `moka::sync::Cache` (lock-free reads,
//!   shared across all hook invocations in the daemon's lifetime).
//!
//! ## Scope
//!
//! The classifier maps `(tool_name, tool_input)` to a set of recommended
//! `TouringCommand` candidates, drawing from the **full Touring CLI surface**
//! (~80 commands grouped into 12 clusters: ast/index/wiring/tantivy/memory/
//! learning/session/decompose/generate/quality/assist/health). It is
//! intentionally NOT limited to the 12 categories of REGRA #18 — those
//! categories are semantic shorthand for the human reader; the actual
//! recommendation engine selects whichever commands best fit the live signal.
//!
//! ## Output contract
//!
//! Returns a JSON string of the shape Claude Code expects from a PreToolUse
//! hook:
//!
//! ```json
//! {
//!   "hookSpecificOutput": {
//!     "hookEventName": "PreToolUse",
//!     "additionalContext": "[TOURING SUGGEST · cluster · conf=0.92] ...\n  MUST  touring index find Foo -j   // <10ms exact lookup\n  ..."
//!   }
//! }
//! ```
//!
//! When the classifier is not confident enough (< 0.7) OR the TTL cache says
//! the same (tool, input) was suggested in the last 5 minutes, returns `"{}"`
//! (the canonical "do nothing" shape).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use crate::action_signature::ActionSignature;
use crate::runtime::HookRuntime;
use crate::workflow::{
    WorkflowEnrichment, WorkflowState, advise_next_step, conversion_for, detect_antipattern,
    detect_stage, validate_glob_pattern,
};

// Needed by Slice 2 retrieval helpers (fail-open DB queries).
use rusqlite;

// ── Cache ────────────────────────────────────────────────────────────────────
//
// Anti-spam TTL: same (tool_name, tool_input_hash) is suppressed for 5 minutes.
// Lock-free reads via moka.

const SUGGESTION_TTL_SECS: u64 = 300;
const CACHE_MAX_CAPACITY: u64 = 4096;

fn cache() -> &'static moka::sync::Cache<u64, ()> {
    static CACHE: OnceLock<moka::sync::Cache<u64, ()>> = OnceLock::new();
    CACHE.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(SUGGESTION_TTL_SECS))
            .build()
    })
}

fn input_hash(tool_name: &str, tool_input: &Value) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    tool_name.hash(&mut hasher);
    // Hash the compact JSON representation — stable for identical inputs.
    tool_input.to_string().hash(&mut hasher);
    hasher.finish()
}

/// Stable `u64` dedupe key for a *generic* banner cluster, in a key space
/// disjoint from [`input_hash`]: the `\u{1}` control-byte prefix cannot appear
/// in a cluster identifier, so a cluster key never collides with a real
/// `(tool_name, tool_input)` hash. Lets each generic banner fire at most once
/// per TTL window (high-signal-rare), cutting banner-blindness from repeated
/// non-specific suggestions.
fn cluster_dedupe_key(cluster: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{1}cluster\u{1}".hash(&mut hasher);
    cluster.hash(&mut hasher);
    hasher.finish()
}

/// Outcome of the generic-banner cluster dedupe check (see [`cluster_dedupe_gate`]).
enum ClusterDecision {
    /// Duplicate generic banner within the TTL window → suppress the suggestion.
    Suppress,
    /// Fire the suggestion this window.
    Proceed,
}

/// F7c arming gate (telemetry §12): the hint-demotion actuator is **OFF by default**
/// — it auto-suppresses ignored hints only when explicitly armed (`TOURING_F7_ACTUATOR_ARMED`
/// set to anything but `0`) AND the A/B gate is green (enforced in [`crate::cli::kpi::hint_demotion_bump`]).
/// Gabriel arms it post-A/B (the F7 HIGH-risk gate); unset ⇒ zero live impact.
fn f7_actuator_armed() -> bool {
    std::env::var("TOURING_F7_ACTUATOR_ARMED").is_ok_and(|v| v != "0")
}

/// Decide whether a banner cluster should fire this TTL window, marking generic
/// banners as seen when cleared. Symbol- or file-specific suggestions always
/// proceed and are never deduped — each carries fresh, input-specific signal. A
/// generic banner (no symbol/file hint — system-health-precheck, git,
/// daemon-status, …) fires at most once per window (high-signal-rare), cutting
/// banner-blindness from repeated non-specific hints.
///
/// Marking happens here rather than on emit because `run` has no early-return
/// between this gate and the point a suggestion is emitted, so the two are
/// equivalent — and this keeps `run`'s control flow flat.
fn cluster_dedupe_gate(classifier: &ClassifierOutput) -> ClusterDecision {
    if classifier.carries_input_specific_signal() {
        return ClusterDecision::Proceed;
    }
    let key = cluster_dedupe_key(&classifier.cluster);
    if cache().get(&key).is_some() {
        return ClusterDecision::Suppress;
    }
    cache().insert(key, ());
    ClusterDecision::Proceed
}

// ── Code Mode induction counter (C8) ───────────────────────────────────────────
//
// A sibling window counter (disjoint key space from `cache` / `cluster_dedupe_key`)
// that counts repeated *scan* operations (grep/rg/find/Grep) so the cli-suggest
// hook can surface a `touring_ctx_execute` orchestration hint once the LLM is
// clearly doing atomic search N times. Explicit shell loops bypass the counter
// (a loop is unambiguous on first sight). See `detect_code_mode`.

/// Window length for the repeated-scan counter. Shorter than the suggestion TTL
/// so the "repeated scanning" signal reflects the *current* burst of activity.
const CODE_MODE_WINDOW_SECS: u64 = 180;

/// Scan count at which the Code Mode hint fires. Set to 3 (not the literal "2nd")
/// to favour precision: by the 3rd atomic search in one window the repeated-scan
/// pattern is unambiguous, cutting false positives on an incidental 2nd grep.
const CODE_MODE_SCAN_THRESHOLD: u32 = 3;

/// Per-window counter of repeated scan operations, keyed by [`scan_class_key`].
/// Value is the running count within the live [`CODE_MODE_WINDOW_SECS`] window.
fn scan_counter() -> &'static moka::sync::Cache<u64, u32> {
    static COUNTER: OnceLock<moka::sync::Cache<u64, u32>> = OnceLock::new();
    COUNTER.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(256)
            .time_to_live(Duration::from_secs(CODE_MODE_WINDOW_SECS))
            .build()
    })
}

/// Fixed key for the repeated-scan window counter, in a key space disjoint from
/// [`input_hash`] and [`cluster_dedupe_key`] (a distinct `\u{2}` control-byte
/// tag that cannot appear in a tool name or cluster id), so the counter never
/// collides with the anti-spam or banner-dedupe caches.
fn scan_class_key() -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}code-mode-scan\u{2}".hash(&mut hasher);
    hasher.finish()
}

/// True iff incrementing `prev` lands exactly on `threshold` — i.e. this is the
/// threshold-crossing edge. Pure (no cache) so the edge logic is unit-testable
/// without the global counter. Saturating to avoid wrap at `u32::MAX`.
fn crosses_threshold(prev: u32, threshold: u32) -> bool {
    prev.saturating_add(1) == threshold
}

/// Increment the live scan-window counter and report whether this call is the
/// threshold-crossing edge (fire once, then suppress until the window expires).
/// The get-then-insert is non-atomic, but a benign race only risks a duplicate
/// nudge under heavy concurrency — acceptable for a fail-open advisory hook.
fn scan_window_crosses_threshold() -> bool {
    let key = scan_class_key();
    let prev = scan_counter().get(&key).unwrap_or(0);
    scan_counter().insert(key, prev.saturating_add(1));
    crosses_threshold(prev, CODE_MODE_SCAN_THRESHOLD)
}

/// The raw classifier confidence for a `(tool, input)` pair — exposed so the
/// conformal calibrator (S-08 / A-A1) can recover the model score for a
/// historical command without the full enrichment payload. `None` when no
/// classifier matches the tool.
pub fn classify_confidence(tool_name: &str, tool_input: &Value) -> Option<f32> {
    classify(tool_name, tool_input).map(|c| c.confidence)
}

/// Data-derived conformal firing threshold for the skill-selection gate
/// (S-08 / A-A1).
///
/// The gate historically used a hardcoded `0.7` cut — a magic constant with no
/// statistical meaning. This distils the recent `bash_outcomes` substrate into
/// split-conformal calibration examples `(raw_confidence, was_valid)` — each
/// historical command re-classified to recover the confidence the classifier
/// *would* assign, paired with whether the action succeeded — and returns
/// `τ = 1 − q̂` ([`crate::conformal::ConformalCalibrator::threshold`]), the
/// threshold carrying a `1 − α` coverage guarantee. Falls back to
/// [`crate::conformal::LEGACY_THRESHOLD`] when the substrate is too thin
/// (`n < MIN_CALIBRATION`).
///
/// Memoised behind a 300 s TTL so the PreToolUse hot path pays the substrate
/// scan at most once per cache window. Fail-open: any error → legacy cut.
fn conformal_gate_threshold(rt: &HookRuntime) -> f32 {
    use crate::conformal::{ConformalCalibrator, DEFAULT_ALPHA, LEGACY_THRESHOLD};

    static CACHE: OnceLock<std::sync::Mutex<Option<(f32, std::time::Instant)>>> = OnceLock::new();
    let cell = CACHE.get_or_init(|| std::sync::Mutex::new(None));
    if let Ok(guard) = cell.lock() {
        if let Some((tau, at)) = *guard {
            if at.elapsed().as_secs() < SUGGESTION_TTL_SECS {
                return tau;
            }
        }
    }

    // Recompute from the bash outcome substrate — the richest live stream.
    let outcomes = rt
        .ctx
        .knowledge
        .recent_bash_outcomes(512)
        .unwrap_or_default();
    let cal = ConformalCalibrator::from_examples(
        DEFAULT_ALPHA,
        outcomes.iter().filter_map(|o| {
            let input = serde_json::json!({ "command": o.command });
            classify_confidence("Bash", &input).map(|c| (f64::from(c), o.success))
        }),
    );
    let tau = if cal.is_calibrated() {
        cal.threshold() as f32
    } else {
        LEGACY_THRESHOLD as f32
    };

    if let Ok(mut guard) = cell.lock() {
        *guard = Some((tau, std::time::Instant::now()));
    }
    tau
}

// ── Public types ─────────────────────────────────────────────────────────────

/// One concrete recommended Touring CLI invocation, with rationale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInvocation {
    /// Fully-formed shell command (placeholder-substituted), e.g.
    /// `"touring ast meta crates/foo.rs --depth summary -j"`.
    pub command: String,
    /// One-line purpose shown next to the command in the rendered output.
    pub purpose: String,
}

/// Live data the daemon already has in memory about the file/symbol the
/// caller is about to touch. Filled by `enrich`. Absent fields = either the
/// asset is not indexed or the API call returned no data; never a panic.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnrichmentData {
    /// `true` when the file appears in the FileKnowledge index.
    pub file_is_indexed: Option<bool>,
    /// `true` when the file's blake3 hash is registered (proxy for "recently
    /// reindexed").
    pub file_has_blake3: Option<bool>,
    /// Symbol existence as reported by `SymbolStore::find_symbol`.
    pub symbol_in_index: Option<bool>,
    /// Number of definitions of the symbol (>1 = homonym → C08 trigger).
    pub symbol_definition_count: Option<u32>,
    /// Gotcha summaries that match the file path.
    pub gotcha_matches: Vec<String>,
    /// Number of dependents (callers / inverse imports) for the file.
    pub dependent_count: Option<u32>,
    /// Number of pub symbols defined in the file.
    pub pub_symbol_count: Option<u32>,
    /// Cognitive complexity score for the file in [0.0, 1.0].
    ///
    /// Sourced from `knowledge.get_cognitive_enrichment(file_path)` (field 0 of
    /// the returned `CognitiveScores` tuple). `None` when the file has no
    /// cognitive enrichment row yet (e.g. never post-edited) or when no
    /// `file_hint` is present in the classifier output.
    ///
    /// Used by `ActionSignature::from_pre_tool_with_cognitive` to set the
    /// `HiComplexity` qualifier when `cognitive_score > 0.7`.
    pub cognitive_score: Option<f32>,

    /// Workflow-stage advice string injected by P8.7 (workflow intelligence).
    ///
    /// `None` until P8.7 wires its detection logic; the field is an extension
    /// point so downstream renderers can surface workflow-stage context
    /// (e.g. "you are in the SCOUT phase — prefer read-only queries") without
    /// requiring a separate enrichment pass.
    ///
    /// # P8.7 wires workflow advice here
    pub workflow_stage_hint: Option<String>,

    /// The ready-to-run `touring index rebuild --dir <project_root>` command,
    /// populated when the file is missing from the blake3 registry
    /// (`file_is_indexed == Some(false)`). A stale index makes every other
    /// enrichment field under-report (dependents/pub_symbols read old data), so
    /// the repair command travels with the signal it degrades — carrying the
    /// REAL project root, never a placeholder (injection-density invariant).
    pub stale_index_hint: Option<String>,
}

/// Final suggestion structure: classifier output + enrichment + rendered text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// Semantic tag — derived from the strongest classifier match. Free-form
    /// short label such as `"symbol-lookup"`, `"pre-edit-rust"`,
    /// `"new-tsx-component"`. Used in the rendered header for the LLM reader.
    pub cluster: String,
    /// Top-priority commands; should be run before the proposed tool.
    pub must: Vec<CommandInvocation>,
    /// High-value complements.
    pub should: Vec<CommandInvocation>,
    /// Lower-priority, situational commands.
    pub may: Vec<CommandInvocation>,
    /// Human-readable explanation.
    pub reason: String,
    /// `[0.0, 1.0]` — emit only when `>= 0.7`.
    pub confidence: f32,
    /// Live data from the daemon (file/symbol enrichment).
    pub enrichment: EnrichmentData,
}

// ── Classifier ───────────────────────────────────────────────────────────────
//
// Intermediate output from `classify`: lists of commands + cluster tag +
// reason + confidence, **before** enrichment is layered on top.

#[derive(Debug, Clone, Default)]
struct ClassifierOutput {
    cluster: String,
    must: Vec<CommandInvocation>,
    should: Vec<CommandInvocation>,
    may: Vec<CommandInvocation>,
    reason: String,
    confidence: f32,
    /// Optional: a symbol the classifier identified as the operand.
    /// Used by enrichment to query `SymbolStore`.
    symbol_hint: Option<String>,
    /// Optional: a file the classifier identified as the operand.
    /// Used by enrichment to query `FileKnowledgeDB`.
    file_hint: Option<String>,
}

impl ClassifierOutput {
    /// `true` when this output carries input-specific signal: a symbol/file
    /// operand, or a code-mode nudge whose MUST embeds the real command,
    /// pattern, or glob verbatim (`code_mode_output` never emits a generic
    /// template for a Bash trigger). Input-specific suggestions are never
    /// cluster-deduped — each carries fresh signal, unlike a generic banner
    /// repeated within the TTL window; identical inputs are still anti-spammed
    /// by the `(tool, input)` hash cache in `run`.
    fn carries_input_specific_signal(&self) -> bool {
        self.symbol_hint.is_some()
            || self.file_hint.is_some()
            || self.cluster.starts_with("code-mode-")
    }
}

fn cmd(command: impl Into<String>, purpose: impl Into<String>) -> CommandInvocation {
    CommandInvocation {
        command: command.into(),
        purpose: purpose.into(),
    }
}

/// True iff `s` looks like an identifier: PascalCase or snake_case with at
/// least three characters and only word chars + underscores.
fn looks_like_symbol(s: &str) -> bool {
    if s.len() < 3 {
        return false;
    }
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return false;
    }
    // Reject overly long candidates (likely free text concatenated).
    s.len() <= 80
}

/// Extension-based code-file classification.
fn is_code_file(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or(""),
        "rs" | "py"
            | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "go"
            | "c"
            | "cpp"
            | "cc"
            | "cxx"
            | "h"
            | "hpp"
            | "java"
            | "kt"
            | "swift"
            | "rb"
            | "php"
            | "scala"
            | "sh"
            | "bash"
    )
}

fn is_rust_file(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s == "rs")
        .unwrap_or(false)
}

fn extract_first_symbol_in_text(text: &str) -> Option<String> {
    // PascalCase first
    let pascal = regex_first_match(r"\b[A-Z][A-Za-z0-9_]{2,}\b", text);
    if let Some(p) = pascal {
        return Some(p);
    }
    // snake_case with at least one underscore
    regex_first_match(r"\b[a-z][a-z0-9_]*_[a-z0-9_]+[a-z0-9]\b", text)
}

/// Lightweight regex match using the `regex` crate already present in deps.
fn regex_first_match(pattern: &str, text: &str) -> Option<String> {
    let re = regex::Regex::new(pattern).ok()?;
    re.find(text).map(|m| m.as_str().to_string())
}

/// Heuristic file-path extraction from a bash command — picks the first token
/// matching a code-file extension.
fn extract_code_file_from_command(cmd: &str) -> Option<String> {
    let re = regex::Regex::new(
        r"[\w./-]+\.(?:rs|py|ts|tsx|js|jsx|go|c|cpp|h|java|kt|swift|rb|php|sh|bash)\b",
    )
    .ok()?;
    re.find(cmd).map(|m| m.as_str().to_string())
}

// ── Per-tool classifiers ────────────────────────────────────────────────────

fn classify(tool_name: &str, tool_input: &Value) -> Option<ClassifierOutput> {
    match tool_name {
        "Bash" => classify_bash(tool_input),
        "Grep" => classify_grep(tool_input),
        "Glob" => classify_glob(tool_input),
        "Read" => classify_read(tool_input),
        "Edit" | "NotebookEdit" => classify_edit(tool_input),
        "Write" => classify_write(tool_input),
        "Task" => classify_task(tool_input),
        "WebFetch" | "WebSearch" => classify_webfetch(tool_input),
        _ => None,
    }
}

/// Classifier for `Task` tool invocations (agent delegation).
///
/// Returns `Some` for any non-empty task input so that Phase 1/2 lesson
/// retrieval is unlocked for agent-spawning operations.  Fail-open: malformed
/// or empty input still returns `Some` with generic guidance rather than `None`,
/// because suppressing enrichment on bad input is worse than providing a hint.
fn classify_task(tool_input: &Value) -> Option<ClassifierOutput> {
    // `subagent_type` is the canonical discriminator for Task calls.
    let subagent_type = tool_input
        .get("subagent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Tailor the cluster label when we know the concrete agent type.
    let cluster = if subagent_type == "unknown" {
        "agent-delegation".into()
    } else {
        format!("agent-delegation-{subagent_type}")
    };

    // The Task call carries its own description/prompt — recall lessons for THAT domain,
    // not a `<task_description>` placeholder (injection-density invariant). The `<task_id>`
    // / `<id>` / `<objective>` below stay marked: those name future entities the caller
    // creates, genuinely absent from the trigger.
    let task_desc = cmd_excerpt(
        tool_input
            .get("description")
            .or_else(|| tool_input.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("the task"),
        80,
    );

    Some(ClassifierOutput {
        cluster,
        must: vec![
            cmd(
                "touring decompose validate <task_id>",
                "verify DAG ordering — no cycles before spawning",
            ),
            cmd(
                "touring wiring orphans -j",
                "REGRA #0 — wire orphan pub symbols before delegating",
            ),
        ],
        should: vec![
            cmd(
                format!("touring memory recall \"{task_desc}\""),
                "recall past lessons for this task domain",
            ),
            cmd(
                "touring doctor -j",
                "daemon health gate before expensive agent spawn",
            ),
        ],
        may: vec![
            cmd(
                "touring decompose get <task_id> -j",
                "inspect subtask status / depends_on chain",
            ),
            cmd(
                "touring session start <id> type \"<objective>\"",
                "open a named session for the delegated agent",
            ),
        ],
        reason: format!(
            "Task spawns a sub-agent ({subagent_type}) in an isolated context; \
             only its final message returns. Verify the DAG, ensure no orphan \
             pub symbols, and confirm daemon health before delegating."
        ),
        confidence: 0.82,
        symbol_hint: None,
        file_hint: None,
    })
}

/// Classifier for `WebFetch` and `WebSearch` tool invocations.
///
/// Both `WebFetch` (has `url` + `prompt`) and `WebSearch` (has `query`) are
/// handled by the same function — they share the same enrichment guidance.
/// Returns `Some` for any recognisable web tool input; fail-open for missing
/// fields.
fn classify_webfetch(tool_input: &Value) -> Option<ClassifierOutput> {
    // Prefer `url` (WebFetch), fall back to `query` (WebSearch), then empty.
    let target = tool_input
        .get("url")
        .or_else(|| tool_input.get("query"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Detect whether this is a search (has `query`) or a fetch (has `url`).
    let is_search = tool_input.get("query").is_some();

    let (op_label, cache_note) = if is_search {
        (
            "WebSearch",
            "results are NOT cached — repeated identical queries hit the network each time",
        )
    } else {
        (
            "WebFetch",
            "results are cached ~15 min — redirected hosts need a fresh call with the new URL",
        )
    };

    let reason = if target.is_empty() {
        format!("{op_label} — prefer official/canonical sources; verify the URL/query before use.")
    } else {
        format!(
            "{op_label} on `{target}` — prefer official/canonical sources; \
             {cache_note}."
        )
    };

    Some(ClassifierOutput {
        cluster: "web-fetch".into(),
        must: vec![cmd(
            "touring memory recall \"<topic>\"",
            "check if the answer is already in local memory before a network call",
        )],
        should: vec![
            cmd(
                "touring tantivy search \"<topic>\"",
                "BM25-ranked local knowledge base — often faster than web",
            ),
            cmd(
                "touring index find <SymbolName>",
                "prefer local symbol lookup over external docs when checking API surface",
            ),
        ],
        may: vec![cmd(
            "touring memory store \"web:<topic>\" \"<finding>\" --tier semantic",
            "persist the web finding so future sessions skip the fetch",
        )],
        reason,
        confidence: 0.78,
        symbol_hint: None,
        file_hint: None,
    })
}

fn classify_bash(tool_input: &Value) -> Option<ClassifierOutput> {
    let command = tool_input.get("command").and_then(|v| v.as_str())?;
    if command.is_empty() {
        return None;
    }

    // Pattern 1: grep/rg for a symbol-like token.
    if (command.starts_with("grep") || command.starts_with("rg ") || command.starts_with("rg\t"))
        && command.contains(' ')
    {
        if let Some(sym) = extract_first_symbol_in_text(command) {
            return Some(ClassifierOutput {
                cluster: "symbol-lookup".into(),
                must: vec![
                    cmd(format!("touring index find {sym} -j"), "<10ms exact lookup"),
                    cmd(
                        format!("touring wiring impact {sym} --depth 2"),
                        "BFS consumers (transitive)",
                    ),
                ],
                should: vec![
                    cmd(
                        format!("touring ast find {sym} -j"),
                        "signature + module path",
                    ),
                    cmd(
                        format!("touring tantivy search \"{sym}\""),
                        "BM25-ranked context hits",
                    ),
                ],
                may: vec![cmd(
                    format!("grep -rn \"{sym}\" crates/ --include='*.rs'"),
                    "VP-Scout Chain 7 (wiring staleness fallback)",
                )],
                reason: format!(
                    "Pattern '{sym}' looks like a symbol — indexed lookup is exact \
                     and constant-time; wiring impact reveals transitive consumers."
                ),
                confidence: 0.92,
                symbol_hint: Some(sym),
                file_hint: None,
            });
        }
    }

    // Pattern 2: cargo build/check/test — health gate first.
    if regex::Regex::new(r"\bcargo\s+(build|check|test|clippy)\b")
        .ok()
        .map(|re| re.is_match(command))
        .unwrap_or(false)
    {
        return Some(ClassifierOutput {
            cluster: "system-health-precheck".into(),
            must: vec![
                cmd("touring doctor -j", "daemon + index health gate"),
                cmd("touring status -j", "composite_health_score + counters"),
            ],
            should: vec![
                cmd("touring e2e -j", "composite system score 0-1"),
                cmd(
                    "touring gate-metrics -j",
                    "live counters (reindex_failure, etc.)",
                ),
            ],
            may: vec![],
            reason: "cargo runs are expensive; touring doctor catches daemon/index \
                     issues in milliseconds before commitment."
                .into(),
            confidence: 0.85,
            symbol_hint: None,
            file_hint: None,
        });
    }

    // Pattern 3: find -name '*.ext' — file enumeration.
    if regex::Regex::new(r"^\s*find\b.*-name\s+['\x22]?\*\.[a-z]+")
        .ok()
        .map(|re| re.is_match(command))
        .unwrap_or(false)
    {
        return Some(ClassifierOutput {
            cluster: "file-enumeration".into(),
            must: vec![cmd(
                format!(
                    "touring index files \"{}\" --limit 200",
                    find_name_glob(command).unwrap_or_else(|| "*".to_string())
                ),
                "BM25 + symbol-aware enumeration",
            )],
            should: vec![cmd(
                "touring ast workspace-info",
                "cargo metadata (packages, features, dependents)",
            )],
            may: vec![],
            reason: "`find` walks the filesystem; `touring index files` queries the \
                     symbol-aware index in <10ms."
                .into(),
            confidence: 0.78,
            symbol_hint: None,
            file_hint: None,
        });
    }

    // Pattern 4: cat/head/tail of a code file.
    if regex::Regex::new(r"^(?:cat|head|tail|less|more)\s+")
        .ok()
        .map(|re| re.is_match(command))
        .unwrap_or(false)
    {
        if let Some(file) = extract_code_file_from_command(command) {
            let is_rs = is_rust_file(&file);
            let mut should = vec![
                cmd(
                    format!("touring ast overview {file} -j"),
                    "module structure + symbol map",
                ),
                cmd(
                    "Read tool with line ranges".to_string(),
                    "raw content, but cirurgical",
                ),
            ];
            if is_rs {
                should.push(cmd(
                    format!("touring ast rust-semantic {file}"),
                    "generics, traits, lifetimes, semantic_complexity",
                ));
                should.push(cmd(
                    format!("touring ast tdg {file}"),
                    "TDG grade A+..F (6 dimensions)",
                ));
            }
            return Some(ClassifierOutput {
                cluster: "raw-read-code-file".into(),
                must: vec![cmd(
                    format!("touring ast meta {file} --depth summary -j"),
                    "blast_radius + quality + cognitive (file-metadata-first)",
                )],
                should,
                may: vec![],
                reason: "Raw cat dumps bytes; ast metadata in <10ms reveals risk \
                         metrics before any read."
                    .into(),
                confidence: 0.86,
                symbol_hint: None,
                file_hint: Some(file),
            });
        }
    }

    // Pattern 5: destructive inline edits (sed -i, awk -i inplace, perl -pi,
    // rm + heredoc) — anti-pattern, route to touring-native tooling.
    if regex::Regex::new(
        r"(sed\s+-i|awk\s+-i\s+inplace|perl\s+-pi|rm\s+.*&&\s*(?:cat|echo|printf)\s*>)",
    )
    .ok()
    .map(|re| re.is_match(command))
    .unwrap_or(false)
    {
        let target = inline_edit_target(command).unwrap_or_else(|| "<file>".to_string());
        return Some(ClassifierOutput {
            cluster: "anti-pattern-bash-edit".into(),
            must: vec![cmd(
                format!("Edit tool --path {target} --operation rewrite|ssr|free-form"),
                "edição-com-gate canonical workflow (17 stage gates)",
            )],
            should: vec![
                cmd(
                    format!("touring ast meta {target} --depth summary -j"),
                    "blast radius before edit",
                ),
                cmd("touring pre-edit", "score gate (>= 0.8) + CILA budget"),
            ],
            may: vec![],
            reason: "Inline bash edits bypass VGP, blast_radius, format, TDG grade, \
                     atomic snapshot, and gotcha match — 17 gates skipped."
                .into(),
            confidence: 0.94,
            symbol_hint: None,
            file_hint: None,
        });
    }

    // Pattern 6: git command — REGRA #11 prohibits git in TACO.
    if regex::Regex::new(r"^\s*git\s+")
        .ok()
        .map(|re| re.is_match(command))
        .unwrap_or(false)
    {
        return Some(ClassifierOutput {
            cluster: "regra-11-git-prohibited".into(),
            must: vec![
                cmd(
                    "touring memory recall \"<topic>\"",
                    "history substitute (git log replacement)",
                ),
                cmd(
                    "touring status -j",
                    "current state (git status replacement)",
                ),
            ],
            should: vec![cmd(
                "touring ast blast <file>",
                "diff/impact view (git diff replacement)",
            )],
            may: vec![],
            reason: "REGRA #11 — git is prohibited in TACO. Touring is the source \
                     of truth; the block_git.sh hook will reject this command."
                .into(),
            confidence: 0.99,
            symbol_hint: None,
            file_hint: None,
        });
    }

    // Pattern 7: pgrep / ps for touring daemon — point to doctor.
    if regex::Regex::new(r"(pgrep|ps\s+-).*touring")
        .ok()
        .map(|re| re.is_match(command))
        .unwrap_or(false)
    {
        return Some(ClassifierOutput {
            cluster: "daemon-status".into(),
            must: vec![cmd(
                "touring doctor -j",
                "all 5 health components in one JSON",
            )],
            should: vec![cmd(
                "touring status -j",
                "symbol_count + composite_health_score",
            )],
            may: vec![],
            reason: "Process inspection is OS-level; `touring doctor` checks daemon \
                     socket, knowledge DB, circuit breaker, project_db all at once."
                .into(),
            confidence: 0.82,
            symbol_hint: None,
            file_hint: None,
        });
    }

    // Pattern 8: Bash command carrying inline executable code (python -c, bash -c,
    // node -e, ruby -e, perl -e, sh -c, or a shebang-style script invocation).
    // CEG wired pair P6.4 — advise routing through `touring exec` (X0..X9 pipeline).
    // Fail-open: regex failure returns `false`, enrichment is simply skipped.
    if bash_command_carries_executable_code(command) {
        let exec_excerpt = cmd_excerpt(command, 120);
        let exec_prefix = command.split_whitespace().next().unwrap_or("exec");
        return Some(ClassifierOutput {
            cluster: "exec-gate-advisory".into(),
            must: vec![cmd(
                format!("touring exec \"{exec_excerpt}\""),
                "CEG X0..X9 pipeline: capture → classify → sandbox → gate → learn",
            )],
            should: vec![
                cmd(
                    "touring gate-metrics -j | jq '{ceg_captured_count, ceg_sandboxed_count, workflow_advice_emitted_count}'",
                    "live CEG activity counters (P6.4 synergy pair)",
                ),
                cmd(
                    format!("touring memory recall \"exec:{exec_prefix}\""),
                    "past outcomes for this command class",
                ),
            ],
            may: vec![
                // P8.7 wires workflow advice here
                cmd(
                    "touring wiring orphans -j",
                    "REGRA #0 — verify no new orphan pub symbols after execution",
                ),
            ],
            reason: "Command carries inline executable code; the CEG (X0..X9) pipeline \
                     adds sandbox isolation, capability classification, dry-run preview, \
                     and RL feedback. Use `touring exec` to route through the gate."
                .into(),
            confidence: 0.80,
            symbol_hint: None,
            file_hint: None,
        });
    }

    None
}

/// Returns `true` when a Bash command carries inline executable code that
/// should be routed through the CEG (Code Execution Gateway, X0..X9).
///
/// Detects:
/// - Interpreter `-c` / `-e` flags: `python -c`, `python3 -c`, `bash -c`,
///   `sh -c`, `node -e`, `ruby -e`, `perl -e`, `php -r`.
/// - Direct script invocation: command ending in `.py`, `.sh`, `.rb`, `.js`.
///
/// Fail-open: any regex compilation error returns `false` so the caller
/// gracefully skips the pattern rather than panicking.
///
/// # CEG wiring
///
/// Registered as WIRED_PAIR (`CEG gateway (X0..X9)`, `cli_suggester enrichment`)
/// in `crates/touring-server/src/cli/synergy.rs` (Wave P6.4).
fn bash_command_carries_executable_code(command: &str) -> bool {
    // Interpreter inline-code flags: python -c "...", bash -c '...', node -e, etc.
    let inline_re = regex::Regex::new(
        r"(?x)
        (?:python3?|bash|sh|node|ruby|perl|php|bun)
        \s+
        -[ce]\s+
        ['\x22]",
    )
    .ok();
    if inline_re
        .as_ref()
        .map(|re| re.is_match(command))
        .unwrap_or(false)
    {
        return true;
    }
    // Direct script invocation ending in a known executable extension.
    let script_re = regex::Regex::new(r"(?:^|\s)[\w./~-]+\.(?:py|sh|rb|js|pl|php)\b").ok();
    script_re
        .as_ref()
        .map(|re| re.is_match(command))
        .unwrap_or(false)
}

fn classify_grep(tool_input: &Value) -> Option<ClassifierOutput> {
    let pattern = tool_input.get("pattern").and_then(|v| v.as_str())?;
    if pattern.is_empty() {
        return None;
    }

    // Pattern looks like a literal symbol → route to index find.
    // Guard: only consider the pattern a symbol when it is ALREADY a clean
    // identifier (no whitespace, no regex meta-chars). Otherwise the
    // alphanumeric filter below would happily concatenate "TODO fix the thing"
    // into "TODOfixthething" — a perfectly plausible (but wrong) PascalCase.
    let has_whitespace_or_meta = pattern.chars().any(|c| {
        c.is_whitespace()
            || matches!(
                c,
                '.' | '*' | '+' | '?' | '|' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '\\'
            )
    });
    let clean: String = if has_whitespace_or_meta {
        String::new()
    } else {
        pattern
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect()
    };
    if looks_like_symbol(&clean) {
        return Some(ClassifierOutput {
            cluster: "symbol-lookup".into(),
            must: vec![cmd(
                format!("touring index find {clean} -j"),
                "<10ms exact lookup with definition locations",
            )],
            should: vec![
                cmd(
                    format!("touring wiring impact {clean} --depth 2"),
                    "transitive consumers (BFS)",
                ),
                cmd(
                    format!("touring ast find {clean} -j"),
                    "signature + module path + line",
                ),
            ],
            may: vec![cmd(
                format!("touring tantivy fuzzy \"{clean}\" 2"),
                "fuzzy fallback for typos",
            )],
            reason: format!(
                "Pattern '{clean}' is a clean identifier; the symbol index is \
                 BM25-ranked and exact. Wiring impact reveals who depends on it."
            ),
            confidence: 0.91,
            symbol_hint: Some(clean),
            file_hint: None,
        });
    }

    // Otherwise, free-text → tantivy BM25.
    Some(ClassifierOutput {
        cluster: "free-text-search".into(),
        must: vec![cmd(
            format!("touring tantivy search \"{pattern}\""),
            "BM25-ranked hits with snippets",
        )],
        should: vec![cmd(
            format!("touring tantivy fuzzy \"{pattern}\" 2"),
            "edit-distance 2 fallback",
        )],
        may: vec![cmd(
            format!("touring search symbols \"{pattern}\""),
            "BM25 rank limited to symbols",
        )],
        reason: "Free-text Grep is O(workspace); tantivy provides BM25-ranked hits \
                 with snippets in <10ms."
            .into(),
        confidence: 0.74,
        symbol_hint: None,
        file_hint: None,
    })
}

fn classify_glob(tool_input: &Value) -> Option<ClassifierOutput> {
    let pattern = tool_input.get("pattern").and_then(|v| v.as_str())?;
    if pattern.is_empty() {
        return None;
    }
    let is_rust_pattern = pattern.ends_with("*.rs") || pattern.contains(".rs");
    Some(ClassifierOutput {
        cluster: "file-enumeration".into(),
        must: vec![cmd(
            format!("touring index files \"{pattern}\" --limit 200"),
            "symbol-aware enumeration with metadata",
        )],
        should: if is_rust_pattern {
            vec![cmd(
                "touring ast workspace-info",
                "cargo packages + features + dependents",
            )]
        } else {
            vec![cmd(
                "touring tantivy search \"<topic>\"",
                "consider BM25 over file names when looking by concept",
            )]
        },
        may: vec![],
        reason: "Glob enumerates files; touring index files adds pub-symbol counts \
                 + quality scores + language detection per file."
            .into(),
        confidence: 0.72,
        symbol_hint: None,
        file_hint: None,
    })
}

fn classify_read(tool_input: &Value) -> Option<ClassifierOutput> {
    let file = tool_input.get("file_path").and_then(|v| v.as_str())?;
    if !is_code_file(file) {
        // Not a code file — stay silent (Read of .md/.json/.toml is fine).
        return None;
    }
    let is_rs = is_rust_file(file);
    let mut should = vec![
        cmd(
            format!("touring ast overview {file} -j"),
            "structure + symbols + imports",
        ),
        cmd(format!("touring ast tdg {file}"), "TDG grade A+..F"),
    ];
    if is_rs {
        should.push(cmd(
            format!("touring ast rust-semantic {file}"),
            "syn: generics, traits, lifetimes, unsafe, async",
        ));
    }
    Some(ClassifierOutput {
        cluster: if is_rs {
            "read-rust-comprehend".into()
        } else {
            "read-code-comprehend".into()
        },
        must: vec![cmd(
            format!("touring ast meta {file} --depth summary -j"),
            "blast_radius + quality + cognitive + fan_in/fan_out",
        )],
        should,
        may: vec![cmd(
            format!("touring file-knowledge extended {file}"),
            "23 metadata fields (community, modularity, etc.)",
        )],
        reason: if is_rs {
            "Rust file — semantic info (generics/traits/unsafe), TDG grade, and \
             blast radius inform the read before raw bytes."
                .into()
        } else {
            "Code file — structure + quality grade complement raw content.".into()
        },
        confidence: 0.84,
        symbol_hint: None,
        file_hint: Some(file.to_string()),
    })
}

fn classify_edit(tool_input: &Value) -> Option<ClassifierOutput> {
    let file = tool_input
        .get("file_path")
        .or_else(|| tool_input.get("notebook_path"))
        .and_then(|v| v.as_str())?;
    if !is_code_file(file) {
        return None;
    }
    let is_rs = is_rust_file(file);
    let mut must = vec![
        cmd(
            format!("touring ast meta {file} --depth summary -j"),
            "file-metadata-first (golden rule)",
        ),
        cmd(format!("touring ast blast {file}"), "full dependency tree"),
    ];
    if is_rs {
        must.push(cmd(format!("touring ast tdg {file}"), "STOP at grade D/F"));
    }
    let should = vec![
        cmd(
            "touring pre-edit".to_string(),
            "score >= 0.8 gate (CILA budget)",
        ),
        cmd(
            format!("touring gotcha match {file}"),
            "known pitfalls for this file",
        ),
        cmd(
            format!("Edit tool --path {file} ..."),
            "edição-com-gate canonical (17 stage gates)",
        ),
    ];
    Some(ClassifierOutput {
        cluster: if is_rs {
            "pre-edit-triage-rust".into()
        } else {
            "pre-edit-triage-code".into()
        },
        must,
        should,
        may: vec![cmd(
            format!("touring health-delta status {file}"),
            "per-path streak (alert if regression > 3)",
        )],
        reason: if is_rs {
            "Rust edit — STOP at TDG D/F; blast_radius > 10 requires a mitigation \
             plan; Edit tool applies the 17 stage gates (VGP, blast, format, \
             TDG, atomic snapshot, gotcha, wiring delta, RL reward)."
                .into()
        } else {
            "Code edit — file-metadata-first principle; pre-edit score gates the \
             operation."
                .into()
        },
        confidence: 0.89,
        symbol_hint: None,
        file_hint: Some(file.to_string()),
    })
}

fn classify_write(tool_input: &Value) -> Option<ClassifierOutput> {
    let file = tool_input.get("file_path").and_then(|v| v.as_str())?;
    if !is_code_file(file) {
        return None;
    }
    let (kind, create_cmd) = match Path::new(file).extension().and_then(|s| s.to_str()) {
        Some("rs") => (
            "RustModule",
            format!("Write tool --path {file} --kind RustModule --intent \"<intent>\""),
        ),
        Some("py") => (
            "PythonScript",
            format!("Write tool --path {file} --intent \"<intent>\""),
        ),
        Some("ts") => (
            "TypeScriptModule",
            format!("Write tool --path {file} --intent \"<intent>\""),
        ),
        Some("tsx") => (
            "ReactComponent",
            format!("Write tool --path {file} --intent \"<intent>\""),
        ),
        _ => (
            "generic",
            format!("Write tool --path {file} --intent \"<intent>\""),
        ),
    };
    Some(ClassifierOutput {
        cluster: format!("new-{}", kind.to_lowercase()),
        must: vec![cmd(
            create_cmd,
            "edição-com-gate canonical (VGP + atomic + post-validate)",
        )],
        should: vec![
            cmd(
                "touring index find <SymbolName>",
                "collision check before defining new symbols",
            ),
            cmd(
                "touring generate verify --symbol <name>",
                "VGP gate at template stage",
            ),
        ],
        may: vec![cmd(
            "touring wiring suggest <new_symbol>",
            "auto-wire hints after creation",
        )],
        reason: format!(
            "New {kind} — touring-native tooling runs the 12-stage create pipeline (doctor + \
             discover + VGP + render + atomic write + post-validate + memory + RL). \
             Plain Write bypasses all of these."
        ),
        confidence: 0.92,
        symbol_hint: None,
        file_hint: Some(file.to_string()),
    })
}

// ── Enrichment ───────────────────────────────────────────────────────────────
//
// Layers live daemon state on top of the classifier output. All queries are
// best-effort: any error is swallowed, the relevant `EnrichmentData` field
// stays `None`, and the suggestion continues to render.

fn enrich(rt: &HookRuntime, classifier: &ClassifierOutput) -> EnrichmentData {
    let mut data = EnrichmentData::default();

    // Symbol enrichment.
    if let Some(ref sym) = classifier.symbol_hint {
        if let Some(ref store) = rt.infra.symbol_store {
            match store.find_symbol(sym) {
                Ok(locs) => {
                    data.symbol_in_index = Some(!locs.is_empty());
                    data.symbol_definition_count = Some(locs.len() as u32);
                }
                Err(_) => {
                    data.symbol_in_index = Some(false);
                }
            }
        }
    }

    // File enrichment.
    if let Some(ref file) = classifier.file_hint {
        // Normalise to a project-relative path when possible.
        let rel = make_relative_to_project(rt, file);

        // blake3 hash registry as a proxy for "file is indexed".
        match rt.ctx.knowledge.get_blake3_hash(&rel) {
            Ok(Some(_)) => {
                data.file_is_indexed = Some(true);
                data.file_has_blake3 = Some(true);
            }
            Ok(None) => {
                data.file_is_indexed = Some(false);
                data.file_has_blake3 = Some(false);
                // REGRA #0 potencialização: an unindexed file means the fields
                // below under-report (dependents / pub_symbols read stale data).
                // Carry the ready-to-run repair command with the real project
                // root so the reader can restore the signal source.
                data.stale_index_hint = Some(format!(
                    "touring index rebuild --dir {}",
                    rt.project_root.display()
                ));
            }
            Err(_) => {}
        }

        // Gotcha matches for the file.
        let gotchas = rt.ctx.knowledge.get_gotchas_for_file(&rel);
        if !gotchas.is_empty() {
            data.gotcha_matches = gotchas.iter().take(3).map(|g| g.gotcha.clone()).collect();
        }

        // Dependents count (inverse imports).
        if let Ok(deps) = rt.ctx.knowledge.get_dependents(&rel) {
            data.dependent_count = Some(deps.len() as u32);
        }

        // Pub symbol count in this file.
        if let Some(ref store) = rt.infra.symbol_store {
            if let Ok(syms) = store.find_symbols_in_file(&rel) {
                data.pub_symbol_count = Some(syms.len() as u32);
            }
        }

        // Cognitive complexity score — from `cognitive_enrichment` table.
        // `get_cognitive_enrichment` returns `Option<CognitiveScores>` where
        // CognitiveScores = (cognitive_score: f64, complexity_signal: f64,
        //                     fan_in_signal: f64, fan_out_signal: f64, doc_signal: f64).
        // We take field 0 and cast to f32.  Fail-open: any error → None.
        if let Ok(Some(scores)) = rt.ctx.knowledge.get_cognitive_enrichment(&rel) {
            data.cognitive_score = Some(scores.0 as f32);
        }
    }

    // P8.7 — Workflow Intelligence: stage detection + next-step advice.
    //
    // Uses a fresh (empty) WorkflowState because cli_suggester is stateless —
    // it doesn't persist tool history across invocations.  The stage is inferred
    // purely from the ActionSignature of the current tool call, which is
    // sufficient for the single-call advisory hint.
    //
    // Fail-open: any panic or error in the workflow layer is caught and silently
    // discarded so the hook always exits 0.
    data.workflow_stage_hint = workflow_enrichment_hint(classifier);

    data
}

/// Build the workflow-layer enrichment hint for the current classifier output.
///
/// Returns `None` when nothing useful can be inferred.  Never panics —
/// all workflow calls are purely deterministic pure functions.
fn workflow_enrichment_hint(classifier: &ClassifierOutput) -> Option<String> {
    // Build a minimal ActionSignature from the classifier cluster so that
    // detect_stage / detect_antipattern can operate without the full daemon ctx.
    // We synthesise a lightweight sig from the cluster tag: the cluster string
    // maps 1:1 to tool_class via the classifier naming convention.
    let (tool_class, intent_class) = cluster_to_sig_classes(&classifier.cluster);
    use crate::action_signature::ContextQualifier;
    let sig = ActionSignature {
        tool_class: tool_class.to_owned(),
        intent_class: intent_class.to_owned(),
        context_qualifier: ContextQualifier::Plain,
    };
    let state = WorkflowState::new();

    let mut we = WorkflowEnrichment::default();

    // Stage detection + advice.
    let stage = detect_stage(&sig, &state);
    we.stage_label = Some(stage.label().to_owned());
    let advice = advise_next_step(stage, None);
    we.next_step_hint = Some(advice.next_step.to_owned());

    // Antipattern conversion hint (Bash only) — advisory Warn, never Deny.
    if let Some(ap) = detect_antipattern(&sig, &state) {
        let cv = conversion_for(ap.kind);
        if cv.should_surface() {
            we.antipattern_hint = Some(cv.as_hint());
        }
    }

    // Glob validation hint — surface when the classifier is file-enumeration
    // and the pattern was extracted from the cluster context.
    if classifier.cluster == "file-enumeration" {
        if let Some(ref pattern) = glob_pattern_from_classifier(classifier) {
            let result = validate_glob_pattern(pattern, None);
            if let Some(hint) = result.hint() {
                we.glob_hint = Some(hint.to_owned());
            }
        }
    }

    we.render()
}

/// Map the classifier `cluster` tag to `(tool_class, intent_class)` for use
/// in `ActionSignature`.  Conservative: unmapped clusters get generic values.
fn cluster_to_sig_classes(cluster: &str) -> (&'static str, &'static str) {
    match cluster {
        "symbol-lookup" => ("bash", "grep"),
        "pre-edit-rust" | "pre-edit-triage-rust" => ("edit", "plain"),
        "read-rust-comprehend" | "read-code-comprehend" => ("read", "plain"),
        "file-enumeration" => ("glob", "plain"),
        "new-tsx-component" | "new-ts-module" => ("write", "plain"),
        "system-health-precheck" => ("bash", "cargo"),
        "exec-gate-advisory" => ("bash", "plain"),
        _ => ("bash", "plain"),
    }
}

/// Extract the glob pattern string from the classifier output for validation.
/// The classifier stores the pattern in the `must[0].command` field as
/// `touring index files "<pattern>" --limit 200`.
fn glob_pattern_from_classifier(classifier: &ClassifierOutput) -> Option<String> {
    let cmd = classifier.must.first().map(|c| c.command.as_str())?;
    // Extract the quoted pattern from: touring index files "<pattern>" --limit 200
    let start = cmd.find('"')? + 1;
    let end = cmd.rfind('"')?;
    if end > start {
        Some(cmd[start..end].to_owned())
    } else {
        None
    }
}

/// Best-effort conversion of an absolute path to one relative to
/// `runtime.project_root`. Falls back to the original path if not under it.
fn make_relative_to_project(rt: &HookRuntime, path: &str) -> String {
    let root = rt.project_root.to_string_lossy();
    if let Some(stripped) = path.strip_prefix(root.as_ref()) {
        let s = stripped.trim_start_matches('/');
        if !s.is_empty() {
            return s.to_string();
        }
    }
    path.to_string()
}

// ── Slice 2: Error-lesson retrieval + ranking ────────────────────────────────
//
// Retrieves past-error lessons from three in-process sources:
//   1. `bash_outcomes` (failures for the same tool_class command)
//   2. `edit_history`  (edits with error_pattern, keyed by intent/language)
//   3. Memory DB       (action-scoped `outcome:<tool_class>:*:failure` keys)
//
// Plus the already-computed gotcha_matches from enrichment (source 4).
//
// All retrieval is fail-open: any DB error → empty Vec.  No `.unwrap()` / `.expect()`.
// Latency budget: ≤3ms total (SQLite queries LIMIT-bounded to 10–20 rows each).

/// A single ranked lesson item ready for injection.
#[derive(Debug, Clone)]
struct LessonItem {
    /// Short text displayed to the LLM (≤120 chars).
    text: String,
    /// Composite ranking score (higher = more important).
    score: f64,
    /// First 50 chars of the underlying error pattern — used for diversity dedup.
    pattern_prefix: String,
}

/// Severity weight per gotcha severity string.
fn severity_weight(severity: &str) -> f64 {
    match severity {
        "critical" => 3.0,
        "warning" => 2.0,
        _ => 1.0, // "info" or unknown
    }
}

/// Exponential recency weight with a 30-day half-life.
/// `age_days` is the number of days since the event.  Clamps to [0, 365].
fn recency_weight(age_days: f64) -> f64 {
    let age = age_days.clamp(0.0, 365.0);
    // half_life = 30d  →  decay = ln(2)/30
    let decay = std::f64::consts::LN_2 / 30.0;
    (-decay * age).exp()
}

/// Frequency weight saturating at 5 hits.
fn frequency_weight(hits: u32) -> f64 {
    (hits as f64 / 5.0).min(1.0)
}

/// Parse a SQLite datetime string `"YYYY-MM-DD HH:MM:SS"` and return age in days.
/// Falls back to 0.0 (= "just now") on parse failure — conservative (keeps item visible).
fn age_days_from_sqlite(ts: &str) -> f64 {
    // Attempt to parse via chrono if available; otherwise do a fast approximate parse.
    // We avoid a chrono dep here by computing from the epoch manually using only stdlib.
    // The precision goal is "roughly how many 30-day windows ago" — seconds don't matter.
    fn parse_ymd(s: &str) -> Option<(i64, u32, u32)> {
        let b = s.as_bytes();
        if b.len() < 10 {
            return None;
        }
        let y = std::str::from_utf8(&b[0..4]).ok()?.parse::<i64>().ok()?;
        let m = std::str::from_utf8(&b[5..7]).ok()?.parse::<u32>().ok()?;
        let d = std::str::from_utf8(&b[8..10]).ok()?.parse::<u32>().ok()?;
        Some((y, m, d))
    }
    // Reference: today's Julian Day Number (approximate — good to ±1d).
    let now_jdn = {
        // Use UNIX_EPOCH seconds → Julian Day (J2000 epoch offset).
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // JDN of 1970-01-01 = 2440588
        2440588 + secs / 86400
    };
    let Some((y, m, d)) = parse_ymd(ts) else {
        return 0.0;
    };
    // Simple JDN formula (valid for dates >= 1900).
    let a = (14u32.saturating_sub(m)) / 12;
    let yr = y + 4800 - a as i64;
    let mo = m + 12 * a - 3;
    let jdn_ts =
        d as i64 + (153 * mo as i64 + 2) / 5 + 365 * yr + yr / 4 - yr / 100 + yr / 400 - 32045;
    let diff = now_jdn as i64 - jdn_ts;
    diff.max(0) as f64
}

/// TTL for the cached federated-DB discovery. The PreToolUse hook fires on
/// every tool call, so the `~/.claude` filesystem scan behind
/// [`crate::cli_handlers::discover_canonical_dbs`] runs at most once per this
/// window; results are shared across all calls in between.
const FEDERATED_DB_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// Cached federated-DB lists: `(refreshed_at, memory_dbs, knowledge_dbs)`.
type FederatedDbSet = (
    std::time::Instant,
    std::sync::Arc<[std::path::PathBuf]>,
    std::sync::Arc<[std::path::PathBuf]>,
);

/// Process-wide cache of every project `memory.db` / `knowledge.db`, refreshed
/// lazily once per [`FEDERATED_DB_TTL`].
static FEDERATED_DB_CACHE: std::sync::Mutex<Option<FederatedDbSet>> = std::sync::Mutex::new(None);

/// True when a federated-DB cache entry refreshed at `refreshed_at` is still
/// within `ttl` as of `now`. Pure — extracted so the TTL boundary is unit-
/// testable without touching the process-global [`FEDERATED_DB_CACHE`].
/// Saturates on clock skew (`now` before `refreshed_at` → treated as fresh).
fn federated_cache_is_fresh(
    refreshed_at: std::time::Instant,
    ttl: std::time::Duration,
    now: std::time::Instant,
) -> bool {
    now.saturating_duration_since(refreshed_at) < ttl
}

/// Returns `(memory_dbs, knowledge_dbs)` — every project DB discovered under
/// `~/.claude` — for federated lesson retrieval. The filesystem scan is cached
/// for [`FEDERATED_DB_TTL`] so the hot-path hook pays it at most once per
/// window. Fail-open: a poisoned lock is recovered, never panics.
fn federated_db_paths(
    rt: &HookRuntime,
) -> (
    std::sync::Arc<[std::path::PathBuf]>,
    std::sync::Arc<[std::path::PathBuf]>,
) {
    let mut guard = FEDERATED_DB_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((refreshed_at, mem, know)) = guard.as_ref() {
        if federated_cache_is_fresh(*refreshed_at, FEDERATED_DB_TTL, std::time::Instant::now()) {
            return (std::sync::Arc::clone(mem), std::sync::Arc::clone(know));
        }
    }
    let claude_dir = crate::cli_handlers::touring_claude_dir();
    let mem_primary = touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
    let know_primary = touring_foundation::TouringConfig::knowledge_db_canonical(&rt.project_root);
    let mem: std::sync::Arc<[std::path::PathBuf]> =
        crate::cli_handlers::discover_canonical_dbs(&mem_primary, &claude_dir, "memory.db").into();
    let know: std::sync::Arc<[std::path::PathBuf]> =
        crate::cli_handlers::discover_canonical_dbs(&know_primary, &claude_dir, "knowledge.db")
            .into();
    *guard = Some((
        std::time::Instant::now(),
        std::sync::Arc::clone(&mem),
        std::sync::Arc::clone(&know),
    ));
    (mem, know)
}

/// Source 1 + 2 combined: failures from `bash_outcomes` (bash tool_class) and
/// from `edit_history` (edit/write tool_class), federated across every
/// project's knowledge DB so a failure recorded under any project is surfaced.
fn collect_db_lessons(rt: &HookRuntime, sig: &ActionSignature) -> Vec<LessonItem> {
    let (_, knowledge_dbs) = federated_db_paths(rt);
    let mut items = Vec::new();

    // ── Source 1: bash_outcomes (federated) ──────────────────────────────────
    if sig.tool_class == "bash" {
        // intent_class is the command_short (e.g. "cargo", "touring", "ruff").
        for (cmd_short, pattern, executed_at) in
            query_bash_failures(&knowledge_dbs, &sig.intent_class, 10)
        {
            let age = age_days_from_sqlite(&executed_at);
            let text = format!("Bash `{}` failed: {}", cmd_short, truncate(&pattern, 80));
            let score =
                severity_weight("warning") * recency_weight(age) * frequency_weight(1) * 0.7; // plain signature match weight
            items.push(LessonItem {
                text,
                score,
                pattern_prefix: truncate(&pattern, 50),
            });
        }
    }

    // ── Source 2: edit_history (federated) ───────────────────────────────────
    if sig.tool_class == "edit" || sig.tool_class == "write" {
        // intent_class is the file extension (e.g. "rs", "py", "ts").
        let lang = &sig.intent_class;
        for (pattern, age) in query_edit_failures(&knowledge_dbs, lang, 10) {
            let text = format!("Edit `{}` file failed: {}", lang, truncate(&pattern, 80));
            let score =
                severity_weight("warning") * recency_weight(age) * frequency_weight(1) * 0.7; // plain match weight
            items.push(LessonItem {
                text,
                score,
                pattern_prefix: truncate(&pattern, 50),
            });
        }
    }

    items
}

/// Federated query of `edit_history` for failures (non-null `error_pattern`)
/// for `language`, across every knowledge DB in `knowledge_dbs`. Returns
/// `(error_pattern, age_days)` pairs, at most `limit` rows per DB. Fail-open:
/// any SQL/IO error on a DB skips just that DB.
///
/// `edit_history` schema: `touring-analysis/src/e2e/schema_guard.rs:25`.
fn query_edit_failures(
    knowledge_dbs: &[std::path::PathBuf],
    language: &str,
    limit: usize,
) -> Vec<(String, f64)> {
    // Match by file extension in `file_path` OR by the `language` column.
    let ext_like = format!("%.{language}");
    let sql = "SELECT error_pattern,
                      julianday('now') - julianday(edited_at) as age_days
               FROM edit_history
               WHERE error_pattern IS NOT NULL
                 AND (file_path LIKE ?1 OR language = ?2)
               ORDER BY id DESC
               LIMIT ?3";
    let mut out: Vec<(String, f64)> = Vec::new();
    for db in knowledge_dbs {
        let Ok(conn) = rusqlite::Connection::open(db) else {
            continue;
        };
        let Ok(mut stmt) = conn.prepare(sql) else {
            continue;
        };
        let rows = stmt.query_map(rusqlite::params![ext_like, language, limit as i64], |row| {
            let pattern: String = row.get(0)?;
            let age: f64 = row.get::<_, f64>(1).unwrap_or(0.0);
            Ok((pattern, age))
        });
        if let Ok(rows) = rows {
            out.extend(rows.filter_map(|r| r.ok()));
        }
    }
    out
}

/// Federated query of `bash_outcomes` for failed runs of `command_short`,
/// across every knowledge DB in `knowledge_dbs`. Returns
/// `(command_short, error_pattern, executed_at)` tuples, at most `limit` rows
/// per DB; the caller derives age via [`age_days_from_sqlite`]. Fail-open per
/// DB.
fn query_bash_failures(
    knowledge_dbs: &[std::path::PathBuf],
    command_short: &str,
    limit: usize,
) -> Vec<(String, String, String)> {
    let sql = "SELECT command_short, error_pattern, executed_at
               FROM bash_outcomes
               WHERE command_short = ?1 AND success = 0
                 AND error_pattern IS NOT NULL
               ORDER BY executed_at DESC
               LIMIT ?2";
    let mut out: Vec<(String, String, String)> = Vec::new();
    for db in knowledge_dbs {
        let Ok(conn) = rusqlite::Connection::open(db) else {
            continue;
        };
        let Ok(mut stmt) = conn.prepare(sql) else {
            continue;
        };
        let rows = stmt.query_map(rusqlite::params![command_short, limit as i64], |row| {
            let cmd_short: String = row.get(0)?;
            let pattern: String = row.get(1)?;
            let executed_at: String = row.get(2)?;
            Ok((cmd_short, pattern, executed_at))
        });
        if let Ok(rows) = rows {
            out.extend(rows.filter_map(|r| r.ok()));
        }
    }
    out
}

/// Source 3: memory DB — action-scoped `outcome:<tool_class>:*:failure` keys,
/// federated across every project's `memory.db` so a transcript-mined lesson
/// from any project is injected. The current project's DB is queried first.
fn collect_memory_lessons(rt: &HookRuntime, sig: &ActionSignature) -> Vec<LessonItem> {
    let (memory_dbs, _) = federated_db_paths(rt);
    memory_dbs
        .iter()
        .flat_map(|db| collect_memory_lessons_one_db(db, sig))
        .collect()
}

/// Queries one `memory.db` for `outcome:<tool_class>:*:failure` rows and maps
/// them to ranked [`LessonItem`]s. Mirrors the `memory_recall_sql` approach
/// from `cli_handlers.rs`. Fail-open: any SQL/IO error → empty Vec.
fn collect_memory_lessons_one_db(
    mem_db_path: &std::path::Path,
    sig: &ActionSignature,
) -> Vec<LessonItem> {
    let conn = match rusqlite::Connection::open(mem_db_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    // Match keys like `outcome:<tool_class>:<intent_class>:failure` OR
    // `outcome:<tool_class>:*:failure` (broader class match).
    let key_prefix = format!("outcome:{}:%:failure", sig.tool_class);
    let mut stmt =
        match conn.prepare("SELECT key, value FROM memory_entries WHERE key LIKE ?1 LIMIT 15") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
    stmt.query_map(rusqlite::params![key_prefix], |row| {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        Ok((key, value))
    })
    .map(|rows| {
        rows.filter_map(|r| r.ok())
            .map(|(key, value)| {
                // Boost score when intent_class also matches.
                let sig_match = if key.contains(&sig.intent_class) {
                    1.0 // exact qualifier match
                } else {
                    0.7 // plain / cross-class
                };
                let text = format!("Past failure [{}]: {}", key, truncate(&value, 80));
                let score = severity_weight("warning")
                    * recency_weight(0.0) // no timestamp in memory_entries
                    * frequency_weight(1)
                    * sig_match;
                LessonItem {
                    text,
                    score,
                    pattern_prefix: truncate(&value, 50),
                }
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Source 4: gotcha_matches already in enrichment — fold them into the ranking.
fn collect_gotcha_lessons(enrichment: &EnrichmentData) -> Vec<LessonItem> {
    enrichment
        .gotcha_matches
        .iter()
        .map(|g| LessonItem {
            text: g.clone(),
            score: severity_weight("warning") * recency_weight(0.0) * frequency_weight(3),
            pattern_prefix: truncate(g, 50),
        })
        .collect()
}

/// Rank all lesson items, deduplicate by `pattern_prefix[:50]`, and return the
/// top-K entries that fit within `budget_chars` total.
fn rank_and_trim(mut items: Vec<LessonItem>, budget_chars: usize) -> Vec<LessonItem> {
    // Stable sort by descending score.
    items.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut seen_prefixes: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut result = Vec::new();
    let mut used = 0usize;

    for item in items {
        // Diversity: skip if we already have an item with the same pattern prefix.
        if seen_prefixes.contains(&item.pattern_prefix) {
            continue;
        }
        // Budget check: "- <text>\n" costs text.len() + 4 chars.
        let cost = item.text.len() + 4;
        if used + cost > budget_chars {
            break;
        }
        seen_prefixes.insert(item.pattern_prefix.clone());
        used += cost;
        result.push(item);
    }
    result
}

/// Truncate `s` to at most `max_chars` characters, appending `…` if truncated.
fn truncate(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Main entry point for Slice 2: retrieve + rank + render error lessons.
///
/// Returns `None` when there is nothing useful to inject (empty after ranking).
/// Never panics — all DB errors are swallowed internally.
fn retrieve_and_render_lessons(
    rt: &HookRuntime,
    sig: &ActionSignature,
    enrichment: &EnrichmentData,
) -> Option<String> {
    const LESSON_BUDGET: usize = 800;

    // Collect from all sources — each is independently fail-open.
    let mut all: Vec<LessonItem> = Vec::new();
    all.extend(collect_db_lessons(rt, sig));
    // `memory.db` lessons are keyed only by tool_class and carry no timestamp
    // (`recency_weight(0.0)` ≡ 1.0), so the same transcript-keyed failures
    // resurface on every invocation of a tool class — pure banner-blindness.
    // The actionable signal already comes from gotcha matches (context-specific)
    // plus recency-weighted `collect_db_lessons` (both kept on by default); this
    // generic, undated source is opt-in. Set TOURING_SUGGESTER_PAST_FAILURES=1.
    if std::env::var("TOURING_SUGGESTER_PAST_FAILURES").is_ok() {
        all.extend(collect_memory_lessons(rt, sig));
    }
    all.extend(collect_gotcha_lessons(enrichment));

    let ranked = rank_and_trim(all, LESSON_BUDGET);
    if ranked.is_empty() {
        return None;
    }

    let mut out = String::from("\n  \u{26a0} lições de erros passados para esta ação:");
    for item in &ranked {
        out.push_str(&format!("\n  - {}", item.text));
    }
    Some(out)
}

// ── Rendering ────────────────────────────────────────────────────────────────

fn render(s: &Suggestion) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "[TOURING SUGGEST · {} · conf={:.2}]\n",
        s.cluster, s.confidence
    ));

    // Enrichment line(s) — only emitted when at least one field is populated.
    let e = &s.enrichment;
    let mut enrich_parts: Vec<String> = Vec::new();
    if let Some(in_index) = e.symbol_in_index {
        let n = e.symbol_definition_count.unwrap_or(0);
        enrich_parts.push(format!(
            "symbol_in_index={} (defs={})",
            if in_index { "yes" } else { "no" },
            n
        ));
    }
    if let Some(idx) = e.file_is_indexed {
        enrich_parts.push(format!("file_indexed={}", if idx { "yes" } else { "no" }));
    }
    if let Some(n) = e.dependent_count {
        enrich_parts.push(format!("dependents={n}"));
    }
    if let Some(n) = e.pub_symbol_count {
        enrich_parts.push(format!("pub_symbols={n}"));
    }
    if !e.gotcha_matches.is_empty() {
        enrich_parts.push(format!("gotchas={}", e.gotcha_matches.len()));
    }
    if !enrich_parts.is_empty() {
        out.push_str("  Enrichment: ");
        out.push_str(&enrich_parts.join(", "));
        out.push('\n');
    }
    if let Some(hint) = &e.stale_index_hint {
        out.push_str(&format!(
            "  Stale-index: {hint}\n            // file absent from blake3 registry — enrichment above may under-report\n"
        ));
    }
    if !e.gotcha_matches.is_empty() {
        out.push_str("  Gotcha hits:\n");
        for g in &e.gotcha_matches {
            out.push_str(&format!("    · {g}\n"));
        }
    }

    // Commands.
    for c in &s.must {
        out.push_str(&format!(
            "  MUST    {}\n            // {}\n",
            c.command, c.purpose
        ));
    }
    for c in &s.should {
        out.push_str(&format!(
            "  SHOULD  {}\n            // {}\n",
            c.command, c.purpose
        ));
    }
    for c in &s.may {
        out.push_str(&format!(
            "  MAY     {}\n            // {}\n",
            c.command, c.purpose
        ));
    }

    out.push_str(&format!("  Reason: {}\n", s.reason));
    out.push_str(&format!(
        "  (cached {SUGGESTION_TTL_SECS}s — set TOURING_SUGGESTER_DISABLED=1 to silence)"
    ));
    out
}

// ── Code Mode induction (C8) ───────────────────────────────────────────────────
//
// When the LLM issues an explicit shell loop or a repeated scan (grep/rg/find/
// Grep) within the live window, suggest collapsing the work into one
// `touring_ctx_execute` sandbox run instead of N atomic tool round-trips —
// Anthropic CodeAct / "Code Mode": 30-200× token compression. The hint takes
// priority over the per-call classifier at the moment the repeated-work pattern
// becomes apparent (the higher-order nudge is worth more than the Nth per-call
// hint); the 1st and 3rd+ scans still get the normal per-call suggestion.

/// What kind of repeated-work pattern triggered the Code Mode hint.
enum CodeModeKind {
    /// An explicit shell iteration construct (`for … in … do`, `while read`,
    /// `xargs`) — fires on first sight (unambiguous fan-out).
    Loop,
    /// A single atomic search (grep/rg/find/Grep) — contributes to the window
    /// counter; fires only on the threshold-crossing edge.
    Scan,
}

/// True iff `command` starts an atomic content/file search that `ctx_execute`
/// could fold into one pass.
fn is_scan_command(command: &str) -> bool {
    let c = command.trim_start();
    c.starts_with("grep ")
        || c.starts_with("grep\t")
        || c.starts_with("rg ")
        || c.starts_with("rg\t")
        || c.starts_with("egrep ")
        || c.starts_with("fgrep ")
        || c.starts_with("ag ")
        || (c.starts_with("find ") && c.contains("-name"))
}

/// True iff `command` contains an explicit shell iteration construct that fans a
/// per-item operation across a set — the canonical "do this N times" that one
/// `ctx_execute` run collapses.
fn is_shell_loop(command: &str) -> bool {
    let for_loop = command.contains("for ") && command.contains(" in ") && command.contains("do");
    let while_loop =
        command.contains("while ") && command.contains("read") && command.contains("do");
    for_loop || while_loop || command.contains("xargs")
}

/// Classify a tool call into a [`CodeModeKind`], or `None` when it is neither a
/// loop nor a scan. Pure — the window counter lives in [`detect_code_mode`].
/// `Read` is deliberately excluded (too frequent → noisy); an explicit `for`
/// loop over files already covers the read-in-loop case with high precision.
fn code_mode_kind(tool_name: &str, tool_input: &Value) -> Option<CodeModeKind> {
    match tool_name {
        "Bash" => {
            let command = tool_input.get("command").and_then(|v| v.as_str())?;
            if is_shell_loop(command) {
                Some(CodeModeKind::Loop)
            } else if is_scan_command(command) {
                Some(CodeModeKind::Scan)
            } else {
                None
            }
        }
        // A Grep tool call is itself an atomic search.
        "Grep" => Some(CodeModeKind::Scan),
        _ => None,
    }
}

/// Compose a concrete, ready-to-run **code-mode-without-MCP** command from the real
/// tool input. Per the MCP/Anthropic "programmatic tool calling" best practice the
/// snippet filters in the sandbox and prints only the digest (count + first hits),
/// never the raw matches. Targets `touring run` (the CLI code-mode channel) so no MCP
/// server is required. Pattern + glob travel as JSON `--args` (read from `sys.argv`),
/// so no regex/glob escaping leaks into `--code`. `None` when the input cannot be
/// specialized — the caller then emits the generic `touring run` template.
fn code_mode_command(tool_name: &str, tool_input: &Value) -> Option<String> {
    let (pattern, glob) = extract_scan_target(tool_name, tool_input)?;
    let args = serde_json::json!([pattern, glob]);
    let code = r#"import sys,glob,re; pat,g=sys.argv[1],sys.argv[2]; hits=[(f,i+1) for f in glob.glob(g,recursive=True) for i,l in enumerate(open(f,encoding="utf-8",errors="ignore")) if re.search(pat,l)]; print(len(hits),"hits"); [print(f"{f}:{n}") for f,n in hits[:30]]"#;
    Some(format!(
        "touring run --lang python --args '{args}' --code '{code}'"
    ))
}

/// Extract the iterated glob from a `for VAR in GLOB …` loop — the one piece of an
/// arbitrary shell loop that IS mechanically derivable. The per-item body is not
/// (it needs a bash parser), so it stays a marked placeholder downstream: honest
/// density, never a guessed translation. `None` for numeric / command-substitution
/// loops (no glob to carry).
fn loop_glob(command: &str) -> Option<String> {
    let after_in = command.split(" in ").nth(1)?;
    let first = after_in
        .split(';')
        .next()?
        .split_whitespace()
        .next()?
        .trim_matches(|c| c == '"' || c == '\'');
    (first.contains('*') || first.contains('/')).then(|| first.to_string())
}

/// Specialize a loop into a concrete `touring run` carrying the real glob (so the
/// nudge shows the actual file set, per the injection-density invariant); the
/// per-file op is the one marked placeholder. `None` when no glob is derivable —
/// the caller then falls back to the fully-generic template.
fn loop_code_mode_command(command: &str) -> Option<String> {
    let glob = loop_glob(command)?;
    let args = serde_json::json!([glob]);
    // Double quotes inside the python body: the whole snippet is wrapped in
    // single quotes on the shell line, so embedded single quotes would split
    // the wrapper and break the suggested command (density invariant demands
    // a RUNNABLE nudge, not merely a specific one).
    let code = "import glob,sys; files=glob.glob(sys.argv[1],recursive=True); print(len(files),\"files\")  # then your per-file op over files; print only the digest";
    Some(format!(
        "touring run --lang python --args '{args}' --code '{code}'"
    ))
}

/// Render the real shell command verbatim as a `touring run --lang bash` sandbox call.
/// The density-correct fallback for an arbitrary loop whose glob is not mechanically
/// derivable (e.g. a command-substitution iterable like `$(pgrep …)`): the ACTUAL
/// command travels — no guessed python translation, no `<placeholder>` (the loop body
/// IS derivable, just as bash). Embedded single quotes use the `'\''` shell idiom;
/// over-long commands are capped so the nudge stays dense (high signal-to-token).
fn bash_code_mode_command(command: &str) -> String {
    const MAX: usize = 200;
    let body: String = if command.chars().count() > MAX {
        let head: String = command.chars().take(MAX).collect();
        format!("{head}…")
    } else {
        command.to_string()
    };
    let escaped = body.replace('\'', r"'\''");
    format!("touring run --lang bash --code '{escaped}'")
}

/// The most specific `touring run` command for a fired Code Mode kind: a scan
/// carries (pattern, glob); a loop carries its glob. `None` ⇒ the caller emits the
/// generic template (the honest fallback when nothing is derivable).
fn specialized_command(kind: &CodeModeKind, tool_name: &str, tool_input: &Value) -> Option<String> {
    match kind {
        CodeModeKind::Scan => code_mode_command(tool_name, tool_input),
        CodeModeKind::Loop => {
            loop_code_mode_command(tool_input.get("command").and_then(Value::as_str)?)
        }
    }
}

/// Extract `(regex_pattern, recursive_glob)` from a scan — the structured `Grep` tool
/// (high precision) or a `grep`/`rg` Bash command (best-effort). `None` for inputs that
/// are not scans (e.g. shell loops) so the caller falls back to the generic template.
fn extract_scan_target(tool_name: &str, tool_input: &Value) -> Option<(String, String)> {
    match tool_name {
        "Grep" => {
            let pattern = tool_input.get("pattern").and_then(Value::as_str)?;
            if pattern.is_empty() {
                return None;
            }
            let path = tool_input
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or(".");
            let glob = tool_input.get("glob").and_then(Value::as_str);
            Some((pattern.to_string(), scan_glob(path, glob)))
        }
        "Bash" => {
            let command = tool_input.get("command").and_then(Value::as_str)?;
            if !is_scan_command(command) {
                return None;
            }
            parse_grep_command(command)
        }
        _ => None,
    }
}

/// Build a recursive glob from a Grep `path` + optional `glob` filter:
/// `("crates/", Some("*.rs"))` → `"crates/**/*.rs"`.
fn scan_glob(path: &str, glob: Option<&str>) -> String {
    let base = path.trim_end_matches('/');
    let base = if base.is_empty() { "." } else { base };
    match glob {
        Some(g) => format!("{base}/**/{g}"),
        None => format!("{base}/**/*"),
    }
}

/// Best-effort `(pattern, glob)` from a `grep`/`rg` command: the first non-flag token
/// after the command word is the pattern (unquoted); a later token containing `/` is
/// the path; `--include=GLOB` narrows the filter.
fn parse_grep_command(command: &str) -> Option<(String, String)> {
    let mut toks = command.split_whitespace();
    let _verb = toks.next()?; // grep / rg / egrep / …
    let mut pattern: Option<String> = None;
    let mut path = ".";
    for t in toks {
        if t.starts_with('-') {
            continue; // skip flags (best-effort; ignores flags that take a value)
        }
        if pattern.is_none() {
            pattern = Some(t.trim_matches(|c| c == '"' || c == '\'').to_string());
        } else if t.contains('/') || t == "." {
            path = t;
        }
    }
    let pattern = pattern.filter(|p| !p.is_empty())?;
    Some((pattern, scan_glob(path, include_filter(command).as_deref())))
}

/// `GLOB` from `--include=GLOB` in a grep command, unquoted, if present.
fn include_filter(command: &str) -> Option<String> {
    let rest = command.split("--include=").nth(1)?;
    let g = rest
        .split_whitespace()
        .next()?
        .trim_matches(|c| c == '"' || c == '\'');
    (!g.is_empty()).then(|| g.to_string())
}

/// Cap a command for a dense nudge (high signal-to-token): the real command, trimmed
/// and truncated with `…` past `max` chars. Specific (real content), yet bounded — so
/// `touring exec "<command>"` becomes the actual command instead of a `<placeholder>`.
fn cmd_excerpt(command: &str, max: usize) -> String {
    let c = command.trim();
    if c.chars().count() > max {
        let head: String = c.chars().take(max).collect();
        format!("{head}…")
    } else {
        c.to_string()
    }
}

/// The `*.ext` value from `find … -name VALUE` — the glob a `find -name` scan iterates,
/// so the `index files` nudge carries the real pattern instead of `<pattern>`.
fn find_name_glob(command: &str) -> Option<String> {
    let rest = command.split("-name").nth(1)?;
    let g = rest
        .split_whitespace()
        .next()?
        .trim_matches(|c| c == '"' || c == '\'');
    (!g.is_empty()).then(|| g.to_string())
}

/// The file an inline `sed -i`/`awk -i inplace`/`perl -pi` edit targets — the last
/// path-like token (skips flags and the substitution script). Lets the Edit tool /
/// ast-meta nudge carry the real path instead of `<file>`. `None` when not derivable.
fn inline_edit_target(command: &str) -> Option<String> {
    command
        .split_whitespace()
        .map(|t| t.trim_matches(|c| c == '"' || c == '\''))
        .filter(|t| !t.starts_with('-'))
        .rev()
        .find(|t| t.contains('/') || (t.contains('.') && !t.contains('*')))
        .map(std::string::ToString::to_string)
}

/// Build the Code Mode [`ClassifierOutput`] for a fired [`CodeModeKind`]. When the
/// real input is a specializable scan, `must` is a **concrete** `touring run` command
/// (pattern + glob derived from the input); otherwise it is the generic `touring run`
/// template. Either way the channel is the CLI sandbox (R1), never the MCP tool — the
/// goal is code-mode **without** MCP. Pure and unit-testable; no symbol/file hint, so
/// the generic-banner dedupe ([`cluster_dedupe_gate`]) also caps it at once per window.
fn code_mode_output(kind: &CodeModeKind, tool_name: &str, tool_input: &Value) -> ClassifierOutput {
    let (cluster, reason) = match kind {
        CodeModeKind::Loop => (
            "code-mode-loop",
            "Explicit shell loop fans a per-item op across a set. One `touring run` \
             executes the whole loop in the sandbox (1 call vs N) — code-mode WITHOUT \
             MCP, 30-200× token compression (Anthropic CodeAct / programmatic tool calling).",
        ),
        CodeModeKind::Scan => (
            "code-mode-scan",
            "Repeated atomic search this window. Run it once via `touring run` \
             (code-mode WITHOUT MCP): the sandbox walks all files in a single pass and \
             returns only the digest — N round-trips collapse to 1.",
        ),
    };
    // `must` always carries a CONCRETE command (injection-density invariant): the
    // specialized form when a pattern/glob is derivable, else the real shell command
    // verbatim as `--lang bash`. The generic `<placeholder>` template is never emitted
    // for a Bash trigger — the loop body IS derivable, just not as a python translation.
    let must_command = specialized_command(kind, tool_name, tool_input).unwrap_or_else(|| {
        bash_code_mode_command(
            tool_input
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or(""),
        )
    });
    let must = vec![cmd(
        must_command,
        "Code Mode without MCP — one `touring run` computes in the sandbox and \
         returns only the result (N calls → 1)",
    )];
    // `should` only when a real free-text pattern is derivable — otherwise omit it
    // rather than emit a `<pattern>` placeholder (density: specific or absent).
    let should = extract_scan_target(tool_name, tool_input)
        .map(|(pattern, _glob)| {
            vec![cmd(
                format!("touring tantivy search \"{pattern}\""),
                "if it is one free-text search, BM25-ranked hits in <10ms (no code)",
            )]
        })
        .unwrap_or_default();
    ClassifierOutput {
        cluster: cluster.into(),
        must,
        should,
        may: vec![],
        reason: reason.into(),
        confidence: 0.95,
        symbol_hint: None,
        file_hint: None,
    }
}

/// Code Mode induction gate (C8): returns a ready [`ClassifierOutput`] when the
/// call is an explicit loop (fires immediately) or a repeated scan that crosses
/// the window threshold; `None` otherwise. Bypasses the conformal gate by design
/// — detection is precise (explicit syntax or a counted burst), unlike the fuzzy
/// regex classifier the gate guards.
fn detect_code_mode(tool_name: &str, tool_input: &Value) -> Option<ClassifierOutput> {
    match code_mode_kind(tool_name, tool_input)? {
        CodeModeKind::Loop => Some(code_mode_output(&CodeModeKind::Loop, tool_name, tool_input)),
        CodeModeKind::Scan => {
            if scan_window_crosses_threshold() {
                Some(code_mode_output(&CodeModeKind::Scan, tool_name, tool_input))
            } else {
                None
            }
        }
    }
}

/// Pick the classifier output for `run`: Code Mode induction (C8) takes priority
/// when it fires, otherwise the per-tool classifier gated by the conformal
/// threshold. Extracted from `run` to keep its control flow flat (CC ≤ 15).
fn select_classifier(
    rt: &HookRuntime,
    tool_name: &str,
    tool_input: &Value,
) -> Option<ClassifierOutput> {
    if let Some(code_mode) = detect_code_mode(tool_name, tool_input) {
        return Some(code_mode);
    }
    let gate = conformal_gate_threshold(rt);
    match classify(tool_name, tool_input) {
        Some(c) if c.confidence >= gate => Some(c),
        _ => None,
    }
}

// ── Public entry point ───────────────────────────────────────────────────────

/// F2 — per-session "a redirect was just suggested" marker. Set when the
/// suggester emits; consumed on the session's next PreToolUse to measure
/// suggestion-uptake. moka TTL bounds stale sessions (same idiom as `cache`).
fn pending_suggestion() -> &'static moka::sync::Cache<String, ()> {
    static PENDING: OnceLock<moka::sync::Cache<String, ()>> = OnceLock::new();
    PENDING.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(SUGGESTION_TTL_SECS))
            .build()
    })
}

/// F2 — session key for the uptake cache. Claude Code threads `session_id` into
/// every hook payload; absent it (non-CC callers), a single shared slot is used.
fn session_key(payload: &Value) -> String {
    payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string()
}

/// F2 — did the current action follow a coupling redirect? True when the tool is
/// a `touring …` CLI / `touring run` code-mode invocation (the redirect target of
/// ~every suggestion). Conservative 1-step window (doc §9): anything else counts
/// as "not followed".
fn action_is_touring_redirect(tool_name: &str, tool_input: &Value) -> bool {
    if !tool_name.eq_ignore_ascii_case("bash") {
        return false;
    }
    tool_input
        .get("command")
        .and_then(|v| v.as_str())
        .is_some_and(|cmd| cmd.split_whitespace().any(|tok| tok == "touring"))
}

/// F3 — which side of the prior-bash→prior-touring axis a `Bash` action falls on.
/// Non-bash tools and neutral bash (`cargo`, `ls`, `jq`) map to `None` — counted
/// in neither the numerator nor the denominator of `adoption_ratio`.
#[derive(Debug, PartialEq, Eq)]
enum AdoptionClass {
    /// `Bash` invocation of `touring` — the prior-touring side (numerator).
    Touring,
    /// Raw-shell inspection antipattern (grep/cat/find/sed) — the prior-bash side.
    Antipattern,
}

/// F3 — classify an action for `adoption_ratio` (doc §9, the mother coupling KPI).
///
/// Reuses F2's [`action_is_touring_redirect`] (numerator) and the shared
/// [`detect_antipattern`] detector (denominator). Only **raw-bash** antipatterns
/// count: gating on `tool_class == "bash"` excludes the stateful Edit/Read hygiene
/// antipatterns, which would false-fire under the stateless empty `WorkflowState`.
/// Pure + infallible (no daemon, no enrichment) — `from_pre_tool` derives
/// `tool_class`/`intent_class` from `tool_name` + command alone.
fn classify_adoption(tool_name: &str, tool_input: &Value) -> Option<AdoptionClass> {
    if action_is_touring_redirect(tool_name, tool_input) {
        return Some(AdoptionClass::Touring);
    }
    let sig = ActionSignature::from_pre_tool(tool_name, tool_input, None, 0, None, None);
    if sig.tool_class == "bash" && detect_antipattern(&sig, &WorkflowState::new()).is_some() {
        return Some(AdoptionClass::Antipattern);
    }
    None
}

/// F3 — classify the current action and fold it into the adoption_ratio counters.
/// Extracted from `run` to keep the hot path flat. Fail-open + infallible.
fn record_adoption(tool_name: &str, tool_input: &Value) {
    match classify_adoption(tool_name, tool_input) {
        Some(AdoptionClass::Touring) => crate::shared::gate_metrics::record_adoption_touring(),
        Some(AdoptionClass::Antipattern) => {
            crate::shared::gate_metrics::record_adoption_antipattern()
        }
        None => {}
    }
}

// ── Task #6 — pillar induction (the active layer of the compounding structure) ──
//
// rule + skill + CLAUDE.md-pointer are the passive layers (knowledge); the
// empirical lesson (cont.¹⁰) is that passive knowledge does NOT induce — the
// builder had every master command documented and still reached for atomic tools.
// So this layer actively nudges the two pillars the upstream classifiers miss:
//   • `MasterCli`   — the proven adoption gap: an atomic `touring index/ast/wiring`
//                     call where a fused master command (scout/read/map/blast/
//                     investigate/guard) serves better.
//   • `LearningMemory` — raw-shell search of docs/history where `touring memory
//                     recall` may already hold the answer (Reflexo #3).
// `CodeMode` and `Intelligence` already fire upstream (C8 `detect_code_mode`, the
// read-rust classifier), so `classify_pillar` stays silent for them — no double
// nudge. Graduated DEFAULT-OFF, mirroring the F7c actuator: unset env ⇒ no
// classification, no emission, zero live impact. The loop closes via the
// `pillar_induction_{emitted,followed}` counters → `pillar_induction_ratio` KPI →
// F7. Per the roadmap thesis (affordance, not persuasion), if uptake stays low
// while armed, that telemetry is the evidence that pushes toward affordance
// (productization) — the experiment, not just the nudge. Gabriel arms it post-A/B.

/// Is the pillar-induction layer armed? `TOURING_PILLAR_INDUCTION_ARMED` unset
/// (or `0`) ⇒ OFF. Mirrors [`f7_actuator_armed`]: arming is a human decision so
/// the shipped default never changes live behaviour.
fn pillar_induction_armed() -> bool {
    std::env::var("TOURING_PILLAR_INDUCTION_ARMED").is_ok_and(|v| v != "0")
}

/// The pillars this active layer induces — the two compounding differentials the
/// upstream classifiers miss. The structure's other two pillars are already induced
/// by existing hook classifiers (code-mode by C8 `detect_code_mode`, intelligence
/// by the read-rust classifier — both observed firing live), so they are not
/// re-modelled here: the compounding structure covers all four across its four
/// layers; this enum names only what the new layer adds. `as_tag` is the stable
/// cluster/telemetry identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pillar {
    /// `touring scout/read/map/blast/investigate/guard` — fuse N atomic calls.
    MasterCli,
    /// `touring memory recall` — reuse prior lessons before researching anew.
    LearningMemory,
}

impl Pillar {
    fn as_tag(self) -> &'static str {
        match self {
            Pillar::MasterCli => "master-cli",
            Pillar::LearningMemory => "learning-memory",
        }
    }
}

/// Map the action the LLM is about to take to the pillar that would serve it
/// better — but only for the two pillars the upstream classifiers miss. Pure +
/// infallible (no daemon, no state). `None` for everything else, including the
/// `CodeMode`/`Intelligence` actions already covered by `select_classifier`.
fn classify_pillar(tool_name: &str, tool_input: &Value) -> Option<Pillar> {
    if !tool_name.eq_ignore_ascii_case("bash") {
        return None;
    }
    let command = tool_input.get("command").and_then(Value::as_str)?;
    // MasterCli: an atomic `touring <verb>` discovery call a master would fuse —
    // the empirically-proven adoption gap (the cobrança).
    if master_cli_command(command).is_some() {
        return Some(Pillar::MasterCli);
    }
    // LearningMemory: raw-shell search over docs/history/lessons — recall first.
    // Specific-or-absent (injection-density invariant): only when the search
    // topic is mechanically derivable, so the recall query is never a placeholder.
    if is_memory_search(command) && parse_grep_command(command).is_some() {
        return Some(Pillar::LearningMemory);
    }
    None
}

/// The atomic → master mapping table: one row per fuseable `touring` verb pair,
/// with whether the master carries the atomic's first argument. Split out of
/// [`master_cli_command`] to keep both functions under the complexity gate.
fn atomic_to_master(verb1: &str, verb2: &str) -> Option<(&'static str, bool)> {
    Some(match (verb1, verb2) {
        ("index", "find") | ("ast", "find") | ("tantivy", "search") => ("scout", true),
        ("ast", "blast") | ("wiring", "impact") => ("blast", true),
        ("ast", "meta") | ("ast", "overview") => ("read", true),
        ("ast", "tdg") => ("guard", true),
        ("wiring", "orphans") => ("guard", false),
        ("wiring", "audit") => ("investigate", false),
        _ => return None,
    })
}

/// Derive the fused master command from an atomic `touring …` discovery call:
/// `touring index find Foo` → `touring scout Foo`; `touring ast blast f.rs` →
/// `touring blast f.rs`. Returns `(must_command, master_name, carried_arg)`; `None`
/// when the command is not a fuseable atomic. Carrying the real argument over is
/// what makes the nudge actionable — generic banners do not induce (cont.¹⁰).
fn master_cli_command(command: &str) -> Option<(String, String, Option<String>)> {
    let toks: Vec<&str> = command.split_whitespace().collect();
    let pos = toks.iter().position(|&t| t == "touring")?;
    let rest = &toks[pos + 1..];
    let (master, needs_arg) = atomic_to_master(rest.first()?, rest.get(1).copied().unwrap_or(""))?;
    let arg = rest
        .get(2)
        .map(|s| s.trim_matches(|c| c == '"' || c == '\'').to_string())
        .filter(|_| needs_arg);
    let must = match &arg {
        Some(a) => format!("touring {master} {a}"),
        None => format!("touring {master}"),
    };
    Some((must, master.to_string(), arg))
}

/// True when a `grep`/`rg`/`ag` command searches docs / memory / lessons — the
/// case where `touring memory recall` reuses a prior answer instead of starting
/// from scratch (Reflexo #3). Conservative: only raw-shell search of knowledge
/// surfaces, never code identifiers.
fn is_memory_search(command: &str) -> bool {
    let first = command.split_whitespace().next().unwrap_or("");
    let is_search = matches!(first, "grep" | "rg" | "ag" | "egrep" | "ugrep");
    is_search
        && [
            "docs/",
            "memory",
            "lesson",
            "/.claude/",
            "diary",
            "CHANGELOG",
        ]
        .iter()
        .any(|kw| command.contains(kw))
}

/// Did the action following a pillar nudge actually take the pillar — i.e. invoke
/// a master command or `memory recall`? Precise predicate (narrower than F2's
/// broad [`action_is_touring_redirect`]): the pillar layer measures whether the
/// LLM reached for the *differential*, not just any `touring` call.
fn action_followed_pillar(tool_name: &str, tool_input: &Value) -> bool {
    if !tool_name.eq_ignore_ascii_case("bash") {
        return false;
    }
    let cmd = tool_input
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("");
    let toks: Vec<&str> = cmd.split_whitespace().collect();
    let Some(pos) = toks.iter().position(|&t| t == "touring") else {
        return false;
    };
    let next = toks.get(pos + 1).copied();
    matches!(
        next,
        Some("scout" | "read" | "map" | "blast" | "investigate" | "guard" | "audit")
    ) || (next == Some("memory") && toks.get(pos + 2) == Some(&"recall"))
}

/// Build the synthetic pillar-induction [`ClassifierOutput`] (armed-only). It
/// flows through the normal `run` pipeline (cluster dedupe, enrich, F7c gate,
/// render, F2 emit), so a followed nudge already counts in `suggestion_uptake`;
/// the pillar counters add the per-layer dimension F7 needs. `None` when disarmed
/// or no pillar applies — the default-OFF guard lives here so `run` stays flat.
fn pillar_classifier(tool_name: &str, tool_input: &Value) -> Option<ClassifierOutput> {
    if !pillar_induction_armed() {
        return None;
    }
    let pillar = classify_pillar(tool_name, tool_input)?;
    let command = tool_input
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("");
    match pillar {
        Pillar::MasterCli => Some(master_cli_nudge(command, pillar)),
        Pillar::LearningMemory => Some(learning_memory_nudge(command, pillar)),
    }
}

/// Build the MasterCli nudge — fully specific: the MUST is the derived master
/// command (real argument carried), the SHOULD appears only when a concrete symbol
/// can travel with it, and the rationale names the principle (decision-matrix
/// C03/C04, code-mode without MCP). No placeholder when the input permits
/// derivation — the injection-density invariant (feedback 2026-06-29).
fn master_cli_nudge(command: &str, pillar: Pillar) -> ClassifierOutput {
    // `master_cli_command` already proved `Some` in `classify_pillar`; the
    // `unwrap_or_else` is a defensive total fallback, never a panic path.
    let (must_cmd, master, arg) = master_cli_command(command)
        .unwrap_or_else(|| ("touring scout".to_string(), "scout".to_string(), None));
    let should = match &arg {
        Some(sym) if master == "scout" => vec![cmd(
            format!("touring investigate \"{sym}\""),
            "same symbol, broader question: search + index + wiring + memory in one pass",
        )],
        _ => vec![],
    };
    ClassifierOutput {
        cluster: format!("pillar-{}", pillar.as_tag()),
        must: vec![cmd(
            must_cmd,
            "master command — one call fuses the index/ast/wiring lookups N atomic \
             `touring` calls would take (Touring decision-matrix C03/C04; code-mode \
             without MCP, Anthropic programmatic tool calling)",
        )],
        should,
        may: vec![],
        reason: format!(
            "Atomic `touring {master}`-class lookup. The master commands you built fuse \
             these into one call — using them is the differential under-used (cont.¹⁰)."
        ),
        confidence: 0.9,
        symbol_hint: None,
        file_hint: None,
    }
}

/// Build the LearningMemory nudge — the search term travels from the grep into a
/// concrete `recall`/`investigate`, so the suggestion shows the exact query to run,
/// not a placeholder (injection-density invariant). Grounded in Reflexo #3 / C09.
fn learning_memory_nudge(command: &str, pillar: Pillar) -> ClassifierOutput {
    // `classify_pillar` admits LearningMemory only when the topic parses; the
    // fallback is defensive-only and still carries the real command excerpt
    // (density invariant — never a placeholder).
    let topic = parse_grep_command(command)
        .map(|(p, _)| p)
        .unwrap_or_else(|| cmd_excerpt(command, 40));
    ClassifierOutput {
        cluster: format!("pillar-{}", pillar.as_tag()),
        must: vec![cmd(
            format!("touring memory recall \"{topic}\""),
            "reuse a prior lesson/answer before researching docs from scratch (Reflexo #3 / \
             decision-matrix C09)",
        )],
        should: vec![cmd(
            format!("touring investigate \"{topic}\""),
            "if recall misses: topic map across search + index + wiring + memory",
        )],
        may: vec![],
        reason: format!(
            "Raw-shell search of docs/history for \"{topic}\". `touring memory recall` may \
             already hold the answer — learning-memory is a differential under-used."
        ),
        confidence: 0.85,
        symbol_hint: None,
        file_hint: None,
    }
}

/// Task #6 — per-session "a pillar nudge was just emitted" marker, parallel to
/// [`pending_suggestion`] so the pillar layer's follow-through is measured with
/// its precise predicate without disturbing F2's `suggestion_uptake`. moka TTL
/// bounds stale sessions (same idiom as `pending_suggestion`).
fn pending_pillar() -> &'static moka::sync::Cache<String, ()> {
    static PENDING_PILLAR: OnceLock<moka::sync::Cache<String, ()>> = OnceLock::new();
    PENDING_PILLAR.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(SUGGESTION_TTL_SECS))
            .build()
    })
}

/// Close the prior suggestion's uptake loop for this session: F2 (any redirect)
/// and the Task #6 pillar layer (a master/recall command, precise predicate).
/// Extracted from `run` to keep its control flow flat. Fail-open: pure cache ops
/// + Relaxed atomics, no error path.
fn eval_uptake(session: &str, tool_name: &str, tool_input: &Value) {
    if pending_suggestion().remove(session).is_some()
        && action_is_touring_redirect(tool_name, tool_input)
    {
        crate::shared::gate_metrics::record_suggestion_followed();
    }
    if pending_pillar().remove(session).is_some() && action_followed_pillar(tool_name, tool_input) {
        crate::shared::gate_metrics::record_pillar_induction_followed();
    }
}

/// Pick the classifier for `run`: the upstream `select_classifier` (C8 code-mode +
/// conformal per-tool) first, then the armed-only Task #6 pillar layer covering the
/// master-cli / learning-memory gap. The `bool` is `true` for a pillar nudge so
/// `run` can tag its emission for per-pillar telemetry. Extracted to keep `run`
/// under the complexity gate.
fn resolve_classifier(
    rt: &HookRuntime,
    tool_name: &str,
    tool_input: &Value,
) -> Option<(ClassifierOutput, bool)> {
    if let Some(c) = select_classifier(rt, tool_name, tool_input) {
        return Some((c, false));
    }
    pillar_classifier(tool_name, tool_input).map(|c| (c, true))
}

/// Record an emission's telemetry and arm the per-session uptake markers: the F2
/// redirect counter always, plus the Task #6 per-pillar counter + parallel marker
/// when `is_pillar`. Extracted from `run` to keep it under the complexity gate.
/// Fail-open: counter calls + cache inserts are infallible.
fn record_emission(context_len: usize, session: String, is_pillar: bool) {
    crate::shared::gate_metrics::record_enrichment_emitted(context_len);
    // F2: this emission is a redirect suggestion — count it and arm the uptake
    // measurement for this session's next action.
    crate::shared::gate_metrics::record_suggestion_emitted();
    pending_suggestion().insert(session.clone(), ());
    // Task #6: when this is the pillar layer's own nudge, count it and arm the
    // precise per-pillar follow-through (parallel cache so F2 is undisturbed).
    if is_pillar {
        crate::shared::gate_metrics::record_pillar_induction_emitted();
        pending_pillar().insert(session, ());
    }
}

/// Hook entry point — invoked by `touring-hook cli-suggest` (registered in
/// `hook_registry::ALL_DAEMON_HOOK_NAMES`).
///
/// Reads `tool_name` and `tool_input` from `payload`, evaluates suggestion-uptake
/// for the prior suggestion (F2), classifies, enriches with live daemon state,
/// applies TTL cache, and returns JSON. Always returns a valid JSON string
/// (empty `"{}"` when no suggestion fires).
pub fn run(rt: &HookRuntime, payload: &Value) -> String {
    if std::env::var("TOURING_SUGGESTER_DISABLED").is_ok() {
        return "{}".into();
    }

    let tool_name = match payload.get("tool_name").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return "{}".into(),
    };
    let empty = Value::Object(serde_json::Map::new());
    let tool_input = payload.get("tool_input").unwrap_or(&empty);

    // F2 suggestion-uptake (doc §9): every PreToolUse is the "next action" after a
    // possibly-pending redirect. Evaluate uptake BEFORE the anti-spam / no-classifier
    // early returns so a non-emitting call still closes the prior suggestion's loop.
    // Fail-open: pure cache + atomic ops, no error path.
    let session = session_key(payload);
    eval_uptake(&session, tool_name, tool_input);

    // F3 adoption_ratio (doc §9, the mother coupling KPI): classify EVERY Bash
    // action — touring-canonical vs raw-shell antipattern — BEFORE the anti-spam /
    // no-classifier early returns, so the denominator sees all actions (the TTL
    // cache below would otherwise drop repeated identical antipatterns from the
    // count). Fail-open: pure classification + Relaxed atomics, no error path.
    record_adoption(tool_name, tool_input);

    // TTL cache: anti-spam for identical (tool, input) pairs.
    let h = input_hash(tool_name, tool_input);
    if cache().get(&h).is_some() {
        return "{}".into();
    }

    // C8 + S-08/A-A1: Code Mode induction (repeated scan/loop → `ctx_execute`)
    // takes priority; otherwise the per-tool classifier fires only when its
    // confidence clears the *conformal* threshold (τ = 1 − q̂, coverage ≥ 1 − α),
    // not a hardcoded 0.7. Both paths live in `select_classifier` to keep this
    // hot path flat. See `detect_code_mode` and `crate::conformal`.
    let (classifier, is_pillar) = match resolve_classifier(rt, tool_name, tool_input) {
        Some(pair) => pair,
        None => return "{}".into(),
    };

    // Generic-banner cluster dedupe (see `cluster_dedupe_gate`): a banner that
    // carries no input-specific signal fires at most once per TTL window. The
    // input-hash cache above still anti-spams identical inputs; symbol/file-
    // specific suggestions are never deduped (each is fresh signal).
    if matches!(cluster_dedupe_gate(&classifier), ClusterDecision::Suppress) {
        return "{}".into();
    }

    let enrichment = enrich(rt, &classifier);
    let suggestion = Suggestion {
        cluster: classifier.cluster,
        must: classifier.must,
        should: classifier.should,
        may: classifier.may,
        reason: classifier.reason,
        confidence: classifier.confidence,
        enrichment,
    };

    // F7c (telemetry §12) — armed-only hint demotion. The suggestion has already
    // cleared the conformal gate (in `select_classifier`); when the actuator is armed
    // AND the A/B gate confirms the coupling, raise the bar by the demotion bump and
    // suppress hints that no longer clear it (the engine raises the bump when uptake
    // shows hints are being ignored). Default OFF (`TOURING_F7_ACTUATOR_ARMED` unset)
    // ⇒ no signal read, no suppression, zero live impact — Gabriel arms it post-A/B.
    if f7_actuator_armed() {
        let (uptake, ab) = crate::cli::kpi::actuator_signals();
        let bump = crate::cli::kpi::hint_demotion_bump(uptake, ab);
        if suggestion.confidence < conformal_gate_threshold(rt) + bump {
            return "{}".into();
        }
    }

    let mut context = render(&suggestion);

    // Compute the ActionSignature and append its key as an observable line.
    // `from_pre_tool_with_cognitive` / `to_key` are infallible pure functions (no error
    // path), so this additive step cannot affect the hook's normal output.
    // Slice 3 Part 1: thread cognitive_score into the signature so HiComplexity
    // can trigger when no blast-radius signal is present.
    let sig = ActionSignature::from_pre_tool_with_cognitive(
        tool_name,
        tool_input,
        suggestion.enrichment.dependent_count,
        suggestion.enrichment.cognitive_score,
        suggestion.enrichment.gotcha_matches.len(),
        suggestion.enrichment.file_is_indexed,
        suggestion.enrichment.symbol_in_index,
    );
    context.push_str(&format!("\n  sig={}", sig.to_key()));

    // Slice 2: inject ranked error-lesson section (fail-open — any retrieval
    // error returns None and we skip the section entirely).
    // Slice 3 Part 3: when a lesson is injected, write a one-shot flag so
    // post_tool_rl can award a quality bonus if the tool subsequently succeeds.
    // cache_result is infallible; any failure is silently absorbed by the hook.
    if let Some(lessons) = retrieve_and_render_lessons(rt, &sig, &suggestion.enrichment) {
        context.push_str(&lessons);
        // Write injection flag — post_tool_rl reads "__meta__" / "__action_sig_lesson_injected__".
        rt.ctx
            .result_cache
            .cache_result("__meta__", "__action_sig_lesson_injected__", "1".into());
    }

    cache().insert(h, ());

    // TR-2 — STR observability: record enrichment bytes only when context is
    // non-empty. `context.len()` is the UTF-8 byte length of additionalContext,
    // used as a token-count proxy for the Signal-to-Token Ratio metric.
    // Fail-open: the counter call is infallible (~1 ns, Relaxed atomic).
    if !context.is_empty() {
        record_emission(context.len(), session, is_pillar);
    }

    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "additionalContext": context
        }
    })
    .to_string()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "cli_suggester_tests.rs"]
mod tests;
