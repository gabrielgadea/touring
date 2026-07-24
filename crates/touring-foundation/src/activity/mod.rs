//! touring_activity — Append-only activity.jsonl event store with SHA-256 projection gate for Touring. Event-sourced agent state per ESAA pattern..

pub mod event;
pub mod projection;
pub mod store;
pub mod verify;
