//! Self-improving context injection templates with UCB1 selection and mutation.
//!
//! # Design
//!
//! Each [`ContextTemplate`] contains:
//! - A text pattern with `{placeholder}` sections
//! - Reward statistics (sum + count) for computing average reward
//! - Lineage tracking (parent_id, version)
//!
//! [`TemplateLibrary`] manages a population of templates and provides:
//! - **Selection** via UCB1 (exploration/exploitation balance)
//! - **Reward recording** from downstream feedback
//! - **Evolution** by mutating low-performing templates
//! - **Persistence** via JSON serialization
//!
//! # Example
//!
//! ```
//! use touring_intelligence::rl::templates::TemplateLibrary;
//!
//! let mut lib = TemplateLibrary::new();
//! let selected = lib.select();
//! let id = selected.id.clone();
//! // ... use template, observe outcome ...
//! lib.record_reward(&id, 0.8);
//! ```

use serde::{Deserialize, Serialize};
use std::path::Path;

// ── ContextTemplate ──────────────────────────────────────────────────────

/// A context injection template with reward tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTemplate {
    /// Unique template identifier.
    pub id: String,
    /// Template version (incremented on mutation).
    pub version: u32,
    /// The template text with `{placeholders}`.
    pub text: String,
    /// Sections included in this template (ordered).
    pub sections: Vec<String>,
    /// Total reward accumulated.
    pub reward_sum: f64,
    /// Number of times evaluated.
    pub eval_count: u32,
    /// Parent template ID (if mutated from another).
    pub parent_id: Option<String>,
}

impl ContextTemplate {
    /// Average reward. Returns 0.0 if never evaluated.
    #[inline]
    pub fn avg_reward(&self) -> f64 {
        if self.eval_count == 0 {
            0.0
        } else {
            self.reward_sum / self.eval_count as f64
        }
    }

    /// UCB1 score for exploration/exploitation balance.
    ///
    /// Unexplored templates return `f64::INFINITY` to guarantee they are tried.
    /// Formula: `avg_reward + sqrt(2 * ln(total_evals) / eval_count)`
    #[inline]
    pub fn ucb1_score(&self, total_evals: u32) -> f64 {
        if self.eval_count == 0 {
            return f64::INFINITY;
        }
        let exploitation = self.avg_reward();
        let exploration = (2.0 * (total_evals as f64).ln() / self.eval_count as f64).sqrt();
        exploitation + exploration
    }
}

// ── TemplateLibrary ──────────────────────────────────────────────────────

/// Mutation type applied during template evolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationType {
    /// Rotate sections: move first to last.
    Rotate,
    /// Drop one section from the template.
    DropSection,
    /// Add a section from the available pool.
    AddSection,
    /// Swap separator between `\n\n` and ` | `.
    SwapSeparator,
}

/// All known section names that can be added to a template.
const AVAILABLE_SECTIONS: &[&str] = &[
    "overview",
    "gotchas",
    "relations",
    "errors",
    "metrics",
    "history",
];

/// Template library with evolution capabilities.
///
/// Manages a population of [`ContextTemplate`]s, selecting the best via UCB1,
/// recording reward feedback, and evolving low-performers via mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateLibrary {
    /// All templates in the library.
    pub templates: Vec<ContextTemplate>,
    /// Total evaluations across all templates.
    pub total_evals: u32,
}

impl TemplateLibrary {
    /// Create a new library with three default templates of increasing detail.
    pub fn new() -> Self {
        Self {
            templates: vec![
                ContextTemplate {
                    id: "default_minimal".into(),
                    version: 1,
                    text: "{gotchas}".into(),
                    sections: vec!["gotchas".into()],
                    reward_sum: 0.0,
                    eval_count: 0,
                    parent_id: None,
                },
                ContextTemplate {
                    id: "default_standard".into(),
                    version: 1,
                    text: "{gotchas}\n{relations}".into(),
                    sections: vec!["gotchas".into(), "relations".into()],
                    reward_sum: 0.0,
                    eval_count: 0,
                    parent_id: None,
                },
                ContextTemplate {
                    id: "default_full".into(),
                    version: 1,
                    text: "{overview}\n{gotchas}\n{relations}\n{errors}".into(),
                    sections: vec![
                        "overview".into(),
                        "gotchas".into(),
                        "relations".into(),
                        "errors".into(),
                    ],
                    reward_sum: 0.0,
                    eval_count: 0,
                    parent_id: None,
                },
            ],
            total_evals: 0,
        }
    }

    /// Select the best template via UCB1.
    ///
    /// Returns the template with the highest UCB1 score. Unexplored templates
    /// are always preferred (score = infinity).
    ///
    /// # Panics
    ///
    /// Panics if the library has no templates.
    pub fn select(&self) -> &ContextTemplate {
        self.templates
            .iter()
            .max_by(|a, b| {
                a.ucb1_score(self.total_evals)
                    .partial_cmp(&b.ucb1_score(self.total_evals))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("library must have at least one template")
    }

    /// Record a reward for the given template.
    ///
    /// Updates the template's `reward_sum` and `eval_count`, and increments
    /// the library's `total_evals`.
    pub fn record_reward(&mut self, template_id: &str, reward: f64) {
        if let Some(t) = self.templates.iter_mut().find(|t| t.id == template_id) {
            t.reward_sum += reward;
            t.eval_count += 1;
            self.total_evals += 1;
        }
    }

    /// Evolve the library by mutating low-performing templates.
    ///
    /// A template is mutated if it has at least `min_evals` evaluations
    /// and its average reward is below `low_reward_threshold`.
    ///
    /// Mutation strategies cycle through: Rotate, DropSection, AddSection,
    /// SwapSeparator. Each low-performing template gets one mutation type,
    /// cycling by index. The original is kept; the mutant is appended
    /// with a fresh eval count.
    ///
    /// Returns the number of mutations applied.
    pub fn evolve(&mut self, min_evals: u32, low_reward_threshold: f64) -> usize {
        let mut mutations = 0;
        let mut new_templates: Vec<ContextTemplate> = Vec::new();
        let mutation_types = [
            MutationType::Rotate,
            MutationType::DropSection,
            MutationType::AddSection,
            MutationType::SwapSeparator,
        ];

        for (idx, t) in self.templates.iter().enumerate() {
            if t.eval_count >= min_evals && t.avg_reward() < low_reward_threshold {
                // SAFETY: idx % mutation_types.len() is always < mutation_types.len().
                #[allow(clippy::indexing_slicing)]
                let mutation = mutation_types[idx % mutation_types.len()];
                let mut new_sections = t.sections.clone();

                let separator = match mutation {
                    MutationType::Rotate => {
                        if new_sections.len() > 1 {
                            let first = new_sections.remove(0);
                            new_sections.push(first);
                        }
                        "\n"
                    }
                    MutationType::DropSection => {
                        if new_sections.len() > 1 {
                            // Drop the last section (least likely to be essential)
                            new_sections.pop();
                        }
                        "\n"
                    }
                    MutationType::AddSection => {
                        // Add the first available section not already present
                        for &candidate in AVAILABLE_SECTIONS {
                            if !new_sections.iter().any(|s| s == candidate) {
                                new_sections.push(candidate.to_string());
                                break;
                            }
                        }
                        "\n"
                    }
                    MutationType::SwapSeparator => {
                        // Toggle separator: if text uses "\n\n" use " | ", otherwise use "\n\n"
                        if t.text.contains("\n\n") {
                            " | "
                        } else {
                            "\n\n"
                        }
                    }
                };

                let new_text = new_sections
                    .iter()
                    .map(|s| format!("{{{s}}}"))
                    .collect::<Vec<_>>()
                    .join(separator);

                let mutated = ContextTemplate {
                    id: format!("{}_v{}", t.id, t.version + 1),
                    version: t.version + 1,
                    text: new_text,
                    sections: new_sections,
                    reward_sum: 0.0,
                    eval_count: 0,
                    parent_id: Some(t.id.clone()),
                };
                new_templates.push(mutated);
                mutations += 1;
            }
        }

        self.templates.extend(new_templates);
        mutations
    }

    /// Prune underperforming templates from the library.
    ///
    /// Removes templates that have at least `min_evals` evaluations and
    /// whose average reward is below `max_avg_reward`. Templates that
    /// have not yet been evaluated enough are always retained.
    pub fn prune(&mut self, min_evals: u64, max_avg_reward: f64) {
        self.templates.retain(|t| {
            (t.eval_count as u64) < min_evals
                || (t.reward_sum / t.eval_count.max(1) as f64) >= max_avg_reward
        });
    }

    /// Save the library to a JSON file.
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Load a library from a JSON file.
    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(std::io::Error::other)
    }

    /// Load from file, or create a new default library if the file doesn't exist.
    ///
    /// If the file exists but is corrupt or unreadable, logs a warning via
    /// `tracing` and returns a fresh default library instead of propagating the error.
    pub fn load_or_default(path: &Path) -> Self {
        if path.exists() {
            Self::load(path).unwrap_or_else(|e| {
                tracing::warn!(
                    "[templates] Failed to load {}, using default: {e}",
                    path.display()
                );
                Self::new()
            })
        } else {
            Self::new()
        }
    }

    /// Number of templates in the library.
    #[inline]
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Whether the library is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }
}

impl Default for TemplateLibrary {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs asserted non-empty before indexing
    use super::*;

    #[test]
    fn test_new_library_has_defaults() {
        let lib = TemplateLibrary::new();
        assert_eq!(lib.templates.len(), 3);
        assert_eq!(lib.total_evals, 0);

        let ids: Vec<&str> = lib.templates.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"default_minimal"));
        assert!(ids.contains(&"default_standard"));
        assert!(ids.contains(&"default_full"));
    }

    #[test]
    fn test_select_unexplored_first() {
        let lib = TemplateLibrary::new();
        // All unexplored => UCB1 = infinity for all; any is valid
        let selected = lib.select();
        assert_eq!(selected.eval_count, 0);
    }

    #[test]
    fn test_record_reward_updates_stats() {
        let mut lib = TemplateLibrary::new();
        lib.record_reward("default_minimal", 0.5);

        let t = lib
            .templates
            .iter()
            .find(|t| t.id == "default_minimal")
            .unwrap();
        assert_eq!(t.eval_count, 1);
        assert!((t.reward_sum - 0.5).abs() < f64::EPSILON);
        assert_eq!(lib.total_evals, 1);
    }

    #[test]
    fn test_avg_reward_calculation() {
        let mut lib = TemplateLibrary::new();
        lib.record_reward("default_minimal", 0.6);
        lib.record_reward("default_minimal", 0.8);

        let t = lib
            .templates
            .iter()
            .find(|t| t.id == "default_minimal")
            .unwrap();
        assert_eq!(t.eval_count, 2);
        assert!((t.avg_reward() - 0.7).abs() < 1e-10);
    }

    #[test]
    fn test_avg_reward_zero_when_no_evals() {
        let lib = TemplateLibrary::new();
        let t = &lib.templates[0];
        assert!((t.avg_reward() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_ucb1_prefers_high_reward() {
        let mut lib = TemplateLibrary::new();

        // Give "minimal" high reward, "standard" low reward, "full" low reward
        for _ in 0..20 {
            lib.record_reward("default_minimal", 0.9);
            lib.record_reward("default_standard", 0.1);
            lib.record_reward("default_full", 0.1);
        }

        let selected = lib.select();
        // With same eval count, high reward should dominate
        assert_eq!(selected.id, "default_minimal");
    }

    #[test]
    fn test_ucb1_explores_low_eval() {
        let mut lib = TemplateLibrary::new();

        // Give "minimal" many evals with moderate reward
        for _ in 0..100 {
            lib.record_reward("default_minimal", 0.5);
        }
        // Give "standard" just 1 eval with moderate reward
        lib.record_reward("default_standard", 0.5);
        // Leave "full" unexplored

        // Unexplored "full" should be selected (UCB1 = infinity)
        let selected = lib.select();
        assert_eq!(selected.id, "default_full");
    }

    #[test]
    fn test_evolve_mutates_low_reward() {
        let mut lib = TemplateLibrary::new();

        // Give "default_standard" enough evals with low reward
        for _ in 0..10 {
            lib.record_reward("default_standard", 0.1);
        }
        // Give others high reward so they are not mutated
        for _ in 0..10 {
            lib.record_reward("default_minimal", 0.9);
            lib.record_reward("default_full", 0.9);
        }

        let before = lib.templates.len();
        let mutations = lib.evolve(5, 0.5);

        assert_eq!(mutations, 1);
        assert_eq!(lib.templates.len(), before + 1);

        // The mutant should exist
        let mutant = lib
            .templates
            .iter()
            .find(|t| t.parent_id.as_deref() == Some("default_standard"))
            .expect("mutant should exist");
        assert_eq!(mutant.eval_count, 0);
        assert!((mutant.reward_sum - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_evolve_skips_high_reward() {
        let mut lib = TemplateLibrary::new();

        for _ in 0..10 {
            lib.record_reward("default_minimal", 0.9);
            lib.record_reward("default_standard", 0.9);
            lib.record_reward("default_full", 0.9);
        }

        let before = lib.templates.len();
        let mutations = lib.evolve(5, 0.5);

        assert_eq!(mutations, 0);
        assert_eq!(lib.templates.len(), before);
    }

    #[test]
    fn test_evolve_skips_low_evals() {
        let mut lib = TemplateLibrary::new();

        // Only 2 evals (below min_evals=5)
        lib.record_reward("default_standard", 0.1);
        lib.record_reward("default_standard", 0.1);

        let before = lib.templates.len();
        let mutations = lib.evolve(5, 0.5);

        assert_eq!(mutations, 0);
        assert_eq!(lib.templates.len(), before);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("templates.json");

        let mut lib = TemplateLibrary::new();
        lib.record_reward("default_minimal", 0.7);
        lib.record_reward("default_standard", 0.3);
        lib.save(&path).expect("save");

        let loaded = TemplateLibrary::load(&path).expect("load");
        assert_eq!(loaded.templates.len(), lib.templates.len());
        assert_eq!(loaded.total_evals, lib.total_evals);

        let t = loaded
            .templates
            .iter()
            .find(|t| t.id == "default_minimal")
            .unwrap();
        assert_eq!(t.eval_count, 1);
        assert!((t.reward_sum - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_mutated_template_has_parent() {
        let mut lib = TemplateLibrary::new();

        for _ in 0..10 {
            lib.record_reward("default_full", 0.05);
        }

        lib.evolve(5, 0.5);

        let mutant = lib
            .templates
            .iter()
            .find(|t| t.parent_id.as_deref() == Some("default_full"))
            .expect("mutant with parent_id");

        assert_eq!(mutant.parent_id.as_deref(), Some("default_full"));
        assert_eq!(mutant.version, 2);
        assert!(mutant.id.contains("_v2"));
    }

    #[test]
    fn test_mutated_sections_rotated() {
        // Create a custom library where the index-0 template has multiple sections.
        // Index 0 gets Rotate mutation.
        let mut lib = TemplateLibrary {
            templates: vec![ContextTemplate {
                id: "multi".into(),
                version: 1,
                text: "{overview}\n{gotchas}\n{relations}\n{errors}".into(),
                sections: vec![
                    "overview".into(),
                    "gotchas".into(),
                    "relations".into(),
                    "errors".into(),
                ],
                reward_sum: 0.0,
                eval_count: 0,
                parent_id: None,
            }],
            total_evals: 0,
        };

        for _ in 0..10 {
            lib.record_reward("multi", 0.05);
        }

        lib.evolve(5, 0.5);

        let mutant = lib
            .templates
            .iter()
            .find(|t| t.parent_id.as_deref() == Some("multi"))
            .unwrap();

        // Index 0 → Rotate: [gotchas, relations, errors, overview]
        assert_eq!(
            mutant.sections,
            vec!["gotchas", "relations", "errors", "overview"]
        );
        assert_eq!(mutant.text, "{gotchas}\n{relations}\n{errors}\n{overview}");
    }

    #[test]
    fn test_record_reward_nonexistent_is_noop() {
        let mut lib = TemplateLibrary::new();
        lib.record_reward("nonexistent_template", 1.0);

        // Nothing should change
        assert_eq!(lib.total_evals, 0);
        for t in &lib.templates {
            assert_eq!(t.eval_count, 0);
        }
    }

    #[test]
    fn test_default_trait() {
        let lib = TemplateLibrary::default();
        assert_eq!(lib.templates.len(), 3);
        assert_eq!(lib.total_evals, 0);
    }

    #[test]
    fn test_len_and_is_empty() {
        let lib = TemplateLibrary::new();
        assert_eq!(lib.len(), 3);
        assert!(!lib.is_empty());

        let empty = TemplateLibrary {
            templates: vec![],
            total_evals: 0,
        };
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    // ── Mutation expansion tests ──────────────────────────────────────────

    #[test]
    fn test_mutation_drop_section() {
        // DropSection is mutation_types[1], so we need a template at index 1
        // to trigger it. default_standard is at index 1.
        let mut lib = TemplateLibrary::new();

        // Make default_standard (index 1) low-performing
        for _ in 0..10 {
            lib.record_reward("default_standard", 0.1);
        }
        // Keep others high
        for _ in 0..10 {
            lib.record_reward("default_minimal", 0.9);
            lib.record_reward("default_full", 0.9);
        }

        let before = lib.templates.len();
        let mutations = lib.evolve(5, 0.5);
        assert_eq!(mutations, 1);
        assert_eq!(lib.templates.len(), before + 1);

        let mutant = lib
            .templates
            .iter()
            .find(|t| t.parent_id.as_deref() == Some("default_standard"))
            .expect("mutant should exist");

        // default_standard has sections [gotchas, relations] -> drop last -> [gotchas]
        assert_eq!(mutant.sections, vec!["gotchas"]);
        assert_eq!(mutant.text, "{gotchas}");
    }

    #[test]
    fn test_mutation_add_section() {
        // AddSection is mutation_types[2], so we need a template at index 2.
        // default_full is at index 2 with sections [overview, gotchas, relations, errors].
        let mut lib = TemplateLibrary::new();

        for _ in 0..10 {
            lib.record_reward("default_full", 0.1);
        }
        for _ in 0..10 {
            lib.record_reward("default_minimal", 0.9);
            lib.record_reward("default_standard", 0.9);
        }

        lib.evolve(5, 0.5);

        let mutant = lib
            .templates
            .iter()
            .find(|t| t.parent_id.as_deref() == Some("default_full"))
            .expect("mutant should exist");

        // default_full already has [overview, gotchas, relations, errors].
        // First available section NOT in that list is "metrics".
        assert!(mutant.sections.contains(&"metrics".to_string()));
        assert_eq!(mutant.sections.len(), 5);
    }

    #[test]
    fn test_prune_removes_low_performers() {
        let mut lib = TemplateLibrary::new();

        // Give one template low reward with enough evals
        for _ in 0..20 {
            lib.record_reward("default_minimal", 0.1);
        }
        // Give others high reward
        for _ in 0..20 {
            lib.record_reward("default_standard", 0.9);
            lib.record_reward("default_full", 0.9);
        }

        assert_eq!(lib.len(), 3);
        lib.prune(10, 0.5);
        assert_eq!(lib.len(), 2);

        // default_minimal should be gone (avg 0.1 < 0.5 threshold)
        assert!(!lib.templates.iter().any(|t| t.id == "default_minimal"));
        // Others should remain
        assert!(lib.templates.iter().any(|t| t.id == "default_standard"));
        assert!(lib.templates.iter().any(|t| t.id == "default_full"));
    }

    #[test]
    fn test_prune_keeps_low_eval_templates() {
        let mut lib = TemplateLibrary::new();

        // Give low reward but only 2 evals (below min_evals=10)
        lib.record_reward("default_minimal", 0.1);
        lib.record_reward("default_minimal", 0.1);

        lib.prune(10, 0.5);
        // Should not be pruned — not enough evals to judge
        assert_eq!(lib.len(), 3);
    }

    #[test]
    fn test_library_size_bounded_after_mutations() {
        let mut lib = TemplateLibrary::new();

        // Make all templates low-performing
        for _ in 0..10 {
            lib.record_reward("default_minimal", 0.1);
            lib.record_reward("default_standard", 0.1);
            lib.record_reward("default_full", 0.1);
        }

        // Evolve adds mutants
        let mutations = lib.evolve(5, 0.5);
        assert_eq!(mutations, 3);
        assert_eq!(lib.len(), 6); // 3 originals + 3 mutants

        // Prune removes the low-performing originals
        lib.prune(5, 0.5);
        // Only the fresh mutants (eval_count=0, below min_evals) survive
        // plus any originals that meet threshold (none do)
        assert_eq!(lib.len(), 3, "only fresh mutants should remain after prune");
        for t in &lib.templates {
            assert_eq!(
                t.eval_count, 0,
                "surviving templates should be fresh mutants"
            );
        }
    }

    #[test]
    fn test_mutation_swap_separator() {
        // SwapSeparator is mutation_types[3], so template at index 3.
        // We need 4 low-performing templates. Start with defaults (3) + add one more.
        let mut lib = TemplateLibrary::new();

        // Add a 4th template at index 3
        lib.templates.push(ContextTemplate {
            id: "custom_pipe".into(),
            version: 1,
            text: "{gotchas}\n{relations}".into(),
            sections: vec!["gotchas".into(), "relations".into()],
            reward_sum: 0.0,
            eval_count: 0,
            parent_id: None,
        });

        // Make only index 3 low-performing
        for _ in 0..10 {
            lib.record_reward("custom_pipe", 0.1);
        }
        for _ in 0..10 {
            lib.record_reward("default_minimal", 0.9);
            lib.record_reward("default_standard", 0.9);
            lib.record_reward("default_full", 0.9);
        }

        lib.evolve(5, 0.5);

        let mutant = lib
            .templates
            .iter()
            .find(|t| t.parent_id.as_deref() == Some("custom_pipe"))
            .expect("mutant should exist");

        // Original uses "\n" (no "\n\n"), so swap should produce "\n\n"
        assert_eq!(mutant.text, "{gotchas}\n\n{relations}");
    }

    // ── load_or_default tests ─────────────────────────────────────────────

    #[test]
    fn test_load_or_default_missing_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("nonexistent.json");

        let lib = TemplateLibrary::load_or_default(&path);
        assert_eq!(lib.templates.len(), 3, "should return default library");
        assert_eq!(lib.total_evals, 0);
    }

    #[test]
    fn test_load_or_default_valid_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("templates.json");

        let mut lib = TemplateLibrary::new();
        lib.record_reward("default_minimal", 0.9);
        lib.save(&path).expect("save");

        let loaded = TemplateLibrary::load_or_default(&path);
        assert_eq!(loaded.total_evals, 1);
        let t = loaded
            .templates
            .iter()
            .find(|t| t.id == "default_minimal")
            .unwrap();
        assert_eq!(t.eval_count, 1);
    }

    #[test]
    fn test_load_or_default_corrupt_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("corrupt.json");
        std::fs::write(&path, "NOT VALID JSON!!!").expect("write corrupt");

        let lib = TemplateLibrary::load_or_default(&path);
        // Should fall back to default instead of panicking
        assert_eq!(lib.templates.len(), 3);
        assert_eq!(lib.total_evals, 0);
    }
}
