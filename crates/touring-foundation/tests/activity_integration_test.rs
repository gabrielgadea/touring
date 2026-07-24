//! Integration tests for `touring_activity`.

use tempfile::tempdir;
use touring_foundation::activity::event::{Actor, Event, EventAction, EventId};
use touring_foundation::activity::store::EventStore;

#[test]
fn integration_event_id_new_and_parse() {
    let id = EventId::new();
    assert!(EventId::parse(id.as_str()).is_some());
}

#[test]
fn integration_event_verify_projection() {
    let event = Event::new(
        1,
        EventAction::TaskStarted,
        Actor::Agent("test".into()),
        None,
    );
    assert!(event.verify_projection());
}

#[test]
fn integration_store_append_and_replay() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("events.jsonl");
    let store = EventStore::open(path.clone()).expect("EventStore::open");

    let event = store
        .append(EventAction::TaskStarted, Actor::Agent("test".into()), None)
        .expect("append");
    assert_eq!(event.seq, 1);

    drop(store);
    let store2 = EventStore::open(path).expect("EventStore::open");
    let replayed = store2.replay().expect("replay");
    assert_eq!(replayed.len(), 1);
}

#[test]
fn integration_store_monotonic_seq() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("events.jsonl");
    let store = EventStore::open(path).expect("EventStore::open");

    for i in 0..5u64 {
        let event = store
            .append(EventAction::ToolInvoked, Actor::Daemon("d".into()), None)
            .expect("append");
        assert_eq!(event.seq, i + 1);
    }
}

#[test]
fn integration_store_empty_replay() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("empty.jsonl");
    let store = EventStore::open(path).expect("EventStore::open");
    let events = store.replay().expect("replay");
    assert!(events.is_empty());
}
