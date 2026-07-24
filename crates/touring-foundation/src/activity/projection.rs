//! Deterministic projection algorithm for activity events.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Projection result for a single event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventProjection {
    /// Event sequence number.
    pub seq: u64,
    /// Entity this event affects.
    pub entity: EntityKey,
    /// Action that was performed.
    pub action: String,
    /// Timestamp of projection computation.
    pub computed_at_ns: u64,
}

/// Key for grouping entities.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct EntityKey {
    /// Entity type (e.g., "agent", "session", "daemon").
    pub entity_type: String,
    /// Entity identifier.
    pub entity_id: String,
}

impl EntityKey {
    /// Construct a new [`EntityKey`] from any `Into<String>` for
    /// the two fields. Used by activity ingest paths that produce
    /// keys on the fly from raw events.
    pub fn new(entity_type: impl Into<String>, entity_id: impl Into<String>) -> Self {
        Self {
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
        }
    }
}

/// Aggregated view of an entity's activity.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntityActivity {
    /// The [`EntityKey`] this aggregation is for.
    pub entity_key: EntityKey,
    /// Total number of events observed for this entity.
    pub event_count: u64,
    /// Histogram of action → count. Useful for dashboards
    /// answering "which actions does this entity perform most?".
    pub action_counts: HashMap<String, u64>,
    /// Sequence number of the first event in the projection
    /// window (or `None` for an entity with no events yet).
    pub first_seq: Option<u64>,
    /// Sequence number of the most recent event (or `None` for
    /// an entity with no events yet).
    pub last_seq: Option<u64>,
}

impl EntityActivity {
    /// Ingest one event into this entity's activity. Updates the
    /// count, the per-action histogram, and the
    /// `first_seq`/`last_seq` min/max envelope.
    pub fn add_event(&mut self, seq: u64, action: &str) {
        self.event_count += 1;
        *self.action_counts.entry(action.to_string()).or_insert(0) += 1;
        self.first_seq = Some(self.first_seq.map(|f| f.min(seq)).unwrap_or(seq));
        self.last_seq = Some(self.last_seq.map(|l| l.max(seq)).unwrap_or(seq));
    }
}

/// Projection state for the entire store.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoreProjection {
    /// Per-entity activity map keyed by [`EntityKey`].
    pub entities: HashMap<EntityKey, EntityActivity>,
    /// Total number of events processed by this projection.
    pub total_events: u64,
    /// Nanosecond-resolution timestamp at which this projection
    /// snapshot was computed (system clock).
    pub computed_at_ns: u64,
}

impl StoreProjection {
    /// Project a batch of events into entity activity summaries.
    pub fn project_events(events: &[(u64, String, String, String)]) -> Self {
        let mut entities: HashMap<EntityKey, EntityActivity> = HashMap::new();
        let mut total = 0u64;
        let computed_at_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is before UNIX_EPOCH")
            .as_nanos() as u64;

        for (seq, entity_type, entity_id, action) in events {
            total += 1;
            let key = EntityKey::new(entity_type, entity_id);
            let entry = entities
                .entry(key.clone())
                .or_insert_with(|| EntityActivity {
                    entity_key: key,
                    event_count: 0,
                    action_counts: HashMap::new(),
                    first_seq: None,
                    last_seq: None,
                });
            entry.add_event(*seq, action);
        }

        Self {
            entities,
            total_events: total,
            computed_at_ns,
        }
    }

    /// Get activity for a specific entity.
    pub fn get_entity(&self, key: &EntityKey) -> Option<&EntityActivity> {
        self.entities.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_key_new() {
        let k = EntityKey::new("agent", "kazuba");
        assert_eq!(k.entity_type, "agent");
        assert_eq!(k.entity_id, "kazuba");
    }

    #[test]
    fn project_events_single() {
        let events = vec![(
            1,
            "agent".to_string(),
            "kazuba".to_string(),
            "task_started".to_string(),
        )];
        let proj = StoreProjection::project_events(&events);
        assert_eq!(proj.total_events, 1);
        let k = EntityKey::new("agent", "kazuba");
        let ent = proj.get_entity(&k).unwrap();
        assert_eq!(ent.event_count, 1);
        assert_eq!(*ent.action_counts.get("task_started").unwrap(), 1);
    }

    #[test]
    fn project_events_multiple_same_entity() {
        let events = vec![
            (
                1,
                "agent".to_string(),
                "kazuba".to_string(),
                "task_started".to_string(),
            ),
            (
                2,
                "agent".to_string(),
                "kazuba".to_string(),
                "task_completed".to_string(),
            ),
            (
                3,
                "agent".to_string(),
                "kazuba".to_string(),
                "task_started".to_string(),
            ),
        ];
        let proj = StoreProjection::project_events(&events);
        let k = EntityKey::new("agent", "kazuba");
        let ent = proj.get_entity(&k).unwrap();
        assert_eq!(ent.event_count, 3);
        assert_eq!(*ent.action_counts.get("task_started").unwrap(), 2);
        assert_eq!(*ent.action_counts.get("task_completed").unwrap(), 1);
    }

    #[test]
    fn empty_events_returns_empty_projection() {
        let events: Vec<(u64, String, String, String)> = vec![];
        let proj = StoreProjection::project_events(&events);
        assert_eq!(proj.total_events, 0);
        assert!(proj.entities.is_empty());
    }
}
