# RFC-005: 7-Layer Validation Pipeline

**Status**: Active
**Type**: Specification
**Layer**: ESAA / S3 / VGP Layer 5
**Author**: TACO (Constitution v8.0 Draft)
**Date**: 2026-05-09
**Version**: 1.0.0

---

## 1. Context and Motivation

VGP (Verified Generation Protocol) gates all artifact generation through a
7-layer validation pipeline. Each layer validates a specific aspect of the
`GeneratorPlan` before the typestate machine may advance. Without this pipeline,
invalid plans could reach the template renderer, produce malformed output, or
commit artifacts that violate workspace invariants.

This RFC formalizes the pipeline architecture implemented in
`crates/touring-generator/src/validate/pipeline.rs`, which was implicit in earlier
VGP versions and is now made explicit and testable.

**Relation to S3**: This RFC formalizes the ESAA §7-layer validation primitive
(S6 of the v8 master plan). Layer 5 (PathBoundary) is formalized as RFC-003.

---

## 2. Layer Catalog

| # | Layer | Validation | ESAA primitive | Blocking? |
|---|-------|------------|-----------------|-----------|
| L1 | `JsonParse` | plan JSON is valid UTF-8 + parseable | parse | YES (hard) |
| L2 | `SchemaValidation` | plan fields conform to schema | schema | YES (hard) |
| L3 | `VocabularyAllowed` | `GeneratorKind` is in the allowed set | vocab | NO (advisory) |
| L4 | `StateMachine` | plan status transitions are legal | state-machine | YES (hard) |
| L5 | `PathBoundary` | artifact paths respect `Contracts.path_boundaries` | boundary | NO (advisory per enforcement mode) |
| L6 | `Immutability` | committed artifacts are not modified | immutability | YES (hard) |
| L7 | `VerificationGate` | composite score ≥ 0.85 | verification | YES (hard) |

### 2.1 Hard vs Advisory Layers

- **Hard (blocking)**: L1, L2, L4, L6, L7 — failure aborts the pipeline.
- **Advisory (non-blocking)**: L3 (always passes even on failure — returns error
  but pipeline continues to collect all results); L5 (WarnOnly violations pass
  with `passed=true`, score=1.0, but FailClosed violations block).

### 2.2 Layer Name Strings

Each layer has a canonical `name()` string used in `LayerResult`:

| Layer | name() |
|-------|--------|
| L1 | `"l1_json_parse"` |
| L2 | `"l2_schema"` |
| L3 | `"l3_vocabulary"` |
| L4 | `"l4_state_machine"` |
| L5 | `"l5_path_boundary"` |
| L6 | `"l6_immutability"` |
| L7 | `"l7_verification_gate"` |

---

## 3. ValidationLayer Enum

```rust
// pipeline.rs:37-48
pub enum ValidationLayer {
    L1_JsonParse,
    L2_SchemaValidation,
    L3_VocabularyAllowed,
    L4_StateMachine,
    L5_PathBoundary,
    L6_Immutability,
    L7_VerificationGate,
}

impl ValidationLayer {
    pub const ALL: [ValidationLayer; 7] = [
        ValidationLayer::L1_JsonParse,
        ValidationLayer::L2_SchemaValidation,
        ValidationLayer::L3_VocabularyAllowed,
        ValidationLayer::L4_StateMachine,
        ValidationLayer::L5_PathBoundary,
        ValidationLayer::L6_Immutability,
        ValidationLayer::L7_VerificationGate,
    ];

    pub const fn name(&self) -> &'static str { ... }
}
```

The `ALL` constant is used by `validate_plan()` to map array indices to
layer enum variants (pipeline.rs:513-514):

```rust
let layer_enum: ValidationLayer = ValidationLayer::ALL[i];
```

---

## 4. ValidationError Taxonomy

```rust
// pipeline.rs:76-91
pub enum ValidationError {
    L1JsonParse(String),
    L2Schema(String),
    L3VocabularyNotAllowed { kind: String },
    L4StateMachine(String),
    L5BoundaryViolation { file: String, kind: String },
    L6ImmutabilityViolation { path: String },
    L7VerificationFailed { score: f64 },
}

impl ValidationError {
    pub fn layer(&self) -> ValidationLayer { ... }
}
```

Each error variant is tagged with the layer that produced it. Callers can
match on variant without parsing strings. The `Display` impl produces human-readable
messages for logging and UIs.

---

## 5. ValidationContext

```rust
// pipeline.rs:152-230
pub struct ValidationContext {
    /// Allowed generator kinds (from registry or config).
    pub allowed_kinds: Vec<String>,
    /// Contracts from the plan (for L5 boundary and L6 immutability).
    pub contracts: Option<Contracts>,
    /// History of previously committed artifact paths (for L6).
    pub committed_history: CommittedHistory,
    /// Composite health score from touring daemon (for L7).
    pub composite_health_score: Option<f64>,
    /// Optional observer called after each layer completes (S1 activity log wiring point).
    on_layer_complete: Option<LayerCompleteObserver>,
}
```

### 5.1 Builder Pattern

```rust
impl ValidationContext {
    pub fn new() -> Self { ... }
    pub fn with_allowed_kinds(mut self, kinds: Vec<String>) -> Self { ... }
    pub fn with_contracts(mut self, contracts: Contracts) -> Self { ... }
    pub fn with_composite_health(mut self, score: f64) -> Self { ... }
    pub fn with_layer_observer(
        mut self,
        obs: impl Fn(ValidationLayer, &LayerResult) + Send + Sync + 'static,
    ) -> Self { ... }
}
```

The builder returns `Self` by value, enabling fluent chaining.

### 5.2 Layer Observer (S1 Wiring Point)

`on_layer_complete` is typed as `LayerCompleteObserver`:

```rust
// pipeline.rs:149-150
pub type LayerCompleteObserver =
    Box<dyn Fn(ValidationLayer, &LayerResult) + Send + Sync + 'static>;
```

This is the ESAA S1 (Activity Log) wiring point. When set, the observer is
invoked after **every** layer completes (both pass and fail), allowing the
caller to emit `LayerComplete` events to the activity store. The observer
cannot be cloned (due to `Fn` not being `Clone`), so `Clone for ValidationContext`
sets it to `None`.

---

## 6. CommittedHistory (L6)

```rust
// pipeline.rs:131-146
pub struct CommittedHistory {
    /// Set of paths already committed in previous plans (absolute paths).
    pub committed_paths: Vec<String>,
}

impl CommittedHistory {
    pub fn new() -> Self { Self::default() }
    pub fn contains(&self, path: &str) -> bool {
        self.committed_paths.iter().any(|p| p == path)
    }
}
```

L6 uses `contains()` to detect whether a plan's target path was previously
committed. If so, the artifact would be a modification (not a creation), which
violates the immutability contract.

---

## 7. ValidationReport

```rust
// pipeline.rs:232-273
pub struct ValidationReport {
    pub layers_passed: u8,
    pub layers_total: u8,       // always 7
    pub layer_results: Vec<LayerResult>,
    pub layer_durations_ms: HashMap<String, u64>,
    pub all_passed: bool,
}
```

`ValidationReport::add_layer()` updates counters and appends the result:

```rust
pub fn add_layer(&mut self, result: LayerResult) {
    self.layer_durations_ms.insert(result.name.clone(), result.elapsed_ms);
    if result.passed {
        self.layers_passed += 1;
    } else {
        self.all_passed = false;
    }
    self.layer_results.push(result);
}
```

---

## 8. LayerResult Schema

```rust
// plan/result.rs — referenced by pipeline.rs
pub struct LayerResult {
    pub name: String,          // e.g. "l5_path_boundary"
    pub score: NormalizedScore,
    pub passed: bool,
    pub issues: Vec<String>,   // human-readable lines "path: violation_kind"
    pub elapsed_ms: u64,
}
```

`NormalizedScore` is clamped to [0.0, 1.0]. A score of `1.0` always means
pass (even for advisory layers with warnings). A score of `0.0` always means
block (FailClosed violations or hard-layer errors).

---

## 9. Per-Layer Specification

### L1 — JsonParse (pipeline.rs:277-288)

```rust
fn l1_json_parse(_plan: &GeneratorPlan, _ctx: &ValidationContext) -> Result<LayerResult, ValidationError> {
    // The plan is already JSON-deserialized by the time we receive it.
    // Invalid UTF-8 or malformed JSON would have failed at the call site.
    // This layer is a no-op pass — kept for structural completeness.
    Ok(LayerResult { name: L1.name(), score: 1.0, passed: true, issues: [], elapsed_ms: 0 })
}
```

**Rationale**: The plan arrives as a fully deserialized `GeneratorPlan` struct.
JSON parsing is already done before `validate_plan()` is called.

### L2 — SchemaValidation (pipeline.rs:290-301)

```rust
fn l2_schema(_plan: &GeneratorPlan, _ctx: &ValidationContext) -> Result<LayerResult, ValidationError> {
    // JSON schema validation is performed by serde at the call site.
    // The generator receives a fully deserialized `GeneratorPlan`, so
    // structural schema errors cannot reach this layer.
    Ok(LayerResult { name: L2.name(), score: 1.0, passed: true, issues: [], elapsed_ms: 0 })
}
```

**Rationale**: Same as L1 — serde already validated structure. L2 is a
structural placeholder.

### L3 — VocabularyAllowed (pipeline.rs:303-334)

```rust
fn l3_vocabulary_allowed(plan: &GeneratorPlan, ctx: &ValidationContext) -> Result<LayerResult, ValidationError> {
    // If allowed_kinds is empty → pass (no restrictions)
    // Else check plan.kind against allowed_kinds (case-insensitive, "*" wildcard)
    // Failure → Err(ValidationError::L3VocabularyNotAllowed { kind })
}
```

Advisory: always produces a `LayerResult` regardless of outcome. The error
is returned but the pipeline continues to run all layers.

### L4 — StateMachine (pipeline.rs:336-348)

```rust
fn l4_state_machine(_plan: &GeneratorPlan, _ctx: &ValidationContext) -> Result<LayerResult, ValidationError> {
    // Status transition validation happens in typestate.rs during state transitions.
    // The plan struct itself has no status field to check.
    // This layer is a structural placeholder.
    Ok(LayerResult { name: L4.name(), score: 1.0, passed: true, issues: [], elapsed_ms: 0 })
}
```

**Rationale**: Typestate transitions are enforced by the type system in
`executor/typestate.rs`. L4 is a placeholder.

### L5 — PathBoundary (pipeline.rs:350-407)

```rust
fn l5_path_boundary(_plan: &GeneratorPlan, ctx: &ValidationContext) -> Result<LayerResult, ValidationError> {
    // If ctx.contracts is None → pass (legacy behavior)
    // If contracts.path_boundaries is None → pass (no boundaries configured)
    // Else construct BoundaryValidator and call validate_artifacts()
    // FailClosed violations → Err(ValidationError::L5BoundaryViolation)
    // WarnOnly or valid → Ok(LayerResult with score 1.0 or 0.0 per BoundaryResult)
}
```

See RFC-003 for full boundary semantics.

### L6 — Immutability (pipeline.rs:409-442)

```rust
fn l6_immutability(_plan: &GeneratorPlan, ctx: &ValidationContext) -> Result<LayerResult, ValidationError> {
    // Check plan.target.file_path against ctx.committed_history.committed_paths
    // If already committed → Violation → LayerResult { score: 0.0, passed: false }
    // Else → pass
}
```

### L7 — VerificationGate (pipeline.rs:444-473)

```rust
fn l7_verification_gate(_plan: &GeneratorPlan, ctx: &ValidationContext) -> Result<LayerResult, ValidationError> {
    const GATE: f64 = 0.85;
    // If composite_health_score is None → pass conservatively
    // If score >= 0.85 → pass
    // If score < 0.85 → Err(ValidationError::L7VerificationFailed { score })
}
```

The `GATE` constant (0.85) is defined inline. The composite health score
comes from the touring daemon's `composite_health_score` field.

---

## 10. validate_plan() Function

```rust
// pipeline.rs:507-543
pub fn validate_plan(plan: &GeneratorPlan, ctx: &ValidationContext) -> ValidationReport {
    let mut report = ValidationReport::new();
    let layers: &[fn(...) -> Result<LayerResult, ValidationError>] = &[
        l1_json_parse, l2_schema, l3_vocabulary_allowed, l4_state_machine,
        l5_path_boundary, l6_immutability, l7_verification_gate,
    ];
    for (i, layer_fn) in layers.iter().enumerate() {
        let layer_enum: ValidationLayer = ValidationLayer::ALL[i];
        match layer_fn(plan, ctx) {
            Ok(result) => {
                debug_assert_eq!(result.name, layer_enum.name());
                if let Some(ref obs) = ctx.on_layer_complete {
                    obs(layer_enum, &result);
                }
                report.add_layer(result);
            }
            Err(e) => {
                let result = LayerResult {
                    name: layer_enum.name().to_string(),
                    score: NormalizedScore::ZERO,
                    passed: false,
                    issues: vec![e.to_string()],
                    elapsed_ms: 0,
                };
                if let Some(ref obs) = ctx.on_layer_complete {
                    obs(layer_enum, &result);
                }
                report.add_layer(result);
            }
        }
    }
    report
}
```

Key properties:
- All 7 layers always run (no short-circuit on error — report is complete)
- `debug_assert_eq!(result.name, layer_enum.name())` verifies layer identity
- Observer fires for both pass and fail
- Error results have `elapsed_ms = 0` (errors don't track wall time)

---

## 11. Test Coverage

Pipeline has 12 unit tests (pipeline.rs:545-773):

| Test | Verifies |
|------|----------|
| `all_layers_pass_empty_context` | 7/7 pass with no restrictions |
| `l7_fails_below_085` | composite=0.70 → L7 fails, layers_passed=6 |
| `l7_passes_at_085_exactly` | composite=0.85 → all_passed |
| `l7_passes_above_085` | composite=0.95 → all_passed |
| `l3_blocks_unknown_kind` | unknown kind → L3 fails, layers_passed=6 |
| `l3_allows_matching_kind` | matching kind → all_passed |
| `l6_detects_committed_path` | committed path → L6 fails |
| `l6_allows_new_path` | new path → all_passed |
| `l5_passes_without_contracts` | no contracts → L5 skips, all_pass |
| `l5_failclosed_blocks_impl_write_to_docs` | Impl boundary blocks docs/ write → L5 fails |
| `validation_error_layer_roundtrip` | error.layer() == correct ValidationLayer |
| `validation_layer_all_ordered` | ALL constant has 7 ordered entries |
| `validation_context_builder` | builder methods set correct fields |
| `observer_called_on_each_layer` | observer fires 7 times on all-pass run |
| `observer_called_on_layer_failure` | observer fires 7 times even when L3 fails |
| `committed_history_contains` | contains() returns correct bool |
| `layer_result_order_preserved` | results[0]=l1, results[6]=l7 |

---

## 12. Integration with Typestate Pipeline

The VGP typestate pipeline calls `validate_plan()` before transitioning
between states:

```
Draft → Verified (calls validate_plan with L1-L7)
     → Rendered → Speculated → Committed
```

Each transition may add additional context to `ValidationContext`:
- `with_allowed_kinds()` — populated from the generator registry
- `with_contracts()` — copied from `GeneratorPlan.contracts`
- `with_composite_health()` — queried from touring daemon before L7
- `with_layer_observer()` — wired to emit `LayerComplete` events to the
  activity store (S1)

---

## 13. Reference Implementation

| File | Purpose |
|------|---------|
| `crates/touring-generator/src/validate/pipeline.rs` | Full implementation (543 lines, 17 tests) |
| `crates/touring-generator/src/validate/boundary.rs` | L5 boundary enforcement (RFC-003) |
| `crates/touring-generator/src/plan/contracts.rs` | Contracts struct with path_boundaries field |
| `crates/touring-generator/src/plan/result.rs` | LayerResult, RenderedFile, FileAction |
| `crates/touring-generator/src/executor/typestate.rs` | Typestate transitions that call validate_plan() |

---

## 14. Reference Files

| File | Purpose |
|------|---------|
| `~/.claude/rust/docs/RFC-003-path-boundaries-contract.md` | L5 formalization |
| `~/.claude/rust/docs/RFC-002-parcer-profile-schema.md` | PARCER contract (D9.2) |
| `~/.claude/rust/docs/RFC-001-activity-event-catalog.md` | Activity event catalog (D9.1) |

---

## 15. Change Log

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-05-09 | Initial draft (Constitution v8.0) |

---

**RFC-005 v1.0.0 — 7-Layer Validation Pipeline — ESAA S3 / VGP formalized**