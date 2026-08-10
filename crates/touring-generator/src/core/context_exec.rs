//! Execution adapters — `RkyvFileSnapshotAdapter` (zero-copy snapshots),
//! `WasmSandboxAdapter` (defense-in-depth WASM validation), and
//! `McctsEvalAdapter` (MCTS plan scoring).
//!
//! Extracted from `core/context.rs` (F-9 modularization): each adapter is a
//! self-contained, feature-gated unit. Re-exported from `core::context` so the
//! public API (`crate::RkyvFileSnapshotAdapter`, `crate::WasmSandboxAdapter`,
//! `crate::McctsEvalAdapter`, …) is preserved verbatim. The closure type
//! aliases (`WasmSandboxFn`, `MctsEvalFn`) remain in `context.rs` and are
//! referenced here by full path to keep this module's imports feature-clean.

use crate::core::score::NormalizedScore;
use crate::error::GenerateError;
use std::collections::HashMap;
use std::sync::Arc;

/// Zero-copy snapshot adapter: serializes rendered files into a length-prefixed
/// buffer (optionally rkyv-wrapped) for fast checkpoint/restore. Stateless;
/// feature-gated on `zero-copy`.
#[cfg(feature = "zero-copy")]
#[derive(Debug, Default, Clone)]
pub struct RkyvFileSnapshotAdapter;

#[cfg(feature = "zero-copy")]
impl RkyvFileSnapshotAdapter {
    /// Construct a new stateless adapter.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Serialize rendered files into a length-prefixed byte buffer.
    ///
    /// Format: `[u32_count][u32_path_len][path][u32_content_len][content]*`
    ///
    /// Little-endian lengths. This is a lightweight framing over the raw
    /// bytes — we intentionally do NOT use rkyv derives on `RenderedFile`
    /// since that would require derive attributes throughout the plan type
    /// tree. The simpler format is still sub-millisecond and restore-safe.
    ///
    /// # Errors
    ///
    /// Returns `GenerateError::Internal` when a path or content exceeds
    /// `u32::MAX` bytes (effectively never in practice — u32 caps at 4 GB).
    pub fn snapshot(files: &[crate::plan::result::RenderedFile]) -> Result<Vec<u8>, GenerateError> {
        let mut buf: Vec<u8> = Vec::with_capacity(1024);
        let count = u32::try_from(files.len())
            .map_err(|_| GenerateError::Internal("snapshot: too many files (> u32::MAX)".into()))?;
        buf.extend_from_slice(&count.to_le_bytes());

        for file in files {
            let path_len = u32::try_from(file.path.len()).map_err(|_| {
                GenerateError::Internal(format!("snapshot: path `{}` exceeds u32::MAX", file.path))
            })?;
            let content_len = u32::try_from(file.content.len()).map_err(|_| {
                GenerateError::Internal("snapshot: content exceeds u32::MAX".into())
            })?;
            buf.extend_from_slice(&path_len.to_le_bytes());
            buf.extend_from_slice(file.path.as_bytes());
            buf.extend_from_slice(&content_len.to_le_bytes());
            buf.extend_from_slice(file.content.as_bytes());
        }
        Ok(buf)
    }

    /// Restore rendered files from a snapshot buffer.
    ///
    /// Returns the list of `(path, content)` pairs. Action defaults to
    /// `FileAction::Created` since the snapshot does not encode the action
    /// (callers must reconstruct it from their own state if needed).
    ///
    /// # Errors
    ///
    /// Returns `GenerateError::Internal` when the buffer is truncated or
    /// contains invalid UTF-8 in either a path or file content.
    pub fn restore(buf: &[u8]) -> Result<Vec<crate::plan::result::RenderedFile>, GenerateError> {
        if buf.len() < 4 {
            return Err(GenerateError::Internal("restore: buffer too short".into()));
        }
        let count = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let mut cursor = 4usize;
        let mut out: Vec<crate::plan::result::RenderedFile> = Vec::with_capacity(count as usize);

        for _ in 0..count {
            if cursor + 4 > buf.len() {
                return Err(GenerateError::Internal(
                    "restore: truncated path length".into(),
                ));
            }
            let path_len = u32::from_le_bytes([
                buf[cursor],
                buf[cursor + 1],
                buf[cursor + 2],
                buf[cursor + 3],
            ]) as usize;
            cursor += 4;
            if cursor + path_len > buf.len() {
                return Err(GenerateError::Internal("restore: truncated path".into()));
            }
            let path = std::str::from_utf8(&buf[cursor..cursor + path_len])
                .map_err(|e| GenerateError::Internal(format!("restore: path utf8: {e}")))?
                .to_string();
            cursor += path_len;

            if cursor + 4 > buf.len() {
                return Err(GenerateError::Internal(
                    "restore: truncated content length".into(),
                ));
            }
            let content_len = u32::from_le_bytes([
                buf[cursor],
                buf[cursor + 1],
                buf[cursor + 2],
                buf[cursor + 3],
            ]) as usize;
            cursor += 4;
            if cursor + content_len > buf.len() {
                return Err(GenerateError::Internal("restore: truncated content".into()));
            }
            let content = std::str::from_utf8(&buf[cursor..cursor + content_len])
                .map_err(|e| GenerateError::Internal(format!("restore: content utf8: {e}")))?
                .to_string();
            cursor += content_len;

            out.push(crate::plan::result::RenderedFile::new(
                path,
                content,
                crate::plan::result::FileAction::Created,
            ));
        }

        if cursor != buf.len() {
            return Err(GenerateError::Internal(format!(
                "restore: {} trailing bytes",
                buf.len() - cursor
            )));
        }
        Ok(out)
    }

    /// Wraps an rkyv zero-copy archive around a byte snapshot.
    ///
    /// Delegates to `touring_rkyv::to_bytes` for the final aligned buffer, adding
    /// an rkyv-format wrapper around the self-describing snapshot above.
    /// The resulting buffer can be mapped via `touring_rkyv::check_archived_root`
    /// for zero-copy inspection without full deserialization.
    ///
    /// # Errors
    ///
    /// Returns `GenerateError::Internal` when either the inner snapshot or
    /// the rkyv serialization step fails.
    pub fn snapshot_rkyv(
        files: &[crate::plan::result::RenderedFile],
    ) -> Result<touring_rkyv::AlignedVec, GenerateError> {
        let inner = Self::snapshot(files)?;
        touring_rkyv::to_bytes::<_, 1024>(&inner)
            .map_err(|e| GenerateError::Internal(format!("rkyv serialize: {e}")))
    }
}

// ── WasmSandboxAdapter (PLN2 section 8.1 — feature `wasm-sandbox`) ──────────

/// WASM sandbox execution adapter wrapping `touring_bindings::wasm::WasmRunner`.
///
/// Loads a pre-compiled WASM module (from bytes or WAT source) and executes
/// it via `WasmModule::call_evaluate()` with the rendered template content
/// as the `PluginContext.input`. Returns the plugin's output string on
/// success, or a `GenerateError::Internal` on sandbox failure.
///
/// # Wiring
///
/// The adapter is typically wired into `GeneratorContext::wasm_sandbox_fn`
/// to pre-validate rendered template output before committing. A failing
/// WASM validation is logged but does NOT block commit — the hard gate is
/// the `wiring_gate_fn`. The sandbox is a defense-in-depth validator.
///
/// # POTENCIALIZAR
///
/// Connects `touring_bindings::wasm::WasmRunner` (previously orphan) to the
/// generator pipeline via the `wasm_sandbox_fn` closure already invoked
/// during Verified → Rendered transition. Enables plugin-driven validation
/// of generated content using fuel-metered, memory-bounded WASM execution.
///
/// # Embedded default WAT
///
/// If no module is provided at construction, the adapter loads a minimal
/// Error from the [`WasmSandboxAdapter`] WAT/WASM constructors
/// (F-8 / RBP-03: typed in place of `String`).
#[cfg(feature = "wasm-sandbox")]
#[derive(Debug, thiserror::Error)]
pub enum WasmSandboxError {
    /// The underlying `WasmRunner` failed to initialize or load the module.
    #[error("{0}")]
    Runner(String),
}

/// "always-success" WAT source that returns `1` (success) from `evaluate()`.
/// This makes the adapter immediately usable in tests and fail-open defaults.
#[cfg(feature = "wasm-sandbox")]
pub struct WasmSandboxAdapter {
    runner: touring_bindings::wasm::WasmRunner,
    module: touring_bindings::wasm::WasmModule,
}

#[cfg(feature = "wasm-sandbox")]
impl std::fmt::Debug for WasmSandboxAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmSandboxAdapter").finish_non_exhaustive()
    }
}

#[cfg(feature = "wasm-sandbox")]
impl WasmSandboxAdapter {
    /// Minimal WAT source for an always-success validator module.
    ///
    /// Exports an `evaluate() -> i32` function that returns `1` (success).
    /// Used as the default sandbox payload when no custom module is provided.
    pub const DEFAULT_WAT: &'static str = r#"
        (module
          (func $evaluate (export "evaluate") (result i32)
            i32.const 1))
    "#;

    /// Construct with the embedded `DEFAULT_WAT` always-success module.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` when the embedded WAT source cannot be compiled —
    /// this should never happen in practice and indicates a build-time bug.
    pub fn with_default_wat() -> Result<Self, WasmSandboxError> {
        Self::with_wat(Self::DEFAULT_WAT)
    }

    /// Construct from an inline WAT source string.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` when the WAT source is malformed or the
    /// underlying `WasmRunner` engine fails to initialize.
    pub fn with_wat(wat_source: &str) -> Result<Self, WasmSandboxError> {
        let runner = touring_bindings::wasm::WasmRunner::new()
            .map_err(|e| WasmSandboxError::Runner(e.to_string()))?;
        let module = runner
            .load_wat(wat_source)
            .map_err(|e| WasmSandboxError::Runner(e.to_string()))?;
        Ok(Self { runner, module })
    }

    /// Construct from pre-compiled WASM bytes (`.wasm` binary).
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` when the bytes do not parse as a valid WASM
    /// module or when the runner cannot instantiate it.
    pub fn with_wasm_bytes(wasm_bytes: &[u8]) -> Result<Self, WasmSandboxError> {
        let runner = touring_bindings::wasm::WasmRunner::new()
            .map_err(|e| WasmSandboxError::Runner(e.to_string()))?;
        let module = runner
            .load_module(wasm_bytes)
            .map_err(|e| WasmSandboxError::Runner(e.to_string()))?;
        Ok(Self { runner, module })
    }

    /// Returns a reference to the underlying `WasmRunner` for inspection.
    #[must_use]
    pub fn runner(&self) -> &touring_bindings::wasm::WasmRunner {
        &self.runner
    }

    /// Run the sandbox against a rendered content string.
    ///
    /// The `code` argument is passed as `PluginContext.input` and `lang`
    /// as a config entry keyed `lang`, so plugins can dispatch based on
    /// the target language. Returns the plugin's output string, or an
    /// error when WASM execution fails or reports failure.
    ///
    /// # Errors
    ///
    /// Returns `GenerateError::Internal` when:
    /// - The WASM module call fails (fuel exhausted, trap, etc.)
    /// - The plugin reports `success = false`
    pub fn run(&self, code: &str, lang: &str) -> Result<String, GenerateError> {
        let ctx = touring_bindings::wasm::plugin::PluginContext::new(code.to_string())
            .with_config("lang", lang.to_string());
        match self.module.call_evaluate(&ctx) {
            Ok(result) => {
                if result.success {
                    Ok(result.output)
                } else {
                    Err(GenerateError::Internal(format!(
                        "wasm sandbox: plugin reported failure (fuel={})",
                        result.fuel_consumed
                    )))
                }
            }
            Err(e) => Err(GenerateError::Internal(format!(
                "wasm sandbox: execution failed: {e}"
            ))),
        }
    }

    /// Build a `WasmSandboxFn` closure that invokes this adapter.
    #[must_use]
    pub fn into_closure(self) -> crate::core::context::WasmSandboxFn {
        let adapter = Arc::new(self);
        Arc::new(move |code: &str, lang: &str| adapter.run(code, lang))
    }
}

// ── McctsEvalAdapter (PLN2 section 8.1 — feature `mcts-synthesis`) ──────────

/// MCTS evaluation adapter wrapping `touring_intelligence::reasoning::cognitive_mcts`.
///
/// Provides plan-state scoring via `GraphInformedMCTS::search` on a shared
/// `SemanticGraph`. The adapter translates `plan_id` strings to `u64` root
/// states via deterministic hashing (`FxHash`), runs a single MCTS search,
/// and normalizes the resulting `confidence` into a `NormalizedScore`
/// consumable by the `mcts_eval_fn` closure in `GeneratorContext`.
///
/// # Wiring
///
/// This adapter is only meaningful when paired with a `SemanticGraph` that
/// already has nodes + edges for the plans being scored. Typical usage:
///
/// ```rust,ignore
/// use std::sync::Arc;
/// use touring_generator::{SemanticGraphAdapter, McctsEvalAdapter};
///
/// let graph_adapter = Arc::new(SemanticGraphAdapter::new(path));
/// let mcts_adapter = McctsEvalAdapter::with_graph(graph_adapter.graph());
/// let mcts_fn = mcts_adapter.into_closure();
/// ```
///
/// The `mcts_eval_fn` returns `NormalizedScore::ZERO` when:
/// - The `plan_id` has no corresponding node in the graph
/// - MCTS search returns `None` (insufficient branching)
/// - The graph is empty
///
/// # POTENCIALIZAR
///
/// Wires `touring_intelligence::reasoning::GraphInformedMCTS` into the generator pipeline
/// via the `mcts_eval_fn` closure already invoked by the typestate executor
/// during Rendered → Speculated transition. Previously returned `0.0` (noop).
#[cfg(feature = "mcts-synthesis")]
pub struct McctsEvalAdapter {
    engine: touring_intelligence::reasoning::GraphInformedMCTS,
    graph: Arc<touring_intelligence::reasoning::semantic_graph::SemanticGraph>,
}

#[cfg(feature = "mcts-synthesis")]
impl std::fmt::Debug for McctsEvalAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McctsEvalAdapter").finish_non_exhaustive()
    }
}

#[cfg(feature = "mcts-synthesis")]
impl McctsEvalAdapter {
    /// Construct with a shared `SemanticGraph` and default MCTS config.
    #[must_use]
    pub fn with_graph(
        graph: Arc<touring_intelligence::reasoning::semantic_graph::SemanticGraph>,
    ) -> Self {
        let config =
            touring_intelligence::reasoning::cognitive_mcts::CognitiveMCTSConfig::default();
        Self {
            engine: touring_intelligence::reasoning::GraphInformedMCTS::new(config),
            graph,
        }
    }

    /// Construct with custom MCTS configuration.
    #[must_use]
    pub fn with_config(
        graph: Arc<touring_intelligence::reasoning::semantic_graph::SemanticGraph>,
        config: touring_intelligence::reasoning::cognitive_mcts::CognitiveMCTSConfig,
    ) -> Self {
        Self {
            engine: touring_intelligence::reasoning::GraphInformedMCTS::new(config),
            graph,
        }
    }

    /// Returns the underlying graph for inspection or composition.
    #[must_use]
    pub fn graph(&self) -> Arc<touring_intelligence::reasoning::semantic_graph::SemanticGraph> {
        Arc::clone(&self.graph)
    }

    /// Evaporates pheromones in the MCTS engine between search sessions.
    ///
    /// Call between planning epochs to prevent stale pheromone trails from
    /// dominating new searches. Touring-hooks may call this from a periodic
    /// maintenance task; `McctsEvalAdapter` holds its own pheromone layer
    /// via the internal `GraphInformedMCTS`.
    pub fn evaporate(&self) {
        self.engine.evaporate();
    }

    /// Deterministic `u64` hash from a string plan id.
    ///
    /// Uses `rustc_hash::FxHasher` (already in touring-cognitive's closure)
    /// to map an arbitrary string key into the MCTS `root_state` space.
    #[must_use]
    pub fn hash_state(key: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();
        key.hash(&mut hasher);
        hasher.finish()
    }

    /// Run an MCTS search for a given key and normalize the confidence.
    ///
    /// Returns `NormalizedScore::ZERO` if the graph is empty or MCTS cannot
    /// produce a result. Otherwise, returns the clamped `MCTSResult.confidence`.
    #[must_use]
    pub fn evaluate(&self, key: &str) -> NormalizedScore {
        let root_state = Self::hash_state(key);

        // Build node_id/reverse maps from the current graph state.
        let mut node_id_map: HashMap<String, u64> = HashMap::new();
        let mut reverse_map: HashMap<u64, String> = HashMap::new();

        // Seed root mapping so search has an entry point for the queried key.
        node_id_map.insert(key.to_string(), root_state);
        reverse_map.insert(root_state, key.to_string());

        // Include all graph neighbors of key + their neighbors (1-hop out)
        // so MCTS has branching options.
        let mut frontier: Vec<String> = self.graph.neighbors(key);
        for nbr in frontier.drain(..) {
            let h = Self::hash_state(&nbr);
            node_id_map.insert(nbr.clone(), h);
            reverse_map.insert(h, nbr);
        }

        let result = self
            .engine
            .search(root_state, &self.graph, &node_id_map, &reverse_map);

        match result {
            Some(r) => NormalizedScore::clamped(r.confidence),
            None => NormalizedScore::ZERO,
        }
    }

    /// Build a `MctsEvalFn` closure that invokes this adapter.
    ///
    /// Arc-wraps the adapter so the closure is `Send + Sync + 'static`.
    #[must_use]
    pub fn into_closure(self) -> crate::core::context::MctsEvalFn {
        let adapter = Arc::new(self);
        Arc::new(move |state: &str| adapter.evaluate(state))
    }
}
