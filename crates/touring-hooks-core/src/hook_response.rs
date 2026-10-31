//! HookResponse — Testable alternative to `process::exit`.
//!
//! Extracted from hook_runtime.rs as part of Track A decomposition.
//! Provides 6 HookResponse variants for Claude Code hook protocol.
//!
//! ## D5: Compression Optimization
//! `to_json()` is called 45× in dispatch table builders. To avoid per-call
//! heap allocation from intermediate `serde_json::Value` trees, a
//! `LazyResponse` wrapper caches the serialized `String` behind an
//! `OnceLock`, eliminating repeated serialization of static responses
//! like `Allow` (always `{}`).

// HookResponse uses serde_json::Value for JSON serialization

use std::path::PathBuf;
use std::sync::OnceLock;

/// Serialization-optimized response wrapper.
///
/// Lazily serializes a `HookResponse` once via the existing `to_json()`
/// path and caches the resulting `String` in an `OnceLock`. Subsequent
/// calls to `as_str()` return a borrowed reference to the cached string,
/// eliminating repeated `serde_json::Value` tree construction for hot-path
/// responses like `Allow` (always `{}`).
#[derive(Debug)]
pub struct LazyResponse {
    inner: HookResponse,
    // Cached serialized string, populated on first `as_str()` call.
    cache: OnceLock<String>,
}

impl LazyResponse {
    /// Wrap a `HookResponse` for lazy serialization.
    pub fn new(inner: HookResponse) -> Self {
        Self {
            inner,
            cache: OnceLock::new(),
        }
    }

    /// Returns the serialized JSON as a `&str`.
    ///
    /// First call: serializes `self.inner` via the legacy `to_json()` and
    /// stores it in the cache. Subsequent calls return the cached string
    /// directly — zero re-serialization.
    pub fn as_str(&self) -> &str {
        if let Some(cached) = self.cache.get() {
            return cached.as_str();
        }
        // First call — serialize using the proven `to_json()` path, then cache.
        let owned = self.inner.to_json();
        let _ = self.cache.set(owned);
        // Unwrap is safe: we just set it above.
        self.cache
            .get()
            .expect("cache populated by set() above")
            .as_str()
    }

    /// Returns the raw bytes from the cache, or an empty slice if not yet cached.
    pub fn as_bytes(&self) -> &[u8] {
        self.cache.get().map(|s| s.as_bytes()).unwrap_or(&[])
    }

    /// Returns a reference to the inner `HookResponse`.
    pub fn inner(&self) -> &HookResponse {
        &self.inner
    }
}

impl Clone for LazyResponse {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            cache: OnceLock::new(),
        }
    }
}

impl PartialEq for LazyResponse {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for LazyResponse {}

impl HookResponse {
    /// Convert to `LazyResponse` for use in hot-path dispatch tables.
    ///
    /// Call this at registration time instead of `to_json()` to get
    /// a `LazyResponse` that avoids per-call serialization overhead.
    pub fn to_lazy(self) -> LazyResponse {
        LazyResponse::new(self)
    }
}

/// Response from a hook — testable alternative to `process::exit`.
///
/// Hooks can return this instead of calling the diverging `emit_*()` methods.
/// The binary entry point converts this to stdout + exit code.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum HookResponse {
    /// Allow the tool invocation (exit 0, no output).
    Allow,
    /// Inject context (exit 0, stdout JSON with additionalContext).
    Context {
        /// The context text injected into the model's prompt.
        context: String,
        /// Originating hook event name, if known.
        event_name: Option<String>,
    },
    /// Deny tool execution (PreToolUse only).
    /// Returns permissionDecision: "deny" with reason.
    Deny {
        /// Human-readable reason shown for the denial.
        reason: String,
        /// Optional additional context injected alongside the denial.
        context: Option<String>,
        /// Originating hook event name, if known.
        event_name: Option<String>,
    },
    /// Block after tool execution (PostToolUse).
    /// Returns top-level decision: "block" with reason.
    Block {
        /// Human-readable reason shown for the block.
        reason: String,
        /// Optional additional context injected alongside the block.
        context: Option<String>,
        /// Originating hook event name, if known.
        event_name: Option<String>,
    },
    /// Halt session entirely.
    /// Returns continue: false with stopReason.
    Halt {
        /// Reason reported as the session stop reason.
        reason: String,
    },
    /// Context injection with modified tool input.
    /// Returns additionalContext + updatedInput in hookSpecificOutput.
    /// Used by PreToolUse hooks to normalize or correct tool inputs.
    ContextWithUpdatedInput {
        /// The context text injected into the model's prompt.
        context: String,
        /// Originating hook event name, if known.
        event_name: Option<String>,
        /// The corrected tool input that replaces the original.
        updated_input: serde_json::Value,
    },
    /// Context injection with a replacement for the tool RESULT (PostToolUse only).
    /// Returns `updatedToolOutput` (+ `additionalContext` when `context` is non-empty)
    /// in hookSpecificOutput. Claude Code validates the value against the tool's own
    /// output schema and falls back to the original output when it does not match, so
    /// the producer must hand back the whole result object, not a bare string.
    ContextWithUpdatedToolOutput {
        /// Additional context string injected next to the result; empty = none.
        context: String,
        /// Originating hook event name, if known.
        event_name: Option<String>,
        /// The tool result object that replaces the original before the model sees it.
        updated_tool_output: serde_json::Value,
    },
}

impl HookResponse {
    /// Serialize this response to stdout and call `process::exit(0)`.
    ///
    /// This is the bridge between the testable `HookResponse` and the
    /// diverging behavior required by Claude Code hook protocol.
    ///
    /// `Allow` emits nothing — an empty hook response is signalled by empty
    /// stdout. Every other variant prints [`HookResponse::stdout_text`], which
    /// serializes byte-identically to [`HookResponse::to_json`]: one source of
    /// truth for the wire format, so the emit/to_json drift-bug class cannot exist.
    ///
    /// S14: the exit reason is recorded BEFORE the process exits (through the sink
    /// the runtime layer installs, see [`set_emit_reason_sink`]) — without it the
    /// hook trace kept the install default `handler-diverged` and a clean exit read
    /// like a crash.
    ///
    /// S-2 (27/10/2026): the answer goes through the injection policy
    /// ([`touring_foundation::injection_policy`]) on its way out, at the level of
    /// this process's environment; S-4p (26/10/2026): on the standalone route (a
    /// dedupe scope recorded by the client) it also goes through the epoch dedupe
    /// the daemon applies. With `TOURING_HOOK_INJECTION` unset and no scope
    /// recorded the bytes are exactly what they always were.
    pub fn emit(self) -> ! {
        if matches!(self, HookResponse::Allow) {
            note_emit_reason("emit-allow");
        } else {
            note_emit_reason("emit-json");
            let (level, sink) = touring_foundation::injection_policy::current();
            println!(
                "{}",
                self.stdout_text(level, sink.as_deref(), dedup_scope())
            );
        }
        std::process::exit(0);
    }

    /// The exact text [`HookResponse::emit`] writes to stdout: the serialization
    /// ([`HookResponse::to_json`]), the injection policy (`level`, recording the cut
    /// in the journal at `sink`) and the epoch dedupe (`scope`). Split from `emit` —
    /// which exits the process — to be testable.
    ///
    /// The policy runs BEFORE the dedupe: what it cut never reached the model, so it
    /// must not enter the epoch state. The site label is [`SITE_EMIT_RUNTIME`]
    /// (`emit-runtime`): it is the label the standalone emit funnel always wrote and
    /// the one the journals already hold — the type was unified around it.
    ///
    /// [`SITE_EMIT_RUNTIME`]: touring_foundation::injection_policy::SITE_EMIT_RUNTIME
    pub fn stdout_text(
        &self,
        level: touring_foundation::injection_policy::InjectionLevel,
        sink: Option<&std::path::Path>,
        scope: Option<&(PathBuf, String)>,
    ) -> String {
        let json = self.to_json();
        let json = touring_foundation::injection_policy::apply_at(
            level,
            touring_foundation::injection_policy::SITE_EMIT_RUNTIME,
            &json,
            sink,
        );
        dedup_with_scope(&json, scope)
    }

    /// Build a context response (injects additionalContext).
    pub fn context(context: impl Into<String>) -> Self {
        HookResponse::Context {
            context: context.into(),
            event_name: None,
        }
    }

    /// Build a context response with event name.
    pub fn context_with_event(context: impl Into<String>, event_name: impl Into<String>) -> Self {
        HookResponse::Context {
            context: context.into(),
            event_name: Some(event_name.into()),
        }
    }

    /// Build a deny response.
    pub fn deny(reason: impl Into<String>) -> Self {
        HookResponse::Deny {
            reason: reason.into(),
            context: None,
            event_name: None,
        }
    }

    /// Build a deny response with context.
    pub fn deny_with_context(reason: impl Into<String>, context: impl Into<String>) -> Self {
        HookResponse::Deny {
            reason: reason.into(),
            context: Some(context.into()),
            event_name: None,
        }
    }

    /// Build a block response.
    pub fn block(reason: impl Into<String>) -> Self {
        HookResponse::Block {
            reason: reason.into(),
            context: None,
            event_name: None,
        }
    }

    /// Build a halt response.
    pub fn halt(reason: impl Into<String>) -> Self {
        HookResponse::Halt {
            reason: reason.into(),
        }
    }

    /// Build a context-with-updated-input response.
    pub fn context_with_updated_input(
        context: impl Into<String>,
        updated_input: serde_json::Value,
    ) -> Self {
        HookResponse::ContextWithUpdatedInput {
            context: context.into(),
            event_name: None,
            updated_input,
        }
    }

    /// Build an allow response.
    pub fn allow() -> Self {
        HookResponse::Allow
    }

    /// Serialize to stdout JSON string for use in hook dispatch tables.
    ///
    /// Used by `build_dispatch_table()` — each handler returns `HookResponse`
    /// and the dispatch calls `.to_json()` to produce the final stdout line.
    pub fn to_json(&self) -> String {
        let value = match self {
            HookResponse::Allow => {
                serde_json::json!({})
            }
            HookResponse::Context {
                context,
                event_name,
            } => {
                let truncated = truncate_context(context);
                let mut hso = serde_json::json!({
                    "additionalContext": truncated,
                });
                if let Some(name) = event_name {
                    hso["hookEventName"] = serde_json::Value::String(name.clone());
                }
                serde_json::json!({
                    "hookSpecificOutput": hso,
                })
            }
            HookResponse::Deny {
                reason,
                context,
                event_name,
            } => {
                let mut hso = serde_json::json!({
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                });
                if let Some(ctx) = context {
                    hso["additionalContext"] = serde_json::Value::String(ctx.clone());
                }
                if let Some(name) = event_name {
                    hso["hookEventName"] = serde_json::Value::String(name.clone());
                }
                serde_json::json!({
                    "hookSpecificOutput": hso,
                })
            }
            HookResponse::Block {
                reason,
                context,
                event_name,
            } => {
                let mut hso = serde_json::json!({});
                if let Some(ctx) = context {
                    hso["additionalContext"] = serde_json::Value::String(ctx.clone());
                }
                if let Some(name) = event_name {
                    hso["hookEventName"] = serde_json::Value::String(name.clone());
                }
                serde_json::json!({
                    "decision": "block",
                    "reason": reason,
                    "hookSpecificOutput": hso,
                })
            }
            HookResponse::Halt { reason } => {
                serde_json::json!({
                    "continue": false,
                    "stopReason": reason,
                })
            }
            HookResponse::ContextWithUpdatedInput {
                context,
                event_name,
                updated_input,
            } => {
                let truncated = truncate_context(context);
                let mut hso = serde_json::json!({
                    "additionalContext": truncated,
                    "updatedInput": updated_input,
                });
                if let Some(name) = event_name {
                    hso["hookEventName"] = serde_json::Value::String(name.clone());
                }
                serde_json::json!({
                    "hookSpecificOutput": hso,
                })
            }
            HookResponse::ContextWithUpdatedToolOutput {
                context,
                event_name,
                updated_tool_output,
            } => updated_tool_output_value(context, event_name.as_deref(), updated_tool_output),
        };
        let mut value = value;
        stamp_invoking_event(&mut value);
        serde_json::to_string(&value).unwrap_or_default()
    }
}

/// Wire value of [`HookResponse::ContextWithUpdatedToolOutput`]: `updatedToolOutput`
/// always, `additionalContext` only when there is context to carry (an empty string
/// would put a blank reminder next to the result).
fn updated_tool_output_value(
    context: &str,
    event_name: Option<&str>,
    updated_tool_output: &serde_json::Value,
) -> serde_json::Value {
    let mut hso = serde_json::json!({
        "updatedToolOutput": updated_tool_output,
    });
    if !context.is_empty() {
        let truncated = truncate_context(context);
        hso["additionalContext"] = serde_json::Value::String(truncated);
    }
    if let Some(name) = event_name {
        hso["hookEventName"] = serde_json::Value::String(name.to_string());
    }
    serde_json::json!({
        "hookSpecificOutput": hso,
    })
}

/// S-4p (26/10/2026) — the dedupe scope of the STANDALONE route: (root, session).
///
/// The daemon dedupes every handler's output in `maybe_dedup_output`
/// (touring-dispatch/src/daemon.rs); the standalone route — daemon down, 3 s
/// timeout, `TOURING_NO_DAEMON` — printed without dedupe, and it is through it
/// that the Rust families (`session-contract` 95.8%, `similar-fail` 92.3%,
/// `pipefail-evidence` 90.5% of excess in the 400-session corpus) repeated when
/// the daemon was down. The `touring-hook` client records the scope ONCE, only on
/// entering the standalone route; the daemon process never sets it — the dedupe
/// of ITS answer already happened in the daemon, and deduping again would empty
/// the first emission (the remaining lines are already in the state) — so
/// [`HookResponse::emit`] dedupes EXCLUSIVELY on the standalone route. Unset ⇒
/// bytes identical to those before S-4p.
///
/// I-3 (31/10/2026): the state moved here with the unified `HookResponse`
/// (the `emit` that reads it lives on the canonical type); the runtime layer
/// re-exports [`set_dedup_scope`] / [`dedup_scope`] under their old paths.
static DEDUP_SCOPE: OnceLock<(PathBuf, String)> = OnceLock::new();

/// Record the standalone route's dedupe scope. The first call wins; an empty
/// session records nothing (no session means no per-epoch state file, and the
/// dedupe would have no key).
pub fn set_dedup_scope(root: PathBuf, session: &str) {
    if !session.is_empty() {
        let _ = DEDUP_SCOPE.set((root, session.to_string()));
    }
}

/// The recorded scope, if any — read by [`HookResponse::emit`] and by the
/// standalone `task-completed`.
pub fn dedup_scope() -> Option<&'static (PathBuf, String)> {
    DEDUP_SCOPE.get()
}

/// The testable core of the `emit` dedupe: with a scope, returns the JSON with the
/// epoch's repetitions omitted; without one (daemon route, tests, binaries that did
/// not record it) returns the JSON intact — fail-open by construction.
pub fn dedup_with_scope(json: &str, scope: Option<&(PathBuf, String)>) -> String {
    match scope {
        Some((root, session)) => {
            touring_hooks_shared::injection_dedup::dedup_hook_output(root, session, json)
        }
        None => json.to_string(),
    }
}

/// Sink the runtime layer installs so [`HookResponse::emit`] can say WHY the
/// process exits (`emit-allow` / `emit-json`). The hook trace lives in
/// `touring-hook-runtime` (it reads the stdin state kept there), a layer ABOVE
/// this crate, so `emit` reaches it through this pointer instead of a dependency.
/// `touring_hook_runtime::hook_trace::install` registers it; the trace's
/// `set_reason` is a no-op until `install` armed it, so "not registered" and
/// "registered, trace unarmed" are the same observable behaviour.
static EMIT_REASON_SINK: OnceLock<fn(&'static str)> = OnceLock::new();

/// Install the exit-reason sink. The first call wins.
pub fn set_emit_reason_sink(sink: fn(&'static str)) {
    let _ = EMIT_REASON_SINK.set(sink);
}

/// Forward `reason` to the installed sink, if any.
fn note_emit_reason(reason: &'static str) {
    if let Some(sink) = EMIT_REASON_SINK.get() {
        sink(reason);
    }
}

/// Marker that precedes the advisories carried inside a deny's `permissionDecisionReason`.
pub const ALSO_NON_BLOCKING: &str = "[também, sem bloquear]";

/// The event every PreToolUse verdict answers as.
const PRE_TOOL_USE: &str = "PreToolUse";

/// A deny's reason with the advisories that rode along. The deny stays a deny.
#[must_use]
pub fn deny_reason_with(reason: &str, avisos: &[String]) -> String {
    if avisos.is_empty() {
        return reason.to_string();
    }
    format!("{reason}\n\n{ALSO_NON_BLOCKING} {}", avisos.join("\n"))
}

/// The `additionalContext` of a non-deny verdict: the advisories first, the verdict's own
/// context after.
#[must_use]
pub fn context_with(avisos: &[String], context: Option<&str>) -> String {
    match context.filter(|c| !c.is_empty()) {
        Some(c) if !avisos.is_empty() => format!("{}\n{c}", avisos.join("\n")),
        Some(c) => c.to_string(),
        None => avisos.join("\n"),
    }
}

impl HookResponse {
    /// This verdict with the deposited `avisos` attached — deny > rewrite/advisory > nothing.
    ///
    /// * `Allow` has nothing to be more severe than: the advisories become the context.
    /// * `Deny` / `Block` / `Halt` keep their decision; the advisories enter the reason.
    /// * `Context` / `ContextWithUpdatedInput` / `ContextWithUpdatedToolOutput` keep
    ///   their payload; the advisories lead the context.
    ///
    /// An empty `avisos` returns `self` untouched.
    ///
    /// The advisories pass through the injection policy first
    /// ([`HookResponse::with_advisories_at`]) at the current request's level.
    /// Standalone code reads `TOURING_HOOK_INJECTION` from its own environment;
    /// the project actor applies the client level carried by `DaemonRequest`.
    #[must_use]
    pub fn with_advisories(self, avisos: Vec<String>) -> HookResponse {
        let (level, sink) = touring_foundation::injection_policy::current();
        self.with_advisories_at(level, sink.as_deref(), avisos)
    }

    /// [`HookResponse::with_advisories`] at an explicit injection `level`.
    ///
    /// An advisory that rides along a verdict contaminates it: a deny carries it in
    /// `permissionDecisionReason`, any other verdict in `additionalContext`, and the
    /// policy keeps a payload that mixes sermon and fact WHOLE. So the filter acts
    /// BEFORE the gluing, advisory by advisory
    /// ([`touring_foundation::injection_policy::retain_allowed`]): the gate's own
    /// reason and context are never touched, only what would have been appended to
    /// them. What it cuts goes to the journal at `sink`.
    #[must_use]
    pub fn with_advisories_at(
        self,
        level: touring_foundation::injection_policy::InjectionLevel,
        sink: Option<&std::path::Path>,
        avisos: Vec<String>,
    ) -> HookResponse {
        let avisos = touring_foundation::injection_policy::retain_allowed(
            level,
            touring_foundation::injection_policy::SITE_VERDICT,
            avisos,
            sink,
        );
        if avisos.is_empty() {
            return self;
        }
        match self {
            HookResponse::Allow => HookResponse::Context {
                context: context_with(&avisos, None),
                event_name: Some(PRE_TOOL_USE.to_string()),
            },
            HookResponse::Context {
                context,
                event_name,
            } => HookResponse::Context {
                context: context_with(&avisos, Some(&context)),
                event_name,
            },
            HookResponse::Deny {
                reason,
                context,
                event_name,
            } => HookResponse::Deny {
                reason: deny_reason_with(&reason, &avisos),
                context,
                event_name,
            },
            HookResponse::Block {
                reason,
                context,
                event_name,
            } => HookResponse::Block {
                reason: deny_reason_with(&reason, &avisos),
                context,
                event_name,
            },
            HookResponse::Halt { reason } => HookResponse::Halt {
                reason: deny_reason_with(&reason, &avisos),
            },
            HookResponse::ContextWithUpdatedInput {
                context,
                event_name,
                updated_input,
            } => HookResponse::ContextWithUpdatedInput {
                context: context_with(&avisos, Some(&context)),
                event_name,
                updated_input,
            },
            HookResponse::ContextWithUpdatedToolOutput {
                context,
                event_name,
                updated_tool_output,
            } => HookResponse::ContextWithUpdatedToolOutput {
                context: context_with(&avisos, Some(&context)),
                event_name,
                updated_tool_output,
            },
        }
    }
}

/// The Claude Code event this PROCESS answers — the input's `hook_event_name`.
///
/// S214 (2026-09-25): Claude Code rejects an output whose
/// `hookSpecificOutput.hookEventName` differs from the invoking event, and the
/// handlers named their event by hand — a Rust identifier (`pre_bash`) in the
/// validation deny, a fixed `PostToolUse` answering a `PostToolUseFailure`.
/// Set ONCE by the `touring-hook` client from its stdin, never by the daemon,
/// which answers many events: the client stamps the daemon's JSON with
/// [`stamp_event_name`] instead. Unset ⇒ every serialization is unchanged.
static INVOKING_EVENT: OnceLock<String> = OnceLock::new();

/// Record the invoking event from a hook input. First call wins; no-op when absent.
pub fn set_invoking_event(input: &serde_json::Value) {
    if let Some(name) = input
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .filter(|n| !n.is_empty())
    {
        let _ = INVOKING_EVENT.set(name.to_string());
    }
}

/// Answer as `event`: set `hookSpecificOutput.hookEventName` when the output
/// carries a `hookSpecificOutput` object. Returns whether the output changed.
pub fn stamp_event_name(output: &mut serde_json::Value, event: &str) -> bool {
    let Some(hso) = output
        .get_mut("hookSpecificOutput")
        .and_then(|h| h.as_object_mut())
    else {
        return false;
    };
    if hso.get("hookEventName").and_then(|n| n.as_str()) == Some(event) {
        return false;
    }
    hso.insert(
        "hookEventName".to_string(),
        serde_json::Value::String(event.to_string()),
    );
    true
}

/// [`stamp_event_name`] with the process's invoking event, if one was recorded.
pub fn stamp_invoking_event(output: &mut serde_json::Value) -> bool {
    INVOKING_EVENT
        .get()
        .is_some_and(|event| stamp_event_name(output, event))
}

/// Top-level fields of Claude Code's `SyncHookJSONOutput` (queried via Context7,
/// 25/09/2026) — any other key on stdout breaks the platform's output contract.
pub const TOP_LEVEL_FIELDS: &[&str] = &[
    "continue",
    "suppressOutput",
    "stopReason",
    "decision",
    "systemMessage",
    "terminalSequence",
    "reason",
    "hookSpecificOutput",
];

/// Events that carry NO `hookSpecificOutput` — top-level fields only.
pub const TOP_LEVEL_ONLY_EVENTS: &[&str] = &[
    "PreCompact",
    "PostCompact",
    "SessionEnd",
    "TeammateIdle",
    "TaskCompleted",
    "TaskCreated",
    "InstructionsLoaded",
    "ConfigChange",
    "WorktreeRemove",
    "StopFailure",
];

/// S224 (2026-09-26): conform an answer to Claude Code's output contract for
/// `event`. Every top-level key outside [`TOP_LEVEL_FIELDS`] — and a
/// `hookSpecificOutput` on a [`TOP_LEVEL_ONLY_EVENTS`] event — is moved off the
/// answer and returned, for the caller to write to stderr, the diagnostics
/// channel. The `task-completed` receipt carried 14 such keys (`event`, `RL`,
/// `finalize_status`, `diary_hint`, …) on stdout, so it reached no one. `None`
/// when the answer already conforms, or is not a JSON object.
pub fn split_off_contract_violations(
    output: &mut serde_json::Value,
    event: Option<&str>,
) -> Option<serde_json::Value> {
    let answer = output.as_object_mut()?;
    let mut removed = serde_json::Map::new();
    let unknown: Vec<String> = answer
        .keys()
        .filter(|key| !TOP_LEVEL_FIELDS.contains(&key.as_str()))
        .cloned()
        .collect();
    for key in unknown {
        if let Some(value) = answer.remove(&key) {
            removed.insert(key, value);
        }
    }
    if event.is_some_and(|e| TOP_LEVEL_ONLY_EVENTS.contains(&e))
        && let Some(specific) = answer.remove("hookSpecificOutput")
    {
        removed.insert("hookSpecificOutput".to_string(), specific);
    }
    (!removed.is_empty()).then_some(serde_json::Value::Object(removed))
}

/// [`split_off_contract_violations`] with the process's invoking event, if one
/// was recorded (unset ⇒ only the event-independent rule applies).
pub fn split_off_invoking_event_violations(
    output: &mut serde_json::Value,
) -> Option<serde_json::Value> {
    split_off_contract_violations(output, INVOKING_EVENT.get().map(String::as_str))
}

/// Truncate context to stay under Claude Code's 10K limit (9,500 UTF-8 safe).
///
/// `pub` (was `pub(crate)`): `HookRuntime` in the touring-hooks dispatch layer
/// calls this across the Phase C crate boundary (4 call sites).
pub fn truncate_context(context: &str) -> String {
    const MAX_CHARS: usize = 9_500;
    if context.len() <= MAX_CHARS {
        return context.to_string();
    }
    let mut end = MAX_CHARS;
    while end > 0 && !context.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}...\n[truncated: {} chars total]",
        &context[..end],
        context.len()
    )
}

#[cfg(test)]
mod s224_tests {
    use super::split_off_contract_violations;
    use serde_json::json;

    #[test]
    fn unknown_top_level_keys_leave_the_answer_and_come_back_as_diagnostics() {
        let mut out =
            json!({"event": "task-completed", "RL": "reward_signal", "systemMessage": "m"});
        let removed = split_off_contract_violations(&mut out, Some("TaskCompleted"))
            .expect("two keys the platform does not know");
        assert_eq!(out, json!({"systemMessage": "m"}), "the known field stays");
        assert_eq!(
            removed,
            json!({"event": "task-completed", "RL": "reward_signal"})
        );
    }

    #[test]
    fn specific_output_leaves_only_a_top_level_only_event() {
        let answer =
            json!({"hookSpecificOutput": {"hookEventName": "X", "additionalContext": "c"}});
        let mut top_level_only = answer.clone();
        assert!(
            split_off_contract_violations(&mut top_level_only, Some("TaskCompleted")).is_some()
        );
        assert_eq!(top_level_only, json!({}));
        let mut specific = answer.clone();
        assert!(split_off_contract_violations(&mut specific, Some("PostToolUse")).is_none());
        assert_eq!(specific, answer, "PostToolUse admits hookSpecificOutput");
        let mut unknown_event = answer.clone();
        assert!(split_off_contract_violations(&mut unknown_event, None).is_none());
    }

    #[test]
    fn a_conforming_answer_is_untouched() {
        let answer = json!({"decision": "block", "reason": "r", "suppressOutput": true});
        let mut out = answer.clone();
        assert!(split_off_contract_violations(&mut out, Some("TaskCompleted")).is_none());
        assert_eq!(out, answer);
        let mut not_an_object = json!("text");
        assert!(split_off_contract_violations(&mut not_an_object, Some("TaskCompleted")).is_none());
    }
}

#[cfg(test)]
mod s214_tests {
    use super::stamp_event_name;
    use serde_json::json;

    #[test]
    fn stamp_replaces_a_hand_named_event() {
        let mut out = json!({"hookSpecificOutput": {"hookEventName": "pre_bash", "permissionDecision": "deny"}});
        assert!(stamp_event_name(&mut out, "PreToolUse"));
        assert_eq!(out["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "deny");
    }

    #[test]
    fn stamp_inserts_a_missing_event_name() {
        let mut out = json!({"decision": "block", "reason": "r", "hookSpecificOutput": {}});
        assert!(stamp_event_name(&mut out, "PostToolUseFailure"));
        assert_eq!(
            out["hookSpecificOutput"]["hookEventName"],
            "PostToolUseFailure"
        );
    }

    #[test]
    fn stamp_leaves_outputs_without_specific_output_alone() {
        for mut out in [json!({}), json!({"continue": false, "stopReason": "x"})] {
            let before = out.clone();
            assert!(!stamp_event_name(&mut out, "PreToolUse"));
            assert_eq!(out, before);
        }
    }

    #[test]
    fn stamp_reports_no_change_when_the_name_already_matches() {
        let mut out = json!({"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": "c"}});
        assert!(!stamp_event_name(&mut out, "PostToolUse"));
    }
}

/// S-2 (27/10/2026) — a política de injeção no `emit` do tipo canônico.
/// `emit` sai do processo; o que ele escreve é `stdout_text`, testado com
/// nível, diário e escopo EXPLÍCITOS (nenhum teste mexe no ambiente).
#[cfg(test)]
mod s2_injection_policy_tests {
    use super::HookResponse;
    use serde_json::{Value, json};
    use touring_foundation::injection_policy::InjectionLevel;

    const SERMON: &str =
        "[TOURING SUGGEST · code-mode-scan · conf=0.95]\n  MUST    touring run --lang bash";

    /// O que `emit` escreveria para `r` no `level`, ou `None` se calaria.
    fn written(
        r: HookResponse,
        level: InjectionLevel,
        sink: Option<&std::path::Path>,
    ) -> Option<String> {
        (!matches!(r, HookResponse::Allow)).then(|| r.stdout_text(level, sink, None))
    }

    /// O contrato C13 no `emit` do núcleo: sob `no-advisory` o sermão sai dos
    /// três veredictos que o carregam e a decisão de cada um fica intacta.
    #[test]
    fn test_no_advisory_drops_sermon_keeps_decision() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sink = touring_foundation::injection_journal::journal_path_in(dir.path());
        let level = InjectionLevel::NoAdvisory;

        let deny = HookResponse::Deny {
            reason: "G2: pipe sem pipefail".into(),
            context: Some(SERMON.into()),
            event_name: Some("PreToolUse".into()),
        };
        let v: Value =
            serde_json::from_str(&written(deny, level, Some(&sink)).expect("deny escreve"))
                .expect("JSON");
        assert!(v["hookSpecificOutput"].get("additionalContext").is_none());
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(
            v["hookSpecificOutput"]["permissionDecisionReason"],
            "G2: pipe sem pipefail"
        );

        let block = HookResponse::Block {
            reason: "gate falhou".into(),
            context: Some(SERMON.into()),
            event_name: Some("PostToolUse".into()),
        };
        let v: Value =
            serde_json::from_str(&written(block, level, Some(&sink)).expect("block escreve"))
                .expect("JSON");
        assert_eq!(v["decision"], "block");
        assert_eq!(v["reason"], "gate falhou");
        assert!(v["hookSpecificOutput"].get("additionalContext").is_none());

        let rewrite = HookResponse::ContextWithUpdatedInput {
            context: SERMON.into(),
            event_name: Some("PreToolUse".into()),
            updated_input: json!({"command": "set -o pipefail; ls"}),
        };
        let v: Value =
            serde_json::from_str(&written(rewrite, level, Some(&sink)).expect("escreve"))
                .expect("JSON");
        assert_eq!(
            v["hookSpecificOutput"]["updatedInput"],
            json!({"command": "set -o pipefail; ls"}),
            "a entrada reescrita é decisão, não contexto"
        );
        assert!(v["hookSpecificOutput"].get("additionalContext").is_none());

        // Três cortes, três linhas, todas deste sítio.
        let lines: Vec<Value> = std::fs::read_to_string(&sink)
            .expect("diário")
            .lines()
            .map(|l| serde_json::from_str(l).expect("linha JSON"))
            .collect();
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|l| l["site"] == "emit-runtime"));
    }

    /// `off` num `Context` puro: o envelope vira `{}`; `Halt` (só decisão)
    /// atravessa `off` byte a byte; `Allow` não escreve nada em nível algum.
    #[test]
    fn test_off_empties_context_keeps_halt_and_allow_stays_silent() {
        let ctx = HookResponse::context_with_event(SERMON, "PostToolUse");
        assert_eq!(
            written(ctx, InjectionLevel::Off, None).as_deref(),
            Some("{}")
        );

        let halt = || HookResponse::halt("fim de sessão");
        assert_eq!(
            written(halt(), InjectionLevel::Off, None),
            written(halt(), InjectionLevel::Full, None),
            "uma decisão pura não muda com o nível"
        );

        for level in [
            InjectionLevel::Full,
            InjectionLevel::NoAdvisory,
            InjectionLevel::Off,
        ] {
            assert_eq!(written(HookResponse::Allow, level, None), None);
        }
    }

    /// Sem política (`Full`, o default) o `emit` escreve a serialização de
    /// sempre, variante a variante: a identidade de bytes do default.
    #[test]
    fn test_full_writes_the_serialization_byte_for_byte() {
        let variants = || {
            vec![
                HookResponse::context_with_event(SERMON, "PostToolUse"),
                HookResponse::deny_with_context("razão", SERMON),
                HookResponse::block("razão"),
                HookResponse::halt("fim"),
                HookResponse::context_with_updated_input(SERMON, json!({"command": "ls"})),
            ]
        };
        for (r, expected) in variants().into_iter().zip(variants()) {
            let direct = expected.to_json();
            assert_eq!(
                written(r, InjectionLevel::Full, None).as_deref(),
                Some(direct.as_str())
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_variant() {
        let resp = HookResponse::allow();
        assert_eq!(resp, HookResponse::Allow);
    }

    #[test]
    fn test_context_variant() {
        let resp = HookResponse::context("test context");
        match resp {
            HookResponse::Context {
                context,
                event_name,
            } => {
                assert_eq!(context, "test context");
                assert!(event_name.is_none());
            }
            _ => panic!("expected Context variant"),
        }
    }

    #[test]
    fn test_deny_variant() {
        let resp = HookResponse::deny("too complex");
        match resp {
            HookResponse::Deny {
                reason,
                context,
                event_name,
            } => {
                assert_eq!(reason, "too complex");
                assert!(context.is_none());
                assert!(event_name.is_none());
            }
            _ => panic!("expected Deny variant"),
        }
    }

    #[test]
    fn test_halt_variant() {
        let resp = HookResponse::halt("catastrophic failure");
        match resp {
            HookResponse::Halt { reason } => {
                assert_eq!(reason, "catastrophic failure");
            }
            _ => panic!("expected Halt variant"),
        }
    }

    #[test]
    fn test_context_with_updated_input() {
        let json = serde_json::json!({"path": "/new/path"});
        let resp = HookResponse::context_with_updated_input("path normalized", json.clone());
        match resp {
            HookResponse::ContextWithUpdatedInput {
                context,
                event_name,
                updated_input,
            } => {
                assert_eq!(context, "path normalized");
                assert!(event_name.is_none());
                assert_eq!(updated_input, json);
            }
            _ => panic!("expected ContextWithUpdatedInput variant"),
        }
    }

    #[test]
    fn test_truncate_context_short() {
        let short = "short context";
        assert_eq!(truncate_context(short), short);
    }

    #[test]
    fn test_truncate_context_long() {
        let long = "x".repeat(20_000);
        let truncated = truncate_context(&long);
        assert!(truncated.len() <= 9_550); // 9,500 + "[...truncated]"
        assert!(truncated.contains("[truncated:"));
    }

    #[test]
    fn test_lazy_response_cache() {
        let resp = HookResponse::allow();
        let lazy = resp.to_lazy();

        // First call: caches the result
        let s1 = lazy.as_str();
        assert_eq!(s1, "{}");

        // Second call: returns cached value (same pointer)
        let s2 = lazy.as_str();
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_lazy_response_context() {
        let resp = HookResponse::context("test context");
        let lazy = resp.to_lazy();

        let s = lazy.as_str();
        assert!(s.contains("test context"));
        assert!(s.contains("hookSpecificOutput"));
    }

    #[test]
    fn test_lazy_response_clone_independent() {
        let resp = HookResponse::halt("oops");
        let lazy1 = resp.to_lazy();
        let _lazy2 = lazy1.clone();

        // Both should work independently
        assert!(lazy1.as_str().contains("oops"));
        assert!(lazy1.as_str().len() > 0);
    }
}

/// I-3 — the exit-reason sink: `emit` reaches the runtime layer's hook trace through
/// a pointer installed once, not through a dependency.
#[cfg(test)]
mod i3_emit_reason_sink_tests {
    use super::{note_emit_reason, set_emit_reason_sink};
    use std::sync::Mutex;

    static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

    fn record(reason: &'static str) {
        SEEN.lock().expect("lock").push(reason);
    }

    #[test]
    fn note_emit_reason_reaches_the_installed_sink() {
        set_emit_reason_sink(record);
        note_emit_reason("emit-json");
        note_emit_reason("emit-allow");
        assert_eq!(
            *SEEN.lock().expect("lock"),
            vec!["emit-json", "emit-allow"],
            "o sink recebe cada razão, na ordem"
        );
    }
}
