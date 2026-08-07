//! `GeneratorContext` v2 — decoupled via traits, multi-LLM, observability-ready.
//!
//! PLN2 section 8.1 additions:
//! - `symbol_index`: direct `touring_intelligence::index::IncrementalIndex` for in-process lookups
//! - `fuzzy_index`: `FuzzyMatcher` trait for O(log N) symbol suggestions
//! - `schema_registry`: v1→v2 plan schema migration registry
//! - 7 closure fields for cross-crate integration without circular deps

use crate::core::capacity::CapacityLimits;
use crate::core::score::NormalizedScore;
use crate::error::GenerateError;
use crate::plan::contracts::SymbolRef;
use crate::plan::result::RenderedFile;
use crate::plan::schema::GeneratorPlan;
use crate::registry::plan_registry::PlanRegistry;
use crate::speculate::bridge::SpeculateBridge;
use crate::template::engine::TemplateEngine;
use crate::vgp::engine::VgpEngine;
use camino::Utf8PathBuf;
use moka::sync::Cache;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::LazyLock;
use uuid::Uuid;

// ── Module-level caches for PLN2 multidimensional RL rewards ─────────────────

/// Caches average quality scores indexed by `plan_id` for RL reward injection.
/// Populated by `QualityGateAdapter::check()` before returning.
/// Uses moka `W-TinyLFU` cache with 10 000 entry capacity.
pub static QUALITY_SCORES: LazyLock<Cache<String, f64>> = LazyLock::new(|| Cache::new(10_000));

/// Caches wiring baseline scores indexed by `plan_id` for RL reward injection.
/// Populated by `AnalysisGateAdapter::check()` before returning.
pub static WIRING_SCORES: LazyLock<Cache<String, f64>> = LazyLock::new(|| Cache::new(10_000));

/// Caches blast radius affected counts indexed by `file_path` for RL reward.
/// Populated by `blast_radius_check()` async advisory.
pub static BLAST_COUNTS: LazyLock<Cache<String, usize>> = LazyLock::new(|| Cache::new(10_000));

/// Caches E2E health scores indexed by `plan_id` for RL reward injection.
/// Populated by async `HealthGateAdapter::check()` after commit.
pub static HEALTH_SCORES: LazyLock<Cache<String, f64>> = LazyLock::new(|| Cache::new(10_000));

// ── FuzzyMatcher trait (PLN2 section 8.1 — BkTreeFuzzy abstraction) ──────────

// ── Fuzzy matching (extracted to `context_fuzzy`, F-9 modularization) ─────────
// `FuzzyMatcher`, `NoopFuzzyMatcher`, `BkTreeFuzzyAdapter`, and `FuzzySuggestion`
// now live in `crate::core::context_fuzzy`; re-exported here so every existing
// `core::context::*` path (and the `lib.rs` re-exports) resolves unchanged.
#[cfg(feature = "simd-fuzzy")]
pub use crate::core::context_fuzzy::BkTreeFuzzyAdapter;
pub use crate::core::context_fuzzy::{FuzzyMatcher, FuzzySuggestion, NoopFuzzyMatcher};

// ── Schema registry + wiring gates (extracted to `context_wiring`, F-9) ───────
// `SchemaRegistry`, `SynWiringGateAdapter`, and `CompositeWiringGate` now live
// in `crate::core::context_wiring`; re-exported here so every `core::context::*`
// path (and the `lib.rs` re-exports) resolves unchanged. `WiringGateFn` and the
// `AnalysisGateAdapter` / `WiringGateError` types stay below in this module.
#[cfg(feature = "analysis-gate")]
pub use crate::core::context_wiring::CompositeWiringGate;
pub use crate::core::context_wiring::{SchemaRegistry, SynWiringGateAdapter};

// ── RkyvFileSnapshotAdapter (PLN2 section 8.1 — feature `zero-copy`) ────────

#[cfg(feature = "mcts-synthesis")]
pub use crate::core::context_exec::McctsEvalAdapter;
/// Zero-copy file snapshot adapter using rkyv for sub-millisecond rollback.
///
/// Serializes rendered-file pairs `(path, content)` into an rkyv archive for
/// ultra-fast restore. Used as a checkpoint primitive: before commit, the
/// executor can call `snapshot_rendered()` to capture the prepared artifacts,
/// then `restore_rendered()` later if a rollback is needed without re-running
/// the template engine.
///
/// # Performance
///
/// rkyv 0.7 produces an aligned byte buffer that deserializes via validated
/// archive access (`rkyv::check_archived_root`). For a typical plan with
/// 1–3 rendered files the snapshot/restore cycle is < 100 µs — versus
/// 5–20 ms to re-render from the Tera engine.
///
/// # POTENCIALIZAR
///
/// Activates the `rkyv` workspace dep that was previously only pulled in
/// by `touring-rkyv`. The `zero-copy` feature now has a concrete user.
///
/// # Why NOT `touring_rkyv::templates`?
///
/// This module uses raw `rkyv` for **internal pipeline snapshots** (speculative
/// validation, plan rollback, commit tracking), not for cross-crate IPC. The
/// `touring_rkyv::templates` are designed for cross-crate data sharing with
/// versioned schemas. touring-generator's `RenderedFile` is an ephemeral
/// pipeline artifact — its snapshot is not consumed by other crates and has
/// a different lifecycle (short-lived, process-local) than IPC types
/// (persistent, cross-process). The custom binary format in `snapshot()`
/// and `rkyv::to_bytes` in `snapshot_rkyv` serve this internal use case
/// without coupling to the shared template system.
// ── Execution adapters (extracted to `context_exec`, F-9 modularization) ──────
// `RkyvFileSnapshotAdapter`, `WasmSandboxAdapter`/`WasmSandboxError`, and
// `McctsEvalAdapter` now live in `crate::core::context_exec`; re-exported here
// so every `core::context::*` path (and the `lib.rs` re-exports) resolves
// unchanged. The `WasmSandboxFn`/`MctsEvalFn` type aliases stay in this module.
#[cfg(feature = "zero-copy")]
pub use crate::core::context_exec::RkyvFileSnapshotAdapter;
#[cfg(feature = "wasm-sandbox")]
pub use crate::core::context_exec::{WasmSandboxAdapter, WasmSandboxError};

// ── Telemetry + NLP ranker (extracted to `context_telemetry`, F-9) ────────────
// `TracingTelemetrySink` and `NlpPlanRankerAdapter` now live in
// `crate::core::context_telemetry`; re-exported here so every `core::context::*`
// path (and the `lib.rs` re-exports) resolves unchanged.
#[cfg(feature = "nlp-reranking")]
pub use crate::core::context_telemetry::NlpPlanRankerAdapter;
#[cfg(feature = "observability")]
pub use crate::core::context_telemetry::TracingTelemetrySink;

// ── AnalysisGateAdapter (extracted to `context_gates`, F-9 modularization) ────
// `AnalysisGateAdapter`, `WiringGateError`, and `WIRING_GATE_BYPASSED_COUNT` now
// live in `crate::core::context_gates`; re-exported here so every
// `core::context::*` path (and the `lib.rs` re-exports) resolves unchanged.
#[cfg(feature = "analysis-gate")]
pub use crate::core::context_gates::{
    AnalysisGateAdapter, WIRING_GATE_BYPASSED_COUNT, WiringGateError,
};

// ── Quality / health gates + semantic graph + scores (extracted to
//    `context_quality`, F-9 modularization) ──────────────────────────────────
// These now live in `crate::core::context_quality`; re-exported here so every
// `core::context::*` path (and the `lib.rs` re-exports) resolves unchanged.
// `PlanSimilarityScore` is also re-exported for `context_telemetry`'s back-ref.
// The `SemanticGraphFn` / `CognitiveNexusFn` aliases + `HEALTH_SCORES` cache
// stay in this module.
#[cfg(feature = "enrichment-gate")]
pub use crate::core::context_quality::EnrichmentTriggerFn;
#[cfg(feature = "cognitive-nexus")]
pub use crate::core::context_quality::SemanticGraphAdapter;
pub use crate::core::context_quality::{
    HealthDeltaComputeFn, HealthDeltaRecordFn, PlanSimilarityScore,
};
#[cfg(feature = "health-gate")]
pub use crate::core::context_quality::{HealthGateAdapter, HealthGateFn};
#[cfg(feature = "quality-gate")]
pub use crate::core::context_quality::{QualityGateAdapter, QualityGateFn};

// ── Provider traits ───────────────────────────────────────────────────────────

/// `DSPy` signature name identifier.
pub type DspySignatureName = String;

/// `DSPy` input key-value map.
pub type DspyInputs = std::collections::HashMap<String, serde_json::Value>;

/// `DSPy` output key-value map.
pub type DspyOutputs = std::collections::HashMap<String, serde_json::Value>;

/// Error from an LLM provider.
#[derive(Debug, thiserror::Error)]
#[error("LLM error from '{provider}': {message}")]
pub struct LlmError {
    /// Name of the LLM provider that produced the error.
    pub provider: String,
    /// Human-readable error message returned by the provider.
    pub message: String,
}

/// Pluggable LLM provider — enables multi-LLM and mock testing.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Execute a `DSPy` signature with structured inputs → outputs.
    async fn execute_signature(
        &self,
        signature: &DspySignatureName,
        inputs: &DspyInputs,
    ) -> Result<DspyOutputs, LlmError>;

    /// Estimate token count for a text (used by `PlanRegistry` for budget).
    fn estimate_tokens(&self, text: &str) -> u32;

    /// Provider name for telemetry and memory keys.
    fn name(&self) -> &'static str;
}

/// Memory tier for persisting entries.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub enum MemoryTier {
    /// Long-lived, cross-session semantic memory.
    Semantic,
    /// Short-lived, session-local memory.
    Local,
}

/// Memory entry kind.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub enum MemoryKind {
    /// A lesson learned from a prior outcome.
    Lesson,
    /// A recurring pattern worth reusing.
    Pattern,
    /// An analytical insight derived from observation.
    Insight,
    /// A known pitfall to avoid.
    Gotcha,
}

/// Error from the memory provider.
#[derive(Debug, thiserror::Error)]
#[error("memory error: {0}")]
pub struct MemoryError(pub String);

/// A recalled memory entry.
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    /// Unique key under which the entry is stored.
    pub key: String,
    /// Stored value (the memory content).
    pub value: String,
    /// Number of times this entry has been recalled.
    pub access_count: u32,
}

/// Memory statistics.
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    /// Total number of stored memory entries across all tiers.
    pub total_entries: u64,
    /// Number of entries in the semantic (long-lived) tier.
    pub semantic_entries: u64,
}

/// Pluggable memory provider — touring memory store/recall abstraction.
pub trait MemoryProvider: Send + Sync {
    /// Store a memory entry.
    ///
    /// # Errors
    /// Returns `MemoryError` if the underlying store fails.
    fn store(
        &self,
        key: &str,
        value: &str,
        tier: MemoryTier,
        kind: MemoryKind,
    ) -> Result<(), MemoryError>;

    /// Recall memory entries matching `query`.
    ///
    /// # Errors
    /// Returns `MemoryError` if the underlying recall fails.
    fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>, MemoryError>;

    /// Returns aggregate statistics about the memory store.
    fn stats(&self) -> MemoryStats;
}

/// Pluggable RL reward sink — decouples from touring-learning direct dep.
pub trait RlRewardSink: Send + Sync {
    /// Inject a reward signal for the given tool.
    fn inject(&self, tool: &str, reward: NormalizedScore, context: &str);

    /// Returns the current EMA reward for a tool if tracked.
    fn ema(&self, tool: &str) -> Option<f64>;
}

/// Pluggable telemetry sink — lifecycle metrics.
pub trait TelemetrySink: Send + Sync {
    /// Record a typestate lifecycle transition with its elapsed time in nanoseconds.
    fn record_lifecycle_transition(&self, from: &str, to: &str, plan_id: Uuid, elapsed_ns: u64);

    /// Increment a named monotonic counter by `value`.
    fn increment_counter(&self, name: &'static str, value: u64);

    /// Record a single sample `value` into the named histogram.
    fn record_histogram(&self, name: &'static str, value: f64);
}

/// Append-only audit log for human overrides and security events.
pub trait AuditLog: Send + Sync {
    /// Append an audit entry recording a human override or security event.
    fn append(&self, entry: crate::plan::result::AuditEntry);
}

// ── No-op implementations for testing ────────────────────────────────────────

/// No-op LLM provider — returns empty outputs, used in unit tests.
pub struct NoopLlm;

#[async_trait::async_trait]
impl LlmProvider for NoopLlm {
    async fn execute_signature(
        &self,
        _sig: &DspySignatureName,
        _inputs: &DspyInputs,
    ) -> Result<DspyOutputs, LlmError> {
        Ok(DspyOutputs::new())
    }

    fn estimate_tokens(&self, text: &str) -> u32 {
        // Rough estimate: 4 chars per token. text.len() is bounded by usize;
        // dividing by 4 keeps it well within u32 for any realistic input.
        u32::try_from(text.len() / 4).unwrap_or(u32::MAX)
    }

    fn name(&self) -> &'static str {
        "noop"
    }
}

// ── HTTP-backed LLM providers (feature `llm-http`) ───────────────────────────
//
// Real `LlmProvider` implementations that drive an OpenAI-compatible chat API and
// a local Ollama server. The prompt-building and response-parsing helpers are pure
// (no I/O) so they are unit-tested without a network. Selection is via
// `llm_provider_from_env`; absent configuration falls back to `NoopLlm`.

/// Construct an [`LlmError`] for `provider` with `message`.
#[cfg(feature = "llm-http")]
fn llm_err(provider: &str, message: &str) -> LlmError {
    LlmError {
        provider: provider.to_string(),
        message: message.to_string(),
    }
}

/// System prompt instructing the model to answer a named `DSPy` signature as a
/// single JSON object whose keys are the signature's output fields.
#[cfg(feature = "llm-http")]
fn dspy_system_prompt(signature: &DspySignatureName) -> String {
    format!(
        "You are executing the DSPy signature `{signature}`. Read the JSON input \
         fields and respond with ONLY a single JSON object mapping each output \
         field name to its value. Do not include prose, markdown, or code fences."
    )
}

/// User prompt carrying the signature inputs as a compact JSON object.
#[cfg(feature = "llm-http")]
fn dspy_user_prompt(inputs: &DspyInputs) -> String {
    serde_json::to_string(inputs).unwrap_or_else(|_| "{}".to_string())
}

/// Interpret an LLM `content` string as `DspyOutputs`: if it parses as a JSON
/// object, use it directly; otherwise wrap the raw text under a `response` key.
#[cfg(feature = "llm-http")]
fn content_to_outputs(content: &str) -> DspyOutputs {
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(content) {
        map.into_iter().collect()
    } else {
        let mut out = DspyOutputs::new();
        out.insert(
            "response".to_string(),
            serde_json::Value::String(content.to_string()),
        );
        out
    }
}

/// Parse an `OpenAI` `/chat/completions` response body into `DspyOutputs`.
///
/// # Errors
/// Returns [`LlmError`] if the body is not valid JSON or lacks
/// `choices[0].message.content`.
#[cfg(feature = "llm-http")]
fn parse_openai_response(body: &str) -> Result<DspyOutputs, LlmError> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| llm_err("openai", &e.to_string()))?;
    let content = v
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| llm_err("openai", "missing choices[0].message.content"))?;
    Ok(content_to_outputs(content))
}

/// Parse an Ollama `/api/chat` response body into `DspyOutputs`.
///
/// # Errors
/// Returns [`LlmError`] if the body is not valid JSON or lacks `message.content`.
#[cfg(feature = "llm-http")]
fn parse_ollama_response(body: &str) -> Result<DspyOutputs, LlmError> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| llm_err("ollama", &e.to_string()))?;
    let content = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| llm_err("ollama", "missing message.content"))?;
    Ok(content_to_outputs(content))
}

/// Select the configured [`LlmProvider`] from the environment, defaulting to
/// [`NoopLlm`].
///
/// `TOURING_LLM_PROVIDER` chooses the backend (`openai` | `ollama` | `noop`,
/// default `noop`). `OpenAI` requires `OPENAI_API_KEY`; if it is unset the factory
/// degrades to [`NoopLlm`] rather than failing.
#[cfg(feature = "llm-http")]
#[must_use]
pub fn llm_provider_from_env() -> std::sync::Arc<dyn LlmProvider> {
    match std::env::var("TOURING_LLM_PROVIDER").as_deref() {
        Ok("openai") => OpenAiLlm::from_env().map_or_else(
            |_| std::sync::Arc::new(NoopLlm) as std::sync::Arc<dyn LlmProvider>,
            |p| std::sync::Arc::new(p),
        ),
        Ok("ollama") => std::sync::Arc::new(OllamaLlm::from_env()),
        _ => std::sync::Arc::new(NoopLlm),
    }
}

/// LLM provider backed by an `OpenAI`-compatible `/chat/completions` endpoint.
#[cfg(feature = "llm-http")]
pub struct OpenAiLlm {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

#[cfg(feature = "llm-http")]
impl OpenAiLlm {
    /// Construct from an explicit API key, model id, and base URL.
    #[must_use]
    pub fn new(api_key: String, model: String, base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
            base_url,
        }
    }

    /// Construct from the environment: `OPENAI_API_KEY` (required), `OPENAI_MODEL`
    /// (default `gpt-4o-mini`), `OPENAI_BASE_URL` (default `https://api.openai.com/v1`).
    ///
    /// # Errors
    /// Returns [`LlmError`] if `OPENAI_API_KEY` is not set.
    pub fn from_env() -> Result<Self, LlmError> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| llm_err("openai", "OPENAI_API_KEY is not set"))?;
        let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        Ok(Self::new(api_key, model, base_url))
    }
}

#[cfg(feature = "llm-http")]
#[async_trait::async_trait]
impl LlmProvider for OpenAiLlm {
    async fn execute_signature(
        &self,
        signature: &DspySignatureName,
        inputs: &DspyInputs,
    ) -> Result<DspyOutputs, LlmError> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": dspy_system_prompt(signature) },
                { "role": "user", "content": dspy_user_prompt(inputs) },
            ],
            "response_format": { "type": "json_object" },
            "temperature": 0.0,
        });
        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| llm_err("openai", &e.to_string()))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| llm_err("openai", &e.to_string()))?;
        if !status.is_success() {
            return Err(llm_err("openai", &format!("HTTP {status}: {text}")));
        }
        parse_openai_response(&text)
    }

    fn estimate_tokens(&self, text: &str) -> u32 {
        u32::try_from(text.len() / 4).unwrap_or(u32::MAX)
    }

    fn name(&self) -> &'static str {
        "openai"
    }
}

/// LLM provider backed by a local Ollama server (`/api/chat`).
#[cfg(feature = "llm-http")]
pub struct OllamaLlm {
    client: reqwest::Client,
    model: String,
    base_url: String,
}

#[cfg(feature = "llm-http")]
impl OllamaLlm {
    /// Construct from an explicit model id and base URL.
    #[must_use]
    pub fn new(model: String, base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            model,
            base_url,
        }
    }

    /// Construct from the environment: `OLLAMA_MODEL` (default `llama3.2`),
    /// `OLLAMA_BASE_URL` (default `http://localhost:11434`). Never fails — a local
    /// Ollama needs no credentials.
    #[must_use]
    pub fn from_env() -> Self {
        let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".to_string());
        let base_url = std::env::var("OLLAMA_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        Self::new(model, base_url)
    }
}

#[cfg(feature = "llm-http")]
#[async_trait::async_trait]
impl LlmProvider for OllamaLlm {
    async fn execute_signature(
        &self,
        signature: &DspySignatureName,
        inputs: &DspyInputs,
    ) -> Result<DspyOutputs, LlmError> {
        let body = serde_json::json!({
            "model": self.model,
            "stream": false,
            "format": "json",
            "messages": [
                { "role": "system", "content": dspy_system_prompt(signature) },
                { "role": "user", "content": dspy_user_prompt(inputs) },
            ],
        });
        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| llm_err("ollama", &e.to_string()))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| llm_err("ollama", &e.to_string()))?;
        if !status.is_success() {
            return Err(llm_err("ollama", &format!("HTTP {status}: {text}")));
        }
        parse_ollama_response(&text)
    }

    fn estimate_tokens(&self, text: &str) -> u32 {
        u32::try_from(text.len() / 4).unwrap_or(u32::MAX)
    }

    fn name(&self) -> &'static str {
        "ollama"
    }
}

#[cfg(all(test, feature = "llm-http"))]
mod llm_http_tests {
    use super::{
        LlmProvider, OllamaLlm, OpenAiLlm, content_to_outputs, parse_ollama_response,
        parse_openai_response,
    };

    #[test]
    fn openai_response_parses_json_object_content() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"{\"answer\":42,\"ok\":true}"}}]}"#;
        let out = parse_openai_response(body).expect("parse");
        assert_eq!(
            out.get("answer").and_then(serde_json::Value::as_i64),
            Some(42)
        );
        assert_eq!(
            out.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn openai_response_wraps_non_json_content() {
        let body = r#"{"choices":[{"message":{"content":"plain text reply"}}]}"#;
        let out = parse_openai_response(body).expect("parse");
        assert_eq!(
            out.get("response").and_then(serde_json::Value::as_str),
            Some("plain text reply")
        );
    }

    #[test]
    fn openai_response_errors_on_missing_content() {
        assert!(parse_openai_response(r#"{"choices":[]}"#).is_err());
        assert!(parse_openai_response("not json").is_err());
    }

    #[test]
    fn ollama_response_parses_message_content() {
        let body = r#"{"model":"llama3.2","message":{"role":"assistant","content":"{\"k\":\"v\"}"},"done":true}"#;
        let out = parse_ollama_response(body).expect("parse");
        assert_eq!(out.get("k").and_then(serde_json::Value::as_str), Some("v"));
    }

    #[test]
    fn ollama_response_errors_on_missing_content() {
        assert!(parse_ollama_response(r#"{"done":true}"#).is_err());
    }

    #[test]
    fn content_to_outputs_handles_both_shapes() {
        assert!(content_to_outputs("{\"a\":1}").contains_key("a"));
        assert!(content_to_outputs("hello").contains_key("response"));
    }

    #[test]
    fn provider_names_and_token_estimate() {
        let openai = OpenAiLlm::new("k".into(), "m".into(), "http://x".into());
        let ollama = OllamaLlm::new("m".into(), "http://x".into());
        assert_eq!(openai.name(), "openai");
        assert_eq!(ollama.name(), "ollama");
        // 4 chars/token heuristic shared with NoopLlm.
        assert_eq!(openai.estimate_tokens("abcdefgh"), 2);
        assert_eq!(ollama.estimate_tokens("abcdefgh"), 2);
    }
}

/// No-op memory provider — discards all writes, returns empty recalls.
pub struct NoopMemory;

impl MemoryProvider for NoopMemory {
    fn store(
        &self,
        _k: &str,
        _v: &str,
        _t: MemoryTier,
        _k2: MemoryKind,
    ) -> Result<(), MemoryError> {
        Ok(())
    }
    fn recall(&self, _q: &str, _l: usize) -> Result<Vec<MemoryEntry>, MemoryError> {
        Ok(Vec::new())
    }
    fn stats(&self) -> MemoryStats {
        MemoryStats::default()
    }
}

/// Production memory provider backed by the `touring` CLI.
///
/// Persists lessons and patterns across sessions via the touring daemon.
/// All methods degrade gracefully when the daemon is unreachable.
#[cfg(feature = "memory-integration")]
pub struct TouringMemoryProvider {
    /// Project root, used as working directory for CLI subprocess calls.
    project_root: std::path::PathBuf,
}

#[cfg(feature = "memory-integration")]
impl TouringMemoryProvider {
    /// Construct with the given project root.
    #[must_use]
    pub fn new(project_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

#[cfg(feature = "memory-integration")]
impl MemoryProvider for TouringMemoryProvider {
    fn store(
        &self,
        key: &str,
        value: &str,
        tier: MemoryTier,
        kind: MemoryKind,
    ) -> Result<(), MemoryError> {
        let tier_arg = match tier {
            MemoryTier::Semantic => "semantic",
            MemoryTier::Local => "local",
        };
        let kind_arg = match kind {
            MemoryKind::Lesson => "lesson",
            MemoryKind::Pattern => "pattern",
            MemoryKind::Insight => "insight",
            MemoryKind::Gotcha => "gotcha",
        };
        let status = std::process::Command::new("touring")
            .args([
                "memory", "store", key, value, "--tier", tier_arg, "--type", kind_arg,
            ])
            .current_dir(&self.project_root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| MemoryError(e.to_string()))?;
        if status.success() {
            Ok(())
        } else {
            Err(MemoryError(format!(
                "touring memory store exited with {status}"
            )))
        }
    }

    fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>, MemoryError> {
        let Ok(output) = std::process::Command::new("touring")
            .args(["memory", "recall", query, "-j"])
            .current_dir(&self.project_root)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
        else {
            return Ok(Vec::new()); // graceful degradation when daemon unreachable
        };

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let raw = String::from_utf8_lossy(&output.stdout);
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return Ok(Vec::new());
        };
        let Some(arr) = v.as_array() else {
            return Ok(Vec::new());
        };

        let entries = arr
            .iter()
            .filter_map(|item| {
                let key = item.get("key").and_then(|k| k.as_str())?;
                let value = item.get("value").and_then(|v| v.as_str()).unwrap_or("");
                let access_count = item
                    .get("access_count")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|c| u32::try_from(c).ok())
                    .unwrap_or(0);
                Some(MemoryEntry {
                    key: key.to_owned(),
                    value: value.to_owned(),
                    access_count,
                })
            })
            .take(limit)
            .collect();

        Ok(entries)
    }

    fn stats(&self) -> MemoryStats {
        let Ok(output) = std::process::Command::new("touring")
            .args(["memory", "stats", "-j"])
            .current_dir(&self.project_root)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
        else {
            return MemoryStats::default();
        };

        if !output.status.success() {
            return MemoryStats::default();
        }

        let raw = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str::<serde_json::Value>(&raw)
            .map(|v| MemoryStats {
                total_entries: v
                    .get("total_entries")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                semantic_entries: v
                    .get("semantic_entries")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
            })
            .unwrap_or_default()
    }
}

/// No-op RL reward sink.
pub struct NoopRlSink;

impl RlRewardSink for NoopRlSink {
    fn inject(&self, _tool: &str, _reward: NormalizedScore, _ctx: &str) {}
    fn ema(&self, _tool: &str) -> Option<f64> {
        None
    }
}

/// LinUCB-backed RL reward sink — wires `OnlineRLEngine` from touring-learning.
///
/// Activated under the `rl-integration` feature. Receives normalized reward
/// signals from the generator pipeline and feeds them into the EMA tracker,
/// enabling the generator to participate in the global RL reward loop.
#[cfg(feature = "rl-integration")]
pub struct LinUCBRewardSink {
    engine: std::sync::Mutex<touring_intelligence::rl::OnlineRLEngine>,
    qtable: std::sync::Mutex<touring_intelligence::rl::QTable>,
    linucb: std::sync::Mutex<touring_intelligence::rl::LinUCBBandit>,
}

#[cfg(feature = "rl-integration")]
impl Default for LinUCBRewardSink {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "rl-integration")]
impl LinUCBRewardSink {
    /// Construct with default `OnlineRLEngine`, `QTable`, and `LinUCBBandit`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: std::sync::Mutex::new(touring_intelligence::rl::OnlineRLEngine::with_defaults()),
            qtable: std::sync::Mutex::new(touring_intelligence::rl::QTable::new()),
            linucb: std::sync::Mutex::new(touring_intelligence::rl::LinUCBBandit::default()),
        }
    }
}

#[cfg(feature = "rl-integration")]
impl RlRewardSink for LinUCBRewardSink {
    fn inject(&self, tool: &str, reward: NormalizedScore, _ctx: &str) {
        let imm = touring_intelligence::rl::ImmediateReward {
            tool_name: tool.to_string(),
            accepted: reward.value() >= 0.5,
            latency_ms: 0,
            error_count: 0,
            cila_level: 0,
            file_type: 3, // "other" category
            quality_score: Some(reward.value()),
        };
        // Best-effort: skip if any lock is contended.
        if let (Ok(mut eng), Ok(mut qt), Ok(mut linucb)) =
            (self.engine.lock(), self.qtable.lock(), self.linucb.lock())
        {
            let _ = eng.process_reward(&imm, &mut qt, &mut linucb);
        }
    }

    fn ema(&self, _tool: &str) -> Option<f64> {
        self.engine.lock().ok().map(|eng| eng.ema_reward())
    }
}

/// No-op telemetry sink.
pub struct NoopTelemetry;

impl TelemetrySink for NoopTelemetry {
    fn record_lifecycle_transition(&self, _f: &str, _t: &str, _id: Uuid, _ns: u64) {}
    fn increment_counter(&self, _n: &'static str, _v: u64) {}
    fn record_histogram(&self, _n: &'static str, _v: f64) {}
}

/// No-op audit log.
pub struct NoopAuditLog;

impl AuditLog for NoopAuditLog {
    fn append(&self, _entry: crate::plan::result::AuditEntry) {}
}

/// Production audit log that emits structured events via the `tracing` subscriber.
///
/// Surfaces human overrides and security events in log streams without
/// coupling the generator crate to any external persistence dependency.
#[cfg(feature = "observability")]
pub struct TracingAuditLog;

#[cfg(feature = "observability")]
impl AuditLog for TracingAuditLog {
    fn append(&self, entry: crate::plan::result::AuditEntry) {
        tracing::info!(
            audit_actor = %entry.actor,
            audit_action = %entry.action,
            audit_plan_id = %entry.plan_id,
            audit_approved = entry.approved,
            audit_reason = ?entry.reason,
            "generator audit event"
        );
    }
}

// ── Closure type aliases (PLN2 section 8.1 — eliminates type_complexity) ─────

/// Infer semantically similar plans from the cognitive graph.
pub type SemanticGraphFn = Arc<dyn Fn(&GeneratorPlan) -> Option<Vec<SymbolRef>> + Send + Sync>;

/// Emit ACO pheromone signal for template selection RL.
pub type PheromoneUpdateFn = Arc<dyn Fn(&str, NormalizedScore) + Send + Sync>;

/// Retrieve cross-session plan similarity from the cognitive nexus.
pub type CognitiveNexusFn = Arc<dyn Fn(&str) -> Option<PlanSimilarityScore> + Send + Sync>;

/// Post-commit wiring gate — validates generated files for orphan exports.
pub type WiringGateFn =
    Arc<dyn Fn(&[RenderedFile], &str) -> Result<(), GenerateError> + Send + Sync>;

/// WASM sandbox execution for user-supplied templates.
pub type WasmSandboxFn = Arc<dyn Fn(&str, &str) -> Result<String, GenerateError> + Send + Sync>;

/// MCTS evaluation function for plan synthesis scoring.
pub type MctsEvalFn = Arc<dyn Fn(&str) -> NormalizedScore + Send + Sync>;

/// `DSPy` signature execution — routes to the active LLM cortex.
pub type DspySigFn = Arc<dyn Fn(&DspySignatureName, &DspyInputs) -> DspyOutputs + Send + Sync>;

/// Post-commit hook: upserts a generated artifact into `FileKnowledgeDB`.
///
/// `file_path` is the absolute path of the written file; `content` is the raw
/// bytes. Errors are **non-fatal** — callers log a warning and continue so that
/// a knowledge-db outage never blocks code generation.
///
/// Injected from `touring-hooks::knowledge_upsert_handler` or a custom closure
/// wrapping `touring-index::FileKnowledgeDB::upsert`.
pub type KnowledgeUpsertFn = Arc<dyn Fn(&str, &[u8]) -> Result<(), String> + Send + Sync>;

// ── Session lifecycle closures (P2 — touring session integration) ────────────

/// Start a touring session. Returns the `session_id`.
/// Injected from touring-server wrapping `touring session start <id> <type> <objective>`.
pub type SessionStartFn = Arc<dyn Fn(&str, &str) -> Result<String, String> + Send + Sync>;

/// Create a touring session checkpoint.
/// Injected from touring-server wrapping `touring session checkpoint <id> <data>`.
pub type SessionCheckpointFn = Arc<dyn Fn(&str, &str) -> Result<(), String> + Send + Sync>;

/// Assess a touring session and return quality score.
/// Injected from touring-server wrapping `touring session assess <id>`.
pub type SessionAssessFn = Arc<dyn Fn(&str) -> Result<f64, String> + Send + Sync>;

// ── Decompose bridge closure (P3 — task system integration) ─────────────────

/// Update a subtask status in the touring decompose DAG.
/// Args: (`task_id`, `subtask_id`, `status_string`).
/// Injected from touring-server wrapping `touring decompose update`.
pub type DecomposeUpdateFn = Arc<dyn Fn(&str, &str, &str) -> Result<(), String> + Send + Sync>;

/// Create a task with subtasks in the touring decompose DAG.
/// Args: (`task_type`, `description`) -> `task_id`.
/// Injected from touring-server wrapping `touring decompose create`.
pub type DecomposeCreateFn = Arc<dyn Fn(&str, &str) -> Result<String, String> + Send + Sync>;

// ── Concolic integration (security-gate / touring-offensive) ───────────────────

/// Concolic analysis function for pre-tool hook integration.
/// Injected from `ConcolicPreToolAdapter` via `touring-offensive`.
#[cfg(feature = "security-gate")]
pub type ConcolicAnalyzeFn =
    Arc<dyn Fn(&str) -> touring_offensive::concolic::ConcolicResult + Send + Sync + 'static>;

/// Central context passed through the entire plan executor pipeline.
///
/// PLN2 v2 — adds `symbol_index`, `fuzzy_index`, `schema_registry` (direct deps)
/// and 8 optional closure fields for cross-crate integration without circular deps.
/// All collaborators are `Arc`-wrapped for cheap clone across async tasks.
#[derive(Clone)]
pub struct GeneratorContext {
    // === Foundation ============================================================
    /// Project root for path resolution and path traversal checks.
    pub project_root: Utf8PathBuf,

    /// Direct in-process symbol index (touring-index).
    /// Used for fast, batched lookups that bypass the CLI subprocess.
    pub symbol_index: Arc<touring_intelligence::index::IncrementalIndex>,

    /// Fuzzy symbol name matcher — BK-tree or SIMD-accelerated.
    /// Provides O(log N) suggestions for missing symbol names in VGP reports.
    pub fuzzy_index: Arc<dyn FuzzyMatcher>,

    // === Core engines ==========================================================
    /// VGP engine — moka-backed cache + rayon `spawn_blocking`.
    pub vgp_engine: Arc<VgpEngine>,

    /// Template engine — `OnceLock<Tera>` pre-compiled.
    pub template_engine: Arc<TemplateEngine>,

    /// Speculate bridge — wraps `touring_code::ast::speculate_v2`.
    pub speculate_bridge: Arc<SpeculateBridge>,

    /// Plan schema migration registry — v1→v2 migration support.
    pub schema_registry: Arc<SchemaRegistry>,

    /// Registry of in-flight plans.
    pub plan_registry: Arc<PlanRegistry>,

    // === Injected via traits (no direct dep on hot crates) =====================
    /// Memory provider (store + recall).
    pub memory: Arc<dyn MemoryProvider>,

    /// LLM provider (multi-LLM capable).
    pub llm: Arc<dyn LlmProvider>,

    /// RL reward sink.
    pub rl: Arc<dyn RlRewardSink>,

    /// Telemetry sink.
    pub telemetry: Arc<dyn TelemetrySink>,

    // === Closures for cross-crate integration (PLN2 section 8.1) ===============
    //
    // These fields use `Option<Arc<dyn Fn(...)>>` to:
    //  a) Avoid circular Cargo.toml dependencies between crates
    //  b) Allow graceful degradation when integrations are not wired
    //  c) Enable injection by touring-server without coupling touring-generator
    //     to touring-hooks, touring-cognitive, touring-cortex, etc.
    /// Infer semantically similar plans from the cognitive graph.
    /// Injected from `touring-cognitive::SemanticGraph::add_node`.
    pub semantic_graph_fn: Option<SemanticGraphFn>,

    /// Emit ACO pheromone signal for template selection RL.
    /// Injected from `touring-simd::AcoPheromone::adjust_threshold_from_feedback`.
    pub pheromone_fn: Option<PheromoneUpdateFn>,

    /// Retrieve cross-session plan similarity score from the cognitive nexus.
    /// Injected from `touring-cognitive::CognitiveNexus::enrich_context`.
    pub cognitive_nexus_fn: Option<CognitiveNexusFn>,

    /// Post-commit wiring gate — validates generated files have no orphan exports.
    /// Injected from `touring-analysis::count_orphans` + wiring score threshold.
    pub wiring_gate_fn: Option<WiringGateFn>,

    /// Concolic analysis for pre-tool hook integration.
    /// Injected from `ConcolicPreToolAdapter` via touring-offensive.
    /// Active under feature `security-gate`.
    #[cfg(feature = "security-gate")]
    pub concolic_analyze_fn: Option<ConcolicAnalyzeFn>,

    /// Wave 19 (2026-04-18) — Health delta tracking for generated files.
    ///
    /// Closure-based bridge to `touring-hooks::health_delta` (avoids circular
    /// dep). When wired, the generator pipeline records pre-health BEFORE
    /// writing each artifact (read disk if file exists) and computes the
    /// signed delta AFTER write — closing the same RL/observability loop
    /// that `pre_edit`/`post_edit` already use for hand-edits. This ensures
    /// generator-emitted code is judged by the SAME quality criteria as
    /// CC-edited code.
    ///
    /// Signature:
    /// - `(file_path, source) -> Option<f32>` for `record_pre_health`
    /// - The other half (`compute_signals_delta`) is invoked via a
    ///   separate closure stored alongside; both are paired via the
    ///   `HealthDeltaPair` injection in `make_context`.
    pub health_delta_record_fn: Option<HealthDeltaRecordFn>,
    /// Wave 19 — paired with `health_delta_record_fn`. Computes the
    /// signed delta after write. Returns `Some(delta_value)` when both
    /// pre-record and compute succeed, `None` otherwise.
    pub health_delta_compute_fn: Option<HealthDeltaComputeFn>,

    /// WASM sandbox execution for user-supplied templates.
    /// Injected from `touring-wasm::WasmCacheManager`.
    pub wasm_sandbox_fn: Option<WasmSandboxFn>,

    /// MCTS evaluation function for plan synthesis scoring.
    /// Injected from `touring-cortex::MCTSCodeSynthesisHandler`.
    pub mcts_eval_fn: Option<MctsEvalFn>,

    /// `DSPy` signature execution — routes to the active LLM cortex.
    /// Injected from `touring-cortex::code_generation_sig`.
    pub dspy_sig_fn: Option<DspySigFn>,

    /// Post-commit hook: upserts generated artifact into `FileKnowledgeDB`.
    /// Injected from `touring-hooks::knowledge_upsert_handler`.
    /// Non-fatal on error — generation continues even if knowledge-db is unavailable.
    pub knowledge_upsert_fn: Option<KnowledgeUpsertFn>,

    // === Session lifecycle (P2 — touring session auto-integration) =============
    /// Start a touring session for this plan execution.
    pub session_start_fn: Option<SessionStartFn>,

    /// Checkpoint the touring session mid-pipeline.
    pub session_checkpoint_fn: Option<SessionCheckpointFn>,

    /// Assess session quality after pipeline completion.
    pub session_assess_fn: Option<SessionAssessFn>,

    // === Decompose bridge (P3 — task system integration) =======================
    /// Create a task in the touring decompose DAG for this plan.
    pub decompose_create_fn: Option<DecomposeCreateFn>,

    /// Update subtask status in the touring decompose DAG.
    pub decompose_update_fn: Option<DecomposeUpdateFn>,

    // === Capacity + audit ======================================================
    /// Bounded concurrency semaphore.
    pub backpressure: Arc<tokio::sync::Semaphore>,

    /// Resource capacity limits.
    pub capacity: CapacityLimits,

    /// Audit log for security events and human overrides.
    pub audit_log: Arc<dyn AuditLog>,

    // === Analysis gates (PLN2 — quality + health + blast check) ================
    /// Post-commit quality gate — validates antipatterns, unwraps, complexity.
    /// Active under feature `quality-gate`.
    #[cfg(feature = "quality-gate")]
    pub quality_gate_fn: Option<QualityGateFn>,

    /// Pre-built quality gate adapter — avoids double `QualityPipeline` init.
    /// Stored separately so `evaluate_quality_gate` can reuse it without cloning the Fn.
    #[cfg(feature = "quality-gate")]
    pub quality_gate_adapter: Option<QualityGateAdapter>,

    /// Post-commit health gate — validates project health via `touring e2e`.
    /// Active under feature `health-gate`.
    #[cfg(feature = "health-gate")]
    pub health_gate_fn: Option<HealthGateFn>,

    /// Post-commit enrichment trigger — fires `touring post-write` for each artifact,
    /// invoking the full daemon enrichment pipeline (Tantivy FTS, gotcha, wiring).
    /// Active under feature `enrichment-gate`.
    #[cfg(feature = "enrichment-gate")]
    pub enrichment_trigger_fn: Option<EnrichmentTriggerFn>,
}

impl GeneratorContext {
    /// Inject an RL reward signal via the configured sink.
    pub fn rl_reward(&self, tool: &str, reward: f64, context: &str) {
        self.rl
            .inject(tool, NormalizedScore::clamped(reward), context);
    }

    /// Record a lifecycle transition in telemetry.
    pub fn record_transition(&self, from: &str, to: &str, plan_id: Uuid, elapsed_ns: u64) {
        self.telemetry
            .record_lifecycle_transition(from, to, plan_id, elapsed_ns);
    }

    // ── Closure dispatch helpers (PLN2 section 8.1) ────────────────────────────

    /// Emit ACO pheromone signal. No-ops if `pheromone_fn` is not injected.
    pub fn pheromone_update(&self, tool: &str, score: NormalizedScore) {
        if let Some(f) = &self.pheromone_fn {
            f(tool, score);
        }
    }

    /// Retrieve cross-session plan similarity. Returns `None` if nexus not wired.
    #[must_use]
    pub fn evaluate_plan_similarity(&self, key: &str) -> Option<PlanSimilarityScore> {
        self.cognitive_nexus_fn.as_ref().and_then(|f| f(key))
    }

    /// Run the post-commit wiring gate. Returns `Ok(())` if gate is not wired.
    /// Stores the wiring baseline score in `WIRING_SCORES` for RL reward injection.
    ///
    /// # Errors
    /// Returns `GenerateError` if the wiring gate rejects the generated files.
    pub fn evaluate_wiring_gate(
        &self,
        files: &[RenderedFile],
        plan_id: &str,
    ) -> Result<(), GenerateError> {
        if let Some(f) = &self.wiring_gate_fn {
            f(files, plan_id)
        } else {
            Ok(())
        }
    }

    /// Wave 19 — Record pre-commit health for a single artifact.
    ///
    /// Reads the file from disk (so the closure operates on the OLD source,
    /// before this commit overwrites it) and caches its quality score in
    /// the shared `health_delta` cache. New files (missing on disk) are
    /// silently skipped — `compute_signals_delta` will treat them as
    /// first-observation (no delta).
    ///
    /// Non-blocking: returns immediately, regardless of result. Used by
    /// `executor::typestate::Speculated::commit()` per artifact.
    pub fn record_pre_health_for_artifact(&self, file_path: &str) {
        let Some(record_fn) = &self.health_delta_record_fn else {
            return;
        };
        // Read the OLD content from disk (about to be overwritten).
        let Ok(old_src) = std::fs::read_to_string(file_path) else {
            // New file — no pre-content to record. Skip.
            return;
        };
        let _ = record_fn(file_path, &old_src);
    }

    /// Wave 19 — Compute the signed health delta for a freshly-written
    /// artifact. Returns `(delta, is_regression, is_improvement)` when
    /// both pre-record (from `record_pre_health_for_artifact`) and
    /// post-compute succeed; `None` when no pre-record exists or the
    /// path is unsupported.
    ///
    /// The new source is the rendered content (already in memory — no
    /// disk read needed). Used by `commit()` AFTER `write_artifact_atomically`.
    #[must_use]
    pub fn compute_health_delta_for_artifact(
        &self,
        file_path: &str,
        new_source: &str,
    ) -> Option<(f32, bool, bool)> {
        let compute_fn = self.health_delta_compute_fn.as_ref()?;
        compute_fn(file_path, new_source)
    }

    /// Wave 19 — Builder method to wire the `health_delta` closure pair.
    /// Both record and compute are paired; injecting one without the other
    /// is a no-op (compute would never find pre-records and vice-versa).
    #[must_use]
    pub fn with_health_delta(
        mut self: Arc<Self>,
        record_fn: HealthDeltaRecordFn,
        compute_fn: HealthDeltaComputeFn,
    ) -> Arc<Self> {
        // Mutate via Arc::get_mut — only works when the Arc has a single
        // strong reference (typically right after `make_context` builds it).
        if let Some(inner) = Arc::get_mut(&mut self) {
            inner.health_delta_record_fn = Some(record_fn);
            inner.health_delta_compute_fn = Some(compute_fn);
        } else {
            tracing::warn!("with_health_delta: Arc has multiple refs, closures NOT wired");
        }
        self
    }

    /// Run the post-commit quality gate. Returns `Ok(())` if gate is not wired.
    /// Also computes and caches the average quality score in `QUALITY_SCORES`
    /// keyed by `plan_id` for multidimensional RL reward injection.
    ///
    /// # Errors
    /// Returns `GenerateError` if the quality gate function returns an error.
    #[cfg(feature = "quality-gate")]
    pub fn evaluate_quality_gate(
        &self,
        files: &[RenderedFile],
        plan_id: &str,
    ) -> Result<(), GenerateError> {
        if let Some(ref f) = self.quality_gate_fn {
            // Compute avg score and cache it before calling the gate (which may return early).
            {
                let avg = self
                    .quality_gate_adapter
                    .as_ref()
                    .map_or(0.0, |a| a.average_score(files));
                if avg > 0.0 {
                    QUALITY_SCORES.insert(plan_id.to_string(), avg);
                }
            }
            f(files)
        } else {
            Ok(())
        }
    }

    /// Run the post-commit health gate. Returns `Ok(())` if gate is not wired.
    /// Health check is advisory only — failures are logged but do NOT block.
    ///
    /// # Errors
    /// Returns `GenerateError` if the health gate function returns an error.
    #[cfg(feature = "health-gate")]
    pub fn evaluate_health_gate(&self, project_root: &str) -> Result<(), GenerateError> {
        if let Some(ref f) = self.health_gate_fn {
            f(project_root)
        } else {
            Ok(())
        }
    }

    /// Fire the daemon enrichment pipeline for generated artifacts.
    ///
    /// Calls `touring post-write` for each path, which triggers:
    /// - Tantivy FTS indexing
    /// - Gotcha detection
    /// - Wiring update
    /// - Knowledge DB enrichment
    ///
    /// Non-blocking — runs in a `tokio::spawn` task.
    #[cfg(feature = "enrichment-gate")]
    pub fn trigger_enrichment(&self, paths: &[String], project_root: &str) {
        if let Some(ref f) = self.enrichment_trigger_fn {
            f(paths, project_root);
        }
    }

    /// Execute code in the WASM sandbox. Returns an empty string if not wired.
    ///
    /// # Errors
    /// Returns `GenerateError` if WASM execution fails.
    pub fn sandbox_execute(&self, code: &str, lang: &str) -> Result<String, GenerateError> {
        if let Some(f) = &self.wasm_sandbox_fn {
            f(code, lang)
        } else {
            Ok(String::new())
        }
    }

    /// Score a plan state via MCTS. Returns `NormalizedScore::ZERO` if not wired.
    #[must_use]
    pub fn mcts_evaluate(&self, state: &str) -> NormalizedScore {
        self.mcts_eval_fn
            .as_ref()
            .map_or(NormalizedScore::ZERO, |f| f(state))
    }

    /// Execute a `DSPy` signature. Returns empty outputs if not wired.
    pub fn execute_dspy(&self, sig: &DspySignatureName, inputs: &DspyInputs) -> DspyOutputs {
        self.dspy_sig_fn
            .as_ref()
            .map_or_else(DspyOutputs::new, |f| f(sig, inputs))
    }

    /// Invoke the post-commit knowledge upsert hook, if wired.
    ///
    /// Non-fatal: logs a warning on error and continues so that a
    /// `FileKnowledgeDB` outage never blocks artifact generation.
    ///
    /// # Arguments
    /// * `file_path` — absolute path of the written artifact
    /// * `content`   — raw bytes written to disk
    pub fn evaluate_knowledge_upsert(&self, file_path: &str, content: &[u8]) {
        if let Some(ref f) = self.knowledge_upsert_fn
            && let Err(e) = f(file_path, content)
        {
            tracing::warn!(
                file_path = file_path,
                error = %e,
                "knowledge_upsert_fn failed — continuing (non-fatal)"
            );
        }
    }

    // ── Session lifecycle dispatch (P2) ────────────────────────────────────────

    /// Start a touring session for this plan. Returns `session_id` or `None` if not wired.
    #[must_use]
    pub fn session_start(&self, plan_id: &str, objective: &str) -> Option<String> {
        self.session_start_fn
            .as_ref()
            .and_then(|f| match f(plan_id, objective) {
                Ok(id) => Some(id),
                Err(e) => {
                    tracing::warn!(plan_id, error = %e, "session_start_fn failed");
                    None
                }
            })
    }

    /// Checkpoint the touring session. Non-fatal on error.
    pub fn session_checkpoint(&self, session_id: &str, data: &str) {
        if let Some(f) = &self.session_checkpoint_fn
            && let Err(e) = f(session_id, data)
        {
            tracing::warn!(session_id, error = %e, "session_checkpoint_fn failed");
        }
    }

    /// Assess session quality. Returns score or `None` if not wired.
    #[must_use]
    pub fn session_assess(&self, session_id: &str) -> Option<f64> {
        self.session_assess_fn
            .as_ref()
            .and_then(|f| match f(session_id) {
                Ok(score) => Some(score),
                Err(e) => {
                    tracing::warn!(session_id, error = %e, "session_assess_fn failed");
                    None
                }
            })
    }

    // ── Decompose bridge dispatch (P3) ────────────────────────────────────────

    /// Create a task in the decompose DAG. Returns `task_id` or `None`.
    #[must_use]
    pub fn decompose_create_task(&self, task_type: &str, description: &str) -> Option<String> {
        self.decompose_create_fn.as_ref().and_then(|f| {
            match f(task_type, description) {
                Ok(id) => Some(id),
                Err(e) => {
                    tracing::warn!(task_type, error = %e, "decompose_create_fn failed");
                    // R7: Penalize decompose bridge failures so RL learns the pattern.
                    self.rl_reward("decompose_bridge", -0.5, "create_failed");
                    None
                }
            }
        })
    }

    /// Update a subtask status in the decompose DAG. Non-fatal on error.
    pub fn decompose_update_status(&self, task_id: &str, subtask_id: &str, status: &str) {
        if let Some(f) = &self.decompose_update_fn
            && let Err(e) = f(task_id, subtask_id, status)
        {
            tracing::warn!(task_id, subtask_id, status, error = %e, "decompose_update_fn failed");
            // R7: Penalize decompose bridge failures so RL learns the pattern.
            self.rl_reward("decompose_bridge", -0.3, "update_failed");
        }
    }

    /// Find semantically similar plans via the cognitive graph.
    /// Returns an empty vec if `semantic_graph_fn` is not wired.
    #[must_use]
    pub fn find_similar_plans(&self, plan: &GeneratorPlan) -> Vec<SymbolRef> {
        self.semantic_graph_fn
            .as_ref()
            .and_then(|f| f(plan))
            .unwrap_or_default()
    }

    /// Builder that injects a `MctsEvalFn` into an existing `GeneratorContext`.
    ///
    /// Returns a new `Arc<GeneratorContext>` with `mcts_eval_fn` set to the
    /// provided closure, leaving all other fields unchanged (shallow Arc clone).
    #[cfg(feature = "mcts-synthesis")]
    #[must_use]
    pub fn with_mcts_eval(self: Arc<Self>, mcts_fn: MctsEvalFn) -> Arc<Self> {
        let mut ctx = Arc::try_unwrap(self).unwrap_or_else(|arc| (*arc).clone());
        ctx.mcts_eval_fn = Some(mcts_fn);
        Arc::new(ctx)
    }

    /// Construct a production context with injected fuzzy and RL providers.
    ///
    /// Called by `touring-server::generator_tools::make_context()`. Accepts real
    /// implementations of `FuzzyMatcher` and `RlRewardSink` while keeping
    /// all other providers at their no-op defaults (same as `for_testing`).
    ///
    /// Closure fields `pheromone_fn`, `wiring_gate_fn`, and `knowledge_upsert_fn`
    /// are populated; the remaining four (`semantic_graph_fn`, `cognitive_nexus_fn`,
    /// `mcts_eval_fn`, `dspy_sig_fn`) stay `None` pending cross-crate adapters.
    ///
    /// Returns `Self` (not `Arc<Self>`) — callers wrap with `Arc::new()` after
    /// injecting all closure fields via `&mut self` references. This eliminates
    /// the fragile `Arc::get_mut` pattern where only the FIRST `get_mut` succeeds
    /// when multiple conditional blocks try to mutate the same Arc.
    #[must_use]
    pub fn with_closures(
        fuzzy_index: Arc<dyn FuzzyMatcher>,
        rl: Arc<dyn RlRewardSink>,
        pheromone_fn: Option<PheromoneUpdateFn>,
        wiring_gate_fn: Option<WiringGateFn>,
        knowledge_upsert_fn: Option<KnowledgeUpsertFn>,
    ) -> Self {
        let metrics: Arc<dyn TelemetrySink> = Arc::new(NoopTelemetry);
        let file_cache = Arc::new(tokio::sync::RwLock::new(
            touring_intelligence::index::FileCache::new(),
        ));
        let symbol_index = Arc::new(touring_intelligence::index::IncrementalIndex::new(
            Arc::clone(&file_cache),
        ));
        Self {
            project_root: Utf8PathBuf::from(
                std::env::var("TOURING_PROJECT_ROOT")
                    .unwrap_or_else(|_| "/tmp/touring".to_string()),
            ),
            symbol_index: Arc::clone(&symbol_index),
            fuzzy_index,
            vgp_engine: Arc::new(
                VgpEngine::with_subprocess(Arc::clone(&metrics))
                    .with_index(Arc::clone(&symbol_index)),
            ),
            template_engine: Arc::new(TemplateEngine::new(Arc::clone(&metrics))),
            speculate_bridge: Arc::new(SpeculateBridge::new(Arc::clone(&metrics))),
            schema_registry: Arc::new(SchemaRegistry::new("2.0.0")),
            plan_registry: Arc::new(PlanRegistry::new()),
            memory: Arc::new(NoopMemory),
            // B-W2/A8 (2026-06-13): production selects the LLM provider from the
            // environment (`TOURING_LLM_PROVIDER`); degrades to `NoopLlm` when the
            // `llm-http` feature is off or no provider is configured.
            #[cfg(feature = "llm-http")]
            llm: llm_provider_from_env(),
            #[cfg(not(feature = "llm-http"))]
            llm: Arc::new(NoopLlm),
            rl,
            telemetry: Arc::clone(&metrics),
            semantic_graph_fn: None,
            pheromone_fn,
            cognitive_nexus_fn: None,
            wiring_gate_fn,
            // Wave 19: health_delta closures default to None; wired post-construction via
            // `with_health_delta(...)` builder when callers want the dynamic-quality loop.
            health_delta_record_fn: None,
            health_delta_compute_fn: None,
            wasm_sandbox_fn: None,
            mcts_eval_fn: None,
            dspy_sig_fn: None,
            knowledge_upsert_fn,
            session_start_fn: None,
            session_checkpoint_fn: None,
            session_assess_fn: None,
            decompose_create_fn: None,
            decompose_update_fn: None,
            #[cfg(feature = "quality-gate")]
            quality_gate_fn: None,
            #[cfg(feature = "quality-gate")]
            quality_gate_adapter: None,
            #[cfg(feature = "health-gate")]
            health_gate_fn: None,
            #[cfg(feature = "enrichment-gate")]
            enrichment_trigger_fn: None,
            #[cfg(feature = "security-gate")]
            concolic_analyze_fn: None,
            backpressure: Arc::new(tokio::sync::Semaphore::new(64)),
            capacity: CapacityLimits::default(),
            audit_log: Arc::new(NoopAuditLog),
        }
    }

    /// Construct a minimal context wired with all no-op providers.
    ///
    /// Use in unit tests and integration tests where real providers are not needed.
    /// All closure fields are `None` — inject real implementations for integration tests.
    #[must_use]
    pub fn for_testing() -> Arc<Self> {
        let metrics: Arc<dyn TelemetrySink> = Arc::new(NoopTelemetry);
        let file_cache = Arc::new(tokio::sync::RwLock::new(
            touring_intelligence::index::FileCache::new(),
        ));
        let symbol_index = Arc::new(touring_intelligence::index::IncrementalIndex::new(
            Arc::clone(&file_cache),
        ));
        Arc::new(Self {
            project_root: Utf8PathBuf::from("/tmp/touring-generator-test"),
            symbol_index: Arc::clone(&symbol_index),
            fuzzy_index: Arc::new(NoopFuzzyMatcher),
            vgp_engine: Arc::new(
                VgpEngine::with_subprocess(Arc::clone(&metrics))
                    .with_index(Arc::clone(&symbol_index)),
            ),
            template_engine: Arc::new(TemplateEngine::new(Arc::clone(&metrics))),
            speculate_bridge: Arc::new(SpeculateBridge::new(Arc::clone(&metrics))),
            schema_registry: Arc::new(SchemaRegistry::new("2.0.0")),
            plan_registry: Arc::new(PlanRegistry::new()),
            memory: Arc::new(NoopMemory),
            llm: Arc::new(NoopLlm),
            rl: Arc::new(NoopRlSink),
            telemetry: Arc::clone(&metrics),
            // Closure fields — None in testing; inject real closures in production
            semantic_graph_fn: None,
            pheromone_fn: None,
            cognitive_nexus_fn: None,
            wiring_gate_fn: None,
            // Wave 19: health_delta closures (paired)
            health_delta_record_fn: None,
            health_delta_compute_fn: None,
            wasm_sandbox_fn: None,
            mcts_eval_fn: None,
            dspy_sig_fn: None,
            knowledge_upsert_fn: None,
            // Session lifecycle (P2)
            session_start_fn: None,
            session_checkpoint_fn: None,
            session_assess_fn: None,
            // Decompose bridge (P3)
            decompose_create_fn: None,
            decompose_update_fn: None,
            #[cfg(feature = "quality-gate")]
            quality_gate_fn: None,
            #[cfg(feature = "quality-gate")]
            quality_gate_adapter: None,
            #[cfg(feature = "health-gate")]
            health_gate_fn: None,
            #[cfg(feature = "enrichment-gate")]
            enrichment_trigger_fn: None,
            #[cfg(feature = "security-gate")]
            concolic_analyze_fn: None,
            backpressure: Arc::new(tokio::sync::Semaphore::new(64)),
            capacity: CapacityLimits::default(),
            audit_log: Arc::new(NoopAuditLog),
        })
    }
}

#[cfg(all(test, feature = "quality-gate"))]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod quality_gate_multilang_tests {
    use super::*;
    use crate::plan::result::{FileAction, RenderedFile};
    use touring_analysis::engine::AnalysisConfig;

    fn rf(path: &str, content: &str) -> RenderedFile {
        RenderedFile::new(path, content.to_string(), FileAction::Created)
    }

    fn strict_gate() -> QualityGateAdapter {
        // Relax thresholds so multi-lang tests exercise the dispatch (not just unwrap count).
        QualityGateAdapter::new(AnalysisConfig::standard()).with_thresholds(0, 0, 0.0)
    }

    // ── detect_language — dispatch coverage ───────────────────────────────────

    #[test]
    fn detect_language_covers_all_eight_supported() {
        let cases = &[
            ("src/a.rs", "rust"),
            ("src/a.py", "python"),
            ("src/a.pyi", "python"),
            ("src/a.ts", "typescript"),
            ("src/a.tsx", "typescript"),
            ("src/a.js", "javascript"),
            ("src/a.mjs", "javascript"),
            ("src/a.cjs", "javascript"),
            ("src/a.jsx", "javascript"),
            ("src/a.go", "go"),
            ("src/a.c", "c"),
            ("src/a.h", "c"),
            ("src/a.cpp", "cpp"),
            ("src/a.cc", "cpp"),
            ("src/a.cxx", "cpp"),
            ("src/a.hpp", "cpp"),
            ("src/a.java", "java"),
        ];
        for (path, expected) in cases {
            assert_eq!(
                QualityGateAdapter::detect_language(path),
                Some(*expected),
                "lang detection mismatch for {path}",
            );
        }
    }

    #[test]
    fn detect_language_returns_none_for_unsupported() {
        for path in &["README.md", "Cargo.toml", "x.yaml", "x.unknown", "noext"] {
            assert_eq!(
                QualityGateAdapter::detect_language(path),
                None,
                "expected None for {path}",
            );
        }
    }

    #[test]
    fn detect_language_is_case_insensitive() {
        assert_eq!(QualityGateAdapter::detect_language("File.RS"), Some("rust"));
        assert_eq!(
            QualityGateAdapter::detect_language("X.TSX"),
            Some("typescript")
        );
        assert_eq!(QualityGateAdapter::detect_language("Y.JAVA"), Some("java"));
    }

    // ── extract_inputs — filtering + language string ─────────────────────────

    #[test]
    fn extract_inputs_filters_unsupported_files() {
        let files = vec![
            rf("code.rs", "fn main() {}"),
            rf("README.md", "# docs"),
            rf("script.py", "def main(): pass"),
            rf("manifest.toml", "[package]"),
            rf("ui.tsx", "export const X = 1;"),
        ];
        let inputs = QualityGateAdapter::extract_inputs(&files);
        let langs: Vec<&str> = inputs.iter().map(|(_p, _c, l)| *l).collect();
        assert_eq!(langs.len(), 3);
        assert!(langs.contains(&"rust"));
        assert!(langs.contains(&"python"));
        assert!(langs.contains(&"typescript"));
    }

    #[test]
    fn extract_inputs_empty_for_non_code_only() {
        let files = vec![rf("README.md", "docs"), rf("x.unknown", "raw")];
        assert!(QualityGateAdapter::extract_inputs(&files).is_empty());
    }

    // ── check() — multi-lang antipattern detection ───────────────────────────

    #[test]
    fn check_rejects_python_bare_except() {
        let gate = strict_gate();
        let files = vec![rf(
            "bad.py",
            "def parse(s):\n    try:\n        return int(s)\n    except:\n        return None\n",
        )];
        let err = gate.check(&files).expect_err("bare except must fail gate");
        let msg = format!("{err}");
        assert!(msg.contains("[python]"), "lang tag missing: {msg}");
    }

    #[test]
    fn check_rejects_typescript_any_cast() {
        let gate = strict_gate();
        let files = vec![rf(
            "bad.ts",
            "export function widen(x: unknown): number {\n  return x as any;\n}\n",
        )];
        let err = gate.check(&files).expect_err("`as any` must fail gate");
        let msg = format!("{err}");
        assert!(msg.contains("[typescript]"), "lang tag missing: {msg}");
    }

    #[test]
    fn check_rejects_tsx_console_log() {
        let gate = strict_gate();
        let files = vec![rf(
            "app.tsx",
            "export const App = () => {\n  console.log('debug');\n  return null;\n};\n",
        )];
        let err = gate.check(&files).expect_err(".tsx should still flag");
        let msg = format!("{err}");
        assert!(
            msg.contains("[typescript]"),
            "tsx routed to typescript: {msg}"
        );
    }

    #[test]
    fn check_rejects_javascript_var() {
        let gate = strict_gate();
        let files = vec![rf(
            "bad.js",
            "function add(a, b) {\n  var sum = a + b;\n  return sum;\n}\n",
        )];
        let err = gate.check(&files).expect_err("`var` must fail gate");
        let msg = format!("{err}");
        assert!(msg.contains("[javascript]"), "lang tag missing: {msg}");
    }

    #[test]
    fn check_rejects_go_panic() {
        let gate = strict_gate();
        let files = vec![rf(
            "bad.go",
            "package main\nfunc crash() {\n  panic(\"boom\")\n}\n",
        )];
        let err = gate.check(&files).expect_err("Go panic must fail gate");
        let msg = format!("{err}");
        assert!(msg.contains("[go]"), "lang tag missing: {msg}");
    }

    #[test]
    fn check_rejects_java_broad_catch() {
        let gate = strict_gate();
        let files = vec![rf(
            "Bad.java",
            "public class Bad {\n  void run() {\n    try {} catch(Exception e) {}\n  }\n}\n",
        )];
        let err = gate.check(&files).expect_err("broad catch must fail gate");
        let msg = format!("{err}");
        assert!(msg.contains("[java]"), "lang tag missing: {msg}");
    }

    #[test]
    fn check_accepts_clean_python() {
        // Lenient thresholds so accept path is exercised without tripping on score floor.
        let gate =
            QualityGateAdapter::new(AnalysisConfig::standard()).with_thresholds(100, 100, 0.0);
        let files = vec![rf(
            "good.py",
            "def add(a: int, b: int) -> int:\n    return a + b\n",
        )];
        gate.check(&files).expect("clean python must pass");
    }

    #[test]
    fn check_accepts_clean_typescript() {
        let gate =
            QualityGateAdapter::new(AnalysisConfig::standard()).with_thresholds(100, 100, 0.0);
        let files = vec![rf(
            "good.ts",
            "export function add(a: number, b: number): number {\n  return a + b;\n}\n",
        )];
        gate.check(&files).expect("clean ts must pass");
    }

    #[test]
    fn check_skips_unknown_extension_silently() {
        let gate = strict_gate();
        let files = vec![rf(
            "notes.md",
            "# unsafe { panic!() } var console.log panic(",
        )];
        gate.check(&files)
            .expect("unknown ext must not trigger gate");
    }

    // ── Semantic Fusion Gate (Wave 7 — syn + tree-sitter) ────────────────────

    #[test]
    fn semantic_threshold_default_is_disabled() {
        // New defaults must keep existing callers green: semantic gate OFF by default.
        let gate = QualityGateAdapter::new(AnalysisConfig::standard());
        let files = vec![rf(
            "abstract.rs",
            // Generic-heavy but safe code — would lower health_score
            // but semantic gate disabled should not trip it.
            "pub fn deep<'a, T, U, V>(x: &'a T, y: U, z: V) -> &'a T \
             where T: Send + Sync + Clone + std::fmt::Debug + 'static, \
                   U: IntoIterator<Item = T> + Default + Copy, \
                   V: From<T> + Into<U> { x }\n",
        )];
        gate.check(&files)
            .expect("semantic gate disabled must pass abstract rust");
    }

    #[test]
    fn semantic_threshold_rejects_unsafe_rust() {
        let gate = QualityGateAdapter::new(AnalysisConfig::standard())
            .with_thresholds(100, 100, 0.0) // relax non-semantic thresholds
            .with_semantic_threshold(0.9); // strict semantic floor
        let files = vec![rf(
            "unsafe.rs",
            "pub unsafe fn dangerous() {\n    unsafe { std::ptr::null::<u8>(); }\n}\n",
        )];
        let err = gate
            .check(&files)
            .expect_err("unsafe must fail semantic gate");
        let msg = format!("{err}");
        assert!(
            msg.contains("[rust-semantic]"),
            "expected [rust-semantic] tag, got: {msg}",
        );
        assert!(msg.contains("unsafe="), "must report unsafe count: {msg}");
    }

    #[test]
    fn semantic_threshold_accepts_clean_rust() {
        let gate = QualityGateAdapter::new(AnalysisConfig::standard())
            .with_thresholds(100, 100, 0.0)
            .with_semantic_threshold(0.9);
        let files = vec![rf(
            "clean.rs",
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        )];
        gate.check(&files)
            .expect("trivial safe rust must pass semantic gate");
    }

    #[test]
    fn semantic_threshold_rejects_dynamic_python() {
        // P-D parity: the semantic gate now inspects non-Rust files. A Python
        // file drenched in `eval` (the cross-language analog of `unsafe`) must
        // be rejected exactly as unsafe Rust is.
        let gate = QualityGateAdapter::new(AnalysisConfig::standard())
            .with_thresholds(100, 100, 0.0)
            .with_semantic_threshold(0.9);
        let files = vec![rf(
            "dynamic.py",
            "def run(x):\n    return eval(x) + eval(x) + eval(x) + eval(x)\n",
        )];
        let err = gate
            .check(&files)
            .expect_err("eval-heavy python must fail semantic gate");
        let msg = format!("{err}");
        assert!(
            msg.contains("[python-semantic]"),
            "expected [python-semantic] tag, got: {msg}",
        );
        assert!(
            msg.contains("dynamic_escapes="),
            "must report dynamic escape count: {msg}",
        );
    }

    #[test]
    fn semantic_threshold_accepts_clean_typescript() {
        // Clean, fully-typed TypeScript clears the same bar — parity means the
        // gate is polyglot, not a blanket non-Rust rejection.
        let gate = QualityGateAdapter::new(AnalysisConfig::standard())
            .with_thresholds(100, 100, 0.0)
            .with_semantic_threshold(0.9);
        let files = vec![rf(
            "clean.ts",
            "export function add(a: number, b: number): number { return a + b; }\n",
        )];
        gate.check(&files)
            .expect("clean typed typescript must pass semantic gate");
    }

    #[test]
    fn semantic_threshold_accepts_clean_python() {
        // P-D parity: the gate now INSPECTS Python (no longer a non-Rust no-op).
        // A clean, fully-typed Python function with no dynamic escapes is
        // healthy and passes — proving the gate is polyglot, not a bypass.
        let gate = QualityGateAdapter::new(AnalysisConfig::standard())
            .with_thresholds(100, 100, 0.0)
            .with_semantic_threshold(0.9);
        let files = vec![rf(
            "code.py",
            "def add(a: int, b: int) -> int:\n    return a + b\n",
        )];
        gate.check(&files)
            .expect("clean typed python must pass the polyglot semantic gate");
    }

    #[test]
    fn semantic_threshold_tolerates_unparseable_rust() {
        // from_source returns None on parse failure — gate must not crash,
        // instead skip the file (conservative fail-open for unparseable).
        let gate = QualityGateAdapter::new(AnalysisConfig::standard())
            .with_thresholds(100, 100, 0.0)
            .with_semantic_threshold(0.9);
        let files = vec![rf(
            "broken.rs",
            "this is {{{{ not valid rust at all &&&& ::::\n",
        )];
        gate.check(&files)
            .expect("unparseable rust must not crash the gate");
    }

    #[test]
    fn average_score_fuses_semantic_for_rust() {
        // With fusion on, per-file score = (tree_sitter + syn_health) / 2.
        // Simple Rust: tree_sitter ~ 1.0, syn_health ~ 1.0 → avg ~ 1.0.
        let gate = QualityGateAdapter::new(AnalysisConfig::standard())
            .with_thresholds(100, 100, 0.0)
            .with_semantic_threshold(0.1);
        let files = vec![rf("simple.rs", "pub fn ok() -> i32 { 42 }\n")];
        let fused = gate.average_score(&files);
        assert!(
            fused > 0.5,
            "simple rust must score > 0.5 in fused mode, got {fused}"
        );
        assert!(
            fused <= 1.0,
            "fused score must be bounded by 1.0, got {fused}"
        );
    }

    #[test]
    fn average_score_ignores_semantic_for_non_rust() {
        // Fusion enabled but file is TypeScript — should only use tree-sitter path.
        let gate = QualityGateAdapter::new(AnalysisConfig::standard())
            .with_thresholds(100, 100, 0.0)
            .with_semantic_threshold(0.9);
        let files = vec![rf(
            "ok.ts",
            "export function add(a: number, b: number): number { return a + b; }\n",
        )];
        let score = gate.average_score(&files);
        assert!(score > 0.0, "ts must produce a positive tree-sitter score");
    }

    // ── average_score — multi-lang aggregation ───────────────────────────────

    #[test]
    fn average_score_aggregates_across_languages() {
        let gate =
            QualityGateAdapter::new(AnalysisConfig::standard()).with_thresholds(100, 100, 0.0);
        let files = vec![
            rf("a.rs", "fn ok() -> i32 { 1 }\n"),
            rf("b.py", "def ok() -> int:\n    return 1\n"),
            rf("c.ts", "export const ok = (): number => 1;\n"),
        ];
        let avg = gate.average_score(&files);
        assert!(
            avg > 0.0,
            "average must be positive for 3 clean files, got {avg}"
        );
        assert!(avg <= 1.0, "average must be bounded by 1.0, got {avg}");
    }
}
