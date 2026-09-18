//! Enriched blast radius — categorized impact with severity scoring.

use indexmap::IndexMap;
use std::collections::HashSet;

use super::{BlastRadius, SymbolIndex};

/// AST-3: Impact categories for enriched blast radius analysis.
///
/// The one place that says what each category weighs. The severity score and the
/// signal layer (`touring-hook-handlers`) used to carry these as two separate triples
/// of bare numbers, and the test that "checked" the weights summed literals it
/// declared itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImpactCategory {
    /// Files that directly import the changed file.
    DirectDependents,
    /// Files reachable transitively (not direct).
    TransitiveDependents,
    /// Files frequently co-edited with the changed file.
    CoEdited,
}

impl ImpactCategory {
    /// Every category, most direct first.
    pub const ALL: [Self; 3] = [
        Self::DirectDependents,
        Self::TransitiveDependents,
        Self::CoEdited,
    ];

    /// Share of the enriched severity carried by one file of this category.
    /// The three shares sum to 1.
    #[must_use]
    pub fn severity_weight(self) -> f64 {
        match self {
            Self::DirectDependents => 0.5,
            Self::TransitiveDependents => 0.3,
            Self::CoEdited => 0.2,
        }
    }

    /// Discount applied to the severity when one file of this category becomes a signal.
    #[must_use]
    pub fn signal_discount(self) -> f32 {
        match self {
            Self::DirectDependents => 1.0,
            Self::TransitiveDependents => 0.5,
            Self::CoEdited => 0.3,
        }
    }

    /// Prefix of the signal label (`direct:<file>`), stable across releases.
    #[must_use]
    pub fn signal_prefix(self) -> &'static str {
        match self {
            Self::DirectDependents => "direct",
            Self::TransitiveDependents => "transitive",
            Self::CoEdited => "co_edit",
        }
    }
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

impl EnrichedBlastRadius {
    /// The files that fall in one impact category.
    #[must_use]
    pub fn files(&self, category: ImpactCategory) -> &[String] {
        match category {
            ImpactCategory::DirectDependents => &self.direct_dependents,
            ImpactCategory::TransitiveDependents => &self.transitive_dependents,
            ImpactCategory::CoEdited => &self.co_edited_files,
        }
    }
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

    let mut enriched = EnrichedBlastRadius {
        base,
        direct_dependents,
        transitive_dependents,
        co_edited_files,
        severity: 0.0,
    };
    let count = |c: ImpactCategory| enriched.files(c).len() as f64;
    let total: f64 = ImpactCategory::ALL.into_iter().map(count).sum();
    if total > 0.0 {
        let weighted: f64 = ImpactCategory::ALL
            .into_iter()
            .map(|c| c.severity_weight() * count(c))
            .sum();
        enriched.severity = weighted / total;
    }
    enriched
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
        // The weights the score actually uses — this test used to sum three literals of its own.
        let sum: f64 = ImpactCategory::ALL
            .into_iter()
            .map(ImpactCategory::severity_weight)
            .sum();
        assert!((sum - 1.0).abs() < 1e-10, "severity weights sum to {sum}");
    }

    #[test]
    fn test_severity_is_the_weighted_mean_of_the_categories() {
        // one direct + one transitive: (0.5 + 0.3) / 2
        let mut index = SymbolIndex::new();
        index.index_file("a.py", "x = 1", Lang::Python).unwrap();
        index
            .reverse_deps
            .insert("a.py".to_string(), vec!["b.py".to_string()]);
        index
            .reverse_deps
            .insert("b.py".to_string(), vec!["c.py".to_string()]);
        let result = compute_enriched_blast_radius(&index, "a.py", &IndexMap::new());
        assert_eq!(result.files(ImpactCategory::DirectDependents), ["b.py"]);
        assert!(
            result
                .files(ImpactCategory::TransitiveDependents)
                .contains(&"c.py".to_string())
        );
        let expected = (0.5 * 1.0 + 0.3 * result.transitive_dependents.len() as f64)
            / (1.0 + result.transitive_dependents.len() as f64);
        assert!((result.severity - expected).abs() < 1e-12, "{}", result.severity);
    }

    #[test]
    fn test_signal_prefixes_keep_their_wire_names() {
        // Signal consumers match on these prefixes; renaming one silently orphans them.
        let labels: Vec<&str> = ImpactCategory::ALL.into_iter().map(ImpactCategory::signal_prefix).collect();
        assert_eq!(labels, ["direct", "transitive", "co_edit"]);
    }
}
