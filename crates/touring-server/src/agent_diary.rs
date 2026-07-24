//! Agent Diary System — Wing + diary hierarchy for agent memory persistence
//!
//! Provides structured diary storage with AAAK compressed entry support.
//! Organized under `wing_{agent_name}/diary/` key hierarchy in the memory store.
//!
//! # Key Hierarchy
//!
//! - `wing_{agent}/diary/meta` — Agent metadata (created_at, entry_count)
//! - `wing_{agent}/diary/entries/{timestamp}` — Individual diary entries
//! - `wing_{agent}/diary/topics/{topic}` — Topic index for entry lookups
//!
//! # AAAK Dialect (Adaptive Abbreviated Agent Knowledge)
//!
//! Compressed entry format for high-frequency logging:
//! - `#T` = timestamp shorthand
//! - `#P` = phase marker
//! - `#R` = result/score
//! - `#L` = lesson learned
//! - `#W` = warning
//! - `#E` = error

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use crate::memory_store::MemoryStore;

/// Errors for diary operations
#[derive(Error, Debug)]
pub enum DiaryError {
    /// No diary exists for the named agent.
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    /// The requested diary entry does not exist.
    #[error("Entry not found: {0}")]
    EntryNotFound(String),
    /// The supplied diary topic is invalid.
    #[error("Invalid topic: {0}")]
    InvalidTopic(String),
    /// A diary entry failed to serialize or deserialize.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    /// The backing memory store reported an error.
    #[error("Memory store error: {0}")]
    MemoryStore(String),
}

impl From<super::memory_store::MemoryStoreError> for DiaryError {
    fn from(e: super::memory_store::MemoryStoreError) -> Self {
        DiaryError::MemoryStore(e.to_string())
    }
}

/// Result type for diary operations
pub type Result<T> = std::result::Result<T, DiaryError>;

/// AAAK dialect markers for compressed entries
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AaakMarker {
    /// Timestamp shorthand — expanded to ISO 8601
    Timestamp,
    /// Phase marker (planning, execution, validation, etc.)
    Phase,
    /// Result or score (numeric or categorical)
    Result,
    /// Lesson learned — key insight from experience
    Lesson,
    /// Warning — potential issue or anti-pattern detected
    Warning,
    /// Error — failure or exception encountered
    Error,
}

impl AaakMarker {
    /// Parse a AAAK marker from a tag string
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag.to_uppercase().as_str() {
            "T" | "TIME" | "TIMESTAMP" => Some(Self::Timestamp),
            "P" | "PHASE" => Some(Self::Phase),
            "R" | "RESULT" | "SCORE" => Some(Self::Result),
            "L" | "LESSON" => Some(Self::Lesson),
            "W" | "WARN" | "WARNING" => Some(Self::Warning),
            "E" | "ERR" | "ERROR" => Some(Self::Error),
            _ => None,
        }
    }

    /// Get the marker tag string
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Timestamp => "T",
            Self::Phase => "P",
            Self::Result => "R",
            Self::Lesson => "L",
            Self::Warning => "W",
            Self::Error => "E",
        }
    }
}

/// Compressed AAAK entry format
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AaakEntry {
    markers: Vec<(AaakMarker, String)>,
    raw_content: String,
}

impl AaakEntry {
    /// Parse an AAAK formatted string into structured markers
    pub fn parse(content: &str) -> Self {
        let mut markers = Vec::new();
        let mut raw_content = content.to_string();

        // Pattern: #[TPLWER] value or #[T:value]
        let re = regex::Regex::new(r"#\[([TPLWER])(?::([^]]+))?\]")
            .expect("AAAK marker regex is a valid literal");

        for cap in re.captures_iter(content) {
            let tag = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let value = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let full_match = cap.get(0).map(|m| m.as_str()).unwrap_or("");

            if let Some(marker) = AaakMarker::from_tag(tag) {
                markers.push((marker, value.to_string()));
                raw_content = raw_content.replace(full_match, "");
            }
        }

        // Clean up extra whitespace
        raw_content = raw_content.split_whitespace().collect::<Vec<_>>().join(" ");

        Self {
            markers,
            raw_content,
        }
    }

    /// Expand to full structured entry
    pub fn expand(&self) -> StructuredEntry {
        let mut expanded = HashMap::new();

        for (marker, value) in &self.markers {
            let key = match marker {
                AaakMarker::Timestamp => "timestamp",
                AaakMarker::Phase => "phase",
                AaakMarker::Result => "result",
                AaakMarker::Lesson => "lesson",
                AaakMarker::Warning => "warning",
                AaakMarker::Error => "error",
            };
            expanded.insert(key.to_string(), value.clone());
        }

        StructuredEntry {
            timestamp: Utc::now(),
            content: self.raw_content.clone(),
            markers: expanded,
            is_aaak: true,
        }
    }
}

/// Structured diary entry (expanded form)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredEntry {
    /// When the entry was created.
    pub timestamp: DateTime<Utc>,
    /// Free-text body of the diary entry.
    pub content: String,
    /// AAAK markers keyed by their tag (e.g. `P`, `R`, `L`).
    pub markers: HashMap<String, String>,
    /// Whether the entry is written in the AAAK dialect.
    pub is_aaak: bool,
}

impl StructuredEntry {
    /// Create a new structured entry
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            content: content.into(),
            markers: HashMap::new(),
            is_aaak: false,
        }
    }

    /// Add a marker to the entry
    pub fn with_marker(mut self, marker: AaakMarker, value: impl Into<String>) -> Self {
        self.markers.insert(marker.tag().to_string(), value.into());
        self
    }

    /// Add a phase marker
    pub fn with_phase(mut self, phase: impl Into<String>) -> Self {
        self.markers.insert("P".to_string(), phase.into());
        self
    }

    /// Add a result/score marker
    pub fn with_result(mut self, result: impl Into<String>) -> Self {
        self.markers.insert("R".to_string(), result.into());
        self
    }

    /// Add a lesson marker
    pub fn with_lesson(mut self, lesson: impl Into<String>) -> Self {
        self.markers.insert("L".to_string(), lesson.into());
        self
    }

    /// Check if entry contains a specific marker
    pub fn has_marker(&self, marker: AaakMarker) -> bool {
        self.markers.contains_key(marker.tag())
    }

    /// Get marker value
    pub fn get_marker(&self, marker: AaakMarker) -> Option<&String> {
        self.markers.get(marker.tag())
    }
}

/// Agent metadata stored in diary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMeta {
    /// Name of the agent that owns the diary.
    pub agent_name: String,
    /// When the agent's diary was first created.
    pub created_at: DateTime<Utc>,
    /// Total number of entries written by the agent.
    pub entry_count: usize,
    /// Timestamp of the most recent entry, if any.
    pub last_entry_at: Option<DateTime<Utc>>,
}

impl AgentMeta {
    /// Create new agent metadata
    pub fn new(agent_name: impl Into<String>) -> Self {
        Self {
            agent_name: agent_name.into(),
            created_at: Utc::now(),
            entry_count: 0,
            last_entry_at: None,
        }
    }

    /// Increment entry count
    pub fn increment(&mut self) {
        self.entry_count += 1;
        self.last_entry_at = Some(Utc::now());
    }
}

/// AgentDiary — Main diary structure for agent memory persistence
///
/// # Hierarchy
///
/// - `wing_/diary/meta` - AgentMeta (JSON)
/// - `wing_/diary/entries/<timestamp>` - StructuredEntry (JSON)
/// - `wing_/diary/topics/<topic>` - `Vec<timestamp>` (topic index)
#[derive(Debug, Clone)]
pub struct AgentDiary {
    agent_name: String,
    /// In-memory cache of metadata
    meta: Option<AgentMeta>,
}

impl AgentDiary {
    /// Create a new agent diary
    pub fn new(agent_name: impl Into<String>) -> Self {
        Self {
            agent_name: agent_name.into(),
            meta: None,
        }
    }

    /// Get the key prefix for this agent's diary
    fn wing_prefix(&self) -> String {
        format!("wing_{}", self.agent_name)
    }

    /// Get the meta key
    fn meta_key(&self) -> String {
        format!("{}/diary/meta", self.wing_prefix())
    }

    /// Get the entries prefix
    fn entries_prefix(&self) -> String {
        format!("{}/diary/entries", self.wing_prefix())
    }

    /// Get the topics prefix
    fn topics_prefix(&self) -> String {
        format!("{}/diary/topics", self.wing_prefix())
    }

    /// Get entry key for a specific timestamp
    fn entry_key(&self, timestamp: &DateTime<Utc>) -> String {
        format!("{}/{}", self.entries_prefix(), timestamp.to_rfc3339())
    }

    /// Get topic key
    fn topic_key(&self, topic: &str) -> String {
        format!("{}/{}", self.topics_prefix(), topic)
    }

    /// Load metadata from memory store
    pub fn load_meta(&mut self, memory_store: &MemoryStore) -> Result<()> {
        let meta_key = self.meta_key();
        if let Some(json) = memory_store
            .get(&meta_key, "working")
            .map_err(|e| DiaryError::MemoryStore(e.to_string()))?
        {
            let meta: AgentMeta = serde_json::from_str(&json).map_err(DiaryError::Serialization)?;
            self.meta = Some(meta);
        }
        Ok(())
    }

    /// Save metadata to memory store
    fn save_meta(&self, memory_store: &MemoryStore) -> Result<()> {
        let meta = self
            .meta
            .as_ref()
            .ok_or_else(|| DiaryError::AgentNotFound(self.agent_name.clone()))?;
        let json = serde_json::to_string(meta).map_err(DiaryError::Serialization)?;
        let entry = crate::memory_store::MemoryEntry::new(self.meta_key(), "working", json);
        memory_store
            .store(entry)
            .map_err(|e| DiaryError::MemoryStore(e.to_string()))?;
        Ok(())
    }

    /// Initialize agent meta if not exists
    pub fn ensure_meta(&mut self, memory_store: &MemoryStore) -> Result<()> {
        if self.meta.is_none() {
            // Try to load existing
            if self.load_meta(memory_store).is_err() {
                // Create new
                self.meta = Some(AgentMeta::new(self.agent_name.clone()));
                self.save_meta(memory_store)?;
            }
        }
        Ok(())
    }

    /// diary_write — Store a diary entry for an agent
    ///
    /// # Arguments
    /// * `agent_name` — Name of the agent
    /// * `entry` — Content to store (supports AAAK compressed format)
    /// * `topic` — Topic/category for the entry (optional)
    ///
    /// # AAAK Format
    /// Entries starting with `#AAK` are parsed as compressed:
    /// `//AAAKe #[T] #[P:phase] #[R:0.85] #[L:lesson text]`
    /// Note: Use `//AAAKe` prefix in docs to avoid rustdoc parsing issues
    pub fn diary_write(
        &mut self,
        memory_store: &MemoryStore,
        entry: &str,
        topic: Option<&str>,
    ) -> Result<DateTime<Utc>> {
        // Ensure meta exists
        self.ensure_meta(memory_store)?;

        let timestamp = Utc::now();
        let is_aaak = entry.starts_with("#AAK");

        // Parse or create structured entry
        let structured = if is_aaak {
            // Strip #AAK prefix and parse AAAK
            let content = entry.trim_start_matches("#AAK").trim();
            AaakEntry::parse(content).expand()
        } else {
            StructuredEntry::new(entry)
        };

        // Serialize entry
        let entry_json = serde_json::to_string(&structured).map_err(DiaryError::Serialization)?;

        // Store entry
        let entry_key = self.entry_key(&timestamp);
        let mem_entry = crate::memory_store::MemoryEntry::new(entry_key, "working", entry_json);
        memory_store
            .store(mem_entry)
            .map_err(|e| DiaryError::MemoryStore(e.to_string()))?;

        // Update topic index
        if let Some(topic_name) = topic {
            let topic_key = self.topic_key(topic_name);
            let timestamps_json = memory_store
                .get(&topic_key, "working")
                .map_err(|e| DiaryError::MemoryStore(e.to_string()))?;
            let mut timestamps: Vec<DateTime<Utc>> = if let Some(json) = timestamps_json {
                serde_json::from_str(&json).unwrap_or_default()
            } else {
                Vec::new()
            };
            timestamps.push(timestamp);
            let ts_json = serde_json::to_string(&timestamps).map_err(DiaryError::Serialization)?;
            let topic_entry = crate::memory_store::MemoryEntry::new(topic_key, "working", ts_json);
            memory_store
                .store(topic_entry)
                .map_err(|e| DiaryError::MemoryStore(e.to_string()))?;
        }

        // Update meta
        if let Some(meta) = &mut self.meta {
            meta.increment();
            self.save_meta(memory_store)?;
        }

        Ok(timestamp)
    }

    /// diary_read — Retrieve diary entries for an agent
    ///
    /// # Arguments
    /// * `agent_name` — Name of the agent
    /// * `last_n` — Number of recent entries to retrieve (None = all)
    pub fn diary_read(
        &self,
        memory_store: &MemoryStore,
        last_n: Option<usize>,
    ) -> Result<Vec<StructuredEntry>> {
        let entries_prefix = self.entries_prefix();

        // List all entries for this agent via a deterministic key-prefix scan.
        // The semantic `query()` path routes through the RLM FTS/embedding layer,
        // which is unavailable in the direct-file CLI context and fails with
        // "RLM error". A diary listing needs exact prefix membership, not
        // semantic ranking — `scan_prefix` is the correct plain-SQL path and is
        // exactly what `diary_read_by_topic`/`diary_read_by_project` already use. (A12)
        let limit = last_n.unwrap_or(100).max(100);
        let matches = memory_store
            .scan_prefix(&entries_prefix, "working", limit)
            .map_err(|e| DiaryError::MemoryStore(e.to_string()))?;

        let mut entries: Vec<StructuredEntry> = matches
            .into_iter()
            .filter(|m| m.key.starts_with(&entries_prefix))
            .filter_map(|m| serde_json::from_str(&m.value).ok())
            .collect();

        // Sort by timestamp descending
        entries.sort_by_key(|b| std::cmp::Reverse(b.timestamp));

        // Limit to last_n if specified
        if let Some(n) = last_n {
            entries.truncate(n);
        }

        Ok(entries)
    }

    /// Read entries by topic
    pub fn diary_read_by_topic(
        &self,
        memory_store: &MemoryStore,
        topic: &str,
    ) -> Result<Vec<StructuredEntry>> {
        let topic_key = self.topic_key(topic);

        let timestamps_json = memory_store
            .get(&topic_key, "working")?
            .ok_or_else(|| DiaryError::InvalidTopic(topic.to_string()))?;

        let timestamps: Vec<DateTime<Utc>> =
            serde_json::from_str(&timestamps_json).map_err(DiaryError::Serialization)?;

        let mut entries = Vec::new();
        for ts in timestamps {
            let entry_key = self.entry_key(&ts);
            if let Ok(Some(json)) = memory_store.get(&entry_key, "working") {
                if let Ok(entry) = serde_json::from_str::<StructuredEntry>(&json) {
                    entries.push(entry);
                }
            }
        }

        Ok(entries)
    }

    /// Get agent metadata
    pub fn get_meta(&self) -> Option<&AgentMeta> {
        self.meta.as_ref()
    }

    /// Get entry count
    pub fn entry_count(&self) -> usize {
        self.meta.as_ref().map(|m| m.entry_count).unwrap_or(0)
    }

    // ── W6: Project-scoped diary (2026-04-19) ──────────────────────────────

    /// Key prefix for project-scoped entries: `wing_{agent}/diary/projects/{project}/entries`
    fn project_entries_prefix(&self, project: &str) -> String {
        format!("{}/diary/projects/{}/entries", self.wing_prefix(), project)
    }

    /// Key for the project index: `wing_{agent}/diary/projects/_index`
    fn project_index_key(&self) -> String {
        format!("{}/diary/projects/_index", self.wing_prefix())
    }

    /// Write a diary entry scoped to a project and optional task/subtask.
    ///
    /// Delegates flat-hierarchy write (topic index + meta + AAAK parsing) to
    /// `diary_write`, then additionally stores the entry under the project prefix
    /// and updates the project index. Task/subtask IDs are embedded as markers.
    pub fn diary_write_project(
        &mut self,
        memory_store: &MemoryStore,
        content: &str,
        project: &str,
        task_id: Option<&str>,
        subtask_id: Option<&str>,
        topic: Option<&str>,
    ) -> Result<DateTime<Utc>> {
        // diary_write handles: ensure_meta, AAAK parsing, flat entry, topic index, meta update
        let timestamp = self.diary_write(memory_store, content, topic)?;

        // Store a project-tagged copy under the project prefix
        self.store_project_entry(
            memory_store,
            content,
            project,
            task_id,
            subtask_id,
            &timestamp,
        )?;

        // Register this project in the agent's project index
        self.update_project_index(memory_store, project)?;

        Ok(timestamp)
    }

    /// Write a single entry under the project prefix with task/subtask markers.
    fn store_project_entry(
        &self,
        memory_store: &MemoryStore,
        content: &str,
        project: &str,
        task_id: Option<&str>,
        subtask_id: Option<&str>,
        timestamp: &DateTime<Utc>,
    ) -> Result<()> {
        let is_aaak = content.starts_with("#AAK");
        let mut entry = if is_aaak {
            AaakEntry::parse(content.trim_start_matches("#AAK").trim()).expand()
        } else {
            StructuredEntry::new(content)
        };

        entry
            .markers
            .insert("project".to_string(), project.to_string());
        if let Some(tid) = task_id {
            entry.markers.insert("task".to_string(), tid.to_string());
        }
        if let Some(sid) = subtask_id {
            entry.markers.insert("subtask".to_string(), sid.to_string());
        }
        entry.timestamp = *timestamp;

        let json = serde_json::to_string(&entry).map_err(DiaryError::Serialization)?;
        let key = format!(
            "{}/{}",
            self.project_entries_prefix(project),
            timestamp.to_rfc3339()
        );
        let mem = crate::memory_store::MemoryEntry::new(key.clone(), "working", json);
        memory_store
            .store(mem)
            .map_err(|e| DiaryError::MemoryStore(e.to_string()))?;

        // ── FTS5 Ingestion ──────────────────────────────────────────────────
        // Store the entry content in SemanticRecall so `touring memory recall`
        // can find it via FTS5 text search. Metadata enables filtered queries.
        let recall = memory_store.recall();
        let fts_metadata = serde_json::json!({
            "key": format!("diary:{}:{}", self.agent_name, project),
            "agent": self.agent_name,
            "project": project,
            "task": task_id,
            "subtask": subtask_id,
            "timestamp": timestamp.to_rfc3339(),
            "entry_key": key,
        });
        let _ = recall.store_chunk(&entry.content, None, Some(&fts_metadata));
        // ── /FTS5 ───────────────────────────────────────────────────────────

        Ok(())
    }

    /// Update the project index with the given project name (idempotent).
    fn update_project_index(&self, memory_store: &MemoryStore, project: &str) -> Result<()> {
        let index_key = self.project_index_key();
        let existing_json = memory_store
            .get(&index_key, "working")
            .map_err(|e| DiaryError::MemoryStore(e.to_string()))?;
        let mut projects: Vec<String> = existing_json
            .as_deref()
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default();

        if !projects.iter().any(|p| p == project) {
            projects.push(project.to_string());
            let json = serde_json::to_string(&projects).map_err(DiaryError::Serialization)?;
            let mem = crate::memory_store::MemoryEntry::new(index_key, "working", json);
            memory_store
                .store(mem)
                .map_err(|e| DiaryError::MemoryStore(e.to_string()))?;
        }
        Ok(())
    }

    /// Read diary entries scoped to a specific project.
    ///
    /// Returns entries sorted by timestamp descending. Pass `last_n` to limit.
    pub fn diary_read_by_project(
        &self,
        memory_store: &MemoryStore,
        project: &str,
        last_n: Option<usize>,
    ) -> Result<Vec<StructuredEntry>> {
        let prefix = self.project_entries_prefix(project);

        // scan_prefix does SQL LIKE key-prefix lookup — no FTS, no false positives.
        let matches = memory_store
            .scan_prefix(&prefix, "working", last_n.unwrap_or(100).max(100))
            .map_err(|e| DiaryError::MemoryStore(e.to_string()))?;

        let mut entries: Vec<StructuredEntry> = matches
            .into_iter()
            .filter_map(|m| serde_json::from_str(&m.value).ok())
            .collect();

        entries.sort_by_key(|b| std::cmp::Reverse(b.timestamp));

        if let Some(n) = last_n {
            entries.truncate(n);
        }

        Ok(entries)
    }

    /// List all projects that have diary entries for this agent.
    pub fn diary_list_projects(&self, memory_store: &MemoryStore) -> Result<Vec<String>> {
        let index_key = self.project_index_key();
        if let Some(json) = memory_store
            .get(&index_key, "working")
            .map_err(|e| DiaryError::MemoryStore(e.to_string()))?
        {
            let projects: Vec<String> = serde_json::from_str(&json).unwrap_or_default();
            Ok(projects)
        } else {
            Ok(Vec::new())
        }
    }

    /// Read diary entries filtered by project and task_id.
    ///
    /// Returns all project entries where the `"task"` marker matches `task_id`.
    pub fn diary_read_by_task(
        &self,
        memory_store: &MemoryStore,
        project: &str,
        task_id: &str,
    ) -> Result<Vec<StructuredEntry>> {
        let all = self.diary_read_by_project(memory_store, project, None)?;
        Ok(all
            .into_iter()
            .filter(|e| e.markers.get("task").is_some_and(|t| t == task_id))
            .collect())
    }
}

/// Builder for creating AgentDiary instances
#[derive(Debug, Default)]
pub(crate) struct AgentDiaryBuilder {
    agent_name: Option<String>,
}

impl AgentDiaryBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the agent name
    pub fn agent_name(mut self, name: impl Into<String>) -> Self {
        self.agent_name = Some(name.into());
        self
    }

    /// Build the AgentDiary
    pub fn build(self) -> Result<AgentDiary> {
        let agent_name = self
            .agent_name
            .ok_or_else(|| DiaryError::InvalidTopic("agent_name required".to_string()))?;
        Ok(AgentDiary::new(agent_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aaak_marker_parsing() {
        assert_eq!(AaakMarker::from_tag("T"), Some(AaakMarker::Timestamp));
        assert_eq!(AaakMarker::from_tag("P"), Some(AaakMarker::Phase));
        assert_eq!(AaakMarker::from_tag("R"), Some(AaakMarker::Result));
        assert_eq!(AaakMarker::from_tag("L"), Some(AaakMarker::Lesson));
        assert_eq!(AaakMarker::from_tag("W"), Some(AaakMarker::Warning));
        assert_eq!(AaakMarker::from_tag("E"), Some(AaakMarker::Error));
        assert_eq!(AaakMarker::from_tag("X"), None);
    }

    #[test]
    fn test_aaak_entry_parse() {
        let input = "#[T:2024-01-01T10:00:00Z] #[P:planning] #[R:0.85] Some content here";
        let parsed = AaakEntry::parse(input);

        assert_eq!(parsed.markers.len(), 3);
        assert_eq!(parsed.markers[0].0, AaakMarker::Timestamp);
        assert_eq!(parsed.markers[0].1, "2024-01-01T10:00:00Z");
        assert_eq!(parsed.markers[1].0, AaakMarker::Phase);
        assert_eq!(parsed.markers[1].1, "planning");
        assert_eq!(parsed.markers[2].0, AaakMarker::Result);
        assert_eq!(parsed.markers[2].1, "0.85");
        assert!(parsed.raw_content.contains("Some content here"));
    }

    #[test]
    fn test_structured_entry() {
        let entry = StructuredEntry::new("Test content")
            .with_phase("execution")
            .with_result("success")
            .with_lesson("Always validate input");

        assert_eq!(entry.content, "Test content");
        assert_eq!(
            entry.get_marker(AaakMarker::Phase),
            Some(&"execution".to_string())
        );
        assert_eq!(
            entry.get_marker(AaakMarker::Result),
            Some(&"success".to_string())
        );
        assert!(entry.has_marker(AaakMarker::Lesson));
        assert!(!entry.is_aaak);
    }

    #[test]
    fn test_agent_meta() {
        let mut meta = AgentMeta::new("test_agent");

        assert_eq!(meta.entry_count, 0);
        assert!(meta.last_entry_at.is_none());

        meta.increment();
        assert_eq!(meta.entry_count, 1);
        assert!(meta.last_entry_at.is_some());

        meta.increment();
        assert_eq!(meta.entry_count, 2);
    }

    #[test]
    fn test_diary_builder() {
        let diary = AgentDiaryBuilder::new()
            .agent_name("orchestrator")
            .build()
            .expect("valid agent name");

        assert!(
            diary.get_meta().is_none(),
            "meta should be None before loading"
        );
        assert_eq!(diary.entry_count(), 0);
    }

    #[test]
    fn test_diary_key_generation() {
        let diary = AgentDiary::new("scout");

        assert_eq!(diary.wing_prefix(), "wing_scout");
        assert_eq!(diary.meta_key(), "wing_scout/diary/meta");
        assert_eq!(diary.entries_prefix(), "wing_scout/diary/entries");
        assert_eq!(diary.topics_prefix(), "wing_scout/diary/topics");
        assert_eq!(
            diary.topic_key("planning"),
            "wing_scout/diary/topics/planning"
        );
    }
}
