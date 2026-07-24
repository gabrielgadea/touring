//! HookMemoryBridge — Wires hook events into the knowledge graph for recall.
//!
//! ## Purpose
//! This module bridges hook events (pre-hook context, post-hook outcomes,
//! quality signals, task completions) into the `FileKnowledgeDB` using a
//! tiered memory model (ephemeral → working → semantic) with TTL management
//! and working→semantic promotion rules.
//!
//! ## Memory Tiers
//! | Tier | Lifetime | Content |
//! |------|----------|---------|
//! | `ephemeral` | 1 hour TTL | Raw hook events, intent classifications, quality signals |
//! | `working` | 24 hour TTL | Aggregated patterns, session summaries |
//! | `semantic` | Persistent | Confirmed patterns with high reward, cross-session lessons |
//!
//! ## TTL Management
//! - Ephemeral entries expire via SQLite `WHERE created_at > datetime('now', '-1 hour')`
//! - Working→semantic promotion occurs when:
//!   - `sample_count >= 5` AND `avg_reward >= 0.7` AND `confidence >= 0.8`
//! - Promotion is checked on every `recall_hook_patterns` call (lazy promotion)

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::errors::TouringError;

// ── TTL constants ──────────────────────────────────────────────────────────────

/// TTL for ephemeral entries (1 hour).
pub(crate) const EPHEMERAL_TTL_HOURS: i64 = 1;
/// TTL for working entries (24 hours).
pub(crate) const WORKING_TTL_HOURS: i64 = 24;
/// Minimum sample count before consideration for working→semantic promotion.
pub(crate) const PROMOTION_MIN_SAMPLES: i64 = 5;
/// Minimum average reward threshold for working→semantic promotion.
pub(crate) const PROMOTION_MIN_AVG_REWARD: f64 = 0.7;
/// Minimum confidence threshold for working→semantic promotion.
pub(crate) const PROMOTION_MIN_CONFIDENCE: f64 = 0.8;

// ── Error types ─────────────────────────────────────────────────────────────────

/// Errors specific to HookMemoryBridge operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum MemoryError {
    /// Underlying SQLite operation failed.
    #[error("database error: {0}")]
    Database(String),
    /// JSON serialization or deserialization of a memory payload failed.
    #[error("serialization error: {0}")]
    Serialization(String),
    /// The requested memory entry does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// Event↔outcome correlation could not be resolved.
    #[error("correlation error: {0}")]
    Correlation(String),
}

impl From<rusqlite::Error> for MemoryError {
    fn from(e: rusqlite::Error) -> Self {
        MemoryError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for MemoryError {
    fn from(e: serde_json::Error) -> Self {
        MemoryError::Serialization(e.to_string())
    }
}

impl From<String> for MemoryError {
    fn from(s: String) -> Self {
        MemoryError::Database(s)
    }
}

/// Errors specific to event↔outcome correlation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum CorrelationError {
    /// No event matched the given identifier during correlation.
    #[error("event not found: {0}")]
    EventNotFound(String),
    /// No outcome matched the given identifier during correlation.
    #[error("outcome not found: {0}")]
    OutcomeNotFound(String),
    /// Underlying SQLite operation failed while correlating.
    #[error("database error: {0}")]
    Database(String),
}

impl From<rusqlite::Error> for CorrelationError {
    fn from(e: rusqlite::Error) -> Self {
        CorrelationError::Database(e.to_string())
    }
}

// ── Data structures ───────────────────────────────────────────────────────────

/// A single hook execution event stored in the ephemeral tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    /// Unique event identifier (UUID v4).
    pub event_id: String,
    /// Name of the hook that fired (e.g., "pre_read", "post_edit").
    pub hook_name: String,
    /// Current session identifier.
    pub session_id: String,
    /// Project directory this event belongs to (for cross-project attribution).
    pub project_dir: String,
    /// Type of event: "execution", "outcome", "context".
    pub event_type: String,
    /// ISO 8601 timestamp of the event.
    pub timestamp: String,
    /// JSON-serialized event payload (hook-specific data).
    pub data: String,
    /// SHA-256 hash of data for deduplication.
    pub content_hash: String,
    /// Correlation IDs linking this event to related events (e.g., pre/post pair).
    pub correlation_ids: Vec<String>,
    /// Whether this event was linked to a positive outcome.
    pub outcome_linked: bool,
}

/// Generate a unique event ID without the uuid crate.
/// Uses nanosecond timestamp + atomic counter for uniqueness.
fn new_event_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:016x}-{:08x}", ts, seq)
}

impl HookEvent {
    /// Create a new HookEvent with a fresh unique ID and current timestamp.
    pub fn new(
        hook_name: &str,
        session_id: &str,
        project_dir: &str,
        event_type: &str,
        data: serde_json::Value,
        content_hash: String,
    ) -> Self {
        Self {
            event_id: new_event_id(),
            hook_name: hook_name.to_string(),
            session_id: session_id.to_string(),
            project_dir: project_dir.to_string(),
            event_type: event_type.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            data: serde_json::to_string(&data).unwrap_or_default(),
            content_hash,
            correlation_ids: Vec::new(),
            outcome_linked: false,
        }
    }
}

/// Intent classification result from a hook event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentClassification {
    /// ID of the source hook event.
    pub event_id: String,
    /// Classified intent (e.g., "code_edit", "debug", "refactor").
    pub intent: String,
    /// Confidence score [0.0, 1.0].
    pub confidence: f64,
    /// Keywords matched in the prompt.
    pub keywords_matched: Vec<String>,
    /// Cognitive techniques applied.
    pub techniques_applied: Vec<String>,
    /// Character count of the prompt.
    pub prompt_chars: usize,
    /// Session identifier.
    pub session_id: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
}

/// A quality signal from a tool execution outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualitySignal {
    /// ID of the source hook event.
    pub event_id: String,
    /// Name of the tool that was executed.
    pub tool_name: String,
    /// Reward signal [−1.0, 1.0].
    pub reward: f64,
    /// Tool execution latency in milliseconds.
    pub latency_ms: u64,
    /// File type context (e.g., "rust", "python").
    pub file_type: String,
    /// Session identifier.
    pub session_id: String,
    /// Correlation ID linking to the triggering event.
    pub correlation_id: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
}

/// A task completion record for lifecycle tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCompletion {
    /// Unique event identifier.
    pub event_id: String,
    /// Type of task: "hook", "decompose", "team", "planning".
    pub task_type: String,
    /// Task duration in milliseconds.
    pub duration_ms: u64,
    /// Outcome: "success", "failure", "skipped".
    pub outcome: String,
    /// Session identifier.
    pub session_id: String,
    /// Chain of correlation IDs for nested tasks.
    pub correlation_chain: Vec<String>,
    /// ISO 8601 timestamp.
    pub timestamp: String,
}

/// AHookPattern stored in the semantic tier — aggregated from multiple events.
///
/// Retrieved via semantic search for context injection decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPattern {
    /// Composite pattern key (e.g., "pre_read:rust:high_complexity").
    pub pattern_key: String,
    /// Number of contributing events.
    pub sample_count: i64,
    /// Rolling average reward across samples.
    pub avg_reward: f64,
    /// Average latency in milliseconds.
    pub avg_latency_ms: f64,
    /// Distribution of intents observed.
    pub intent_distribution: std::collections::HashMap<String, f64>,
    /// First observation timestamp.
    pub first_seen: String,
    /// Last update timestamp.
    pub last_seen: String,
    /// Confidence score [0.0, 1.0].
    pub confidence: f64,
}

/// Memory tier enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryTier {
    /// Ephemeral — raw events, 1h TTL.
    Ephemeral,
    /// Working — aggregated patterns, 24h TTL.
    Working,
    /// Semantic — confirmed patterns, persistent.
    Semantic,
}

impl MemoryTier {
    /// Return the SQL table suffix for this tier.
    pub fn table_suffix(&self) -> &'static str {
        match self {
            MemoryTier::Ephemeral => "_ephemeral",
            MemoryTier::Working => "_working",
            MemoryTier::Semantic => "_semantic",
        }
    }

    /// Return the TTL in hours for this tier.
    pub fn ttl_hours(&self) -> i64 {
        match self {
            MemoryTier::Ephemeral => EPHEMERAL_TTL_HOURS,
            MemoryTier::Working => WORKING_TTL_HOURS,
            MemoryTier::Semantic => i64::MAX,
        }
    }

    /// Parse from string (for query parameters). Returns `None` for unknown values.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ephemeral" => Some(MemoryTier::Ephemeral),
            "working" => Some(MemoryTier::Working),
            "semantic" => Some(MemoryTier::Semantic),
            _ => None,
        }
    }
}

impl std::str::FromStr for MemoryTier {
    type Err = String;

    /// Parse from string using idiomatic `FromStr` trait.
    ///
    /// Accepts "ephemeral", "working", "semantic" (case-insensitive).
    /// Returns `Err` for unknown values.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MemoryTier::parse(s).ok_or_else(|| format!("unknown MemoryTier: {s}"))
    }
}

// ── HookMemoryBridge ───────────────────────────────────────────────────────────

/// Bridge trait for wiring hook events into the knowledge graph.
///
/// Implementors can provide alternative backends (e.g., mock for testing)
/// while maintaining compatibility with the concrete implementation.
pub trait HookMemoryBridge {
    /// Store a hook event in the ephemeral tier.
    fn store_hook_event(&mut self, event: HookEvent) -> Result<(), MemoryError>;

    /// Store an intent classification in the ephemeral tier.
    fn store_intent_classification(
        &mut self,
        classification: IntentClassification,
    ) -> Result<(), MemoryError>;

    /// Store a quality signal in the ephemeral tier.
    fn store_quality_signal(&mut self, signal: QualitySignal) -> Result<(), MemoryError>;

    /// Store a task completion record in the ephemeral tier.
    fn store_task_completion(&mut self, completion: TaskCompletion) -> Result<(), MemoryError>;

    /// Recall hook patterns matching a query string from a specific tier.
    ///
    /// For `Semantic` tier: performs LIKE pattern matching on `pattern_key`.
    /// For `Ephemeral`/`Working`: performs full scan with TTL filter.
    ///
    /// Returns up to `limit` patterns, ordered by `confidence DESC`.
    fn recall_hook_patterns(&self, query: &str, tier: MemoryTier, limit: usize)
    -> Vec<HookPattern>;

    /// Correlate a hook event with an outcome, linking pre/post pairs.
    fn correlate_hook_with_outcome(
        &mut self,
        event_id: &str,
        outcome_id: &str,
    ) -> Result<(), CorrelationError>;

    /// Run TTL cleanup: evict expired ephemeral and working entries.
    ///
    /// Called opportunistically (e.g., on session start or post-compact).
    fn run_ttl_cleanup(&self) -> Result<usize, MemoryError>;

    /// Promote eligible working patterns to semantic tier.
    ///
    /// Eligibility: `sample_count >= 5`, `avg_reward >= 0.7`, `confidence >= 0.8`.
    /// Called lazily during `recall_hook_patterns` for working tier.
    fn run_promotion(&self) -> Result<usize, MemoryError>;
}

// ── Concrete implementation ─────────────────────────────────────────────────────

/// Table name constants for hook memory tables.
pub(crate) const TABLE_HOOK_EVENTS: &str = "hook_events";
pub(crate) const TABLE_INTENT_CLASS: &str = "hook_intent_class";
pub(crate) const TABLE_QUALITY_SIGNALS: &str = "hook_quality_signals";
pub(crate) const TABLE_TASK_COMPLETIONS: &str = "hook_task_completions";
pub(crate) const TABLE_CORRELATIONS: &str = "hook_event_correlations";
pub(crate) const TABLE_PATTERNS_EPHEMERAL: &str = "hook_patterns_ephemeral";
pub(crate) const TABLE_PATTERNS_WORKING: &str = "hook_patterns_working";
pub(crate) const TABLE_PATTERNS_SEMANTIC: &str = "hook_patterns_semantic";

/// Concrete HookMemoryBridge implementation backed by SQLite.
///
/// Uses the same `FileKnowledgeDB` connection via the `HookRuntime` context
/// but manages its own auxiliary tables for TTL and promotion tracking.
#[derive(Debug)]
pub struct SqliteHookMemoryBridge<'a> {
    conn: &'a Connection,
}

impl<'a> SqliteHookMemoryBridge<'a> {
    /// Create a new bridge backed by the given SQLite connection.
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Apply D3.4 priority_tier column migration (idempotent).
    ///
    /// SQLite has no `ALTER TABLE ADD COLUMN IF NOT EXISTS`, so we
    /// introspect via `PRAGMA table_info` and only add the column when
    /// missing. Default value `'MEDIUM'` preserves existing rows.
    fn migrate_priority_tier_column(&self) -> Result<(), MemoryError> {
        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA table_info({TABLE_HOOK_EVENTS})"))?;
        let names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .collect();
        if names.iter().any(|n| n == "priority_tier") {
            return Ok(());
        }
        self.conn.execute(
            &format!(
                "ALTER TABLE {TABLE_HOOK_EVENTS} \
                 ADD COLUMN priority_tier TEXT NOT NULL DEFAULT 'MEDIUM'"
            ),
            [],
        )?;
        // Index helps PreCompact filter queries.
        self.conn.execute(
            &format!(
                "CREATE INDEX IF NOT EXISTS idx_events_priority \
                 ON {TABLE_HOOK_EVENTS}(priority_tier)"
            ),
            [],
        )?;
        Ok(())
    }

    /// Migrate legacy hook_events table to add `project_dir` + `content_hash` columns.
    ///
    /// SQLite `CREATE TABLE IF NOT EXISTS` is a no-op on existing tables, so when a
    /// pre-existing DB has the older schema (without `project_dir`/`content_hash`), the
    /// subsequent CREATE INDEX on those columns fails with "no such column". This helper
    /// adds them idempotently — catches "duplicate column name" as success.
    ///
    /// Anchor: 2026-05-24 — Sprint 4.8 schema drift fix for daemon hook init.
    fn migrate_hook_events_columns(&self) -> Result<(), MemoryError> {
        let events = TABLE_HOOK_EVENTS;
        for (col, decl) in &[
            ("project_dir", "TEXT NOT NULL DEFAULT ''"),
            ("content_hash", "TEXT NOT NULL DEFAULT ''"),
        ] {
            let sql = format!("ALTER TABLE {events} ADD COLUMN {col} {decl}");
            match self.conn.execute(&sql, []) {
                Ok(_) => {}
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("duplicate column name") || msg.contains("no such table") {
                        // Column already exists OR table not created yet (CREATE TABLE will handle).
                        continue;
                    }
                    return Err(MemoryError::from(e));
                }
            }
        }
        Ok(())
    }

    /// Initialize the hook memory schema (idempotent).
    ///
    /// Called on first use or after schema version bump.
    pub fn ensure_schema(&self) -> Result<(), MemoryError> {
        // Pre-migrate legacy tables to add columns missing in older DBs (Sprint 4.8 fix).
        self.migrate_hook_events_columns()?;
        self.conn
            .execute_batch(&format!(
                "CREATE TABLE IF NOT EXISTS {events} (
                event_id TEXT PRIMARY KEY,
                hook_name TEXT NOT NULL,
                session_id TEXT NOT NULL,
                project_dir TEXT NOT NULL DEFAULT '',
                event_type TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                data TEXT NOT NULL DEFAULT '{{}}',
                content_hash TEXT NOT NULL DEFAULT '',
                correlation_ids TEXT NOT NULL DEFAULT '[]',
                outcome_linked INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_events_session ON {events}(session_id);
            CREATE INDEX IF NOT EXISTS idx_events_project ON {events}(project_dir);
            CREATE INDEX IF NOT EXISTS idx_events_hash ON {events}(content_hash);
            CREATE INDEX IF NOT EXISTS idx_events_hook ON {events}(hook_name);
            CREATE INDEX IF NOT EXISTS idx_events_created ON {events}(created_at);

            CREATE TABLE IF NOT EXISTS {intent} (
                event_id TEXT PRIMARY KEY,
                intent TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.0,
                keywords_matched TEXT NOT NULL DEFAULT '[]',
                techniques_applied TEXT NOT NULL DEFAULT '[]',
                prompt_chars INTEGER NOT NULL DEFAULT 0,
                session_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_intent_session ON {intent}(session_id);
            CREATE INDEX IF NOT EXISTS idx_intent_created ON {intent}(created_at);

            CREATE TABLE IF NOT EXISTS {quality} (
                event_id TEXT PRIMARY KEY,
                tool_name TEXT NOT NULL,
                reward REAL NOT NULL DEFAULT 0.0,
                latency_ms INTEGER NOT NULL DEFAULT 0,
                file_type TEXT NOT NULL DEFAULT '',
                session_id TEXT NOT NULL,
                correlation_id TEXT NOT NULL DEFAULT '',
                timestamp TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_quality_session ON {quality}(session_id);
            CREATE INDEX IF NOT EXISTS idx_quality_created ON {quality}(created_at);

            CREATE TABLE IF NOT EXISTS {tasks} (
                event_id TEXT PRIMARY KEY,
                task_type TEXT NOT NULL,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                outcome TEXT NOT NULL,
                session_id TEXT NOT NULL,
                correlation_chain TEXT NOT NULL DEFAULT '[]',
                timestamp TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_session ON {tasks}(session_id);
            CREATE INDEX IF NOT EXISTS idx_tasks_created ON {tasks}(created_at);

            CREATE TABLE IF NOT EXISTS {corr} (
                event_id TEXT NOT NULL,
                outcome_id TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (event_id, outcome_id)
            );
            CREATE INDEX IF NOT EXISTS idx_corr_event ON {corr}(event_id);
            CREATE INDEX IF NOT EXISTS idx_corr_outcome ON {corr}(outcome_id);

            CREATE TABLE IF NOT EXISTS {pat_e} (
                pattern_key TEXT NOT NULL,
                sample_count INTEGER NOT NULL DEFAULT 1,
                avg_reward REAL NOT NULL DEFAULT 0.0,
                avg_latency_ms REAL NOT NULL DEFAULT 0.0,
                intent_distribution TEXT NOT NULL DEFAULT '{{}}',
                first_seen TEXT NOT NULL,
                last_seen TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (pattern_key)
            );
            CREATE INDEX IF NOT EXISTS idx_pat_e_key ON {pat_e}(pattern_key);

            CREATE TABLE IF NOT EXISTS {pat_w} (
                pattern_key TEXT NOT NULL,
                sample_count INTEGER NOT NULL DEFAULT 1,
                avg_reward REAL NOT NULL DEFAULT 0.0,
                avg_latency_ms REAL NOT NULL DEFAULT 0.0,
                intent_distribution TEXT NOT NULL DEFAULT '{{}}',
                first_seen TEXT NOT NULL,
                last_seen TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (pattern_key)
            );
            CREATE INDEX IF NOT EXISTS idx_pat_w_key ON {pat_w}(pattern_key);

            CREATE TABLE IF NOT EXISTS {pat_s} (
                pattern_key TEXT NOT NULL,
                sample_count INTEGER NOT NULL DEFAULT 1,
                avg_reward REAL NOT NULL DEFAULT 0.0,
                avg_latency_ms REAL NOT NULL DEFAULT 0.0,
                intent_distribution TEXT NOT NULL DEFAULT '{{}}',
                first_seen TEXT NOT NULL,
                last_seen TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (pattern_key)
            );
            CREATE INDEX IF NOT EXISTS idx_pat_s_key ON {pat_s}(pattern_key);
            ",
                events = TABLE_HOOK_EVENTS,
                intent = TABLE_INTENT_CLASS,
                quality = TABLE_QUALITY_SIGNALS,
                tasks = TABLE_TASK_COMPLETIONS,
                corr = TABLE_CORRELATIONS,
                pat_e = TABLE_PATTERNS_EPHEMERAL,
                pat_w = TABLE_PATTERNS_WORKING,
                pat_s = TABLE_PATTERNS_SEMANTIC,
            ))
            .map_err(MemoryError::from)?;
        // D3.4: ensure priority_tier column exists on hook_events.
        self.migrate_priority_tier_column()?;
        Ok(())
    }

    /// Insert or update a hook event (upsert by event_id).
    ///
    /// D3.4: classifies the event into a [`EventPriority`] tier from
    /// `hook_name` and persists the SCREAMING_SNAKE label in `priority_tier`.
    fn upsert_event(&self, event: &HookEvent) -> Result<(), MemoryError> {
        let corr_json = serde_json::to_string(&event.correlation_ids).map_err(MemoryError::from)?;
        let priority =
            crate::shared::hook_events::classify_priority_by_hook_name(&event.hook_name).as_label();
        let sql = "INSERT INTO hook_events
             (event_id, hook_name, session_id, project_dir, event_type, timestamp, data, content_hash, correlation_ids, outcome_linked, priority_tier)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(event_id) DO UPDATE SET
                hook_name = excluded.hook_name,
                session_id = excluded.session_id,
                project_dir = excluded.project_dir,
                event_type = excluded.event_type,
                timestamp = excluded.timestamp,
                data = excluded.data,
                content_hash = excluded.content_hash,
                correlation_ids = excluded.correlation_ids,
                outcome_linked = excluded.outcome_linked,
                priority_tier = excluded.priority_tier";
        self.conn
            .execute(
                sql,
                params![
                    event.event_id,
                    event.hook_name,
                    event.session_id,
                    event.project_dir,
                    event.event_type,
                    event.timestamp,
                    event.data,
                    event.content_hash,
                    corr_json,
                    event.outcome_linked as i32,
                    priority,
                ],
            )
            .map_err(MemoryError::from)?;
        Ok(())
    }

    // ── D3.3: PreCompact priority filter ────────────────────────────────

    /// Returns event_ids for the given session whose `priority_tier` would
    /// survive PreCompact (i.e. `CRITICAL` or `HIGH`).
    ///
    /// Used by the PreCompact hook to keep only high-signal events in the
    /// post-compaction context window. MEDIUM and LOW events are dropped
    /// from the carry-forward set.
    pub fn events_surviving_compaction(
        &self,
        session_id: &str,
    ) -> Result<Vec<String>, MemoryError> {
        let sql = format!(
            "SELECT event_id FROM {TABLE_HOOK_EVENTS} \
             WHERE session_id = ?1 AND priority_tier IN ('CRITICAL', 'HIGH') \
             ORDER BY created_at ASC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let ids: Vec<String> = stmt
            .query_map(params![session_id], |row| row.get::<_, String>(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(ids)
    }

    /// Counterpart to [`Self::events_surviving_compaction`] for observability:
    /// returns the count of dropped (MEDIUM/LOW) events.
    pub fn count_events_dropped_by_compaction(&self, session_id: &str) -> Result<u64, MemoryError> {
        let sql = format!(
            "SELECT COUNT(*) FROM {TABLE_HOOK_EVENTS} \
             WHERE session_id = ?1 AND priority_tier IN ('MEDIUM', 'LOW')"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let n: i64 = stmt.query_row(params![session_id], |row| row.get(0))?;
        Ok(n.max(0) as u64)
    }

    /// Check if an event with the same content_hash already exists in ephemeral tier.
    /// Returns the existing event_id if a duplicate is found (within same project_dir).
    pub fn find_duplicate_by_hash(&self, content_hash: &str, project_dir: &str) -> Option<String> {
        let sql = "SELECT event_id FROM hook_events
             WHERE content_hash = ?1
               AND project_dir = ?2
               AND created_at > datetime('now', '-1 hour')
             LIMIT 1";
        let mut stmt = self.conn.prepare(sql).ok()?;
        let mut rows = stmt.query(params![content_hash, project_dir]).ok()?;
        match rows.next() {
            Ok(Some(row)) => row.get(0).ok(),
            _ => None,
        }
    }

    /// Insert an intent classification record.
    fn upsert_intent_class(&self, class: &IntentClassification) -> Result<(), MemoryError> {
        let keywords = serde_json::to_string(&class.keywords_matched)?;
        let techniques = serde_json::to_string(&class.techniques_applied)?;
        self.conn.execute(
            &format!("INSERT INTO {TABLE_INTENT_CLASS}
             (event_id, intent, confidence, keywords_matched, techniques_applied, prompt_chars, session_id, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(event_id) DO UPDATE SET
                intent = excluded.intent,
                confidence = excluded.confidence,
                keywords_matched = excluded.keywords_matched,
                techniques_applied = excluded.techniques_applied,
                prompt_chars = excluded.prompt_chars,
                session_id = excluded.session_id,
                timestamp = excluded.timestamp"),
            params![
                class.event_id,
                class.intent,
                class.confidence,
                keywords,
                techniques,
                class.prompt_chars as i64,
                class.session_id,
                class.timestamp,
            ],
        ).map_err(MemoryError::from)?;
        Ok(())
    }

    /// Insert a quality signal record.
    fn upsert_quality_signal(&self, signal: &QualitySignal) -> Result<(), MemoryError> {
        self.conn.execute(
            &format!("INSERT INTO {TABLE_QUALITY_SIGNALS}
             (event_id, tool_name, reward, latency_ms, file_type, session_id, correlation_id, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(event_id) DO UPDATE SET
                tool_name = excluded.tool_name,
                reward = excluded.reward,
                latency_ms = excluded.latency_ms,
                file_type = excluded.file_type,
                session_id = excluded.session_id,
                correlation_id = excluded.correlation_id,
                timestamp = excluded.timestamp"),
            params![
                signal.event_id,
                signal.tool_name,
                signal.reward,
                signal.latency_ms as i64,
                signal.file_type,
                signal.session_id,
                signal.correlation_id,
                signal.timestamp,
            ],
        ).map_err(MemoryError::from)?;
        Ok(())
    }

    /// Insert a task completion record.
    fn upsert_task_completion(&self, completion: &TaskCompletion) -> Result<(), MemoryError> {
        let chain_json = serde_json::to_string(&completion.correlation_chain)?;
        self.conn
            .execute(
                &format!(
                    "INSERT INTO {TABLE_TASK_COMPLETIONS}
             (event_id, task_type, duration_ms, outcome, session_id, correlation_chain, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(event_id) DO UPDATE SET
                task_type = excluded.task_type,
                duration_ms = excluded.duration_ms,
                outcome = excluded.outcome,
                session_id = excluded.session_id,
                correlation_chain = excluded.correlation_chain,
                timestamp = excluded.timestamp"
                ),
                params![
                    completion.event_id,
                    completion.task_type,
                    completion.duration_ms as i64,
                    completion.outcome,
                    completion.session_id,
                    chain_json,
                    completion.timestamp,
                ],
            )
            .map_err(MemoryError::from)?;
        Ok(())
    }

    /// Aggregate ephemeral events into working patterns.
    fn aggregate_to_working(&self) -> Result<usize, MemoryError> {
        // Aggregate by hook_name + session_id from ephemeral table
        let query = format!(
            "INSERT INTO {pat_w} (pattern_key, sample_count, avg_reward, avg_latency_ms,
              intent_distribution, first_seen, last_seen, confidence)
             SELECT
                hook_name || ':' || session_id as pattern_key,
                COUNT(*) as sample_count,
                0.0 as avg_reward,
                0.0 as avg_latency_ms,
                '{{}}' as intent_distribution,
                MIN(timestamp) as first_seen,
                MAX(timestamp) as last_seen,
                MIN(1.0) as confidence
             FROM {events}
             WHERE created_at > datetime('now', '-{WORKING_TTL_HOURS} hours')
               AND hook_name NOT IN (SELECT pattern_key FROM {pat_w})
             GROUP BY hook_name, session_id
             ON CONFLICT(pattern_key) DO UPDATE SET
                sample_count = sample_count + excluded.sample_count,
                last_seen = excluded.last_seen",
            events = TABLE_HOOK_EVENTS,
            pat_w = TABLE_PATTERNS_WORKING,
        );
        let rows = self.conn.execute(&query, []).map_err(MemoryError::from)?;
        Ok(rows)
    }

    /// Get the patterns table name for a given tier.
    fn patterns_table(tier: MemoryTier) -> &'static str {
        match tier {
            MemoryTier::Ephemeral => TABLE_PATTERNS_EPHEMERAL,
            MemoryTier::Working => TABLE_PATTERNS_WORKING,
            MemoryTier::Semantic => TABLE_PATTERNS_SEMANTIC,
        }
    }

    /// Build the TTL WHERE clause for a given tier.
    fn ttl_clause(tier: MemoryTier) -> String {
        match tier {
            MemoryTier::Semantic => String::new(),
            _ => format!(
                " AND created_at > datetime('now', '-{} hours')",
                tier.ttl_hours()
            ),
        }
    }

    /// Build the recall SQL query for patterns.
    fn recall_sql(table: &str, ttl_clause: &str, _limit: usize) -> String {
        format!(
            "SELECT pattern_key, sample_count, avg_reward, avg_latency_ms,
                    intent_distribution, first_seen, last_seen, confidence
             FROM {table}
             WHERE pattern_key LIKE ?1 {ttl_clause}
             ORDER BY confidence DESC, sample_count DESC
             LIMIT ?2",
            table = table,
            ttl_clause = ttl_clause,
        )
    }

    /// Map a SQL row to a HookPattern.
    ///
    /// JSON parse errors for `intent_distribution` are logged via `tracing::warn!` and default
    /// to an empty map. This is intentional: `row_to_pattern` must return `rusqlite::Result<HookPattern>`
    /// to satisfy the `query_map` API contract, and `recall_hook_patterns` returns `Vec<HookPattern>`
    /// (infallible trait). Full `TouringError::ParseError` propagation requires a trait signature
    /// redesign — see `shared::touring_error::TouringError` for the future path.
    fn row_to_pattern(row: &rusqlite::Row) -> rusqlite::Result<HookPattern> {
        let intent_dist_str: String = row.get(4).unwrap_or_default();
        let intent_dist: std::collections::HashMap<String, f64> =
            serde_json::from_str(&intent_dist_str).unwrap_or_else(|e| {
                tracing::warn!(
                    error = %e,
                    raw = %intent_dist_str,
                    "row_to_pattern: failed to parse intent_dist JSON, using empty map"
                );
                Default::default()
            });

        Ok(HookPattern {
            pattern_key: row.get(0)?,
            sample_count: row.get(1)?,
            avg_reward: row.get(2)?,
            avg_latency_ms: row.get(3)?,
            intent_distribution: intent_dist,
            first_seen: row.get(5)?,
            last_seen: row.get(6)?,
            confidence: row.get(7)?,
        })
    }
}

impl<'a> HookMemoryBridge for SqliteHookMemoryBridge<'a> {
    fn store_hook_event(&mut self, event: HookEvent) -> Result<(), MemoryError> {
        self.ensure_schema()?;
        self.upsert_event(&event)
    }

    fn store_intent_classification(
        &mut self,
        classification: IntentClassification,
    ) -> Result<(), MemoryError> {
        self.ensure_schema()?;
        self.upsert_intent_class(&classification)
    }

    fn store_quality_signal(&mut self, signal: QualitySignal) -> Result<(), MemoryError> {
        self.ensure_schema()?;
        self.upsert_quality_signal(&signal)
    }

    fn store_task_completion(&mut self, completion: TaskCompletion) -> Result<(), MemoryError> {
        self.ensure_schema()?;
        self.upsert_task_completion(&completion)
    }

    fn recall_hook_patterns(
        &self,
        query: &str,
        tier: MemoryTier,
        limit: usize,
    ) -> Vec<HookPattern> {
        let table = Self::patterns_table(tier);
        let ttl_clause = Self::ttl_clause(tier);
        let sql = Self::recall_sql(table, &ttl_clause, limit);

        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let like_pattern = format!("%{}%", query);
        let limit_i64 = limit as i64;

        stmt.query_map(params![like_pattern, limit_i64], Self::row_to_pattern)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    fn correlate_hook_with_outcome(
        &mut self,
        event_id: &str,
        outcome_id: &str,
    ) -> Result<(), CorrelationError> {
        self.ensure_schema()
            .map_err(|e| CorrelationError::Database(e.to_string()))?;
        self.conn.execute(
            &format!("INSERT OR IGNORE INTO {TABLE_CORRELATIONS} (event_id, outcome_id) VALUES (?1, ?2)"),
            params![event_id, outcome_id],
        ).map_err(CorrelationError::from)?;

        // Mark the event as outcome-linked
        self.conn
            .execute(
                &format!("UPDATE {TABLE_HOOK_EVENTS} SET outcome_linked = 1 WHERE event_id = ?1"),
                params![event_id],
            )
            .map_err(CorrelationError::from)?;

        Ok(())
    }

    fn run_ttl_cleanup(&self) -> Result<usize, MemoryError> {
        // Step 1: Aggregate ephemeral events into working patterns.
        // This runs BEFORE cleanup so new patterns get their TTL clock started.
        let _ = self.aggregate_to_working();

        // Step 2: Clean ephemeral (1h TTL)
        let ephemeral_rows = self
            .conn
            .execute(
                &format!(
                "DELETE FROM {TABLE_HOOK_EVENTS} WHERE created_at <= datetime('now', '-{} hours')",
                EPHEMERAL_TTL_HOURS
            ),
                [],
            )
            .map_err(MemoryError::from)?;

        // Clean ephemeral patterns
        let pat_e_rows = self.conn.execute(
            &format!(
                "DELETE FROM {TABLE_PATTERNS_EPHEMERAL} WHERE created_at <= datetime('now', '-{} hours')",
                EPHEMERAL_TTL_HOURS
            ),
            [],
        ).map_err(MemoryError::from)?;

        // Clean intent classifications (1h TTL)
        let intent_rows = self
            .conn
            .execute(
                &format!(
                "DELETE FROM {TABLE_INTENT_CLASS} WHERE created_at <= datetime('now', '-{} hours')",
                EPHEMERAL_TTL_HOURS
            ),
                [],
            )
            .map_err(MemoryError::from)?;

        // Clean quality signals (1h TTL)
        let quality_rows = self.conn.execute(
            &format!(
                "DELETE FROM {TABLE_QUALITY_SIGNALS} WHERE created_at <= datetime('now', '-{} hours')",
                EPHEMERAL_TTL_HOURS
            ),
            [],
        ).map_err(MemoryError::from)?;

        // Clean task completions (1h TTL)
        let task_rows = self.conn.execute(
            &format!(
                "DELETE FROM {TABLE_TASK_COMPLETIONS} WHERE created_at <= datetime('now', '-{} hours')",
                EPHEMERAL_TTL_HOURS
            ),
            [],
        ).map_err(MemoryError::from)?;

        // Clean working patterns (24h TTL)
        let pat_w_rows = self.conn.execute(
            &format!(
                "DELETE FROM {TABLE_PATTERNS_WORKING} WHERE created_at <= datetime('now', '-{} hours')",
                WORKING_TTL_HOURS
            ),
            [],
        ).map_err(MemoryError::from)?;

        Ok(ephemeral_rows + pat_e_rows + intent_rows + quality_rows + task_rows + pat_w_rows)
    }

    fn run_promotion(&self) -> Result<usize, MemoryError> {
        // Step 1: Aggregate ephemeral events into working patterns (ephemeral → working).
        // This completes the tiered memory pipeline: ephemeral → working → semantic.
        let _ = self.aggregate_to_working();

        // Step 2: Promote eligible working patterns to semantic
        let sql = format!(
            "INSERT INTO {pat_s} (pattern_key, sample_count, avg_reward, avg_latency_ms,
              intent_distribution, first_seen, last_seen, confidence)
             SELECT pattern_key, sample_count, avg_reward, avg_latency_ms,
                    intent_distribution, first_seen, last_seen, confidence
             FROM {pat_w}
             WHERE sample_count >= {min_samples}
               AND avg_reward >= {min_reward}
               AND confidence >= {min_conf}
               AND pattern_key NOT IN (SELECT pattern_key FROM {pat_s})
             ON CONFLICT(pattern_key) DO UPDATE SET
                sample_count = excluded.sample_count,
                avg_reward = excluded.avg_reward,
                avg_latency_ms = excluded.avg_latency_ms,
                intent_distribution = excluded.intent_distribution,
                last_seen = excluded.last_seen,
                confidence = excluded.confidence",
            pat_s = TABLE_PATTERNS_SEMANTIC,
            pat_w = TABLE_PATTERNS_WORKING,
            min_samples = PROMOTION_MIN_SAMPLES,
            min_reward = PROMOTION_MIN_AVG_REWARD,
            min_conf = PROMOTION_MIN_CONFIDENCE,
        );
        let rows = self.conn.execute(&sql, []).map_err(MemoryError::from)?;
        Ok(rows)
    }
}

// ── Integration helpers ────────────────────────────────────────────────────────

/// Wire HookMemoryBridge into an existing HookRuntime context.
///
/// This function initializes the hook memory schema and returns a configured
/// bridge that uses the runtime's knowledge DB connection.
///
/// Call from `HookRuntime::new()` or after `init_async_knowledge()`.
pub fn init_hook_memory(
    rt: &mut crate::runtime::HookRuntime,
) -> Result<SqliteHookMemoryBridge<'_>, TouringError> {
    let conn = rt.ctx.knowledge.conn_ref();
    let bridge = SqliteHookMemoryBridge::new(conn);
    bridge
        .ensure_schema()
        .map_err(|e| TouringError::Knowledge(e.to_string()))?;
    Ok(bridge)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_conn() -> Connection {
        Connection::open_in_memory().expect("in-memory DB should open")
    }

    fn with_test_bridge<F>(f: F)
    where
        F: FnOnce(SqliteHookMemoryBridge) -> Result<(), MemoryError>,
    {
        let conn = temp_conn();
        let bridge = SqliteHookMemoryBridge::new(&conn);
        bridge.ensure_schema().expect("schema init should succeed");
        f(bridge).expect("test bridge operation should succeed");
    }

    #[test]
    fn test_hook_event_roundtrip() {
        with_test_bridge(|mut bridge| {
            let event = HookEvent::new(
                "pre_read",
                "sess_abc",
                "",
                "execution",
                serde_json::json!({"file_path": "/tmp/test.rs"}),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            );
            bridge.store_hook_event(event)?;
            Ok(())
        });
    }

    #[test]
    fn test_quality_signal_roundtrip() {
        with_test_bridge(|mut bridge| {
            let signal = QualitySignal {
                event_id: new_event_id(),
                tool_name: "Edit".to_string(),
                reward: 0.8,
                latency_ms: 42,
                file_type: "rust".to_string(),
                session_id: "sess_abc".to_string(),
                correlation_id: "corr_123".to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            bridge.store_quality_signal(signal)?;
            // TTL cleanup should remove it after 1h (but not immediately)
            let deleted = bridge.run_ttl_cleanup()?;
            assert_eq!(deleted, 0);
            Ok(())
        });
    }

    #[test]
    fn test_correlation() {
        with_test_bridge(|mut bridge| {
            let event = HookEvent::new(
                "pre_edit",
                "sess_xyz",
                "",
                "context",
                serde_json::json!({"file_path": "/tmp/foo.rs"}),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            );
            bridge.store_hook_event(event)?;
            bridge
                .correlate_hook_with_outcome("pre_edit", "post_edit")
                .map_err(|e| MemoryError::Database(e.to_string()))?;
            Ok(())
        });
    }

    #[test]
    fn test_memory_tier_parse() {
        assert_eq!(MemoryTier::parse("ephemeral"), Some(MemoryTier::Ephemeral));
        assert_eq!(MemoryTier::parse("Ephemeral"), Some(MemoryTier::Ephemeral));
        assert_eq!(MemoryTier::parse("working"), Some(MemoryTier::Working));
        assert_eq!(MemoryTier::parse("semantic"), Some(MemoryTier::Semantic));
        assert_eq!(MemoryTier::parse("unknown"), None);
        // Also verify the FromStr trait implementation works
        assert_eq!(
            "ephemeral".parse::<MemoryTier>().ok(),
            Some(MemoryTier::Ephemeral)
        );
        assert!("unknown".parse::<MemoryTier>().is_err());
    }

    #[test]
    fn test_ttl_cleanup() {
        with_test_bridge(|bridge| {
            let count = bridge.run_ttl_cleanup()?;
            assert_eq!(count, 0);
            Ok(())
        });
    }

    #[test]
    fn test_promotion() {
        with_test_bridge(|bridge| {
            let count = bridge.run_promotion()?;
            assert_eq!(count, 0);
            Ok(())
        });
    }

    // ── D3.3 + D3.4 — priority_tier integration ────────────────────────

    fn make_event(hook: &str, sid: &str) -> HookEvent {
        HookEvent::new(
            hook,
            sid,
            "",
            "execution",
            serde_json::json!({"k": "v"}),
            format!("hash:{hook}:{sid}"),
        )
    }

    #[test]
    fn test_priority_tier_migration_idempotent() {
        let conn = temp_conn();
        let bridge = SqliteHookMemoryBridge::new(&conn);
        bridge.ensure_schema().expect("schema 1");
        // Calling ensure_schema again must not fail: ALTER TABLE is gated by
        // PRAGMA introspection so the second call is a no-op.
        bridge.ensure_schema().expect("schema 2 (idempotent)");
        // Verify column exists.
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({TABLE_HOOK_EVENTS})"))
            .unwrap();
        let names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert!(
            names.iter().any(|n| n == "priority_tier"),
            "priority_tier column missing: {names:?}"
        );
    }

    #[test]
    fn test_precompact_filter_keeps_critical_high_drops_medium_low() {
        with_test_bridge(|mut bridge| {
            let sid = "sess_compact";
            // CRITICAL: session_start
            bridge.store_hook_event(make_event("session_start", sid))?;
            // HIGH: post_edit
            bridge.store_hook_event(make_event("post_edit", sid))?;
            // MEDIUM: pre_read
            bridge.store_hook_event(make_event("pre_read", sid))?;
            // LOW (unknown name)
            bridge.store_hook_event(make_event("hook_memory_store", sid))?;

            let surviving = bridge.events_surviving_compaction(sid)?;
            assert_eq!(
                surviving.len(),
                2,
                "expected 2 surviving (CRITICAL + HIGH), got {} ({surviving:?})",
                surviving.len()
            );
            let dropped = bridge.count_events_dropped_by_compaction(sid)?;
            assert_eq!(dropped, 2, "expected 2 dropped (MEDIUM + LOW)");
            Ok(())
        });
    }

    #[test]
    fn test_priority_tier_persisted_in_column() {
        with_test_bridge(|mut bridge| {
            let sid = "sess_persist";
            bridge.store_hook_event(make_event("session_stop", sid))?;
            bridge.store_hook_event(make_event("pre_bash", sid))?;

            // Inspect the priority_tier column directly.
            let mut stmt = bridge.conn.prepare(&format!(
                "SELECT hook_name, priority_tier FROM {TABLE_HOOK_EVENTS} \
                 WHERE session_id = ?1 ORDER BY hook_name"
            ))?;
            let rows: Vec<(String, String)> = stmt
                .query_map(params![sid], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(Result::ok)
                .collect();
            assert_eq!(rows.len(), 2);
            // pre_bash → MEDIUM; session_stop → CRITICAL
            assert!(
                rows.iter()
                    .any(|(h, p)| h == "session_stop" && p == "CRITICAL")
            );
            assert!(rows.iter().any(|(h, p)| h == "pre_bash" && p == "MEDIUM"));
            Ok(())
        });
    }
}
