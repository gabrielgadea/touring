//! Audit statistics and loader implementation.
//! Separated from audit.rs to keep cyclomatic complexity manageable.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::errors::{AuditError, Result};

// ── Data structures ───────────────────────────────────────────────────────────

/// An audit event from `audit.jsonl`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditEvent {
    /// Unix timestamp (seconds, fractional) when the event was recorded.
    pub timestamp: f64,
    /// Claude Code session this event belongs to.
    pub session_id: String,
    /// Name of the tool that triggered the event.
    pub tool_name: String,
    /// Lifecycle hook event name (e.g. `PreToolUse`).
    pub hook_event: String,
    /// Gate decision for the event (e.g. `allow`, `deny`, `block`).
    pub decision: String,
    /// Path of the file the tool acted on, if any.
    pub file_path: String,
    /// Free-form details describing the event.
    pub details: String,
}

impl Default for AuditEvent {
    fn default() -> Self {
        Self {
            timestamp: 0.0,
            session_id: String::new(),
            tool_name: String::new(),
            hook_event: String::new(),
            decision: "allow".into(),
            file_path: String::new(),
            details: String::new(),
        }
    }
}

impl AuditEvent {
    /// Returns `true` if the event was denied or blocked.
    pub fn is_denied(&self) -> bool {
        self.decision == "deny" || self.decision == "block"
    }

    /// Returns `true` if the event is a gate decision point (`PreToolUse`).
    pub fn is_decision_point(&self) -> bool {
        self.hook_event == "PreToolUse"
    }
}

/// Aggregated statistics for a session.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SessionStats {
    /// Identifier of the session being summarized.
    pub session_id: String,
    /// Total number of events in the session.
    pub event_count: usize,
    /// Per-tool event counts.
    pub tool_counts: HashMap<String, usize>,
    /// Number of denied/blocked events.
    pub deny_count: usize,
    /// Per-tool denied/blocked event counts.
    pub deny_by_tool: HashMap<String, usize>,
    /// Number of events that touched a file.
    pub files_touched: usize,
    /// Timestamp of the first event in the session.
    pub first_event: f64,
    /// Timestamp of the last event in the session.
    pub last_event: f64,
}

impl SessionStats {
    /// Wall-clock span of the session in seconds.
    pub fn duration_secs(&self) -> f64 {
        self.last_event - self.first_event
    }

    /// Fraction of events that were denied (0.0 when no events).
    pub fn deny_rate(&self) -> f64 {
        if self.event_count == 0 {
            return 0.0;
        }
        self.deny_count as f64 / self.event_count as f64
    }
}

/// Aggregated statistics for a tool.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ToolStats {
    /// Name of the tool being summarized.
    pub tool_name: String,
    /// Total number of times the tool was invoked.
    pub total_uses: usize,
    /// Number of invocations that were denied/blocked.
    pub deny_count: usize,
    /// Number of distinct sessions that used the tool.
    pub sessions: usize,
    /// Per-file touch counts for the tool.
    pub files_touched: HashMap<String, usize>,
}

impl ToolStats {
    /// Fraction of invocations that were denied (0.0 when no uses).
    pub fn deny_rate(&self) -> f64 {
        if self.total_uses == 0 {
            0.0
        } else {
            self.deny_count as f64 / self.total_uses as f64
        }
    }

    /// Complement of `deny_rate` — fraction of allowed invocations.
    pub fn reliability(&self) -> f64 {
        1.0 - self.deny_rate()
    }
}

/// Summary statistics for the entire audit file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditSummary {
    /// Total events across the whole audit file.
    pub total_events: usize,
    /// Number of distinct sessions observed.
    pub total_sessions: usize,
    /// Number of distinct tools observed.
    pub total_tools: usize,
    /// Total denied/blocked events.
    pub deny_count: usize,
    /// Fraction of all events that were denied.
    pub deny_rate: f64,
    /// Number of decision-point events (`PreToolUse`).
    pub decision_count: usize,
    /// Fraction of decision-point events that were denied.
    pub decision_deny_rate: f64,
}

// ── Loader ───────────────────────────────────────────────────────────────────

/// Loads and indexes audit events from `audit.jsonl`.
#[derive(Debug, Clone)]
pub struct AuditLoader {
    audit_path: PathBuf,
    sessions: HashMap<String, SessionStats>,
    tools: HashMap<String, ToolStats>,
    events: Vec<AuditEvent>,
}

impl AuditLoader {
    /// Create a new loader pointing at the given Claude home directory.
    pub fn new(claude_home: impl AsRef<Path>) -> Self {
        let audit_path = claude_home.as_ref().join("compliance/audit.jsonl");
        Self {
            audit_path,
            sessions: HashMap::new(),
            tools: HashMap::new(),
            events: Vec::new(),
        }
    }

    /// Load all events from `audit.jsonl` and build indexes.
    pub fn load(&mut self) -> Result<usize> {
        if !self.audit_path.exists() {
            return Err(AuditError::NotFound(self.audit_path.clone()));
        }

        let file = File::open(&self.audit_path)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        let mut sessions: HashMap<String, SessionStats> = HashMap::new();
        let mut tools: HashMap<String, ToolStats> = HashMap::new();

        for (line_no, raw) in reader.lines().enumerate() {
            let raw_line = raw?;
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            let event: AuditEvent = serde_json::from_str(line).map_err(|e| AuditError::Json {
                line: line_no + 1,
                source: e,
            })?;

            Self::update_session(&mut sessions, &event);
            Self::update_tool(&mut tools, &event);
            events.push(event);
        }

        // Count unique sessions per tool
        let mut session_set: HashMap<String, ()> = HashMap::new();
        for event in &events {
            session_set.insert(event.session_id.clone(), ());
        }
        for tool in tools.values_mut() {
            tool.sessions = session_set.len();
        }

        self.events = events;
        self.sessions = sessions;
        self.tools = tools;

        Ok(self.events.len())
    }

    fn update_session(sessions: &mut HashMap<String, SessionStats>, event: &AuditEvent) {
        let session = sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| SessionStats {
                session_id: event.session_id.clone(),
                first_event: event.timestamp,
                last_event: event.timestamp,
                ..Default::default()
            });

        session.event_count += 1;
        session.last_event = event.timestamp;
        *session
            .tool_counts
            .entry(event.tool_name.clone())
            .or_insert(0) += 1;

        if event.is_denied() {
            session.deny_count += 1;
            *session
                .deny_by_tool
                .entry(event.tool_name.clone())
                .or_insert(0) += 1;
        }
        if !event.file_path.is_empty() {
            session.files_touched += 1;
        }
    }

    fn update_tool(tools: &mut HashMap<String, ToolStats>, event: &AuditEvent) {
        let tool = tools
            .entry(event.tool_name.clone())
            .or_insert_with(|| ToolStats {
                tool_name: event.tool_name.clone(),
                ..Default::default()
            });

        tool.total_uses += 1;
        if event.is_denied() {
            tool.deny_count += 1;
        }
        if !event.file_path.is_empty() {
            *tool
                .files_touched
                .entry(event.file_path.clone())
                .or_insert(0) += 1;
        }
    }

    /// Returns all loaded events.
    pub fn events(&self) -> &[AuditEvent] {
        &self.events
    }

    /// Returns events for a specific session.
    pub fn events_for_session(&self, session_id: &str) -> Vec<&AuditEvent> {
        self.events
            .iter()
            .filter(|e| e.session_id == session_id)
            .collect()
    }

    /// Returns the summary of audit coverage.
    pub fn summary(&self) -> AuditSummary {
        let total_events = self.events.len();
        let total_sessions = self.sessions.len();
        let total_tools = self.tools.len();
        let deny_count = self.events.iter().filter(|e| e.is_denied()).count();
        let decision_count = self.events.iter().filter(|e| e.is_decision_point()).count();

        AuditSummary {
            total_events,
            total_sessions,
            total_tools,
            deny_count,
            deny_rate: if total_events > 0 {
                deny_count as f64 / total_events as f64
            } else {
                0.0
            },
            decision_count,
            decision_deny_rate: if decision_count > 0 {
                deny_count as f64 / decision_count as f64
            } else {
                0.0
            },
        }
    }

    /// Returns sessions sorted by event count.
    pub fn top_sessions(&self, limit: usize) -> Vec<&SessionStats> {
        let mut sessions: Vec<_> = self.sessions.values().collect();
        sessions.sort_by_key(|b| std::cmp::Reverse(b.event_count));
        sessions.truncate(limit);
        sessions
    }

    /// Returns tools sorted by total uses.
    pub fn top_tools(&self, limit: usize) -> Vec<&ToolStats> {
        let mut tools: Vec<_> = self.tools.values().collect();
        tools.sort_by_key(|b| std::cmp::Reverse(b.total_uses));
        tools.truncate(limit);
        tools
    }
}
