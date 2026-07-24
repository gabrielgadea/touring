//! touring-web-server — library surface for the dashboard backend.
//!
//! The binary entrypoint at `src/main.rs` is a thin shim that calls
//! [`run`]. Tests (under `tests/`) use [`build_app`] together with
//! [`AppState::new_for_test`] / [`WsState::new_for_test`] to spin up a
//! `tower::Service` directly — no TCP bind, no subprocess.
//!
//! Public surface:
//! - [`AppState`] / [`WsState`] — typed state for the two router halves.
//! - [`build_app`] — wire all routes (V1 + V2 P10.9 history, P10.8 ws).
//! - [`run`] — convenience: bind 0.0.0.0:3000 and serve.
//! - [`AppError`] — IntoResponse-aware error envelope.
//! - [`snapshots::SnapshotStore`] — JSONL ring buffer (P10.9). Web-local
//!   store; NOT the `touring_server::snapshot::SnapshotStore` homonym
//!   (independent types — cross-audit 2026-06-11, F3).
//! - [`socket::ws_quality`] / [`socket::WsState`] — WebSocket handler (P10.8).

pub mod snapshots;
pub mod socket;
mod viz_graph;

use viz_graph::*;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderValue, Response, StatusCode, header},
    response::{IntoResponse, Json},
    routing::get,
};
use serde_json::Value;
use thiserror::Error;
use tower_http::{
    cors::{AllowOrigin, Any, CorsLayer},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

pub use snapshots::{DEFAULT_CAPACITY, SnapshotStore};
pub use socket::{WsState, ws_quality};

/// Application error envelope. Each variant maps to a meaningful HTTP
/// status (404 missing asset, 502 failed `touring`/`dot` subprocess,
/// 500 internal IO) and renders as a JSON body `{"error": "..."}` so
/// clients can branch on status AND message. (The old impl rendered
/// every variant as `500 text/plain` — cross-audit 2026-06-11, F-06.)
#[derive(Error, Debug)]
pub enum AppError {
    /// `touring` CLI invocation failed.
    #[error("touring command failed: {0}")]
    TouringCommand(String),
    /// `touring` CLI returned non-JSON stdout.
    #[error("touring output parse failed")]
    TouringParse,
    /// `dot` (graphviz) post-processing failed.
    #[error("DOT processing failed: {0}")]
    DotProcess(String),
    /// Generic IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// UTF-8 decode error.
    #[error("UTF-8 parse error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    /// Static asset missing under `dist_path`.
    #[error("File not found: {0}")]
    FileNotFound(String),
}

impl AppError {
    /// HTTP status for this error kind — 502 for upstream subprocess
    /// failures (`touring`/`dot`), 404 for missing static assets, 500
    /// for internal IO/encoding errors.
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::TouringCommand(_) | AppError::TouringParse | AppError::DotProcess(_) => {
                StatusCode::BAD_GATEWAY
            }
            AppError::FileNotFound(_) => StatusCode::NOT_FOUND,
            AppError::Io(_) | AppError::Utf8(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response<Body> {
        let body = serde_json::json!({ "error": self.to_string() }).to_string();
        Response::builder()
            .status(self.status_code())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("Internal error"))
                    .expect("static fallback response always builds")
            })
    }
}

/// Shared state for the V1 router half (REST + SSE).
#[derive(Clone)]
pub struct AppState {
    /// Filesystem root for the WASM bundle (`dist/`).
    pub dist_path: std::path::PathBuf,
    /// Working directory for `touring` CLI invocations.
    pub project_path: std::path::PathBuf,
    /// Workspace root (used by `viz workspace`).
    pub workspace_path: std::path::PathBuf,
    /// Persistent ring buffer of past quality signals (P10.9).
    pub snapshot_store: Arc<SnapshotStore>,
}

impl AppState {
    /// Build a deterministic test state. Snapshot file lives inside
    /// `tmp_dir`; CLI calls run from `tmp_dir` so they are unlikely to
    /// succeed and tests can rely on the `{"error": ...}` envelope.
    pub fn new_for_test(tmp_dir: std::path::PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&tmp_dir);
        let store = Arc::new(SnapshotStore::new(
            tmp_dir.join("snapshots.jsonl"),
            DEFAULT_CAPACITY,
        ));
        Self {
            dist_path: tmp_dir.clone(),
            project_path: tmp_dir.clone(),
            workspace_path: tmp_dir,
            snapshot_store: store,
        }
    }
}

/// GET /api/health
pub async fn api_health(State(state): State<AppState>) -> Json<Value> {
    let output = tokio::process::Command::new("touring")
        .current_dir(&state.project_path)
        .args(["doctor", "-j"])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => serde_json::from_slice::<Value>(&o.stdout)
            .map(Json)
            .unwrap_or_else(|_| Json(serde_json::json!({"error": "parse error"}))),
        _ => Json(serde_json::json!({"error": "touring unavailable"})),
    }
}

/// GET /api/status
pub async fn api_status(State(state): State<AppState>) -> Json<Value> {
    let output = tokio::process::Command::new("touring")
        .current_dir(&state.project_path)
        .args(["status", "-j"])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => serde_json::from_slice::<Value>(&o.stdout)
            .map(Json)
            .unwrap_or_else(|_| Json(serde_json::json!({"error": "parse error"}))),
        _ => Json(serde_json::json!({"error": "touring unavailable"})),
    }
}

/// GET /api/orphans
pub async fn api_orphans(State(state): State<AppState>) -> Json<Value> {
    let output = tokio::process::Command::new("touring")
        .current_dir(&state.project_path)
        // clap-derive migration (2026-06-10) dropped `-j` here; the command
        // now emits JSON by default (cross-audit 2026-06-11, F-01).
        .args(["wiring", "orphans"])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => serde_json::from_slice::<Value>(&o.stdout)
            .map(Json)
            .unwrap_or_else(|_| Json(serde_json::json!({"error": "parse error"}))),
        _ => Json(serde_json::json!({"error": "touring unavailable"})),
    }
}

/// GET `/api/search?q=<query>`
pub async fn api_search(
    State(state): State<AppState>,
    Query(p): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let q = p.get("q").cloned().unwrap_or_default();
    let output = tokio::process::Command::new("touring")
        .current_dir(&state.project_path)
        .args(["index", "find", &q, "-j"])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => serde_json::from_slice::<Value>(&o.stdout)
            .map(Json)
            .unwrap_or_else(|_| Json(serde_json::json!({"error": "parse error"}))),
        _ => Json(serde_json::json!({"error": "touring unavailable"})),
    }
}

/// GET /api/wiring/modules
pub async fn api_wiring_modules(State(state): State<AppState>) -> Json<Value> {
    let output = tokio::process::Command::new("touring")
        .current_dir(&state.project_path)
        // Same clap-derive migration note as api_orphans: JSON is the default.
        .args(["wiring", "modules"])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => serde_json::from_slice::<Value>(&o.stdout)
            .map(Json)
            .unwrap_or_else(|_| Json(serde_json::json!({"error": "parse error"}))),
        _ => Json(serde_json::json!({"error": "touring unavailable"})),
    }
}

/// GET `/api/memory?q=<query>`
pub async fn api_memory(
    State(state): State<AppState>,
    Query(p): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let q = p.get("q").cloned().unwrap_or_default();
    let output = tokio::process::Command::new("touring")
        .current_dir(&state.project_path)
        // clap-derive migration dropped `-j` on `memory recall` too — JSON
        // is the default output (cross-audit 2026-06-11, follow-up to F-01).
        .args(["memory", "recall", &q])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => serde_json::from_slice::<Value>(&o.stdout)
            .map(Json)
            .unwrap_or_else(|_| Json(serde_json::json!({"error": "parse error"}))),
        _ => Json(serde_json::json!({"error": "touring unavailable"})),
    }
}

/// GET /api/memory/stats — aggregate counters from `touring memory stats`
/// (`memory_entry_count`, `file_count`, `relation_count`, `gotcha_stats`).
/// Feeds the Memory page KPI strip, which previously rendered static "—"
/// placeholders (cross-audit 2026-06-11, Wave 3 gap closure).
pub async fn api_memory_stats(State(state): State<AppState>) -> Json<Value> {
    let output = tokio::process::Command::new("touring")
        .current_dir(&state.project_path)
        .args(["memory", "stats"])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => serde_json::from_slice::<Value>(&o.stdout)
            .map(Json)
            .unwrap_or_else(|_| Json(serde_json::json!({"error": "parse error"}))),
        _ => Json(serde_json::json!({"error": "touring unavailable"})),
    }
}

// ──────────────────────────────────────────────────────────────────
// Wave 4 (2026-06-12) — endpoints feeding the new zip-artboard pages
// (/hooks /plans /sessions /cognitive /chains /settings). All wrap the
// touring CLI via shell_touring_value; clap-derive migration note: none
// of these subcommands accept `-j` — JSON is their default output.
// ──────────────────────────────────────────────────────────────────

/// GET /api/gate-metrics — 142 live hook/gate counters incl. latency
/// histograms (`*_latency: {count,p50_us,p90_us,p99_us,p999_us,max_us}`).
pub async fn api_gate_metrics(State(state): State<AppState>) -> Json<Value> {
    Json(shell_touring_value(&state.project_path, &["gate-metrics"]).await)
}

/// GET /api/sessions — `{count, sessions:[{session_id, objective,
/// task_type, created_at, updated_at}]}`.
pub async fn api_sessions(State(state): State<AppState>) -> Json<Value> {
    Json(shell_touring_value(&state.project_path, &["session"]).await)
}

// ── Elite W2-W4 endpoints (SPEC 2026-06-12 §7.2) — every shape was
//    captured by real CLI execution before the handler was written. ──

/// GET /api/learning/status — RL state `{update_count, ema_reward,
/// mean_td_error, linucb_loaded, bandit_type, arm_count}` (already JSON).
pub async fn api_learning_status(State(state): State<AppState>) -> Json<Value> {
    Json(shell_touring_value(&state.project_path, &["learning", "status"]).await)
}

/// GET /api/memory/recall?q= — RRF-fused recall `{count, entries:[{key,…}]}`.
pub async fn api_memory_recall(
    State(state): State<AppState>,
    Query(p): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let q = p.get("q").cloned().unwrap_or_default();
    if q.trim().is_empty() {
        return Json(serde_json::json!({"count": 0, "entries": [], "note": "empty query"}));
    }
    Json(shell_touring_value(&state.project_path, &["memory", "recall", &q]).await)
}

/// GET /api/wiring/impact?symbol=&depth= — BFS blast radius
/// `{symbol, direct_consumers, total_transitive, max_depth, paths:[…]}`.
pub async fn api_wiring_impact(
    State(state): State<AppState>,
    Query(p): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let symbol = p.get("symbol").cloned().unwrap_or_default();
    if symbol.trim().is_empty() {
        return Json(serde_json::json!({"error": "symbol query param required"}));
    }
    let depth = p
        .get("depth")
        .and_then(|d| d.parse::<u8>().ok())
        .map(|d| d.clamp(1, 5))
        .unwrap_or(2)
        .to_string();
    Json(
        shell_touring_value(
            &state.project_path,
            &[
                "wiring", "impact", &symbol, "--depth", &depth, "--format", "json",
            ],
        )
        .await,
    )
}

/// GET /api/wiring/suggest?symbol= — wiring suggestions for one orphan
/// (or the all-orphan scan when `symbol` is omitted).
pub async fn api_wiring_suggest(
    State(state): State<AppState>,
    Query(p): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    match p
        .get("symbol")
        .map(String::as_str)
        .filter(|s| !s.trim().is_empty())
    {
        Some(sym) => {
            Json(shell_touring_value(&state.project_path, &["wiring", "suggest", sym]).await)
        }
        None => Json(shell_touring_value(&state.project_path, &["wiring", "suggest"]).await),
    }
}

/// GET /api/decompose/task?id= — one task DAG
/// `{task, subtask_count, subtasks:[…]}`.
pub async fn api_decompose_task(
    State(state): State<AppState>,
    Query(p): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let id = p.get("id").cloned().unwrap_or_default();
    if id.trim().is_empty() {
        return Json(serde_json::json!({"error": "id query param required"}));
    }
    Json(shell_touring_value(&state.project_path, &["decompose", "get", &id]).await)
}

/// GET /api/sessions/{id} — `touring session assess` detail
/// `{assessed_at, health_summary, metrics, quality_score, session_id}`.
pub async fn api_session_detail(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<Value> {
    Json(shell_touring_value(&state.project_path, &["session", "assess", &id]).await)
}

/// Curated MCP tool whitelist — every tool maps 1:1 to a FIXED read-only
/// `touring` argv template (SPEC 2026-06-12 §6.1). `{arg}` placeholders
/// are substituted with the validated request arg; nothing else from the
/// request ever reaches the command line (no arbitrary shell, ever).
pub const MCP_TOOL_WHITELIST: &[(&str, &str, &[&str])] = &[
    (
        "touring_status",
        "Composite daemon/index/wiring/RL dashboard",
        &["status", "-j"],
    ),
    (
        "touring_doctor",
        "Daemon + index health gate (6 components)",
        &["doctor", "-j"],
    ),
    (
        "touring_e2e",
        "End-to-end composite system score",
        &["e2e", "-j"],
    ),
    (
        "touring_gate_metrics",
        "142 live hook/gate counters",
        &["gate-metrics"],
    ),
    (
        "touring_quality_signal",
        "Sentrux 5-axis quality signal",
        &["quality-signal", "-j"],
    ),
    (
        "touring_wiring_orphans",
        "Orphan pub symbols registry",
        &["wiring", "orphans", "-j"],
    ),
    (
        "touring_wiring_impact",
        "BFS blast radius of a symbol",
        &[
            "wiring", "impact", "{arg}", "--depth", "3", "--format", "json",
        ],
    ),
    (
        "touring_index_find",
        "Exact symbol lookup",
        &["index", "find", "{arg}", "-j"],
    ),
    (
        "touring_memory_recall",
        "RRF-fused memory recall",
        &["memory", "recall", "{arg}"],
    ),
    (
        "touring_learning_status",
        "RL bandit state",
        &["learning", "status"],
    ),
    (
        "touring_decompose_status",
        "Global task DAG totals",
        &["decompose", "status"],
    ),
    (
        "touring_decompose_templates",
        "W1-W10 workflow templates",
        &["decompose", "templates"],
    ),
    ("touring_jobs_list", "Async job registry", &["jobs", "list"]),
    (
        "touring_session_list",
        "Tracked agent sessions",
        &["session"],
    ),
    (
        "touring_suggest_next",
        "RL next-action recommendation",
        &["suggest", "next"],
    ),
];

/// Validate a user-supplied tool arg: bounded length, conservative
/// charset (symbol/path/query-safe), no leading dash (no flag injection).
pub fn valid_tool_arg(arg: &str) -> bool {
    !arg.is_empty()
        && arg.len() <= 200
        && !arg.starts_with('-')
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '/' | '-' | ' '))
}

/// Resolve a whitelisted tool + optional arg into the final argv.
/// Returns `Err(reason)` for unknown tools / invalid / missing args.
pub fn resolve_tool_argv(tool: &str, arg: Option<&str>) -> Result<Vec<String>, ResolveToolError> {
    let (_, _, template) = MCP_TOOL_WHITELIST
        .iter()
        .find(|(name, _, _)| *name == tool)
        .ok_or_else(|| format!("unknown tool: {tool} (not in whitelist)"))?;
    let needs_arg = template.contains(&"{arg}");
    match (needs_arg, arg) {
        (true, None) => Err(format!("tool {tool} requires an arg").into()),
        (true, Some(a)) if !valid_tool_arg(a) => Err(
            "invalid arg: 1-200 chars of [A-Za-z0-9_:./- ], no leading dash"
                .to_string()
                .into(),
        ),
        (true, Some(a)) => Ok(template
            .iter()
            .map(|t| {
                if *t == "{arg}" {
                    a.to_string()
                } else {
                    (*t).to_string()
                }
            })
            .collect()),
        (false, _) => Ok(template.iter().map(|t| (*t).to_string()).collect()),
    }
}

/// Error from [`resolve_tool_argv`] (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ResolveToolError(pub String);

impl From<String> for ResolveToolError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

/// GET /api/mcp/tools — the curated whitelist catalog.
pub async fn api_mcp_tools(State(_state): State<AppState>) -> Json<Value> {
    let tools: Vec<Value> = MCP_TOOL_WHITELIST
        .iter()
        .map(|(name, doc, template)| {
            serde_json::json!({
                "name": name,
                "doc": doc,
                "needs_arg": template.contains(&"{arg}"),
                "cli_equivalent": format!("touring {}", template.join(" ")),
            })
        })
        .collect();
    Json(serde_json::json!({"count": tools.len(), "tools": tools}))
}

/// POST /api/mcp/call — execute (or dry-run) a whitelisted tool.
/// Body: `{tool, arg?, dry_run?}` — dry_run DEFAULTS TO TRUE.
pub async fn api_mcp_call(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let tool = body
        .get("tool")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let arg = body
        .get("arg")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty());
    let dry_run = body
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let argv = match resolve_tool_argv(tool, arg) {
        Ok(a) => a,
        Err(e) => return Json(serde_json::json!({"error": e.to_string()})),
    };
    let cli = format!("touring {}", argv.join(" "));
    if dry_run {
        return Json(serde_json::json!({"dry_run": true, "argv": argv, "cli": cli}));
    }
    let argv_ref: Vec<&str> = argv.iter().map(String::as_str).collect();
    let output = shell_touring_value(&state.project_path, &argv_ref).await;
    Json(serde_json::json!({"dry_run": false, "cli": cli, "output": output}))
}

/// GET /api/jobs — async job registry (`touring jobs list`).
pub async fn api_jobs(State(state): State<AppState>) -> Json<Value> {
    Json(shell_touring_value(&state.project_path, &["jobs", "list"]).await)
}

/// GET `/api/speculate?file=<rel>` — run `touring shadow validate` over the
/// CURRENT content of a project file (6-layer speculative score: Syntax /
/// SymbolResolution / Structural / ImportCheck / Complexity / CfgImpact).
/// The path is resolved under the project root and canonicalized — any
/// escape attempt is rejected.
pub async fn api_speculate(
    State(state): State<AppState>,
    Query(p): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let rel = p.get("file").cloned().unwrap_or_default();
    if rel.trim().is_empty() {
        return Json(serde_json::json!({"error": "file query param required"}));
    }
    let root = match tokio::fs::canonicalize(&state.project_path).await {
        Ok(r) => r,
        Err(e) => return Json(serde_json::json!({"error": format!("bad project root: {e}")})),
    };
    let candidate = match tokio::fs::canonicalize(root.join(&rel)).await {
        Ok(c) => c,
        Err(_) => return Json(serde_json::json!({"error": format!("file not found: {rel}")})),
    };
    if !candidate.starts_with(&root) {
        return Json(serde_json::json!({"error": "path escapes project root"}));
    }
    let content = match tokio::fs::read_to_string(&candidate).await {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"error": format!("read failed: {e}")})),
    };
    if content.len() > 512 * 1024 {
        return Json(serde_json::json!({"error": "file too large for speculation (512 KiB max)"}));
    }
    let payload = serde_json::json!({"file_path": rel, "content": content}).to_string();
    let child = tokio::process::Command::new("touring")
        .current_dir(&state.project_path)
        .args(["shadow", "validate"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"error": format!("spawn failed: {e}")})),
    };
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        if let Err(e) = stdin.write_all(payload.as_bytes()).await {
            return Json(serde_json::json!({"error": format!("stdin write failed: {e}")}));
        }
    }
    match child.wait_with_output().await {
        Ok(o) if o.status.success() => serde_json::from_slice::<Value>(&o.stdout)
            .map(Json)
            .unwrap_or_else(|_| {
                Json(serde_json::json!({"error": "shadow validate emitted non-JSON"}))
            }),
        Ok(o) => Json(serde_json::json!({
            "error": format!("shadow validate exited {}", o.status),
        })),
        Err(e) => Json(serde_json::json!({"error": format!("wait failed: {e}")})),
    }
}

/// Canonical on-disk location of the editable quality ruleset — a FIXED
/// path under the project root (never taken from the request: no
/// traversal surface). Schema: `touring-analysis::rules::MetricRuleSet`.
pub fn quality_rules_path(project_path: &std::path::Path) -> std::path::PathBuf {
    project_path
        .join(".claude")
        .join("touring")
        .join("quality-rules.toml")
}

/// Starter ruleset served when no file exists yet — the canonical
/// `MetricRuleSet` v1.0 example from `touring-analysis/src/rules/parser.rs`.
pub const DEFAULT_QUALITY_RULES: &str = r#"version = "1.0"

[[rule]]
name = "no-god-files"
applies_to = "**/*.rs"
metric = "file_lines"
op = "lt"
threshold = 1000
severity = "warn"
message = "files should be < 1000 LOC"

[[rule]]
name = "no-cycles"
metric = "cycle_count"
op = "eq"
threshold = 0
severity = "deny"
"#;

/// GET /api/quality/rules — `{exists, path, content}` for the canonical
/// editable ruleset (default template when the file is absent).
pub async fn api_quality_rules_get(State(state): State<AppState>) -> Json<Value> {
    let path = quality_rules_path(&state.project_path);
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => Json(serde_json::json!({
            "exists": true,
            "path": path.display().to_string(),
            "content": content,
        })),
        Err(_) => Json(serde_json::json!({
            "exists": false,
            "path": path.display().to_string(),
            "content": DEFAULT_QUALITY_RULES,
        })),
    }
}

/// PUT /api/quality/rules — persist the ruleset after TOML validation.
/// Body is the raw TOML text; replies `{saved, path}` or `{error}` with
/// the real parse message (line/column) so the editor can surface it.
pub async fn api_quality_rules_put(State(state): State<AppState>, body: String) -> Json<Value> {
    if body.len() > 64 * 1024 {
        return Json(serde_json::json!({"error": "ruleset too large (64 KiB max)"}));
    }
    if let Err(e) = body.parse::<toml::Table>() {
        return Json(serde_json::json!({"error": format!("invalid TOML: {e}")}));
    }
    let path = quality_rules_path(&state.project_path);
    if let Some(parent) = path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return Json(serde_json::json!({"error": format!("mkdir failed: {e}")}));
        }
    }
    match tokio::fs::write(&path, &body).await {
        Ok(()) => Json(serde_json::json!({
            "saved": true,
            "path": path.display().to_string(),
            "bytes": body.len(),
        })),
        Err(e) => Json(serde_json::json!({"error": format!("write failed: {e}")})),
    }
}

/// GET /api/decompose — global DAG totals `{total_tasks, total_subtasks}`.
pub async fn api_decompose(State(state): State<AppState>) -> Json<Value> {
    Json(shell_touring_value(&state.project_path, &["decompose", "status"]).await)
}

/// GET /api/decompose/templates — the 10 reusable workflow templates W1-W10.
pub async fn api_decompose_templates(State(state): State<AppState>) -> Json<Value> {
    Json(shell_touring_value(&state.project_path, &["decompose", "templates"]).await)
}

/// GET /api/decompose/ready — `{ready_subtasks, blocked_subtasks,
/// parallel_groups, ...}` for the active DAG.
pub async fn api_decompose_ready(State(state): State<AppState>) -> Json<Value> {
    Json(shell_touring_value(&state.project_path, &["decompose", "ready"]).await)
}

/// GET /api/cognitive — quality-delta metrics `{has_graph, has_predictor,
/// initialized}`.
pub async fn api_cognitive(State(state): State<AppState>) -> Json<Value> {
    Json(shell_touring_value(&state.project_path, &["cognitive", "metrics"]).await)
}

/// GET /api/cognitive/engines — health map keyed by engine name, e.g.
/// `{cognitive_runtime:{status,graph}, crdt_graph:{status}, ...}`.
pub async fn api_cognitive_engines(State(state): State<AppState>) -> Json<Value> {
    Json(shell_touring_value(&state.project_path, &["cognitive", "engines"]).await)
}

/// GET /api/wiring/chains — `{chain_count, rebuilt, chains?:[...]}`.
pub async fn api_wiring_chains(State(state): State<AppState>) -> Json<Value> {
    Json(shell_touring_value(&state.project_path, &["wiring", "chains"]).await)
}

// ──────────────────────────────────────────────────────────────────
// Wave 4 P10 (2026-05-09) — Sentrux dashboard endpoints.
// ──────────────────────────────────────────────────────────────────

/// GET /api/quality/signal[?root=&no_diagnostics=1]
///
/// Wraps `touring e2e -j` (the canonical quality scorer) and reshapes its
/// phase-array output into the `WorkspaceQualitySignal` shape consumed
/// by the dashboard. Mapping:
///
/// | Dashboard axis  | Source phase / metric                       |
/// |-----------------|---------------------------------------------|
/// | signal_0_10000  | `overall_score * 10_000`                    |
/// | signal_normalized| `overall_score`                             |
/// | bottleneck      | phase with lowest `score`                   |
/// | modularity      | analysis / knowledge phase score             |
/// | acyclicity      | `1.0 - wiring.orphan_rate_pct/100` — orphan-rate PROXY, not a cycle scan |
/// | depth           | index phase score                            |
/// | equality        | learning phase score (test pass rate proxy)  |
/// | redundancy      | knowledge phase score                        |
/// | raw.cycle_count | `wiring.broken_chains` — broken-chain PROXY, not Tarjan SCC |
/// | raw.total_*     | `index.symbol_count`, `wiring.total_pub_symbols` |
pub async fn api_quality_signal(
    State(state): State<AppState>,
    Query(p): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let target = p
        .get("root")
        .cloned()
        .unwrap_or_else(|| state.workspace_path.to_string_lossy().to_string());
    let cwd = if std::path::Path::new(&target).is_absolute() {
        std::path::PathBuf::from(&target)
    } else {
        state.workspace_path.clone()
    };

    let output = tokio::process::Command::new("touring")
        .current_dir(&cwd)
        .args(["e2e", "-j"])
        .output()
        .await;

    let raw: Value = match output {
        Ok(o) if o.status.success() => serde_json::from_slice(&o.stdout)
            .unwrap_or_else(|_| serde_json::json!({"error": "parse error"})),
        Ok(o) => serde_json::json!({
            "error":  "touring exited non-zero",
            "stderr": String::from_utf8_lossy(&o.stderr),
        }),
        Err(e) => serde_json::json!({"error": format!("spawn failed: {e}")}),
    };

    Json(reshape_e2e_to_quality_signal(&raw, &target))
}

/// Map a `touring e2e -j` envelope into the `WorkspaceQualitySignal`
/// shape used by the WASM dashboard. Errors propagate via the `error`
/// key while still emitting numeric defaults so the radar/sparkline
/// have something to render.
fn reshape_e2e_to_quality_signal(raw: &Value, root: &str) -> Value {
    if let Some(err) = raw.get("error").and_then(Value::as_str) {
        return serde_json::json!({
            "signal_0_10000":    0_u32,
            "signal_normalized": 0.0_f64,
            "bottleneck":        "unavailable",
            "root_causes":       {"modularity":0.0,"acyclicity":0.0,"depth":0.0,"equality":0.0,"redundancy":0.0},
            "raw":               {"modularity_q":0.0,"cycle_count":0,"max_depth":0,"complexity_gini":0.0,"redundancy_ratio":0.0,"total_functions":0,"total_nodes":0,"total_edges":0},
            "root":              root,
            "error":             err,
        });
    }

    let overall = raw
        .get("overall_score")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let phases = raw
        .get("phases")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let phase = |name: &str| -> Value {
        phases
            .iter()
            .find(|p| p.get("phase").and_then(Value::as_str) == Some(name))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}))
    };
    let phase_score = |name: &str| -> f64 {
        phase(name)
            .get("score")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };

    let wiring = phase("wiring");
    let index_p = phase("index");

    let modularity = phase_score("analysis").max(phase_score("knowledge"));
    let orphan_rate = wiring
        .get("metrics")
        .and_then(|m| m.get("orphan_rate_pct"))
        .and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .or(v.as_f64())
        })
        .unwrap_or(0.0);
    let acyclicity = (1.0 - orphan_rate / 100.0).clamp(0.0, 1.0);
    let depth = phase_score("index");
    let equality = phase_score("learning");
    let redundancy = phase_score("knowledge");

    // Bottleneck = lowest-score phase.
    let mut min_phase = ("balanced".to_string(), 1.1_f64);
    for p in &phases {
        if let (Some(name), Some(score)) = (
            p.get("phase").and_then(Value::as_str),
            p.get("score").and_then(Value::as_f64),
        ) {
            if score < min_phase.1 {
                min_phase = (name.to_string(), score);
            }
        }
    }
    let bottleneck = match min_phase.0.as_str() {
        "analysis" | "knowledge" => "modularity",
        "wiring" => "acyclicity",
        "index" => "depth",
        "learning" => "equality",
        "runtime" => "redundancy",
        _ => "balanced",
    };

    let total_functions = index_p
        .get("metrics")
        .and_then(|m| m.get("symbol_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_edges = wiring
        .get("metrics")
        .and_then(|m| m.get("total_consumers"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_nodes = wiring
        .get("metrics")
        .and_then(|m| m.get("total_pub_symbols"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cycle_count = wiring
        .get("metrics")
        .and_then(|m| m.get("broken_chains"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    serde_json::json!({
        "signal_0_10000":    (overall * 10_000.0) as u32,
        "signal_normalized": overall,
        "bottleneck":        bottleneck,
        "root_causes": {
            "modularity": modularity,
            "acyclicity": acyclicity,
            "depth":      depth,
            "equality":   equality,
            "redundancy": redundancy,
        },
        "raw": {
            "modularity_q":      modularity,
            "cycle_count":       cycle_count,
            "max_depth":         0,
            "complexity_gini":   0.0,
            "redundancy_ratio":  1.0 - redundancy,
            "total_functions":   total_functions,
            "total_nodes":       total_nodes,
            "total_edges":       total_edges,
        },
        "root":  root,
        "error": Value::Null,
    })
}

/// GET /api/quality/rules/evaluate?ruleset_path=...
pub async fn api_quality_rules_evaluate(
    State(state): State<AppState>,
    Query(p): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let ruleset = p.get("ruleset_path").cloned().unwrap_or_default();
    if ruleset.is_empty() {
        return Json(serde_json::json!({"error": "missing ruleset_path"}));
    }
    let mut args: Vec<String> = vec![
        "quality-signal".into(),
        "-j".into(),
        "--rules".into(),
        ruleset,
    ];
    if let Some(root) = p.get("root") {
        args.push("--root".into());
        args.push(root.clone());
    }
    shell_touring_json(&state.project_path, &args).await
}

/// GET /api/quality/diff?prev=&curr=
pub async fn api_quality_diff(
    State(state): State<AppState>,
    Query(p): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let prev = match p.get("prev") {
        Some(v) if !v.is_empty() => v.clone(),
        _ => return Json(serde_json::json!({"error": "missing prev path"})),
    };
    let curr = match p.get("curr") {
        Some(v) if !v.is_empty() => v.clone(),
        _ => return Json(serde_json::json!({"error": "missing curr path"})),
    };
    let prev_args = [
        "quality-signal",
        "-j",
        "--root",
        prev.as_str(),
        "--no-diagnostics",
    ];
    let curr_args = [
        "quality-signal",
        "-j",
        "--root",
        curr.as_str(),
        "--no-diagnostics",
    ];
    let (a, b) = tokio::join!(
        shell_touring_value(&state.project_path, &prev_args),
        shell_touring_value(&state.project_path, &curr_args),
    );
    Json(serde_json::json!({
        "previous":      a,
        "current":       b,
        "previous_root": prev,
        "current_root":  curr,
    }))
}

/// Parses the `workspaces` spec (`name:/path` pairs, comma-separated)
/// into filesystem roots. A bare `/path` entry (no `name:` prefix) is
/// accepted as-is — the old `split(':').nth(1)` silently dropped such
/// entries, and a trailing `name:` yielded an empty root (cross-audit
/// 2026-06-11, F-08).
fn parse_federation_roots(spec: &str) -> Vec<String> {
    spec.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| match s.split_once(':') {
            Some((_name, path)) => {
                let p = path.trim();
                (!p.is_empty()).then(|| p.to_string())
            }
            None => Some(s.to_string()),
        })
        .collect()
}

/// GET /api/quality/federation?workspaces=ws1:/path,ws2:/path
pub async fn api_quality_federation(
    State(state): State<AppState>,
    Query(p): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let spec = p.get("workspaces").cloned().unwrap_or_default();
    if spec.is_empty() {
        return Json(serde_json::json!({"error": "missing workspaces"}));
    }
    let roots = parse_federation_roots(&spec);
    if roots.is_empty() {
        return Json(serde_json::json!({"error": "no roots in spec"}));
    }
    let federate_arg = roots.join(",");
    let args = ["status", "-j", "--federate", federate_arg.as_str()];
    let value = shell_touring_value(&state.project_path, &args).await;
    Json(value.get("federation").cloned().unwrap_or(value))
}

/// Shared shell helper — invoke `touring` with args and parse stdout JSON.
async fn shell_touring_json(project: &std::path::Path, args: &[String]) -> Json<Value> {
    Json(shell_touring_value(project, args).await)
}

async fn shell_touring_value<S: AsRef<str>>(project: &std::path::Path, args: &[S]) -> Value {
    let argv: Vec<String> = args.iter().map(|s| s.as_ref().to_string()).collect();
    let output = tokio::process::Command::new("touring")
        .current_dir(project)
        .args(&argv)
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => serde_json::from_slice::<Value>(&o.stdout)
            .unwrap_or_else(|_| serde_json::json!({"error": "parse error"})),
        Ok(o) => serde_json::json!({
            "error":  "touring exited non-zero",
            "stderr": String::from_utf8_lossy(&o.stderr),
        }),
        Err(e) => serde_json::json!({"error": format!("spawn failed: {e}")}),
    }
}

// ──────────────────────────────────────────────────────────────────
// Wave 4 P10.7 (2026-05-09) — Server-Sent Events for live dashboard.
// ──────────────────────────────────────────────────────────────────

use axum::response::sse::{Event, KeepAlive, Sse};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::IntervalStream;

/// GET /api/events
pub async fn api_events(
    State(state): State<AppState>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let interval = tokio::time::interval(Duration::from_secs(10));
    let project = state.project_path.clone();
    let store = state.snapshot_store.clone();

    // Async subprocess like every other handler — the previous
    // std::thread::spawn + blocking std::process::Command parked an OS
    // thread per tick and blocked the SSE poll on .join() (cross-audit
    // 2026-06-11, F-02).
    let stream = IntervalStream::new(interval).then(move |_tick| {
        let project = project.clone();
        let store = store.clone();
        async move {
            let value = tokio::process::Command::new("touring")
                .current_dir(&project)
                .args(["quality-signal", "-j", "--no-diagnostics"])
                .output()
                .await
                .ok()
                .and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok())
                .unwrap_or_else(|| serde_json::json!({"error": "snapshot unavailable"}));

            // P10.9 — mirror to persistent ring/JSONL.
            store.append(value.clone());

            Ok(Event::default()
                .json_data(value)
                .unwrap_or_else(|_| Event::default()))
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// GET /api/quality/history?limit=N — last N persisted snapshots (newest first).
pub async fn api_quality_history(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let limit: usize = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100)
        .min(1000);
    let snaps = state.snapshot_store.recent(limit);
    Json(serde_json::json!({
        "count":  snaps.len(),
        "empty":  state.snapshot_store.is_empty(),
        "limit":  limit,
        "snapshots": snaps,
    }))
}

/// GET /api/viz/wiring/svg
pub async fn api_viz_svg(State(state): State<AppState>) -> axum::response::Result<Response<Body>> {
    let dot_output = tokio::process::Command::new("touring")
        .current_dir(&state.project_path)
        .args(["viz", "wiring"])
        .output()
        .await
        .map_err(|e| AppError::TouringCommand(e.to_string()))?;
    if !dot_output.status.success() {
        return Err(AppError::TouringCommand(
            String::from_utf8_lossy(&dot_output.stderr).to_string(),
        )
        .into());
    }
    let dot_input = dot_output.stdout;
    let mut dot_child = tokio::process::Command::new("dot")
        .args(["-Tsvg"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| AppError::DotProcess(e.to_string()))?;
    if let Some(mut stdin) = dot_child.stdin.take() {
        tokio::io::AsyncWriteExt::write_all(&mut stdin, &dot_input)
            .await
            .map_err(|e| AppError::DotProcess(e.to_string()))?;
        drop(stdin);
    }
    let output = dot_child
        .wait_with_output()
        .await
        .map_err(|e| AppError::DotProcess(e.to_string()))?;
    if !output.status.success() {
        return Err(
            AppError::DotProcess(String::from_utf8_lossy(&output.stderr).to_string()).into(),
        );
    }
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/svg+xml")
        .body(Body::from(output.stdout))
        .expect("valid svg response");
    Ok(response)
}

/// GET /api/viz/workspace[?include_orphans=0&include_tests=0&enrich=0]
///
/// V1 path: shells out to `touring viz workspace --format json` (file-level
/// dependency graph from the wiring DB).
///
/// V2 enrichment (Wave 4 P10 cross-audit, 2026-05-09): the bare CLI only
/// emits files that appear in `wiring_map` — typically ~787 of 1288 source
/// files. To deliver the *full* picture, we walk `crates/<*>/src/**/*.rs`
/// in `workspace_path` and merge any file that isn't already a node into
/// the graph as a bare node (no edges, but `is_test` inferred from path,
/// `is_orphan=true` since it has no recorded consumers/producers).
///
/// Disable enrichment with `?enrich=0` to retrieve the raw CLI shape.
pub async fn api_viz_workspace(
    State(state): State<AppState>,
    Query(p): Query<std::collections::HashMap<String, String>>,
) -> axum::response::Result<Response<Body>> {
    let include_orphans = !matches!(
        p.get("include_orphans").map(String::as_str),
        Some("0") | Some("false"),
    );
    let include_tests = !matches!(
        p.get("include_tests").map(String::as_str),
        Some("0") | Some("false"),
    );
    let enrich = !matches!(
        p.get("enrich").map(String::as_str),
        Some("0") | Some("false"),
    );

    let mut args: Vec<&str> = vec!["viz", "workspace", "--format", "json"];
    if include_orphans {
        args.push("--include-orphans");
    }
    if include_tests {
        args.push("--include-tests");
    }

    let output = tokio::process::Command::new("touring")
        .current_dir(&state.workspace_path)
        .args(&args)
        .output()
        .await
        .map_err(|e| AppError::TouringCommand(e.to_string()))?;
    if !output.status.success() {
        return Err(
            AppError::TouringCommand(String::from_utf8_lossy(&output.stderr).to_string()).into(),
        );
    }

    let bytes = if enrich {
        let mut graph: Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|_| serde_json::json!({"nodes": [], "edges": []}));
        let workspace_path = state.workspace_path.clone();
        let merged = tokio::task::spawn_blocking(move || {
            normalize_and_deduplicate_nodes(&mut graph, &workspace_path);
            enrich_workspace_graph(&mut graph, &workspace_path, include_tests);
            tag_existing_edges(&mut graph, &workspace_path);
            enrich_module_decls(&mut graph, &workspace_path);
            enrich_symbol_relations(&mut graph, &workspace_path);
            enrich_external_deps(&mut graph, &workspace_path);
            enrich_crate_deps(&mut graph, &workspace_path);
            // Backfill is_external on every node whose crate resolves to "external"
            // — this guarantees the outer-pearl Pauling shell catches Python scripts,
            // single-file ext:* nodes, and CLI-emitted single-segment paths.
            backfill_external_flag(&mut graph);
            // Must run AFTER all enrichments so the score sees the full edge set.
            // Powers the dependency-gravity Pauling layout (core_score) and the
            // warm-to-cool edge gradient (per-endpoint outflow/inflow signal).
            compute_core_scores(&mut graph);
            refine_node_labels(&mut graph);
            serde_json::to_vec(&graph).unwrap_or_else(|_| Vec::new())
        })
        .await
        .unwrap_or_default();
        if merged.is_empty() {
            output.stdout
        } else {
            merged
        }
    } else {
        output.stdout
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::CACHE_CONTROL,
            "no-store, no-cache, must-revalidate, max-age=0",
        )
        .body(Body::from(bytes))
        .expect("valid json response");
    Ok(response)
}

// ── workspace-graph viz enrichment helpers extracted to viz_graph.rs (F-9) ──

/// GET / — serve `index.html`.
pub async fn serve_index(State(state): State<AppState>) -> Result<Response<Body>, AppError> {
    let body = tokio::fs::read(&state.dist_path.join("index.html")).await?;
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html")
        .body(Body::from(body))
        .expect("valid response");
    Ok(response)
}

/// GET /* — serve dist files (with SPA fallback to index.html).
pub async fn serve_dist(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Result<Response<Body>, AppError> {
    let p = path.trim_start_matches('/');
    let base = if p.is_empty() || !p.contains('.') {
        state.dist_path.join("index.html")
    } else {
        state.dist_path.join(p)
    };
    let (status, mime, body) = if base.exists() {
        // Detect mime from the file actually being served, not the URL path.
        // SPA routes like `/dashboard` resolve `base` to `index.html`, so
        // checking `p.ends_with(...)` would mis-classify them as octet-stream.
        let base_name = base.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let mime = if base_name.ends_with(".wasm") {
            "application/wasm"
        } else if base_name.ends_with(".js") || base_name.ends_with(".mjs") {
            "application/javascript"
        } else if base_name.ends_with(".css") {
            "text/css"
        } else if base_name.ends_with(".html") {
            "text/html; charset=utf-8"
        } else if base_name.ends_with(".svg") {
            "image/svg+xml"
        } else if base_name.ends_with(".png") {
            "image/png"
        } else if base_name.ends_with(".ico") {
            "image/x-icon"
        } else if base_name.ends_with(".json") || base_name.ends_with(".map") {
            "application/json"
        } else {
            "application/octet-stream"
        };
        let body = tokio::fs::read(&base).await?;
        (StatusCode::OK, mime.to_string(), body)
    } else {
        let body = tokio::fs::read(&state.dist_path.join("index.html")).await?;
        (StatusCode::OK, "text/html; charset=utf-8".to_string(), body)
    };
    let response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(body))
        .expect("valid response");
    Ok(response)
}

/// Wire all dashboard routes (REST + SSE + WebSocket) into a single
/// [`Router`] with the supplied state values. The two halves use distinct
/// state types so axum requires a sub-router merge.
pub fn build_app(app_state: AppState, ws_state: WsState) -> Router {
    let ws_router: Router = Router::new()
        .route("/ws/quality", get(ws_quality))
        .with_state(ws_state);

    Router::new()
        .route("/api/health", get(api_health))
        .route("/api/status", get(api_status))
        .route("/api/orphans", get(api_orphans))
        .route("/api/search", get(api_search))
        .route("/api/wiring/modules", get(api_wiring_modules))
        .route("/api/memory", get(api_memory))
        .route("/api/memory/stats", get(api_memory_stats))
        .route("/api/gate-metrics", get(api_gate_metrics))
        .route("/api/sessions", get(api_sessions))
        .route("/api/decompose", get(api_decompose))
        .route("/api/decompose/templates", get(api_decompose_templates))
        .route("/api/decompose/ready", get(api_decompose_ready))
        .route("/api/cognitive", get(api_cognitive))
        .route("/api/cognitive/engines", get(api_cognitive_engines))
        .route("/api/wiring/chains", get(api_wiring_chains))
        .route("/api/viz/wiring/svg", get(api_viz_svg))
        .route("/api/viz/workspace", get(api_viz_workspace))
        .route("/api/quality/signal", get(api_quality_signal))
        .route(
            "/api/quality/rules/evaluate",
            get(api_quality_rules_evaluate),
        )
        .route("/api/quality/diff", get(api_quality_diff))
        .route("/api/quality/federation", get(api_quality_federation))
        .route("/api/quality/history", get(api_quality_history))
        .route("/api/events", get(api_events))
        // Elite W2-W4 endpoints (SPEC 2026-06-12 §7.2)
        .route("/api/learning/status", get(api_learning_status))
        .route("/api/memory/recall", get(api_memory_recall))
        .route("/api/wiring/impact", get(api_wiring_impact))
        .route("/api/wiring/suggest", get(api_wiring_suggest))
        .route("/api/decompose/task", get(api_decompose_task))
        .route("/api/sessions/{id}", get(api_session_detail))
        .route(
            "/api/quality/rules",
            get(api_quality_rules_get).put(api_quality_rules_put),
        )
        // Elite W4 — new surfaces (SPEC §6)
        .route("/api/mcp/tools", get(api_mcp_tools))
        .route("/api/mcp/call", axum::routing::post(api_mcp_call))
        .route("/api/jobs", get(api_jobs))
        .route("/api/speculate", get(api_speculate))
        .route("/", get(serve_index))
        .route("/{*path}", get(serve_dist))
        .with_state(app_state)
        .merge(ws_router)
        .layer(
            // SEC-02: restrict cross-origin access to localhost only. The
            // dashboard is served same-origin, so permissive `Any` only enabled
            // any website's JS to read this API cross-origin. Localhost on any
            // port stays allowed for local dev tooling.
            CorsLayer::new()
                .allow_origin(AllowOrigin::predicate(|origin, _parts| {
                    is_localhost_origin(origin.as_bytes())
                }))
                .allow_methods(Any)
                .allow_headers(Any),
        )
        // SEC-05: defense-in-depth response headers. `X-Frame-Options: DENY`
        // blocks clickjacking (a malicious external page iframing the loopback
        // dashboard); `X-Content-Type-Options: nosniff` stops MIME-sniffing. A
        // strict CSP is intentionally omitted — the dashboard UI relies on inline
        // assets and would break under it.
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(TraceLayer::new_for_http())
}

/// SEC-02: returns `true` only for loopback HTTP origins (`http://localhost`,
/// `http://127.0.0.1`, `http://[::1]`) on any port. After the host the next byte
/// must be `:` (port) or end-of-string, so `http://localhost.evil.com` is
/// rejected (prefix-injection guard). Used by the CORS layer.
fn is_localhost_origin(origin: &[u8]) -> bool {
    let ok = |host: &[u8]| {
        origin
            .strip_prefix(host)
            .is_some_and(|rest| rest.is_empty() || rest[0] == b':')
    };
    ok(b"http://localhost") || ok(b"http://127.0.0.1") || ok(b"http://[::1]")
}

/// SEC-02: resolve the web bind address — loopback `127.0.0.1:3000` by default,
/// overridable via `TOURING_WEB_BIND`. An unparseable override falls back to
/// loopback (never silently to a wildcard).
fn resolve_web_bind(raw: Option<String>) -> SocketAddr {
    const DEFAULT: SocketAddr =
        SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 3000);
    match raw {
        None => DEFAULT,
        Some(s) => s.parse().unwrap_or_else(|e| {
            tracing::warn!("invalid TOURING_WEB_BIND={s:?} ({e}); using {DEFAULT}");
            DEFAULT
        }),
    }
}

/// Convenience entrypoint for the binary: bind `127.0.0.1:3000` (loopback by
/// default; override with `TOURING_WEB_BIND`) and serve.
/// Honours `TOURING_DASHBOARD_SNAPSHOTS` for the persistent JSONL ring path.
pub async fn run() {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let crate_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crates_dir = crate_dir
        .parent()
        .expect("CARGO_MANIFEST_DIR always has a parent (the crates/ directory)");
    let workspace_root = crates_dir
        .parent()
        .expect("crates/ directory always has a parent (the workspace root)");
    let dist_path = crates_dir.join("touring-web/dist");
    // The dashboard's purpose is WORKSPACE intelligence: rooting the touring
    // CLI at the sub-crate made /api/health + /api/status report the
    // touring-web index (2,148 symbols) instead of the workspace (71,436) —
    // cross-audit 2026-06-11, F-01/BUG-2. Override via TOURING_WEB_PROJECT.
    let project_path = std::env::var_os("TOURING_WEB_PROJECT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| workspace_root.to_path_buf());
    let workspace_path = workspace_root.to_path_buf();

    tracing::info!("Serving dist from: {:?}", dist_path);
    tracing::info!("Touring project context: {:?}", project_path);
    tracing::info!("Workspace root: {:?}", workspace_path);

    let snapshots_path = std::env::var("TOURING_DASHBOARD_SNAPSHOTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs_home().join(".claude/touring/dashboard-snapshots.jsonl"));
    let snapshot_store = Arc::new(SnapshotStore::new(snapshots_path.clone(), DEFAULT_CAPACITY));
    tracing::info!(
        "Snapshot store: {:?} (loaded {} entries)",
        snapshots_path,
        snapshot_store.len(),
    );

    let app_state = AppState {
        dist_path: dist_path.clone(),
        project_path: project_path.clone(),
        workspace_path,
        snapshot_store: snapshot_store.clone(),
    };
    let ws_state = WsState {
        project_path,
        store: snapshot_store,
    };

    let app = build_app(app_state, ws_state);

    // SEC-02: bind loopback by default — the dashboard has no authentication, so
    // it must NOT be reachable from the LAN unless the operator explicitly opts
    // in via TOURING_WEB_BIND (e.g. "0.0.0.0:3000"), which fires a loud warning.
    let addr = resolve_web_bind(std::env::var("TOURING_WEB_BIND").ok());
    if !addr.ip().is_loopback() {
        tracing::warn!(
            %addr,
            "⚠ touring-web bound to a non-loopback address — the dashboard has NO \
             authentication; only expose it on a trusted network"
        );
    }
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Could not bind to address");

    tracing::info!("touring-web-server listening on http://{}", addr);
    axum::serve(listener, app).await.expect("Server error");
}

/// Resolve the user's home directory without bringing in a new dep.
/// Falls back to `/tmp` so the binary still runs in stripped containers.
pub fn dirs_home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
