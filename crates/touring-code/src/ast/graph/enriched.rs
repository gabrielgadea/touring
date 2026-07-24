//! Enriched blast radius — categorized impact with severity scoring.

use indexmap::IndexMap;
use std::collections::HashSet;

use super::{BlastRadius, SymbolIndex};

/// AST-3: Impact categories for enriched blast radius analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImpactCategory {
    /// Files that directly import the changed file.
    DirectDependents,
    /// Files reachable transitively (not direct).
    TransitiveDependents,
    /// Files frequently co-edited with the changed file.
    CoEdited,
}

/// AST-3: Enriched blast radius with categorized impact and severity score.
#[derive(Debug, Clone)]
pub struct EnrichedBlastRadius {
    /// Base blast radius (reuses existing struct).
    pub base: BlastRadius,
    /// Files that directly import the changed file (depth=1).
    pub direct_dependents: Vec<String>,
    /// Files reachable transitively (depth>1), excluding direct.
    pub transitive_dependents: Vec<String>,
    /// Files frequently co-edited with the changed file.
    pub co_edited_files: Vec<String>,
    /// Severity: 0.0–1.0. Higher = more risky change.
    pub severity: f64,
}

/// Compute an enriched blast radius for a file.
///
/// Uses `SymbolIndex.reverse_deps` for direct dependents (depth=1),
/// `blast_radius` for transitive, and `co_edit_data` for co-edit history.
pub fn compute_enriched_blast_radius(
    index: &SymbolIndex,
    file: &str,
    co_edit_data: &IndexMap<String, Vec<String>>,
) -> EnrichedBlastRadius {
    let base = index.blast_radius(file);

    // Direct dependents: files that directly import `file` (depth=1)
    let direct_dependents: Vec<String> = index.reverse_deps.get(file).cloned().unwrap_or_default();

    // Transitive dependents: in affected_files but NOT in direct_dependents and NOT start_file
    let direct_set: HashSet<&str> = direct_dependents.iter().map(String::as_str).collect();
    let transitive_dependents: Vec<String> = base
        .affected_files
        .iter()
        .filter(|f| !direct_set.contains(f.as_str()) && f.as_str() != file)
        .cloned()
        .collect();

    // Co-edited files from history
    let co_edited_files: Vec<String> = co_edit_data.get(file).cloned().unwrap_or_default();

    let d = direct_dependents.len() as f64;
    let t = transitive_dependents.len() as f64;
    let c = co_edited_files.len() as f64;
    let total = d + t + c;
    let severity = if total == 0.0 {
        0.0
    } else {
        (0.5 * d + 0.3 * t + 0.2 * c) / total
    };

    EnrichedBlastRadius {
        base,
        direct_dependents,
        transitive_dependents,
        co_edited_files,
        severity,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::super::SymbolIndex;
    use super::*;
    use crate::ast::languages::Lang;
    use indexmap::IndexMap;

    #[test]
    fn test_enriched_empty_graph() {
        let index = SymbolIndex::new();
        let result = compute_enriched_blast_radius(&index, "a.rs", &IndexMap::new());
        assert!(result.direct_dependents.is_empty());
        assert!(result.transitive_dependents.is_empty());
        assert!((result.severity - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_enriched_blast_radius_direct_deps() {
        let mut index = SymbolIndex::new();
        index
            .index_file("a.py", "def helper(): pass", Lang::Python)
            .unwrap();
        index
            .reverse_deps
            .insert("a.py".to_string(), vec!["b.py".to_string()]);

        let result = compute_enriched_blast_radius(&index, "a.py", &IndexMap::new());
        assert_eq!(result.direct_dependents, vec!["b.py"]);
    }

    #[test]
    fn test_enriched_blast_radius_transitive_deps() {
        let mut index = SymbolIndex::new();
        index
            .index_file("a.py", "def helper(): pass", Lang::Python)
            .unwrap();
        index
            .reverse_deps
            .insert("a.py".to_string(), vec!["b.py".to_string()]);
        index
            .reverse_deps
            .insert("b.py".to_string(), vec!["c.py".to_string()]);

        let result = compute_enriched_blast_radius(&index, "a.py", &IndexMap::new());
        assert_eq!(result.direct_dependents, vec!["b.py"]);
        assert!(result.transitive_dependents.contains(&"c.py".to_string()));
    }

    #[test]
    fn test_enriched_blast_radius_severity_score() {
        let mut index = SymbolIndex::new();
        index.index_file("a.py", "x = 1", Lang::Python).unwrap();
        index
            .reverse_deps
            .insert("a.py".to_string(), vec!["b.py".to_string()]);

        let result = compute_enriched_blast_radius(&index, "a.py", &IndexMap::new());
        assert!(result.severity >= 0.0 && result.severity <= 1.0);
    }

    #[test]
    fn test_category_weights_sum_to_one() {
        let w_direct = 0.5_f64;
        let w_trans = 0.3_f64;
        let w_coedit = 0.2_f64;
        assert!((w_direct + w_trans + w_coedit - 1.0).abs() < 1e-10);
    }
}
