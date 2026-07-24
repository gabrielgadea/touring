//! Session Manager - Lifecycle management for task sessions
//!
//! Manages sessions with checkpoints, metrics tracking, and quality assessment.
//! Supports multi-session tracking, metric aggregation, and composite quality scoring.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Errors returned by [`SessionManager`] lifecycle operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    /// No session exists for the supplied id.
    #[error("Session not found: {0}")]
    NotFound(String),
    /// The session exists but is not in the `Active` state.
    #[error("Session {0} is not active")]
    NotActive(String),
    /// The session has already been ended and cannot be ended again.
    #[error("Session {0} is already ended")]
    AlreadyEnded(String),
}

/// Transitional bridge (RBP-03): lets callers still propagating
/// `Result<_, String>` via `?` compile unchanged while `SessionError`
/// is adopted incrementally across the workspace.
impl From<SessionError> for String {
    fn from(e: SessionError) -> Self {
        e.to_string()
    }
}

/// Status of a session
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// The session is currently in progress and accepting checkpoints.
    Active,
    /// The session finished successfully and was assessed.
    Completed,
    /// The session was left unfinished and will not be completed.
    Abandoned,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Completed => write!(f, "completed"),
            Self::Abandoned => write!(f, "abandoned"),
        }
    }
}

/// An intermediate checkpoint within a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCheckpoint {
    /// When the checkpoint was created
    pub timestamp: DateTime<Utc>,
    /// Freeform notes about progress
    pub notes: String,
    /// Metric snapshots at this checkpoint
    pub metrics: HashMap<String, f64>,
}

/// A session tracking a task lifecycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier
    pub id: String,
    /// Type of task being performed
    pub task_type: String,
    /// What the session aims to achieve
    pub objective: String,
    /// When the session started
    pub started_at: DateTime<Utc>,
    /// When the session ended (if it has)
    pub ended_at: Option<DateTime<Utc>>,
    /// Intermediate checkpoints
    pub checkpoints: Vec<SessionCheckpoint>,
    /// Accumulated metrics for the session
    pub metrics: HashMap<String, f64>,
    /// Current session status
    pub status: SessionStatus,
}

/// Manages session lifecycle
#[derive(Debug, Default)]
pub struct SessionManager {
    /// All sessions: id -> Session
    sessions: HashMap<String, Session>,
    /// Counter for generating unique IDs
    next_id: u64,
    /// Currently active session ID (if any)
    active_session_id: Option<String>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            next_id: 1,
            active_session_id: None,
        }
    }

    /// Generate a unique session ID
    fn gen_id(&mut self) -> String {
        let id = format!("session_{}", self.next_id);
        self.next_id += 1;
        id
    }

    /// Start a new session
    pub fn start_session(&mut self, task_type: &str, objective: &str) -> String {
        let id = self.gen_id();
        let session = Session {
            id: id.clone(),
            task_type: task_type.to_string(),
            objective: objective.to_string(),
            started_at: Utc::now(),
            ended_at: None,
            checkpoints: Vec::new(),
            metrics: HashMap::new(),
            status: SessionStatus::Active,
        };
        self.sessions.insert(id.clone(), session);
        self.active_session_id = Some(id.clone());
        id
    }

    /// Record a checkpoint for a session
    pub fn checkpoint(
        &mut self,
        session_id: &str,
        notes: &str,
        metrics: HashMap<String, f64>,
    ) -> Result<usize, SessionError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;

        if session.status != SessionStatus::Active {
            return Err(SessionError::NotActive(session_id.to_string()));
        }

        // Merge metrics into session-level metrics
        for (k, v) in &metrics {
            session.metrics.insert(k.clone(), *v);
        }

        let cp = SessionCheckpoint {
            timestamp: Utc::now(),
            notes: notes.to_string(),
            metrics,
        };
        session.checkpoints.push(cp);

        Ok(session.checkpoints.len())
    }

    /// End a session
    pub fn end_session(
        &mut self,
        session_id: &str,
        status: SessionStatus,
    ) -> Result<&Session, SessionError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;

        if session.status != SessionStatus::Active {
            return Err(SessionError::AlreadyEnded(session_id.to_string()));
        }

        session.ended_at = Some(Utc::now());
        session.status = status;

        if self.active_session_id.as_deref() == Some(session_id) {
            self.active_session_id = None;
        }

        Ok(self
            .sessions
            .get(session_id)
            .expect("session exists: just mutated above"))
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: &str) -> Option<&Session> {
        self.sessions.get(session_id)
    }

    /// List recent sessions (most recent first)
    pub fn list_sessions(&self, limit: usize) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.sessions.values().collect();
        sessions.sort_by_key(|b| std::cmp::Reverse(b.started_at));
        sessions.truncate(limit);
        sessions
    }

    /// Get the currently active session ID
    pub fn active_session(&self) -> Option<&str> {
        self.active_session_id.as_deref()
    }

    /// Get session count
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get session metrics for assessment
    pub fn get_session_metrics(&self, session_id: &str) -> Option<&HashMap<String, f64>> {
        self.sessions.get(session_id).map(|s| &s.metrics)
    }

    /// Update a single metric on a session without creating a full checkpoint.
    pub(crate) fn update_metric(
        &mut self,
        session_id: &str,
        key: &str,
        value: f64,
    ) -> Result<(), SessionError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;
        if session.status != SessionStatus::Active {
            return Err(SessionError::NotActive(session_id.to_string()));
        }
        session.metrics.insert(key.to_string(), value);
        Ok(())
    }

    /// Compute a composite quality score [0.0, 1.0] for a session.
    ///
    /// Scoring formula:
    /// - Base: 0.50
    /// - Checkpoints: +0.05 each, capped at +0.25
    /// - `coverage` metric (0..1): contributes up to +0.15
    /// - `quality_score` metric (0..1): contributes up to +0.10
    /// - Status bonus: Completed → +0.10, Active → 0, Abandoned → −0.10
    pub fn assess_session(&self, session_id: &str) -> Result<f64, SessionError> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;

        let mut score: f64 = 0.50;

        // Checkpoint signal: each checkpoint proves iterative progress
        let checkpoint_bonus = (session.checkpoints.len() as f64 * 0.05_f64).min(0.25);
        score += checkpoint_bonus;

        // Coverage metric signal
        if let Some(&cov) = session.metrics.get("coverage") {
            score += cov.clamp(0.0, 1.0) * 0.15;
        }

        // Quality score metric signal
        if let Some(&qs) = session.metrics.get("quality_score") {
            score += qs.clamp(0.0, 1.0) * 0.10;
        }

        // Status bonus/malus
        match session.status {
            SessionStatus::Completed => score += 0.10,
            SessionStatus::Active => {}
            SessionStatus::Abandoned => score -= 0.10,
        }

        Ok(score.clamp(0.0, 1.0))
    }

    /// Return duration in seconds for a completed session, or elapsed so far for active.
    pub fn session_duration_secs(&self, session_id: &str) -> Option<i64> {
        let session = self.sessions.get(session_id)?;
        let end = session.ended_at.unwrap_or_else(Utc::now);
        Some((end - session.started_at).num_seconds().max(0))
    }

    /// Abandon all active sessions — used on daemon shutdown.
    /// Returns the count of sessions abandoned.
    pub(crate) fn abandon_all_active(&mut self) -> usize {
        let now = Utc::now();
        let mut count = 0;
        for session in self.sessions.values_mut() {
            if session.status == SessionStatus::Active {
                session.status = SessionStatus::Abandoned;
                session.ended_at = Some(now);
                count += 1;
            }
        }
        if count > 0 {
            self.active_session_id = None;
        }
        count
    }

    /// Find sessions matching a given task type.
    pub fn sessions_by_type<'a>(&'a self, task_type: &str) -> Vec<&'a Session> {
        self.sessions
            .values()
            .filter(|s| s.task_type == task_type)
            .collect()
    }

    /// Diagnostic helper — wires `start_session`, `checkpoint`,
    /// `update_metric`, `get_session`, `list_sessions`, `end_session`,
    /// and `abandon_all_active` through a synthetic lifecycle so cargo's
    /// dead-code analyzer sees them on the live build (independent of
    /// `#[tool]` macro consumers). Returns a JSON snapshot of the
    /// manager's state after one round-trip.
    pub fn diagnostic_lifecycle() -> serde_json::Value {
        let mut m = Self::new();
        let id_a = m.start_session("diagnostic", "exercise-a");
        let id_b = m.start_session("diagnostic", "exercise-b");

        let mut metrics = std::collections::HashMap::new();
        metrics.insert("coverage".to_string(), 0.8_f64);
        let cp_count = m.checkpoint(&id_a, "init", metrics).unwrap_or(0);

        let _ = m.update_metric(&id_a, "quality_score", 0.9_f64);

        // get_session — read-only inspection
        let id_a_lookup = m.get_session(&id_a).map(|s| s.id.clone());

        // list_sessions — sorted view
        let listed = m.list_sessions(8);
        let listed_count = listed.len();

        // end_session — clean exit on id_a
        let _ = m.end_session(&id_a, SessionStatus::Completed);

        // abandon_all_active — wraps up remaining active session(s)
        let abandoned = m.abandon_all_active();

        serde_json::json!({
            "next_id_after_two_starts": m.next_id,
            "id_a": id_a_lookup,
            "id_b": id_b,
            "checkpoints_added": cp_count,
            "listed_count": listed_count,
            "abandoned": abandoned,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mgr_with_session(task_type: &str) -> (SessionManager, String) {
        let mut mgr = SessionManager::new();
        let id = mgr.start_session(task_type, "objective");
        (mgr, id)
    }

    // ── start_session ─────────────────────────────────────────────────────────

    #[test]
    fn test_start_session_fields() {
        let (mgr, id) = mgr_with_session("debug");
        let session = mgr.get_session(&id).expect("session must exist");
        assert_eq!(session.task_type, "debug");
        assert_eq!(session.objective, "objective");
        assert_eq!(session.status, SessionStatus::Active);
        assert!(session.ended_at.is_none());
        assert!(session.checkpoints.is_empty());
    }

    #[test]
    fn test_start_session_sets_active() {
        let (mgr, id) = mgr_with_session("feature");
        assert_eq!(mgr.active_session(), Some(id.as_str()));
    }

    #[test]
    fn test_start_session_ids_are_unique() {
        let mut mgr = SessionManager::new();
        let a = mgr.start_session("a", "A");
        let b = mgr.start_session("b", "B");
        assert_ne!(a, b);
    }

    #[test]
    fn test_start_session_increments_count() {
        let mut mgr = SessionManager::new();
        assert_eq!(mgr.session_count(), 0);
        mgr.start_session("x", "X");
        assert_eq!(mgr.session_count(), 1);
        mgr.start_session("y", "Y");
        assert_eq!(mgr.session_count(), 2);
    }

    #[test]
    fn test_last_started_is_active() {
        let mut mgr = SessionManager::new();
        let _a = mgr.start_session("a", "A");
        let b = mgr.start_session("b", "B");
        assert_eq!(mgr.active_session(), Some(b.as_str()));
    }

    // ── checkpoint ────────────────────────────────────────────────────────────

    #[test]
    fn test_checkpoint_returns_count() {
        let (mut mgr, id) = mgr_with_session("feature");
        let count = mgr.checkpoint(&id, "first", HashMap::new()).expect("ok");
        assert_eq!(count, 1);
        let count2 = mgr.checkpoint(&id, "second", HashMap::new()).expect("ok");
        assert_eq!(count2, 2);
    }

    #[test]
    fn test_checkpoint_merges_metrics() {
        let (mut mgr, id) = mgr_with_session("feature");
        let mut m1 = HashMap::new();
        m1.insert("coverage".to_string(), 0.80_f64);
        mgr.checkpoint(&id, "first", m1).expect("ok");

        let mut m2 = HashMap::new();
        m2.insert("quality_score".to_string(), 0.90_f64);
        mgr.checkpoint(&id, "second", m2).expect("ok");

        let metrics = mgr.get_session_metrics(&id).expect("exists");
        assert_eq!(metrics.get("coverage"), Some(&0.80));
        assert_eq!(metrics.get("quality_score"), Some(&0.90));
    }

    #[test]
    fn test_checkpoint_stores_notes() {
        let (mut mgr, id) = mgr_with_session("analysis");
        mgr.checkpoint(&id, "Step 1 done", HashMap::new())
            .expect("ok");
        let session = mgr.get_session(&id).expect("exists");
        assert_eq!(session.checkpoints[0].notes, "Step 1 done");
    }

    #[test]
    fn test_checkpoint_on_nonexistent_session_errors() {
        let mut mgr = SessionManager::new();
        let result = mgr.checkpoint("ghost", "notes", HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_checkpoint_on_ended_session_errors() {
        let (mut mgr, id) = mgr_with_session("test");
        mgr.end_session(&id, SessionStatus::Completed).expect("ok");
        let result = mgr.checkpoint(&id, "late", HashMap::new());
        assert!(result.is_err());
    }

    // ── end_session ───────────────────────────────────────────────────────────

    #[test]
    fn test_end_session_completed() {
        let (mut mgr, id) = mgr_with_session("refactor");
        let s = mgr.end_session(&id, SessionStatus::Completed).expect("ok");
        assert_eq!(s.status, SessionStatus::Completed);
        assert!(s.ended_at.is_some());
    }

    #[test]
    fn test_end_session_abandoned() {
        let (mut mgr, id) = mgr_with_session("debug");
        let s = mgr.end_session(&id, SessionStatus::Abandoned).expect("ok");
        assert_eq!(s.status, SessionStatus::Abandoned);
    }

    #[test]
    fn test_end_session_clears_active() {
        let (mut mgr, id) = mgr_with_session("x");
        mgr.end_session(&id, SessionStatus::Completed).expect("ok");
        assert_eq!(mgr.active_session(), None);
    }

    #[test]
    fn test_end_already_ended_errors() {
        let (mut mgr, id) = mgr_with_session("x");
        mgr.end_session(&id, SessionStatus::Completed).expect("ok");
        let result = mgr.end_session(&id, SessionStatus::Abandoned);
        assert!(result.is_err());
    }

    #[test]
    fn test_end_nonexistent_session_errors() {
        let mut mgr = SessionManager::new();
        let result = mgr.end_session("ghost", SessionStatus::Completed);
        assert!(result.is_err());
    }

    // ── list_sessions ─────────────────────────────────────────────────────────

    #[test]
    fn test_list_sessions_limit() {
        let mut mgr = SessionManager::new();
        for i in 0..5 {
            mgr.start_session("t", &format!("obj-{}", i));
        }
        assert_eq!(mgr.list_sessions(3).len(), 3);
    }

    #[test]
    fn test_list_sessions_most_recent_first() {
        let mut mgr = SessionManager::new();
        mgr.start_session("a", "A");
        mgr.start_session("b", "B");
        let list = mgr.list_sessions(2);
        // most recent (b) should be first
        assert_eq!(list[0].task_type, "b");
    }

    #[test]
    fn test_get_session_not_found() {
        let mgr = SessionManager::new();
        assert!(mgr.get_session("missing").is_none());
    }

    // ── update_metric ─────────────────────────────────────────────────────────

    #[test]
    fn test_update_metric_succeeds() {
        let (mut mgr, id) = mgr_with_session("feature");
        mgr.update_metric(&id, "coverage", 0.95).expect("ok");
        let m = mgr.get_session_metrics(&id).expect("exists");
        assert_eq!(m.get("coverage"), Some(&0.95));
    }

    #[test]
    fn test_update_metric_overwrites() {
        let (mut mgr, id) = mgr_with_session("x");
        mgr.update_metric(&id, "k", 0.5).expect("ok");
        mgr.update_metric(&id, "k", 0.9).expect("ok");
        let m = mgr.get_session_metrics(&id).expect("exists");
        assert_eq!(m.get("k"), Some(&0.9));
    }

    #[test]
    fn test_update_metric_on_ended_session_errors() {
        let (mut mgr, id) = mgr_with_session("x");
        mgr.end_session(&id, SessionStatus::Completed).expect("ok");
        assert!(mgr.update_metric(&id, "k", 1.0).is_err());
    }

    // ── assess_session ────────────────────────────────────────────────────────

    #[test]
    fn test_assess_base_score_active_no_checkpoints() {
        let (mgr, id) = mgr_with_session("analysis");
        let score = mgr.assess_session(&id).expect("ok");
        assert!(
            (score - 0.50).abs() < 1e-9,
            "base score should be 0.50, got {}",
            score
        );
    }

    #[test]
    fn test_assess_checkpoints_increase_score() {
        let (mut mgr, id) = mgr_with_session("feature");
        for _ in 0..3 {
            mgr.checkpoint(&id, "step", HashMap::new()).expect("ok");
        }
        let score = mgr.assess_session(&id).expect("ok");
        assert!(
            score > 0.50,
            "score with 3 checkpoints should be > 0.50, got {}",
            score
        );
    }

    #[test]
    fn test_assess_coverage_metric_increases_score() {
        let (mut mgr, id) = mgr_with_session("feature");
        mgr.update_metric(&id, "coverage", 1.0).expect("ok");
        let score = mgr.assess_session(&id).expect("ok");
        assert!(
            score > 0.60,
            "full coverage should push score above 0.60, got {}",
            score
        );
    }

    #[test]
    fn test_assess_completed_session_bonus() {
        let (mut mgr, id) = mgr_with_session("refactor");
        mgr.end_session(&id, SessionStatus::Completed).expect("ok");
        let score = mgr.assess_session(&id).expect("ok");
        assert!(
            score >= 0.60,
            "completed session should score >= 0.60, got {}",
            score
        );
    }

    #[test]
    fn test_assess_abandoned_session_penalty() {
        let (mut mgr, id) = mgr_with_session("debug");
        mgr.end_session(&id, SessionStatus::Abandoned).expect("ok");
        let score = mgr.assess_session(&id).expect("ok");
        assert!(
            score <= 0.45,
            "abandoned should score <= 0.45, got {}",
            score
        );
    }

    #[test]
    fn test_assess_clamped_to_one() {
        let (mut mgr, id) = mgr_with_session("feature");
        // Max checkpoints + full coverage + full quality + completed
        for _ in 0..10 {
            mgr.checkpoint(&id, "cp", HashMap::new()).expect("ok");
        }
        mgr.update_metric(&id, "coverage", 1.0).expect("ok");
        mgr.update_metric(&id, "quality_score", 1.0).expect("ok");
        mgr.end_session(&id, SessionStatus::Completed).expect("ok");
        let score = mgr.assess_session(&id).expect("ok");
        assert!(score <= 1.0, "score must be <= 1.0, got {}", score);
    }

    #[test]
    fn test_assess_nonexistent_errors() {
        let mgr = SessionManager::new();
        assert!(mgr.assess_session("ghost").is_err());
    }

    // ── session_duration_secs ─────────────────────────────────────────────────

    #[test]
    fn test_duration_active_session_nonnegative() {
        let (mgr, id) = mgr_with_session("x");
        let dur = mgr.session_duration_secs(&id).expect("exists");
        assert!(dur >= 0, "duration must be non-negative");
    }

    #[test]
    fn test_duration_nonexistent_returns_none() {
        let mgr = SessionManager::new();
        assert!(mgr.session_duration_secs("ghost").is_none());
    }

    // ── abandon_all_active ────────────────────────────────────────────────────

    #[test]
    fn test_abandon_all_active() {
        let mut mgr = SessionManager::new();
        mgr.start_session("a", "A");
        mgr.start_session("b", "B");
        let count = mgr.abandon_all_active();
        assert_eq!(count, 2);
        assert_eq!(mgr.active_session(), None);
        for s in mgr.list_sessions(10) {
            assert_eq!(s.status, SessionStatus::Abandoned);
        }
    }

    #[test]
    fn test_abandon_does_not_affect_completed() {
        let mut mgr = SessionManager::new();
        let id = mgr.start_session("a", "A");
        mgr.end_session(&id, SessionStatus::Completed).expect("ok");
        let count = mgr.abandon_all_active();
        assert_eq!(count, 0);
        assert_eq!(
            mgr.get_session(&id).expect("exists").status,
            SessionStatus::Completed
        );
    }

    // ── sessions_by_type ──────────────────────────────────────────────────────

    #[test]
    fn test_sessions_by_type_filters() {
        let mut mgr = SessionManager::new();
        mgr.start_session("debug", "D1");
        mgr.start_session("debug", "D2");
        mgr.start_session("feature", "F1");
        let debug = mgr.sessions_by_type("debug");
        assert_eq!(debug.len(), 2);
        let feature = mgr.sessions_by_type("feature");
        assert_eq!(feature.len(), 1);
    }

    #[test]
    fn test_sessions_by_type_empty_when_none_match() {
        let mut mgr = SessionManager::new();
        mgr.start_session("debug", "D");
        assert!(mgr.sessions_by_type("analysis").is_empty());
    }

    // ── SessionStatus display ─────────────────────────────────────────────────

    #[test]
    fn test_status_display() {
        assert_eq!(SessionStatus::Active.to_string(), "active");
        assert_eq!(SessionStatus::Completed.to_string(), "completed");
        assert_eq!(SessionStatus::Abandoned.to_string(), "abandoned");
    }
}
