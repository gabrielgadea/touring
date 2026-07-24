# RFC-003: Path Boundaries Contract

**Status**: Active
**Type**: Specification
**Layer**: ESAA / S3 / VGP Layer 5
**Author**: TACO (Constitution v8.0 Draft)
**Date**: 2026-05-09
**Version**: 1.0.0

---

## 1. Context and Motivation

VGP (Verified Generation Protocol) enforces preconditions on symbol existence,
file presence, and structural invariants before any artifact is committed. Layer 5
adds path-boundary enforcement: a mechanism that validates whether generated
artifact paths comply with the `TaskKind`-specific allowlists defined in the
`Contracts.path_boundaries` field.

Without path boundaries, a `Spec`-task agent could inadvertently write Rust source
code; an `Impl`-task agent could overwrite documentation. This layer closes that
gap by validating artifact paths against glob patterns **before** the `Speculated`
state is entered in the VGP typestate pipeline.

**Relation to S3**: This RFC formalizes the VGP Layer 5 implementation delivered
in `crates/touring-generator/src/validate/boundary.rs` and the `PathBoundaries`
struct in `crates/touring-generator/src/plan/contracts.rs`.

---

## 2. Core Types

### 2.1 TaskKind — Principle of Least Privilege

Each `TaskKind` has different read/write permissions. Agents operating in a given
kind **MUST NOT** write outside their allowed write set.

| Variant | Description | Default Enforcement |
|---------|-------------|---------------------|
| `Spec` | Specification: docs/**, *.md | FailClosed |
| `Impl` | Implementation: crates/**/src/**, tests/** | FailClosed |
| `QA` | QA: tests/**, docs/qa/** | FailClosed |
| `Doc` | Documentation: docs/**, *.md | FailClosed |
| `Audit` | Read-only analysis — no write permissions | WarnOnly |
| `Hotfix` | Restricted emergency changes | FailClosed |

### 2.2 BoundaryEnforcement

| Variant | Behavior |
|---------|----------|
| `FailClosed` (default) | Any violation blocks the pipeline — `BoundaryResult::Violations` |
| `WarnOnly` | Violations are reported but do not block — `BoundaryResult::Warnings` |

### 2.3 PathBoundaries Struct

```rust
// touring-generator/src/plan/contracts.rs:29-44
pub struct PathBoundaries {
    pub task_kind: TaskKind,
    pub read: Vec<String>,           // glob patterns — allowed read locations
    pub write: Vec<String>,          // glob patterns — allowed write locations
    pub forbidden_write: Vec<String>, // glob patterns — explicitly forbidden (override allowlist)
    pub enforcement: BoundaryEnforcement,
}
```

**Prefix-match semantics**: `"crates/foo"` matches `"crates/foo/src/lib.rs"` — globset
uses the same semantics as the Rust `glob` crate.

### 2.4 BoundaryViolation

```rust
// touring-generator/src/validate/boundary.rs:15-21
pub struct BoundaryViolation {
    pub file_path: String,
    pub task_kind: TaskKind,
    pub violation_kind: ViolationKind,
    pub matched_pattern: String,
}
```

### 2.5 ViolationKind

```rust
// touring-generator/src/validate/boundary.rs:23-27
pub enum ViolationKind {
    /// Path matches a forbidden_write pattern — never permitted.
    ForbiddenWrite,
    /// Path does not match any allowed write pattern.
    NotAllowedWrite,
}
```

### 2.6 BoundaryResult

```rust
// touring-generator/src/validate/boundary.rs:30-35
pub enum BoundaryResult {
    /// All artifact paths pass boundary checks.
    Valid,
    /// One or more paths violate boundaries in WarnOnly mode.
    Warnings(Vec<BoundaryViolation>),
    /// One or more paths violate boundaries in FailClosed mode.
    Violations(Vec<BoundaryViolation>),
}
```

---

## 3. Default Boundary Presets

Each `TaskKind` has a default `PathBoundaries` preset defined in
`boundary.rs:193-238`. These presets are constructed by the `default_boundary()`
function.

### 3.1 Spec — Documentation and Specification Files

```rust
TaskKind::Spec => PathBoundaries {
    task_kind: TaskKind::Spec,
    read: vec!["docs/".into(), "spec/".into(), "*.md".into()],
    write: vec!["docs/".into(), "docs/**".into(), "spec/".into(), "spec/**".into(), "*.md".into()],
    forbidden_write: vec!["crates/**".into(), "src/**".into(), "tests/**".into(), "benches/**".into()],
    enforcement: BoundaryEnforcement::FailClosed,
}
```

**Rationale**: Spec tasks read docs and markdown; they write exclusively to
documentation trees. Rust code directories are explicitly forbidden.

### 3.2 Impl — Implementation Files

```rust
TaskKind::Impl => PathBoundaries {
    task_kind: TaskKind::Impl,
    read: vec!["crates/**".into(), "src/**".into(), "tests/**".into()],
    write: vec!["crates/**".into(), "src/**".into(), "tests/**".into()],
    forbidden_write: vec!["docs/**".into(), "spec/**".into()],
    enforcement: BoundaryEnforcement::FailClosed,
}
```

**Rationale**: Impl tasks write to code directories. Documentation trees are
explicitly excluded to prevent accidental doc Pollution in implementation phases.

### 3.3 QA — Tests and Quality Assurance

```rust
TaskKind::QA => PathBoundaries {
    task_kind: TaskKind::QA,
    read: vec!["crates/**".into(), "src/**".into(), "tests/**".into(), "benches/**".into()],
    write: vec!["tests/**".into(), "benches/**".into()],
    forbidden_write: vec!["crates/*/src/**".into(), "docs/**".into(), "spec/**".into()],
    enforcement: BoundaryEnforcement::FailClosed,
}
```

**Rationale**: QA writes only to test/benchmark directories. Source code in
`crates/*/src/**` is off-limits to QA agents.

### 3.4 Doc — General Documentation

```rust
TaskKind::Doc => PathBoundaries {
    task_kind: TaskKind::Doc,
    read: vec!["crates/**".into(), "src/**".into(), "docs/**".into(), "*.md".into()],
    write: vec!["docs/**".into(), "*.md".into()],
    forbidden_write: vec!["crates/**".into(), "src/**".into(), "tests/**".into(), "benches/**".into()],
    enforcement: BoundaryEnforcement::FailClosed,
}
```

### 3.5 Audit — Read-Only Analysis

```rust
TaskKind::Audit => PathBoundaries {
    task_kind: TaskKind::Audit,
    read: vec!["crates/**".into(), "src/**".into(), "tests/**".into(), "docs/**".into()],
    write: vec!["docs/audit/**".into()],
    forbidden_write: vec!["crates/**".into(), "src/**".into(), "tests/**".into()],
    enforcement: BoundaryEnforcement::WarnOnly,
}
```

**Rationale**: Audit is read-only by design. `WarnOnly` means violations produce
warnings but do not block the pipeline. Audit agents may write audit reports to
`docs/audit/` only.

### 3.6 Hotfix — Emergency Changes

```rust
TaskKind::Hotfix => PathBoundaries {
    task_kind: TaskKind::Hotfix,
    read: vec!["crates/**".into(), "src/**".into(), "tests/**".into()],
    write: vec!["crates/**".into(), "src/**".into()],
    forbidden_write: vec!["docs/**".into(), "spec/**".into()],
    enforcement: BoundaryEnforcement::FailClosed,
}
```

**Rationale**: Hotfix has minimal blast radius — write access limited to code
directories only. Documentation/spec trees are off-limits.

---

## 4. GlobSet Mechanics

The `GlobCache` (boundary.rs:38-68) wraps two `GlobSet` instances:

```rust
struct GlobCache {
    write_patterns: GlobSet,     // built from `PathBoundaries.write`
    forbidden_patterns: GlobSet, // built from `PathBoundaries.forbidden_write`
}
```

**Evaluation order** (boundary.rs:88-106):

```
1. If path matches forbidden_patterns → ForbiddenWrite violation
2. Else if path does NOT match write_patterns → NotAllowedWrite violation
3. Else → path is valid
```

The `BoundaryValidator::validate_artifacts()` (boundary.rs:108-131) evaluates
each `RenderedFile` in the artifact list and aggregates violations:

```rust
pub fn validate_artifacts(&self, artifacts: &[RenderedFile]) -> BoundaryResult {
    let mut violations: Vec<BoundaryViolation> = Vec::new();
    for artifact in artifacts {
        if let Some(v) = self.check_path(&artifact.path) {
            violations.push(v);
        }
    }
    match self.enforcement {
        BoundaryEnforcement::FailClosed if !violations.is_empty() => {
            BoundaryResult::Violations(violations)
        }
        BoundaryEnforcement::WarnOnly => {
            if violations.is_empty() {
                BoundaryResult::Valid
            } else {
                BoundaryResult::Warnings(violations)
            }
        }
        _ => BoundaryResult::Valid,
    }
}
```

---

## 5. Integration with VGP Typestate Pipeline

Boundary enforcement runs as **VGP Layer 5** — after Layer 4 (State Machine)
and before Layer 6 (Immutability) and Layer 7 (Verification Gate).

```
L1_JsonParse → L2_SchemaValidation → L3_VocabularyAllowed →
L4_StateMachine → L5_PathBoundary → L6_Immutability → L7_VerificationGate
```

The `BoundaryResult` is converted to a `LayerResult` (boundary.rs:133-180):

```rust
impl From<(BoundaryResult, std::time::Instant)> for LayerResult {
    fn from((result, started): (BoundaryResult, std::time::Instant)) -> Self {
        // Valid → LayerResult { score: 1.0, passed: true }
        // Warnings → LayerResult { score: 1.0, passed: true, issues: [...] }
        // Violations → LayerResult { score: 0.0, passed: false, issues: [...] }
    }
}
```

The `LayerResult` name is always
`ValidationLayer::L5_PathBoundary.name()` — used by the pipeline aggregator
to identify which layer produced a given result.

---

## 6. Contracts.path_boundaries Field

The `Contracts` struct (contracts.rs:58-100) carries an optional
`path_boundaries` field:

```rust
/// Optional path boundaries for VGP Layer 5 (boundary) enforcement.
/// When None, legacy behavior (no path enforcement).
#[serde(skip_serializing_if = "Option::is_none")]
pub path_boundaries: Option<PathBoundaries>,
```

When `path_boundaries` is `None`, the validation layer is a **no-op** — all
artifacts pass without boundary checking (legacy behavior).

When `path_boundaries` is `Some(b)`, a `BoundaryValidator::new(&b)` is constructed
and invoked on the rendered artifact list.

---

## 7. LayerResult Schema

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Always `"L5_PathBoundary"` |
| `score` | `NormalizedScore` | `1.0` if Valid/Warnings; `0.0` if Violations |
| `passed` | `bool` | `true` for Valid and Warnings; `false` for Violations |
| `issues` | `Vec<String>` | Human-readable lines `"path: violation_kind"` for each violation |
| `elapsed_ms` | `u64` | Wall-clock milliseconds spent in this layer |

---

## 8. Test Coverage

Boundary enforcement has 9 unit tests in `boundary.rs:240-379`:

| Test | What it verifies |
|------|------------------|
| `impl_write_allowed_for_crate_file` | `Impl` boundary allows `crates/**` paths |
| `spec_forbidden_for_crate_file` | `Spec` boundary blocks `crates/foo/src/lib.rs` with Violations |
| `audit_warn_only_enforcement` | `Audit` produces Warnings (not Violations) for forbidden write |
| `warn_only_enforcement_no_block` | `Impl` with WarnOnly produces Warnings for `docs/changes.md` |
| `fail_closed_blocks_violation` | FailClosed mode blocks `src/lib.rs` under Spec boundaries |
| `layer_result_from_valid_result` | LayerResult score=1, passed=true on Valid |
| `layer_result_from_violations` | LayerResult score=0, passed=false on Violations |
| `default_boundary_task_kind_coverage` | All 6 TaskKind variants produce non-empty write/forbidden lists |
| `prefix_match_semantics_crates` | `"crates/foo"` matches `"crates/foo/src/lib.rs"` prefix semantics |
| `glob_meta_chars` | `**/*.rs` pattern matches `crates/bar/src/main.rs` |

---

## 9. Reference Implementation

| File | Purpose |
|------|---------|
| `crates/touring-generator/src/validate/boundary.rs` | BoundaryValidator, GlobCache, BoundaryViolation, ViolationKind, default_boundary, tests |
| `crates/touring-generator/src/validate/pipeline.rs` | ValidationLayer enum (L1–L7), ValidationReport, LayerResult |
| `crates/touring-generator/src/plan/contracts.rs` | PathBoundaries, TaskKind, BoundaryEnforcement |
| `crates/touring-generator/src/plan/result.rs` | RenderedFile, FileAction |

---

## 10. Reference Files

| File | Purpose |
|------|---------|
| `~/.claude/agents/touring-scouter.parcer.yaml` | Scouter profile — references L5 enforcement |
| `~/.claude/agents/touring-engineer.parcer.yaml` | Engineer profile — L5 enforced during engineer phase |
| `~/.claude/agents/touring-auditor.parcer.yaml` | Auditor profile — pre-impl audit verifies L5 compliance |
| `~/.claude/rust/docs/RFC-002-parcer-profile-schema.md` | PARCER contract (D9.2) |

---

## 11. Change Log

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-05-09 | Initial draft (Constitution v8.0) |

---

**RFC-003 v1.0.0 — Path Boundaries Contract — ESAA S3 / VGP L5 formalized**