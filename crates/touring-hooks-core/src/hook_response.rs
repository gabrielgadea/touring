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
}

impl HookResponse {
    /// Serialize this response to stdout and call `process::exit(0)`.
    ///
    /// This is the bridge between the testable `HookResponse` and the
    /// diverging behavior required by Claude Code hook protocol.
    ///
    /// S-2 (27/10/2026): the answer goes through the injection policy
    /// ([`touring_foundation::injection_policy`]) on its way out, at the level
    /// of this process's environment. With `TOURING_HOOK_INJECTION` unset the
    /// bytes are exactly what they always were.
    pub fn emit(self) -> ! {
        if let Some(output) = self.emit_value() {
            let (level, sink) = touring_foundation::injection_policy::current();
            println!("{}", Self::stdout_text(&output, level, sink.as_deref()));
        }
        std::process::exit(0);
    }

    /// The text `emit` writes to stdout for `output`: its compact JSON, filtered
    /// by the injection policy at `level` (cuts recorded in the journal at
    /// `sink`). Split from `emit` — which exits the process — to be testable.
    fn stdout_text(
        output: &serde_json::Value,
        level: touring_foundation::injection_policy::InjectionLevel,
        sink: Option<&std::path::Path>,
    ) -> String {
        let json = serde_json::to_string(output).unwrap_or_default();
        touring_foundation::injection_policy::apply_at(
            level,
            touring_foundation::injection_policy::SITE_EMIT_CORE,
            &json,
            sink,
        )
        .into_owned()
    }

    /// The JSON object `emit` prints, or `None` for `Allow` (which prints
    /// nothing: an empty stdout is the hook protocol's "no answer").
    fn emit_value(self) -> Option<serde_json::Value> {
        match self {
            HookResponse::Allow => None,
            HookResponse::Context {
                context,
                event_name,
            } => {
                let truncated = truncate_context(&context);
                let mut hso = serde_json::json!({
                    "additionalContext": truncated,
                });
                if let Some(name) = event_name {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["hookEventName"] = serde_json::Value::String(name);
                    }
                }
                let output = serde_json::json!({
                    "hookSpecificOutput": hso,
                });
                Some(output)
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
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["additionalContext"] = serde_json::Value::String(ctx);
                    }
                }
                if let Some(name) = event_name {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["hookEventName"] = serde_json::Value::String(name);
                    }
                }
                let output = serde_json::json!({
                    "hookSpecificOutput": hso,
                });
                Some(output)
            }
            HookResponse::Block {
                reason,
                context,
                event_name,
            } => {
                let mut hso = serde_json::json!({});
                if let Some(ctx) = context {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["additionalContext"] = serde_json::Value::String(ctx);
                    }
                }
                if let Some(name) = event_name {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["hookEventName"] = serde_json::Value::String(name);
                    }
                }
                let output = serde_json::json!({
                    "decision": "block",
                    "reason": reason,
                    "hookSpecificOutput": hso,
                });
                Some(output)
            }
            HookResponse::Halt { reason } => {
                let output = serde_json::json!({
                    "continue": false,
                    "stopReason": reason,
                });
                Some(output)
            }
            HookResponse::ContextWithUpdatedInput {
                context,
                event_name,
                updated_input,
            } => {
                let truncated = truncate_context(&context);
                let mut hso = serde_json::json!({
                    "additionalContext": truncated,
                    "updatedInput": updated_input,
                });
                if let Some(name) = event_name {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["hookEventName"] = serde_json::Value::String(name);
                    }
                }
                let output = serde_json::json!({
                    "hookSpecificOutput": hso,
                });
                Some(output)
            }
        }
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
        };
        let mut value = value;
        stamp_invoking_event(&mut value);
        serde_json::to_string(&value).unwrap_or_default()
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
/// `emit` sai do processo; o que ele escreve é `stdout_text(emit_value())`,
/// testado com nível e diário EXPLÍCITOS (nenhum teste mexe no ambiente).
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
        r.emit_value()
            .map(|o| HookResponse::stdout_text(&o, level, sink))
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
        assert!(lines.iter().all(|l| l["site"] == "emit-core"));
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
            let value = expected.emit_value().expect("não é Allow");
            let direct = serde_json::to_string(&value).expect("serializa");
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
