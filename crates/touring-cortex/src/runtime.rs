//! CortexRuntime — Central coordinator for the Cortex hook engine.

use std::io::Read as _;
use std::path::PathBuf;
use std::sync::Arc;

use touring_intelligence::rl::evolution::LearningPersistence;
use touring_intelligence::rl::memory::recall::SemanticRecall;
use touring_intelligence::rl::memory::rlm::RlmMemory;

use crate::fascicles::dispatcher::FascicleDispatcher;
use crate::fascicles::evidence::Evidence;
use crate::fascicles::registry::HandlerRegistry;
use crate::handlers as builtin_handlers;
use crate::pipeline::Pipeline;
use crate::types::HookEvent;

/// Type alias for the knowledge database reference.
pub type KnowledgeRef = touring_hooks::knowledge::FileKnowledgeDB;

/// Errors returned by [`CortexRuntime::run`] (F-8 / RBP-03: a typed error in
/// place of the previous stringly-typed `Result<_, String>`). Each variant
/// carries the underlying message, so the sole consumer (`{e}` Display into
/// anyhow) is unaffected.
#[derive(Debug, thiserror::Error)]
pub enum CortexRuntimeError {
    /// The event string did not match any known hook event.
    #[error("Unknown event: {0}")]
    UnknownEvent(String),
    /// Reading the hook payload from stdin failed.
    #[error("stdin read failed: {0}")]
    Stdin(String),
    /// The dedicated Cortex worker thread could not be joined.
    #[error("Cortex thread error: {0}")]
    ThreadRecv(String),
    /// Cortex runtime initialisation failed inside the worker thread.
    #[error("{0}")]
    Init(String),
    /// Serialising the Cortex output to JSON failed.
    #[error("JSON serialize error: {0}")]
    Serialize(String),
}

/// The central Cortex runtime — coordinates initialization and execution.
#[derive(Debug)]
pub struct CortexRuntime {
    /// Shared reference to the knowledge database (knowledge.db).
    pub knowledge: Arc<KnowledgeRef>,
    /// RLM memory database (memory.db) — episodic memory entries.
    pub rlm: Option<Arc<RlmMemory>>,
    /// Semantic recall database (memory.db) — FTS5 chunks + embeddings.
    pub recall: Option<Arc<SemanticRecall>>,
    /// Learning persistence (graph.db — Wilson/Drift/QTable/LinUCB + hook events).
    pub persistence: Option<Arc<LearningPersistence>>,
    /// The handler pipeline.
    pub pipeline: Pipeline,
    /// Project root directory.
    pub project_root: PathBuf,
    /// FascicleDispatcher for O(1) handler dispatch (optional, lazy-initialized).
    /// Initialized in `init()` with an empty registry; can be populated later
    /// via `init_fascicle_dispatcher()` once a tokio runtime is available.
    fascicle_dispatcher: Option<Arc<FascicleDispatcher>>,
}

impl CortexRuntime {
    /// Initialize the Cortex runtime.
    ///
    /// 1. Detect project root
    /// 2. Open consolidated domain databases (knowledge.db, memory.db, graph.db)
    /// 3. Register all builtin handlers
    pub(crate) fn init() -> Result<Self, String> {
        let project_root = detect_project_root();

        // Ensure the consolidated DB directory exists
        let touring_dir = project_root.join(".claude").join("touring");
        if !touring_dir.exists() {
            std::fs::create_dir_all(&touring_dir)
                .map_err(|e| format!("Cannot create touring dir: {e}"))?;
        }

        // Use canonical paths from TouringConfig (consolidated DBs)
        let db_path = touring_foundation::TouringConfig::knowledge_db_canonical(&project_root);
        let knowledge =
            KnowledgeRef::new(&db_path).map_err(|e| format!("Cannot open knowledge DB: {e}"))?;
        // FileKnowledgeDB wraps rusqlite::Connection which is !Sync.
        // Arc is used for shared ownership, not cross-thread sharing.
        // The Cortex pipeline runs single-threaded (CLI model).
        #[allow(clippy::arc_with_non_send_sync)]
        let knowledge = Arc::new(knowledge);

        // Open RLM memory database (consolidated memory.db, fail-open)
        let memory_path = touring_foundation::TouringConfig::memory_db_canonical(&project_root);
        #[allow(clippy::arc_with_non_send_sync)]
        let rlm = RlmMemory::new(&memory_path).ok().map(Arc::new);

        // Open semantic recall database (shares consolidated memory.db, fail-open)
        #[allow(clippy::arc_with_non_send_sync)]
        let recall = SemanticRecall::new(&memory_path, 384).ok().map(Arc::new);

        // Open learning persistence (uses consolidated graph.db for Wilson/Drift/QTable)
        let graph_path = touring_foundation::TouringConfig::graph_db_canonical(&project_root);
        let persistence = {
            let p = LearningPersistence::new(&graph_path);
            if p.ensure_tables().is_ok() {
                Some(Arc::new(p))
            } else {
                None
            }
        };

        let mut pipeline = Pipeline::new();
        builtin_handlers::register_all(&mut pipeline);

        Ok(Self {
            knowledge,
            rlm,
            recall,
            persistence,
            pipeline,
            project_root,
            // Initialize FascicleDispatcher with empty registry.
            // Subscribing to tourinHooks broadcast requires a tokio runtime,
            // so init_fascicle_dispatcher() must be called separately after
            // the runtime is available (e.g., from hook_runtime when SessionBus is set).
            fascicle_dispatcher: Some(Arc::new(FascicleDispatcher::new(
                HandlerRegistry::new(),
                std::sync::mpsc::sync_channel(256).1,
            ))),
        })
    }

    /// Main entry point: parse event, read stdin, execute pipeline, print output.
    ///
    /// ALWAYS exits 0 — the Cortex must never block user operations on error.
    ///
    /// # Thread Safety
    ///
    /// This function is designed to be called from any context, including within
    /// an existing Tokio runtime. It spawns a dedicated thread to avoid conflicts
    /// with the caller's runtime context.
    pub fn run(event_str: &str) -> Result<(), CortexRuntimeError> {
        // Handle --help before initializing (no DB access needed)
        if matches!(event_str, "--help" | "-h" | "help") {
            Self::print_help();
            return Ok(());
        }

        // Parse event and read stdin in the current thread (no async needed)
        let event = HookEvent::parse(event_str)
            .ok_or_else(|| CortexRuntimeError::UnknownEvent(event_str.to_string()))?;
        let input = read_stdin().map_err(|e| CortexRuntimeError::Stdin(e.to_string()))?;

        // Run cortex processing in a dedicated thread with result passing via channel.
        // This avoids "Cannot start a runtime from within a runtime" panics when
        // called from within an existing Tokio context (e.g., touring-server).
        let (tx, rx) = std::sync::mpsc::channel::<Result<crate::CortexOutput, String>>();
        std::thread::spawn(move || {
            let rt = match Self::init() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            };
            let mut ctx = crate::context::CortexContext::from_input_full(
                event,
                input,
                Arc::clone(&rt.knowledge),
                rt.rlm.clone(),
                rt.recall.clone(),
                rt.persistence.clone(),
                rt.project_root.clone(),
            );
            let output = rt.pipeline.execute(&mut ctx);
            let _ = tx.send(Ok(output));
        });

        let output = rx
            .recv()
            .map_err(|e| CortexRuntimeError::ThreadRecv(e.to_string()))?
            .map_err(CortexRuntimeError::Init)?;

        // Only print JSON if there's something to say
        if output.hook_specific_output.is_some()
            || output.decision.is_some()
            || output.suppress_output
        {
            let json = serde_json::to_string(&output)
                .map_err(|e| CortexRuntimeError::Serialize(e.to_string()))?;
            println!("{json}");
        }

        Ok(())
    }

    /// Print cortex subcommand help to stderr.
    fn print_help() {
        eprintln!("touring cortex — Centralized hook execution engine");
        eprintln!();
        eprintln!("USAGE: touring cortex <event>");
        eprintln!();
        eprintln!("EVENTS:");
        eprintln!("  session-start    SessionStart hook (load knowledge)");
        eprintln!("  session-stop     Stop hook (persist session state)");
        eprintln!("  pre-read         PreToolUse Read (inject file knowledge)");
        eprintln!("  post-read        PostToolUse Read (learn from content)");
        eprintln!("  pre-bash         PreToolUse Bash (recall past outcomes)");
        eprintln!("  post-bash        PostToolUse Bash (capture outcome)");
        eprintln!("  pre-edit         PreToolUse Edit/Write (impact analysis)");
        eprintln!("  post-edit        PostToolUse Edit/Write (track changes)");
        eprintln!("  pre-compact      PreCompact (crystallize learnings)");
        eprintln!("  post-compact     PostCompact (recovery context)");
        eprintln!("  subagent-start   SubagentStart (context for subagent)");
        eprintln!("  subagent-stop    SubagentStop (capture subagent result)");
        eprintln!("  task-completed   TaskCompleted (verify test coverage)");
        eprintln!("  pre-tool         PreToolUse (generic)");
        eprintln!();
        eprintln!("Reads JSON from stdin (pipe). Outputs JSON to stdout.");
        eprintln!("ALWAYS exits 0 — hooks must never block user operations.");
    }

    /// Returns a reference to the FascicleDispatcher, if initialized.
    ///
    /// The dispatcher is created during `init()` with an empty registry.
    /// It can be populated via handler registration after initialization.
    #[inline]
    #[must_use]
    pub fn fascicle_dispatcher(&self) -> Option<&Arc<FascicleDispatcher>> {
        self.fascicle_dispatcher.as_ref()
    }

    /// Subscribes the FascicleDispatcher to Evidence broadcast from touring-hooks.
    ///
    /// This bridges from `tokio::sync::broadcast::Receiver<Evidence>` (touring-hooks)
    /// to `std::sync::mpsc::Receiver<Evidence>` (FascicleDispatcher).
    ///
    /// This is only possible when a Tokio runtime is available — the bridge
    /// spawns an async task that forwards evidence events from the broadcast
    /// channel to the std mpsc receiver that the FascicleDispatcher consumes.
    ///
    /// # Panics
    /// Panics if no Tokio runtime is currently active.
    pub fn subscribe_fascicle_to_touring_hooks_bus(
        &self,
        tokio_rx: tokio::sync::broadcast::Receiver<touring_simd::cortex::Evidence>,
    ) where
        Evidence: From<touring_simd::cortex::Evidence>,
    {
        let Some(fd) = &self.fascicle_dispatcher else {
            tracing::warn!("fascicle_dispatcher not initialized — cannot subscribe to bus");
            return;
        };
        // Clone the SyncSender from the dispatch channel before spawning.
        // Arc<FascicleDispatcher> is !Send (contains Receiver<T>), but
        // SyncSender<T> is Send + Clone + 'static — safe to move into async task.
        let std_sender = fd.dispatch_channel().sender();

        // Spawn bridge task in current Tokio runtime
        let rt_handle = tokio::runtime::Handle::current();
        rt_handle.spawn(async move {
            let mut tokio_rx = tokio_rx;
            loop {
                match tokio_rx.recv().await {
                    Ok(simd_evidence) => {
                        let cortex_evidence = Evidence::from(simd_evidence);
                        // IC-5: Forward directly to FascicleDispatcher dispatch channel.
                        // Previously created a temporary throwaway channel per event (data-loss).
                        if std_sender.try_send(cortex_evidence).is_err() {
                            tracing::warn!(
                                "fascicle_bridge: dispatch_channel full — evidence dropped"
                            );
                        } else {
                            tracing::trace!(
                                "fascicle_bridge: evidence forwarded to dispatch_channel"
                            );
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "fascicle_bridge: broadcast lagged");
                    }
                }
            }
        });
    }
}

/// Read and parse JSON from stdin with a 2-second timeout.
///
/// Returns `{}` immediately if stdin is a terminal. For non-terminal stdin
/// (pipes, sockets), reads with a timeout to prevent indefinite blocking
/// when no data is available (e.g. socket-based stdin in agent runners).
fn read_stdin() -> Result<serde_json::Value, String> {
    use std::io::IsTerminal;

    if std::io::stdin().is_terminal() {
        return Ok(serde_json::json!({}));
    }

    // Read stdin in a background thread with a timeout.
    // This handles the edge case where stdin is a socket (not a pipe)
    // that never sends data or EOF.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut input = String::new();
        let result = std::io::stdin().read_to_string(&mut input);
        let _ = tx.send((input, result));
    });

    match rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok((input, Ok(_))) if !input.trim().is_empty() => {
            serde_json::from_str(&input).map_err(|e| format!("JSON parse error: {e}"))
        }
        Ok(_) => Ok(serde_json::json!({})),
        Err(_) => Ok(serde_json::json!({})), // Timeout — no data available
    }
}

/// Detect project root from env var or cwd.
fn detect_project_root() -> PathBuf {
    std::env::var("CLAUDE_PROJECT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}
