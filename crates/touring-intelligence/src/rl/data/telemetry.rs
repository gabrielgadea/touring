//! Telemetry data loader — reads `~/.claude/telemetry/1p_failed_events.*.json` files.
//!
//! Each file contains multiple JSON lines (one event per line) with structure:
//! `{"event_type": "ClaudeCodeInternalEvent", "event_data": {...}}`

pub use super::errors::TelemetryError;

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Result type for telemetry operations.
pub type Result<T> = std::result::Result<T, TelemetryError>;

// ── Data structures ───────────────────────────────────────────────────────────

/// Raw telemetry event wrapper from telemetry files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    /// Discriminator string (e.g. `ClaudeCodeInternalEvent`).
    #[serde(rename = "event_type")]
    pub event_type: String,
    /// Decoded inner payload of the event.
    #[serde(rename = "event_data")]
    pub event_data: TelemetryEventData,
}

/// Inner event data fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEventData {
    #[serde(rename = "event_name")]
    pub event_name: String,
    #[serde(rename = "client_timestamp")]
    pub client_timestamp: f64,
    pub model: Option<String>,
    #[serde(rename = "session_id")]
    pub session_id: String,
    #[serde(rename = "user_type")]
    pub user_type: Option<String>,
    pub betas: Option<Vec<String>>,
    pub env: Option<serde_json::Value>,
    pub entrypoint: Option<String>,
    #[serde(rename = "is_interactive")]
    pub is_interactive: Option<bool>,
    #[serde(rename = "client_type")]
    pub client_type: Option<String>,
    pub process: Option<String>,
    #[serde(rename = "additional_metadata")]
    pub additional_metadata: Option<serde_json::Value>,
    #[serde(rename = "event_id")]
    pub event_id: String,
    #[serde(rename = "device_id")]
    pub device_id: Option<String>,
}

impl Default for TelemetryEventData {
    fn default() -> Self {
        Self {
            event_name: String::new(),
            client_timestamp: 0.0,
            model: None,
            session_id: String::new(),
            user_type: None,
            betas: None,
            env: None,
            entrypoint: None,
            is_interactive: None,
            client_type: None,
            process: None,
            additional_metadata: None,
            event_id: String::new(),
            device_id: None,
        }
    }
}

impl TelemetryEvent {
    /// Returns true if this is a ClaudeCodeInternalEvent.
    pub fn is_internal(&self) -> bool {
        self.event_type == "ClaudeCodeInternalEvent"
    }
}

/// Aggregated telemetry statistics for a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetrySessionStats {
    /// Identifier of the session being summarized.
    pub session_id: String,
    /// Total number of telemetry events in the session.
    pub event_count: usize,
    /// Timestamp of the first event in the session.
    pub first_event: f64,
    /// Timestamp of the last event in the session.
    pub last_event: f64,
    /// Per-event-name occurrence counts.
    pub event_names: HashMap<String, usize>,
    /// Per-model occurrence counts.
    pub models: HashMap<String, usize>,
}

impl TelemetrySessionStats {
    /// Duration of the session in seconds.
    pub fn duration_secs(&self) -> f64 {
        self.last_event - self.first_event
    }
}

/// Summary statistics for the entire telemetry dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySummary {
    /// Total telemetry events loaded across all files.
    pub total_events: usize,
    /// Number of distinct sessions observed.
    pub total_sessions: usize,
    /// Number of telemetry files successfully loaded.
    pub total_files: usize,
    /// Count of events of type `ClaudeCodeInternalEvent`.
    pub internal_event_count: usize,
}

// ── Loader ───────────────────────────────────────────────────────────────────

/// Loads and indexes telemetry events from `~/.claude/telemetry/*.json`.
#[derive(Debug, Clone)]
pub struct TelemetryLoader {
    telemetry_dir: PathBuf,
    sessions: HashMap<String, TelemetrySessionStats>,
    events: Vec<TelemetryEvent>,
    files_loaded: usize,
}

impl TelemetryLoader {
    /// Create a new loader pointing at the given Claude home directory.
    pub fn new(claude_home: impl AsRef<Path>) -> Self {
        let telemetry_dir = claude_home.as_ref().join("telemetry");
        Self {
            telemetry_dir,
            sessions: HashMap::new(),
            events: Vec::new(),
            files_loaded: 0,
        }
    }

    /// Load all telemetry files matching `1p_failed_events.*.json`.
    pub fn load(&mut self) -> Result<usize> {
        if !self.telemetry_dir.exists() {
            return Err(TelemetryError::NotFound(self.telemetry_dir.clone()));
        }

        let entries = fs::read_dir(&self.telemetry_dir).map_err(TelemetryError::Io)?;

        let mut paths: Vec<PathBuf> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name.starts_with("1p_failed_events.")
                && name.ends_with(".json")
            {
                paths.push(path);
            }
        }

        if paths.is_empty() {
            return Err(TelemetryError::NotFound(self.telemetry_dir.clone()));
        }

        let mut events = Vec::new();
        let mut sessions: HashMap<String, TelemetrySessionStats> = HashMap::new();
        let mut files_loaded = 0;

        for path in &paths {
            let file = File::open(path).map_err(TelemetryError::Io)?;
            let reader = BufReader::new(file);
            let file_events = Self::read_events(reader)?;

            for event in file_events {
                if event.is_internal() {
                    Self::update_session(&mut sessions, &event);
                    events.push(event);
                }
            }
            files_loaded += 1;
        }

        self.events = events;
        self.sessions = sessions;
        self.files_loaded = files_loaded;

        Ok(self.events.len())
    }

    fn read_events<R: BufRead>(reader: R) -> Result<Vec<TelemetryEvent>> {
        let mut events = Vec::new();

        for raw in reader.lines() {
            let raw_line = raw.map_err(TelemetryError::Io)?;
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            if let Ok(event) = serde_json::from_str::<TelemetryEvent>(line) {
                events.push(event);
            }
        }

        Ok(events)
    }

    fn update_session(
        sessions: &mut HashMap<String, TelemetrySessionStats>,
        event: &TelemetryEvent,
    ) {
        let sid = &event.event_data.session_id;
        let ts = event.event_data.client_timestamp;
        let name = &event.event_data.event_name;

        let session = sessions
            .entry(sid.clone())
            .or_insert_with(|| TelemetrySessionStats {
                session_id: sid.clone(),
                first_event: ts,
                last_event: ts,
                ..Default::default()
            });

        session.event_count += 1;
        session.last_event = ts;
        *session.event_names.entry(name.clone()).or_insert(0) += 1;

        if let Some(ref model) = event.event_data.model {
            *session.models.entry(model.clone()).or_insert(0) += 1;
        }
    }

    /// Returns all loaded events.
    pub fn events(&self) -> &[TelemetryEvent] {
        &self.events
    }

    /// Returns events for a specific session.
    pub fn events_for_session(&self, session_id: &str) -> Vec<&TelemetryEvent> {
        self.events
            .iter()
            .filter(|e| e.event_data.session_id == session_id)
            .collect()
    }

    /// Returns the summary of telemetry coverage.
    pub fn summary(&self) -> TelemetrySummary {
        let total_events = self.events.len();
        let total_sessions = self.sessions.len();
        let internal_event_count = self.events.iter().filter(|e| e.is_internal()).count();

        TelemetrySummary {
            total_events,
            total_sessions,
            total_files: self.files_loaded,
            internal_event_count,
        }
    }

    /// Returns session statistics for a given session.
    pub fn session_stats(&self, session_id: &str) -> Option<&TelemetrySessionStats> {
        self.sessions.get(session_id)
    }
}
