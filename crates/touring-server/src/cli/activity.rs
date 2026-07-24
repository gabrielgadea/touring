//! `touring activity` — append-only event log with deterministic projection.
//!
//! Subcommands:
//!
//! - `touring activity append <action> [--actor <actor>] [--payload <json>]` —
//!   append a new event to the activity log.
//!
//! - `touring activity replay [--limit <n>]` — replay all events and produce
//!   the canonical projection.
//!
//! - `touring activity verify` — verify the stored hash against recomputed projection.
//!
//! - `touring activity projection` — show the current projected state hash.
//!
//! - `touring activity status` — show event count, last seq, store path.
//!
//! All subcommands operate on `<project>/.claude/touring/activity.jsonl`.
//! The store is append-only; replay is deterministic and produces a SHA-256 hash.

use std::path::PathBuf;

use super::common::flag_value;
use anyhow::{Result, bail};
use sha2::Digest;

const ACTIVITY_DIR: &str = ".claude/touring";
const ACTIVITY_FILE: &str = "activity.jsonl";

/// Run the `activity` CLI subcommand.
pub fn run(args: &[String]) -> Result<()> {
    let sub = args.get(2).map(String::as_str).unwrap_or("status");
    match sub {
        "append" => activity_append(args),
        "replay" => activity_replay(args),
        "verify" => activity_verify(args),
        "projection" => activity_projection(args),
        "status" => activity_status(args),
        other => {
            bail!(
                "unknown activity subcommand `{other}` — expected \
                 `append`, `replay`, `verify`, `projection`, or `status`"
            )
        }
    }
}

fn parse_action(s: &str) -> Result<touring_foundation::activity::event::EventAction> {
    match s {
        "task_started" => Ok(touring_foundation::activity::event::EventAction::TaskStarted),
        "task_completed" => Ok(touring_foundation::activity::event::EventAction::TaskCompleted),
        "tool_invoked" => Ok(touring_foundation::activity::event::EventAction::ToolInvoked),
        "hook_fired" => Ok(touring_foundation::activity::event::EventAction::HookFired),
        "session_started" => Ok(touring_foundation::activity::event::EventAction::SessionStarted),
        "session_ended" => Ok(touring_foundation::activity::event::EventAction::SessionEnded),
        "learning_signal" => Ok(touring_foundation::activity::event::EventAction::LearningSignal),
        "memory_stored" => Ok(touring_foundation::activity::event::EventAction::MemoryStored),
        "error_occurred" => Ok(touring_foundation::activity::event::EventAction::ErrorOccurred),
        "wire_integrated" => Ok(touring_foundation::activity::event::EventAction::WireIntegrated),
        "index_rebuilt" => Ok(touring_foundation::activity::event::EventAction::IndexRebuilt),
        "daemon_health" => Ok(touring_foundation::activity::event::EventAction::DaemonHealth),
        other => anyhow::bail!("unknown action: {other}"),
    }
}

// ── activity append ───────────────────────────────────────────────────────────

fn activity_append(args: &[String]) -> Result<()> {
    let action = args
        .get(3)
        .ok_or_else(|| anyhow::anyhow!("action required (e.g. task_started, vgp_verify)"))?;
    let actor = flag_value(args, "--actor").unwrap_or("Orchestrator");
    let payload_str = flag_value(args, "--payload").unwrap_or("{}");
    let payload: serde_json::Value = serde_json::from_str(payload_str)
        .map_err(|e| anyhow::anyhow!("invalid JSON payload: {e}"))?;

    let store = open_store()?;
    store
        .append(
            parse_action(action)?,
            touring_foundation::activity::event::Actor::Agent(actor.to_string()),
            Some(payload),
        )
        .map_err(|e| anyhow::anyhow!("append failed: {e}"))?;

    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "subcommand": "append",
            "action": action
        })
    );
    Ok(())
}

// ── activity replay ───────────────────────────────────────────────────────────

fn activity_replay(args: &[String]) -> Result<()> {
    let limit = flag_value(args, "--limit").and_then(|s| s.parse::<usize>().ok());

    let store = open_store()?;
    let projection = store
        .replay()
        .map_err(|e| anyhow::anyhow!("replay failed: {e}"))?;

    let total = projection.len();
    let displayed = match limit {
        Some(n) => projection.iter().rev().take(n).cloned().collect::<Vec<_>>(),
        None => projection.clone(),
    };

    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "subcommand": "replay",
            "total_events": total,
            "projection_hash": compute_store_hash(&store)?,
            "displayed_events": displayed.len()
        })
    );
    Ok(())
}

// ── activity verify ───────────────────────────────────────────────────────────

fn activity_verify(_args: &[String]) -> Result<()> {
    let store = open_store()?;
    let results = store
        .verify()
        .map_err(|e| anyhow::anyhow!("verify failed: {e}"))?;

    let total = results.len();
    let failures: Vec<_> = results.iter().filter(|(_, r)| r.is_err()).collect();

    if failures.is_empty() {
        let hash = compute_store_hash(&store)?;
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "subcommand": "verify",
                "total_events": total,
                "failures": 0,
                "hash": hash
            })
        );
    } else {
        let failed_seqs: Vec<u64> = failures.iter().map(|(s, _)| *s).collect();
        println!(
            "{}",
            serde_json::json!({
                "status": "corrupted",
                "subcommand": "verify",
                "total_events": total,
                "failures": failures.len(),
                "failed_seqs": failed_seqs
            })
        );
    }
    Ok(())
}

fn compute_store_hash(store: &touring_foundation::activity::store::EventStore) -> Result<String> {
    use touring_foundation::activity::verify::Verifier;
    let events = store
        .replay()
        .map_err(|e| anyhow::anyhow!("replay failed: {e}"))?;
    if events.is_empty() {
        return Ok("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string());
    }
    // Verify each event's projection before composing the rollup hash so we
    // surface integrity violations early — REGRA #0: wire Verifier into the
    // CLI hash path instead of leaving the import unused.
    let verifier = Verifier::new();
    verifier
        .verify_batch(&events)
        .map_err(|e| anyhow::anyhow!("activity store integrity check failed: {e}"))?;
    let mut hasher = sha2::Sha256::new();
    for event in &events {
        hasher.update(event.id.as_str().as_bytes());
        hasher.update(event.seq.to_le_bytes());
    }
    Ok(hex::encode(hasher.finalize()))
}

// ── activity projection ─────────────────────────────────────────────────────

fn activity_projection(_args: &[String]) -> Result<()> {
    let store = open_store()?;
    let projection = store
        .replay()
        .map_err(|e| anyhow::anyhow!("replay failed: {e}"))?;
    let hash = compute_store_hash(&store)?;

    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "subcommand": "projection",
            "event_count": projection.len(),
            "hash": hash
        })
    );
    Ok(())
}

// ── activity status ──────────────────────────────────────────────────────────

fn activity_status(_args: &[String]) -> Result<()> {
    let store = open_store()?;
    let events = store
        .replay()
        .map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let last_seq = events.last().map(|e| e.seq).unwrap_or(0);
    let path = store_path()?;

    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "subcommand": "status",
            "event_count": events.len(),
            "last_seq": last_seq,
            "path": path
        })
    );
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn open_store() -> Result<touring_foundation::activity::store::EventStore> {
    let path = store_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    touring_foundation::activity::store::EventStore::open(path)
        .map_err(|e| anyhow::anyhow!("cannot open event store: {e}"))
}

fn store_path() -> Result<PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|_| anyhow::anyhow!("cannot determine current directory"))?;
    Ok(cwd.join(ACTIVITY_DIR).join(ACTIVITY_FILE))
}
