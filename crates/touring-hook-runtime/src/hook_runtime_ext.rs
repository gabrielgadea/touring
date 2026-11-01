//! Extension methods for `HookRuntime` (symbol index / persistence / response
//! builders / stdin io) — extracted from `hook_runtime.rs` (F-9) as a second
//! `impl HookRuntime` block (inherent methods resolve via the type, no re-export).

use crate::hook_runtime::{HookPersistError, HookResponse, HookRuntime, StdinError};
use crate::hook_runtime::{find_cargo_workspace_root, hash_path};
use serde_json::Value;
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
    /// Read and parse JSON from stdin (2 s deadline, [`MAX_STDIN_BYTES`] cap).
    ///
    /// Compatibility wrapper over [`HookRuntime::read_stdin_detailed`] for ADVISORY
    /// callers: every unreadable outcome (timeout, I/O error, over the cap) still yields
    /// `{}` — **loudly** (F0.3b, 2026-09-01: recorded for the hook trace
    /// ([`last_stdin_read`]) and warned, never swallowed) — and only a non-JSON payload is
    /// an `Err`. A GUARD must not use this: it cannot tell "nothing to inspect" from
    /// "could not inspect". The guard path (`touring-hook`) calls `read_stdin_detailed`
    /// and denies on [`StdinReadState::is_unreadable`].
    pub fn read_stdin() -> Result<Value, StdinError> {
        let read = Self::read_stdin_detailed();
        match read.error {
            Some(e) => Err(StdinError(e)),
            None => Ok(read.value),
        }
    }
    /// Read the hook payload from stdin and say HOW the read ended (C4, 2026-10-17).
    ///
    /// The reader tolerates a non-blocking stdin (Claude Code hands hooks a libuv
    /// socketpair) by waiting out `EAGAIN` until EOF or the deadline instead of surfacing
    /// `WouldBlock` as "no input". The outcome is recorded for the hook trace
    /// ([`last_stdin_read`]) and warned.
    pub fn read_stdin_detailed() -> StdinRead {
        use std::io::IsTerminal;
        if std::io::stdin().is_terminal() {
            set_last_stdin_read(StdinReadState::Terminal, 0);
            return StdinRead::with_value(serde_json::json!({}), StdinReadState::Terminal, 0);
        }
        let read = read_payload(std::io::stdin(), STDIN_TIMEOUT, MAX_STDIN_BYTES);
        set_last_stdin_read(read.state, read.bytes);
        match read.state {
            StdinReadState::Ok | StdinReadState::Terminal | StdinReadState::Unread => {}
            StdinReadState::Empty => {
                tracing::warn!("hook stdin: empty payload — handler runs on `{{}}`");
            }
            StdinReadState::OkRecovered => {
                tracing::warn!(
                    bytes = read.bytes,
                    "hook stdin: no clean EOF, but the bytes received are one complete JSON \
                     document — using it"
                );
            }
            degraded => {
                tracing::warn!(
                    state = degraded.as_str(),
                    bytes = read.bytes,
                    error = read.error.as_deref().unwrap_or(""),
                    "hook stdin unreadable — an advisory runs on `{{}}`, a guard denies"
                );
            }
        }
        read
    }
    /// The deny a GUARD answers when it could not read its input (C4, 2026-10-17).
    ///
    /// Fail-closed with the reason NAMED and the way out spelled: a guard that runs on a
    /// `{}` it substituted for the real payload sees an empty command and allows — it
    /// "inspected" nothing and said yes. The reason teaches the retry, and the human
    /// switch for a persistently broken pipe.
    pub fn build_stdin_unreadable_deny(hook: &str, read: &StdinRead) -> HookResponse {
        let cause = match read.state {
            StdinReadState::Timeout => format!(
                "sem EOF em {}s e os {} bytes recebidos não formam um JSON completo",
                STDIN_TIMEOUT.as_secs(),
                read.bytes
            ),
            StdinReadState::ReadError => {
                format!("erro de E/S ao ler o stdin após {} bytes", read.bytes)
            }
            StdinReadState::TooLarge => format!(
                "payload acima do teto de {} MiB",
                MAX_STDIN_BYTES / (1024 * 1024)
            ),
            StdinReadState::ParseError => format!(
                "{} bytes chegaram mas não são JSON ({})",
                read.bytes,
                read.error.as_deref().unwrap_or("parse error")
            ),
            other => other.as_str().to_string(),
        };
        let reason = format!(
            "[entrada-ilegível] {hook}: a entrada do hook ficou ilegível ({cause}); nada foi \
             inspecionado, então a guarda recusa em vez de liberar. Saída: repita a mesma \
             chamada (falha transitória do pipe); se o motivo for o teto, divida a operação em \
             partes menores. Se persistir, é o harness que quebrou o pipe: avise o Gabriel \
             (kill switch humano: {STDIN_GUARD_DISABLED_ENV}=1; diagnóstico: \
             `touring doctor -j` e o stdin_state em TOURING_HOOK_TRACE_FILE)."
        );
        HookResponse::Deny {
            reason,
            context: None,
            event_name: Some("PreToolUse".to_string()),
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
        crate::hook_trace::set_reason("emit-json"); // S14: the exit reason, not the default
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
        Self::detect_project_root_for(None)
    }

    /// [`Self::detect_project_root`] com o `cwd` do PAYLOAD do hook na cadeia (HOOKCWD, 30.4.97).
    ///
    /// Toda chamada do Claude Code carrega o `cwd` da sessão no payload. Sem
    /// `CLAUDE_PROJECT_DIR` (uma chamada manual do hook, um smoke) a raiz saía de
    /// `TOURING_PROJECT_ROOT` — que o settings global fixa em `~/projects/touring` — ou da subida
    /// estrutural até o marcador `~/.claude/touring`; o daemon pinado em OUTRO projeto recusa em
    /// 0 ms e o CC engole o erro calado (smoke do `analise`, 10/10/2026, casos A e B).
    ///
    /// A posição do `cwd` do payload é a segunda: `CLAUDE_PROJECT_DIR` continua primeiro (é o que o
    /// harness diz ser a raiz), e o `cwd` da sessão — específico, por chamada — vence uma env
    /// global e uma subida estrutural, que descrevem o PROCESSO, não quem está perguntando. Ver
    /// [`pick_hook_project_root`].
    pub fn detect_project_root_for(payload_cwd: Option<&std::path::Path>) -> PathBuf {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let home = std::env::var_os("HOME").map(PathBuf::from);
        pick_hook_project_root(
            std::env::var_os("CLAUDE_PROJECT_DIR").map(PathBuf::from),
            payload_cwd,
            std::env::var_os("TOURING_PROJECT_ROOT").map(PathBuf::from),
            &cwd,
            home.as_deref(),
        )
    }
}

/// A cadeia da raiz do hook, PURA sobre os valores de ambiente (testável sem mutar o do processo):
///
/// 1. `claude_dir` (`CLAUDE_PROJECT_DIR`) — explícito do harness, devolvido como veio.
/// 2. `payload_cwd` — o `cwd` do payload do hook, se for um diretório absoluto que existe; vira a
///    raiz do projeto que o contém pela MESMA regra que o daemon aplica ao `project_root` que
///    recebe (`TouringConfig::normalize_project_root_inner`): um cwd de subdiretório e a raiz do
///    projeto são a mesma pergunta, e o cliente não pode responder diferente do servidor.
/// 3. `touring_root` (`TOURING_PROJECT_ROOT`) — o pin explícito do processo.
/// 4. A raiz do workspace Cargo acima do `cwd` do processo.
/// 5. O marcador `.claude/touring` mais EXTERNO acima do `cwd` do processo.
/// 6. `$HOME/.claude` — fallback legado.
pub(crate) fn pick_hook_project_root(
    claude_dir: Option<PathBuf>,
    payload_cwd: Option<&std::path::Path>,
    touring_root: Option<PathBuf>,
    cwd: &std::path::Path,
    home: Option<&std::path::Path>,
) -> PathBuf {
    // (1) Highest-priority explicit override (legacy env var name).
    if let Some(p) = claude_dir {
        return p;
    }
    // (2) O cwd da sessão que está perguntando. Relativo ou inexistente não diz nada (e um
    //     cwd relativo resolvido aqui ancoraria no cwd do PROCESSO, não no da sessão).
    if let Some(c) = payload_cwd.filter(|c| c.is_absolute() && c.is_dir()) {
        return touring_foundation::TouringConfig::normalize_project_root_inner(c, home);
    }
    // (3) Touring-specific override.
    if let Some(p) = touring_root {
        return p;
    }

    // (4) Cargo workspace root (strongest structural signal for Rust trees).
    if let Some(ws_root) = find_cargo_workspace_root(cwd) {
        return ws_root;
    }

    // (5) Outermost `.claude/touring/` marker — walk all the way to `/`,
    //     remembering the topmost ancestor that contains the marker. Using
    //     outermost (not innermost) prevents stub DBs accidentally created in
    //     subdirectories from shadowing the real project root.
    let mut highest_match: Option<PathBuf> = None;
    let mut dir = cwd.to_path_buf();
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

    // (6) Legacy fallback.
    home.map(|h| h.join(".claude"))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// How the hook payload read from stdin ended (F0.3b, 2026-09-01).
///
/// Exposed for the per-invocation trace ([`crate::hook_trace`]) so a live
/// hook that ran on `{}` says WHY on disk instead of vanishing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdinReadState {
    /// `read_stdin` has not run in this process.
    Unread,
    /// stdin is a TTY — no payload expected.
    Terminal,
    /// Payload read to EOF and parsed.
    Ok,
    /// EOF with zero bytes.
    Empty,
    /// No EOF within the deadline.
    Timeout,
    /// A non-retryable I/O error while reading.
    ReadError,
    /// Bytes arrived but were not JSON.
    ParseError,
    /// No clean EOF (timeout or I/O error) but the bytes received already form ONE
    /// complete JSON object — used as the payload, counted apart from `Ok` (C4).
    OkRecovered,
    /// More than [`MAX_STDIN_BYTES`] arrived; the read stopped at the cap (C4).
    TooLarge,
}

impl StdinReadState {
    /// Stable snake-ish names for the trace line.
    pub fn as_str(self) -> &'static str {
        match self {
            StdinReadState::Unread => "unread",
            StdinReadState::Terminal => "terminal",
            StdinReadState::Ok => "ok",
            StdinReadState::Empty => "empty",
            StdinReadState::Timeout => "timeout",
            StdinReadState::ReadError => "read-error",
            StdinReadState::ParseError => "parse-error",
            StdinReadState::OkRecovered => "ok-recovered",
            StdinReadState::TooLarge => "too-large",
        }
    }

    /// `true` when the hook could NOT inspect its input: no usable payload reached it
    /// (C4). A GUARD denies on this; an ADVISORY keeps failing open on `{}`. `Empty`
    /// (zero bytes, clean EOF), `Terminal` and `OkRecovered` are not unreadable — the first
    /// two are "nothing was sent" (allowed, counted), the last carries a whole payload.
    pub fn is_unreadable(self) -> bool {
        matches!(
            self,
            StdinReadState::Timeout
                | StdinReadState::ReadError
                | StdinReadState::ParseError
                | StdinReadState::TooLarge
        )
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => StdinReadState::Terminal,
            2 => StdinReadState::Ok,
            3 => StdinReadState::Empty,
            4 => StdinReadState::Timeout,
            5 => StdinReadState::ReadError,
            6 => StdinReadState::ParseError,
            7 => StdinReadState::OkRecovered,
            8 => StdinReadState::TooLarge,
            _ => StdinReadState::Unread,
        }
    }
}

static LAST_STDIN_STATE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static LAST_STDIN_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn set_last_stdin_read(state: StdinReadState, bytes: u64) {
    LAST_STDIN_STATE.store(state as u8, std::sync::atomic::Ordering::Relaxed);
    LAST_STDIN_BYTES.store(bytes, std::sync::atomic::Ordering::Relaxed);
}

/// Outcome of the last [`HookRuntime::read_stdin`] in this process:
/// `(state, bytes_read)`. `Unread` until it runs.
pub fn last_stdin_read() -> (StdinReadState, u64) {
    (
        StdinReadState::from_u8(LAST_STDIN_STATE.load(std::sync::atomic::Ordering::Relaxed)),
        LAST_STDIN_BYTES.load(std::sync::atomic::Ordering::Relaxed),
    )
}

/// Deadline for the hook payload to reach EOF.
pub const STDIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Hard cap on the hook payload (C4, 2026-10-17): 16 MiB.
///
/// Measured over 833,048 live invocations (`hook_trace.jsonl`, 17/10/2026): the largest
/// payload ever sent to ANY hook was 6.1 MB (`post-tool-batch`, an advisory) and the
/// largest to a guard was 126 KB (`pre-write`). 16 MiB leaves 2.6× headroom over the
/// former — so no advisory regresses — and 130× over the latter, while still bounding
/// memory: the reader stops at the cap instead of buffering a runaway stream.
pub const MAX_STDIN_BYTES: usize = 16 * 1024 * 1024;

/// Human-only switch that restores the old fail-open for guards on an unreadable stdin.
pub const STDIN_GUARD_DISABLED_ENV: &str = "TOURING_HOOK_STDIN_GUARD_DISABLED";

/// `true` when [`STDIN_GUARD_DISABLED_ENV`]`=1` — a guard then fails open as before C4.
pub fn stdin_guard_disabled() -> bool {
    std::env::var(STDIN_GUARD_DISABLED_ENV).is_ok_and(|v| v == "1")
}

/// What one stdin read produced, and how it ended (C4).
#[derive(Debug, Clone)]
pub struct StdinRead {
    /// The parsed payload; `{}` whenever `state` is not `Ok`/`OkRecovered`.
    pub value: Value,
    /// How the read ended.
    pub state: StdinReadState,
    /// Bytes received (before parsing).
    pub bytes: u64,
    /// Set only for `ParseError`: the parser message.
    pub error: Option<String>,
}

impl StdinRead {
    fn with_value(value: Value, state: StdinReadState, bytes: u64) -> Self {
        Self {
            value,
            state,
            bytes,
            error: None,
        }
    }
}

/// Read `reader` and classify the outcome, free of process-global state (C4).
///
/// The classification is the whole point: the F0.3b libuv socketpair can leave a
/// COMPLETE payload without an EOF (timeout with the JSON already in hand) — that is
/// `OkRecovered`, never `{}`; a timeout/error with no complete JSON object, a non-JSON
/// payload and an over-cap stream are unreadable, never `{}`-and-allow for a guard.
pub fn read_payload<R: std::io::Read + Send + 'static>(
    reader: R,
    timeout: std::time::Duration,
    max_bytes: usize,
) -> StdinRead {
    let (text, state) = read_bounded(reader, timeout, max_bytes);
    let bytes = text.len() as u64;
    let empty = || serde_json::json!({});
    match state {
        StdinReadState::Ok if !text.trim().is_empty() => match serde_json::from_str(&text) {
            Ok(v) => StdinRead::with_value(v, StdinReadState::Ok, bytes),
            Err(e) => StdinRead {
                value: empty(),
                state: StdinReadState::ParseError,
                bytes,
                error: Some(format!("JSON parse error: {e}")),
            },
        },
        // EOF with nothing (or only whitespace) in it: nothing was sent.
        StdinReadState::Ok | StdinReadState::Empty => {
            StdinRead::with_value(empty(), StdinReadState::Empty, bytes)
        }
        StdinReadState::Timeout | StdinReadState::ReadError => {
            match serde_json::from_str::<Value>(&text) {
                Ok(v) if v.is_object() => {
                    StdinRead::with_value(v, StdinReadState::OkRecovered, bytes)
                }
                _ => StdinRead::with_value(empty(), state, bytes),
            }
        }
        other => StdinRead::with_value(empty(), other, bytes),
    }
}

/// Read `reader` to EOF on a helper thread, bounded by `timeout` and `max_bytes`.
///
/// Tolerates a **non-blocking** descriptor: `WouldBlock` (EAGAIN) is waited
/// out, never surfaced as "no more input" — Claude Code hands hooks a libuv
/// socketpair and the write side may be shut down after the child's first
/// read, which is exactly where `read_to_string` used to drop the payload.
/// The helper accumulates into a shared buffer so that a timeout can still hand back
/// the bytes already received (C4: they may be a whole payload that never got its EOF).
/// On timeout the helper thread is abandoned (the process exits shortly after in every
/// caller). The returned text is what was received, `TooLarge` included (cut at the cap).
pub fn read_bounded<R: std::io::Read + Send + 'static>(
    mut reader: R,
    timeout: std::time::Duration,
    max_bytes: usize,
) -> (String, StdinReadState) {
    use std::sync::{Arc, Mutex};
    let shared: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&shared);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        let outcome = loop {
            match reader.read(&mut chunk) {
                Ok(0) => break Ok(StdinReadState::Ok),
                Ok(n) => {
                    let mut buf = sink.lock().unwrap_or_else(|p| p.into_inner());
                    let room = max_bytes.saturating_sub(buf.len());
                    if n > room {
                        buf.extend_from_slice(&chunk[..room]);
                        break Ok(StdinReadState::TooLarge);
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Err(e) => break Err(e),
            }
        };
        let _ = tx.send(outcome);
    });
    let state = match rx.recv_timeout(timeout) {
        Ok(Ok(StdinReadState::Ok)) => StdinReadState::Ok,
        Ok(Ok(other)) => other,
        Ok(Err(_)) => StdinReadState::ReadError,
        Err(_) => StdinReadState::Timeout,
    };
    let buf = shared.lock().unwrap_or_else(|p| p.into_inner());
    let text = String::from_utf8_lossy(&buf).into_owned();
    let state = match state {
        StdinReadState::Ok if text.is_empty() => StdinReadState::Empty,
        s => s,
    };
    (text, state)
}

/// F0.3b (2026-09-01) — `read_stdin` must survive the transport Claude Code
/// actually uses (a libuv socketpair whose write side may be shut down AFTER
/// the child's first read) and must never degrade to `{}` in silence.
#[cfg(test)]
mod stdin_read_tests {
    use super::{
        HookRuntime, MAX_STDIN_BYTES, STDIN_GUARD_DISABLED_ENV, StdinRead, StdinReadState,
        read_bounded, read_payload,
    };
    use crate::hook_runtime::HookResponse;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    fn payload(n: usize) -> String {
        format!("{{\"tool_input\":{{\"command\":\"{}\"}}}}", "x".repeat(n))
    }

    const WAIT: Duration = Duration::from_secs(2);

    /// A reader that yields `data` and then fails with a non-retryable I/O error.
    struct FailsAfter {
        data: Vec<u8>,
        sent: bool,
    }
    impl Read for FailsAfter {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.sent {
                return Err(std::io::Error::other("pipe broke"));
            }
            self.sent = true;
            let n = self.data.len().min(buf.len());
            buf[..n].copy_from_slice(&self.data[..n]);
            Ok(n)
        }
    }

    /// The live failure shape: non-blocking fd, data arrives AFTER the first
    /// read (EAGAIN with empty buffer), EOF arrives AFTER the data is consumed
    /// (EAGAIN again). A `read_to_string` that surfaces `WouldBlock` loses the
    /// payload; the reader must wait out EAGAIN until EOF or the deadline.
    #[test]
    fn nonblocking_socket_with_late_shutdown_is_read_whole() {
        let (mut writer, reader) = UnixStream::pair().expect("socketpair");
        reader.set_nonblocking(true).expect("nonblocking child end");
        let expected = payload(3000);
        let sent = expected.clone();
        let producer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            writer.write_all(sent.as_bytes()).expect("write");
            std::thread::sleep(Duration::from_millis(60));
            writer
                .shutdown(std::net::Shutdown::Write)
                .expect("shutdown write side");
        });
        let (input, state) = read_bounded(reader, WAIT, MAX_STDIN_BYTES);
        producer.join().expect("producer");
        assert_eq!(state, StdinReadState::Ok, "input={input:?}");
        assert_eq!(input, expected);
    }

    #[test]
    fn blocking_socket_is_read_whole() {
        let (mut writer, reader) = UnixStream::pair().expect("socketpair");
        let expected = payload(500);
        writer.write_all(expected.as_bytes()).expect("write");
        writer
            .shutdown(std::net::Shutdown::Write)
            .expect("shutdown write side");
        let (input, state) = read_bounded(reader, WAIT, MAX_STDIN_BYTES);
        assert_eq!(state, StdinReadState::Ok);
        assert_eq!(input, expected);
    }

    #[test]
    fn empty_input_is_reported_as_empty_not_ok() {
        let (writer, reader) = UnixStream::pair().expect("socketpair");
        writer
            .shutdown(std::net::Shutdown::Write)
            .expect("shutdown write side");
        let (input, state) = read_bounded(reader, WAIT, MAX_STDIN_BYTES);
        assert_eq!(state, StdinReadState::Empty);
        assert!(input.is_empty());
    }

    #[test]
    fn writer_that_never_closes_is_reported_as_timeout() {
        let (mut writer, reader) = UnixStream::pair().expect("socketpair");
        writer.write_all(b"{\"partial\":").expect("write");
        let started = std::time::Instant::now();
        let (input, state) = read_bounded(reader, Duration::from_millis(150), MAX_STDIN_BYTES);
        assert_eq!(state, StdinReadState::Timeout);
        assert!(started.elapsed() >= Duration::from_millis(150));
        // C4: the bytes already received survive the timeout (they used to be dropped).
        assert_eq!(input, "{\"partial\":");
        drop(writer);
    }

    #[test]
    fn state_names_are_stable_for_the_trace_line() {
        assert_eq!(StdinReadState::Ok.as_str(), "ok");
        assert_eq!(StdinReadState::Empty.as_str(), "empty");
        assert_eq!(StdinReadState::Timeout.as_str(), "timeout");
        assert_eq!(StdinReadState::Terminal.as_str(), "terminal");
        assert_eq!(StdinReadState::ReadError.as_str(), "read-error");
        assert_eq!(StdinReadState::ParseError.as_str(), "parse-error");
        assert_eq!(StdinReadState::OkRecovered.as_str(), "ok-recovered");
        assert_eq!(StdinReadState::TooLarge.as_str(), "too-large");
    }

    // ── C4 (2026-10-17): the four states a guard has to tell apart ─────────────────

    /// (a) The F0.3b shape: the whole payload arrived, the EOF never did. The JSON in hand
    /// is used — never `{}` — and the state is its own counted value.
    #[test]
    fn c4_timeout_with_a_complete_json_is_recovered_not_dropped() {
        let (mut writer, reader) = UnixStream::pair().expect("socketpair");
        let body = payload(200);
        writer.write_all(body.as_bytes()).expect("write");
        let r = read_payload(reader, Duration::from_millis(150), MAX_STDIN_BYTES);
        assert_eq!(r.state, StdinReadState::OkRecovered);
        assert_eq!(r.value["tool_input"]["command"], "x".repeat(200));
        assert_eq!(r.bytes, body.len() as u64);
        assert!(!r.state.is_unreadable());
        drop(writer);
    }

    /// (b) Timeout with a PARTIAL document: nothing usable — unreadable, value `{}`.
    #[test]
    fn c4_timeout_with_a_truncated_json_is_unreadable() {
        let (mut writer, reader) = UnixStream::pair().expect("socketpair");
        writer
            .write_all(br#"{"tool_input":{"command":"rm -rf /"#)
            .expect("write");
        let r = read_payload(reader, Duration::from_millis(150), MAX_STDIN_BYTES);
        assert_eq!(r.state, StdinReadState::Timeout);
        assert!(r.state.is_unreadable());
        assert_eq!(r.value, serde_json::json!({}));
        assert!(r.bytes > 0, "the received bytes are counted, not zeroed");
        drop(writer);
    }

    /// (b) Timeout with ZERO bytes (open, silent pipe) is unreadable too — it is not "empty".
    #[test]
    fn c4_silent_open_pipe_is_a_timeout_not_empty() {
        let (writer, reader) = UnixStream::pair().expect("socketpair");
        let r = read_payload(reader, Duration::from_millis(150), MAX_STDIN_BYTES);
        assert_eq!(r.state, StdinReadState::Timeout);
        assert!(r.state.is_unreadable());
        assert_eq!(r.bytes, 0);
        drop(writer);
    }

    /// (b) A non-retryable I/O error: with no complete JSON it is unreadable; with a complete
    /// JSON already received it is recovered like the timeout case.
    #[test]
    fn c4_read_error_is_unreadable_unless_the_json_is_already_whole() {
        let broken = |data: &str| {
            read_payload(
                FailsAfter {
                    data: data.as_bytes().to_vec(),
                    sent: false,
                },
                WAIT,
                MAX_STDIN_BYTES,
            )
        };
        let partial = broken(r#"{"tool_input":{"comm"#);
        assert_eq!(partial.state, StdinReadState::ReadError);
        assert!(partial.state.is_unreadable());
        assert_eq!(partial.value, serde_json::json!({}));
        let whole = broken(r#"{"tool_input":{"command":"ls"}}"#);
        assert_eq!(whole.state, StdinReadState::OkRecovered);
        assert_eq!(whole.value["tool_input"]["command"], "ls");
    }

    /// (b) Over the byte cap: the read STOPS at the cap (bounded memory) and is unreadable.
    #[test]
    fn c4_payload_over_the_cap_is_too_large_and_the_read_stops_at_the_cap() {
        let (mut writer, reader) = UnixStream::pair().expect("socketpair");
        let producer = std::thread::spawn(move || {
            // More than the cap, then EOF; a broken pipe after the reader stopped is fine.
            let _ = writer.write_all(&vec![b'a'; 3000]);
        });
        let r = read_payload(reader, WAIT, 1000);
        producer.join().expect("producer");
        assert_eq!(r.state, StdinReadState::TooLarge);
        assert!(r.state.is_unreadable());
        assert_eq!(r.bytes, 1000, "exactly the cap was buffered, no more");
        assert_eq!(r.value, serde_json::json!({}));
    }

    /// Exactly AT the cap is still readable; one byte over is not.
    #[test]
    fn c4_the_cap_boundary_is_inclusive() {
        let body = payload(100);
        let at = |max: usize| {
            let (mut w, r) = UnixStream::pair().expect("socketpair");
            w.write_all(body.as_bytes()).expect("write");
            w.shutdown(std::net::Shutdown::Write).expect("shutdown");
            read_payload(r, WAIT, max)
        };
        assert_eq!(at(body.len()).state, StdinReadState::Ok);
        assert_eq!(at(body.len() - 1).state, StdinReadState::TooLarge);
    }

    /// (c) EOF with zero bytes: allowed, but its own counted state — and not unreadable.
    #[test]
    fn c4_empty_is_counted_and_allowed() {
        let (w, r) = UnixStream::pair().expect("socketpair");
        w.shutdown(std::net::Shutdown::Write).expect("shutdown");
        let read = read_payload(r, WAIT, MAX_STDIN_BYTES);
        assert_eq!(read.state, StdinReadState::Empty);
        assert!(!read.state.is_unreadable());
        assert_eq!(read.value, serde_json::json!({}));
    }

    /// Bytes + clean EOF that are not JSON: unreadable, with the parser message kept.
    #[test]
    fn c4_non_json_after_a_clean_eof_is_a_parse_error() {
        let (mut w, r) = UnixStream::pair().expect("socketpair");
        w.write_all(b"this is not json").expect("write");
        w.shutdown(std::net::Shutdown::Write).expect("shutdown");
        let read = read_payload(r, WAIT, MAX_STDIN_BYTES);
        assert_eq!(read.state, StdinReadState::ParseError);
        assert!(read.state.is_unreadable());
        assert!(
            read.error
                .as_deref()
                .is_some_and(|e| e.contains("JSON parse error"))
        );
    }

    #[test]
    fn c4_a_normal_payload_is_ok_and_parsed() {
        let (mut w, r) = UnixStream::pair().expect("socketpair");
        w.write_all(payload(10).as_bytes()).expect("write");
        w.shutdown(std::net::Shutdown::Write).expect("shutdown");
        let read = read_payload(r, WAIT, MAX_STDIN_BYTES);
        assert_eq!(read.state, StdinReadState::Ok);
        assert!(!read.state.is_unreadable());
        assert_eq!(read.value["tool_input"]["command"], "x".repeat(10));
        assert!(read.error.is_none());
    }

    /// `is_unreadable` is a total, explicit table — a new variant must be classified here.
    #[test]
    fn c4_unreadable_classification_is_exactly_the_four_failure_states() {
        use StdinReadState::*;
        let all = [
            (Unread, false),
            (Terminal, false),
            (Ok, false),
            (Empty, false),
            (Timeout, true),
            (ReadError, true),
            (ParseError, true),
            (OkRecovered, false),
            (TooLarge, true),
        ];
        for (state, expected) in all {
            assert_eq!(state.is_unreadable(), expected, "{}", state.as_str());
            assert_eq!(
                StdinReadState::from_u8(state as u8),
                state,
                "the trace round-trip must keep {}",
                state.as_str()
            );
        }
    }

    /// The deny teaches: it names the hook, the cause and the way out, and speaks as the
    /// PreToolUse event.
    #[test]
    fn c4_the_guard_deny_names_the_cause_and_the_way_out() {
        let read = StdinRead {
            value: serde_json::json!({}),
            state: StdinReadState::Timeout,
            bytes: 37,
            error: None,
        };
        let HookResponse::Deny {
            reason, event_name, ..
        } = HookRuntime::build_stdin_unreadable_deny("pre-bash", &read)
        else {
            panic!("a guard that cannot read its input must DENY");
        };
        assert_eq!(event_name.as_deref(), Some("PreToolUse"));
        assert!(reason.contains("[entrada-ilegível] pre-bash"), "{reason}");
        assert!(reason.contains("37 bytes"), "names the cause: {reason}");
        assert!(reason.contains("nada foi inspecionado"), "{reason}");
        assert!(reason.contains("repita"), "teaches the retry: {reason}");
        assert!(
            reason.contains(STDIN_GUARD_DISABLED_ENV),
            "the human way out: {reason}"
        );
        let too_big = StdinRead {
            state: StdinReadState::TooLarge,
            bytes: MAX_STDIN_BYTES as u64,
            ..read
        };
        let HookResponse::Deny { reason, .. } =
            HookRuntime::build_stdin_unreadable_deny("pre-write", &too_big)
        else {
            panic!("deny expected");
        };
        assert!(reason.contains("teto de 16 MiB"), "{reason}");
    }
}

/// HOOKCWD (30.4.97) — a cadeia da raiz do hook com o `cwd` do payload. Os casos A e B são os do
/// smoke do `analise` (10/10/2026, 30.4.96): sem `CLAUDE_PROJECT_DIR`, a raiz saía de
/// `TOURING_PROJECT_ROOT` global (A) ou da subida até o marcador `~/.claude/touring` (B), e o
/// daemon pinado no `analise` recusava em 0 ms.
#[cfg(test)]
mod hook_root_tests {
    use super::pick_hook_project_root;
    use std::path::{Path, PathBuf};

    struct Mundo {
        _tmp: tempfile::TempDir,
        analise: PathBuf,
        touring: PathBuf,
        casa: PathBuf,
    }

    /// `analise` e `touring` são projetos (`.touring/`); `casa` é um HOME com o marcador global.
    fn mundo() -> Mundo {
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = tmp.path().canonicalize().expect("canonical");
        let analise = base.join("analise");
        let touring = base.join("touring");
        let casa = base.join("casa");
        for d in [
            analise.join(".touring"),
            analise.join("sub/dir"),
            touring.join(".touring"),
            casa.join(".claude/touring"),
            casa.join("trabalho"),
        ] {
            std::fs::create_dir_all(d).expect("mkdir");
        }
        Mundo {
            _tmp: tmp,
            analise,
            touring,
            casa,
        }
    }

    fn escolhe(
        claude: Option<&Path>,
        payload: Option<&Path>,
        touring_root: Option<&Path>,
        cwd: &Path,
        home: &Path,
    ) -> PathBuf {
        pick_hook_project_root(
            claude.map(Path::to_path_buf),
            payload,
            touring_root.map(Path::to_path_buf),
            cwd,
            Some(home),
        )
    }

    /// Caso A: `TOURING_PROJECT_ROOT` global aponta para OUTRO projeto; o cwd do payload manda.
    #[test]
    fn caso_a_o_cwd_do_payload_vence_touring_project_root() {
        let m = mundo();
        let sub = m.analise.join("sub/dir");
        assert_eq!(
            escolhe(None, Some(&sub), Some(&m.touring), &m.touring, &m.casa),
            m.analise,
            "o cwd da sessão (um subdiretório do analise) resolve para a raiz do analise"
        );
    }

    /// Caso B: sem env alguma, o processo roda em `~/trabalho`; a subida estrutural chegaria a um
    /// marcador global `.claude/touring` — no smoke, o do HOME real; a raiz saía do projeto. O cwd
    /// do payload manda.
    ///
    /// A premissa do defeito é afirmada pela PROPRIEDADE, não por um valor: ONDE a subida termina
    /// depende dos marcadores `.claude/touring` que a máquina tem acima do tmp (esta tem um
    /// `/tmp/.claude/touring` perdido, e a subida para ali em vez de no `casa` do teste). O que o
    /// defeito exige — e o que vale em toda máquina — é que a raiz NÃO seja o projeto do payload.
    #[test]
    fn caso_b_o_cwd_do_payload_vence_a_subida_estrutural() {
        let m = mundo();
        let trabalho = m.casa.join("trabalho");
        let sem_payload = escolhe(None, None, None, &trabalho, &m.casa);
        assert!(
            !sem_payload.starts_with(&m.analise),
            "premissa do defeito: sem cwd de payload a raiz ({}) não é o projeto que perguntou",
            sem_payload.display()
        );
        assert_eq!(
            escolhe(None, Some(&m.analise.join("sub")), None, &trabalho, &m.casa),
            m.analise
        );
    }

    /// `CLAUDE_PROJECT_DIR` continua PRIMEIRO — devolvido como veio, sem normalizar.
    #[test]
    fn claude_project_dir_continua_primeiro() {
        let m = mundo();
        assert_eq!(
            escolhe(
                Some(&m.touring),
                Some(&m.analise.join("sub")),
                None,
                &m.casa,
                &m.casa
            ),
            m.touring
        );
    }

    /// Sem payload a cadeia antiga vale: `TOURING_PROJECT_ROOT` manda; sem ele, a subida
    /// estrutural (cujo ponto de parada depende dos marcadores da máquina — ver o caso B) segue
    /// respondendo, e não é o cwd de payload que nunca existiu.
    #[test]
    fn sem_cwd_de_payload_a_cadeia_antiga_vale() {
        let m = mundo();
        assert_eq!(
            escolhe(None, None, Some(&m.touring), &m.casa, &m.casa),
            m.touring
        );
        let estrutural = escolhe(None, None, None, &m.casa, &m.casa);
        assert!(
            estrutural.is_absolute() && !estrutural.starts_with(&m.analise),
            "a subida estrutural responde sem payload: {}",
            estrutural.display()
        );
    }

    /// Um cwd de payload que não diz nada (relativo, inexistente, arquivo) é IGNORADO: nunca vira
    /// raiz, e um relativo jamais é resolvido contra o cwd do PROCESSO.
    #[test]
    fn cwd_de_payload_invalido_e_ignorado() {
        let m = mundo();
        let arquivo = m.analise.join("arquivo.txt");
        std::fs::write(&arquivo, "x").expect("arquivo");
        for ruim in [
            Path::new("sub/dir"),
            Path::new("/nao/existe/g97"),
            arquivo.as_path(),
        ] {
            assert_eq!(
                escolhe(None, Some(ruim), Some(&m.touring), &m.casa, &m.casa),
                m.touring,
                "cwd de payload inválido ({}) cai na cadeia antiga",
                ruim.display()
            );
        }
    }

    /// Um cwd sem marcador de projeto resolve como o daemon resolve: o HOME (a loja global).
    #[test]
    fn cwd_de_payload_sem_projeto_resolve_como_o_daemon() {
        let m = mundo();
        assert_eq!(
            escolhe(
                None,
                Some(&m.casa.join("trabalho")),
                Some(&m.touring),
                &m.touring,
                &m.casa
            ),
            m.casa
        );
    }
}
