//! Contracts — symbol and file-level preconditions for a `GeneratorPlan`.

use crate::core::score::NormalizedScore;
use crate::plan::schema::{CrateDep, Invariant};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Task kind for path boundary enforcement.
/// Each kind has different read/write permissions (principle of least privilege).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// Specification: docs/**/*. Write locked to docs/.
    #[default]
    Spec,
    /// Implementation: crates/**/src/**, tests/**. Write locked to code dirs.
    Impl,
    /// QA: tests/**, docs/qa/**. Write locked to test/qa dirs.
    QA,
    /// Documentation: docs/**, *.md. Write locked to docs/ and markdown files.
    Doc,
    /// Audit: read-only analysis. No write permissions.
    Audit,
    /// Hotfix: restricted emergency changes. Minimal blast radius.
    Hotfix,
}

/// Path boundaries for VGP Layer 5 enforcement.
/// Prefix-match semantics: "crates/foo" matches "crates/foo/src/lib.rs".
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct PathBoundaries {
    /// Which kind this boundary applies to.
    pub task_kind: TaskKind,
    /// Allowed read locations (glob patterns).
    pub read: Vec<String>,
    /// Allowed write locations (glob patterns).
    pub write: Vec<String>,
    /// Explicitly forbidden write locations (override write allowlist).
    pub forbidden_write: Vec<String>,
    /// Enforcement mode: fail closed (block) or warn only.
    pub enforcement: BoundaryEnforcement,
}

/// Enforcement mode for boundary violations.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryEnforcement {
    /// Block generation on boundary violation (fail closed).
    #[default]
    FailClosed,
    /// Warn but allow on boundary violation.
    WarnOnly,
}

/// All contracts that must pass during VGP verification.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct Contracts {
    /// Symbols that MUST exist in the index.
    #[schemars(length(max = 256))]
    pub symbols_must_exist: Vec<SymbolRef>,

    /// Symbols that MUST NOT exist (collision check).
    #[schemars(length(max = 128))]
    pub symbols_must_not_exist: Vec<SymbolRef>,

    /// Trait bounds that must be implemented.
    #[schemars(length(max = 32))]
    pub traits_implemented: Vec<String>,

    /// Pub symbols the plan intends to export.
    pub exports: Vec<String>,

    /// Crate dependencies required.
    pub dependencies: Vec<CrateDep>,

    /// Structural invariants to enforce.
    pub invariants: Vec<Invariant>,

    /// Files that MUST exist on disk before generation (UTF-8 path strings).
    pub files_must_exist: Vec<String>,

    /// Files that MUST NOT exist — prevents accidental overwrite without backup.
    pub files_must_not_exist: Vec<String>,

    /// Wiring integration requirements.
    pub wiring_requirements: WiringRequirements,

    /// Optional path boundaries for VGP Layer 5 (boundary) enforcement.
    /// When None, legacy behavior (no path enforcement).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_boundaries: Option<PathBoundaries>,

    /// Entity IDs that MUST exist and satisfy their admission criteria at generation time.
    /// VGP Layer 6 (Entity Registry) — D5.6 of Touring v8 Master Plan S5.
    /// Failure emits `output.rejected` with `error_code: ENTITY_VIOLATION`.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub entities_must_exist: Vec<EntityIdRef>,
}

/// Entity ID reference for VGP Layer 6 (Entity Registry) contract.
/// Corresponds to `touring_identity::EntityId` — lightweight string-based
/// identifier used in generator plans without pulling the full type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct EntityIdRef(String);

impl EntityIdRef {
    /// Construct from a string slice.
    #[inline]
    #[must_use]
    pub fn new(s: &str) -> Self {
        Self(s.to_string())
    }

    /// Returns the raw string.
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for EntityIdRef {
    #[inline]
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for EntityIdRef {
    #[inline]
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for EntityIdRef {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Contracts {
    /// Returns a synthetic `plan_id` for tracing.
    #[must_use]
    pub fn plan_id(&self) -> Uuid {
        Uuid::nil()
    }

    /// Returns true if all contract lists are empty (no-op contracts).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols_must_exist.is_empty()
            && self.symbols_must_not_exist.is_empty()
            && self.files_must_exist.is_empty()
            && self.invariants.is_empty()
    }
}

/// Wiring integration requirements for generated symbols.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct WiringRequirements {
    /// Generated pub symbols must have at least N consumers.
    pub min_consumers_per_export: u8,
    /// Minimum integration score from `touring-analysis`.
    pub min_integration_score: NormalizedScore,
    /// Explicit consumer hints (symbols that should use the generated output).
    pub consumer_hints: Vec<SymbolRef>,
}

/// A reference to a symbol in the codebase.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SymbolRef {
    /// Symbol name (may include generic params, e.g. `Vec<T>`).
    #[schemars(length(min = 1, max = 256))]
    pub name: String,
    /// Optional crate scope filter.
    pub crate_name: Option<String>,
    /// Optional module path filter (e.g. `crate::plan::contracts`).
    pub module_path: Option<String>,
    /// Disambiguation hint for overloaded symbols.
    pub definition_hint: Option<DefinitionHint>,
}

impl SymbolRef {
    /// Construct a minimal `SymbolRef` with only a name.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            crate_name: None,
            module_path: None,
            definition_hint: None,
        }
    }

    /// Construct a `SymbolRef` scoped to a crate.
    #[must_use]
    pub fn in_crate(name: impl Into<String>, crate_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            crate_name: Some(crate_name.into()),
            module_path: None,
            definition_hint: None,
        }
    }
}

/// Disambiguation hint for symbols with multiple definitions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum DefinitionHint {
    /// Select the first definition encountered (no further disambiguation).
    FirstDefinition,
    /// Disambiguate by the named trait the symbol is implemented for.
    TraitImpl(String),
    /// Disambiguate by the named generic parameter.
    GenericParam(String),
    /// File + line number (last resort — use symbolic hints when possible).
    FileLine {
        /// Path of the file containing the definition.
        file: String,
        /// 1-based line number of the definition within the file.
        line: u32,
    },
}
