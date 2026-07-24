//! Memory recall model — parsed from `touring memory recall -j`.

use serde::Deserialize;

/// A memory entry from touring's semantic + FTS5 store.
#[derive(Debug, Clone, Deserialize)]
pub struct MemoryEntry {
    /// Memory key (e.g. "lesson:wave_2026_05_03").
    #[serde(rename = "key")]
    pub key: String,
    /// Memory value/content stored under the key.
    #[serde(rename = "value")]
    pub value: String,
    /// Memory tier (e.g. "semantic", "episodic").
    #[serde(rename = "tier")]
    pub tier: Option<String>,
    /// Relevance score from recall query (0.0–1.0).
    #[serde(rename = "score")]
    pub score: Option<f64>,
}

/// Memory recall report wrapper — parsed from `touring memory recall -j`.
#[derive(Debug, Clone, Deserialize)]
pub struct MemoryRecallReport {
    /// List of matching memory entries.
    #[serde(rename = "entries")]
    pub entries: Vec<MemoryEntry>,
}

/// Enriched memory search response with timestamp and metadata (S-3 deliverable).
#[derive(Debug, Clone, Deserialize)]
pub struct MemorySearchResponse {
    /// Matching memory entries.
    #[serde(rename = "entries")]
    pub entries: Vec<MemoryEntry>,
    /// Query that produced these results.
    #[serde(rename = "query")]
    pub query: String,
    /// Timestamp when the search was performed (Unix epoch millis).
    #[serde(rename = "timestamp")]
    pub timestamp: i64,
    /// Total number of results found (may be more than returned).
    #[serde(rename = "total")]
    pub total: Option<usize>,
}

impl MemorySearchResponse {
    /// Create from daemon's direct array response (no wrapper object).
    pub fn from_entries(entries: Vec<MemoryEntry>) -> Self {
        Self {
            entries,
            query: String::new(),
            timestamp: 0,
            total: None,
        }
    }

    /// Returns the number of entries in this response.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_entry_deserialize() {
        let json = r#"{"key":"lesson:test","value":"test content","tier":"semantic","score":0.95}"#;
        let entry: MemoryEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.key, "lesson:test");
        assert_eq!(entry.value, "test content");
        assert_eq!(entry.tier, Some("semantic".to_string()));
        assert_eq!(entry.score, Some(0.95));
    }

    #[test]
    fn test_memory_recall_report_empty() {
        let json = r#"{"entries":[]}"#;
        let report: MemoryRecallReport = serde_json::from_str(json).unwrap();
        assert!(report.entries.is_empty());
    }

    #[test]
    fn test_memory_search_response() {
        let json = r#"{"entries":[],"query":"test","timestamp":1715000000000}"#;
        let response: MemorySearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.query, "test");
        assert_eq!(response.timestamp, 1715000000000);
        assert!(response.is_empty());
    }
}
