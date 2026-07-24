//! Property tests for `touring_activity` — monotonic event_seq and projection invariants.

use proptest::prelude::*;
use tempfile::tempdir;
use touring_foundation::activity::event::{Actor, EventAction};
use touring_foundation::activity::store::EventStore;

proptest! {
    /// Property: event.seq is strictly monotonically increasing across many appends.
    #[test]
    fn proptest_monotonic_seq_append(count in 1..500i32) {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("events.jsonl");
        let store = EventStore::open(path.clone()).expect("EventStore::open");

        for i in 0..count {
            let event = store
                .append(EventAction::ToolInvoked, Actor::Daemon("proptest".into()), None)
                .expect("append");
            let expected_seq = (i + 1) as u64;
            prop_assert_eq!(event.seq, expected_seq, "seq at append {} should be {}", i, expected_seq);
        }

        drop(store);
        let store2 = EventStore::open(path).expect("EventStore::open");
        let replayed = store2.replay().expect("replay");
        prop_assert_eq!(replayed.len(), count as usize);

        for (i, event) in replayed.into_iter().enumerate() {
            let expected_seq_replay = (i + 1) as u64;
            prop_assert_eq!(event.seq, expected_seq_replay,
                "replayed event seq should be {} but was {}", expected_seq_replay, event.seq);
        }
    }
}

proptest! {
    /// Property: parallel appends to different stores produce independent seq streams.
    #[test]
    fn proptest_independent_seq_streams(count in 2..20i32) {
        let dir = tempdir().expect("tempdir");

        let mut store_infos: Vec<(String, usize)> = Vec::new();

        for i in 0..count {
            let path = dir.path().join(format!("stream_{}.jsonl", i));
            let store = EventStore::open(path).expect("EventStore::open");
            let append_count = 10 + i as usize;
            store_infos.push((format!("stream_{}.jsonl", i), append_count));

            let s = store;
            for j in 0..append_count {
                s.append(EventAction::TaskStarted, Actor::Agent(format!("agent_{}", j)), None)
                    .expect("append");
            }
        }

        for (idx, (_, append_count)) in store_infos.iter().enumerate() {
            let path = dir.path().join(format!("stream_{}.jsonl", idx));
            let store2 = EventStore::open(path).expect("reopen");
            let replayed = store2.replay().expect("replay");
            prop_assert_eq!(replayed.len(), *append_count);
            for (i, event) in replayed.iter().enumerate() {
                prop_assert_eq!(event.seq, (i + 1) as u64);
            }
        }
    }
}

proptest! {
    /// Property: empty store has no events, and first event always gets seq = 1.
    #[test]
    fn proptest_empty_then_first_seq(tmp in "[a-z0-9]{1,30}") {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(format!("{}.jsonl", tmp));

        let store = EventStore::open(path.clone()).expect("EventStore::open");
        let events = store.replay().expect("replay");
        prop_assert!(events.is_empty(), "new store should have zero events");

        drop(store);

        let store2 = EventStore::open(path).expect("EventStore::open");
        let first = store2
            .append(EventAction::HookFired, Actor::Agent("first".into()), None)
            .expect("first append");
        prop_assert_eq!(first.seq, 1, "first event must have seq=1");
    }
}
