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
    detect_stage, exit_code_through_pipe, validate_glob_pattern,
};

// Needed by Slice 2 retrieval helpers (fail-open DB queries).
use rusqlite;

// ── Cache ────────────────────────────────────────────────────────────────────
//
// Anti-spam TTL: same (tool_name, tool_input_hash) is suppressed for 5 minutes.
// Lock-free reads via moka.

const SUGGESTION_TTL_SECS: u64 = 300;
const CACHE_MAX_CAPACITY: u64 = 4096;

/// Teto de projetos com τ conformal memoizado simultaneamente. Um daemon atende
/// poucos projetos por janela; o limite existe para que o cache não cresça
/// indefinidamente num processo que vive muito mais que qualquer sessão.
const CONFORMAL_TAU_MAX_PROJECTS: u64 = 64;

fn cache() -> &'static moka::sync::Cache<u64, ()> {
    static CACHE: OnceLock<moka::sync::Cache<u64, ()>> = OnceLock::new();
    CACHE.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(SUGGESTION_TTL_SECS))
            .build()
    })
}

/// P1/S-1.1 (2026-09-04) — the ledger of what was already SAID, which the input
/// cache structurally cannot be.
///
/// [`cache`] anti-spams identical `(tool_name, tool_input)` pairs: it asks *"same
/// question?"*. The window is paid in ANSWERS, and the two diverge — the same
/// text is reached from different inputs. Measured over 10 transcripts / 406 user
/// turns (04/09/2026): **24,1 %** of this emitter's blocks were still
/// byte-identical repeats despite that cache, and across all emitters repeats
/// were **35,1 % of the whole injection — 948 KB**, with `past-lessons` at
/// 86,4 %.
///
/// A second copy of the same bytes carries no proposition the first did not, so
/// its information density is zero by construction (IDR, Eixo 4). Inside the TTL
/// window — the window in which the first copy is demonstrably still in context —
/// the repeat collapses to a one-line reference.
///
/// Keyed by `(session, content)`: another session never saw the first copy, and
/// suppressing there would hide a block that context has never carried.
fn emitted_content() -> &'static moka::sync::Cache<(String, u64), std::time::Instant> {
    static EMITTED: OnceLock<moka::sync::Cache<(String, u64), std::time::Instant>> =
        OnceLock::new();
    EMITTED.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(SUGGESTION_TTL_SECS))
            .build()
    })
}

/// Human kill switch for the content dedup, in the shape the rest of this file
/// uses (`TOURING_SUGGESTER_DISABLED`, `TOURING_CODE_GATES_DISABLED`). The model
/// never sets it at run time.
fn dedup_disabled() -> bool {
    std::env::var("TOURING_DEDUP_DISABLED").is_ok_and(|v| v != "0")
}

/// `Some(reference)` when this exact text already reached this session inside the
/// TTL window; `None` the first time (and the text is recorded).
///
/// A REFERENCE, never silence. Absence has two causes — "nothing to say" and
/// "the emitter broke" — and a block that simply vanished is indistinguishable
/// from the second. The line names how long ago and how many bytes it saved, so
/// the elision is auditable in the transcript it appears in.
fn repeat_reference(session: &str, context: &str) -> Option<String> {
    if dedup_disabled() || context.is_empty() {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    context.hash(&mut hasher);
    let key = (session.to_string(), hasher.finish());
    match emitted_content().get(&key) {
        Some(first_seen) => {
            let reference = format!(
                "↑ repetido nesta sessão (há {}s, {} B elididos)",
                first_seen.elapsed().as_secs(),
                context.len()
            );
            // A dedup that costs more than it saves is not a dedup. Measured
            // block sizes vary by two orders of magnitude — `touring-suggest`
            // averages 1 399 B, but `past-lessons` averages 48 B and repeats
            // 86,4 % of the time, which is precisely the family a long reference
            // would make WORSE. The arithmetic decides, not the intention.
            (reference.len() < context.len()).then_some(reference)
        }
        None => {
            emitted_content().insert(key, std::time::Instant::now());
            None
        }
    }
}

fn input_hash(project_root: &Path, tool_name: &str, tool_input: &Value) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    project_root.hash(&mut hasher);
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
///
/// The key is scoped by `project_root`, because [`cache`] is a process-wide
/// `static` and the daemon that evaluates this hook is long-lived and serves
/// **more than one project**. Keyed by cluster alone, a banner emitted while
/// working in project A silently suppressed the same banner in project B — for
/// a reader who had never seen it. "Once per window" is a property of one
/// project's reader, not of the process.
///
/// The same scoping is what makes the e2e suite honest. Those tests exercise
/// the REAL production clusters (`anti-pattern-bash-edit`, `system-health-
/// precheck`), so they cannot dodge the collision by inventing a unique cluster
/// name the way the unit tests below do; sharing one process, whichever test
/// reached a cluster first consumed it and its neighbour asserted over an empty
/// suggestion. Each test builds its runtime in its own tempdir, so scoping by
/// root restores the isolation `make_runtime` already promised (found by a
/// flaky `cli_suggester_e2e`, 24/08/2026).
fn cluster_dedupe_key(project_root: &Path, cluster: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{1}cluster\u{1}".hash(&mut hasher);
    project_root.hash(&mut hasher);
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
fn cluster_dedupe_gate(project_root: &Path, classifier: &ClassifierOutput) -> ClusterDecision {
    if classifier.carries_input_specific_signal() {
        return ClusterDecision::Proceed;
    }
    let key = cluster_dedupe_key(project_root, &classifier.cluster);
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

/// Scan count at which the Code Mode hint fires. Tightened 3→2 (29/08, ordem de
/// Gabriel): with G1_DENY_AT now at 3, the hint must speak BEFORE the deny —
/// the 2nd atomic search is the last chance to teach without blocking.
const CODE_MODE_SCAN_THRESHOLD: u32 = 2;

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
fn scan_class_key(project_root: &Path) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}code-mode-scan\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
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
fn scan_window_crosses_threshold(project_root: &Path) -> bool {
    let key = scan_class_key(project_root);
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

    // Memoizado POR PROJETO. O τ é destilado do substrato de `rt` — os outcomes
    // daquele projeto — e o daemon que hospeda este cache é longo-vivo, servindo
    // mais de um. Numa célula única, o τ do primeiro projeto a chegar governava
    // a régua de disparo dos demais por 300 s: o gate mais consequente do hook
    // (decide se a sugestão sai) respondia a dados de outro repositório.
    // `moka` (o mesmo mecanismo dos demais caches deste módulo) dá teto e
    // expiração nativos: um mapa sem limite cresceria uma entrada por projeto
    // pela vida inteira de um daemon longo-vivo, e o TTL viraria checagem manual.
    static CACHE: OnceLock<moka::sync::Cache<std::path::PathBuf, f32>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CONFORMAL_TAU_MAX_PROJECTS)
            .time_to_live(Duration::from_secs(SUGGESTION_TTL_SECS))
            .build()
    });
    if let Some(tau) = cache.get(&rt.project_root) {
        return tau;
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

    cache.insert(rt.project_root.clone(), tau);
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
    /// Code-quality score for the file in [0.0, 1.0] — **higher is better**.
    ///
    /// Sourced from `knowledge.get_cognitive_enrichment(file_path)` (field 0 of
    /// the returned `CognitiveScores` tuple). `None` when the file has no
    /// cognitive enrichment row yet (e.g. never post-edited) or when no
    /// `file_hint` is present in the classifier output.
    ///
    /// Used by `ActionSignature::from_pre_tool_with_cognitive` to set the
    /// `HiComplexity` qualifier when the score is LOW (see `QUALITY_LOW_THRESHOLD`).
    ///
    /// The direction is the producer's, not this comment's: `analyze_quality`
    /// (`touring-code/src/ast/quality.rs`) declares *"All scores are in [0.0, 1.0]
    /// where higher is better"* and computes PENALTIES — high cyclomatic complexity
    /// subtracts. This field used to be called `cognitive_score` and this doc used to
    /// call it a "complexity score", which is how `ActionSignature` came to fire
    /// `HiComplexity` on the CLEANEST files and never on the messiest (measured
    /// 03/09/2026: README.md 1.000, cli_suggester.rs 0.352 at 5791 lines with a CC=23
    /// function). A consumer that re-documents what it consumes creates a copy; this
    /// one was born wrong. Read the producer, not this line.
    pub quality_score: Option<f32>,

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

/// The DESTRUCTIVE verbs of REGRA #11 v2, mirroring the EXECUTOR —
/// `~/.claude/hooks/block_git.sh` (`DESTRUCTIVE_RE` plus the `stash` and
/// `restore` special cases). This list exists so the text this nudge
/// DECLARES and the predicate that actually DENIES the command derive from
/// the same source (D8: enforcement lives in the executor, and the prompt
/// never promises what the executor does not apply). `scripts/
/// test_git_nudge_matches_executor.py` reads the shell regex and fails if
/// the two drift apart.
pub(crate) const GIT_DESTRUCTIVE_VERBS: &[&str] = &[
    "reset --hard",
    "reset --merge",
    "reset --keep",
    "checkout --",
    "checkout -f",
    "switch --discard-changes",
    "clean -f",
    "rebase",
    "push --force",
    "push -f",
    "branch -D",
    "filter-branch",
    "filter-repo",
    "reflog expire",
    "gc --prune",
    "stash",
    "restore",
];

/// True when `command` belongs to the DESTRUCTIVE class of REGRA #11 v2.
///
/// Mirrors the executor's carve-outs exactly: `stash list`/`stash show` are
/// read-only; `restore --staged` (without `--worktree`) only unstages; plain
/// `reset` (mixed/soft) never touches the working tree; `clean -n` is a
/// dry-run. Anything already carrying the deliberate per-command token is not
/// re-gated — the ritual happened upstream.
pub(crate) fn git_is_destructive(command: &str) -> bool {
    if command.contains("GIT_DESTRUCTIVE_OK=1") {
        return false;
    }
    // Descarte barato — e a razão de ser da lista estar AQUI, no caminho de
    // execução, e não só na documentação: se nenhum verbo declarado aparece
    // sequer como substring, não há o que testar. Uma lista que o predicado
    // não percorre é declaração que envelhece em silêncio, exatamente o que
    // este arquivo passou a guardar contra (D8).
    if !GIT_DESTRUCTIVE_VERBS.iter().any(|verb| {
        // Só o primeiro token: a lista declara "reset --hard", mas o descarte
        // pergunta apenas se a palavra `reset` aparece no comando.
        let head = verb.split_whitespace().next().unwrap_or(verb);
        command.contains(head)
    }) {
        return false;
    }
    let re = match regex::Regex::new(
        r"git\s+(reset\s+[^|;&]*--(hard|merge|keep)|checkout[^|;&]*(\s--(\s|$)|\s-f\b|\s--force\b|\s\.\s*$)|switch\s+[^|;&]*--discard-changes|clean\s+[^|;&]*(-[a-z]*f|--force)|rebase\b|push[^|;&]*(\s--force(-with-lease)?\b|\s-f\b)|branch[^|;&]*\s-D\b|filter-branch\b|filter-repo\b|reflog\s+expire|gc\s+[^|;&]*--prune)",
    ) {
        Ok(re) => re,
        Err(_) => return false,
    };
    if re.is_match(command) {
        return true;
    }
    // `git stash`: every mutating form is gated; only list/show are read-only.
    let stash_hit = regex::Regex::new(r"git\s+stash\b")
        .ok()
        .zip(regex::Regex::new(r"git\s+stash\s+(list|show)\b").ok())
        .is_some_and(|(s, ro)| s.is_match(command) && !ro.is_match(command));
    if stash_hit {
        return true;
    }
    // `git restore` discards worktree changes unless it is purely --staged.
    regex::Regex::new(r"git\s+restore\b")
        .ok()
        .zip(regex::Regex::new(r"git\s+restore\s+[^|;&]*--staged").ok())
        .zip(regex::Regex::new(r"git\s+restore\s+[^|;&]*--worktree").ok())
        .is_some_and(|((r, s), w)| {
            r.is_match(command) && (!s.is_match(command) || w.is_match(command))
        })
}

fn classify_bash(tool_input: &Value) -> Option<ClassifierOutput> {
    let command = tool_input.get("command").and_then(|v| v.as_str())?;
    if command.is_empty() {
        return None;
    }

    // Pattern 1: grep/rg for a symbol-like token.
    if matches!(resolved_verb(command), Some("grep") | Some("rg"))
        && command.contains(' ')
        && let Some(sym) = extract_first_symbol_in_text(command)
    {
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
        && let Some(file) = extract_code_file_from_command(command)
    {
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

    // Pattern 5: destructive inline edits (sed -i, awk -i inplace, perl -pi,
    // rm + heredoc) — anti-pattern, route to touring-native tooling.
    // O predicado da escrita cega é UM SÓ — o mesmo do gate G9 (âncora em
    // posição de comando): antes o regex solto casava a PROSA `sed -i`
    // dentro de outro comando (observado vivo 26/08 num `decompose update`
    // cujo texto citava o padrão) — advisory barulhento ensina a ignorar.
    if is_inline_blind_edit(command)
        || regex::Regex::new(r"rm\s+.*&&\s*(?:cat|echo|printf)\s*>")
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

    // Pattern 6: git — REGRA #11 v2 (Gabriel, 2026-08-23) PERMITS git. Only
    // the DESTRUCTIVE class is gated, and it is gated by a RITUAL, not a ban.
    // Until 2026-08-25 this arm claimed "git is prohibited … block_git.sh will
    // reject this command", which was false twice over for `git status`: the
    // ban was revoked, and the guard allows read-only/additive git. A nudge
    // that misstates the executor is the D8 anti-pattern inside the product.
    if regex::Regex::new(r"^\s*(\w+=\S+\s+)*git\s+")
        .ok()
        .map(|re| re.is_match(command))
        .unwrap_or(false)
    {
        if git_is_destructive(command) {
            return Some(ClassifierOutput {
                cluster: "regra-11-git-destructive".into(),
                must: vec![
                    cmd(
                        "git status --porcelain && git stash list",
                        "(1) MEDIR — nomear os arquivos que a operação pode descartar",
                    ),
                    cmd(
                        "git checkout -b safety/$(date +%F)-<slug> && git add -A \
                         && git commit -m \"WIP safety pre-<op>\"",
                        "(2) SNAPSHOT — commit WIP; stash como guarda segue BANIDO \
                         (destruiu 162 módulos em 06/04/2026)",
                    ),
                    cmd(
                        "GIT_DESTRUCTIVE_OK=1 <o mesmo comando>",
                        "(4) EXECUTAR — token por-comando, jamais exportado na sessão",
                    ),
                ],
                should: vec![cmd(
                    "touring memory store \"pre-<op>:<ts>\" \"<estado em risco>\" --tier semantic",
                    "snapshot semântico do que o git não versiona (DBs, hooks, untracked)",
                )],
                may: vec![],
                reason: "REGRA #11 v2 — git é PERMITIDO, mas esta operação é da classe \
                         DESTRUTIVA: exige o ritual anti-perda (medir → snapshot → \
                         confirmar) ANTES do token. O executor `block_git.sh` NEGA \
                         este comando enquanto não carregar GIT_DESTRUCTIVE_OK=1. \
                         Confirmar com Gabriel se: force-push, histórico publicado, \
                         ou mudanças que esta sessão não criou."
                    .into(),
                confidence: 0.99,
                symbol_hint: None,
                file_hint: None,
            });
        }
        // Read-only / additive git: o executor PERMITE, nada é exigido, e o
        // silêncio é a resposta correta.
        //
        // Houve aqui um arm `regra-11-git-safe` (MAY informativo, confiança
        // 0.55) sugerindo `memory recall`/`status` como complemento. Ele nunca
        // executou: `select_classifier` descarta tudo abaixo do gate conformal
        // (`LEGACY_THRESHOLD = 0.7`), então o arm era código inerte —
        // aparentemente correto, provadamente morto. Descoberto no cross-audit
        // de 26/08 pelo teste E2E (o unitário não pegava, porque exercita
        // `classify_bash` ANTES do gate).
        //
        // Inflar a confiança para furar o gate seria a correção errada: `git
        // status` é dos comandos mais frequentes que existem, e um banner nele
        // taxa o caso comum sem exigir ação nenhuma — o oposto da invariante
        // de densidade. O valor da REGRA #11 v2 está no arm DESTRUTIVO, que
        // tem 0.99 e passa folgado.
        return None;
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

/// The portfolio index, cached and invalidated by the file's mtime.
///
/// A plain `OnceLock` was wrong here: the suggester runs in-daemon, the daemon
/// is long-lived, and `touring portfolio refresh` would therefore never be seen
/// until a restart — a portfolio that silently ages is worse than none, because
/// it keeps asserting stale prior art with full confidence. An mtime check is a
/// `stat` (microseconds) and is exact, so no TTL guesswork is needed.
type CachedPortfolio = (
    std::sync::Arc<touring_foundation::portfolio::store::PortfolioIndex>,
    Option<std::time::SystemTime>,
);
static PORTFOLIO: std::sync::OnceLock<std::sync::Mutex<Option<CachedPortfolio>>> =
    std::sync::OnceLock::new();

/// Last-modified time of the portfolio index file, when it exists.
fn portfolio_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(touring_foundation::portfolio::store::index_path())
        .ok()?
        .modified()
        .ok()
}

/// The portfolio index, reloaded whenever the file on disk has changed.
fn portfolio_index() -> Option<std::sync::Arc<touring_foundation::portfolio::store::PortfolioIndex>>
{
    let cell = PORTFOLIO.get_or_init(|| std::sync::Mutex::new(None));
    let mtime = portfolio_mtime();
    let mut guard = cell.lock().ok()?;
    if let Some((index, cached_mtime)) = guard.as_ref()
        && *cached_mtime == mtime
    {
        return Some(std::sync::Arc::clone(index));
    }
    let loaded = std::sync::Arc::new(
        touring_foundation::portfolio::store::load()
            .unwrap_or_else(|_| touring_foundation::portfolio::store::PortfolioIndex::empty()),
    );
    *guard = Some((std::sync::Arc::clone(&loaded), mtime));
    Some(loaded)
}

/// True for header lines that carry legal or tooling boilerplate, not purpose.
fn is_boilerplate(line: &str) -> bool {
    const MARKERS: &[&str] = &[
        "copyright",
        "all rights reserved",
        "licensed under",
        "spdx-license",
        "-*- coding",
        "this file is part of",
        "generated by",
        "do not edit",
        "autogenerated",
        "auto-generated",
    ];
    let lower = line.to_lowercase();
    MARKERS.iter().any(|m| lower.contains(m))
}

/// Derive a natural-language intent for a file about to be created.
///
/// Real derived value, never a placeholder (the injection-density invariant):
/// the leading docstring / `//!` header of the content being written when there
/// is one, else the file stem split into words. `generate_pdf.py` yields
/// "generate pdf" — enough for the portfolio to find prior art.
fn intent_for_new_file(file: &str, content: Option<&str>) -> Option<String> {
    /// Python triple-double-quote docstring delimiter.
    const TRIPLE_D: &str = "\"\"\"";
    /// Python triple-single-quote docstring delimiter.
    const TRIPLE_S: &str = "'''";
    if let Some(body) = content {
        for line in body.lines().take(12) {
            let t = line.trim();
            // C3 (2026-09-02): a codetag (`# #tags: kind:script …`) is a FACET
            // line, not a purpose — used as the intent it sent the portfolio
            // hunting for "kind:script purpose:diagnostic" and returned an
            // unrelated `cofre.py` on every Write of a tagged script. Skip it
            // (and the shebang) so the docstring below wins.
            if t.contains("#tags:") || t.starts_with("#!") {
                continue;
            }
            let prose = t
                .strip_prefix("//!")
                .or_else(|| t.strip_prefix(TRIPLE_D))
                .or_else(|| t.strip_prefix(TRIPLE_S))
                .or_else(|| t.strip_prefix("# "));
            if let Some(prose) = prose {
                let prose = prose
                    .trim()
                    .trim_end_matches(TRIPLE_D)
                    .trim_end_matches(TRIPLE_S)
                    .trim();
                // Boilerplate is not a purpose. A licence banner would send the
                // portfolio hunting for "copyright ... all rights reserved" and
                // inject that noise on every Write.
                if prose.len() >= 20 && !is_boilerplate(prose) {
                    return Some(prose.chars().take(120).collect());
                }
            }
        }
    }
    let stem = Path::new(file).file_stem()?.to_str()?;
    let words: Vec<&str> = stem
        .split(['_', '-', '.'])
        .filter(|w| w.len() >= 3 && *w != "mod" && *w != "lib" && *w != "main")
        .collect();
    (words.len() >= 2).then(|| words.join(" "))
}

/// Prior art for a file about to be created, when the portfolio has any.
///
/// Fails open in every direction: a missing, empty or unreadable index simply
/// yields `None` and the Write suggestion renders exactly as it did before.
fn portfolio_prior_art(
    file: &str,
    content: Option<&str>,
) -> Option<(String, touring_foundation::portfolio::PortfolioAnswer)> {
    let intent = intent_for_new_file(file, content)?;
    let index = portfolio_index()?;
    if index.is_empty() {
        return None;
    }
    let answer = touring_foundation::portfolio::query::answer(&index, &intent, 3);
    if answer.prior_art.is_empty() {
        return None;
    }
    Some((intent, answer))
}

fn classify_write(tool_input: &Value) -> Option<ClassifierOutput> {
    let file = tool_input.get("file_path").and_then(|v| v.as_str())?;
    if !is_code_file(file) {
        return None;
    }
    // Prior art BEFORE creation — the whole point of the portfolio (REGRA #0).
    let prior_art = portfolio_prior_art(file, tool_input.get("content").and_then(|v| v.as_str()));
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
        should: {
            let mut v = vec![
                cmd(
                    "touring index find <SymbolName>",
                    "collision check before defining new symbols",
                ),
                cmd(
                    "touring generate verify --symbol <name>",
                    "VGP gate at template stage",
                ),
            ];
            if let Some((intent, _)) = prior_art.as_ref() {
                // Front of the list: prior art decides whether to write at all.
                v.insert(
                    0,
                    cmd(
                        format!("touring portfolio \"{intent}\""),
                        "prior-art por propósito — veredito obrigatório: reuse | extend | supersede | create_new",
                    ),
                );
            }
            v
        },
        may: vec![cmd(
            "touring wiring suggest <new_symbol>",
            "auto-wire hints after creation",
        )],
        reason: match prior_art.as_ref() {
            Some((intent, answer)) => {
                let top = answer
                    .prior_art
                    .iter()
                    .map(|h| h.entry.display_path.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let gaps = answer.gaps.join("; ");
                format!(
                    "New {kind} — o portfólio já tem {} artefato(s) para \"{intent}\": {top}. \
                     Lacunas: {gaps}. Isto é EVIDÊNCIA, não restrição: decida \
                     reuse | extend | supersede | create_new e justifique; consulte a lente \
                     externa antes de superar. O pipeline touring-native (VGP + atomic + \
                     post-validate) continua valendo para o que for escrito.",
                    answer.prior_art.len(),
                )
            }
            None => format!(
                "New {kind} — touring-native tooling runs the 12-stage create pipeline (doctor + \
                 discover + VGP + render + atomic write + post-validate + memory + RL). \
                 Plain Write bypasses all of these."
            ),
        },
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
    if let Some(ref sym) = classifier.symbol_hint
        && let Some(ref store) = rt.infra.symbol_store
    {
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
        if let Some(ref store) = rt.infra.symbol_store
            && let Ok(syms) = store.find_symbols_in_file(&rel)
        {
            data.pub_symbol_count = Some(syms.len() as u32);
        }

        // Code-QUALITY score (higher is better) — from the `cognitive_enrichment`
        // table, field 0 of `CognitiveScores` = (quality_score, complexity_signal,
        // fan_in_signal, fan_out_signal, doc_signal). Written by `post_edit` from
        // `analyze_quality`, whose doc declares the direction; do not paraphrase it
        // here — a consumer that re-documents what it consumes creates a copy, and
        // that copy is how this value came to be read backwards until 04/09/2026.
        // Fail-open: any error → None.
        if let Ok(Some(scores)) = rt.ctx.knowledge.get_cognitive_enrichment(&rel) {
            data.quality_score = Some(scores.0 as f32);
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
    // The counter that measures this path. Until 2026-08-20 it was incremented
    // only from `touring-ceg/gateway/metrics.rs` and the manual `touring gate`
    // verb — never from here, the hook that fires on every session. Result:
    // `workflow_advice_emitted_count` read 0 while advice was being injected
    // ~10x in a 25-minute window, and `workflow_antipattern_detected_count`
    // read 0 next to a non-zero `adoption_antipattern_count`: two counters for
    // the same phenomenon, one fed from a cold path and one from none.
    crate::shared::gate_metrics::record_workflow_advice_emitted();

    // Antipattern conversion hint (Bash only) — advisory Warn, never Deny.
    if let Some(ap) = detect_antipattern(&sig, &state) {
        // Counted on DETECTION, which is what the counter's name says —
        // surfacing is a separate decision (`should_surface`) and would
        // undercount the detector by however often it stays quiet.
        crate::shared::gate_metrics::record_workflow_antipattern_detected();
        let cv = conversion_for(ap.kind);
        if cv.should_surface() {
            we.antipattern_hint = Some(cv.as_hint());
        }
    }

    // Glob validation hint — surface when the classifier is file-enumeration
    // and the pattern was extracted from the cluster context.
    if classifier.cluster == "file-enumeration"
        && let Some(ref pattern) = glob_pattern_from_classifier(classifier)
    {
        let result = validate_glob_pattern(pattern, None);
        if let Some(hint) = result.hint() {
            we.glob_hint = Some(hint.to_owned());
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
    if let Some((refreshed_at, mem, know)) = guard.as_ref()
        && federated_cache_is_fresh(*refreshed_at, FEDERATED_DB_TTL, std::time::Instant::now())
    {
        return (std::sync::Arc::clone(mem), std::sync::Arc::clone(know));
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
        let Ok(conn) = open_lessons_db_readonly(db) else {
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
/// Abre um DB de lições para LEITURA APENAS.
///
/// Os três consumidores deste módulo só fazem `SELECT`, e abrir em modo de
/// escrita custou dois defeitos, ambos observados:
///
/// 1. **Deadlock sob concorrência** (capturado sob gdb em 24/08/2026, 3 travas
///    em 40 execuções): o `Drop` de uma conexão de escrita sobre um DB em WAL
///    entra em `sqlite3WalClose` → `unixLock`, pedindo lock EXCLUSIVO de arquivo
///    para o checkpoint. Threads que apenas liam o mesmo arquivo disputavam esse
///    lock com quem fechava, e o processo parava com todas as threads em
///    `pthread_mutex_lock`. Uma conexão read-only não faz checkpoint no close,
///    então não pede o lock exclusivo. Isto NÃO é um problema só de teste: o
///    `cli-suggest` roda in-daemon, e o daemon é multi-thread.
/// 2. **Criação silenciosa de DB alheio**: `Connection::open` traz
///    `SQLITE_OPEN_CREATE`, então consultar um caminho federado inexistente
///    fabricava um banco vazio no lugar. Sem `CREATE`, o caminho ausente
///    simplesmente falha e o chamador segue para o próximo.
fn open_lessons_db_readonly(db: &std::path::Path) -> Result<rusqlite::Connection, rusqlite::Error> {
    rusqlite::Connection::open_with_flags(
        db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
            | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
}

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
        let Ok(conn) = open_lessons_db_readonly(db) else {
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
    let conn = match open_lessons_db_readonly(mem_db_path) {
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
    // P3/S-3.2 (2026-09-04) — sob a escala canônica, não uma constante própria.
    // Este é o ÚNICO orçamento de INJEÇÃO deste arquivo: `BUDGET` (rajada
    // python-inline) e `G1_BODY_BUDGET` dimensionam o PROGRAMA que o remédio
    // entrega, e encolhê-los cortaria a correção em vez do ruído — o plano os
    // listava junto e a leitura desfez o engano. 800 é o mesmo valor de antes;
    // o que muda é a fonte, agora com os overrides `TOURING_CILA_BUDGET_*`.
    let lesson_budget = touring_foundation::cila::cila_budget_read(0);

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

    let ranked = rank_and_trim(all, lesson_budget);
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
/// `VAR=valor` de prefixo (inclusive o bypass `TOURING_GATE_OK=1`): a classe é
/// do comando executado, não do ambiente que o precede. Predicado idêntico ao
/// que `scan_class_of` sempre usou — extraído para os 5 sítios dividirem.
fn is_env_assignment(t: &str) -> bool {
    t.contains('=') && t.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

/// Wrapper de execução reconhecido pelo resolvedor: flags que tomam valor +
/// quantos posicionais o PRÓPRIO wrapper consome (a duração do `timeout`, a
/// máscara do `taskset`, a prioridade do `chrt`).
struct WrapperSpec {
    value_flags: &'static [&'static str],
    positionals: usize,
}

/// S1 (26/08/2026) — os classificadores liam o 1º token: `time grep`,
/// `nice -n 5 find`, `sudo cat`, `env F=1 rg` escapavam de TODOS os gates e
/// nudges ao mesmo tempo (a cobertura nascia zero onde a classe nasce).
fn wrapper_spec(verb: &str) -> Option<WrapperSpec> {
    Some(match verb {
        "env" => WrapperSpec {
            value_flags: &["-u", "--unset", "-C", "--chdir", "-S", "--split-string"],
            positionals: 0,
        },
        "time" => WrapperSpec {
            value_flags: &["-o", "--output", "-f", "--format"],
            positionals: 0,
        },
        "nice" => WrapperSpec { value_flags: &["-n", "--adjustment"], positionals: 0 },
        "ionice" => WrapperSpec { value_flags: &["-c", "-n", "-p", "-P"], positionals: 0 },
        "sudo" => WrapperSpec {
            value_flags: &[
                "-u", "-g", "-h", "-p", "-C", "-r", "-t", "-T", "-U", "-D", "-R",
                "--user", "--group", "--host", "--prompt", "--role", "--type",
                "--chdir", "--chroot",
            ],
            positionals: 0,
        },
        "command" | "builtin" => WrapperSpec { value_flags: &[], positionals: 0 },
        "timeout" => WrapperSpec {
            value_flags: &["-s", "--signal", "-k", "--kill-after"],
            positionals: 1,
        },
        "stdbuf" => WrapperSpec { value_flags: &["-i", "-o", "-e"], positionals: 0 },
        "taskset" => WrapperSpec { value_flags: &["-c", "--cpu-list"], positionals: 1 },
        "chrt" => WrapperSpec { value_flags: &[], positionals: 1 },
        _ => return None,
    })
}

/// Consome o wrapper em `toks[i]` (verbo + flags + posicionais dele) e devolve
/// o índice seguinte; `i` inalterado quando `toks[i]` não é wrapper. Flag
/// idêntica à da tabela consome o valor no token seguinte (`-n 5`); a forma
/// colada (`-n5`) e a longa com `=` (`--signal=KILL`) consomem um só token.
fn skip_wrapper(toks: &[&str], i: usize) -> usize {
    let Some(spec) = toks.get(i).and_then(|v| wrapper_spec(v)) else {
        return i;
    };
    let mut j = i + 1;
    let mut positionals = spec.positionals;
    while j < toks.len() {
        let t = toks[j];
        if is_env_assignment(t) {
            j += 1; // `sudo FOO=1 cmd` / as atribuições do próprio `env`
            continue;
        }
        if t.starts_with('-') && t != "-" {
            j += if spec.value_flags.contains(&t) { 2 } else { 1 };
            continue;
        }
        if positionals > 0 {
            positionals -= 1;
            j += 1;
            continue;
        }
        break; // o verbo real
    }
    j
}

/// Tokens a partir do VERBO REAL: prefixos `VAR=valor` e wrappers de execução
/// (`env`/`time`/`nice`/`sudo`/`command`/`builtin`/`timeout`/`stdbuf`/
/// `ionice`/`taskset`/`chrt`, encadeados em qualquer ordem) removidos. Vazio
/// quando o comando é só invólucro (`env` puro imprime o ambiente). Um único
/// resolvedor para todos os classificadores — consertar um sítio só mascarou
/// o defeito uma vez (memória `definer-module-cinco-sitios`); não duas.
fn resolved_tokens<'a, 'b>(toks: &'b [&'a str]) -> &'b [&'a str] {
    let mut i = 0;
    while i < toks.len() {
        if is_env_assignment(toks[i]) {
            i += 1;
            continue;
        }
        let j = skip_wrapper(toks, i);
        if j == i {
            break;
        }
        i = j;
    }
    &toks[i..]
}

/// Segmentos de comando separados por operadores de sequência (`&&`, `||`,
/// `;`, nova linha). Ingênuo quanto a aspas (`grep 'a;b'` corta dentro da
/// aspa) — seguro para CLASSIFICAÇÃO: o verbo do segmento não muda com o
/// corte, que é o único uso (S1, 26/08).
fn command_segments(cmd: &str) -> Vec<&str> {
    cmd.split(['\n', ';'])
        .flat_map(|part| part.split("&&"))
        .flat_map(|part| part.split("||"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Tokens efetivos: segmenta por operadores de sequência, pula segmentos
/// `cd <dir>` (prefixo de navegação — `cd /x && grep foo` é `grep`), e
/// resolve wrappers no primeiro segmento que não é só navegação. Tudo `cd`
/// (ou nada) devolve vazio. É o resolved_tokens + a lacuna que a estratégia
/// S1 nomeou (`cd <dir> (&&|;\n)`) e a primeira entrega deixou passar.
fn effective_tokens(cmd: &str) -> Vec<&str> {
    for seg in command_segments(cmd) {
        let toks: Vec<&str> = seg.split_whitespace().collect();
        let rest = resolved_tokens(&toks);
        match rest.first() {
            Some(&"cd") => continue, // prefixo de navegação — o próximo decide
            // Segmento que resolve para NADA (`P=x` sozinho, `env` puro): o
            // próximo segmento decide. Sem este braço, `P=x\npython3 f.py`
            // devolvia vazio e TODA classificação morria no assignment —
            // provado ao vivo (29/08): 12 execuções python atrás de
            // `VAR=...\n` e zero denies do G10 no turno de 60 do `analise`.
            None => continue,
            _ => return rest.to_vec(),
        }
    }
    Vec::new()
}

/// O verbo real de `command` — primeiro token depois de segmentos, prefixos
/// `cd` e wrappers.
fn resolved_verb(command: &str) -> Option<&str> {
    effective_tokens(command).first().copied()
}

fn is_scan_command(command: &str) -> bool {
    let rest = effective_tokens(command);
    let Some(&verb) = rest.first() else { return false };
    match verb {
        "grep" | "rg" | "egrep" | "fgrep" | "ag" => rest.len() > 1,
        "find" => rest[1..].iter().any(|t| t.contains("-name")),
        _ => false,
    }
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
            // O comando JÁ É code mode — não há o que induzir. Sem este guard o
            // nudge recebia `touring run --lang python --code '…'` e emitia como
            // MUST `touring run --lang bash --code 'touring run --lang python …'`,
            // ensinando o antipadrão que ele existe para evitar (observado 6× na
            // sessão de 25/08/2026). O mesmo guard já protegia
            // `loop_rewrite_candidate`; faltava na porta dos nudges.
            if command.contains("touring run") || command.contains("touring exec") {
                return None;
            }
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

// `loop_glob` + `loop_code_mode_command` REMOVIDAS em 25/08/2026 (P1).
//
// Extraíam o glob real de um `for … in GLOB` e montavam um programa python cujo
// corpo era `# then your per-file op over files`. A premissa era que o corpo do
// laço "não é derivável sem um parser bash" — verdadeira para TRADUZIR o laço,
// falsa para EXECUTÁ-LO: `bash_code_mode_command` leva o laço verbatim para o
// sandbox e roda, sem traduzir nada. Um remédio com placeholder é meio remédio,
// e a métrica que importa é se o snippet substitui as N chamadas.
// REGRA #0: removidas de fato, não silenciadas por atributo de supressão.

/// E4 (2026-08-24) — a shell loop whose body invokes `touring adw run` is a
/// CAMPAIGN written by hand: no predicate, no curve, no fail-closed signal,
/// invisible to the journal. The runner owns flow iteration
/// (`adw.py::cmd_campaign`); the nudge carries the real flow name when it is
/// derivable. Measured origin: this session's own `for i in 2 3 4; do touring
/// adw run error-teach …` — the anti-pattern the layer exists to replace.
fn campaign_code_mode_command(command: &str) -> Option<String> {
    if !command.contains("touring adw run") {
        return None;
    }
    let flow = command
        .split("touring adw run")
        .nth(1)?
        .split_whitespace()
        .next()
        .filter(|w| !w.starts_with('-'))
        .unwrap_or("<flow>")
        .to_string();
    Some(format!(
        "touring adw campaign {flow} --until '<CODE predicate; exit 0 = converged; \
         may print METRIC=<float>>' --max-rounds 8"
    ))
}


/// Render the real shell command verbatim as a `touring run --lang bash` sandbox call.
/// The density-correct fallback for an arbitrary loop whose glob is not mechanically
/// derivable (e.g. a command-substitution iterable like `$(pgrep …)`): the ACTUAL
/// command travels — no guessed python translation, no `<placeholder>` (the loop body
/// IS derivable, just as bash). Embedded single quotes use the `'\''` shell idiom;
/// over-long commands are capped so the nudge stays dense (high signal-to-token).
fn bash_code_mode_command(command: &str) -> String {
    // INTEIRO. A versão anterior cortava em 200 chars e anexava `…`, entregando
    // um comando que não roda — o mesmo defeito de `fuse_burst_program`
    // (25/08/2026), aqui na forma singular. O comando já está no contexto por
    // definição (foi ele que chegou ao hook), então truncá-lo não economiza
    // nada e destrói o remédio. A emenda do Gabriel é literal: o snippet tem
    // de SUBSTITUIR as chamadas, e um snippet cortado não substitui nada.
    let escaped = command.replace('\'', r"'\''");
    format!("touring run --lang bash --code '{escaped}'")
}

/// The most specific `touring run` command for a fired Code Mode kind: a scan
/// carries (pattern, glob); a loop carries its glob. `None` ⇒ the caller emits the
/// generic template (the honest fallback when nothing is derivable).
fn specialized_command(kind: &CodeModeKind, tool_name: &str, tool_input: &Value) -> Option<String> {
    match kind {
        CodeModeKind::Scan => code_mode_command(tool_name, tool_input),
        CodeModeKind::Loop => {
            let command = tool_input.get("command").and_then(Value::as_str)?;
            // E4 precedence: a loop over `touring adw run` is a hand-written
            // campaign — the runner owns flow iteration, not the shell.
            //
            // Fora esse caso, a especialização em python foi REMOVIDA (P1,
            // 25/08/2026). Ela carregava o glob real mas deixava o corpo como
            // `# then your per-file op over files` — e um snippet que conta
            // arquivos e depois manda o leitor escrever a operação não
            // substitui as N chamadas, que é o que a emenda do Gabriel exige.
            // Devolvendo `None`, o chamador cai em `bash_code_mode_command`,
            // que leva o laço VERBATIM para o sandbox: zero tradução, zero
            // placeholder, e roda. É a mesma derivação que o G8 já usava.
            campaign_code_mode_command(command)
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
    let rest = effective_tokens(command);
    let mut toks = rest.iter().copied();
    let _verb = toks.next()?; // grep / rg / egrep / … (o verbo REAL, pós-cd/wrappers)
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
/// N3a (26/08/2026) — escrita cega inline: `sed -i`/`sed --in-place`/
/// `awk -i inplace`/`perl -pi` EM POSIÇÃO DE COMANDO (início, `|`, `;`,
/// `&&`/`&`, `$(`, nova linha). A âncora em posição de comando separa a
/// invocação do texto — `echo "rode sed -i aqui"` é prosa, não escrita.
/// Lacuna conhecida (documentada, fora do escopo N3a): `find -exec sed -i`
/// e `xargs sed -i` não estão em posição de comando.
fn is_inline_blind_edit(cmd: &str) -> bool {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"(?:^|[|;&\n]|\$\()\s*(?:sed\s+-[a-zA-Z]*i(?:\.\S*)?(?:\s|$)|sed\s+--in-place\b|awk\s+-i\s+inplace\b|perl\s+-pi\b)",
        )
        .expect("G9 regex compila")
    })
    .is_match(cmd)
}

/// S4 (26/08) — classe do executor homogêneo (R4): `python3`/
/// `.venv/bin/python3`/`pytest` & cia — a rajada desenrolada que nenhum gate
/// nomeava (75 chamadas seriadas na sessão dc87e4e0, 5,8% adoção).
/// `python3 -c` fica de fora (o advisory CEG é o dono do inline). Mutação
/// marcada fica de fora por construção (P2.3): redirect de shell,
/// instaladores, git, rm — o marcador é a calibração, não prova de pureza
/// (a limitação é documentada, como no G8).
/// A visão do comando que o filtro de mutação inspeciona: corpos de heredoc
/// removidos (são DADO alimentando stdin, não comando) e redirects inofensivos
/// de FD/descarte neutralizados (`2>&1`, `2>/dev/null`, `>/dev/null`, …) — eles
/// não gravam arquivo nenhum. O `>` que sobrar é redirect real (escrita), e o
/// comando continua fora da rajada exec, como os testes exigem. Furo provado
/// ao vivo (29/08): as execuções python do turno de 60 do `analise` carregavam
/// `2>&1`/`2>/dev/null` e o `contains(">")` sobre o blob inteiro anulava a
/// classe — o G10 ficou mudo o turno todo ("verificador usa menos que o
/// extrator", mais uma encarnação).
fn mutation_scan_view(cmd: &str) -> String {
    // 1) corta corpos de heredoc: da linha após `<<MARKER` até a linha MARKER.
    let mut kept: Vec<&str> = Vec::new();
    let mut skip_until: Option<String> = None;
    for line in cmd.lines() {
        if let Some(marker) = &skip_until {
            if line.trim() == marker.as_str() {
                skip_until = None;
            }
            continue;
        }
        kept.push(line);
        if let Some(pos) = line.find("<<")
            && !line[pos..].starts_with("<<<")
        {
            let raw = line[pos + 2..].trim_start_matches('-').trim();
            let marker = raw
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(|c| c == '\'' || c == '"');
            if !marker.is_empty() {
                skip_until = Some(marker.to_string());
            }
        }
    }
    let mut s = kept.join("\n");
    // 2) neutraliza redirects que não gravam nada (ordem: os prefixados por FD
    // antes do genérico, senão o replace parcial deixa o dígito para trás).
    for inofensivo in [
        "2>&1", "1>&2", "2>/dev/null", "2> /dev/null", "&>/dev/null",
        "&> /dev/null", ">/dev/null", "> /dev/null",
    ] {
        s = s.replace(inofensivo, " ");
    }
    s
}

fn exec_class_of(cmd: &str) -> Option<&'static str> {
    const MUTATING: &[&str] = &[
        ">", "| tee", "pip install", "setup.py install", "rm ", "mv ", "cp ",
        "git ", "kill", "chmod", "chown", "curl", "wget", "ssh", "docker",
        "systemctl", "touch ", "mkdir",
    ];
    let scan = mutation_scan_view(cmd);
    if MUTATING.iter().any(|m| scan.contains(m)) {
        return None;
    }
    let rest = effective_tokens(cmd);
    let verb = rest.first()?;
    let base = verb.rsplit('/').next()?;
    if base == "python" || base == "python3" || base.starts_with("python3.") {
        // Inline (`-c` / `- <<EOF`) entrou na rajada em 29/08 (aperto de
        // Gabriel) como classe PRÓPRIA: a exclusão histórica existia porque o
        // R9 esmagaria heredoc multi-linha no remédio — o python-inline tem
        // remédio 1:1 (o corpo verbatim em `touring run --lang python`), então
        // a razão caiu. Os 5 heredocs do turno de 60 do analise estavam fora.
        if matches!(rest.get(1), Some(&"-c") | Some(&"-")) {
            return Some("python-inline");
        }
        return Some("python");
    }
    if base.starts_with("pytest") {
        return Some("pytest");
    }
    None
}

/// S4 — janela da rajada de execução: 600s. O passo de uma suíte de testes
/// (minutos por chamada) é mais lento que o de uma rajada de inspeção (180s
/// do G1) — a janela curta nunca veria o 10º passo.
const EXEC_BURST_WINDOW_SECS: u64 = 600;
/// S4 — disparo da rajada de execução homogênea. Nasceu em 10 (R4: sessões
/// reais iam a 30-75); **aperto 29/08 (ordem de Gabriel): 10→5** — 5 já pagou
/// 4 round-trips, e o uso pontual legítimo (2-3 pytest de debug) segue fora.
const EXEC_BURST_DENY_AT: u32 = 5;

/// S5 (29/08) — janela do par write→run: a mesma da rajada de execução.
const WRITE_RUN_WINDOW_SECS: u64 = 600;
/// S5 — disparo do par write→run: escrever um script e executá-lo em seguida
/// é o loop execute-observe por definição (28 pares no turno de 60 do
/// `analise`, todos invisíveis porque cada metade é individualmente legítima).
/// **Aperto 29/08 (ordem de Gabriel): 3→2** — o 1º par sempre passa (criar e
/// testar UM script é legítimo); o 2º na mesma janela JÁ é o loop. O limiar
/// baixo não taxa o caso comum: pytest/gates/tools nunca casam — o path que
/// eles executam não foi escrito via `cat >`/`tee` na janela.
const WRITE_RUN_DENY_AT: u32 = 2;

/// Paths de script escritos via `cat >`/`tee` na janela, por (projeto,
/// sessão, path).
fn written_scripts_ledger() -> &'static moka::sync::Cache<u64, ()> {
    static C: OnceLock<moka::sync::Cache<u64, ()>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(WRITE_RUN_WINDOW_SECS))
            .build()
    })
}

/// Contagem de pares write→run por projeto (+ os paths, para o remédio).
fn write_run_pair_ledger() -> &'static moka::sync::Cache<u64, (u32, Vec<String>)> {
    static C: OnceLock<moka::sync::Cache<u64, (u32, Vec<String>)>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(WRITE_RUN_WINDOW_SECS))
            .build()
    })
}

/// A sessão entra na chave (30/08/2026): os ledgers vivem no daemon, um
/// processo só para N sessões CC — chaveado por (projeto, tag), um deny nesta
/// sessão carregava comandos de OUTRA (medido pela peer `analise-a2`: paths de
/// scratchpad alheios no remédio) e inflava a contagem de quem não fez a
/// rajada. O custo: a rajada distribuída entre 2 sessões deixa de somar — o
/// desenho certo, cada sessão responde pelo próprio loop.
fn write_run_key(project_root: &Path, session: &str, tag: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    project_root.hash(&mut h);
    session.hash(&mut h);
    tag.hash(&mut h);
    h.finish()
}

fn is_script_path(p: &str) -> bool {
    let p = p.trim_matches(|c| c == '\'' || c == '"');
    p.ends_with(".py") || p.ends_with(".sh")
}

fn clean_script_path(p: &str) -> String {
    p.trim_matches(|c| c == '\'' || c == '"').to_string()
}

/// O path de script (.py/.sh) que este comando ESCREVE via `cat >`/`tee`.
/// Varre a VIEW (corpos de heredoc removidos): um `cat > y.py` dentro do
/// corpo é dado, não escrita.
fn script_write_target(cmd: &str) -> Option<String> {
    let view = mutation_scan_view(cmd);
    for seg in command_segments(&view) {
        let toks: Vec<&str> = seg.split_whitespace().collect();
        let rest = resolved_tokens(&toks);
        match rest.first() {
            Some(&"cat") => {
                let mut it = rest[1..].iter().copied();
                while let Some(t) = it.next() {
                    if t == ">" || t == ">>" {
                        if let Some(p) = it.next()
                            && is_script_path(p)
                        {
                            return Some(clean_script_path(p));
                        }
                    } else if let Some(p) =
                        t.strip_prefix(">>").or_else(|| t.strip_prefix('>'))
                        && !p.is_empty()
                        && is_script_path(p)
                    {
                        return Some(clean_script_path(p));
                    }
                }
            }
            Some(&"tee") => {
                if let Some(p) = rest[1..].iter().find(|t| !t.starts_with('-'))
                    && is_script_path(p)
                {
                    return Some(clean_script_path(p));
                }
            }
            _ => {}
        }
    }
    None
}

/// Os paths de script que este comando EXECUTA diretamente (python/bash/sh),
/// em QUALQUER segmento — o padrão real do `analise` (24 dos 60) escrevia E
/// executava no MESMO tool_use multi-linha (`cat > x.py <<EOF…EOF` +
/// `python3 x.py`), e um detector que só olha o primeiro verbo efetivo vê o
/// `cat` e nunca o run. Varre a VIEW (heredoc fora); segmento com redirect
/// real de saída é pulado — o remédio `--file` não reproduziria a escrita.
fn script_run_targets(cmd: &str) -> Vec<String> {
    let view = mutation_scan_view(cmd);
    let mut out: Vec<String> = Vec::new();
    for seg in command_segments(&view) {
        if seg.contains('>') {
            continue;
        }
        let toks: Vec<&str> = seg.split_whitespace().collect();
        let rest = resolved_tokens(&toks);
        let Some(verb) = rest.first() else { continue };
        let base = verb.rsplit('/').next().unwrap_or(verb);
        let interpretador = base == "bash"
            || base == "sh"
            || base == "python"
            || base == "python3"
            || base.starts_with("python3.");
        if !interpretador {
            continue;
        }
        if let Some(arg) = rest[1..].iter().find(|t| !t.starts_with('-'))
            && is_script_path(arg)
            && !out.iter().any(|p| p == &clean_script_path(arg))
        {
            out.push(clean_script_path(arg));
        }
    }
    out
}

/// O corpo do programa inline (`python3 -c '<code>'` ou `python3 - <<'M'`),
/// para o remédio 1:1 da classe `python-inline`.
fn python_inline_body(cmd: &str) -> Option<String> {
    // heredoc: corpo entre a linha pós-`<<MARKER` e a linha MARKER
    if let Some(pos) = cmd.find("<<")
        && !cmd[pos..].starts_with("<<<")
    {
        let first_line_end = pos + cmd[pos..].find('\n')?;
        let marker = cmd[pos + 2..first_line_end]
            .trim_start_matches('-')
            .split_whitespace()
            .next()?
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string();
        let body: Vec<&str> = cmd[first_line_end + 1..]
            .lines()
            .take_while(|l| l.trim() != marker)
            .collect();
        return Some(body.join("\n"));
    }
    // -c: o literal após a flag (heurística: o par de aspas externo)
    let idx = cmd.find(" -c ")?;
    let rest = cmd[idx + 4..].trim();
    let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let inner = &rest[1..];
    let end = inner.rfind(quote)?;
    Some(inner[..end].to_string())
}

/// Remédio 1:1 do inline: o MESMO corpo em `touring run --lang python --code`.
/// Sem fusão — a razão histórica de excluir o heredoc da rajada era o R9
/// esmagar multi-linha; com remédio próprio ela caiu (aperto 29/08).
fn python_inline_remedy(cmd: &str) -> String {
    match python_inline_body(cmd) {
        Some(b) if !b.is_empty() && b.len() <= 1500 => format!(
            "touring run --lang python --code '{}'",
            b.replace('\'', "'\\''")
        ),
        _ => "touring run --lang python --code '<o corpo do seu heredoc/-c, verbatim>'"
            .to_string(),
    }
}

/// A classe do python-inline READ-ONLY dentro da rajada de inspeção (aperto
/// 29/08, ordem de Gabriel: *"read-only cai na rajada de inspeção"*). Distinta
/// no ledger — a chave é por classe — e fora de `CODE_MODE_COLLAPSED_CLASSES`,
/// que documenta as classes que `scan_class_of` emite; esta vem do CORPO do
/// programa, não do nome do comando.
const PY_INLINE_INSPECT_CLASS: &str = "python-inline";

/// Indícios de que um corpo python NÃO é leitura pura — deny-list
/// conservadora: escrita em FS, modos mutantes de `open`, subprocesso, rede,
/// import/exec dinâmico e DB (abrir SQLite muda estado de lock mesmo lendo).
/// Falso "escritor" (a marca aparece numa string inocente) só devolve o
/// comando ao G10, mais frouxo — a direção segura; o inverso rotearia um
/// escritor para o deny de inspeção.
const PY_INLINE_WRITER_MARKS: &[&str] = &[
    ".write(",
    ".writelines(",
    "write_text(",
    "write_bytes(",
    ".unlink(",
    ".touch(",
    ".chmod(",
    ".rename(",
    ".rmdir(",
    "os.remove",
    "os.rename",
    "os.replace",
    "os.rmdir",
    "os.removedirs",
    "os.mkdir",
    "os.makedirs",
    "os.symlink",
    "os.link",
    "os.truncate",
    "os.putenv",
    "shutil.",
    "tempfile.",
    "mode='w",
    "mode=\"w",
    "mode='a",
    "mode=\"a",
    "mode='x",
    "mode=\"x",
    "subprocess",
    "os.system",
    "os.popen",
    "os.exec",
    "os.spawn",
    "pty.",
    "socket",
    "urllib",
    "requests",
    "http.client",
    "httpx",
    "ftplib",
    "smtplib",
    "exec(",
    "eval(",
    "__import__",
    "importlib",
    "sqlite3",
    "dbm.",
    "shelve",
];

/// `Some(PY_INLINE_INSPECT_CLASS)` quando o comando é um python-inline cujo
/// corpo é leitura pura — e portanto INSPEÇÃO, sujeita à rajada 2ª/300s. Um
/// corpo inextraível ou com qualquer marca de escrita fica de fora (G10 é o
/// backstop, 5ª/600s).
fn python_inline_readonly_class(cmd: &str) -> Option<&'static str> {
    if exec_class_of(cmd) != Some("python-inline") {
        return None;
    }
    let body = python_inline_body(cmd)?;
    if body.is_empty()
        || PY_INLINE_WRITER_MARKS.iter().any(|m| body.contains(m))
        || py_open_mutating_mode(&body)
    {
        return None;
    }
    Some(PY_INLINE_INSPECT_CLASS)
}

/// `open(...)` com 2º argumento LITERAL contendo `w`/`a`/`x`/`+` — o modo
/// mutante. Um `open(f)`/`open(f, encoding=…)` é leitura e passa; a marca por
/// aspas soltas (`'w'`) foi tentada e mordeu o próprio teste (`re.findall(
/// "x", …)` classificava escritor). Falso positivo residual (uma vírgula de
/// outra chamada seguida de literal com w/a/x) só devolve o comando ao G10.
fn py_open_mutating_mode(body: &str) -> bool {
    let mut rest = body;
    while let Some(i) = rest.find("open(") {
        let after = &rest[i + 5..];
        if let Some(c) = after.find(',') {
            let tail = after[c + 1..].trim_start();
            if let Some(q) = tail.chars().next().filter(|ch| *ch == '\'' || *ch == '"') {
                let inner = &tail[1..];
                if let Some(end) = inner.find(q)
                    && inner[..end].contains(['w', 'a', 'x', '+'])
                {
                    return true;
                }
            }
        }
        rest = after;
    }
    false
}

/// A rajada de python-inline fundida: os CORPOS na ordem, um programa. O
/// contrato de orçamento é o do R9 — inteiro ou fora, jamais truncado; o
/// segundo elemento conta os omitidos.
fn python_inline_burst_program(cmds: &[String]) -> (String, usize) {
    const BUDGET: usize = 4000;
    let mut out = String::new();
    let mut omitidos = 0usize;
    for cmd in cmds {
        let Some(body) = python_inline_body(cmd).filter(|b| !b.is_empty()) else {
            omitidos += 1;
            continue;
        };
        let piece = body.replace('\'', "'\\''");
        if out.len() + piece.len() + 40 > BUDGET {
            omitidos += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n# --- proximo comando da rajada ---\n");
        }
        out.push_str(&piece);
    }
    (out, omitidos)
}

/// S5 — o gate do par write→run. `None` = sem decisão (o fluxo segue).
fn write_run_pair_gate(project_root: &Path, session: &str, cmd: &str) -> Option<String> {
    use crate::shared::gate_metrics::{GateEvent, GateId, record_gate_event};
    // 1 par por comando, mesmo com múltiplos runs no tool_use: o que se conta
    // é o PASSO do loop execute-observe, não o número de interpretações.
    let path = script_run_targets(cmd).into_iter().find(|p| {
        written_scripts_ledger()
            .get(&write_run_key(project_root, session, p))
            .is_some()
    })?;
    let pkey = write_run_key(project_root, session, "\u{0}write-run-pares");
    let (n, mut paths) = write_run_pair_ledger().get(&pkey).unwrap_or_default();
    let n = n + 1;
    if paths.len() < 8 {
        paths.push(path.clone());
    }
    if n < WRITE_RUN_DENY_AT || code_gates_disabled() {
        write_run_pair_ledger().insert(pkey, (n, paths));
        return None;
    }
    write_run_pair_ledger().invalidate(&pkey);
    record_gate_event(GateId::G10, GateEvent::Denied);
    crate::shared::gate_metrics::record_g10_write_run_pair_denied();
    pending_g10().insert(session.to_string(), ());
    // `--lang` é obrigatório no CLI: sem ele a rota emitida NÃO EXECUTA
    // (medido pela peer analise-a2, 30/08 — clap rejeita e o deny vira erro
    // opaco, o antipadrão E4 cometido pelo próprio enforcement). is_script_path
    // só aceita .py/.sh, então o else é python.
    let lang = if path.ends_with(".sh") { " --lang bash" } else { " --lang python" };
    Some(deny_response(format!(
        "[G10 write→run] {n}º script escrito-e-executado na janela de \
         {WRITE_RUN_WINDOW_SECS}s (`{path}`) — o loop execute-observe manual \
         paga 2 round-trips por passo; o script JÁ está em disco, rode o MESMO \
         arquivo no sandbox sem reescrever nada:\n  \
         touring run{lang} --file {path} --timeout-ms 60000\n\
         O sandbox devolve só o que o script imprime (digest no contexto, \
         íntegra no spill), e um `touring run` zera a janela. Bypass \
         por-comando: prefixe {GATE_BYPASS_TOKEN} (contado como bypassed)."
    )))
}

type ExecBurstEntry = (u32, Vec<String>);

fn exec_burst_ledger() -> &'static moka::sync::Cache<u64, ExecBurstEntry> {
    static C: OnceLock<moka::sync::Cache<u64, ExecBurstEntry>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(EXEC_BURST_WINDOW_SECS))
            .build()
    })
}

/// S3 — janela da rajada de INSPEÇÃO, derivada de medição e não de intuição.
///
/// Medido em 115 transcripts desde 01/08/2026 (`scripts/s3_burst_distribution.py`),
/// sobre as 1.118 chamadas das classes que o modo `code` negava por lista fixa:
///
/// | janela | isoladas | volume em rajada ≥2 |
/// |---|---|---|
/// | 60s  | 41,6% | 58,4% |
/// | 120s | 32,1% | 67,9% |
/// | **300s** | **22,5%** | **77,5%** |
/// | 600s | 17,8% | 82,2% |
///
/// 300s é o joelho: de 300 para 600 a janela DOBRA para comprar 4,7 pontos de
/// volume, e cada segundo a mais é uma chance de agrupar duas inspeções que não
/// têm nada a ver uma com a outra — um falso positivo de rajada é exatamente a
/// fricção no caso comum que o S3 existe para remover.
const INSPECT_BURST_WINDOW_SECS: u64 = 300;

/// S3 — a 2ª nega; a 1ª executa intacta.
///
/// A medição é inequívoca no limiar: com janela de 300s, 77,5% do volume das
/// classes de inspeção está em rajadas de tamanho ≥ 2. Deixar a isolada passar
/// renuncia a 22,5% dos denies e preserva os outros 77,5% — o oposto do que a
/// lista fixa fazia, que era cobrar 100% da fricção para capturar o mesmo 77,5%.
/// É a regra que o próprio DeepSeek documentou e que o T3-B já declarava sem
/// nunca conseguir aplicar: *"forcing every edit through a program taxes the
/// common case"*.
const INSPECT_BURST_DENY_AT: u32 = 2;

/// Ledger da rajada de inspeção — mesmo formato do G10, janela própria.
fn inspect_burst_ledger() -> &'static moka::sync::Cache<u64, ExecBurstEntry> {
    static C: OnceLock<moka::sync::Cache<u64, ExecBurstEntry>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(INSPECT_BURST_WINDOW_SECS))
            .build()
    })
}

/// Sessão na chave pela mesma razão de [`write_run_key`]: o ledger é do
/// daemon, e sem ela a janela vazava entre sessões CC do mesmo projeto.
fn inspect_burst_key(project_root: &Path, session: &str, class: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}s3-inspect\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    session.hash(&mut hasher);
    class.hash(&mut hasher);
    hasher.finish()
}

/// As classes de inspeção que o ledger da rajada acompanha — TODAS as que
/// [`scan_class_of`] reconhece.
///
/// A lista fixa que existia aqui (`grep`/`cat`/`find`) errava nos DOIS sentidos,
/// e a mesma medição mostra as duas pontas: `find` é 56,2% isolada e não tem
/// UMA rajada ≥3 em 115 transcripts — negá-la por classe era fricção pura; já
/// `sed-n` (408 chamadas, 81,4% do volume em rajada) e `ls` (338, 71,3%)
/// passavam sempre, e eram o maior volume fan-out não capturado. O predicado de
/// rajada dispensa a lista: ele discrimina pelo que a classe FAZ nesta janela,
/// não pelo nome dela.
const CODE_MODE_COLLAPSED_CLASSES: &[&str] = &["grep", "cat", "find", "ls", "wc", "sed-n"];

/// Sessão na chave pela mesma razão de [`write_run_key`].
fn exec_burst_key(project_root: &Path, session: &str, class: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}g10-exec\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    session.hash(&mut hasher);
    class.hash(&mut hasher);
    hasher.finish()
}

/// S4 — deny de G10 pendente por sessão: o próximo `touring run` fecha o
/// continuation-check (a conversão é o programa agregado).
fn pending_g10() -> &'static moka::sync::Cache<String, ()> {
    static C: OnceLock<moka::sync::Cache<String, ()>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(1024)
            .time_to_live(Duration::from_secs(60))
            .build()
    })
}

/// S4 — o programa R9 (exec-agregado): as N chamadas REAIS, verbatim e na
/// mesma ordem, cada uma seguida do veredito `[ok]`/`[FALHOU N]` (a execução
/// continua nas falhas), RESUMO final — a íntegra fica no spill quando estoura.
///
/// Em BASH, não python (30/08/2026): o template anterior era `--lang python`
/// com `import subprocess` — a capability que o X6 negava sob Sandboxed, e o
/// deny do G10 emitia uma rota que o gate seguinte barrava (medido pela peer
/// `analise-a2`). No mesmo dia Gabriel estendeu o waiver subprocess-only a
/// toda linguagem, então o template python voltaria a executar — mas bash
/// segue sendo a forma certa AQUI: os comandos viajam verbatim, 1:1, sem a
/// camada de reescrita em subprocess que o modelo teria de conferir. Rede e
/// padrões destrutivos seguem hard-deny em toda lang. `--timeout-ms` viaja
/// explícito (E4): N ferramentas reais não cabem no default de 30s.
fn r9_exec_program(cmds: &[String]) -> String {
    let mut corpo = String::from("falhas=0\n");
    let mut ordem = 0usize;
    for cmd in cmds {
        let cabe = corpo.chars().count() + cmd.chars().count() + 64 <= G1_BODY_BUDGET;
        if !cabe {
            continue; // inteiro ou fora, jamais truncado (mesma regra do R9 antigo)
        }
        ordem += 1;
        corpo.push_str(&format!("echo \"== {ordem}/{} ==\"\n", cmds.len()));
        corpo.push_str(cmd);
        corpo.push_str(&format!(
            " || {{ echo \"[FALHOU {ordem}] exit=$?\"; falhas=$((falhas+1)); }}\n"
        ));
    }
    corpo.push_str(&format!(
        "echo \"RESUMO: $(( {ordem} - falhas ))/{ordem} ok\""
    ));
    format!(
        "touring run --lang bash --timeout-ms 120000 --code '{}'",
        corpo.replace('\'', "'\\''")
    )
}

/// S6 (26/08) — intent de prior-art derivado da rajada real: a classe do
/// executor + os alvos recorrentes (BM25 precisa dos termos do TRABALHO,
/// não da forma da rajada). Máx 6 termos — intent longo dilui o ranking.
fn exec_burst_intent(class: &str, cmds: &[String]) -> String {
    /// Componente de trabalho (dir ou pedaço de stem): ≥2 chars, não-dígito,
    /// dedup. Tokens de caminho INTEIROS não entram — `tests/test_1.py`
    /// diluiria o intent em termos que nenhum propósito contém (medido vivo:
    /// 6 termos → required_matches 3 → 2 matches → o portfolio calado com o
    /// artefato certo na prateleira).
    fn push_componente(t: &str, termos: &mut Vec<String>) {
        let t = t.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');
        if t.len() >= 2
            && !t.chars().all(|c| c.is_ascii_digit())
            && !termos.iter().any(|x| x == t)
        {
            termos.push(t.to_string());
        }
    }
    let mut termos: Vec<String> = vec![class.to_string()];
    'outer: for cmd in cmds.iter().take(8) {
        for tok in effective_tokens(cmd).into_iter().skip(1) {
            let limpo = tok.trim_matches(|c: char| "'\"()[]{},".contains(c));
            if limpo.starts_with('-') {
                continue;
            }
            if limpo.contains('/') || limpo.contains('.') {
                let sem_ext = limpo.split('.').next().unwrap_or(limpo);
                for parte in sem_ext.split('/') {
                    for pedaco in parte.split(['_', '-']) {
                        push_componente(pedaco, &mut termos);
                    }
                }
                if termos.len() >= 6 {
                    break 'outer;
                }
            }
        }
    }
    termos.join(" ")
}

/// S6 — o prior art mais próximo da rajada, instanciado: quando o portfólio
/// tem um programa que JÁ funcionou para este intent, a rota vem escrita E
/// vem de um artefato real (a diferença para o esqueleto R9, que é genérico
/// por construção). Fail-open em todas as direções: sem índice, sem entrada,
/// sem entry_point, ou entry_point com placeholder não-derivável → `None` e
/// o deny sai só com o R9 (o comportamento anterior, honesto).
fn portfolio_remedy_for_burst(class: &str, cmds: &[String]) -> Option<String> {
    let index = portfolio_index()?;
    if index.is_empty() {
        return None;
    }
    let intent = exec_burst_intent(class, cmds);
    let answer = touring_foundation::portfolio::query::answer(&index, &intent, 1);
    let top = answer.prior_art.first()?;
    let entry_point = top.entry.entry_point.as_deref()?;
    if entry_point.contains('<') {
        return None; // placeholder não-derivável: apresentar seria fabricar
    }
    Some(format!(
        "\n  prior art (programa que já funcionou para este trabalho): {entry_point} \
         — {} · score {:.2}",
        top.entry.display_path, top.score
    ))
}

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
    // W2 d1/S-2.4 — the 1-line SDK signature form (P23: ~53 tok, calibrated);
    // the full byte-stable contract stays on demand behind `--sdk-stub`.
    let may = vec![cmd(
        "touring run --lang python --orchestrate --code '<uses touring.index_find(sym) | \
         ast_blast(f) | ast_overview(f) | wiring_status() | wiring_impact(sym,d) | \
         memory_recall(q) | tantivy_search(q) | search(q)>'",
        "orchestrate: the script queries the daemon in-sandbox (full typed contract: \
         touring run --sdk-stub)",
    )];
    ClassifierOutput {
        cluster: cluster.into(),
        must,
        should,
        may,
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
fn detect_code_mode(
    project_root: &Path,
    tool_name: &str,
    tool_input: &Value,
) -> Option<ClassifierOutput> {
    match code_mode_kind(tool_name, tool_input)? {
        CodeModeKind::Loop => Some(code_mode_output(&CodeModeKind::Loop, tool_name, tool_input)),
        CodeModeKind::Scan => {
            if scan_window_crosses_threshold(project_root) {
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
    if let Some(code_mode) = detect_code_mode(&rt.project_root, tool_name, tool_input) {
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

/// F3 — classify the current action and fold it into the adoption_ratio counters;
/// also counts EVERY `Bash` action into the code-mode adoption denominator
/// (W0 S-0.1 — unconditional, before classification, so the denominator sees
/// all actions, not just the classified subsets).
/// Extracted from `run` to keep the hot path flat. Fail-open + infallible.
/// N5 (26/08) — a classe da ESCOLHA no eixo da injeção nativa ("use Bash
/// rather than dedicated tools"): `followed` (Bash onde Grep/Glob/Read
/// existia), `resisted` (a tool dedicada), `code_route` (touring run/exec).
/// Pura — testável sem tocar os contadores; espelho 1:1 do
/// `classify_tool_call` em scripts/n5_injection_kpi.py (paridade guardada
/// pelos mesmos casos nos dois lados).
fn native_injection_class(tool_name: &str, tool_input: &Value) -> Option<&'static str> {
    match tool_name {
        "Grep" | "Glob" | "Read" => Some("resisted"),
        "Bash" => {
            let cmd = tool_input.get("command").and_then(Value::as_str)?;
            if cmd.contains("touring run") || cmd.contains("touring exec") {
                Some("code_route")
            } else if matches!(scan_class_of(cmd), Some("grep" | "find" | "cat" | "sed-n")) {
                Some("followed")
            } else {
                None
            }
        }
        _ => None,
    }
}

/// N5 — registra a escolha ANTES de qualquer gate: um deny posterior não
/// apaga o sinal da pressão (o KPI mede escolhas, não execuções). Global por
/// daemon; a taxa POR SESSÃO vem do minerador scripts/n5_injection_kpi.py.
fn record_native_injection(tool_name: &str, tool_input: &Value) {
    use crate::shared::gate_metrics as gm;
    match native_injection_class(tool_name, tool_input) {
        Some("followed") => gm::record_native_injection_followed(),
        Some("resisted") => gm::record_native_injection_resisted(),
        Some("code_route") => gm::record_native_injection_code_route(),
        _ => {}
    }
}

fn record_adoption(tool_name: &str, tool_input: &Value) {
    if tool_name == "Bash" {
        crate::shared::gate_metrics::record_bash_call();
    }
    match classify_adoption(tool_name, tool_input) {
        Some(AdoptionClass::Touring) => crate::shared::gate_metrics::record_adoption_touring(),
        Some(AdoptionClass::Antipattern) => {
            crate::shared::gate_metrics::record_adoption_antipattern()
        }
        None => {}
    }
}

// ── W1 (plano code-mode-total) — gates de code mode com EXECUTOR ─────────────
//
// G2 (exit-code-through-pipe) e G6 (redundant-exact-call): os dois gates da
// simulação com FP ~zero (68 e 46 disparos / 55 sessões). Diferem de todo o
// resto deste arquivo num ponto: NEGAM (`permissionDecision: deny`) com o
// remédio derivado do comando REAL — afordância, não persuasão (D8: o
// enforcement mora no executor; nudges MUST conf 0.95 foram ignorados na
// própria sessão que os emitiu). Orçamento: < 1ms (scan de bytes + moka).

// ── G11 (2026-09-02) — orçamento do bypass ──────────────────────────────────
//
// O token de bypass é por-comando e legítimo. O que não era medido é a SÉRIE:
// medido no autor desta wave, 72 Bash num turno com zero `touring run` e 64
// bypasses. Um desvio mais barato que a rota sancionada é uma afordância na
// direção errada — e nenhum nudge corrige um custo. Este gate faz o segundo
// bypass custar, e faz a rota sancionada ser o que recarrega o direito.

/// Janela do orçamento, alinhada à do G10 (o gate de execução em rajada).
const G11_WINDOW_SECS: u64 = 600;

/// Bypasses ESTRITAMENTE seguidos tolerados. `1` significa: o primeiro passa, o
/// segundo colado no primeiro não.
const G11_MAX_STRICT_RUN: u32 = 1;

/// Bypasses tolerados desde o último uso da rota sancionada, ainda que
/// intercalados por comandos neutros. `2` significa: o terceiro não passa.
const G11_MAX_SINCE_ROUTE: u32 = 2;

/// Contas do orçamento de bypass de um escopo, dentro da janela.
///
/// São DUAS contas porque as duas regras são distintas, e uma só as confundia:
/// medido na estreia, um bypass separado por um comando neutro era negado como
/// "dois seguidos", que não era verdade.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BypassBudget {
    /// Bypasses colados um no outro; qualquer outro comando zera.
    pub(crate) strict_run: u32,
    /// Bypasses desde o último `touring run`; só a rota sancionada zera.
    pub(crate) since_route: u32,
}

/// A decisão do G11 sobre um bypass que ACABOU de ser cobrado.
///
/// `None` libera. `Some(regra)` nega, e a regra nomeada entra na razão do deny —
/// um gate mudo degrada o retry (a lição dos gates falantes, A5).
#[must_use]
pub(crate) fn bypass_verdict(budget: BypassBudget) -> Option<&'static str> {
    if budget.strict_run > G11_MAX_STRICT_RUN {
        return Some("dois bypasses seguidos");
    }
    if budget.since_route > G11_MAX_SINCE_ROUTE {
        return Some("terceiro bypass sem usar a rota");
    }
    None
}

/// Ledger do orçamento, por escopo, com a mesma expiração dos demais gates.
fn bypass_ledger() -> &'static moka::sync::Cache<u64, BypassBudget> {
    static LEDGER: OnceLock<moka::sync::Cache<u64, BypassBudget>> = OnceLock::new();
    LEDGER.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(256)
            .time_to_live(std::time::Duration::from_secs(G11_WINDOW_SECS))
            .build()
    })
}

/// Chave do ledger: o escopo, como nos demais gates deste módulo.
fn bypass_key(project_root: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}code-mode-bypass\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    hasher.finish()
}

/// Cobra um bypass e devolve as contas já atualizadas.
fn charge_bypass(project_root: &Path) -> BypassBudget {
    let key = bypass_key(project_root);
    let prev = bypass_ledger().get(&key).unwrap_or_default();
    let next = BypassBudget {
        strict_run: prev.strict_run.saturating_add(1),
        since_route: prev.since_route.saturating_add(1),
    };
    bypass_ledger().insert(key, next);
    next
}

/// A rota sancionada foi usada: o crédito volta inteiro.
fn credit_sanctioned_route(project_root: &Path) {
    bypass_ledger().invalidate(&bypass_key(project_root));
}

/// Um comando que não é bypass quebra a sequência ESTRITA, e só ela.
///
/// A conta `since_route` sobrevive de propósito: caso contrário, bastaria
/// intercalar um `echo` entre os bypasses para zerar o orçamento, que é
/// exatamente a série espaçada que a R2 existe para impedir.
fn break_strict_run(project_root: &Path) {
    let key = bypass_key(project_root);
    if let Some(prev) = bypass_ledger().get(&key)
        && prev.strict_run > 0
    {
        bypass_ledger().insert(
            key,
            BypassBudget {
                strict_run: 0,
                since_route: prev.since_route,
            },
        );
    }
}

/// `true` quando o comando É a rota sancionada (um programa único no sandbox).
fn is_sanctioned_route(cmd: &str) -> bool {
    let c = cmd.trim_start();
    c.starts_with("touring run ") || c.contains(" touring run ") || c == "touring run"
}

/// Token de bypass POR-COMANDO: viaja no próprio comando (o padrão
/// `GIT_DESTRUCTIVE_OK` — cada uso é uma decisão, nunca um estado exportado).
const GATE_BYPASS_TOKEN: &str = "TOURING_GATE_OK=1";

/// Kill switch global (humano; env do DAEMON — exige `touring daemon-ctl
/// restart` para valer, como `TOURING_PILLAR_INDUCTION_ARMED`).
fn code_gates_disabled() -> bool {
    std::env::var("TOURING_CODE_GATES_DISABLED").map(|v| v == "1") == Ok(true)
}

/// W1 S-1.2 — comandos de estado vivo: chamadas idênticas legitimamente
/// retornam resultados diferentes (saúde, daemon, working tree, contadores).
/// Allowlist EXPLÍCITA, não heurística; ampliar só com dado (S-7.2).
const G6_LIVE_STATE_ALLOWLIST: &[&str] = &[
    "touring doctor",
    "touring status",
    "touring daemon-ctl",
    "touring gate-metrics",
    "touring kpi",
    "git status",
    "git diff",
];

fn g6_allowlisted(cmd: &str) -> bool {
    let trimmed = cmd.trim_start();
    G6_LIVE_STATE_ALLOWLIST.iter().any(|p| trimmed.starts_with(p))
}

/// W1 S-1.2 — ledger de repetição exata: input-hash → (repetições, época de
/// mutação do projeto na primeira vista). TTL espelha o anti-spam (300s).
fn g6_seen() -> &'static moka::sync::Cache<u64, (u32, u64)> {
    static C: OnceLock<moka::sync::Cache<u64, (u32, u64)>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(SUGGESTION_TTL_SECS))
            .build()
    })
}

/// W1 — época de mutação por projeto: todo Edit/Write a avança. Uma repetição
/// G6 vista numa época mais velha é stale (a árvore mudou) e NUNCA dispara —
/// mata a classe ler-depois-de-editar de falso positivo por construção.
fn mutation_epoch() -> &'static moka::sync::Cache<String, u64> {
    static C: OnceLock<moka::sync::Cache<String, u64>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CONFORMAL_TAU_MAX_PROJECTS)
            .build()
    })
}

/// W1 S-1.4 — deny de G2 pendente por sessão: a próxima chamada Bash com
/// `pipefail` dentro de 60s conta como `followed` (o contrato pillar_induction).
/// N3a — deny de G9 pendente por sessão: a próxima chamada Edit/Write da
/// sessão fecha o continuation-check (a conversão é o Edit com os 17 gates).
fn pending_g9() -> &'static moka::sync::Cache<String, ()> {
    static C: OnceLock<moka::sync::Cache<String, ()>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(1024)
            .time_to_live(Duration::from_secs(60))
            .build()
    })
}

fn pending_g2() -> &'static moka::sync::Cache<String, ()> {
    static C: OnceLock<moka::sync::Cache<String, ()>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(1024)
            .time_to_live(Duration::from_secs(60))
            .build()
    })
}

// ── W2 (plano code-mode-total) — G1: teeth no contador de rajada ─────────────

/// W2 S-2.1 — a inspeção repetida da MESMA classe na janela nega (o advisory
/// legado fala uma antes). 90% de precisão-proxy medida na 4ª (51/55 sessões);
/// **aperto 29/08 (ordem de Gabriel): 4→3** — na 3ª o padrão já é inequívoco
/// (o mesmo racional do CODE_MODE_SCAN_THRESHOLD) e o custo do FP segue 1 bypass.
const G1_DENY_AT: u32 = 3;
/// W2 S-2.3 — piso de precisão viva abaixo do qual o G1 se autodemove a
/// advisory (F7: demote é código, não reunião)…
const G1_DEMOTE_FLOOR: f64 = 0.70;
/// …desde que haja volume real: um punhado de eventos iniciais nunca demove.
const G1_DEMOTE_MIN_EVENTS: u64 = 100;

/// W2 S-2.1 — classe da inspeção atômica (a taxonomia da simulação). `None`
/// para comandos que não são inspeção — a rajada só conta o que inspeciona.
fn scan_class_of(cmd: &str) -> Option<&'static str> {
    // Verbo REAL pós-segmentos/prefixos-cd/wrappers (S1, 26/08): `time grep`
    // é `grep`, `cd /x && cat f` é `cat` — antes o 1º token decidia, e
    // `time`/`nice`/`sudo`/`env`/`cd` anulavam a classe para os 4 gates que
    // consomem este classificador de uma vez só.
    let rest = effective_tokens(cmd);
    let first = *rest.first()?;
    match first {
        "grep" | "rg" => Some("grep"),
        // P2.3 (calibração 25/08, 1.312 chamadas reais): `cat > f`/`cat >> f`
        // (heredoc de escrita) é T4 — escrita — não inspeção. O prefixo sozinho
        // negava 27 escritas (20,6% do que a matriz pegaria) sob o modo `code`,
        // que nega na 1ª chamada. Se o primeiro operando não-flag já é um
        // redirect, não há arquivo de leitura: não é classe `cat`.
        "cat" | "head" | "tail" | "less" => {
            // O primeiro operando REAL decide: flags (`-n`) — e o argumento
            // das que tomam valor (`-n 5`, `-c +3`) — são transparentes. Se o
            // que sobra já começa com `>`, não há arquivo de leitura.
            let mut it = rest[1..].iter().copied().peekable();
            let mut primeiro_operando: Option<&str> = None;
            while let Some(t) = it.next() {
                if t == "-n" || t == "-c" {
                    it.next(); // o argumento da flag, não um operando
                    continue;
                }
                if t.starts_with('-') {
                    continue;
                }
                primeiro_operando = Some(t);
                break;
            }
            match primeiro_operando {
                Some(op) if op.starts_with('>') => None,
                _ => Some("cat"),
            }
        }
        "find" | "fd" => Some("find"),
        "ls" => Some("ls"),
        "wc" => Some("wc"),
        // `-n` do sed, não do invólucro: `nice -n 5 sed 5p f` NÃO é sed-n
        // (com o verbo resolvido, o `contains` no comando inteiro herdaria o
        // `-n` do `nice` — o bug latente que a resolução expõe).
        "sed" if rest[1..].contains(&"-n") => Some("sed-n"),
        _ => None,
    }
}

/// W2 — ledger da rajada por (projeto, classe): contagem + os comandos REAIS
/// acumulados (cap 8) — eles viram o corpo do programa no remédio do deny.
/// TTL = a janela do contador legado (o mesmo sinal, agora com memória).
/// Entrada da rajada: `(contagem, comandos, época de mutação da última inserção)`.
///
/// A época entrou junto com a calibração por similaridade: sem ela, uma
/// releitura DEPOIS de um Edit era negada como repetição — e reler o que
/// acabou de mudar é legítimo, não redundância. É a mesma proteção que o G6 já
/// tinha (`mutation_epoch`), agora estendida ao gatilho novo. O teste
/// `g6_mutacao_no_meio_reseta_a_repeticao` pegou a regressão.
type BurstEntry = (u32, Vec<String>, u64);

fn burst_ledger() -> &'static moka::sync::Cache<u64, BurstEntry> {
    static C: OnceLock<moka::sync::Cache<u64, BurstEntry>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(CODE_MODE_WINDOW_SECS))
            .build()
    })
}

fn burst_key(project_root: &Path, class: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}g1-burst\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    class.hash(&mut hasher);
    hasher.finish()
}

/// W2 S-2.3 — deny de G1 pendente por sessão (guarda a CLASSE negada): a
/// próxima chamada da sessão fecha o continuation-check do A/B.
fn pending_g1() -> &'static moka::sync::Cache<String, String> {
    static C: OnceLock<moka::sync::Cache<String, String>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(1024)
            .time_to_live(Duration::from_secs(60))
            .build()
    })
}

/// Resposta advisory do PreToolUse (additionalContext, nunca bloqueia).
fn advisory_response(context: String) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "additionalContext": context,
        }
    })
    .to_string()
}

// ── W3 (plano code-mode-total) — gates de MODO: G3 e G7 + telemetria G4/G5 ───

/// W3 S-3.1 — G3: streak de Edits sem Read recente, POR SESSÃO (modo de
/// sessão, não população: 336 disparos concentrados em 15/55 sessões; os 11
/// erros `edit_string_not_found` medidos são o dano vivo).
fn g3_streak() -> &'static moka::sync::Cache<String, u32> {
    static C: OnceLock<moka::sync::Cache<String, u32>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(1024)
            .time_to_live(Duration::from_secs(3600))
            .build()
    })
}

/// W3 S-3.1 — (sessão, arquivo) lidos recentemente; um Edit de arquivo lido
/// não conta streak.
fn g3_read_files() -> &'static moka::sync::Cache<u64, ()> {
    static C: OnceLock<moka::sync::Cache<u64, ()>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(3600))
            .build()
    })
}

fn g3_read_key(project_root: &Path, session: &str, file: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}g3-read\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    session.hash(&mut hasher);
    file.hash(&mut hasher);
    hasher.finish()
}

/// W3 S-3.2 — G7: contagem de releituras do MESMO arquivo por projeto (os
/// campeões medidos: adw.py 39×, cli_suggester.rs 32× — o sinal mais direto de
/// programa-faltando). 2ª → advisory com R1 instanciado; 3ª → deny
/// (aperto 29/08, ordem de Gabriel: era 3ª/5ª — "5 é muito frouxo").
fn g7_seen() -> &'static moka::sync::Cache<u64, u32> {
    static C: OnceLock<moka::sync::Cache<u64, u32>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(3600))
            .build()
    })
}

fn g7_key(project_root: &Path, file: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "\u{2}g7-target\u{2}".hash(&mut hasher);
    project_root.hash(&mut hasher);
    file.hash(&mut hasher);
    hasher.finish()
}

/// M3 (estratégia paralelização-agentes 29/08/2026) — arquivos DISTINTOS
/// lidos pela sessão na janela de 1h. O gatilho ≥10 é o medido do blog
/// subagents-in-claude-code ("exploring ten or more files" → subagente;
/// razão nº 1: isolamento de contexto — 50 arquivos lidos voltam como 3
/// conclusões). Dispara UMA vez, no exato 10º arquivo novo; advisory, nunca
/// deny — delegar é juízo da sessão, não do gate.
fn m3_distinct_reads() -> &'static moka::sync::Cache<String, u32> {
    static C: OnceLock<moka::sync::Cache<String, u32>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(1024)
            .time_to_live(Duration::from_secs(3600))
            .build()
    })
}

/// M3 — quantos arquivos distintos disparam o advisory de delegação.
const M3_DELEGATION_AT: u32 = 10;

/// W3 S-3.3 — G4 (telemetria): a sessão localizou algo (Grep/Glob) há pouco?
fn g4_last_locate() -> &'static moka::sync::Cache<String, ()> {
    static C: OnceLock<moka::sync::Cache<String, ()>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(1024)
            .time_to_live(Duration::from_secs(120))
            .build()
    })
}

/// W3 S-3.3 — G5 (telemetria): streak de Edits/Writes da sessão; zera na
/// primeira ação Bash e, se a rajada terminou ≥3 sem validação, 1 advisory.
fn g5_edit_streak() -> &'static moka::sync::Cache<String, u32> {
    static C: OnceLock<moka::sync::Cache<String, u32>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(1024)
            .time_to_live(Duration::from_secs(3600))
            .build()
    })
}

/// W6 S-6.3 — linha ADICIONADA de comentário com modal contrafactual e nenhum
/// run_id na vizinhança: a afirmação nunca exercitada (o pior erro de 24/08).
fn counterfactual_comment(added: &str) -> bool {
    const MODAIS: &[&str] = &["seria ", "quebraria", "impediria", "faria com que"];
    added.lines().any(|l| {
        let t = l.trim_start();
        let comentario = t.starts_with("//") || t.starts_with('#') || t.starts_with('*');
        comentario && MODAIS.iter().any(|m| t.contains(m)) && !t.contains("run-")
    })
}

/// W2 S-2.3 — decide deny vs advisory a partir da precisão VIVA. Pura, para o
/// teste não depender dos contadores globais de processo.
fn g1_should_deny(precision: Option<(f64, u64)>) -> bool {
    !matches!(precision,
        Some((p, volume)) if volume >= G1_DEMOTE_MIN_EVENTS && p < G1_DEMOTE_FLOOR)
}

/// A forma em que as ferramentas chegam ao modelo NESTE escopo.
///
/// A definição canônica mora em [`touring_foundation::code_mode`] porque DOIS
/// executores a impõem — este hook (`PreToolUse`) e o handshake MCP
/// (`touring-server::server::apply_curation`, S1 TRANSPORT). Duplicar o tipo e
/// o parser recriaria a falha que este workspace já pagou duas vezes: sítios da
/// mesma regra que divergem (2026-08-23) e uma declaração que promete o que o
/// executor não aplica (achado D8, 2026-08-25).
pub(crate) use touring_foundation::code_mode::CodeModePresentation;

// **Calibração REVISTA pelo S3 (27/08/2026) — a lista fixa saiu, o predicado
// de rajada entrou.** A tabela abaixo é a de 25/08 (1.312 chamadas), mantida
// porque foi ela que motivou a revisão, com o dado de 27/08 ao lado
// (115 transcripts, janela 300s, `scripts/s3_burst_distribution.py`):
//
// | classe | colapsava? | chamadas | % isoladas | % volume em rajada ≥2 |
// |---|---|---|---|---|
// | `grep`  | sim  | 770 | 12,6% | 87,4% |
// | `sed-n` | NÃO  | 408 | 18,6% | 81,4% |
// | `ls`    | NÃO  | 338 | 28,7% | 71,3% |
// | `cat`   | sim  | 316 | 26,6% | 73,4% |
// | `wc`    | NÃO  |  34 | 67,6% | 32,4% |
// | `find`  | sim  |  32 | 56,2% | 43,8% (zero rajadas ≥3) |
//
// A lista fixa errava nos dois sentidos ao mesmo tempo: negava `find`, que é
// majoritariamente isolada e não produziu UMA rajada ≥3 em 115 transcripts, e
// deixava passar `sed-n` e `ls`, que juntas somam 746 chamadas com ~76% do
// volume em rajada. Nome de classe não prediz fan-out; o que prediz é o que a
// classe está fazendo NESTA janela — e isso um contador responde, uma lista não.
//
// Os níveis por classe são exatamente o desenho que o DeepSeek ADIOU (*"its
// design depends on evidence about how models split usage under `both`"*). A
// evidência que faltava a eles é esta tabela: rodamos em `both` instrumentado,
// e depois medimos de novo para corrigir a nossa própria primeira leitura.

/// Resolve a apresentação: **prefixo do comando → env do hook → alias → projeto → default**.
///
/// Duas verdades medidas (25/08) moram nesta ordem:
///
/// 1. **Exportar no shell da tool Bash NÃO chega ao hook.** O processo do hook
///    e o shell que executa o comando são IRMÃOS spawnados pelo Claude Code —
///    `export TOURING_CODE_MODE=…` num é invisível ao outro. A env que esta
///    função lê com `std::env::var` é a do processo do hook (a do daemon que
///    o spawnou), não a "da sessão" do operador.
/// 2. **A via que atravessa é a linha de comando.** É assim que
///    `TOURING_GATE_OK=1 <cmd>` já funciona: o hook lê o prefixo `VAR=valor`
///    do próprio comando. Por isso o nível mais externo é o prefixo — útil
///    sobretudo para RELAXAR por-comando (`TOURING_CODE_MODE=native grep …`),
///    simétrico ao token de bypass.
///
/// `TOURING_CODE_ONLY=1` continua valendo como alias de `Code` (compatibilidade
/// com o piloto S-8.1), agora com o escopo e a calibração que lhe faltavam — ele
/// negava TODA classe, `ls` inclusive.
fn code_mode_presentation(project_root: &Path, cmd: &str) -> CodeModePresentation {
    for token in cmd.split_whitespace() {
        let Some((nome, valor)) = token.split_once('=') else { break };
        if !nome.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') {
            break;
        }
        if nome == "TOURING_CODE_MODE" {
            match CodeModePresentation::parse(valor) {
                Some(m) => return m,
                None => break, // valor inválido não cala os níveis seguintes
            }
        }
    }
    if let Ok(v) = std::env::var("TOURING_CODE_MODE") {
        // parse() é o parser canônico (trim + unquote): uma env com o valor
        // entre aspas agora também resolve.
        if let Some(m) = CodeModePresentation::parse(&v) {
            return m;
        }
    }
    if std::env::var("TOURING_CODE_ONLY").map(|v| v == "1") == Ok(true) {
        return CodeModePresentation::Code;
    }
    // A declaração humana vence SEMPRE: prefixo, env, alias e `touring.toml` já
    // decidiram acima. A política só preenche o espaço que o humano deixou em
    // aberto — sobrescrever uma declaração explícita não seria aprender, seria
    // desobedecer.
    resolve_with_policy(
        project_presentation(project_root),
        code_mode_arm_armed(),
        arm_counts_durable(project_root),
    )
}

/// Lê `[code_mode] mode` de `<root>/.touring/touring.toml`.
///
/// Delega ao parser canônico do `touring-foundation` — o mesmo que o handshake
/// MCP consulta, para que a apresentação declarada e a imposta nunca possam
/// divergir. Reexportado no escopo do módulo porque os testes o consultam
/// diretamente.
pub(crate) use touring_foundation::code_mode::project_presentation;

/// Orçamento total, em chars, do corpo do programa que o remédio entrega.
const G1_BODY_BUDGET: usize = 3000;

/// Funde a rajada acumulada num corpo de programa executável.
///
/// Devolve `(corpo, omitidos)`. A regra é **comando inteiro ou nenhum**: um
/// comando que não cabe no orçamento é DESCARTADO, jamais truncado.
///
/// Origem (2026-08-25, observado ao vivo nesta função): cada comando entrava
/// com `cmd.chars().take(240)`, então uma rajada de greps longos produzia um
/// programa cortado no meio de um caminho (`crates/tou'`) — sintaticamente
/// plausível, com aspas equilibradas, e que **não roda**. Um comando truncado
/// é pior que um ausente: o ausente aparece na contagem que o remédio declara;
/// o truncado se disfarça de programa completo. A emenda do Gabriel (25/08) é
/// exatamente esta — a injeção tem de entregar o snippet que SUBSTITUI as N
/// chamadas, e um snippet que falha não substitui nada.
fn fuse_burst_program(cmds: &[String]) -> (String, usize) {
    let mut corpo = String::new();
    let mut omitidos = 0usize;
    for cmd in cmds {
        let cabe = corpo.chars().count() + cmd.chars().count() + 1 <= G1_BODY_BUDGET;
        if cabe {
            if !corpo.is_empty() {
                corpo.push('\n');
            }
            corpo.push_str(cmd);
        } else {
            omitidos += 1;
        }
    }
    (corpo.replace('\'', "'\\''"), omitidos)
}


/// A rota escrita que a apresentação entregou ao modelo, com o braço que a produziu.
///
/// Contadores de FREQUÊNCIA dizem com que assiduidade um gate age e nada sobre
/// se agir funcionou — telemetria sem consumidor de aprendizado (medido
/// 25/08/2026: zero políticas os liam; os contadores do T3-B eram o caso
/// exemplar, e o gate inteiro saiu no S10). Esta é a metade que faltava: a
/// apresentação vigente vira o braço, e o comando seguinte vira a recompensa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteOffer {
    /// `native` | `code` | `both` — a apresentação em vigor quando a rota saiu.
    pub mode: String,
    /// Quando foi oferecida (secs desde o UNIX_EPOCH).
    pub offered_secs: u64,
}


fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}






/// Reivindica a rota pendente deste (projeto, sessão) — UMA vez.
///
/// Mesma disciplina dos ledgers de decisão e de casos: uma oferta reivindicada
/// some, para que duas leituras nunca creditem a mesma decisão duas vezes. A
/// sessão sai da MESMA `session_key` do pre e do close, ou pre e post estariam
/// falando de turnos diferentes com a mesma cara.
pub fn take_route_offer(project_root: &Path, payload: &Value) -> Option<RouteOffer> {
    let key = format!("{}\u{2}{}", project_root.display(), session_key(payload));
    let offer = route_offers().get(&key)?;
    route_offers().invalidate(&key);
    Some(offer)
}

/// O que um programa de code mode COMPROU em round-trips.
///
/// `adoption_ratio` responde "usou a ferramenta certa?" e nada sobre "a chamada
/// valia um round-trip?" — mede o CANAL, não a ECONOMIA (achado de Gabriel,
/// 26/08/2026: `scan_class_of` não reconhece `touring run`, então N programas
/// diferentes e triviais somam N adoções e zero avisos).
///
/// Um programa que inspeciona UM alvo com UMA operação não fundiu nada: é uma
/// chamada atômica com uma casca. Isso não é indisciplina do modelo — sob a
/// apresentação `code` a chamada atômica é NEGADA, então a casca é obrigatória.
/// Por isso a economia é lida como CUSTO DA APRESENTAÇÃO e não como falta do
/// modelo: é o número que impede o braço `code` de parecer ótimo só porque
/// todos obedecem, quando cada obediência custa um round-trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramShape {
    /// Uma operação sobre um alvo — a casca não comprou nada.
    Trivial,
    /// Fundiu `n` operações/alvos numa chamada só.
    Fused(usize),
}

/// Verbos de inspeção reconhecidos no corpo, em qualquer linguagem.
const INSPECTION_VERBS: [&str; 12] = [
    "grep", "rg", "cat", "head", "tail", "sed", "awk", "ls", "wc", "find", "stat", "jq",
];

/// Extrai o corpo de `touring run … --code '<corpo>'`.
///
/// Sem o corpo não há o que classificar, e `None` mantém o comportamento
/// anterior — jamais adivinhar a forma de um programa que não se leu.
pub(crate) fn extract_run_body(cmd: &str) -> Option<&str> {
    let idx = cmd.find("--code")?;
    let resto = &cmd[idx + "--code".len()..];
    let resto = resto.trim_start();
    let aspa = resto.chars().next()?;
    if aspa != '\'' && aspa != '"' {
        return None;
    }
    let corpo = &resto[aspa.len_utf8()..];
    // A PRIMEIRA aspa igual fecha, não a última. `rfind` capturava além do
    // corpo quando o comando externo trazia mais aspas depois — medido ao vivo
    // em 26/08: `… --code 'cat X' 2>/dev/null | python3 -c "…print('…')"`
    // devolvia um corpo que incluía o pipe inteiro, e o `/dev/null` de dentro
    // dele contava como um segundo alvo: um programa trivial era classificado
    // como fusão. Em shell, string entre aspas simples não contém a própria
    // aspa, então a primeira É a de fechamento.
    let fim = corpo.find(aspa)?;
    Some(&corpo[..fim])
}

/// Classifica o corpo pelo que ele funde. Puro — mais fácil de testar do que
/// de contornar.
///
/// Dois eixos, e o MAIOR decide: quantas operações de inspeção o corpo executa,
/// e quantos alvos distintos ele toca. Um `grep` sobre três arquivos fundiu
/// três; três `sed` sobre o mesmo arquivo fundiram três. Um de cada não fundiu
/// nada.
pub(crate) fn program_shape(body: &str) -> ProgramShape {
    // Casar PALAVRA, não substring: `body.matches("rg")` conta 1 dentro de
    // "Cargo.toml", e `cat /a/Cargo.toml` — um arquivo, uma operação — era
    // classificado como fusão de duas. O teste pegou; a medição ao vivo teria
    // levado semanas para revelar, porque o número errado é plausível.
    let ops = body
        .split(|c: char| c.is_whitespace() || matches!(c, ';' | '|' | '&' | '(' | ')' | '"' | '\''))
        .filter(|t| INSPECTION_VERBS.contains(&t.trim_start_matches("$(")))
        .count()
        // `open(` é chamada de função (Python), não token isolado.
        + body.matches("open(").count();
    let alvos: std::collections::BTreeSet<&str> = body
        .split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '(' || c == ')')
        .filter(|t| t.contains('/') && t.len() > 3 && !t.starts_with('-'))
        // Dispositivos e descritores não são alvos de inspeção: `2>/dev/null`
        // é plumbing do shell, e contá-lo fazia um programa de um arquivo
        // parecer que tocava dois (medido ao vivo em 26/08).
        .filter(|t| !t.trim_start_matches(['>', '<', '&', '1', '2']).starts_with("/dev/"))
        .collect();
    let fundido = ops.max(alvos.len());
    if fundido <= 1 {
        ProgramShape::Trivial
    } else {
        ProgramShape::Fused(fundido)
    }
}

/// O que o modelo fez com a rota que recebeu — puro, para ser mais fácil de
/// testar do que de contornar.
///
/// `Some(1.0)` rodou o programa; `Some(0.0)` recusou a rota com um token de
/// relaxamento; `None` fez outra coisa — e isso NÃO é veredito. Ausência de
/// sinal é desconhecido, jamais zero: a mesma leitura fail-closed que o nó
/// `loop` aplica a `NEW_FINDINGS`.
///
/// A ordem importa: `TOURING_CODE_MODE=native touring run …` **rodou** o
/// programa. Ter relaxado o gate no caminho não desfaz o fato de a rota ter
/// sido tomada.
pub fn classify_route_outcome(cmd: &str) -> Option<f64> {
    // `TOURING_T3_FUSE_DISABLED=1` saiu com o T3-B (S10, 27/08): reconhecer um
    // kill switch que não desliga nada ensinaria o modelo a digitá-lo.
    const BYPASS: [&str; 3] = [
        "TOURING_CODE_MODE=native",
        "TOURING_GATE_OK=1",
        "TOURING_CODE_GATES_DISABLED=1",
    ];
    if cmd.contains("touring run ") || cmd.contains("touring exec ") {
        // A rota foi tomada — mas COMPROU alguma coisa? Um programa que
        // inspeciona um alvo com uma operação é uma chamada atômica com casca:
        // o canal está certo e a economia é nula. Creditar 1.0 aqui ensinaria
        // ao braço que `code` é ótimo porque todos obedecem, quando cada
        // obediência custou um round-trip (achado de Gabriel, 26/08).
        //
        // 0.5 e não 0.0: sob `code` a chamada atômica é NEGADA, então a casca é
        // obrigatória. Punir como recusa culparia o modelo pela política — o
        // custo é da apresentação, e é dela que o número tem de falar.
        return Some(match extract_run_body(cmd).map(program_shape) {
            Some(ProgramShape::Trivial) => 0.5,
            _ => 1.0,
        });
    }
    if BYPASS.iter().any(|t| cmd.contains(t)) {
        return Some(0.0);
    }
    None
}

/// Semeia uma oferta de rota — só para teste, pela MESMA chave do runtime.
///
/// Um teste que montasse a chave por conta própria provaria que a sua fórmula
/// funciona, não que a do gate funciona.
#[cfg(test)]
pub fn turn_ledger_insert_for_test(project_root: &Path, payload: &Value, offer: RouteOffer) {
    let key = format!("{}\u{2}{}", project_root.display(), session_key(payload));
    route_offers().insert(key, offer);
}

/// Nome estável do braço. É a chave dos contadores e da memória — mudá-lo
/// renomeia o braço e zera a evidência acumulada sem avisar ninguém.
pub(crate) fn presentation_label(p: CodeModePresentation) -> &'static str {
    p.label()
}

/// Registra que a apresentação acabou de entregar uma rota escrita.
///
/// Escritor ÚNICO do campo `route`: dois sítios gravando a mesma oferta seriam
/// duas versões da mesma decisão, e a primeira divergência entre eles só
/// apareceria como um braço aprendendo o oposto do que aconteceu — a classe de
/// `comentario-afirma-simetria-inexistente`.
pub(crate) fn record_route_offer(project_root: &Path, session: &str, mode: CodeModePresentation) {
    let label = presentation_label(mode);
    let key = format!("{}\u{2}{session}", project_root.display());
    route_offers().insert(
        key,
        RouteOffer { mode: label.to_string(), offered_secs: now_secs() },
    );
    // `bump_arm` é o escritor único: ele grava a vista durável E a volátil.
    bump_arm(project_root, label, ArmAxis::Offered);
}

/// Onde vive a evidência DURÁVEL do braço, por projeto.
///
/// Medido em 26/08/2026: `t3_turn_first_passed_count` caiu de 2 para 0 em dois
/// minutos, sem nada acontecer além de um restart de daemon. Os contadores de
/// `gate-metrics` são telemetria de processo — e uma política com piso de
/// amostra que lesse dali NUNCA alcançaria o piso entre deploys: existiria,
/// estaria armada, e não escolheria nada. Exatamente o modo de falha que este
/// plano inteiro cataloga. A decisão lê disco; a telemetria segue observando.
/// Ofertas de rota pendentes, por (projeto, sessão).
///
/// Cache PRÓPRIO e não um campo do `TurnBurst`: guardá-la lá fazia
/// `record_route_offer` chamar `get(...).unwrap_or_default()` num deny
/// code-mode onde o turno muitas vezes NÃO existe — e o default tem
/// `first_passed = false`, então a oferta inseria um turno falso e a fusão T3
/// seguinte nunca disparava. Os testes pegaram isso como vítima alternando
/// entre dois vizinhos, a assinatura de estado global compartilhado.
///
/// A separação também é semântica: a oferta sobrevive ao `turn_gate_close` de
/// propósito (o fim do turno é quando o desfecho fica observável, não quando se
/// esquece dele) — sinal de que nunca foi estado de turno.
fn route_offers() -> &'static moka::sync::Cache<String, RouteOffer> {
    static C: OnceLock<moka::sync::Cache<String, RouteOffer>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(1024)
            .time_to_live(Duration::from_secs(60))
            .build()
    })
}

fn arm_store_path(project_root: &Path) -> std::path::PathBuf {
    project_root.join(".claude/touring/code_mode_arm.json")
}

/// Evidência DURÁVEL das famílias de medição sem arquivo próprio — S3 (rajada
/// de inspeção) e pillar-induction. R2 (29/08, ordem de Gabriel): os KPIs
/// `inspect_burst_share` e `pillar_induction_ratio` liam contadores de
/// processo que zeram a cada restart do daemon (3 deploys num dia = 3 apagões
/// da amostra) — a mesma classe que fez a decisão do braço ler
/// `code_mode_arm.json`. Mesmo contrato do arm: leitura ilegível ⇒ zeros (o
/// consumidor fica calado), falha de escrita ⇒ warn, nunca silêncio.
fn durable_evidence_path(project_root: &Path) -> std::path::PathBuf {
    project_root.join(".claude/touring/durable_gate_evidence.json")
}

/// O arquivo inteiro como JSON; ausente/corrompido ⇒ objeto vazio (evidência
/// que não se pode ler é evidência que não existe).
pub(crate) fn read_durable_evidence(project_root: &Path) -> serde_json::Value {
    std::fs::read_to_string(durable_evidence_path(project_root))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}))
}

/// `+1` em `<family>.<field>`, read-modify-write no arquivo durável. Chamado
/// ao lado do contador volátil correspondente — o vivo alimenta o
/// `gate-metrics` da sessão, o durável alimenta o KPI que precisa de DIAS.
pub(crate) fn bump_durable_evidence(project_root: &Path, family: &str, field: &str) {
    let mut v = read_durable_evidence(project_root);
    let Some(obj) = v.as_object_mut() else { return };
    let fam = obj
        .entry(family.to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !fam.is_object() {
        *fam = serde_json::json!({});
    }
    let Some(f) = fam.as_object_mut() else { return };
    let n = f.get(field).and_then(Value::as_u64).unwrap_or(0);
    f.insert(field.to_string(), serde_json::json!(n + 1));
    let path = durable_evidence_path(project_root);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, v.to_string()) {
        tracing::warn!(
            target: "touring::kpi",
            "durable gate evidence write failed at {}: {e}",
            path.display()
        );
    }
}

/// TTL curto: `code_mode_presentation` roda em todo PreToolUse, e ler o arquivo
/// a cada chamada trocaria um problema de durabilidade por um de latência.
fn arm_cache() -> &'static moka::sync::Cache<String, ArmCounts> {
    static C: OnceLock<moka::sync::Cache<String, ArmCounts>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(64)
            .time_to_live(Duration::from_secs(5))
            .build()
    })
}

const ARM_NAMES: [&str; 3] = ["native", "both", "code"];

fn arm_index(mode: &str) -> Option<usize> {
    ARM_NAMES.iter().position(|n| *n == mode)
}

/// Lê o arquivo. Ausente, ilegível ou corrompido ⇒ zeros: evidência que não se
/// pode ler é evidência que não existe, e zeros deixam a política calada (que é
/// o comportamento seguro), em vez de inventar uma escolha.
/// `(oferecidas, tomadas, economicas)` por braço.
///
/// Três números e não dois porque obediência e economia são eixos DIFERENTES:
/// `tomadas/oferecidas` responde "seguiram a rota?" (o canal) e
/// `economicas/tomadas` responde "a rota comprou round-trips?" (a economia).
/// Fundi-los num só faria um braço obedecido e caro parecer excelente — o
/// defeito que Gabriel apontou na `adoption_ratio`.
pub(crate) type ArmCounts = [(u64, u64, u64); 3];

fn read_arm_file(project_root: &Path) -> ArmCounts {
    let mut out = [(0u64, 0u64, 0u64); 3];
    let Ok(txt) = std::fs::read_to_string(arm_store_path(project_root)) else {
        return out;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
        return out;
    };
    for (i, nome) in ARM_NAMES.iter().enumerate() {
        if let Some(par) = v.get(*nome) {
            out[i].0 = par.get("offered").and_then(Value::as_u64).unwrap_or(0);
            out[i].1 = par.get("followed").and_then(Value::as_u64).unwrap_or(0);
            out[i].2 = par.get("economical").and_then(Value::as_u64).unwrap_or(0);
        }
    }
    out
}

fn write_arm_file(project_root: &Path, counts: &ArmCounts) {
    let path = arm_store_path(project_root);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut obj = serde_json::Map::new();
    for (i, nome) in ARM_NAMES.iter().enumerate() {
        obj.insert(
            (*nome).to_string(),
            serde_json::json!({
                "offered": counts[i].0,
                "followed": counts[i].1,
                "economical": counts[i].2,
            }),
        );
    }
    // Falha de escrita NÃO pode ser silenciosa. Se o diretório não for
    // gravável, a evidência fica em zero para sempre e a política se cala —
    // indistinguível de "ainda não há amostra". Um `warn` é a diferença entre
    // um bug diagnosticável e mais um mecanismo que existe e nunca dispara.
    if let Err(e) = std::fs::write(&path, serde_json::Value::Object(obj).to_string()) {
        tracing::warn!(
            path = %path.display(),
            error = %e,
            "P2: evidência do braço code-mode não pôde ser persistida — a política \
             ficará calada e isso NÃO é ausência de amostra"
        );
    }
}

/// Evidência durável do projeto, com cache curto.
pub(crate) fn arm_counts_full(project_root: &Path) -> ArmCounts {
    let key = project_root.display().to_string();
    if let Some(c) = arm_cache().get(&key) {
        return c;
    }
    let counts = read_arm_file(project_root);
    arm_cache().insert(key, counts);
    counts
}

/// O par `(oferecidas, tomadas)` que a POLÍTICA lê. A economia é reportada
/// (KPI) mas ainda não escolhe braço: mudar o critério de escolha é decisão de
/// Gabriel, não efeito colateral de ter passado a medir.
pub(crate) fn arm_counts_durable(project_root: &Path) -> [(u64, u64); 3] {
    let full = arm_counts_full(project_root);
    [
        (full[0].0, full[0].1),
        (full[1].0, full[1].1),
        (full[2].0, full[2].1),
    ]
}

/// Incrementa um eixo do braço e persiste.
///
/// **Orçamento**: este arquivo declara `< 1ms (scan de bytes + moka)` para o
/// caminho quente, e aqui há I/O de disco. Os dois únicos chamadores estão
/// DENTRO de ramos de deny (a fusão T3 e o deny code-mode), então a leitura +
/// escrita de ~200 bytes é paga só quando um gate age — nunca nas chamadas que
/// passam, que são a esmagadora maioria. O orçamento do caminho comum segue
/// intacto; quem paga é o evento raro que produz a evidência.
///
/// Relê o arquivo antes de somar (não o cache): dois daemons per-project ou
/// duas sessões no mesmo projeto somariam sobre uma cópia velha e uma das duas
/// contagens sumiria. Continua havendo uma janela de corrida entre a leitura e
/// a escrita — aceitável para um contador de evidência, e declarada aqui em vez
/// de descoberta depois como número que não bate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArmAxis {
    /// A apresentação entregou uma rota.
    Offered,
    /// O modelo tomou a rota (o canal).
    Followed,
    /// A rota tomada fundiu round-trips (a economia).
    Economical,
}

fn bump_arm(project_root: &Path, mode: &str, eixo: ArmAxis) {
    let Some(i) = arm_index(mode) else { return };
    let mut counts = read_arm_file(project_root);
    // Escritor ÚNICO da contagem do braço. A evidência DURÁVEL (o arquivo, que
    // sobrevive ao restart e alimenta a política) e a VOLÁTIL (os átomos que
    // `gate-metrics -j` publica) são duas VISTAS DA MESMA decisão — não dois
    // registros independentes. Até 26/08 elas eram mantidas por dois call
    // sites adjacentes: funcionava por vizinhança, não por construção, e um
    // terceiro ponto de oferta que chamasse só um dos dois faria as vistas
    // divergirem em silêncio (a política aprendendo de um número que o
    // operador não vê em `gate-metrics`, ou o inverso). É a mesma regra que a
    // docstring de `record_route_offer` já impunha ao campo `route`, aplicada
    // ao vizinho que ela não cobria. Provado por
    // `bump_arm_mantem_duravel_e_volatil_em_sincronia`.
    match eixo {
        ArmAxis::Offered => {
            counts[i].0 = counts[i].0.saturating_add(1);
            touring_foundation::gate_metrics_snapshot::record_code_mode_arm_offered(mode);
        }
        ArmAxis::Followed => {
            counts[i].1 = counts[i].1.saturating_add(1);
            touring_foundation::gate_metrics_snapshot::record_code_mode_arm_followed(mode);
        }
        // Economia não tem contador atômico correspondente: `gate-metrics`
        // publica canal (oferecida/tomada), e a economia é derivada no KPI a
        // partir da forma do programa. Sem par volátil, nada a sincronizar.
        ArmAxis::Economical => counts[i].2 = counts[i].2.saturating_add(1),
    }
    write_arm_file(project_root, &counts);
    arm_cache().insert(project_root.display().to_string(), counts);
}

/// P2 (decisão (b), 26/08/2026) — a política só age quando ARMADA.
///
/// Default-OFF, como o `f7_actuator_armed`: sem a env, nenhum sinal é lido e o
/// comportamento é byte-idêntico ao de antes. Armar é decisão humana, e é o
/// gate de promoção que o survey MSR exige além da fronteira do verificador —
/// aqui o sinal (o modelo seguiu a rota?) é produzido pelo próprio sistema
/// dirigido, então autonomia sem humano degradaria com a iteração.
fn code_mode_arm_armed() -> bool {
    std::env::var("TOURING_CODE_MODE_ARM_ARMED").is_ok_and(|v| v != "0")
}

/// A precedência final, PURA: declaração humana > política armada > default.
///
/// Extraída de `code_mode_presentation` porque a invariante que mais importa
/// aqui — "a política nunca sobrescreve o que o humano declarou" — só era
/// testável mutando `TOURING_CODE_MODE_ARM_ARMED` no processo. E a var passou a
/// ser lida também pelo caminho da fusão, então essa mutação vazava para testes
/// concorrentes: `turn_gate_ignora_o_que_nao_e_fanout` falhava em paralelo e
/// passava com `--test-threads=1` (a assinatura de estado global). Um predicado
/// puro é mais fácil de testar do que de contornar, e não tem vítima.
pub(crate) fn resolve_with_policy(
    declarado: Option<CodeModePresentation>,
    armada: bool,
    counts: [(u64, u64); 3],
) -> CodeModePresentation {
    if let Some(d) = declarado {
        return d;
    }
    if armada
        && let Some(escolhido) = arm_choice_from_counts(counts)
    {
        return escolhido;
    }
    CodeModePresentation::Both
}

/// Piso de amostra por braço — COMPARTILHADO com o KPI que reporta a evidência.
///
/// Uma cópia do número em cada lado divergiria no primeiro ajuste, e aí o
/// painel diria "pronto para promover" enquanto a política ainda se cala (ou o
/// contrário). O juiz e o medidor têm de ler o mesmo predicado.
pub(crate) const ARM_MIN_SAMPLE: u64 = 20;

/// Escolhe o braço pela evidência medida — ou `None` quando ela é fina.
///
/// Puro, para ser mais fácil de testar do que de contornar. Duas regras:
///
/// * **Piso de amostra** (`ARM_MIN_SAMPLE`): abaixo dele "a melhor taxa" é
///   ruído amostral, e promover ruído é exatamente o que `default-OFF +
///   promoção medida` existe para impedir.
/// * **Comparação exige ao menos DOIS braços** com amostra. Um único braço
///   acima do piso não é uma escolha: é o único que existe, e "escolher" o
///   único observado seria confirmar a configuração vigente chamando isso de
///   aprendizado.
///
/// Exploração — oferecer deliberadamente um braço sub-amostrado para aprender
/// sobre ele — NÃO está aqui de propósito: significaria degradar a
/// apresentação para colher dado, o que é outra decisão humana.
pub(crate) fn arm_choice_from_counts(counts: [(u64, u64); 3]) -> Option<CodeModePresentation> {
    let arms = [
        CodeModePresentation::Native,
        CodeModePresentation::Both,
        CodeModePresentation::Code,
    ];
    let mut elegiveis: Vec<(f64, CodeModePresentation)> = Vec::new();
    for (i, (offered, followed)) in counts.iter().enumerate() {
        if *offered >= ARM_MIN_SAMPLE {
            elegiveis.push((*followed as f64 / *offered as f64, arms[i]));
        }
    }
    if elegiveis.len() < 2 {
        return None;
    }
    elegiveis.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (melhor, vice) = (elegiveis[0], elegiveis[1]);
    // Margem mínima: uma diferença dentro do ruído NÃO é evidência. Sem ela,
    // `max_by` desempataria pela ordem do vetor e a apresentação mudaria por
    // acaso — trocar o gate de um projeto no desempate é o oposto de "promoção
    // por evidência medida". Empate ⇒ silêncio ⇒ o default declarado prevalece.
    const ARM_MIN_MARGIN: f64 = 0.05;
    if melhor.0 - vice.0 < ARM_MIN_MARGIN {
        return None;
    }
    Some(melhor.1)
}

/// Reivindica a rota pendente **apenas quando há veredito** — o par
/// (braço, recompensa) pronto para o depósito.
///
/// Classificar antes de reivindicar não é preferência de estilo: uma oferta
/// reivindicada some. Com a ordem invertida, o primeiro PostToolUse de QUALQUER
/// ferramenta — um `Read`, que nem carrega comando — consumia a oferta sem
/// veredito, e o `touring run` logo em seguida já não achava nada para creditar.
/// Deixar a regra no chamador a perderia no segundo chamador; aqui ela é
/// estrutural.
pub fn claim_route_reward(
    project_root: &Path,
    payload: &Value,
    cmd: &str,
) -> Option<(RouteOffer, f64)> {
    let value = classify_route_outcome(cmd)?;
    let offer = take_route_offer(project_root, payload)?;
    if value > 0.0 {
        bump_arm(project_root, &offer.mode, ArmAxis::Followed);
        // 1.0 é o programa que FUNDIU; 0.5 é a casca sobre uma chamada só.
        if value >= 1.0 {
            bump_arm(project_root, &offer.mode, ArmAxis::Economical);
        }
    }
    Some((offer, value))
}


/// W2 S-2.1 — o gate de rajada. `Some(resposta)` curto-circuita; `None` deixa
/// o fluxo (inclusive o advisory legado da 3ª busca) seguir.
/// Fração de tokens que dois comandos compartilham (Jaccard sobre o multiconjunto
/// de tokens), em `[0.0, 1.0]`.
///
/// Calibração proposta por Gabriel (26/08/2026): *"se o comando repetir pelo
/// menos 50% do comando anterior"*. Preenche um buraco real entre os gates —
/// o G6 pega só o byte-idêntico, o G7 só a re-inspeção do MESMO arquivo, e o G1
/// só ao acumular N da mesma classe. `grep X a.rs` seguido de `grep X b.rs` é a
/// assinatura canônica do fan-out serial e não era pego por nenhum deles.
///
/// Jaccard sobre tokens, e não prefixo comum: `sed -n 1,20p f` vs
/// `sed -n 40,60p f` compartilham quase tudo mas divergem cedo no texto, e um
/// prefixo os julgaria distintos. A ordem também não deve importar — o que
/// interessa é quanto do trabalho se repete.
pub(crate) fn command_similarity(a: &str, b: &str) -> f64 {
    let toks = |c: &str| -> std::collections::BTreeSet<String> {
        c.split_whitespace().map(str::to_string).collect()
    };
    let (ta, tb) = (toks(a), toks(b));
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f64;
    let uniao = ta.union(&tb).count() as f64;
    inter / uniao
}

/// Limiar da calibração por similaridade (Gabriel, 26/08/2026).
pub(crate) const G1_SIMILARITY_AT: f64 = 0.5;

fn burst_gate(project_root: &Path, session: &str, cmd: &str) -> Option<String> {
    use crate::shared::gate_metrics::{
        GateEvent, GateId, g1_live_precision, record_g1_continuation, record_gate_event,
    };
    let class = scan_class_of(cmd);
    // S-2.3: a primeira chamada pós-deny fecha o continuation-check. Seguir o
    // REMÉDIO (`touring run`) conta como acerto E como followed: converter a
    // rajada é o melhor desfecho do gate — contá-lo como "other" demoveria um
    // gate funcionando (visto vivo no primeiríssimo deny do G1, 2026-08-25).
    if let Some(denied_class) = pending_g1().remove(session) {
        let seguiu_remedio = cmd.contains("touring run");
        if seguiu_remedio {
            record_gate_event(GateId::G1, GateEvent::Followed);
        }
        record_g1_continuation(seguiu_remedio || class == Some(denied_class.as_str()));
    }
    let class = class?;
    let key = burst_key(project_root, class);
    let epoca_atual = mutation_epoch()
        .get(&project_root.display().to_string())
        .unwrap_or(0);
    let (count, mut cmds, epoca_anterior) =
        burst_ledger().get(&key).unwrap_or((0, Vec::new(), epoca_atual));
    let count = count.saturating_add(1);
    // O comando entra INTEIRO. Quem decide o que cabe é `fuse_burst_program`,
    // na renderização, descartando comando inteiro — nunca cortando um pela
    // metade (ver a origem documentada lá).
    if cmds.len() < 8 && cmd.chars().count() <= G1_BODY_BUDGET {
        cmds.push(cmd.to_string());
    }
    burst_ledger().insert(key, (count, cmds.clone(), epoca_atual));
    // Calibração por SIMILARIDADE: o G1 esperava `G1_DENY_AT` chamadas da mesma
    // classe. Uma variação pequena da chamada anterior já é a mesma inspeção
    // repetida — não é preciso esperar a quarta para saber que a rajada é uma
    // só. Dispara na SEGUNDA quando ≥ 50% dos tokens se repetem.
    let similaridade = if cmds.len() >= 2 && epoca_anterior == epoca_atual {
        command_similarity(&cmds[cmds.len() - 2], cmd)
    } else {
        0.0
    };
    let quase_igual = similaridade >= G1_SIMILARITY_AT;
    if count < G1_DENY_AT && !quase_igual {
        return None;
    }
    if code_gates_disabled() {
        record_gate_event(GateId::G1, GateEvent::Bypassed);
        return None;
    }
    // O remédio é o histórico REAL da rajada como corpo do programa — nunca um
    // template (injection-density), e nunca um comando pela metade.
    let (corpo, omitidos) = fuse_burst_program(&cmds);
    let programa = format!("touring run --lang bash --code '{corpo}'");
    // A omissão é DECLARADA: um remédio silenciosamente incompleto é a mesma
    // família de defeito que o `--brief` que reporta `truncated: false`.
    let nota_omissao = if omitidos > 0 {
        format!(
            "\n  ({omitidos} comando(s) da rajada não couberam no orçamento de \
             {G1_BODY_BUDGET} chars e ficaram DE FORA do programa — rode-os à parte.)"
        )
    } else {
        String::new()
    };
    if !g1_should_deny(g1_live_precision()) {
        // Autodemovido por dado vivo (S-2.3): a tese continua falsificável.
        record_gate_event(GateId::G1, GateEvent::Emitted);
        return Some(
            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "additionalContext": format!(
                        "[G1 rajada — AUTODEMOVIDO a advisory: precisão viva abaixo de \
                         {G1_DEMOTE_FLOOR}] {count}ª inspeção `{class}` na janela. A rajada \
                         acumulada já é o programa:\n  {programa}{nota_omissao}"
                    ),
                }
            })
            .to_string(),
        );
    }
    record_gate_event(GateId::G1, GateEvent::Denied);
    // um deny encerra o turno cego: a próxima chamada é a RESPOSTA do modelo
    // ao deny (o continuation-check A/B vive dela chegando ao burst_gate).
    pending_g1().insert(session.to_string(), class.to_string());
    // A razão DECLARADA tem de ser a do gatilho que disparou. Com a calibração
    // por similaridade, um deny na 2ª chamada exibindo "90% das rajadas ≥4"
    // justificaria o bloqueio por uma estatística que não se aplica a ele — o
    // anti-padrão D8 (o anúncio promete o que o executor não fez) dentro do
    // próprio gate.
    let motivo = if quase_igual && count < G1_DENY_AT {
        format!(
            "repete {:.0}% dos tokens da inspeção anterior da mesma classe, sem \
             mutação no meio — é a mesma varredura variada, não uma pergunta nova",
            similaridade * 100.0
        )
    } else {
        format!(
            "{count}ª inspeção da classe `{class}` em {CODE_MODE_WINDOW_SECS}s — 90% \
             das rajadas ≥{G1_DENY_AT} continuavam iguais (51 disparos/55 sessões)"
        )
    };
    Some(deny_response(format!(
        "[G1 rajada-de-inspeção] {motivo}. O acumulado JÁ é o programa; rode-o de uma \
         vez:\n  {programa}{nota_omissao}\nBypass por-comando: prefixe \
         {GATE_BYPASS_TOKEN} (reseta a janela e é contado)."
    )))
}

/// Resposta de deny do PreToolUse (contrato Claude Code): a razão carrega o
/// remédio derivado do comando REAL (injection-density — nunca placeholder).
fn deny_response(reason: String) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    })
    .to_string()
}

/// G8 — o laço de inspeção pura vira UMA varredura no sandbox.
///
/// Devolve o comando reescrito, ou `None` quando o laço não é candidato. Puro
/// (sem I/O) para ser exercitável em unit test — o predicado que decide precisa
/// ser mais fácil de testar do que de contornar.
///
/// Entra: `for`/`while` cujo corpo só inspeciona (grep/cat/head/tail/ls/find/
/// echo/wc/awk/sed/jq/stat/basename). Fica de fora, por construção, tudo que
/// tem efeito (kill, git, cargo, rm, mv, install, deploy), redireciona para
/// arquivo, usa heredoc (o corpo é dado, não programa) ou já é code mode.
pub(crate) fn loop_rewrite_candidate(cmd: &str) -> Option<String> {
    if std::env::var("TOURING_G8_REWRITE_DISABLED").map(|v| v == "1") == Ok(true) {
        return None;
    }
    let trimmed = cmd.trim();
    // Já é code mode / master CLI — nada a converter.
    if trimmed.contains("touring run") || trimmed.contains("touring exec") {
        return None;
    }
    // Heredoc: o corpo é DADO (mesma razão do G2), e o quoting não sobrevive.
    if trimmed.contains("<<") {
        return None;
    }
    // Aspas simples desbalanceadas quebrariam o `--code '...'`.
    if !trimmed.matches('\'').count().is_multiple_of(2) {
        return None;
    }
    // Precisa de um laço de verdade — `for x in` / `while `.
    let has_loop = trimmed.split(['\n', ';', '|', '&']).any(|part| {
        let p = part.trim_start();
        p.starts_with("for ") && p.contains(" in ") || p.starts_with("while ")
    });
    if !has_loop {
        return None;
    }
    // Qualquer verbo com efeito colateral tira o comando do escopo.
    const EFFECTFUL: &[&str] = &[
        "kill", "pkill", "git ", "cargo", "rm ", "mv ", "cp ", "install", "update-touring",
        "npm", "make ", "chmod", "chown", "mkdir", "touch ", "tee ", "curl", "wget", "ssh",
        "docker", "systemctl", "sudo", "python3 -c", "python3 -m", "pip", ">>", "> /",
    ];
    if EFFECTFUL.iter().any(|verb| trimmed.contains(verb)) {
        return None;
    }
    // Limite de tamanho: acima disso o comando reescrito polui mais do que ajuda.
    if trimmed.len() > 1200 {
        return None;
    }
    let escaped = trimmed.replace('\'', r"'\''");
    Some(format!("touring run --lang bash --code '{escaped}'"))
}

/// W1 — G2 + G6 num só passe. `Some(resposta)` curto-circuita `run`; `None`
/// segue para os classificadores. Corre ANTES do anti-spam: um deny repetido
/// jamais pode ser engolido pelo cache de sugestões. Recebe `project_root`
/// cru (não `HookRuntime`) para ser testável em unit tests.
pub(crate) fn code_mode_gates(
    project_root: &Path,
    session: &str,
    tool_name: &str,
    tool_input: &Value,
) -> Option<String> {
    use crate::shared::gate_metrics::{GateEvent, GateId, record_gate_event};
    let project = project_root.display().to_string();
    // Toda mutação avança a época do projeto (G6 nunca dispara através dela).
    if matches!(tool_name, "Edit" | "Write" | "NotebookEdit") {
        // G9 (N3a): a conversão do deny é o Edit com gates — a próxima
        // Edit/Write/NotebookEdit da sessão fecha o continuation-check.
        if pending_g9().remove(session).is_some() {
            record_gate_event(GateId::G9, GateEvent::Followed);
        }
        let epoch = mutation_epoch().get(&project).unwrap_or(0);
        mutation_epoch().insert(project.clone(), epoch + 1);
        // G5 (telemetria): a rajada de edits cresce; o fim dela é a primeira
        // ação Bash (onde o advisory único pode falar).
        let streak = g5_edit_streak().get(session).unwrap_or(0);
        g5_edit_streak().insert(session.to_string(), streak.saturating_add(1));
        // W3 S-3.1 — G3: Edit sem Read recente do arquivo, POR SESSÃO.
        // Write fica FORA do gate: criação não tem o que ler e overwrite já
        // exige Read no harness — o G3 negou um Write de criação vivo em
        // 24/08 (strategy doc). O conteúdo escrito é conhecido pela sessão,
        // então o Write REGISTRA o arquivo como lido (Edit seguinte passa).
        if let Some(fp) = tool_input.get("file_path").and_then(Value::as_str) {
            if tool_name == "Write" {
                g3_read_files().insert(g3_read_key(project_root, session, fp), ());
            } else if g3_read_files().get(&g3_read_key(project_root, session, fp)).is_some() {
                g3_streak().insert(session.to_string(), 0);
            } else if !code_gates_disabled() {
                let n = g3_streak().get(session).unwrap_or(0).saturating_add(1);
                g3_streak().insert(session.to_string(), n);
                // Aperto 29/08 (ordem de Gabriel): 1 advisory e o 2º seguido
                // nega — editar sem ler é a origem dos edit_string_not_found;
                // um aviso basta.
                if n >= 2 {
                    record_gate_event(GateId::G3, GateEvent::Denied);
                    return Some(deny_response(format!(
                        "[G3 edit-sem-read] {n}º Edit sem Read recente nesta sessão — os \
                         11 erros `edit_string_not_found` medidos nascem exatamente aqui. \
                         Leia primeiro: Read {fp} (o Read reseta o gate)."
                    )));
                }
                record_gate_event(GateId::G3, GateEvent::Emitted);
                return Some(advisory_response(format!(
                    "[G3 edit-sem-read] Edit de {fp} sem Read recente (advisory {n}/1 — \
                     o 2º seguido nega). Read {fp} zera o contador."
                )));
            }
        }
        // W6 S-6.3 — contrafactual em comentário adicionado (advisory, NUNCA
        // deny: prosa em comentário tem falso positivo real).
        let adicionado = tool_input
            .get("new_string")
            .or_else(|| tool_input.get("content"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if counterfactual_comment(adicionado) {
            crate::shared::gate_metrics::record_e3_counterfactual();
            return Some(advisory_response(
                "[E3 contrafactual-sem-endereço] comentário adicionado afirma o que \
                 ACONTECERIA sem citar um run_id — a classe do arm_marker de 24/08 \
                 (afirmação falsa nunca exercitada). Exercite (probe FACT= / touring \
                 run) e cite o run, ou remova o modal."
                    .to_string(),
            ));
        }
        return None;
    }
    if tool_name == "Read" {
        let fp = tool_input.get("file_path").and_then(Value::as_str)?;
        // M3 — o ledger (sessão, arquivo) do G3 já distingue arquivo novo de
        // releitura; um miss aqui É um arquivo distinto novo na janela.
        let g3k = g3_read_key(project_root, session, fp);
        let arquivo_novo = g3_read_files().get(&g3k).is_none();
        g3_read_files().insert(g3k, ());
        g3_streak().insert(session.to_string(), 0);
        if arquivo_novo {
            let distintos = m3_distinct_reads()
                .get(session)
                .unwrap_or(0)
                .saturating_add(1);
            m3_distinct_reads().insert(session.to_string(), distintos);
            // Exatamente no limiar (== e não >=): 1 advisory por janela.
            if distintos == M3_DELEGATION_AT && !code_gates_disabled() {
                crate::shared::gate_metrics::record_m3_delegation_advised();
                return Some(advisory_response(format!(
                    "[M3 delegação] {M3_DELEGATION_AT}º arquivo DISTINTO lido nesta janela — \
                     exploração desta largura paga o isolamento de contexto: delegue a 1-3 \
                     subagentes read-only (Explore/general-purpose), cada um com objetivo + \
                     formato de saída + fronteiras, e receba só as conclusões (50 arquivos \
                     lidos voltam como 3 achados). Leitura larga paraleliza; ESCRITA fica \
                     serial no contexto principal — decisões paralelas conflitam. Cadeia \
                     dependente (passo 2 precisa do output integral do passo 1) também fica \
                     no contexto único. Advisory único por sessão/1h."
                )));
            }
        }
        // G4 (telemetria, nunca deny): leitura sem localizar antes.
        if g4_last_locate().get(session).is_none() {
            crate::shared::gate_metrics::record_g4_observed();
        }
        // W3 S-3.2 — G7: re-inspeção do MESMO alvo.
        let key = g7_key(project_root, fp);
        let n = g7_seen().get(&key).unwrap_or(0).saturating_add(1);
        g7_seen().insert(key, n);
        if !code_gates_disabled() {
            if n >= 3 {
                record_gate_event(GateId::G7, GateEvent::Denied);
                return Some(deny_response(format!(
                    "[G7 re-inspeção] {n}ª leitura de {fp} na janela — reler é o sinal \
                     mais direto de programa-faltando (campeões medidos: adw.py 39×, \
                     cli_suggester.rs 32×). Varra de uma vez (R1): touring run --lang \
                     python --args '[\"{fp}\"]' --file <r1_varredura_agregado.py> — \
                     esqueletos: touring memory query \"#kind:snippet \
                     #process:code-mode\". Um `touring run` citando {fp} reseta o gate."
                )));
            }
            if n == 2 {
                record_gate_event(GateId::G7, GateEvent::Emitted);
                return Some(advisory_response(format!(
                    "[G7 re-inspeção] 2ª leitura de {fp} na janela — programa-faltando? \
                     R1 instanciado: touring run --lang python --args '[\"{fp}\"]' \
                     --file r1_varredura_agregado.py (a 3ª leitura nega)."
                )));
            }
        }
        return None;
    }
    if matches!(tool_name, "Grep" | "Glob") {
        g4_last_locate().insert(session.to_string(), ());
        return None;
    }
    if tool_name != "Bash" {
        return None;
    }
    let cmd = tool_input.get("command").and_then(Value::as_str)?;
    // G7 reset: um `touring run` citando um caminho zera a contagem daquele alvo.
    if cmd.contains("touring run") {
        for token in cmd.split_whitespace().filter(|t| t.contains('/')) {
            let limpo = token.trim_matches(|c: char| "'\"[]{},".contains(c));
            g7_seen().invalidate(&g7_key(project_root, limpo));
        }
    }
    // G5 (telemetria): a primeira ação Bash encerra a rajada de edits; se ela
    // tinha >=3 e este comando não é validação, UM advisory (nunca durante).
    let g5 = g5_edit_streak().get(session).unwrap_or(0);
    if g5 > 0 {
        g5_edit_streak().insert(session.to_string(), 0);
        const VALIDA: &[&str] = &[
            "cargo check", "cargo test", "cargo clippy", "pytest", "touring e2e",
            "npm test", "adw lint", "adw test",
        ];
        if g5 >= 3 && !VALIDA.iter().any(|m| cmd.contains(m)) {
            crate::shared::gate_metrics::record_g5_observed();
            return Some(advisory_response(format!(
                "[G5 telemetria] rajada de {g5} edits terminou sem build/teste — P9 \
                 medido em 17%. Valide agora (cargo check/test no crate tocado); a \
                 regressão silenciosa nasce aqui."
            )));
        }
    }
    // followed (S-1.4): houve deny de G2 nesta sessão e o comando agora traz
    // pipefail — a conversão canônica foi adotada.
    if pending_g2().remove(session).is_some() && cmd.contains("pipefail") {
        record_gate_event(GateId::G2, GateEvent::Followed);
    }
    // G11 — a rota sancionada devolve o crédito ANTES de qualquer cobrança, para
    // que um turno que alterna programa e bypass nunca acumule dívida.
    if is_sanctioned_route(cmd) {
        credit_sanctioned_route(project_root);
    } else if !cmd.contains(GATE_BYPASS_TOKEN) {
        break_strict_run(project_root);
    }
    if cmd.contains(GATE_BYPASS_TOKEN) {
        // O orçamento é cobrado ANTES da isenção: o token continua legítimo, mas
        // a SÉRIE passa a custar. O deny do G11 não é bypassável de propósito —
        // um bypass que se auto-libera não é orçamento; a saída é o kill switch
        // humano no env do daemon, que exige decisão fora do turno.
        let budget = charge_bypass(project_root);
        if !code_gates_disabled()
            && let Some(rule) = bypass_verdict(budget)
        {
            record_gate_event(GateId::G1, GateEvent::Emitted);
            return Some(deny_response(format!(
                "[G11 orçamento de bypass] {rule}: {} seguido(s), {} desde a última rota, janela {}s. \
                 O token é por-comando, não uma rota. Use a rota sancionada — funda a \
                 família numa varredura única: `touring run --lang bash --file <script>` \
                 — e o crédito volta inteiro. Sem saída por prefixo: o kill switch é \
                 humano (`TOURING_CODE_GATES_DISABLED=1` no env do daemon + restart).",
                budget.strict_run, budget.since_route, G11_WINDOW_SECS
            )));
        }
        if exit_code_through_pipe(cmd) {
            record_gate_event(GateId::G2, GateEvent::Bypassed);
        }
        if is_inline_blind_edit(cmd) {
            record_gate_event(GateId::G9, GateEvent::Bypassed);
        }
        if exec_class_of(cmd).is_some() {
            record_gate_event(GateId::G10, GateEvent::Bypassed);
        }
        // G1: o bypass consciente reseta a janela da classe (S-2.2) — a
        // exploração legítima recomeça do zero, e o evento fica contado.
        if let Some(class) = scan_class_of(cmd) {
            let key = burst_key(project_root, class);
            if matches!(burst_ledger().get(&key), Some((n, _, _)) if n >= G1_DENY_AT - 1) {
                record_gate_event(GateId::G1, GateEvent::Bypassed);
            }
            burst_ledger().invalidate(&key);
        }
        return None;
    }
    // G2 — heredoc é DADO (o corpo não executa como pipeline DESTE shell);
    // pular evita negar a escrita de um teste que apenas CONTÉM o padrão.
    if !cmd.contains("<<") && exit_code_through_pipe(cmd) {
        if code_gates_disabled() {
            record_gate_event(GateId::G2, GateEvent::Bypassed);
            return None;
        }
        // S-8.6 (spike SUPORTADO, provado vivo 2026-08-25: o rewrite A4
        // `cat`→highlight trocou um comando real desta sessão): o Claude Code
        // honra `updatedInput` de hook de settings.json. O G2 então CORRIGE em
        // vez de negar — zero fricção, correção garantida pelo executor (o
        // teto da afordância). `Followed` é registrado junto com `Emitted`
        // porque a adoção é do executor, não uma esperança.
        if std::env::var("TOURING_G2_REWRITE_DISABLED").map(|v| v == "1") != Ok(true) {
            record_gate_event(GateId::G2, GateEvent::Emitted);
            record_gate_event(GateId::G2, GateEvent::Followed);
            return Some(
                serde_json::json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "allow",
                        "permissionDecisionReason":
                            "[G2 exit-code-through-pipe → REESCRITO] `$?` após pipe \
                             leria o status do último estágio; `set -o pipefail; ` \
                             prefixado (spike S-8.6: updatedInput honrado).",
                        "updatedInput": { "command": format!("set -o pipefail; {cmd}") },
                    }
                })
                .to_string(),
            );
        }
        record_gate_event(GateId::G2, GateEvent::Denied);
        pending_g2().insert(session.to_string(), ());
        let remedy = if cmd.len() <= 1500 {
            format!("set -o pipefail; {cmd}")
        } else {
            "set -o pipefail; <o mesmo comando, verbatim>".to_string()
        };
        return Some(deny_response(format!(
            "[G2 exit-code-through-pipe] `$?` depois de um pipe lê o status do ÚLTIMO \
             estágio (tail/jq), não do programa medido — 68 casos na simulação de 55 \
             sessões; 3 cometidos pela própria sessão que desenhou este gate. \
             Reexecute exatamente:\n  {remedy}\nOu remova o pipe e leia o exit \
             direto. Bypass por-comando: prefixe {GATE_BYPASS_TOKEN} (contado como bypassed)."
        )));
    }
    // G9 (N3a, 2026-08-26) — escrita cega inline negada com a rota derivada:
    // o advisory Pattern 5 falava e o comando executava igual (advisory ≠
    // gate — adoção medida de um não transfere para o outro). Heredoc é
    // dado (o corpo não executa neste shell). ANTES do G8: um laço com
    // `sed -i` é negado aqui, não reescrito para o sandbox.
    if !cmd.contains("<<") && is_inline_blind_edit(cmd) {
        if code_gates_disabled() {
            record_gate_event(GateId::G9, GateEvent::Bypassed);
            return None;
        }
        record_gate_event(GateId::G9, GateEvent::Denied);
        pending_g9().insert(session.to_string(), ());
        let target = inline_edit_target(cmd).unwrap_or_else(|| "<o arquivo>".to_string());
        let cmd_remedio = if cmd.len() <= 1500 {
            cmd.replace('\'', r"'\''") // aspas escapadas — a rota cola no shell (lição G8)
        } else {
            "<o mesmo comando, verbatim>".to_string()
        };
        return Some(deny_response(format!(
            "[G9 escrita-cega-inline] `sed -i`/`awk -i inplace`/`perl -pi` editam às \
             cegas — zero dos 17 gates do Edit (VGP, blast_radius, TDG, snapshot \
             atômico, gotcha). Rotas:\n  \
             1 arquivo:   Edit tool em `{target}` (old_string → new_string, gates completos)\n  \
             mesmo comando, sandbox: touring run --lang bash --code '{cmd_remedio}'\n  \
             estrutural (N arquivos): touring ast grep {target} '<pattern>' --rewrite '<replacement>' \
             (ast-grep — metavars $VAR, NÃO regex de sed)\n  \
             Bypass por-comando: prefixe {GATE_BYPASS_TOKEN} (contado como bypassed)."
        )));
    }
    // G8 — laço de INSPEÇÃO reescrito para o sandbox (2026-08-25).
    //
    // Medido no transcript desta própria sessão (1433 tool calls): das 453
    // oportunidades de code mode, as outras classes ou já estão cobertas ou
    // não compensam. Apertar o limiar da rajada é fricção sem ganho (a mediana
    // de rajada é 1); converter grep para o índice serviria a ~3% (56% dos
    // greps são regex estrutural, que o índice não responde). Os laços, não:
    // 44 casos reais em 47 detectados (94% de precisão), e o programa está
    // ESCRITO no comando — foi decidido inteiro e partido em N round-trips.
    //
    // O remédio é o do G2, que se provou vivo: REESCREVER, não negar. Só
    // laços de inspeção pura entram: o sandbox aplica capabilities por
    // comando — `run-1787620774532-346193` registrou "X6 denied the subprocess
    // capability 'echo' under profile 'Sandboxed'" —, então um laço que mata
    // processo, faz git ou compila fica de fora por construção.
    if let Some(rewritten) = loop_rewrite_candidate(cmd) {
        if code_gates_disabled() {
            record_gate_event(GateId::G8, GateEvent::Bypassed);
        } else {
            record_gate_event(GateId::G8, GateEvent::Emitted);
            record_gate_event(GateId::G8, GateEvent::Followed);
            return Some(
                serde_json::json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "allow",
                        "permissionDecisionReason":
                            "[G8 laço-de-inspeção → REESCRITO] o laço já É o programa; \
                             uma varredura no sandbox devolve só o agregado (Anthropic \
                             CodeAct). Kill switch humano: TOURING_G8_REWRITE_DISABLED=1.",
                        "updatedInput": { "command": rewritten },
                    }
                })
                .to_string(),
            );
        }
    }
    // S5 (29/08) — o par write→run: `cat >`/`tee` de um script registra o
    // alvo; a execução do MESMO path na janela conta um par. Nasceu do turno
    // de 60 Bash do `analise` (28 pares, todos invisíveis: a escrita é T4
    // sancionada e a execução avulsa não acumulava) — o G-turno só falou no
    // Stop, com o custo já pago. Aqui o colapso acontece DURANTE o turno.
    if let Some(path) = script_write_target(cmd) {
        written_scripts_ledger().insert(write_run_key(project_root, session, &path), ());
    }
    if let Some(deny) = write_run_pair_gate(project_root, session, cmd) {
        return Some(deny);
    }
    // G10 (S4, 2026-08-26) — rajada de execução homogênea (R4): N seriadas do
    // mesmo interpretador com 0 `touring run` na janela. O laço desenrolado
    // vira 1 programa (R9). Um `touring run` entre elas zera a contagem — a
    // condição "0 run" é a prova de que a rota já foi oferecida e ignorada.
    // DEPOIS do G8: um laço escrito é reescrito lá; aqui é a rajada serial.
    if let Some(class) = exec_class_of(cmd) {
        if class == "python-inline" {
            // telemetria S5: cada inline visto conta — a calibração segue
            // medindo mesmo com a classe agora dentro da rajada.
            crate::shared::gate_metrics::record_exec_heredoc_inline_seen();
        }
        let key = exec_burst_key(project_root, session, class);
        let (n, mut cmds) = exec_burst_ledger().get(&key).unwrap_or_default();
        let n = n + 1;
        if cmds.len() < 16 {
            cmds.push(cmd.to_string());
        }
        if n < EXEC_BURST_DENY_AT {
            exec_burst_ledger().insert(key, (n, cmds));
        } else if code_gates_disabled() {
            record_gate_event(GateId::G10, GateEvent::Bypassed);
            return None;
        } else {
            record_gate_event(GateId::G10, GateEvent::Denied);
            pending_g10().insert(session.to_string(), ());
            // zera para a próxima rajada — um deny por lote, nunca fadiga
            exec_burst_ledger().invalidate(&key);
            // python-inline tem remédio 1:1 (o corpo do PRÓPRIO comando); as
            // demais classes fundem a rajada acumulada via R9.
            let programa = if class == "python-inline" {
                python_inline_remedy(cmd)
            } else {
                r9_exec_program(&cmds)
            };
            // S6 — a rajada real consulta o portfólio: quando há um programa
            // que já funcionou para este trabalho, o deny entrega os dois
            // (o esqueleto agregado + o prior art instanciado).
            let prior = portfolio_remedy_for_burst(class, &cmds).unwrap_or_default();
            return Some(deny_response(format!(
                "[G10 exec-burst] {n}ª chamada seriada do executor `{class}` (0 `touring run` \
                 na janela de {}s) — a rajada desenrolada JÁ é o programa; rode-o de uma vez:\n  \
                 {programa}\nOs comandos rodam verbatim, sequenciais e na mesma ordem \
                 (denies subprocess-only são advisory em `--lang bash` — o sandbox contém o \
                 filesystem); veredito por comando na saída, íntegra no spill.{prior} \
                 Bypass por-comando: prefixe {GATE_BYPASS_TOKEN} (contado como bypassed).",
                EXEC_BURST_WINDOW_SECS
            )));
        }
    } else if cmd.contains("touring run") || cmd.contains("touring exec") {
        if pending_g10().remove(session).is_some() {
            record_gate_event(GateId::G10, GateEvent::Followed);
        }
        for class in ["python", "pytest", "python-inline"] {
            exec_burst_ledger().invalidate(&exec_burst_key(project_root, session, class));
        }
        // S5 — a rota seguida zera o par write→run junto (mesma razão do S3
        // abaixo: sem isto o próximo script legítimo herdaria uma contagem de
        // um loop que o modelo JÁ converteu em programa).
        write_run_pair_ledger().invalidate(&write_run_key(project_root, session, "\u{0}write-run-pares"));
        // S3 — a rota foi seguida: a janela de inspeção zera junto. Sem isto o
        // ledger seguiria contando uma rajada que o modelo JÁ converteu em
        // programa, e o próximo `grep` legítimo levaria um deny herdado de uma
        // rajada que não existe mais.
        for class in CODE_MODE_COLLAPSED_CLASSES {
            inspect_burst_ledger().invalidate(&inspect_burst_key(project_root, session, class));
        }
        inspect_burst_ledger()
            .invalidate(&inspect_burst_key(project_root, session, PY_INLINE_INSPECT_CLASS));
    }
    // S3 (27/08/2026) — o colapso do modo `code` deixou de ser POR CLASSE e
    // passou a ser POR RAJADA. A 1ª inspeção de uma classe na janela executa
    // intacta; a 2ª em diante volta como UM programa com as duas fundidas.
    //
    // O que mudou e por quê: o braço P2c negava na PRIMEIRA chamada de
    // `grep`/`cat`/`find`. Medindo 115 transcripts (`s3_burst_distribution.py`),
    // 22,5% dessas chamadas são isoladas — para elas o deny cobrava um
    // round-trip inteiro e devolvia a MESMA leitura, que é a definição de
    // taxar o caso comum. Os outros 77,5% estão em rajadas ≥2, e esses o
    // predicado continua pegando, agora com o programa fundido em vez do
    // comando único reescrito.
    //
    // O predicado também é o que finalmente ENTREGA a promessa que o T3-B fazia
    // e nunca cumpriu (`t3_turn_fused = 0`): "a 1ª executa intacta, as K−1
    // voltam como 1 programa". O T3-B pendurava isso no fechamento de turno, um
    // evento que este modelo de execução não produz; aqui é um contador com
    // TTL, que ocorre. Mutação/build seguem passando (scan_class_of só
    // reconhece inspeção), e um `touring run` na janela zera o ledger — a
    // contagem prova que a rota foi oferecida e não usada.
    let apresentacao = code_mode_presentation(project_root, cmd);
    // Aperto 29/08 (ordem de Gabriel): python-inline READ-ONLY é inspeção e
    // cai na MESMA rajada — usado como leitura, o interpretador ganhava 4
    // passes onde `cat` ganha 1 (o G10 só nega na 5ª/600s). A classificação é
    // pelo CORPO (deny-list conservadora): qualquer indício de escrita/rede/
    // subprocesso mantém o comando no G10 — falha na direção frouxa, nunca
    // roteia um escritor ao deny de inspeção.
    let inspect_class = scan_class_of(cmd)
        .filter(|c| CODE_MODE_COLLAPSED_CLASSES.contains(c))
        .or_else(|| python_inline_readonly_class(cmd));
    if apresentacao == CodeModePresentation::Code
        && let Some(class) = inspect_class
        && !code_gates_disabled()
    {
        let key = inspect_burst_key(project_root, session, class);
        let (n, mut cmds) = inspect_burst_ledger().get(&key).unwrap_or_default();
        let n = n + 1;
        if cmds.len() < 16 {
            cmds.push(cmd.to_string());
        }
        if n < INSPECT_BURST_DENY_AT {
            // A isolada passa em SILÊNCIO: um nudge aqui devolveria pela porta
            // dos fundos a fricção que o predicado acabou de tirar da frente.
            inspect_burst_ledger().insert(key, (n, cmds));
            crate::shared::gate_metrics::record_g1_inspect_first_passed();
            bump_durable_evidence(project_root, "s3", "first_passed");
        } else {
            // python-inline funde os CORPOS (o remédio 1:1 por comando já
            // existia; a rajada junta-os na ordem); as classes shell seguem
            // no R9 via fuse_burst_program.
            let (programa, omitidos, lang) = if class == PY_INLINE_INSPECT_CLASS {
                let (p, o) = python_inline_burst_program(&cmds);
                (p, o, "python")
            } else {
                let (p, o) = fuse_burst_program(&cmds);
                (p, o, "bash")
            };
            // zera para a próxima rajada — um deny por lote, nunca fadiga
            // (mesma regra do G10, pela mesma razão).
            inspect_burst_ledger().invalidate(&key);
            record_route_offer(project_root, session, apresentacao);
            record_gate_event(GateId::G1, GateEvent::Denied);
            crate::shared::gate_metrics::record_g1_inspect_burst_denied();
            bump_durable_evidence(project_root, "s3", "denied");
            let nota_omissao = if omitidos > 0 {
                format!(
                    " ({omitidos} comando(s) não coube(ram) no orçamento e foram OMITIDOS \
                     — inteiro ou fora, jamais truncado: rode-os à parte)"
                )
            } else {
                String::new()
            };
            return Some(deny_response(format!(
                "[CODE MODE · rajada] {n}ª inspeção `{class}` em {}s — a 1ª já executou \
                 intacta; esta rajada É o programa, rode-o de uma vez:\n  \
                 touring run --lang {lang} --code '{programa}'\n{nota_omissao}\
                 Sub-chamadas DENTRO do programa não passam por aqui (elas nunca chegam \
                 ao PreToolUse), então a tabela inteira segue disponível lá dentro. \
                 Inspeção ISOLADA de qualquer classe PASSA — só a RAJADA colapsa \
                 (medido em 115 transcripts: 77,5% do volume de inspeção está em \
                 rajadas ≥2). Um `touring run` zera a janela. Escopo: [code_mode] mode \
                 em <projeto>/.touring/touring.toml. Relaxar POR-COMANDO: prefixar \
                 TOURING_CODE_MODE=native (exportar no shell NÃO chega ao hook — \
                 processos irmãos). Bypass de todos os gates: {GATE_BYPASS_TOKEN}.",
                INSPECT_BURST_WINDOW_SECS
            )));
        }
    }
    // `native` cala a indução inteira: o escopo declarou que não quer ser
    // empurrado, e um nudge que ele não pediu é o custo sem a contrapartida.
    if apresentacao == CodeModePresentation::Native {
        return None;
    }
    // G6 — repetição byte-idêntica dentro da janela TTL, sem mutação no meio.
    // ANTES do T3-B: retry cego byte-idêntico é o sinal mais específico e o G6
    // o consome inteiro (advisory → deny); o que passa daqui é inspeção NOVA.
    if !g6_allowlisted(cmd) {
        let h = input_hash(project_root, tool_name, tool_input);
        let epoch = mutation_epoch().get(&project).unwrap_or(0);
        match g6_seen().get(&h) {
            None => g6_seen().insert(h, (0, epoch)),
            Some((repeats, seen_epoch)) if seen_epoch == epoch => {
                if code_gates_disabled() {
                    record_gate_event(GateId::G6, GateEvent::Bypassed);
                    return None;
                }
                g6_seen().insert(h, (repeats + 1, epoch));
                if repeats == 0 {
                    record_gate_event(GateId::G6, GateEvent::Emitted);
                    return Some(
                        serde_json::json!({
                            "hookSpecificOutput": {
                                "hookEventName": "PreToolUse",
                                "additionalContext": format!(
                                    "[G6 redundant-exact-call] comando byte-idêntico há \
                                     <{SUGGESTION_TTL_SECS}s sem nenhuma mutação no meio — o \
                                     resultado já está no seu contexto. Reuse-o, varie o \
                                     input, ou funda a família em 1 `touring run`. A PRÓXIMA \
                                     repetição idêntica será negada."
                                ),
                            }
                        })
                        .to_string(),
                    );
                }
                record_gate_event(GateId::G6, GateEvent::Denied);
                // um deny encerra o turno cego: a próxima chamada é a RESPOSTA
                // do modelo ao deny, não continuação de um batch paralelo.
                let inicio: String = cmd.chars().take(200).collect();
                return Some(deny_response(format!(
                    "[G6 redundant-exact-call] repetição byte-idêntica em \
                     {SUGGESTION_TTL_SECS}s sem mutação no meio — retry cego (5 casos \
                     medidos). O resultado já está no contexto; reuse-o ou varie o \
                     input. Comando: {inicio}\nBypass por-comando: prefixe \
                     {GATE_BYPASS_TOKEN}."
                )));
            }
            // época mudou: a árvore mutou desde a primeira vista — fresh.
            Some((_, _)) => g6_seen().insert(h, (0, epoch)),
        }
    }

    // S10 (2026-08-27) — o T3-B foi REMOVIDO daqui. Medido ao vivo: 3 chamadas
    // Bash de classes distintas no mesmo turno deram `t3_turn_first_passed = 3`
    // e `t3_turn_fused = 0` — cada uma era "a primeira", porque o PostToolUse
    // fecha o turno entre elas. O gate mantinha estado por sessão e NUNCA
    // decidia. O predicado de rajada do S3 entrega o que ele prometia, com uma
    // janela de TEMPO (300s) em vez de um turno que sempre fecha.
    // G1 — rajada de inspeções atômicas da MESMA classe (W2 teeth): decidida
    // depois do G2 (o defeito de leitura vem antes do hábito) e antes do G6.
    if let Some(resp) = burst_gate(project_root, session, cmd) {
        return Some(resp);
    }
    None
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
    let first = resolved_verb(command).unwrap_or("");
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
fn eval_uptake(project_root: &Path, session: &str, tool_name: &str, tool_input: &Value) {
    if pending_suggestion().remove(session).is_some()
        && action_is_touring_redirect(tool_name, tool_input)
    {
        crate::shared::gate_metrics::record_suggestion_followed();
    }
    if pending_pillar().remove(session).is_some() && action_followed_pillar(tool_name, tool_input) {
        crate::shared::gate_metrics::record_pillar_induction_followed();
        bump_durable_evidence(project_root, "pillar", "followed");
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
fn record_emission(project_root: &Path, context_len: usize, session: String, is_pillar: bool) {
    crate::shared::gate_metrics::record_enrichment_emitted(context_len);
    // F2: this emission is a redirect suggestion — count it and arm the uptake
    // measurement for this session's next action.
    crate::shared::gate_metrics::record_suggestion_emitted();
    pending_suggestion().insert(session.clone(), ());
    // Task #6: when this is the pillar layer's own nudge, count it and arm the
    // precise per-pillar follow-through (parallel cache so F2 is undisturbed).
    if is_pillar {
        crate::shared::gate_metrics::record_pillar_induction_emitted();
        bump_durable_evidence(project_root, "pillar", "emitted");
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
    eval_uptake(&rt.project_root, &session, tool_name, tool_input);

    // F3 adoption_ratio (doc §9, the mother coupling KPI): classify EVERY Bash
    // action — touring-canonical vs raw-shell antipattern — BEFORE the anti-spam /
    // no-classifier early returns, so the denominator sees all actions (the TTL
    // cache below would otherwise drop repeated identical antipatterns from the
    // count). Fail-open: pure classification + Relaxed atomics, no error path.
    record_adoption(tool_name, tool_input);

    // N5 — eixo da injeção nativa: mesma disciplina (antes dos gates, sem
    // early-return): cada PreToolUse é uma escolha no eixo.
    record_native_injection(tool_name, tool_input);

    // W1 — gates com executor (G2 deny + G6 escalada): decididos ANTES do
    // anti-spam, porque um deny repetido nunca pode ser engolido pelo cache.
    if let Some(resp) = code_mode_gates(&rt.project_root, &session, tool_name, tool_input) {
        return resp;
    }

    // TTL cache: anti-spam for identical (tool, input) pairs.
    let h = input_hash(&rt.project_root, tool_name, tool_input);
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
    if matches!(
        cluster_dedupe_gate(&rt.project_root, &classifier),
        ClusterDecision::Suppress
    ) {
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
    // Slice 3 Part 1: thread quality_score into the signature so HiComplexity
    // can trigger when no blast-radius signal is present.
    let sig = ActionSignature::from_pre_tool_with_cognitive(
        tool_name,
        tool_input,
        suggestion.enrichment.dependent_count,
        suggestion.enrichment.quality_score,
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

    // P1/S-1.1 — the same bytes twice in one session buy nothing the first time
    // did not. Collapse to a reference AFTER the block is fully assembled (the
    // `sig=` line and any lessons included), because it is the final text the
    // window pays for, not the template it came from.
    if let Some(reference) = repeat_reference(&session, &context) {
        context = reference;
    }

    // P3/S-3.3 — o orçamento do TURNO. O teto por chamada (`cila_budget_*`) nunca
    // limitou o custo real, porque um turno são muitas chamadas: 26,6 % da janela
    // em injeção, 6 650 B/turno medidos, com nenhuma chamada perto do próprio
    // teto. Estourado o orçamento, cai a racionalização e ficam as diretivas — a
    // ordem declarada em `turn_budget`, não uma escolha caso a caso.
    {
        use touring_hooks_shared::turn_budget::{
            charge_turn, directives_only, spent_this_turn, turn_budget,
        };
        let root = &rt.project_root;
        if spent_this_turn(root, &session) + context.len() > turn_budget()
            && let Some(cut) = directives_only(&context)
        {
            context = cut;
        }
        // Cobra o que a janela PAGOU, depois de qualquer elisão.
        charge_turn(root, &session, context.len());
    }

    // TR-2 — STR observability: record enrichment bytes only when context is
    // non-empty. `context.len()` is the UTF-8 byte length of additionalContext,
    // used as a token-count proxy for the Signal-to-Token Ratio metric. Runs
    // AFTER the dedup so the counter reports what the window actually paid.
    // Fail-open: the counter call is infallible (~1 ns, Relaxed atomic).
    if !context.is_empty() {
        record_emission(&rt.project_root, context.len(), session, is_pillar);
    }

    emit(tool_name, tool_input, &context)
}

/// Render the PreToolUse response — advisory by default, mirror-rewrite when
/// one is provably safe.
///
/// A4 (2026-08-07): the suggester's own rule says persuasion does not change
/// `U(a)`; affordance does. When a command has an EXACT mirror in the touring
/// surface, this returns `updatedInput` and the better call simply happens —
/// no nudge to ignore, no capability removed. Every other case is unchanged
/// advisory text, which is the honest answer whenever equivalence cannot be
/// proven. See [`crate::hook_rewrite`] for the bar a rewrite must clear.
fn emit(tool_name: &str, tool_input: &Value, context: &str) -> String {
    use crate::hook_rewrite::{Mode, rewrite_for, rewrite_response};
    if let Some(rw) = rewrite_for(Mode::from_env(), tool_name, tool_input) {
        crate::shared::gate_metrics::record_hook_rewrite_applied();
        return rewrite_response(&rw, context).to_string();
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

#[cfg(test)]
mod intent_codetag_tests {
    use super::intent_for_new_file;

    /// C3 (2026-09-02): the codetag line is a facet list, not a purpose. With
    /// it first, the intent sent to the portfolio was "#tags: kind:script
    /// purpose:diagnostic …" and the answer was an unrelated `cofre.py`; the
    /// docstring is the purpose and must win, shebang or codetag before it.
    #[test]
    fn codetag_and_shebang_never_become_the_intent() {
        let content = "#!/usr/bin/env python3\n# #tags: kind:script purpose:diagnostic domain:code-mode lang:python\n\"\"\"One-shot census of /tmp usage and code-mode adherence.\"\"\"\nimport os\n";
        let intent = intent_for_new_file("/tmp/claude-1000/s/scratchpad/diag_tmp.py", Some(content))
            .expect("docstring intent");
        assert!(intent.starts_with("One-shot census"), "got {intent}");
        assert!(!intent.contains("#tags:"), "got {intent}");
    }

    #[test]
    fn without_a_docstring_the_stem_words_remain_the_intent() {
        let content = "# #tags: kind:script lang:python\nprint(1)\n";
        assert_eq!(
            intent_for_new_file("scripts/generate_pdf_report.py", Some(content)).as_deref(),
            Some("generate pdf report")
        );
    }
}

/// P1/S-1.1 (2026-09-04) — dedup por CONTEÚDO.
///
/// Cada teste usa uma sessão própria: o ledger é um cache global de processo e
/// os testes correm em paralelo, então sessões partilhadas fariam um teste
/// decidir o resultado do outro.
#[cfg(test)]
mod content_dedup_tests {
    use super::*;

    /// Um bloco do tamanho REAL medido para o `touring-suggest` (média 1 399 B).
    const BLOCK: &str = "[TOURING SUGGEST · code-mode-loop · conf=0.95]\n  MUST    touring run --lang bash --code '<o programa fundido>'\n            // Code Mode without MCP — one call computes in the sandbox\n  MAY     touring run --lang python --orchestrate --code '<...>'\n  Reason: Explicit shell loop fans a per-item op across a set.\n  (cached 300s — set TOURING_SUGGESTER_DISABLED=1 to silence)\n  sig=outcome:bash:cat:plain";

    #[test]
    fn the_first_emission_passes_and_the_identical_second_becomes_a_reference() {
        let session = "dedup-primeira-e-segunda";
        assert_eq!(repeat_reference(session, BLOCK), None, "a 1a vez nunca elide");
        let second = repeat_reference(session, BLOCK).expect("a 2a e' repeticao");
        assert!(second.contains("repetido nesta sessão"), "{second}");
        assert!(
            second.len() < BLOCK.len(),
            "a referencia ({} B) tem de custar menos que o bloco ({} B)",
            second.len(),
            BLOCK.len()
        );
        // A elisão é AUDITÁVEL: diz quantos bytes deixaram de ser pagos.
        assert!(second.contains(&BLOCK.len().to_string()), "{second}");
    }

    /// O caso que a estreia deste dedup errou: `past-lessons` repete 86,4 % das
    /// vezes com bloco médio de 48 B, e uma referência mais longa que o bloco
    /// faria a PIOR família ficar pior. A aritmética decide.
    #[test]
    fn a_block_smaller_than_the_reference_is_never_collapsed() {
        let session = "dedup-bloco-minusculo";
        let tiny = "⚡ last fail: cd";
        assert_eq!(repeat_reference(session, tiny), None, "1a vez");
        assert_eq!(
            repeat_reference(session, tiny),
            None,
            "elidir 15 B com uma referencia de ~45 B seria pessimizacao"
        );
    }

    #[test]
    fn another_session_never_inherits_the_first_sessions_ledger() {
        // A sessão B nunca viu o bloco; elidir ali esconderia texto que aquele
        // contexto jamais carregou — absence with two causes, de novo.
        assert_eq!(repeat_reference("dedup-sessao-a", BLOCK), None);
        assert_eq!(repeat_reference("dedup-sessao-a", BLOCK).is_some(), true);
        assert_eq!(
            repeat_reference("dedup-sessao-b", BLOCK),
            None,
            "outra sessao paga a primeira copia normalmente"
        );
    }

    #[test]
    fn a_block_that_differs_by_one_byte_is_not_a_repeat() {
        let session = "dedup-um-byte";
        assert_eq!(repeat_reference(session, BLOCK), None);
        let quase = format!("{BLOCK} ");
        assert_eq!(
            repeat_reference(session, &quase),
            None,
            "so' o byte-identico e' repeticao; qualquer diferenca e' sinal novo"
        );
    }

    #[test]
    fn an_empty_context_is_never_recorded_as_something_said() {
        assert_eq!(repeat_reference("dedup-vazio", ""), None);
        assert_eq!(
            repeat_reference("dedup-vazio", ""),
            None,
            "nao ha' bloco vazio para referenciar"
        );
    }
}

/// G11 (2026-09-02) — o orçamento do bypass, com as duas contas separadas.
#[cfg(test)]
mod g11_bypass_budget_tests {
    use super::*;

    #[test]
    fn the_first_bypass_passes() {
        let b = BypassBudget { strict_run: 1, since_route: 1 };
        assert_eq!(bypass_verdict(b), None);
    }

    #[test]
    fn two_in_a_row_are_denied_by_the_strict_rule() {
        let b = BypassBudget { strict_run: 2, since_route: 2 };
        assert_eq!(bypass_verdict(b), Some("dois bypasses seguidos"));
    }

    /// O caso que a conta estrita NÃO pega: um comando neutro entre os bypasses.
    /// A estreia ao vivo negava este como "dois seguidos", que era falso — e um
    /// gate falante que nomeia a regra errada ensina a correção errada.
    #[test]
    fn an_interleaved_pair_is_not_a_strict_run() {
        let b = BypassBudget { strict_run: 1, since_route: 2 };
        assert_eq!(bypass_verdict(b), None, "dois espaçados ainda passam");
    }

    #[test]
    fn the_third_without_the_route_is_denied_even_when_spaced() {
        let b = BypassBudget { strict_run: 1, since_route: 3 };
        assert_eq!(bypass_verdict(b), Some("terceiro bypass sem usar a rota"));
    }

    #[test]
    fn the_sanctioned_route_is_recognised() {
        assert!(is_sanctioned_route("touring run --lang bash --file x.sh"));
        assert!(is_sanctioned_route("  touring run --lang python --code 'x'"));
        assert!(!is_sanctioned_route("touring index find X"));
        assert!(!is_sanctioned_route("grep -rn foo src/"));
    }

    #[test]
    fn a_neutral_command_breaks_only_the_strict_run() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        charge_bypass(root);
        break_strict_run(root);
        let after = charge_bypass(root);
        assert_eq!(
            after,
            BypassBudget { strict_run: 1, since_route: 2 },
            "o comando neutro zera a sequência estrita e preserva a conta da rota"
        );
        assert_eq!(bypass_verdict(after), None);
    }

    #[test]
    fn the_route_credits_both_counts_back() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        charge_bypass(root);
        charge_bypass(root);
        credit_sanctioned_route(root);
        let after = charge_bypass(root);
        assert_eq!(after, BypassBudget { strict_run: 1, since_route: 1 });
    }
}
