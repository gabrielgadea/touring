//! Event types for the append-only activity event store.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use uuid::Uuid;

pub use std::time::{SystemTime, UNIX_EPOCH};

/// Supported event actions in the activity store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventAction {
    /// A TACO task moved to in-progress.
    TaskStarted,
    /// A TACO task completed (status = completed/partial/failed).
    TaskCompleted,
    /// A Touring tool was invoked by an agent.
    ToolInvoked,
    /// A Claude Code hook lifecycle event fired.
    HookFired,
    /// A new Claude Code session started.
    SessionStarted,
    /// A Claude Code session ended.
    SessionEnded,
    /// A LinUCB reward update or learning bandit arm pull.
    LearningSignal,
    /// A memory tier entry was created (see [`crate::MemoryTier`]).
    MemoryStored,
    /// A `TouringError` was raised and observed.
    ErrorOccurred,
    /// A new pub symbol was wired to a consumer (REGRA #0 telemetry).
    WireIntegrated,
    /// The Touring index was rebuilt.
    IndexRebuilt,
    /// Daemon health snapshot (periodic, low frequency).
    DaemonHealth,
    /// VGP Layer 5 path boundary violation (S4/D4.6)
    BoundaryViolation,
}

impl fmt::Display for EventAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventAction::TaskStarted => write!(f, "task_started"),
            EventAction::TaskCompleted => write!(f, "task_completed"),
            EventAction::ToolInvoked => write!(f, "tool_invoked"),
            EventAction::HookFired => write!(f, "hook_fired"),
            EventAction::SessionStarted => write!(f, "session_started"),
            EventAction::SessionEnded => write!(f, "session_ended"),
            EventAction::LearningSignal => write!(f, "learning_signal"),
            EventAction::MemoryStored => write!(f, "memory_stored"),
            EventAction::ErrorOccurred => write!(f, "error_occurred"),
            EventAction::WireIntegrated => write!(f, "wire_integrated"),
            EventAction::IndexRebuilt => write!(f, "index_rebuilt"),
            EventAction::DaemonHealth => write!(f, "daemon_health"),
            EventAction::BoundaryViolation => write!(f, "boundary_violation"),
        }
    }
}

/// Actor that performed the action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "id")]
pub enum Actor {
    /// A TACO subagent — `id` is the canonical role name
    /// (`touring-scouter`, `touring-engineer`, etc.).
    Agent(String),
    /// An in-process system component (e.g. a CLI subcommand, a
    /// hook handler). `id` is the component name.
    System(String),
    /// The Touring daemon — `id` is the project name from the
    /// per-project routing table.
    Daemon(String),
}

impl Actor {
    /// Return the inner id as `&str`, regardless of variant.
    pub fn display(&self) -> &str {
        match self {
            Actor::Agent(id) => id,
            Actor::System(id) => id,
            Actor::Daemon(id) => id,
        }
    }
}

/// Unique event identifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EventId(String);

impl EventId {
    /// Create a new, globally-unique event id by combining a
    /// nanosecond timestamp with a v4 UUID and hashing the pair
    /// with SHA-256. Truncated to 8 bytes (16 hex chars) for
    /// human-readable storage.
    pub fn new() -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before UNIX_EPOCH")
            .as_nanos();
        let uid = Uuid::new_v4().to_string();
        let combined = format!("{}-{}", ts, uid);
        let mut hasher = Sha256::new();
        hasher.update(combined.as_bytes());
        let hash = hex::encode(&hasher.finalize()[..8]);
        Self(format!("{}-{}", ts, hash))
    }

    /// Best-effort parse — accepts any string that contains a dash
    /// and is at least 20 characters long. Returns `None` for
    /// shorter strings.
    pub fn parse(s: &str) -> Option<Self> {
        if s.contains('-') && s.len() >= 20 {
            Some(Self(s.to_string()))
        } else {
            None
        }
    }

    /// Borrow the underlying id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Core event record for the activity store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Globally-unique event id (see [`EventId`]).
    pub id: EventId,
    /// Monotonic sequence number assigned at insert time. Unique
    /// per actor within a session.
    pub seq: u64,
    /// Action that produced the event.
    pub action: EventAction,
    /// Actor that produced the event.
    pub actor: Actor,
    /// Nanosecond-resolution timestamp from the producer's clock
    /// (typically `SystemTime::now()`).
    pub timestamp_ns: u64,
    /// Optional free-form payload, serialised as JSON. Omitted
    /// from the wire form when `None`.
    pub payload: Option<serde_json::Value>,
    /// SHA-256 hash of all other fields, used by
    /// [`Event::verify_projection`] to detect tampering or
    /// transport corruption.
    pub projection_hash: String,
}

impl Event {
    /// Construct a new event with a fresh id and timestamp, then
    /// compute the projection hash over the resulting record.
    pub fn new(
        seq: u64,
        action: EventAction,
        actor: Actor,
        payload: Option<serde_json::Value>,
    ) -> Self {
        let id = EventId::new();
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before UNIX_EPOCH")
            .as_nanos() as u64;
        let projection_hash =
            Self::compute_projection_hash(&id, &seq, &action, &actor, &timestamp_ns, &payload);
        Self {
            id,
            seq,
            action,
            actor,
            timestamp_ns,
            payload,
            projection_hash,
        }
    }

    /// Recompute the projection hash from the event's content.
    /// Internal — used by [`Event::verify_projection`].
    fn compute_projection_hash(
        id: &EventId,
        seq: &u64,
        action: &EventAction,
        actor: &Actor,
        timestamp_ns: &u64,
        payload: &Option<serde_json::Value>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(id.as_str().as_bytes());
        hasher.update(seq.to_le_bytes());
        hasher.update(action.to_string().as_bytes());
        hasher.update(actor.display().as_bytes());
        hasher.update(timestamp_ns.to_le_bytes());
        if let Some(p) = payload {
            hasher.update(p.to_string().as_bytes());
        }
        hex::encode(hasher.finalize())
    }

    /// Verify the event's projection hash matches its content.
    /// Returns `true` if the stored hash equals a fresh
    /// recomputation. A `false` result indicates the event was
    /// either tampered with or the record was corrupted in
    /// transport.
    pub fn verify_projection(&self) -> bool {
        self.projection_hash
            == Self::compute_projection_hash(
                &self.id,
                &self.seq,
                &self.action,
                &self.actor,
                &self.timestamp_ns,
                &self.payload,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_id_new_unique() {
        assert_ne!(EventId::new().as_str(), EventId::new().as_str());
    }

    #[test]
    fn event_id_parse_valid() {
        let id = EventId::new();
        assert!(EventId::parse(id.as_str()).is_some());
    }

    #[test]
    fn event_id_parse_invalid() {
        assert!(EventId::parse("short").is_none());
    }

    #[test]
    fn event_verify_projection() {
        let e = Event::new(
            1,
            EventAction::TaskStarted,
            Actor::Agent("test".into()),
            None,
        );
        assert!(e.verify_projection());
    }

    #[test]
    fn event_action_display() {
        assert_eq!(EventAction::TaskStarted.to_string(), "task_started");
    }

    #[test]
    fn actor_display() {
        assert_eq!(Actor::Agent("k".into()).display(), "k");
    }
}
