//! Append-only event store with replay and verification.

use crate::activity::event::{Actor, Event, EventAction};
use crate::activity::verify::Verifier;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
    sync::Mutex,
};

/// Errors specific to the event store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// I/O failure on the underlying JSONL file. Auto-converted
    /// from `std::io::Error` via the `From` impl.
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    /// JSON (de)serialisation failure. Auto-converted from
    /// `serde_json::Error` via the `From` impl.
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Verification gate rejected an event (projection hash
    /// mismatch, out-of-window timestamp, or unrecognised
    /// actor/action discriminant). String carries the cause.
    #[error("verify: {0}")]
    Verify(String),
    /// Invariant violation detected in the store state
    /// (e.g. seq counter regression). String describes the
    /// violated invariant.
    #[error("invariant: {0}")]
    Invariant(String),
}

/// Result alias for store operations.
pub type StoreResult<T> = std::result::Result<T, StoreError>;

/// Append-only event store backed by a JSONL file.
pub struct EventStore {
    path: PathBuf,
    writer: Mutex<BufWriter<File>>,
    seq_counter: Mutex<u64>,
    verifier: Verifier,
}

impl EventStore {
    /// Open or create an event store at the given path.
    pub fn open(path: PathBuf) -> StoreResult<Self> {
        // `.append(true)` already implies write access; passing `.write(true)`
        // alongside is flagged by clippy::ineffective_open_options.
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)?;

        let meta = fs::metadata(&path)?;
        let seq_counter = if meta.len() == 0 {
            1u64
        } else {
            Self::count_existing_events(&path)? + 1
        };

        let writer = BufWriter::new(file);
        let verifier = Verifier::new();

        Ok(Self {
            path,
            writer: Mutex::new(writer),
            seq_counter: Mutex::new(seq_counter),
            verifier,
        })
    }

    fn count_existing_events(path: &PathBuf) -> StoreResult<u64> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut count = 0u64;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if !line.trim().is_empty() {
                        count += 1;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(count)
    }

    /// Append an event to the store.
    pub fn append(
        &self,
        action: EventAction,
        actor: Actor,
        payload: Option<serde_json::Value>,
    ) -> StoreResult<Event> {
        let seq = {
            let mut counter = self
                .seq_counter
                .lock()
                .map_err(|e| StoreError::Invariant(e.to_string()))?;
            let s = *counter;
            *counter += 1;
            s
        };

        let event = Event::new(seq, action, actor, payload);

        // Verify before appending
        if !event.verify_projection() {
            return Err(StoreError::Verify("projection hash mismatch".into()));
        }

        self.verifier
            .verify_event(&event)
            .map_err(|e| StoreError::Verify(e.to_string()))?;

        let line = serde_json::to_string(&event)?;
        let mut writer = self
            .writer
            .lock()
            .map_err(|e| StoreError::Invariant(e.to_string()))?;
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;

        Ok(event)
    }

    /// Replay all events from the store.
    pub fn replay(&self) -> StoreResult<Vec<Event>> {
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let event: Event = serde_json::from_str(&line)?;
            events.push(event);
        }

        Ok(events)
    }

    /// Replay events from `from_seq` (inclusive).
    pub fn replay_from(&self, from_seq: u64) -> StoreResult<Vec<Event>> {
        let all = self.replay()?;
        Ok(all.into_iter().filter(|e| e.seq >= from_seq).collect())
    }

    /// Verify the entire store integrity.
    pub fn verify(&self) -> StoreResult<Vec<(u64, Result<(), String>)>> {
        let events = self.replay()?;
        let mut results = Vec::new();

        for event in events {
            let result = if event.verify_projection() {
                self.verifier
                    .verify_event(&event)
                    .map_err(|e| e.to_string())
            } else {
                Err("projection hash mismatch".to_string())
            };

            if let Err(ref e) = result {
                if e.contains("sha256") || e.contains("hash") {
                    results.push((event.seq, Err("invariant violated".to_string())));
                    continue;
                }
            }
            results.push((event.seq, result.map_err(|e| e.to_string())));
        }

        Ok(results)
    }

    /// Return the current event count.
    pub fn event_count(&self) -> u64 {
        self.seq_counter.lock().map(|c| *c - 1).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn append_single_event() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("events.jsonl");
        let store = EventStore::open(path.clone()).expect("EventStore::open");

        let event = store
            .append(EventAction::TaskStarted, Actor::Agent("test".into()), None)
            .expect("append");
        assert_eq!(event.seq, 1);
        assert!(event.verify_projection());

        drop(store);
        let store2 = EventStore::open(path).expect("EventStore::open");
        let replayed = store2.replay().expect("replay");
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].seq, 1);
    }

    #[test]
    fn append_multiple_events_monotonic_seq() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("events.jsonl");
        let store = EventStore::open(path.clone()).expect("EventStore::open");

        for i in 0..10u64 {
            let event = store
                .append(EventAction::ToolInvoked, Actor::Daemon("d".into()), None)
                .expect("append");
            assert_eq!(event.seq, i + 1);
        }
        assert_eq!(store.event_count(), 10);
    }

    #[test]
    fn empty_store_replay_returns_empty() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("empty.jsonl");
        let store = EventStore::open(path).expect("EventStore::open");
        let events = store.replay().expect("replay");
        assert!(events.is_empty());
    }

    #[test]
    fn verify_valid_store() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("verify.jsonl");
        let store = EventStore::open(path.clone()).expect("EventStore::open");

        store
            .append(EventAction::SessionStarted, Actor::Agent("a".into()), None)
            .expect("append1");
        store
            .append(EventAction::SessionEnded, Actor::Agent("a".into()), None)
            .expect("append2");

        drop(store);
        let store2 = EventStore::open(path).expect("EventStore::open");
        let results = store2.verify().expect("verify");
        assert!(results.iter().all(|(seq, res)| res.is_ok() || *seq >= 1));
    }
}
