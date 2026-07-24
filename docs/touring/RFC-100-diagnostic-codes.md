# RFC-100 — Diagnostic Codes

**Status**: NORMATIVE
**Version**: 1.0.0
**Date**: 2026-04-24
**Editor**: Gabriel Gadea
**Wave**: Q4 (waves-Q-R-M-A-T-P plan)
**Inspired by**: THSF RFC-001..RFC-004 (35 diagnostic codes), Rust `Exxxx`, TypeScript `TSxxxx`, PMAT `CB-NNNN`
**Canonical Rust trait**: `touring_core::diagnostic::DiagnosticCode`

---

## 1. Purpose

This RFC defines a workspace-wide convention for **diagnostic codes** —
short, machine-readable identifiers that prefix every diagnostic emitted
by Touring subsystems (wiring, quality, blast radius, generator, memory).

Every diagnostic carries:

1. A unique **code** (e.g. `W-100`, `Q-202`)
2. A **severity** level (`error`, `warning`, `info`, `hint`)
3. A human-readable **message**
4. Optional **file path**, **line**, **help** text

---

## 2. Why diagnostic codes

Without codes, diagnostics are unstructured strings. With codes:

- **Machine-readable**: CI gates filter on `code` field.
- **Searchable**: documentation links to specific issues by code.
- **Stable**: messages can be improved without breaking automation.
- **Composable**: cross-subsystem reports (e.g. `repo-score`) tag findings
  by category.
- **Industry-aligned**: matches Rust (`E0382`), TypeScript (`TS2322`),
  ESLint (`no-unused-vars`), PMAT (`CB-200`).

---

## 3. Range allocations

Each subsystem owns a contiguous numeric range, prefixed by a single
letter that identifies the subsystem.

| Prefix | Range | Subsystem | Owner crate |
|--------|-------|-----------|-------------|
| `W-`   | `100..199` | **Wiring** — orphan symbols, integration scores, cycles | `touring-analysis::wiring` |
| `Q-`   | `200..299` | **Quality** — TDG grades, antipatterns, complexity | `touring-analysis::quality` |
| `B-`   | `300..399` | **Blast radius** — impact analysis, cross-feature | `touring-ast::blast` / `touring-analysis::blast_radius` |
| `G-`   | `400..499` | **Generator** — VGP, shadow validate, render | `touring-generator` |
| `M-`   | `500..599` | **Memory** — recall, retrievers, RRF fusion | `touring-hooks::memory` |
| `R-`   | `600..699` | **Reserved** for future repo-score / RFC subsystem | TBD |
| `T-`   | `700..799` | **Reserved** for future testing / mutation subsystem | TBD |
| `P-`   | `800..899` | **Reserved** for future protocol / decompose subsystem | TBD |
| `S-`   | `900..999` | **Reserved** for future security / audit subsystem | TBD |

**Invariant**: each range allocates 100 codes. Subranges within a range
are conventional (e.g. `W-100..109` = orphan-related, `W-110..119` =
cycle-related), but not enforced by the trait.

**Out-of-range codes** are forbidden — implementations MUST panic in
debug builds and log `error!` in release builds.

---

## 4. Severity levels

```rust
pub enum Severity {
    Error,    // CI must fail
    Warning,  // CI may fail (configurable)
    Info,     // Surface but non-blocking
    Hint,     // Suggestion / future improvement
}
```

| Severity | When to use | CI behavior |
|----------|-------------|-------------|
| `Error` | Invariant violated, unrecoverable state | Exit 1 |
| `Warning` | Degraded quality, may be acceptable | Configurable (default: warn-only) |
| `Info` | Notable observation, no action required | Surface |
| `Hint` | Future improvement opportunity | Surface |

---

## 5. Initial code allocations (v1.0.0)

### 5.1 Wiring (W-100..W-199)

| Code | Severity | Message | Owner |
|------|----------|---------|-------|
| `W-100` | error | orphan pub symbol — no consumers found | wiring::orphans |
| `W-101` | warning | low integration score (<0.5) — consider rewiring | wiring::modules |
| `W-102` | info | cross-feature dependency added | wiring::audit |
| `W-103` | hint | symbol could be exported but is currently private | wiring::audit |
| `W-110` | error | dependency cycle detected | wiring::cycles |
| `W-120` | warning | stale wiring index — rebuild recommended | wiring::status |

### 5.2 Quality (Q-200..Q-299)

| Code | Severity | Message | Owner |
|------|----------|---------|-------|
| `Q-200` | warning | quality_score below 0.5 threshold | quality::report |
| `Q-201` | error | TDG grade F detected (composite < 0.60) | quality::tdg |
| `Q-202` | warning | TDG grade D detected (composite < 0.70) | quality::tdg |
| `Q-203` | info | TDG grade C — edit cautiously | quality::tdg |
| `Q-210` | warning | regression streak >=3 on file | health_delta |
| `Q-220` | info | improvement streak detected | health_delta |
| `Q-230` | warning | high antipattern density (>5 hits/file) | quality::antipatterns |
| `Q-240` | warning | high cyclomatic complexity (>20) | quality::complexity |

### 5.3 Blast radius (B-300..B-399)

| Code | Severity | Message | Owner |
|------|----------|---------|-------|
| `B-300` | warning | blast radius >10 — high impact change | blast::report |
| `B-301` | error | blast radius >50 — refactor required first | blast::report |
| `B-310` | info | blast injection in pre-edit hook | blast::predictive |
| `B-320` | hint | cross-feature blast detected | blast::cross_feature |

### 5.4 Generator (G-400..G-499)

| Code | Severity | Message | Owner |
|------|----------|---------|-------|
| `G-400` | error | VGP failed — symbol not found | generator::vgp |
| `G-401` | error | shadow validate score <0.8 | generator::speculate |
| `G-410` | info | speculative validation passed | generator::speculate |
| `G-420` | warning | template render emitted antipatterns | generator::render |

### 5.5 Memory (M-500..M-599)

| Code | Severity | Message | Owner |
|------|----------|---------|-------|
| `M-500` | warning | memory recall returned 0 results | memory::recall |
| `M-510` | info | TF-IDF retriever activated | memory::retriever |
| `M-520` | info | RRF fusion combined N retrievers | memory::rrf |
| `M-530` | hint | memory entry approaching staleness threshold | memory::ttl |

**Total v1.0.0**: 25 codes allocated (target met).

---

## 6. Trait contract

Every subsystem error type SHOULD implement `DiagnosticCode`:

```rust
pub trait DiagnosticCode {
    fn code(&self) -> &'static str;
    fn severity(&self) -> Severity;
    fn message(&self) -> String;
    fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic::new(self.code(), self.severity(), self.message())
    }
}
```

The default `to_diagnostic()` builds a minimal struct; custom impls
MAY enrich with file path, line number, and help text.

---

## 7. JSON output format

When CLI commands accept `--diagnostics` flag, they emit a `diagnostics`
array per finding:

```json
{
  "diagnostics": [
    {
      "code": "W-100",
      "severity": "error",
      "message": "orphan pub symbol — no consumers found",
      "file": "crates/touring-foo/src/bar.rs",
      "line": 42,
      "help": "Wire to a consumer or remove the pub modifier."
    }
  ]
}
```

Fields:
- `code` — string, MUST match `^[A-Z]-\d{3}$` regex
- `severity` — string, one of `error` / `warning` / `info` / `hint`
- `message` — string, human-readable summary
- `file` — optional string, relative path
- `line` — optional integer, 1-indexed
- `help` — optional string, suggested fix or further reading link

---

## 8. Adding a new code

1. **Pick a range** — must fit within an allocated subsystem range.
2. **Update this RFC** — add row to §5 table.
3. **Add Rust constant** in `touring_core::diagnostic::codes` module.
4. **Wire trait impl** in the owning subsystem.
5. **Add unit test** asserting code + severity + message round-trips.
6. **CI gate**: `cargo test -p touring-core diagnostic` must pass.

---

## 9. Versioning

This RFC follows semver:
- **Major** (2.0.0) — breaking change to trait signature or JSON shape.
- **Minor** (1.1.0) — adding new codes, new severity level.
- **Patch** (1.0.1) — message text refinements only.

Codes themselves NEVER change semantics across minor versions. A code
once published is forever associated with the same condition.

**Deprecation**: codes can be marked `deprecated` (still emitted but
flagged) and removed only in major versions, with a 3-month transition
window.

---

## 10. References

- THSF RFC-001 §5.3 — manifest schema diagnostic codes (template)
- Rust compiler — `Exxxx` codes (https://doc.rust-lang.org/error_codes/)
- TypeScript — `TSxxxx` codes
- PMAT 3.15.0 — `CB-NNNN` compliance codes
- ESLint — rule names as codes
- `serde_json` — wire format

---

**Status**: RFC-100 v1.0.0 PUBLISHED 2026-04-24.
