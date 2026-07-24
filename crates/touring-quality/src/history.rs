//! `ScoreHistory` — append-only JSONL audit trail for every harness run.
//!
//! The history is the **drift detection** substrate: each `run_harness`
//! invocation appends one `HistoryEntry` to
//! `~/.claude/touring/elite-history.jsonl` (or any path the caller
//! specifies). Drift detection reads the last N entries and reports:
//!
//! - **mean composite drift** over a window
//! - **regression** if the latest 3 entries are strictly worse than the
//!   prior 3
//! - **agent/model breakdown** (per-agent composite)

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::score::EliteScore;
use crate::score::EliteTier;

/// One entry in the append-only score history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// UTC instant at which the harness run was recorded.
    pub timestamp: DateTime<Utc>,
    /// Identifier of the agent that produced the change.
    pub agent_id: String,
    /// Name of the model that generated the change.
    pub model: String,
    /// Composite elite score (0.0–1.0) for the run.
    pub composite: f32,
    /// Elite tier derived from the composite score.
    pub tier: EliteTier,
    /// One-line summary of the change (e.g. "feat: harness crate").
    pub change_summary: String,
    /// Number of files in the change.
    pub file_count: usize,
    /// Total LOC added in the change.
    pub added_loc: u64,
}

/// Drift report over a window of history entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    /// Mean composite across the window.
    pub mean_composite: f32,
    /// Min composite in the window.
    pub min_composite: f32,
    /// Max composite in the window.
    pub max_composite: f32,
    /// Standard deviation of composite.
    pub stddev: f32,
    /// Whether the last 3 entries are strictly worse than the prior 3.
    pub regression: bool,
    /// Number of entries analyzed.
    pub window_size: usize,
    /// Per-agent breakdown.
    pub by_agent: Vec<(String, f32)>,
}

impl DriftReport {
    /// Whether the trend is concerning (regression or mean < 0.80).
    #[must_use]
    pub fn is_alert(&self) -> bool {
        self.regression || self.mean_composite < 0.80
    }
}

/// Append-only history backed by a JSONL file.
#[derive(Debug, Clone)]
pub struct ScoreHistory {
    path: PathBuf,
}

impl ScoreHistory {
    /// Open a history at `path` (creates parent dirs on first append).
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Default path: `~/.claude/touring/elite-history.jsonl`.
    pub fn default_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home)
            .join(".claude")
            .join("touring")
            .join("elite-history.jsonl")
    }

    /// Append one entry to the history.
    ///
    /// # Errors
    ///
    /// Returns a `std::io::Error` if the parent directory cannot be created,
    /// the JSONL file cannot be opened for appending, the entry fails to
    /// serialize, or the write to disk fails.
    pub fn append(&self, entry: &HistoryEntry) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
        writeln!(f, "{line}")
    }

    /// Read the last `n` entries (most recent first).
    ///
    /// # Errors
    ///
    /// Currently infallible in practice: a missing history file yields an
    /// empty vector and malformed lines are skipped. The `Result` is retained
    /// so future stricter parsing can surface a `std::io::Error`.
    pub fn last_n(&self, n: usize) -> std::io::Result<Vec<HistoryEntry>> {
        let Ok(f) = File::open(&self.path) else {
            return Ok(Vec::new());
        };
        let reader = BufReader::new(f);
        let mut all: Vec<HistoryEntry> = reader
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| serde_json::from_str(&line).ok())
            .collect();
        all.reverse();
        all.truncate(n);
        Ok(all)
    }

    /// Compute drift over a window of the last `n` entries.
    ///
    /// # Errors
    ///
    /// Returns a `std::io::Error` if reading the underlying history via
    /// [`ScoreHistory::last_n`] fails.
    pub fn drift_detection(&self, n: usize) -> std::io::Result<DriftReport> {
        let entries = self.last_n(n)?;
        Ok(Self::compute_drift(&entries))
    }

    /// Pure function: compute drift from a slice of entries.
    // reason: composite-count casts feed means/variance over a small window; f32 precision suffices.
    #[allow(clippy::cast_precision_loss)]
    #[must_use]
    pub fn compute_drift(entries: &[HistoryEntry]) -> DriftReport {
        use std::collections::HashMap;
        if entries.is_empty() {
            return DriftReport {
                mean_composite: 0.0,
                min_composite: 0.0,
                max_composite: 0.0,
                stddev: 0.0,
                regression: false,
                window_size: 0,
                by_agent: Vec::new(),
            };
        }
        let composites: Vec<f32> = entries.iter().map(|e| e.composite).collect();
        let mean = composites.iter().sum::<f32>() / composites.len() as f32;
        let min = composites.iter().copied().fold(f32::INFINITY, f32::min);
        let max = composites.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let var =
            composites.iter().map(|c| (c - mean).powi(2)).sum::<f32>() / composites.len() as f32;
        let stddev = var.sqrt();

        // Regression = last 3 (most recent) strictly worse than prior 3.
        // Input is expected in chronological order (oldest first, newest last).
        let regression = if entries.len() >= 6 {
            let last3: f32 = entries
                .iter()
                .rev()
                .take(3)
                .map(|e| e.composite)
                .sum::<f32>()
                / 3.0;
            let prev3: f32 = entries
                .iter()
                .rev()
                .skip(3)
                .take(3)
                .map(|e| e.composite)
                .sum::<f32>()
                / 3.0;
            last3 + 0.01 < prev3
        } else {
            false
        };

        // Per-agent mean.
        let mut by_agent: HashMap<String, Vec<f32>> = HashMap::new();
        for e in entries {
            by_agent
                .entry(e.agent_id.clone())
                .or_default()
                .push(e.composite);
        }
        let mut by_agent: Vec<(String, f32)> = by_agent
            .into_iter()
            .map(|(a, v)| (a, v.iter().sum::<f32>() / v.len() as f32))
            .collect();
        by_agent.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        DriftReport {
            mean_composite: mean,
            min_composite: min,
            max_composite: max,
            stddev,
            regression,
            window_size: entries.len(),
            by_agent,
        }
    }

    /// Construct an entry from a score + change metadata.
    #[must_use]
    pub fn entry_from(
        score: &EliteScore,
        agent_id: &str,
        model: &str,
        change_summary: &str,
        file_count: usize,
        added_loc: u64,
    ) -> HistoryEntry {
        HistoryEntry {
            timestamp: Utc::now(),
            agent_id: agent_id.to_string(),
            model: model.to_string(),
            composite: score.composite,
            tier: score.tier,
            change_summary: change_summary.to_string(),
            file_count,
            added_loc,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(composite: f32, agent: &str) -> HistoryEntry {
        HistoryEntry {
            timestamp: Utc::now(),
            agent_id: agent.into(),
            model: "test".into(),
            composite,
            tier: crate::score::tier_for(composite),
            change_summary: "test".into(),
            file_count: 1,
            added_loc: 10,
        }
    }

    #[test]
    fn drift_no_regression() {
        let entries = vec![
            entry(0.95, "a"),
            entry(0.94, "a"),
            entry(0.96, "a"),
            entry(0.95, "a"),
            entry(0.93, "a"),
        ];
        let d = ScoreHistory::compute_drift(&entries);
        assert!((d.mean_composite - 0.946).abs() < 0.01);
        assert!(!d.regression);
    }

    #[test]
    fn drift_detects_regression() {
        // last 3 = [0.80, 0.82, 0.78] avg = 0.80
        // prev 3 = [0.95, 0.96, 0.94] avg = 0.95
        // last3 + 0.01 (0.81) < prev3 (0.95) → regression
        let entries = vec![
            entry(0.95, "a"),
            entry(0.96, "a"),
            entry(0.94, "a"),
            entry(0.80, "a"),
            entry(0.82, "a"),
            entry(0.78, "a"),
        ];
        let d = ScoreHistory::compute_drift(&entries);
        assert!(
            d.regression,
            "expected regression but got regression={}",
            d.regression
        );
    }

    #[test]
    // reason: exact compare of sentinel composites written then read back unchanged via JSONL roundtrip.
    #[allow(clippy::float_cmp)]
    fn append_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist.jsonl");
        let h = ScoreHistory::new(&path);
        h.append(&entry(0.97, "claude")).unwrap();
        h.append(&entry(0.93, "claude")).unwrap();
        let last = h.last_n(10).unwrap();
        assert_eq!(last.len(), 2);
        assert_eq!(last[0].composite, 0.93); // most recent first
        assert_eq!(last[1].composite, 0.97);
    }
}
