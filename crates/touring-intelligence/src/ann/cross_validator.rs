//! Cross Validator - Pln2 Module 3
//!
//! High-performance cross-validation for ANTT regulatory documents.
//!
//! # Features
//!
//! - Contradiction detection between documents
//! - Monetary value consistency verification
//! - Normative citation validation (Resolutions, Laws)
//! - Document chain gap identification
//! - Confidence score propagation (PageRank-like)

use crate::ann::validation_status::ValidationStatus;
use indexmap::IndexMap;
use once_cell::sync::Lazy;
use petgraph::Direction;
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ============================================================================
// NORMATIVE INDEX (Lazy Loaded)
// ============================================================================

static NORMATIVE_PATTERNS: Lazy<NormativePatterns> = Lazy::new(|| {
    NormativePatterns::new().unwrap_or_else(|_| panic!("Failed to compile normative patterns"))
});

struct NormativePatterns {
    resolution: Regex,
    law: Regex,
    decree: Regex,
    acordao: Regex,
}

impl NormativePatterns {
    fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            resolution: Regex::new(
                r"(?i)Resolu[çc][ãa]o\s*(?:ANTT\s*)?(?:n[º°]?\s*)?(\d{1,2}\.?\d{3})[/-](\d{4})",
            )?,
            law: Regex::new(r"(?i)Lei\s*(?:n[º°]?\s*)?(\d{1,2}\.?\d{3})[/-](\d{4})")?,
            decree: Regex::new(r"(?i)Decreto\s*(?:n[º°]?\s*)?(\d{1,2}\.?\d{3})[/-](\d{4})")?,
            acordao: Regex::new(
                r"(?i)Ac[óo]rd[ãa]o\s*(?:n[º°]?\s*)?(\d{1,4})[/-](\d{4})(?:\s*-?\s*TCU)?",
            )?,
        })
    }
}

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// Type of assertion extracted from a document
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssertionType {
    /// A monetary amount (currency + value) extracted from the text.
    MonetaryValue,
    /// A percentage figure.
    Percentage,
    /// A date reference.
    Date,
    /// A citation of a normative act (Resolution, Law, Decree, ruling).
    NormativeCitation,
    /// A reference to another document.
    DocumentCitation,
    /// A plain factual statement.
    FactualClaim,
    /// A concluding statement or finding.
    Conclusion,
    /// A numeric calculation or derivation.
    Calculation,
}

/// Normalized value for comparison
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NormalizedValue {
    /// A monetary amount with its currency code.
    Money {
        /// Numeric amount.
        amount: f64,
        /// Currency code (e.g. `BRL`).
        currency: String,
    },
    /// A percentage value.
    Percent(f64),
    /// A date in normalized string form.
    Date(String),
    /// A normative reference identified by type and number.
    Reference {
        /// Reference category (e.g. `Lei`, `Resolução`).
        type_name: String,
        /// Reference number (e.g. `14.133/2021`).
        number: String,
    },
    /// A bare numeric value.
    Numeric(f64),
}

impl NormalizedValue {
    /// Compare two normalized values, returning similarity [0, 1]
    pub fn similarity(&self, other: &Self) -> f64 {
        match (self, other) {
            (
                NormalizedValue::Money {
                    amount: a,
                    currency: ca,
                },
                NormalizedValue::Money {
                    amount: b,
                    currency: cb,
                },
            ) => {
                if ca != cb {
                    return 0.0;
                }
                let max_val = a.abs().max(b.abs());
                if max_val < 0.01 {
                    return 1.0;
                }
                let diff = ((a - b) / max_val).abs();
                (1.0 - diff).max(0.0)
            }
            (NormalizedValue::Percent(a), NormalizedValue::Percent(b)) => {
                let diff = (a - b).abs();
                if diff < 0.01 {
                    1.0
                } else {
                    (1.0 - diff / 100.0).max(0.0)
                }
            }
            (NormalizedValue::Date(a), NormalizedValue::Date(b)) if a == b => 1.0,
            (
                NormalizedValue::Reference {
                    type_name: ta,
                    number: na,
                },
                NormalizedValue::Reference {
                    type_name: tb,
                    number: nb,
                },
            ) if ta.to_lowercase() == tb.to_lowercase() && na == nb => 1.0,
            (NormalizedValue::Numeric(a), NormalizedValue::Numeric(b)) => {
                let max_val = a.abs().max(b.abs());
                if max_val < 0.01 {
                    return 1.0;
                }
                let diff = ((a - b) / max_val).abs();
                (1.0 - diff).max(0.0)
            }
            _ => 0.0,
        }
    }
}

/// Assertion extracted from a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assertion {
    /// Unique identifier of this assertion.
    pub id: String,
    /// Category of the assertion.
    pub assertion_type: AssertionType,
    /// Raw textual content of the assertion.
    pub content: String,
    /// Parsed/normalized value, when one could be extracted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_value: Option<NormalizedValue>,
    /// Identifier of the document this assertion came from.
    pub source_document: String,
    /// Character span `(start, end)` of the assertion in its source.
    pub position: (usize, usize),
    /// Confidence assigned before cross-validation.
    pub initial_confidence: f64,
    /// Confidence after confidence propagation across the graph.
    pub validated_confidence: f64,
    /// Ids of assertions that support this one.
    pub supporting_assertions: Vec<String>,
    /// Ids of assertions that contradict this one.
    pub contradicting_assertions: Vec<String>,
}

impl Default for Assertion {
    fn default() -> Self {
        Self {
            id: String::new(),
            assertion_type: AssertionType::FactualClaim,
            content: String::new(),
            normalized_value: None,
            source_document: String::new(),
            position: (0, 0),
            initial_confidence: 0.5,
            validated_confidence: 0.0,
            supporting_assertions: Vec::new(),
            contradicting_assertions: Vec::new(),
        }
    }
}

/// Relationship type for evidence
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EvidenceRelationship {
    /// The two assertions are identical.
    Identical,
    /// The assertions agree without being identical.
    Consistent,
    /// One assertion logically implies the other.
    Implies,
    /// One assertion references the other.
    References,
}

/// Evidence supporting an assertion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// Id of the assertion providing this evidence.
    pub source_assertion_id: String,
    /// Similarity between the supported assertion and the source, in `[0, 1]`.
    pub similarity_score: f64,
    /// Nature of the relationship between the assertions.
    pub relationship: EvidenceRelationship,
}

/// Type of contradiction detected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContradictionType {
    /// Numeric values disagree by `diff_percent` percent.
    ValueMismatch {
        /// Relative difference between the two values, in percent.
        diff_percent: f64,
    },
    /// Two dates conflict.
    DateConflict,
    /// Statements are logically incompatible.
    LogicalContradiction,
    /// Normative references conflict.
    ReferenceConflict,
}

impl ContradictionType {
    /// Render this contradiction type as a short human-readable label.
    pub fn to_string_repr(&self) -> String {
        match self {
            ContradictionType::ValueMismatch { diff_percent } => {
                format!("ValueMismatch({:.1}%)", diff_percent)
            }
            ContradictionType::DateConflict => "DateConflict".to_string(),
            ContradictionType::LogicalContradiction => "LogicalContradiction".to_string(),
            ContradictionType::ReferenceConflict => "ReferenceConflict".to_string(),
        }
    }
}

/// Contradiction between two assertions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    /// Id of the first conflicting assertion.
    pub assertion_a_id: String,
    /// Id of the second conflicting assertion.
    pub assertion_b_id: String,
    /// Kind of contradiction detected.
    pub contradiction_type: ContradictionType,
    /// Severity of the contradiction, in `[0, 1]`.
    pub severity: f64,
    /// Human-readable explanation of the contradiction.
    pub description: String,
}

/// Type of gap in document chain
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GapType {
    /// A referenced document is absent from the chain.
    MissingReference,
    /// A calculation lacks supporting verification.
    UnverifiedCalculation,
    /// A required approval step is missing.
    MissingApproval,
    /// The document chain is incomplete.
    IncompleteChain,
}

/// Gap identified in document chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gap {
    /// Category of the gap.
    pub gap_type: GapType,
    /// Human-readable explanation of the gap.
    pub description: String,
    /// Identifier of the document expected to fill the gap, if known.
    pub expected_document: Option<String>,
    /// Severity of the gap, in `[0, 1]`.
    pub severity: f64,
}

/// Result of cross-validation for a single assertion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Id of the assertion this result describes.
    pub assertion_id: String,
    /// Final validation status of the assertion.
    pub status: ValidationStatus,
    /// Confidence after cross-validation, in `[0, 1]`.
    pub confidence: f64,
    /// Evidence found supporting the assertion.
    pub supporting_evidence: Vec<Evidence>,
    /// Contradictions found against the assertion.
    pub contradictions: Vec<Contradiction>,
    /// Gaps detected in the assertion's document chain.
    pub gaps: Vec<Gap>,
}

// ============================================================================
// GRAPH STRUCTURES
// ============================================================================

/// Weight of an edge in the consistency graph, encoding how one assertion
/// relates to another.
#[derive(Debug, Clone)]
pub enum EdgeWeight {
    /// Source supports target with the given strength in `[0, 1]`.
    Supports {
        /// Strength of the support.
        strength: f64,
    },
    /// Source contradicts target with the given severity in `[0, 1]`.
    Contradicts {
        /// Severity of the contradiction.
        severity: f64,
    },
    /// Source references target.
    References,
    /// Source implies target.
    Implies,
}

/// Consistency graph for assertions and their relationships
pub(crate) struct ConsistencyGraph {
    graph: DiGraph<Assertion, EdgeWeight>,
    node_index: IndexMap<String, NodeIndex>,
}

impl std::fmt::Debug for ConsistencyGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsistencyGraph")
            .field("node_count", &self.graph.node_count())
            .field("edge_count", &self.graph.edge_count())
            .finish()
    }
}

impl ConsistencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_index: IndexMap::new(),
        }
    }

    pub(crate) fn add_assertion(&mut self, assertion: Assertion) -> NodeIndex {
        let id = assertion.id.clone();
        let idx = self.graph.add_node(assertion);
        self.node_index.insert(id, idx);
        idx
    }

    pub(crate) fn add_relation(&mut self, from: &str, to: &str, weight: EdgeWeight) {
        if let (Some(&from_idx), Some(&to_idx)) =
            (self.node_index.get(from), self.node_index.get(to))
        {
            self.graph.add_edge(from_idx, to_idx, weight);
        }
    }

    pub(crate) fn get_node(&self, id: &str) -> Option<&Assertion> {
        self.node_index.get(id).map(|&idx| &self.graph[idx])
    }

    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut Assertion> {
        self.node_index.get(id).map(|&idx| &mut self.graph[idx])
    }

    // EC60: test-only helper — cargo check does not compile #[cfg(test)] modules,
    // so the test caller at line 899 is invisible to the dead_code lint. Annotation
    // is intentional: the method is kept for unit test introspection.
    #[allow(dead_code)]
    pub fn get_index(&self, id: &str) -> Option<NodeIndex> {
        self.node_index.get(id).copied()
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn find_contradiction_cycles(&self) -> Vec<Vec<String>> {
        let sccs = kosaraju_scc(&self.graph);

        sccs.into_iter()
            .filter(|scc| scc.len() > 1)
            .filter(|scc| self.has_contradiction_in_scc(scc))
            .map(|scc| {
                scc.into_iter()
                    .map(|idx| self.graph[idx].id.clone())
                    .collect()
            })
            .collect()
    }

    fn has_contradiction_in_scc(&self, scc: &[NodeIndex]) -> bool {
        let scc_set: HashSet<_> = scc.iter().copied().collect();

        for &node in scc {
            for edge in self.graph.edges(node) {
                if scc_set.contains(&edge.target())
                    && matches!(edge.weight(), EdgeWeight::Contradicts { .. })
                {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn propagate_confidence(&mut self, iterations: usize, damping: f64) {
        let n = self.graph.node_count();
        if n == 0 {
            return;
        }

        for _ in 0..iterations {
            let mut new_confidences = vec![0.0; n];

            for node_idx in self.graph.node_indices() {
                let edges: Vec<_> = self
                    .graph
                    .edges_directed(node_idx, Direction::Incoming)
                    .collect();

                let initial = self.graph[node_idx].initial_confidence;
                let idx = node_idx.index();

                if edges.is_empty() {
                    if let Some(slot) = new_confidences.get_mut(idx) {
                        *slot = initial;
                    }
                    continue;
                }

                let incoming: f64 = edges
                    .iter()
                    .map(|edge| {
                        let source_idx = edge.source();
                        let source_confidence = self.graph[source_idx].validated_confidence;

                        match edge.weight() {
                            EdgeWeight::Supports { strength } => source_confidence * strength,
                            EdgeWeight::Contradicts { severity } => {
                                -source_confidence * severity * 0.5
                            }
                            EdgeWeight::References | EdgeWeight::Implies => source_confidence * 0.3,
                        }
                    })
                    .sum();

                if let Some(slot) = new_confidences.get_mut(idx) {
                    *slot = (1.0 - damping) * initial + damping * incoming.max(0.0);
                }
            }

            for node_idx in self.graph.node_indices() {
                if let Some(&conf) = new_confidences.get(node_idx.index()) {
                    self.graph[node_idx].validated_confidence = conf.clamp(0.0, 1.0);
                }
            }
        }

        let max_conf: f64 = self
            .graph
            .node_indices()
            .map(|idx| self.graph[idx].validated_confidence)
            .fold(0.0, f64::max);

        if max_conf > 1.0 {
            for node_idx in self.graph.node_indices() {
                self.graph[node_idx].validated_confidence /= max_conf;
            }
        }
    }

    pub(crate) fn get_edges(&self, id: &str) -> Vec<(String, &EdgeWeight)> {
        if let Some(&idx) = self.node_index.get(id) {
            self.graph
                .edges(idx)
                .map(|edge| (self.graph[edge.target()].id.clone(), edge.weight()))
                .collect()
        } else {
            Vec::new()
        }
    }
}

impl Default for ConsistencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// NORMATIVE INDEX
// ============================================================================

/// Index of known valid normative references
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct NormativeIndex {
    /// Known valid resolution numbers (e.g. `6.063/2025`).
    pub resolutions: HashSet<String>,
    /// Known valid law numbers.
    pub laws: HashSet<String>,
    /// Known valid decree numbers.
    pub decrees: HashSet<String>,
    /// Known valid TCU ruling (acórdão) numbers.
    pub acordaos: HashSet<String>,
}

impl NormativeIndex {
    pub(crate) fn with_defaults() -> Self {
        let mut idx = Self::default();

        for res in [
            "5.818/2018",
            "5.950/2021",
            "5.976/2022",
            "6.000/2022",
            "6.002/2022",
            "6.003/2022",
            "6.025/2023",
            "6.032/2023",
            "6.033/2023",
            "6.048/2024",
            "6.053/2024",
            "6.054/2024",
            "6.063/2025",
        ] {
            idx.resolutions.insert(res.to_string());
        }

        for law in [
            "8.987/1995",
            "10.233/2001",
            "13.848/2019",
            "9.784/1999",
            "8.666/1993",
            "14.133/2021",
        ] {
            idx.laws.insert(law.to_string());
        }

        idx
    }

    /// Return `true` if a normative act of `type_name` with `number` is known.
    pub fn exists(&self, type_name: &str, number: &str) -> bool {
        let normalized_type = type_name.to_lowercase();
        let normalized_number = number.replace(['.', ' '], "");

        let set = match normalized_type.as_str() {
            s if s.contains("resol") => &self.resolutions,
            s if s.contains("lei") => &self.laws,
            s if s.contains("decreto") => &self.decrees,
            s if s.contains("acord") || s.contains("acórd") => &self.acordaos,
            _ => return false,
        };

        set.iter()
            .any(|entry| entry.replace(['.', ' '], "") == normalized_number)
    }

    /// Load a `NormativeIndex` from a JSON file, returning `None` on read or
    /// parse failure.
    pub fn load_from_file(path: &str) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Register a resolution number as known-valid.
    pub fn add_resolution(&mut self, number: &str) {
        self.resolutions.insert(number.to_string());
    }

    /// Register a law number as known-valid.
    pub fn add_law(&mut self, number: &str) {
        self.laws.insert(number.to_string());
    }
}

// ============================================================================
// CROSS VALIDATOR
// ============================================================================

/// Configuration for the cross validator
#[derive(Debug, Clone)]
pub struct ValidatorConfig {
    /// Relative tolerance below which two values are treated as equal.
    pub value_tolerance: f64,
    /// Minimum similarity for two assertions to be considered related.
    pub similarity_threshold: f64,
    /// Number of confidence-propagation iterations to run.
    pub propagation_iterations: usize,
    /// Damping factor applied during confidence propagation.
    pub damping_factor: f64,
    /// Confidence at or above which an assertion is marked confirmed.
    pub confirmed_threshold: f64,
    /// Confidence at or above which an assertion is marked supported.
    pub supported_threshold: f64,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            value_tolerance: 0.01,
            similarity_threshold: 0.80,
            propagation_iterations: 10,
            damping_factor: 0.85,
            confirmed_threshold: 0.80,
            supported_threshold: 0.60,
        }
    }
}

/// Cross-validator for detecting contradictions and propagating confidence
#[derive(Debug)]
pub struct CrossValidator {
    graph: ConsistencyGraph,
    config: ValidatorConfig,
    normative_index: NormativeIndex,
}

impl CrossValidator {
    /// Create a cross-validator with default config and the built-in
    /// normative index.
    pub fn new() -> Self {
        Self {
            graph: ConsistencyGraph::new(),
            config: ValidatorConfig::default(),
            normative_index: NormativeIndex::with_defaults(),
        }
    }

    /// Create a cross-validator with a custom `ValidatorConfig`.
    pub fn with_config(config: ValidatorConfig) -> Self {
        Self {
            graph: ConsistencyGraph::new(),
            config,
            normative_index: NormativeIndex::with_defaults(),
        }
    }

    /// Cross-validate a set of assertions: build the consistency graph,
    /// propagate confidence, and return a `ValidationResult` per assertion.
    pub fn validate(&mut self, assertions: Vec<Assertion>) -> Vec<ValidationResult> {
        self.graph = ConsistencyGraph::new();

        for mut assertion in assertions.clone() {
            assertion.validated_confidence = assertion.initial_confidence;
            self.graph.add_assertion(assertion);
        }

        self.detect_relations(&assertions);
        self.validate_normative_citations(&assertions);

        self.graph.propagate_confidence(
            self.config.propagation_iterations,
            self.config.damping_factor,
        );

        // EC51: First caller of find_contradiction_cycles() + node_count() —
        // surfaces cyclic contradictions (stronger signal than pairwise contradictions).
        // Kosaraju SCC detects groups where assertions mutually contradict each other.
        let cycles = self.graph.find_contradiction_cycles();
        if !cycles.is_empty() {
            tracing::debug!(
                target: "touring_intelligence::ann",
                node_count = self.graph.node_count(),
                cycle_count = cycles.len(),
                "cross_validator: contradiction cycles in assertion graph"
            );
        }

        assertions.iter().map(|a| self.build_result(a)).collect()
    }

    fn detect_relations(&mut self, assertions: &[Assertion]) {
        for i in 0..assertions.len() {
            for j in (i + 1)..assertions.len() {
                let (Some(a), Some(b)) = (assertions.get(i), assertions.get(j)) else {
                    continue;
                };

                if a.assertion_type != b.assertion_type {
                    continue;
                }

                match (&a.normalized_value, &b.normalized_value) {
                    (
                        Some(NormalizedValue::Money {
                            amount: va,
                            currency: ca,
                        }),
                        Some(NormalizedValue::Money {
                            amount: vb,
                            currency: cb,
                        }),
                    ) => {
                        if ca != cb {
                            continue;
                        }
                        let max_val = va.abs().max(vb.abs());
                        if max_val < 0.01 {
                            continue;
                        }
                        let diff = ((va - vb) / max_val).abs();

                        if diff <= self.config.value_tolerance {
                            self.graph.add_relation(
                                &a.id,
                                &b.id,
                                EdgeWeight::Supports {
                                    strength: 1.0 - diff,
                                },
                            );
                            self.graph.add_relation(
                                &b.id,
                                &a.id,
                                EdgeWeight::Supports {
                                    strength: 1.0 - diff,
                                },
                            );
                        } else if diff > 0.10 {
                            self.graph.add_relation(
                                &a.id,
                                &b.id,
                                EdgeWeight::Contradicts {
                                    severity: diff.min(1.0),
                                },
                            );
                            self.graph.add_relation(
                                &b.id,
                                &a.id,
                                EdgeWeight::Contradicts {
                                    severity: diff.min(1.0),
                                },
                            );
                        }
                    }
                    (Some(NormalizedValue::Percent(pa)), Some(NormalizedValue::Percent(pb))) => {
                        let diff = (pa - pb).abs();
                        if diff <= 0.5 {
                            self.graph.add_relation(
                                &a.id,
                                &b.id,
                                EdgeWeight::Supports {
                                    strength: 1.0 - diff / 100.0,
                                },
                            );
                            self.graph.add_relation(
                                &b.id,
                                &a.id,
                                EdgeWeight::Supports {
                                    strength: 1.0 - diff / 100.0,
                                },
                            );
                        } else if diff > 5.0 {
                            self.graph.add_relation(
                                &a.id,
                                &b.id,
                                EdgeWeight::Contradicts {
                                    severity: (diff / 100.0).min(1.0),
                                },
                            );
                            self.graph.add_relation(
                                &b.id,
                                &a.id,
                                EdgeWeight::Contradicts {
                                    severity: (diff / 100.0).min(1.0),
                                },
                            );
                        }
                    }
                    (
                        Some(NormalizedValue::Reference {
                            type_name: ta,
                            number: na,
                        }),
                        Some(NormalizedValue::Reference {
                            type_name: tb,
                            number: nb,
                        }),
                    ) => {
                        if ta.to_lowercase() == tb.to_lowercase() && na == nb {
                            self.graph
                                .add_relation(&a.id, &b.id, EdgeWeight::References);
                            self.graph
                                .add_relation(&b.id, &a.id, EdgeWeight::References);
                        }
                    }
                    _ => {
                        if !a.content.is_empty() && !b.content.is_empty() {
                            let sim = self.text_similarity(&a.content, &b.content);
                            if sim >= self.config.similarity_threshold {
                                self.graph.add_relation(
                                    &a.id,
                                    &b.id,
                                    EdgeWeight::Supports { strength: sim },
                                );
                                self.graph.add_relation(
                                    &b.id,
                                    &a.id,
                                    EdgeWeight::Supports { strength: sim },
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn text_similarity(&self, a: &str, b: &str) -> f64 {
        let words_a: HashSet<&str> = a
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| !w.is_empty())
            .collect();

        let words_b: HashSet<&str> = b
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| !w.is_empty())
            .collect();

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }

    fn validate_normative_citations(&mut self, assertions: &[Assertion]) {
        for assertion in assertions {
            if assertion.assertion_type == AssertionType::NormativeCitation {
                if let Some(NormalizedValue::Reference { type_name, number }) =
                    &assertion.normalized_value
                {
                    let is_valid = self.normative_index.exists(type_name, number);

                    if let Some(node) = self.graph.get_node_mut(&assertion.id) {
                        if is_valid {
                            node.initial_confidence = node.initial_confidence.max(0.90);
                            node.validated_confidence = node.initial_confidence;
                        } else {
                            node.initial_confidence = 0.10;
                            node.validated_confidence = 0.10;
                        }
                    }
                }
            }
        }
    }

    fn build_result(&self, assertion: &Assertion) -> ValidationResult {
        let mut supporting_evidence = Vec::new();
        let mut contradictions = Vec::new();
        let mut confidence = assertion.initial_confidence;

        if let Some(node) = self.graph.get_node(&assertion.id) {
            confidence = node.validated_confidence;

            for (target_id, weight) in self.graph.get_edges(&assertion.id) {
                match weight {
                    EdgeWeight::Supports { strength } => {
                        let relationship = if *strength > 0.95 {
                            EvidenceRelationship::Identical
                        } else {
                            EvidenceRelationship::Consistent
                        };
                        supporting_evidence.push(Evidence {
                            source_assertion_id: target_id,
                            similarity_score: *strength,
                            relationship,
                        });
                    }
                    EdgeWeight::Contradicts { severity } => {
                        contradictions.push(Contradiction {
                            assertion_a_id: assertion.id.clone(),
                            assertion_b_id: target_id,
                            contradiction_type: ContradictionType::ValueMismatch {
                                diff_percent: severity * 100.0,
                            },
                            severity: *severity,
                            description: format!("Diferença de {:.1}% detectada", severity * 100.0),
                        });
                    }
                    EdgeWeight::References => {
                        supporting_evidence.push(Evidence {
                            source_assertion_id: target_id,
                            similarity_score: 1.0,
                            relationship: EvidenceRelationship::References,
                        });
                    }
                    EdgeWeight::Implies => {
                        supporting_evidence.push(Evidence {
                            source_assertion_id: target_id,
                            similarity_score: 0.8,
                            relationship: EvidenceRelationship::Implies,
                        });
                    }
                }
            }
        }

        let gaps = self.detect_gaps(assertion);
        let status = self.determine_status(&supporting_evidence, &contradictions, confidence);

        ValidationResult {
            assertion_id: assertion.id.clone(),
            status,
            confidence,
            supporting_evidence,
            contradictions,
            gaps,
        }
    }

    fn determine_status(
        &self,
        supporting: &[Evidence],
        contradictions: &[Contradiction],
        confidence: f64,
    ) -> ValidationStatus {
        if !contradictions.is_empty() {
            return ValidationStatus::Contradicted;
        }
        if supporting.len() >= 2 && confidence >= self.config.confirmed_threshold {
            return ValidationStatus::Confirmed;
        }
        if !supporting.is_empty() && confidence >= self.config.supported_threshold {
            return ValidationStatus::Supported;
        }
        ValidationStatus::Unverified
    }

    fn detect_gaps(&self, assertion: &Assertion) -> Vec<Gap> {
        let mut gaps = Vec::new();

        if assertion.assertion_type == AssertionType::Calculation
            && assertion.supporting_assertions.is_empty()
        {
            gaps.push(Gap {
                gap_type: GapType::UnverifiedCalculation,
                description: "Cálculo sem memória de cálculo anexa".to_string(),
                expected_document: Some("Memória de cálculo".to_string()),
                severity: 0.7,
            });
        }

        if assertion.assertion_type == AssertionType::DocumentCitation {
            if let Some(NormalizedValue::Reference { number, .. }) = &assertion.normalized_value {
                let has_reference = self.graph.node_index.keys().any(|id| id.contains(number));
                if !has_reference {
                    gaps.push(Gap {
                        gap_type: GapType::MissingReference,
                        description: format!("Documento {} citado mas não localizado", number),
                        expected_document: Some(number.clone()),
                        severity: 0.5,
                    });
                }
            }
        }

        gaps
    }

    /// Extract normative references from `text`, returning `(label, number,
    /// span)` tuples for each Resolution, Law, Decree, or TCU ruling found.
    pub fn extract_normative_references(
        &self,
        text: &str,
    ) -> Vec<(String, String, (usize, usize))> {
        let patterns = &*NORMATIVE_PATTERNS;

        let labeled_patterns: &[(&str, &Regex)] = &[
            ("Resolução", &patterns.resolution),
            ("Lei", &patterns.law),
            ("Decreto", &patterns.decree),
            ("Acórdão TCU", &patterns.acordao),
        ];

        let mut results = Vec::new();
        for &(label, pattern) in labeled_patterns {
            for cap in pattern.captures_iter(text) {
                if let (Some(num), Some(year)) = (cap.get(1), cap.get(2)) {
                    let full_match = cap.get(0).expect("capture group 0 always exists");
                    results.push((
                        label.to_string(),
                        format!("{}/{}", num.as_str(), year.as_str()),
                        (full_match.start(), full_match.end()),
                    ));
                }
            }
        }

        results
    }

    /// Borrow the validator's normative index.
    pub fn normative_index(&self) -> &NormativeIndex {
        &self.normative_index
    }

    /// Mutably borrow the validator's normative index (e.g. to register new
    /// acts).
    pub fn normative_index_mut(&mut self) -> &mut NormativeIndex {
        &mut self.normative_index
    }
}

impl Default for CrossValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate a batch of assertion groups in parallel
pub fn validate_batch(assertion_batches: Vec<Vec<Assertion>>) -> Vec<Vec<ValidationResult>> {
    assertion_batches
        .par_iter()
        .map(|assertions| {
            let mut validator = CrossValidator::new();
            validator.validate(assertions.clone())
        })
        .collect()
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normative_patterns_compile() {
        let _ = &*NORMATIVE_PATTERNS;
    }

    #[test]
    fn test_assertion_default() {
        let a = Assertion::default();
        assert_eq!(a.assertion_type, AssertionType::FactualClaim);
        assert!((a.initial_confidence - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_normalized_value_money_similarity() {
        let v1 = NormalizedValue::Money {
            amount: 100.0,
            currency: "BRL".into(),
        };
        let v2 = NormalizedValue::Money {
            amount: 100.5,
            currency: "BRL".into(),
        };
        let sim = v1.similarity(&v2);
        assert!(sim > 0.99, "Expected high similarity, got {}", sim);
    }

    #[test]
    fn test_normative_index_defaults() {
        let idx = NormativeIndex::with_defaults();
        assert!(idx.exists("Resolução", "5.950/2021"));
        assert!(idx.exists("Lei", "10.233/2001"));
        assert!(!idx.exists("Lei", "99.999/9999"));
    }

    #[test]
    fn test_text_similarity_identical() {
        let validator = CrossValidator::new();
        let sim = validator.text_similarity("hello world", "hello world");
        assert!((sim - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_extract_resolution() {
        let validator = CrossValidator::new();
        let refs = validator
            .extract_normative_references("Conforme Resolução ANTT nº 5.950/2021, artigo 10");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0, "Resolução");
    }

    #[test]
    fn test_extract_law() {
        let validator = CrossValidator::new();
        let refs = validator.extract_normative_references("Lei nº 10.233/2001 estabelece...");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0, "Lei");
    }

    #[test]
    fn test_consistency_graph_basic() {
        let mut graph = ConsistencyGraph::new();
        let a = Assertion {
            id: "a1".into(),
            ..Default::default()
        };
        let idx = graph.add_assertion(a);
        assert_eq!(graph.node_count(), 1);
        assert!(graph.get_node("a1").is_some());
        assert_eq!(graph.get_index("a1"), Some(idx));
    }
}
