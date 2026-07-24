//! Wiring Audit Tool — Integration completeness audit via wiring intelligence.
//!
//! Exposes H83 IntegrationCompletenessHandler data through MCP.
//! Provides orphan detection, per-module integration scores, and recommendations.

use serde::Serialize;

/// Wiring audit result — high-level summary
#[derive(Debug, Clone, Serialize)]
pub struct WiringAuditResult {
    /// Total number of orphan `pub` symbols found.
    pub orphan_count: i64,
    /// Number of modules examined by the audit.
    pub audited_modules: usize,
    /// Number of modules with no orphans (fully integrated).
    pub clean_modules: usize,
    /// Actionable wiring recommendations.
    pub recommendations: Vec<String>,
}

/// Per-module integration score entry
#[derive(Debug, Clone, Serialize)]
pub struct ModuleScore {
    /// Path of the module being scored.
    pub file_path: String,
    /// Integration completeness score (0.0-1.0).
    pub integration_score: f64,
    /// Number of `pub` symbols declared in the module.
    pub total_pub_symbols: i64,
    /// Number of those symbols that have no consumers.
    pub orphan_count: i64,
}

/// Full audit result with per-module breakdown
#[derive(Debug, Clone, Serialize)]
pub struct WiringAuditFull {
    /// Total number of orphan `pub` symbols across all modules.
    pub orphan_count: i64,
    /// Total number of modules in the breakdown.
    pub total_modules: usize,
    /// Per-module integration scores.
    pub modules: Vec<ModuleScore>,
    /// Actionable wiring recommendations.
    pub recommendations: Vec<String>,
    /// Number of dependency cycles detected via Tarjan SCC (F2).
    /// Zero means the wiring graph is acyclic.
    pub cycles_count: usize,
}

impl WiringAuditFull {
    /// Build recommendations from audit data
    pub fn build_recommendations(orphan_count: i64, modules: &[ModuleScore]) -> Vec<String> {
        let mut recs = Vec::new();
        if orphan_count == 0 {
            recs.push("Codebase wiring is clean — no orphan pub symbols detected.".to_string());
            return recs;
        }
        let orphan_modules: Vec<_> = modules.iter().filter(|m| m.orphan_count > 0).collect();
        if !orphan_modules.is_empty() {
            recs.push(format!(
                "{} module(s) have orphan pub symbols — consider wiring into consumers or reducing visibility.",
                orphan_modules.len()
            ));
        }
        let low_score: Vec<_> = modules
            .iter()
            .filter(|m| m.integration_score < 0.5)
            .collect();
        if !low_score.is_empty() {
            recs.push(format!(
                "{} module(s) have integration score < 0.5 — review symbol dependencies.",
                low_score.len()
            ));
        }
        recs
    }
}
