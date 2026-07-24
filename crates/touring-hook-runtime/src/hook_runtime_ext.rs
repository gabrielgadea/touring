//! Extension methods for `HookRuntime` (symbol index / persistence / response
//! builders / stdin io) — extracted from `hook_runtime.rs` (F-9) as a second
//! `impl HookRuntime` block (inherent methods resolve via the type, no re-export).

use crate::hook_runtime::{HookPersistError, HookResponse, HookRuntime, StdinError};
use crate::hook_runtime::{find_cargo_workspace_root, hash_path};
use serde_json::Value;
use std::io::Read as _;
use std::path::PathBuf;
use touring_code::ast::IncrementalEditResult;
use touring_code::ast::{SymbolIndex, SymbolStore};
use touring_intelligence::rl::OnlineRLEngine;
use touring_intelligence::rl::memory::crdt_graph::{CrdtSemanticGraph, NodeWeight};
use touring_intelligence::rl::rl::tiny_transformer::{
    PredictionContext, ToolPrediction, ToolPredictor,
};

impl HookRuntime {
    /// Get a reference to the OnlineRL engine, if available.
    pub fn online_rl_engine(&self) -> Option<&OnlineRLEngine> {
        self.learning.online_rl.as_ref()
    }
    /// Get a reference to the SymbolStore, if available.
    pub fn symbol_store(&self) -> Option<&SymbolStore> {
        self.infra.symbol_store.as_ref()
    }
    /// Get or initialize the in-memory SymbolIndex.
    ///
    /// On first call, loads symbols from the SymbolStore (if available).
    pub fn get_symbol_index(&mut self) -> &SymbolIndex {
        if self.infra.symbol_index.is_none() {
            let mut idx = SymbolIndex::new();
            if let Some(ref store) = self.infra.symbol_store {
                let _ = store.load_into_index(&mut idx);
            }
            self.infra.symbol_index = Some(idx);
        }
        self.infra.symbol_index.as_ref().expect("just created")
    }
    /// Find all locations of a symbol by name.
    pub fn find_symbol(&mut self, name: &str) -> Vec<touring_code::ast::graph::SymbolLocation> {
        self.get_symbol_index()
            .find_symbol(name)
            .into_iter()
            .cloned()
            .collect()
    }
    /// Calculate blast radius for a file with depth limit.
    ///
    /// Returns `BlastRadiusOutput::Rich` with full metadata (file_count, max_distance, affected_symbols).
    /// For hot-path only files, use `petgraph_blast_radius` instead.
    pub fn blast_radius(
        &mut self,
        file_path: &str,
        max_depth: usize,
    ) -> touring_code::ast::BlastRadiusOutput {
        touring_code::ast::BlastRadiusOutput::Rich(
            self.get_symbol_index()
                .blast_radius_with_depth(file_path, max_depth),
        )
    }
    /// Persist symbols for a file into the SymbolStore.
    ///
    /// Called by post_read after extracting symbols via AST.
    pub fn persist_symbols(
        &self,
        file_path: &str,
        symbols: &[touring_code::ast::graph::SymbolLocation],
    ) -> Result<(), String> {
        if let Some(ref store) = self.infra.symbol_store {
            store
                .replace_file_symbols(file_path, symbols)
                .map_err(|e| format!("Failed to persist symbols: {e}"))?;
        }
        Ok(())
    }
    /// Process a file through the incremental pipeline (full parse on first call).
    ///
    /// Subsequent calls with the same `file_path` but different content use
    /// the cached tree for an O(edit) incremental re-parse instead of O(file).
    /// Returns the parse result with symbol delta information.
    pub fn process_file(
        &self,
        file_path: &str,
        content: &str,
    ) -> Result<IncrementalEditResult, String> {
        let pipeline = self
            .infra
            .pipeline
            .as_ref()
            .ok_or("IncrementalPipeline not initialized")?;
        pipeline
            .process_file(file_path, content)
            .map_err(|e| e.to_string())
    }
    /// Get cached symbols for a file from the incremental pipeline.
    ///
    /// Returns symbols from the pipeline's in-memory tree cache (O(1) lookup).
    /// Falls back to the pipeline's SymbolStore if no cached tree exists.
    /// Returns an empty Vec if the pipeline is not initialized or the file
    /// has not been processed.
    pub fn get_cached_symbols(
        &self,
        file_path: &str,
    ) -> Vec<touring_code::ast::graph::SymbolLocation> {
        self.infra
            .pipeline
            .as_ref()
            .map(|p| p.get_symbols(file_path))
            .unwrap_or_default()
    }
    /// Get the incremental pipeline's cache stats: (documents, trees).
    pub fn pipeline_cache_stats(&self) -> Option<(usize, usize)> {
        self.infra.pipeline.as_ref().map(|p| p.cache_stats())
    }
    /// R17: Predict the next likely tool(s) based on recent tool history.
    ///
    /// Delegates to the `TinyTransformerPredictor` (tiny transformer model).
    /// Returns top-k predictions as `ToolPrediction` (tool_name, confidence).
    /// Returns an empty Vec if the predictor is not initialized.
    pub fn predict_next_tools(
        &self,
        tool_history: &[String],
        cila_level: u8,
    ) -> Vec<ToolPrediction> {
        let Some(ref predictor) = self.learning.predictor else {
            return vec![];
        };
        let ctx = PredictionContext {
            recent_tools: tool_history.to_vec(),
            cila_level,
            session_id: String::new(),
        };
        predictor.predict(&ctx, 3)
    }
    /// R18: Record a file relationship in the CRDT graph.
    ///
    /// Creates nodes for both files and an edge between them.
    /// Lazily initializes the graph on first use.
    pub fn record_file_relation(&mut self, from_file: &str, to_file: &str, relation: &str) {
        let graph = self
            .learning
            .crdt_graph
            .get_or_insert_with(CrdtSemanticGraph::new);
        let from_id = hash_path(from_file);
        let to_id = hash_path(to_file);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        graph.add_node(
            1,
            from_id,
            NodeWeight {
                label: from_file.to_string(),
                score: 1.0,
                updated_at: now,
            },
        );
        graph.add_node(
            1,
            to_id,
            NodeWeight {
                label: to_file.to_string(),
                score: 1.0,
                updated_at: now,
            },
        );
        graph.add_edge(1, from_id, to_id, relation);
    }
    /// R18: Save CRDT graph state to disk.
    ///
    /// No-op if no graph has been initialized or loaded.
    /// Persist AgenticRL state for next session warm-start.
    ///
    /// Exports learning_phase_score, active status, and update_count to JSON.
    /// No-op when agentic_rl is None (first-run stays cheap).
    pub fn save_agentic_rl(&self) -> Result<(), HookPersistError> {
        let Some(ref agentic) = self.learning.agentic_rl else {
            return Ok(());
        };
        let data_dir = self.project_root.join(".claude/data");
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data dir {data_dir:?}: {e}"))?;
        let path = data_dir.join("agentic_rl_state.json");
        let state = agentic.export_state();
        let json = serde_json::to_string(&state)
            .map_err(|e| format!("Failed to serialize agentic RL state: {e}"))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("Failed to write agentic RL state: {e}"))?;
        Ok(())
    }

    /// Load persisted AgenticRL state from disk (warm-start). S-02 (2026-05-29).
    ///
    /// The counterpart to [`Self::save_agentic_rl`] — previously absent, which
    /// left the persisted state write-only so learning never carried across
    /// daemon restarts. Behavior:
    /// - file absent → `Ok(())` (cold-start is legal — first run, or a project
    ///   with no prior agentic state)
    /// - file present → deserialize, ensure the `AgenticRL` is initialized via
    ///   `agentic_rl_mut()`, then `restore_state()` to apply the persisted
    ///   `learning_phase_score` (+ recomputed `active`) and `update_count`.
    ///
    /// Returns `Err` only when the file exists but fails to deserialize.
    /// `path` is an owned `PathBuf`, so no borrow of `self.project_root` is held
    /// across the `self.learning.agentic_rl_mut()` mutable borrow.
    pub fn load_agentic_rl(&mut self) -> Result<(), HookPersistError> {
        let path = self.project_root.join(".claude/data/agentic_rl_state.json");
        if !path.exists() {
            return Ok(());
        }
        let json = std::fs::read_to_string(&path).map_err(|e| {
            format!(
                "Failed to read agentic RL state from {}: {e}",
                path.display()
            )
        })?;
        let state: crate::agentic_rl::AgenticRLState = serde_json::from_str(&json)
            .map_err(|e| format!("Failed to deserialize agentic RL state: {e}"))?;
        let agentic = self.learning.agentic_rl_mut();
        agentic.restore_state(&state);
        tracing::info!(
            score = state.learning_phase_score,
            active = state.active,
            update_count = state.update_count,
            "agentic_rl: warm-start load succeeded"
        );
        Ok(())
    }

    /// Persists the CRDT wiring graph to a memory-mapped file on disk, or no-op when no graph is initialized.
    pub fn save_crdt_graph(&self) -> Result<(), HookPersistError> {
        if let Some(ref graph) = self.learning.crdt_graph {
            let path = self.project_root.join(".claude/data/crdt_graph.rkyv");
            graph
                .save_to_mmap(&path)
                .map_err(|e| format!("Failed to save CRDT graph: {e}"))?;
        }
        Ok(())
    }
    /// L7-B Alpha: Load or initialize CRDT graph state (warm-start).
    ///
    /// Behavior:
    /// - If `.claude/data/crdt_graph.rkyv` exists → deserialize via `load_from_mmap`
    /// - If file does NOT exist → proactively initialize an empty graph
    ///
    /// Either path transitions `self.learning.crdt_graph` from `None` to `Some(_)`,
    /// which the daemon health reporter (`cli_handlers.rs`) translates to
    /// `crdt_graph: healthy/loaded` instead of `inactive/cold_start`.
    ///
    /// This solves the L7-B Alpha cold_start issue where the daemon would
    /// save on shutdown but never initialize on startup. Persistence across
    /// restarts is preserved (file → graph) while first-run gets a clean slate
    /// (no file → empty graph).
    ///
    /// Returns `Err(String)` ONLY if the file exists but fails to deserialize
    /// (corruption, version mismatch, partial write). First-run is always `Ok`.
    pub fn load_crdt_graph(&mut self) -> Result<(), HookPersistError> {
        let path = self.project_root.join(".claude/data/crdt_graph.rkyv");
        if !path.exists() {
            self.learning.crdt_graph = Some(CrdtSemanticGraph::new());
            tracing::info!(
                path = % path.display(),
                "crdt_graph: initialized empty (no persisted file)"
            );
            return Ok(());
        }
        match CrdtSemanticGraph::load_from_mmap(&path) {
            Ok(graph) => {
                let node_count = graph.node_count();
                let edge_count = graph.edge_count();
                self.learning.crdt_graph = Some(graph);
                tracing::info!(
                    path = % path.display(), nodes = node_count, edges = edge_count,
                    "crdt_graph: warm-start load succeeded"
                );
                Ok(())
            }
            Err(e) => Err(HookPersistError(format!(
                "Failed to load CRDT graph from {}: {e}",
                path.display()
            ))),
        }
    }
    /// Export consolidated metrics from all runtime subsystems.
    ///
    /// Aggregates hook execution stats, bandit selection metrics,
    /// cache hit rate, and session turn into a single serializable struct.
    /// RL metrics require a `QTable` reference since the runtime does not
    /// own the QTable directly — pass `None` if unavailable.
    pub fn export_metrics(
        &self,
        qtable: Option<&touring_intelligence::rl::QTable>,
    ) -> crate::metrics::RuntimeMetrics {
        use crate::metrics::{BanditMetrics, CacheMetrics, HookMetrics, RlMetrics, RuntimeMetrics};
        let hooks = if let Some(ref qa) = self.ctx.quality_assessment {
            let stats = &qa.streaming_stats;
            HookMetrics {
                total_hooks_fired: stats.total(),
                success_count: stats.success_count,
                failure_count: stats.failure_count,
                avg_latency_ms: stats.avg_latency_ms(),
                max_latency_ms: stats.max_latency_ms,
                success_rate: stats.success_rate(),
            }
        } else {
            HookMetrics::default()
        };
        let rl = qtable.map(|qt| {
            let m = qt.metrics();
            RlMetrics {
                td_error_ema: m.td_error_ema(),
                avg_reward: m.avg_reward(),
                total_updates: m.total_updates(),
                is_converging: m.is_converging(),
                is_diverging: m.is_diverging(),
            }
        });
        let bandit = self.learning.bandit.as_ref().map(|b| {
            let snapshot = b.export_snapshot();
            BanditMetrics {
                total_pulls: b.total_pulls(),
                num_arms: b.num_arms(),
                bandit_type: snapshot.bandit_type,
            }
        });
        let cache = CacheMetrics {
            hit_rate: self.cache_hit_rate(),
        };
        let cognitive = self.cognitive.as_ref().map(|rt| {
            let q_snap = rt.predictor().q_values_snapshot();
            let prediction_accuracy = if q_snap.is_empty() {
                0.0
            } else {
                let sum: f64 = q_snap.values().sum();
                sum / q_snap.len() as f64
            };
            crate::metrics::CognitiveMetrics {
                graph_node_count: rt.graph().node_count(),
                graph_edge_count: rt.graph().edge_count(),
                focus_cache_hit_rate: rt.focus_cache().hit_rate(),
                prediction_accuracy,
                is_connected: true,
                analysis_quality: None,
            }
        });
        RuntimeMetrics {
            hooks,
            rl,
            bandit,
            cognitive,
            cache,
            session_turn: self.session_turn(),
        }
    }
    /// Build an "allow" response (no context, silent pass).
    ///
    /// Unlike `emit_allow()`, this does NOT call `process::exit` —
    /// the caller decides how to handle the response.
    pub fn build_allow() -> HookResponse {
        HookResponse::Allow
    }
    /// Build a context response (injects additionalContext).
    ///
    /// Unlike `emit_context()`, this does NOT call `process::exit`.
    pub fn build_context(context: &str) -> HookResponse {
        HookResponse::Context {
            context: context.to_string(),
            event_name: None,
        }
    }
    /// Build a context response with an explicit event name.
    ///
    /// Unlike `emit_context_for_event()`, this does NOT call `process::exit`.
    pub fn build_context_for_event(context: &str, event_name: &str) -> HookResponse {
        HookResponse::Context {
            context: context.to_string(),
            event_name: Some(event_name.to_string()),
        }
    }
    /// Build a "deny" response (PreToolUse only — blocks tool execution).
    pub fn build_deny(reason: &str) -> HookResponse {
        HookResponse::Deny {
            reason: reason.to_string(),
            context: None,
            event_name: Some("PreToolUse".to_string()),
        }
    }
    /// Build a "deny" response with additional context.
    pub fn build_deny_with_context(reason: &str, context: &str) -> HookResponse {
        HookResponse::Deny {
            reason: reason.to_string(),
            context: Some(context.to_string()),
            event_name: Some("PreToolUse".to_string()),
        }
    }
    /// Build a "block" response (PostToolUse — blocks after execution).
    pub fn build_block(reason: &str) -> HookResponse {
        HookResponse::Block {
            reason: reason.to_string(),
            context: None,
            event_name: Some("PostToolUse".to_string()),
        }
    }
    /// Build a "block" response with additional context.
    pub fn build_block_with_context(reason: &str, context: &str) -> HookResponse {
        HookResponse::Block {
            reason: reason.to_string(),
            context: Some(context.to_string()),
            event_name: Some("PostToolUse".to_string()),
        }
    }
    /// Build a "halt" response (stops the entire session).
    pub fn build_halt(reason: &str) -> HookResponse {
        HookResponse::Halt {
            reason: reason.to_string(),
        }
    }
    /// Build a context response with updated tool input (PreToolUse only).
    ///
    /// Returns both `additionalContext` and `updatedInput` in the hook output,
    /// allowing hooks to normalize or correct tool inputs before execution.
    pub fn build_context_with_updated_input(
        context: &str,
        updated_input: serde_json::Value,
    ) -> HookResponse {
        HookResponse::ContextWithUpdatedInput {
            context: context.to_string(),
            event_name: Some("PreToolUse".to_string()),
            updated_input,
        }
    }
    /// Read and parse JSON from stdin with a 2-second timeout.
    ///
    /// Returns `{}` if stdin is a terminal or no data arrives within timeout.
    pub fn read_stdin() -> Result<Value, StdinError> {
        use std::io::IsTerminal;
        if std::io::stdin().is_terminal() {
            return Ok(serde_json::json!({}));
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut input = String::new();
            let result = std::io::stdin().read_to_string(&mut input);
            let _ = tx.send((input, result));
        });
        match rx.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok((input, Ok(_))) if !input.trim().is_empty() => serde_json::from_str(&input)
                .map_err(|e| StdinError(format!("JSON parse error: {e}"))),
            Ok(_) => Ok(serde_json::json!({})),
            Err(_) => Ok(serde_json::json!({})),
        }
    }
    /// Emit hookSpecificOutput with additionalContext and explicit hookEventName.
    ///
    /// Used by standalone CLI paths (e.g. session-start) that exit the process
    /// directly rather than returning a `HookResponse` through the daemon pipeline.
    /// For daemon-mode callers, use `build_context_for_event` instead.
    pub fn emit_context_for_event(context: &str, event_name: Option<&str>) -> ! {
        let mut hso = serde_json::json!({ "additionalContext" : context, });
        if let Some(name) = event_name {
            #[allow(clippy::indexing_slicing)]
            {
                hso["hookEventName"] = serde_json::Value::String(name.to_string());
            }
        }
        let output = serde_json::json!({ "hookSpecificOutput" : hso, });
        println!("{}", serde_json::to_string(&output).unwrap_or_default());
        std::process::exit(0)
    }
    /// Detect project root by walking up from cwd looking for `.claude/touring/` marker.
    ///
    /// Resolution strategy (in priority order; first match wins):
    ///
    /// 1. **`CLAUDE_PROJECT_DIR`** env var — explicit override from harness/IDE.
    /// 2. **`TOURING_PROJECT_ROOT`** env var — Touring-specific override; used to
    ///    pin the project when CC is launched from a subdirectory (e.g., a skill
    ///    base-path) that would otherwise mis-resolve.
    /// 3. **Cargo workspace root** — walk up from cwd looking for the first
    ///    `Cargo.toml` containing a `[workspace]` table. Cargo forbids nested
    ///    workspaces, so the first match is the unambiguous root.
    /// 4. **Outermost `.claude/touring/` marker** — walk all the way to `/`,
    ///    remembering the topmost ancestor that contains the marker. Using
    ///    outermost (not innermost) prevents stub DBs accidentally created in
    ///    subdirectories from shadowing the real project root.
    /// 5. **`HOME/.claude`** — legacy fallback for non-touring contexts.
    ///
    /// # Why the order matters
    ///
    /// The original implementation took the *innermost* `.claude/touring/`
    /// marker. When CC was launched with cwd inside a subdirectory that had
    /// previously seen a stub DB created (e.g., a vendor/ tree under a skill
    /// base-path), the daemon resolved to that stub instead of the workspace
    /// root above — incremental indexing wrote to the stub silently and the
    /// workspace symbols.db never received new symbols.
    ///
    /// The new order prefers explicit configuration, then Cargo's own concept
    /// of workspace boundary, and only then falls back to marker-based
    /// detection — and even that prefers the outermost match.
    pub fn detect_project_root() -> PathBuf {
        // (1) Highest-priority explicit override (legacy env var name).
        if let Ok(p) = std::env::var("CLAUDE_PROJECT_DIR") {
            return PathBuf::from(p);
        }
        // (2) Touring-specific override.
        if let Ok(p) = std::env::var("TOURING_PROJECT_ROOT") {
            return PathBuf::from(p);
        }

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        // (3) Cargo workspace root (strongest structural signal for Rust trees).
        if let Some(ws_root) = find_cargo_workspace_root(&cwd) {
            return ws_root;
        }

        // (4) Outermost `.claude/touring/` marker — walk to filesystem root and
        //     remember the topmost match. Beats the previous innermost-wins rule
        //     which made stub DBs in subdirs shadow the real workspace.
        let mut highest_match: Option<PathBuf> = None;
        let mut dir = cwd.clone();
        loop {
            if dir.join(".claude").join("touring").exists() {
                highest_match = Some(dir.clone());
            }
            if !dir.pop() {
                break;
            }
        }
        if let Some(root) = highest_match {
            return root;
        }

        // (5) Legacy fallback.
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".claude"))
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}
