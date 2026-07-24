# RFC-004: Entity Identity Registry

**Status**: Active
**Type**: Specification
**Layer**: ESAA / S5 / VGP Layer 6
**Author**: TACO (Constitution v8.0 Draft)
**Date**: 2026-05-09
**Version**: 1.0.0

---

## 1. Context and Motivation

Touring operates across a multi-crate Rust workspace where thousands of symbols
(functions, structs, traits, macros, modules) are tracked. Without a canonical
identity system, symbol references become ambiguous: `Foo` in `crate_a` is not
`Foo` in `crate_b`. A scouter might report an opportunity for `AcoPheromone`
without noting that two homonymous `AcoPheromone` types exist in different crates.

VGP Layer 6 (Entity Registry) establishes a canonical identity schema that:

1. Assigns a stable `EntityId` (SmolStr-backed interned string) to every tracked
   symbol, immune to rename refactors
2. Tracks the **criteria** that make two entities "the same" — exact name,
   fuzzy name with edit distance threshold, or context-scoped within a crate
   subtree
3. Distinguishes between **auto-seeded** entities (from touring index, unconfirmed)
   and **canonical** entities (user-confirmed via `touring entity confirm`)
4. Supports a resolution algorithm that classifies match confidence into five
   tiers (`Exact`, `ContextScoped`, `Fuzzy`, `Ambiguous`, `NotFound`)
5. Records directed relations between entities (`DerivedFrom`, `Supersedes`,
   `Equivalent`, `Wraps`, etc.) for cross-reference and impact analysis

**Relation to S5**: This RFC formalizes the entity identity schema described in
D5.1 of the v8 master plan, implemented in `crates/touring-identity/src/types.rs`.
The entity registry is the ground truth for symbol identity across all TACO
subagents — scouter, architect, engineer, auditor, and scriber all reference the
same entity canonical names.

---

## 2. Core Types

### 2.1 EntityId — Interned Identifier

```rust
// touring-identity/src/types.rs:23-58
pub struct EntityId(SmolStr);

impl EntityId {
    pub fn from_str(s: &str) -> Self { Self(SmolStr::from(s)) }
    pub fn as_str(&self) -> &str { self.0.as_str() }
    pub fn crate_namespace(&self) -> Option<&str> { self.0.split_once("::").map(|(ns, _)| ns) }
}
```

**Canonical name format**: `crate_name::module_path::symbol_name`

Examples:
- `touring-ast::semantic_search::CosineComputer` — struct in touring-ast crate
- `touring-hooks::pre_read::PreReadContext` — struct in touring-hooks
- `touring-core::profile::Profiler` — type in touring-core

**Invariant**: `as_str()` returns the full interned string. There is no
validation of format at construction time — the entity canonical name is
established by the entity itself, not by `EntityId`. `crate_namespace()` parses
the first `::`-delimited segment to extract the crate of origin.

```rust
// types.rs:55-57
pub fn crate_namespace(&self) -> Option<&str> {
    self.0.split_once("::").map(|(ns, _)| ns)
}

#[test]
fn entity_id_crate_namespace() {
    let id = EntityId::from_str("touring-ast::semantic_search::CosineComputer");
    assert_eq!(id.crate_namespace(), Some("touring-ast"));
    let id2 = EntityId::from_str("PlainSymbol");
    assert_eq!(id2.crate_namespace(), None); // no namespace
}
```

### 2.2 EntityKind — Kind of Entity

```rust
// touring-identity/src/types.rs:72-94
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Function,  // named function or method
    Type,     // struct, enum, union, tuple struct
    Module,   // module or namespace
    Constant, // constant or static variable
    Trait,    // trait definition
    Macro,    // macro by name or by bang
    File,     // file or path-backed symbol
    Config,   // configuration key or setting
    Unknown,  // unclassified symbol
}

impl EntityKind {
    /// Default confidence for an exact match of this kind.
    pub fn exact_confidence(self) -> f64 {
        match self {
            EntityKind::Unknown => 0.95, // Unknown has lower default — more uncertainty
            _ => 1.0,
        }
    }
}
```

`Unknown` is used for symbols that cannot be classified into any of the other
eight kinds. Its `exact_confidence()` returns `0.95` instead of `1.0` to reflect
the uncertainty inherent in classifying an unknown kind as an exact match.

### 2.3 Criterion — Named Rule for Entity Equality

A `Criterion` is the rule that makes two entities "the same". An entity can
satisfy multiple criteria simultaneously — this enables multi-mode resolution
(exact name first, then fuzzy fallback, then context-scoped).

```rust
// touring-identity/src/types.rs:106-150
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Criterion {
    pub name: SmolStr,         // discriminating name
    pub description: SmolStr,    // human-readable description
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
            description: SmolStr::from(format!(
                "fuzzy match: {name} (max_ed={max_edit_distance})"
            )),
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
```

#### Criterion Examples

| Criterion | Name | Description |
|-----------|------|-------------|
| `Criterion::exact_name("CosineComputer")` | `"CosineComputer"` | exact name match |
| `Criterion::fuzzy_name("CosineComp", 2)` | `"fuzzy:CosineComp"` | fuzzy match, edit distance ≤ 2 |
| `Criterion::context_scoped("touring-ast::**", "CosineComputer")` | `"ctx:touring-ast::**::CosineComputer"` | same symbol within touring-ast subtree |

The fuzzy name format uses the prefix `"fuzzy:"` and stores the configured
`max_edit_distance` in the description (not the name). The context-scoped
criterion uses `"ctx:"` prefix and embeds both the crate pattern and the symbol
name.

### 2.4 Entity — Canonical Record

```rust
// touring-identity/src/types.rs:152-237
pub struct Entity {
    pub id: EntityId,                               // stable identifier
    pub canonical_name: SmolStr,                    // e.g. "touring-ast::semantic_search::CosineComputer"
    pub kind: EntityKind,                           // kind of entity
    pub crate_name: SmolStr,                       // e.g. "touring-ast"
    pub criteria: Vec<Criterion>,                  // rules this entity satisfies
    pub source_path: Option<SmolStr>,             // file path where defined
    pub definition_line: Option<u32>,              // line number of definition
    pub doc_summary: Option<SmolStr>,              // first line of doc comment
    pub auto_seeded: bool,                        // true if from touring index (D5.8)
    pub canonical: bool,                           // true if user-confirmed via `touring entity confirm`
}
```

**Auto-seeding (D5.8)**: When `auto_seeded = true`, the entity was
automatically seeded from the touring symbol index. Unconfirmed entities should
be treated as candidates, not authoritative. An entity becomes authoritative
only when `canonical = true` (user confirmed via `touring entity confirm`).

**Canonical confirmation (D5.8)**: Only canonical entities appear in
high-confidence resolution results. The scriber uses `canonical` to determine
whether to document a symbol as "verified_existing" or "planned_future".

#### Builder Pattern

```rust
impl Entity {
    pub fn new(
        id: EntityId,
        canonical_name: &str,
        kind: EntityKind,
        crate_name: &str,
    ) -> Self { ... }

    pub fn with_criterion(mut self, criterion: Criterion) -> Self { ... }
    pub fn with_source(mut self, path: &str, line: u32) -> Self { ... }
    pub fn with_doc(mut self, summary: &str) -> Self { ... }
    pub fn with_auto_seeded(mut self) -> Self { ... }
    pub fn with_canonical(mut self) -> Self { ... }

    /// Returns true if canonical name length is within the limit (MAX_CANONICAL_LEN = 256).
    pub fn is_valid(&self) -> bool {
        self.canonical_name.len() <= MAX_CANONICAL_LEN
            && self.criteria.len() <= MAX_CRITERIA_COUNT
    }
}
```

### 2.5 Limits

```rust
// touring-identity/src/types.rs:17-21
pub const MAX_CANONICAL_LEN: usize = 256;   // bytes
pub const MAX_CRITERIA_COUNT: usize = 32;  // max criteria per entity
```

---

## 3. Entity Relations

### 3.1 RelationKind — Semantics of a Directed Link

```rust
// touring-identity/src/types.rs:239-255
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    DerivedFrom,  // this entity was derived from the other (rename, extract)
    Refines,     // this entity refines the contract of the other
    Supersedes,  // this entity is a replacement for the other
    Equivalent,  // this entity is semantically equivalent to the other
    SeeAlso,     // informational link
    Wraps,       // this entity is a wrapper around the other
}
```

### 3.2 EntityRelation — Directed Link

```rust
// touring-identity/src/types.rs:257-286
pub struct EntityRelation {
    pub from: EntityId,                          // subject entity
    pub to: EntityId,                            // object entity
    pub kind: RelationKind,                       // semantic kind of the relation
    pub justification: Option<SmolStr>,          // natural-language justification
}

impl EntityRelation {
    pub fn new(from: EntityId, to: EntityId, kind: RelationKind) -> Self { ... }
    pub fn with_justification(mut self, text: &str) -> Self { ... }
}
```

**Example**: A refactor that renamed `OldCalculator` to `NewCalculator` creates:

```rust
EntityRelation::new(
    EntityId::from_str("touring-ast::NewCalculator"),
    EntityId::from_str("touring-ast::OldCalculator"),
    RelationKind::Supersedes,
).with_justification("OldCalculator was renamed to NewCalculator in the v8 refactor")
```

This allows scouter and architect to trace symbol lineage across renames.

---

## 4. MatchKind and Resolution

### 4.1 MatchKind — Confidence Tier

```rust
// touring-identity/src/types.rs:288-315
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Exact,           // same canonical name, confidence = 1.0
    ContextScoped,   // same name within a crate subtree, confidence ∈ [0.95, 0.99]
    Fuzzy,           // edit distance > 0 but ≤ configured threshold, confidence ∈ [0.70, 0.85]
    Ambiguous,       // multiple candidates with similar scores
    NotFound,        // no match found
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
```

### 4.2 Resolution — Outcome of Name Resolution

```rust
// touring-identity/src/types.rs:317-374
pub struct Resolution {
    pub entity: Option<Entity>,  // resolved entity, if any
    pub match_kind: MatchKind,   // match tier
    pub confidence: f64,          // score in [0.0, 1.0]
    pub reason: SmolStr,         // why the match was selected / why it failed
}

impl Resolution {
    pub fn not_found(name: &str) -> Self { ... }
    pub fn exact(entity: Entity) -> Self { ... }
    pub fn context_scoped(entity: Entity, confidence: f64) -> Self { ... }
    pub fn fuzzy(entity: Entity, confidence: f64) -> Self { ... }
}
```

### 4.3 EntityCandidate — Intermediate State

```rust
// touring-identity/src/types.rs:376-397
pub struct EntityCandidate {
    pub entity: Entity,
    pub match_kind: MatchKind,
    pub confidence: f64,
}

impl EntityCandidate {
    pub fn resolve(self) -> Resolution {
        Resolution {
            entity: Some(self.entity),
            match_kind: self.match_kind,
            confidence: self.confidence,
            reason: SmolStr::from("resolved"),
        }
    }
}
```

---

## 5. Canonical Name Format

The canonical name follows the Rust module path convention:

```
crate_name::module_path::symbol_name
```

Components:
- **crate_name**: the Cargo package name (kebab-case normalized to snake_case via
  crate's workspace package name)
- **module_path**: dot-separated module path from crate root to containing module
- **symbol_name**: the symbol's declared name (exact casing)

Examples:

| Canonical Name | Crate | Module | Symbol |
|---------------|-------|--------|--------|
| `touring-ast::semantic_search::CosineComputer` | touring-ast | semantic_search | CosineComputer |
| `touring-hooks::pre_read::PreReadContext` | touring-hooks | pre_read | PreReadContext |
| `touring-core::profile::Profiler` | touring-core | profile | Profiler |
| `touring-analysis::quality::ComplexityMetrics` | touring-analysis | quality | ComplexityMetrics |

### 5.1 Crate Namespace Extraction

`EntityId::crate_namespace()` returns the first `::`-delimited segment:

```rust
EntityId::from_str("touring-ast::semantic_search::CosineComputer").crate_namespace()
// → Some("touring-ast")
```

This is used by the resolution algorithm to group entities by crate of origin
and to detect cross-crate homonimia.

---

## 6. Resolution Algorithm

### 6.1 Resolution Pipeline

Given a name string and a set of candidate entities, the resolution algorithm
produces a `Resolution`:

```
1. EXACT MATCH — find entity where any criterion satisfies exact_name(name)
   → if found: Resolution::exact(entity), confidence=1.0

2. CONTEXT-SCOPED — find entity where any criterion satisfies
   context_scoped(crate_pattern, symbol_name) AND symbol_name matches
   → if found: Resolution::context_scoped(entity, confidence=0.97)

3. FUZZY MATCH — find entity where any criterion satisfies fuzzy_name(name, max_ed)
   AND Levenshtein distance ≤ max_ed
   → if found: Resolution::fuzzy(entity, confidence ∈ [0.70, 0.85])

4. AMBIGUOUS — if multiple candidates tie at same confidence tier
   → Resolution { match_kind: Ambiguous, confidence: mid-tier }

5. NOT FOUND — no candidate satisfies any criterion
   → Resolution::not_found(name)
```

### 6.2 Confidence Score Computation

The confidence score is computed as:

- **Exact**: always `1.0`
- **ContextScoped**: `0.97` (fixed mid-point of [0.95, 0.99] range)
- **Fuzzy**: `0.80` minus `2 × edit_distance` (so distance=0 gives 0.80,
  distance=1 gives 0.78, capped at lower bound 0.70)
- **Ambiguous**: `0.70` (lower bound of the ambiguous range)
- **NotFound**: `0.0`

### 6.3 High-Confidence Threshold

Only `Resolution` with `match_kind ∈ {Exact, ContextScoped}` AND `confidence ≥ 0.95`
are considered **high-confidence** and eligible for canonical documentation by
the scriber. Fuzzy matches require explicit verification via `touring index find`
before being cited in any output.

---

## 7. Entity Validity Invariants

| # | Invariant | Enforcement |
|---|-----------|-------------|
| V1 | `canonical_name.len() ≤ 256` bytes | `Entity::is_valid()` |
| V2 | `criteria.len() ≤ 32` | `Entity::is_valid()` |
| V3 | `EntityId` is never empty string | construction via `from_str` — SmolStr accepts empty |
| V4 | Each `Entity.id` is unique in the registry | registry-level uniqueness constraint |
| V5 | `auto_seeded=true` implies `canonical=false` | no enforced invariant (user must confirm) |

---

## 8. Auto-Seeding and Canonical Confirmation (D5.8)

### 8.1 Auto-Seeding

When the touring symbol index is rebuilt, all indexed symbols are auto-seeded
as `Entity` with `auto_seeded = true, canonical = false`. The entity is
populated with:

- `id`: `EntityId::from_str(canonical_name)`
- `canonical_name`: from index entry
- `kind`: inferred from symbol kind (Function/Type/Trait/etc.)
- `crate_name`: from the index entry's crate metadata
- `criteria`: single `Criterion::exact_name(symbol_name)` — auto-seeding
  uses exact name only, no fuzzy or context-scoped criteria
- `source_path`, `definition_line`: from index entry
- `doc_summary`: from index entry (if available)
- `auto_seeded = true`
- `canonical = false`

### 8.2 Canonical Confirmation

A user confirms an entity via `touring entity confirm <canonical_name>`. This
transitions `canonical` from `false` to `true`. Only canonical entities appear
in high-confidence resolution results.

**Effect on scriber output**: entities with `canonical = true` are documented
as `verified_existing`. Entities with `auto_seeded = true AND canonical = false`
are documented as `planned_future` (pending confirmation).

---

## 9. Interaction with PARCER (D9.2)

PARCER profiles require that every symbol cited in agent output be classified
according to the **Symbol Verification Table**. The entity registry provides the
canonical ground truth for that classification:

| PARCER Role | Symbol Verification Field | Registry Integration |
|-------------|--------------------------|-----------------------|
| Scouter | `cited_symbols` | `EntityId` + `MatchKind` for every cited symbol |
| Architect | `symbol_verification` | `EntityId` + criteria match for `verified_existing` |
| Engineer | `symbol_verification` | `Entity::with_criterion()` for new symbols created |
| Auditor | `vgp_cross_verification` | `Resolution.confidence` for re-verified claims |
| Scriber | `documented_symbols` | `Entity.canonical` flag for `verified_existing` |

---

## 10. Interaction with VGP Layer 6

VGP Layer 6 (Entity Registry contract) uses `Contracts.entities_must_exist`:

```rust
// contracts.rs:95-99
/// Entity IDs that MUST exist and satisfy their admission criteria at generation time.
/// VGP Layer 6 (Entity Registry) — D5.6 of Touring v8 Master Plan S5.
/// Failure emits `output.rejected` with `error_code: ENTITY_VIOLATION`.
#[serde(skip_serializing_if = "Vec::is_empty", default)]
pub entities_must_exist: Vec<EntityIdRef>,
```

The `EntityIdRef` type is a transparent wrapper around `String`:

```rust
// contracts.rs:105-144
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct EntityIdRef(String);

impl EntityIdRef {
    pub fn new(s: &str) -> Self { Self(s.to_string()) }
    pub fn as_str(&self) -> &str { &self.0 }
}
```

When `entities_must_exist` is non-empty, the VGP Layer 6 validator checks that
each `EntityIdRef` resolves to a canonical entity (confidence ≥ 0.95) before
the plan can be committed. Failure emits `output.rejected` with
`error_code: ENTITY_VIOLATION`.

---

## 11. Schema Limits

| Limit | Value | Rationale |
|-------|-------|-----------|
| `MAX_CANONICAL_LEN` | 256 bytes | Prevents pathological names; fits all realistic crate::module::symbol paths |
| `MAX_CRITERIA_COUNT` | 32 per entity | Supports 32 criteria (exact + 30 fuzzy variants at different thresholds) without pathological case |

---

## 12. Reference Implementation

| File | Purpose |
|------|---------|
| `crates/touring-identity/src/types.rs` | All types in this RFC (503 lines, 12 tests) |
| `crates/touring-generator/src/plan/contracts.rs` | `EntityIdRef`, `entities_must_exist` field |
| `crates/touring-generator/src/validate/pipeline.rs` | L6 layer (placeholder in current impl) |
| `~/.claude/agents/touring-*.parcer.yaml` | PARCER profiles referencing entity registry |

---

## 13. Reference Files

| File | Purpose |
|------|---------|
| `~/.claude/rust/docs/RFC-002-parcer-profile-schema.md` | PARCER contract (D9.2) |
| `~/.claude/rust/docs/RFC-003-path-boundaries-contract.md` | VGP L5 (D9.3) |
| `~/.claude/rust/docs/RFC-005-seven-layer-validation-pipeline.md` | VGP L1-L7 (D9.5) |
| `~/.claude/rust/docs/RFC-001-activity-event-catalog.md` | Activity event catalog (D9.1) |

---

## 14. Tests

Entity identity has 12 unit tests in `types.rs:399-502`:

```rust
// types.rs:399-502 — test module
#[test] fn entity_id_from_str()
#[test] fn entity_id_no_namespace()
#[test] fn criterion_exact_name()
#[test] fn criterion_fuzzy()
#[test] fn entity_new_and_with()
#[test] fn entity_relation_with_justification()
#[test] fn match_kind_confidence_bounds()
#[test] fn resolution_exact()
#[test] fn resolution_not_found()
#[test] fn entity_validity_length()  // verifies is_valid() rejects names > 256 bytes
```

---

## 15. Change Log

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-05-09 | Initial draft (Constitution v8.0) |

---

**RFC-004 v1.0.0 — Entity Identity Registry — ESAA S5 / VGP L6 formalized**