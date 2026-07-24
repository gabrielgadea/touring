//! WebSocket realtime channel for the dashboard (Wave 4 P10.8).
//!
//! Bidirectional upgrade of `/ws/quality`:
//! - Server → client: pushes the latest snapshot every `interval` (default 6s).
//! - Client → server: optional `{"action":"refresh"}` text frame triggers an
//!   immediate snapshot push (bypassing the interval).
//!
//! Push payload is the same JSON returned by `touring quality-signal -j`
//! enriched with `{"_history_len": N}` so the client can size sparkline
//! buffers without a separate fetch.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{
    State,
    ws::{Message, WebSocket, WebSocketUpgrade},
};
use axum::response::IntoResponse;
use serde_json::{Value, json};

use crate::web::server::snapshots::SnapshotStore;

/// Shared state plumbed through axum::extract::State for the WS route.
#[derive(Clone)]
pub struct WsState {
    /// Filesystem path of the project this WS route serves quality data for.
    pub project_path: std::path::PathBuf,
    /// Shared snapshot store the WS handler reads quality snapshots from.
    pub store: Arc<SnapshotStore>,
}

/// Default push cadence. Six seconds keeps the dashboard fluid without
/// hammering the touring CLI.
pub const DEFAULT_PUSH_INTERVAL: Duration = Duration::from_secs(6);

impl WsState {
    /// Deterministic test constructor — mirrors [`super::AppState::new_for_test`]:
    /// the snapshot file lives inside `tmp_dir` and CLI calls run from
    /// `tmp_dir`. To pair with an `AppState` so both router halves observe
    /// the same ring buffer, clone its `snapshot_store` Arc into `store`.
    /// (The module doc advertised this constructor since P10.8 but it was
    /// never implemented — cross-audit 2026-06-11, F1.)
    pub fn new_for_test(tmp_dir: std::path::PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&tmp_dir);
        let store = Arc::new(SnapshotStore::new(
            tmp_dir.join("snapshots.jsonl"),
            crate::web::server::snapshots::DEFAULT_CAPACITY,
        ));
        Self {
            project_path: tmp_dir,
            store,
        }
    }
}

/// GET /ws/quality — Axum upgrade handler.
pub async fn ws_quality(
    upgrade: WebSocketUpgrade,
    State(state): State<WsState>,
) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| handle_quality_socket(socket, state))
}

/// Per-connection driver loop.
async fn handle_quality_socket(mut socket: WebSocket, state: WsState) {
    let mut ticker = tokio::time::interval(DEFAULT_PUSH_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if push_snapshot(&mut socket, &state).await.is_err() { break; }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(t)))
                        if t.contains("\"refresh\"")
                            && push_snapshot(&mut socket, &state).await.is_err() => { break; }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

/// Collect a fresh snapshot (synchronous CLI call wrapped in spawn_blocking)
/// and push it to the client. On any IO failure returns `Err(())`.
async fn push_snapshot(socket: &mut WebSocket, state: &WsState) -> Result<(), ()> {
    let project = state.project_path.clone();
    let store = state.store.clone();

    let snap: Value = tokio::task::spawn_blocking(move || {
        std::process::Command::new("touring")
            .current_dir(&project)
            .args(["quality-signal", "-j", "--no-diagnostics"])
            .output()
            .ok()
            .and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok())
            .unwrap_or_else(|| json!({"error": "snapshot unavailable"}))
    })
    .await
    .unwrap_or_else(|_| json!({"error": "join failed"}));

    store.append(snap.clone());

    let mut payload = snap;
    if let Value::Object(ref mut m) = payload {
        m.insert("_history_len".into(), json!(store.len()));
    }
    let text = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_for_test_builds_isolated_state_with_working_store() {
        let tmp =
            std::env::temp_dir().join(format!("touring-ws-state-test-{}", std::process::id()));
        let ws = WsState::new_for_test(tmp.clone());
        assert_eq!(ws.project_path, tmp);
        ws.store.append(json!({"probe": 1}));
        let recent = ws.store.recent(1);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0]["probe"], 1);
        let _ = std::fs::remove_dir_all(tmp);
    }
}
