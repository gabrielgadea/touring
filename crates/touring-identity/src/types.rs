//! Entity Identity Schema — D5.1 of Touring v8 Master Plan S5.
//!
//! Core types for canonical entity naming across Touring's multi-crate architecture.
//! - [`EntityId`]: interned identifier (SmolStr-backed newtype)
//! - [`Entity`]: canonical name + crate of origin + criteria it satisfies
//! - [`Criterion`]: named rule for entity equality (exact, fuzzy, context-scoped)
//! - [`EntityRelation`]: directed link between two entities
//! - [`RelationKind`]: semantics of a relation (DerivedFrom, Refines, etc.)
//! - [`EntityCandidate`]: entity under evaluation during resolution
//! - [`MatchKind`]: confidence tier for resolution results

use std::fmt;

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Maximum length of an entity canonical name in bytes.
pub const MAX_CANONICAL_LEN: usize = 256;

/// Maximum number of criteria an entity may declare.
pub const MAX_CRITERIA_COUNT: usize = 32;

/// Interned entity identifier — stable across sessions and refactors.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(SmolStr);

impl EntityId {
    /// Constructs an [`EntityId`] from a string slice.
    ///
    /// ```
    /// use touring_identity::EntityId;
    /// let id = EntityId::from_str("touring-ast::CosineComputer");
    /// assert_eq!(id.as_str(), "touring-ast::CosineComputer");
    /// ```
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self(SmolStr::from(s))
    }

    /// Returns the raw string representation.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the crate namespace prefix, if any.
    ///
    /// ```
    /// use touring_identity::EntityId;
    /// let id = EntityId::from_str("touring-ast::CosineComputer");
    /// assert_eq!(id.crate_namespace(), Some("touring-ast"));
    ///
    /// let id2 = EntityId::from_str("PlainSymbol");
    /// assert_eq!(id2.crate_namespace(), None);
    /// ```
    pub fn crate_namespace(&self) -> Option<&str> {
        self.0.split_once("::").map(|(ns, _)| ns)
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for EntityId {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

/// Kind of entity — determines which resolution algorithm applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    /// A named function or method.
    Function,
    /// A struct, enum, union, or tuple struct.
    Type,
    /// A module or namespace.
    Module,
    /// A constant or static variable.
    Constant,
    /// A trait definition.
    Trait,
    /// A macro (by name or by bang).
    Macro,
    /// A file or path-backed symbol.
    File,
    /// A configuration key or setting.
    Config,
    /// An unclassified symbol.
    Unknown,
}

impl EntityKind {
    /// Returns the default confidence for an exact match of this kind.
    pub fn exact_confidence(self) -> f64 {
        match self {
            EntityKind::Unknown => 0.95,
            _ => 1.0,
        }
    }
}

/// A named criterion — the rule that makes two entities "the same".
///
/// Examples:
/// - `exact_name("CosineComputer")` — same exact full path
/// - `fuzzy_name("CosineComp", dist=2)` — edit distance ≤ 2
/// - `context_scoped("touring-ast::**::semantic_search", "CosineComputer")` — same symbol in same crate subtree
/// - `derived_from(other_id)` — this entity is a refactor of that one
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Criterion {
    /// Discriminating name.
    pub name: SmolStr,
    /// Human-readable description.
    pub description: SmolStr,
}

impl Criterion {
    /// Creates an exact-name criterion.
    pub fn exact_name(name: &str) -> Self {
        let s = SmolStr::from(name);
        Self {
            name: s.clone(),
            description: SmolStr::from(format!("exact name match: {name}")),
        }
    }

    /// Creates a fuzzy-name criterion with a maximum edit distance.
    pub fn fuzzy_name(name: &str, max_edit_distance: u8) -> Self {
        Self {
            name: SmolStr::from(format!("fuzzy:{name}")),
            description: SmolStr::from(format!("fuzzy match: {name} (max_ed={max_edit_distance})")),
        }
    }

    /// Creates a context-scoped criterion.
    pub fn context_scoped(crate_pattern: &str, symbol_name: &str) -> Self {
        Self {
            name: SmolStr::from(format!("ctx:{crate_pattern}::{symbol_name}")),
            description: SmolStr::from(format!(
                "context-scoped: {symbol_name} within {crate_pattern}"
            )),
        }
    }
}

/// Canonical entity — the primary record for a named thing in Touring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Stable identifier.
    pub id: EntityId,
    /// Canonical full name (e.g. `touring-ast::semantic_search::CosineComputer`).
    pub canonical_name: SmolStr,
    /// Kind of entity.
    pub kind: EntityKind,
    /// Crate of origin (e.g. `touring-ast`).
    pub crate_name: SmolStr,
    /// Criteria this entity satisfies.
    pub criteria: Vec<Criterion>,
    /// Optional source file path, if file-backed.
    pub source_path: Option<SmolStr>,
    /// Optional line number where the entity is defined.
    pub definition_line: Option<u32>,
    /// Optional markdown doc comment (first line).
    pub doc_summary: Option<SmolStr>,
    /// True if this entity was auto-seeded from the touring index (D5.8).
    /// Unconfirmed entities should be treated as candidates, not authoritative.
    pub auto_seeded: bool,
    /// True once the user has confirmed this entity via `touring entity confirm`.
    /// Only canonical entities appear in high-confidence resolution results.
    pub canonical: bool,
}

impl Entity {
    /// Constructs a minimal entity.
    pub fn new(id: EntityId, canonical_name: &str, kind: EntityKind, crate_name: &str) -> Self {
        Self {
            id,
            canonical_name: SmolStr::from(canonical_name),
            kind,
            crate_name: SmolStr::from(crate_name),
            criteria: Vec::new(),
            source_path: None,
            definition_line: None,
            doc_summary: None,
            auto_seeded: false,
            canonical: false,
        }
    }

    /// Adds a criterion.
    pub fn with_criterion(mut self, criterion: Criterion) -> Self {
        self.criteria.push(criterion);
        self
    }

    /// Adds source location metadata.
    pub fn with_source(mut self, path: &str, line: u32) -> Self {
        self.source_path = Some(SmolStr::from(path));
        self.definition_line = Some(line);
        self
    }

    /// Adds a doc summary.
    pub fn with_doc(mut self, summary: &str) -> Self {
        self.doc_summary = Some(SmolStr::from(summary));
        self
    }

    /// Marks this entity as auto-seeded from the touring index (D5.8).
    pub fn with_auto_seeded(mut self) -> Self {
        self.auto_seeded = true;
        self
    }

    /// Marks this entity as canonical (user-confirmed, D5.8).
    pub fn with_canonical(mut self) -> Self {
        self.canonical = true;
        self
    }

    /// Returns `true` if the canonical name length is within the limit.
    pub fn is_valid(&self) -> bool {
        self.canonical_name.len() <= MAX_CANONICAL_LEN && self.criteria.len() <= MAX_CRITERIA_COUNT
    }
}

/// Kind of directed relation between two entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// This entity was derived from the other (e.g. rename, extract).
    DerivedFrom,
    /// This entity refines the contract of the other (e.g. impl specialize).
    Refines,
    /// This entity supersedes the other (replacement).
    Supersedes,
    /// This entity is semantically equivalent to the other.
    Equivalent,
    /// See also — informational link.
    SeeAlso,
    /// This entity is a wrapper around the other.
    Wraps,
}

/// Directed link between two entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRelation {
    /// The subject entity.
    pub from: EntityId,
    /// The object entity.
    pub to: EntityId,
    /// Semantic kind of the relation.
    pub kind: RelationKind,
    /// Optional natural-language justification.
    pub justification: Option<SmolStr>,
}

impl EntityRelation {
    /// Creates a new relation.
    pub fn new(from: EntityId, to: EntityId, kind: RelationKind) -> Self {
        Self {
            from,
            to,
            kind,
            justification: None,
        }
    }

    /// Creates a relation with a justification.
    pub fn with_justification(mut self, text: &str) -> Self {
        self.justification = Some(SmolStr::from(text));
        self
    }
}

/// Confidence tier for entity resolution results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    /// Exact match — same canonical name, confidence = 1.0.
    Exact,
    /// Context-scoped match — same name within a crate subtree, confidence ∈ [0.95, 0.99].
    ContextScoped,
    /// Fuzzy match — edit distance > 0 but ≤ configured threshold, confidence ∈ [0.7, 0.85].
    Fuzzy,
    /// Ambiguous — multiple candidates with similar scores.
    Ambiguous,
    /// No match found.
    NotFound,
}

impl MatchKind {
    /// Returns the typical confidence range lower bound for this kind.
    pub fn confidence_bound(&self) -> (f64, f64) {
        match self {
            MatchKind::Exact => (1.0, 1.0),
            MatchKind::ContextScoped => (0.95, 0.99),
            MatchKind::Fuzzy => (0.70, 0.85),
            MatchKind::Ambiguous => (0.60, 0.80),
            MatchKind::NotFound => (0.0, 0.0),
        }
    }
}

/// Result of resolving a name to a canonical entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    /// Resolved entity, if any.
    pub entity: Option<Entity>,
    /// Match kind.
    pub match_kind: MatchKind,
    /// Confidence score in [0.0, 1.0].
    pub confidence: f64,
    /// Why the match was selected / why it failed.
    pub reason: SmolStr,
}

impl Resolution {
    /// Constructs a not-found resolution.
    pub fn not_found(name: &str) -> Self {
        Self {
            entity: None,
            match_kind: MatchKind::NotFound,
            confidence: 0.0,
            reason: SmolStr::from(format!("no entity named '{name}' found")),
        }
    }

    /// Constructs an exact match resolution.
    pub fn exact(entity: Entity) -> Self {
        Self {
            entity: Some(entity),
            match_kind: MatchKind::Exact,
            confidence: 1.0,
            reason: SmolStr::from("exact name match"),
        }
    }

    /// Constructs a context-scoped match resolution.
    pub fn context_scoped(entity: Entity, confidence: f64) -> Self {
        let reason = format!("context-scoped match in crate '{}'", entity.crate_name);
        Self {
            entity: Some(entity),
            match_kind: MatchKind::ContextScoped,
            confidence,
            reason: SmolStr::from(reason),
        }
    }

    /// Constructs a fuzzy match resolution.
    pub fn fuzzy(entity: Entity, confidence: f64) -> Self {
        Self {
            entity: Some(entity),
            match_kind: MatchKind::Fuzzy,
            confidence,
            reason: SmolStr::from("fuzzy name match"),
        }
    }
}

/// Candidate entity during resolution (before final selection).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityCandidate {
    /// The entity being considered.
    pub entity: Entity,
    /// Match kind at this stage.
    pub match_kind: MatchKind,
    /// Confidence score at this stage.
    pub confidence: f64,
}

impl EntityCandidate {
    /// Promotes a candidate to a final resolution.
    pub fn resolve(self) -> Resolution {
        Resolution {
            entity: Some(self.entity),
            match_kind: self.match_kind,
            confidence: self.confidence,
            reason: SmolStr::from("resolved"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_id_from_str() {
        let id = EntityId::from_str("touring-ast::CosineComputer");
        assert_eq!(id.as_str(), "touring-ast::CosineComputer");
        assert_eq!(id.crate_namespace(), Some("touring-ast"));
    }

    #[test]
    fn entity_id_no_namespace() {
        let id = EntityId::from_str("PlainSymbol");
        assert_eq!(id.crate_namespace(), None);
    }

    #[test]
    fn criterion_exact_name() {
        let c = Criterion::exact_name("Foo");
        assert_eq!(c.name.as_str(), "Foo");
        assert!(c.description.as_str().contains("exact"));
    }

    #[test]
    fn criterion_fuzzy() {
        let c = Criterion::fuzzy_name("Bar", 2);
        assert!(c.name.as_str().starts_with("fuzzy:"));
        assert!(c.description.as_str().contains("max_ed=2"));
    }

    #[test]
    fn entity_new_and_with() {
        let e = Entity::new(
            EntityId::from_str("touring-ast::Foo"),
            "touring-ast::Foo",
            EntityKind::Function,
            "touring-ast",
        )
        .with_criterion(Criterion::exact_name("Foo"))
        .with_source("src/lib.rs", 42)
        .with_doc("Docs for Foo");

        assert!(e.is_valid());
        assert_eq!(e.criteria.len(), 1);
        assert_eq!(e.source_path.as_ref().unwrap().as_str(), "src/lib.rs");
        assert_eq!(e.definition_line.unwrap(), 42);
        assert_eq!(e.doc_summary.as_ref().unwrap().as_str(), "Docs for Foo");
    }

    #[test]
    fn entity_relation_with_justification() {
        let r = EntityRelation::new(
            EntityId::from_str("A"),
            EntityId::from_str("B"),
            RelationKind::Supersedes,
        )
        .with_justification("B was renamed to A");

        assert_eq!(r.kind, RelationKind::Supersedes);
        assert!(r.justification.is_some());
    }

    #[test]
    fn match_kind_confidence_bounds() {
        assert_eq!(MatchKind::Exact.confidence_bound(), (1.0, 1.0));
        let (lo, hi) = MatchKind::ContextScoped.confidence_bound();
        assert!(lo < hi && hi <= 1.0);
    }

    #[test]
    fn resolution_exact() {
        let e = Entity::new(
            EntityId::from_str("touring::Foo"),
            "touring::Foo",
            EntityKind::Type,
            "touring",
        );
        let r = Resolution::exact(e);
        assert!(r.entity.is_some());
        assert_eq!(r.match_kind, MatchKind::Exact);
        assert_eq!(r.confidence, 1.0);
    }

    #[test]
    fn resolution_not_found() {
        let r = Resolution::not_found("NonExistent");
        assert!(r.entity.is_none());
        assert_eq!(r.match_kind, MatchKind::NotFound);
        assert_eq!(r.confidence, 0.0);
    }

    #[test]
    fn entity_validity_length() {
        let long_name = "x".repeat(MAX_CANONICAL_LEN + 1);
        let e = Entity::new(
            EntityId::from_str(&long_name),
            &long_name,
            EntityKind::Unknown,
            "test",
        );
        assert!(!e.is_valid());
    }
}
