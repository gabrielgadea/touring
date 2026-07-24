//! DB-backed pre-commit gate adapters.
//!
//! Extracted from `core/context.rs` (F-9 modularization): the `AnalysisGateAdapter`
//! (wiring-score gate backed by `touring_analysis::analyze_wiring`) and its
//! `WiringGateError`. Re-exported from `core::context` so the public API
//! (`crate::AnalysisGateAdapter`, `crate::WiringGateError`,
//! `crate::WIRING_GATE_BYPASSED_COUNT`) is preserved verbatim. The
//! `WiringGateFn` type alias and the `WIRING_SCORES` cache live in `context.rs`
//! and are referenced here by full path.

use crate::error::GenerateError;
use std::sync::Arc;

/// DB-backed wiring gate adapter using `touring_analysis::analyze_wiring`.
///
/// Runs a real wiring audit against the project `knowledge.db` before each
/// commit and rejects when the composite wiring score would fall below the
/// configured threshold. Unlike `SynWiringGateAdapter` (syntax-only), this
/// adapter cross-references `pub` symbols against the live consumer map in
/// the database — far more accurate, but requires an open DB connection.
///
/// # Wiring strategy
///
/// The adapter holds a `Mutex<rusqlite::Connection>` captured at construction
/// time. On each `check()` call it re-runs `analyze_wiring(&conn)` to get a
/// fresh baseline snapshot. The gate then evaluates the **projected** score
/// after applying the rendered files — because the files have not been written
/// yet, the projection uses a conservative heuristic: each rendered `.rs` file
/// is assumed to introduce `n_pub_items` potential new orphans (worst case).
///
/// # Thresholds
///
/// - `min_score` — reject if current `WiringReport.score` falls below this
/// - `max_projected_orphan_delta` — reject if projected new orphans exceed this
///
/// POTENCIALIZAR defaults: `min_score = 0.7`, `max_projected_orphan_delta = 5`.
#[cfg(feature = "analysis-gate")]
pub struct AnalysisGateAdapter {
    conn: std::sync::Mutex<rusqlite::Connection>,
    /// Minimum composite wiring score to allow commit. Rejects if below.
    pub min_score: f64,
    /// Maximum projected new orphan symbols the gate tolerates per batch.
    pub max_projected_orphan_delta: usize,
    /// Bypass gate entirely when true. Audited via tracing + counter.
    /// Toggled by `TOURING_WIRING_GATE_DISABLED=1` env var. Default: false.
    pub disabled: bool,
}

/// Counter incremented every time the wiring gate is bypassed via
/// `TOURING_WIRING_GATE_DISABLED=1`. Exposed for audit / `gate-metrics`.
#[cfg(feature = "analysis-gate")]
pub static WIRING_GATE_BYPASSED_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[cfg(feature = "analysis-gate")]
impl std::fmt::Debug for AnalysisGateAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalysisGateAdapter")
            .field("min_score", &self.min_score)
            .field(
                "max_projected_orphan_delta",
                &self.max_projected_orphan_delta,
            )
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "analysis-gate")]
/// Error returned by the wiring-gate constructors ([`AnalysisGateAdapter`] and
/// [`CompositeWiringGate`](crate::core::context::CompositeWiringGate)) when the
/// backing knowledge database cannot be opened.
///
/// Replaces the previous stringly-typed `Result<_, String>` so consumers can match
/// on the failure mode and inspect the underlying [`rusqlite::Error`] via
/// [`std::error::Error::source`], while the `Display` rendering is preserved
/// byte-for-byte (`open knowledge db `<path>`: <error>`).
#[cfg(feature = "analysis-gate")]
#[derive(Debug, thiserror::Error)]
pub enum WiringGateError {
    /// Opening the knowledge `SQLite` database failed (missing path, permission, or
    /// corruption). Carries the rendered DB path plus the underlying open error.
    #[error("open knowledge db `{path}`: {source}")]
    OpenDb {
        /// `Display` rendering of the database path that failed to open.
        path: String,
        /// Underlying `rusqlite` open error.
        #[source]
        source: rusqlite::Error,
    },
}

impl AnalysisGateAdapter {
    /// Open the knowledge DB at `db_path` and build an adapter with defaults.
    ///
    /// # Errors
    ///
    /// Returns [`WiringGateError`] when the database file cannot be opened —
    /// typically because the path does not exist or the process lacks read permission.
    pub fn open(db_path: &std::path::Path) -> Result<Self, WiringGateError> {
        let conn =
            rusqlite::Connection::open(db_path).map_err(|source| WiringGateError::OpenDb {
                path: db_path.display().to_string(),
                source,
            })?;
        Ok(Self {
            conn: std::sync::Mutex::new(conn),
            min_score: 0.7,
            max_projected_orphan_delta: 5,
            disabled: false,
        })
    }

    /// Build an adapter with explicit thresholds.
    ///
    /// # Errors
    ///
    /// Returns [`WiringGateError`] when opening the database fails — same as `open()`.
    pub fn with_thresholds(
        db_path: &std::path::Path,
        min_score: f64,
        max_projected_orphan_delta: usize,
    ) -> Result<Self, WiringGateError> {
        let mut adapter = Self::open(db_path)?;
        adapter.min_score = min_score;
        adapter.max_projected_orphan_delta = max_projected_orphan_delta;
        Ok(adapter)
    }

    /// Open and override thresholds from environment variables.
    ///
    /// Reads (all optional, all default to keeping current value):
    /// - `TOURING_WIRING_GATE_MIN_SCORE` (f64 in [0.0, 1.0]) — overrides `min_score`
    /// - `TOURING_WIRING_GATE_MAX_DELTA` (usize) — overrides `max_projected_orphan_delta`
    /// - `TOURING_WIRING_GATE_DISABLED` (truthy: "1"/"true"/"yes") — full bypass with audit
    ///
    /// Bypass writes a `WARN`-level tracing record at adapter build time so
    /// audit logs always show the configuration in use. Production callers
    /// should monitor `WIRING_GATE_BYPASSED_COUNT` for runtime usage.
    ///
    /// # Errors
    ///
    /// Returns [`WiringGateError`] when opening the database fails — same as `open()`.
    pub fn open_with_env(db_path: &std::path::Path) -> Result<Self, WiringGateError> {
        let mut adapter = Self::open(db_path)?;
        if let Ok(s) = std::env::var("TOURING_WIRING_GATE_MIN_SCORE") {
            if let Ok(v) = s.parse::<f64>() {
                adapter.min_score = v.clamp(0.0, 1.0);
            }
        }
        if let Ok(s) = std::env::var("TOURING_WIRING_GATE_MAX_DELTA") {
            if let Ok(v) = s.parse::<usize>() {
                adapter.max_projected_orphan_delta = v;
            }
        }
        if let Ok(s) = std::env::var("TOURING_WIRING_GATE_DISABLED") {
            let lower = s.to_lowercase();
            if matches!(lower.as_str(), "1" | "true" | "yes" | "on") {
                adapter.disabled = true;
                tracing::warn!(
                    "wiring gate DISABLED via TOURING_WIRING_GATE_DISABLED env var \
                     — audit via WIRING_GATE_BYPASSED_COUNT counter"
                );
            }
        }
        Ok(adapter)
    }

    /// Runs a baseline wiring audit from the captured connection.
    ///
    /// Returns the current `WiringReport` snapshot, or `None` when the connection
    /// is poisoned or the query fails at the analysis layer.
    #[must_use]
    pub fn baseline_report(&self) -> Option<touring_analysis::wiring::WiringReport> {
        let guard = self.conn.lock().ok()?;
        Some(touring_analysis::wiring::analyze_wiring(&guard))
    }

    /// Run the gate against a batch of rendered files.
    ///
    /// Stores the computed quality score in `QUALITY_SCORES` under `plan_id`
    /// for later retrieval by the RL reward injection logic.
    ///
    /// # Errors
    ///
    /// Returns `GenerateError::Internal` when:
    /// - Baseline `WiringReport.score` is below `min_score` (existing wiring already degraded)
    /// - Projected new orphans exceed `max_projected_orphan_delta`
    /// - The adapter cannot acquire its mutex (connection lock poisoned)
    pub fn check(
        &self,
        files: &[crate::plan::result::RenderedFile],
        plan_id: &str,
    ) -> Result<(), GenerateError> {
        // Bypass path: env-configured `disabled=true`. Audit + counter, then skip.
        if self.disabled {
            WIRING_GATE_BYPASSED_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tracing::warn!(
                plan_id = %plan_id,
                files = files.len(),
                "wiring gate BYPASSED (TOURING_WIRING_GATE_DISABLED=1) — REGRA #0 audit"
            );
            return Ok(());
        }

        let baseline = self
            .baseline_report()
            .ok_or_else(|| GenerateError::Internal("analysis gate: DB lock poisoned".into()))?;

        // Store wiring baseline score for RL reward injection.
        crate::core::context::WIRING_SCORES.insert(plan_id.to_string(), baseline.score);

        if baseline.score < self.min_score {
            return Err(GenerateError::Internal(format!(
                "analysis gate: baseline wiring score {:.3} < min {:.3} — fix existing orphans first",
                baseline.score, self.min_score
            )));
        }

        // Projected new orphans: count `pub` items in each .rs file.
        // Non-Rust files contribute 0. Uses a naive line-based scan so the adapter
        // stays feature-independent from `syn-quote` — callers may combine both.
        let mut projected_new_orphans: usize = 0;
        for rendered in files {
            let is_rust = std::path::Path::new(&rendered.path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"));
            if !is_rust {
                continue;
            }
            projected_new_orphans += Self::count_pub_declarations(&rendered.content);
        }

        if projected_new_orphans > self.max_projected_orphan_delta {
            return Err(GenerateError::Internal(format!(
                "analysis gate: projected {} new pub items > max delta {} (baseline score={:.3}, orphans={})",
                projected_new_orphans,
                self.max_projected_orphan_delta,
                baseline.score,
                baseline.orphan_count,
            )));
        }

        Ok(())
    }

    /// Naive line-based `pub` declaration counter.
    ///
    /// Counts top-level `pub fn`, `pub struct`, `pub enum`, `pub trait`,
    /// `pub const`, `pub static`, `pub type`, `pub union`, and `pub mod`
    /// occurrences at the start of a line (after whitespace trim).
    ///
    /// Handles visibility modifiers `pub(crate)`, `pub(super)`, and
    /// `pub(in path::to::mod)` via explicit balanced-paren tracking.
    ///
    /// Does not count `pub use` (re-exports) — those don't introduce new
    /// symbols and should not count against the orphan budget.
    #[must_use]
    pub fn count_pub_declarations(content: &str) -> usize {
        const ITEM_KEYWORDS: &[&str] = &[
            "fn ", "struct ", "enum ", "trait ", "const ", "static ", "type ", "union ", "mod ",
        ];
        let mut count = 0;
        for line in content.lines() {
            let trimmed = line.trim_start();
            let Some(after_pub) = trimmed.strip_prefix("pub") else {
                continue;
            };
            // Strip an optional `(...)` visibility modifier via balanced parens.
            let mut remainder = after_pub;
            if remainder.starts_with('(') {
                let mut depth = 0usize;
                let mut end_idx: Option<usize> = None;
                for (i, ch) in remainder.char_indices() {
                    match ch {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                end_idx = Some(i + ch.len_utf8());
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(e) = end_idx {
                    remainder = &remainder[e..];
                }
            }
            let rest = remainder.trim_start();
            if ITEM_KEYWORDS.iter().any(|kw| rest.starts_with(kw)) {
                count += 1;
            }
        }
        count
    }

    /// Build a `WiringGateFn` closure that invokes this adapter.
    #[must_use]
    pub fn into_closure(self: Arc<Self>) -> crate::core::context::WiringGateFn {
        Arc::new(
            move |files: &[crate::plan::result::RenderedFile], plan_id: &str| {
                self.check(files, plan_id)
            },
        )
    }
}
