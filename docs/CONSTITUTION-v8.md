# CONSTITUTION v8.0 — Touring Agentic Code Orchestration

**Status**: Active
**Type**: Master Constitution Document
**Layer**: ESAA / TACO / ALL
**Author**: TACO (Constitution v8.0 Draft)
**Date**: 2026-05-09
**Version**: 1.0.0

---

## PREAMBLE

Touring v8.0 is the result of nine strategy deliveries across three horizons,
synthesizing ESAA (Event Sourcing for Autonomous Agents), context-mode,
think-in-code, and the existing touring-generator into a coherent agentic
orchestration framework.

This document is the **Constitution** — the single authoritative reference for
the entire TACO system. It synthesizes five RFCs (001–005) into one reference,
clarifies how they interlock, records the S9 deliverable table with live status,
and defines the TACO Phase Protocol that every agent must obey.

**Authority**: Gabriel Gadea. No subagent runs without explicit Horizon approval.
No RFC is amended without this document being updated to reflect the amendment.

---

## PART I — ESAA FRAMEWORK DECLARATION

### What ESAA Means for Touring

ESAA (Event Sourcing for Autonomous Agents) is the foundational philosophy:
_Treat LLMs as intention emitters under contract, not as developers with
unrestricted permissions._

Touring adopts ESAA's architectural primitives as constraints:

| ESAA Primitive | Touring Implementation | RFC |
|---|---|---|
| `activity.jsonl` append-only event log | `touring-activity` crate — monotonic seq, SHA-256 projection_hash | RFC-001 |
| Boundary contracts per task_kind | VGP Layer 5 Path Boundaries — globset enforcement per TaskKind | RFC-003 |
| PARCER profiles (6-dim behavioral contract) | 5 PARCER YAML profiles in `~/.claude/agents/` | RFC-002 |
| 7-layer validation pipeline | VGP typestate + `validate_plan()` in `pipeline.rs` | RFC-005 |
| `output.rejected` error catalog | 7 error codes in `touring-activity` | RFC-001 |
| Immutability invariant | VGP Layer 6 — CommittedHistory blocks re-commit | RFC-005 |
| Entity Identity Registry | `touring-identity` crate — EntityId, Criterion, Resolution | RFC-004 |

---

## PART II — THE FIVE FOUNDATIONAL RFCS

### RFC-001: Activity Event Catalog (D9.1 — COMPLETE ✅)

**File**: `docs/RFC-001-activity-event-catalog.md`

The `touring-activity` crate delivers an append-only event store with monotonic
`seq`, SHA-256 `projection_hash`, and deterministic replay verification.

**Key types**:
- `EventAction` (13 variants including `BoundaryViolation`)
- `EventId` format: `{nanoseconds}-{sha256_prefix}`
- `Actor` enum: `Agent | System | Daemon`
- 7 `output.rejected` error codes: `SEQ_GAP`, `SEQ_DUPLICATE`, `INVALID_EVENTID`,
  `HASH_MISMATCH`, `PAYLOAD_TOO_LARGE`, `INVALID_ACTOR`, `TIMESTAMP_REGRESSION`

**Schema drift note**: `BoundaryViolation` (defined in `event.rs:27`) is NOT in the
JSON Schema enum in `event.schema.json` (lines 24-37). This is a known drift item
for D9.7 audit script #6.

**Invariants**:
- I1: `seq` strictly monotonic across 10k+ events
- I2: SHA-256 of event reproduces `projection_hash` exactly
- I3: No `seq` gap > 1 between consecutive events
- I4: All events pass `verify_projection()`
- I5: `BoundaryViolation` events include `task_kind`, `file_path`, `violation_kind`

---

### RFC-002: PARCER Profile Schema (D9.2 — COMPLETE ✅)

**File**: `docs/RFC-002-parcer-profile-schema.md`

PARCER (Persona·Audience·Rules·Context·Execution·Response) is a 6-dimensional
behavioral contract for all 5 TACO subagents. Each dimension constrains what a
subagent may infer, what it must verify, and how it must respond.

| Dim | Name | Format |
|-----|------|--------|
| P | **Persona** | YAML block — role identity and behavioral constraints |
| A | **Audience** | YAML block — primary consumer + calibration expectations |
| R | **Rules** | YAML list — hard (ALWAYS/NEVER) and soft (SHOULD) |
| C | **Context** | YAML map — `inject` and `never_inject` fields |
| E | **Execution** | Ordered list of phases ("Phase N: ...") |
| R | **Response** | YAML map — `format.schema_ref` + `valid_examples` + `invalid_examples` |

**Existing PARCER profiles** (all confirmed in `~/.claude/agents/`):

| Agent | File | Lines | Hard Rules |
|-------|------|-------|------------|
| touring-scouter | `touring-scouter.parcer.yaml` | 78L | 12 (VP-Scout v1.1, 7 chains) |
| touring-architect | `touring-architect.parcer.yaml` | 78L | 10 (MCTS min 3 paths, Context7) |
| touring-engineer | `touring-engineer.parcer.yaml` | 79L | 10 (mode=acceptEdits, REGRA #0) |
| touring-auditor | `touring-auditor.parcer.yaml` | 76L | 10 (Phase 4.5 mandatory) |
| touring-scriber | `touring-scriber.parcer.yaml` | 70L | 10 (TOON v1.0, REGRA #13) |

---

### RFC-003: Path Boundaries Contract (D9.3 — COMPLETE ✅)

**File**: `docs/RFC-003-path-boundaries-contract.md`

VGP Layer 5 enforces `TaskKind`-specific read/write allowlists via globset.
Prefix-match semantics: `"crates/foo"` matches `"crates/foo/src/lib.rs"`.

**6 TaskKind presets**:

| TaskKind | Write allowed | Forbidden | Enforcement |
|----------|--------------|-----------|-------------|
| `Spec` | `docs/`, `spec/`, `*.md` | `crates/**`, `src/**`, `tests/**` | FailClosed |
| `Impl` | `crates/**`, `src/**`, `tests/**` | `docs/**`, `spec/**` | FailClosed |
| `QA` | `tests/**`, `benches/**` | `crates/*/src/**`, `docs/**` | FailClosed |
| `Doc` | `docs/**`, `*.md` | `crates/**`, `src/**`, `tests/**` | FailClosed |
| `Audit` | `docs/audit/**` | `crates/**`, `src/**`, `tests/**` | WarnOnly |
| `Hotfix` | `crates/**`, `src/**` | `docs/**`, `spec/**` | FailClosed |

**Key types**: `PathBoundaries`, `BoundaryValidator`, `BoundaryViolation`,
`ViolationKind` (`ForbiddenWrite | NotAllowedWrite`), `BoundaryResult`
(`Valid | Warnings | Violations`).

**Glob evaluation order**:
1. If path matches `forbidden_write` → `ForbiddenWrite`
2. Else if path does NOT match `write` → `NotAllowedWrite`
3. Else → `Valid`

---

### RFC-004: Entity Identity Registry (D9.4 — COMPLETE ✅)

**File**: `docs/RFC-004-entity-identity-registry.md`

VGP Layer 6 (Entity Registry contract) establishes canonical symbol identity
across the multi-crate workspace. The registry enables reliable symbol
resolution with confidence tiers, preventing homonimia errors that plagued
earlier scout reports.

**Key types**:
- `EntityId` — SmolStr-backed interned identifier. Format: `crate::module::symbol`
- `EntityKind` — 9 variants: `Function | Type | Module | Constant | Trait | Macro | File | Config | Unknown`
- `Criterion` — 3 constructors: `exact_name`, `fuzzy_name(max_ed)`, `context_scoped(crate_pattern, symbol)`
- `Entity` — full record with 9 fields including `auto_seeded` and `canonical` flags
- `MatchKind` — 5 confidence tiers: `Exact(1.0) | ContextScoped[0.95,0.99] | Fuzzy[0.70,0.85] | Ambiguous[0.60,0.80] | NotFound(0.0)`
- `Resolution` and `EntityCandidate` — resolution algorithm types
- `EntityRelation` — directed link with `RelationKind` (`DerivedFrom | Refines | Supersedes | Equivalent | SeeAlso | Wraps`)

**Limits**:
- `MAX_CANONICAL_LEN = 256` bytes
- `MAX_CRITERIA_COUNT = 32` per entity

**D5.8 flags**:
- `auto_seeded = true`: entity from touring index, unconfirmed
- `canonical = true`: user confirmed via `touring entity confirm`

Only canonical entities appear in high-confidence resolution results.

---

### RFC-005: 7-Layer Validation Pipeline (D9.5 — COMPLETE ✅)

**File**: `docs/RFC-005-seven-layer-validation-pipeline.md`

VGP enforces all `GeneratorPlan` submissions through a 7-layer pipeline before
the typestate machine can advance. Each layer contributes a `LayerResult` to a
`ValidationReport`.

| Layer | Name String | Validates | Blocking? |
|-------|-----------|-----------|-----------|
| L1 | `l1_json_parse` | Plan is valid JSON (no-op — serde already validated) | YES (hard) |
| L2 | `l2_schema` | Plan fields conform to schema (no-op — serde validated) | YES (hard) |
| L3 | `l3_vocabulary` | `GeneratorKind` is in allowed set | NO (advisory) |
| L4 | `l4_state_machine` | Status transitions legal (no-op — typestate enforces) | YES (hard) |
| L5 | `l5_path_boundary` | Artifact paths respect `Contracts.path_boundaries` | NO (advisory per mode) |
| L6 | `l6_immutability` | Target path not in `CommittedHistory` | YES (hard) |
| L7 | `l7_verification_gate` | Composite health score ≥ 0.85 | YES (hard) |

All 7 layers always run to completion — the pipeline does not short-circuit on
error. Error results have `elapsed_ms = 0` and `score = 0.0`.

The `LayerCompleteObserver` (`Box<dyn Fn(ValidationLayer, &LayerResult)>`) is the
S1 (Activity Log) wiring point — fires after every layer for event emission.

---

## PART III — HOW THE RFCS INTERLOCK

```
PARCER (RFC-002) ─────────────────────────────────────────────────────────►
  │ binds all 5 subagent behaviors                                         │
  │                                                                          │
  │  scouter/architect/engineer/auditor/scriber each                      │
  │  cite symbols in JSON output ──────────────────────────────────────┐   │
  │                                                                       │   │
  ▼                                                                       │   │
Entity Registry (RFC-004) ◄──────────────────────────────────────────────┘
  │ provides canonical symbol identity + confidence tiers                  │
  │                                                                          │
  ▼                                                                      │
VGP Layer 5 (RFC-003) ◄── Contracts.path_boundaries ── L5_PathBoundary   │
  │  globset enforcement per TaskKind                                        │
  │                                                                       │
  ▼                                                                       │
VGP Layer 6 ◄── Contracts.entities_must_exist ── EntityIdRef resolution │
  │  (Entity Registry contract)                                            │
  │                                                                       │
  ▼                                                                       │
7-Layer Pipeline (RFC-005) ◄── typestate transitions call validate_plan() │
  │                                                                          │
  ▼                                                                          │
Activity Log (RFC-001) ◄── LayerCompleteObserver fires per layer ───────────►
  │  append-only event store with SHA-256 projection_hash                  │
  │                                                                          │
  ▼                                                                          │
touring-daemon ── composite_health_score ──► L7 VerificationGate (≥ 0.85) ──► COMMIT
```

The chain is:
1. PARCER binds subagent behavior (what they may cite, how they must verify)
2. Subagents cite symbols → Entity Registry resolves with confidence
3. VGP L5 validates artifact paths against TaskKind boundaries
4. VGP L6 validates entity references via `entities_must_exist`
5. All 7 layers run via `validate_plan()` during typestate transitions
6. `LayerCompleteObserver` fires for each layer → Activity Log (RFC-001)
7. L7 VerificationGate requires `composite_health_score ≥ 0.85`
8. Success → `Committed` typestate; CommittedHistory updated for L6

---

## PART IV — S9 DELIVERABLE TABLE (CONSTITUTION v8.0)

S9 is the final Horizon (H3) of the Touring v8.0 Master Plan, delivering the
Constitution v8.0 — the definitive reference for all TACO agents.

**Approval gate** (from master plan line 68-69):
> Gabriel approves each Horizon (H1, H2, H3) before the engineer subagents
> are dispatched. No subagent runs without an explicit "iniciar H3" directive.

| ID | Deliverable | Effort | Status | File |
|----|-------------|--------|--------|------|
| D9.1 | Activity Event Catalog RFC | M | ✅ DONE | `docs/RFC-001-activity-event-catalog.md` |
| D9.2 | PARCER Profile Schema RFC | M | ✅ DONE | `docs/RFC-002-parcer-profile-schema.md` |
| D9.3 | Path Boundaries Contract RFC | M | ✅ DONE | `docs/RFC-003-path-boundaries-contract.md` |
| D9.4 | Entity Identity Registry RFC | XL | ✅ DONE | `docs/RFC-004-entity-identity-registry.md` |
| D9.5 | 7-Layer Validation Pipeline RFC | M | ✅ DONE | `docs/RFC-005-seven-layer-validation-pipeline.md` |
| D9.6 | **Constitution v8.0 master doc** | XL (~1500L) | 🔄 IN PROGRESS | `docs/CONSTITUTION-v8.md` (this file) |
| D9.7 | 12 audit scripts + E2E suite | L | ⏳ PENDING | `audits/2026-05-09-taco-constitution/` |
| D9.8 | 3 pilot projects | M | ⏳ PENDING (requires Gabriel approval) | TBD |
| D9.9 | Touring Skill v5.0.0 | M | ⏳ PENDING | `skills/Touring/SKILL.md` bump v4.32→v5.0 |
| D9.10 | CLAUDE.md REGRA #17 (Entity Identity) | M | ⏳ PENDING | `CLAUDE.md` amendment |
| D9.11 | Release tag content | M | ⏳ PENDING | `~/.claude/touring/release-tag-v31.0.0.txt` |
| D9.12 | 13-day bug-fix contingency | XL | ⏳ PENDING (after D9.8 pilots) | reserved |

**Status key**: ✅ DONE · 🔄 IN PROGRESS · ⏳ PENDING · ❌ BLOCKED

**Schema drift items** (for D9.7 audit):
- `BoundaryViolation` in `event.rs:27` but NOT in `event.schema.json` enum → fix in audit script #6

---

## PART V — TACO PHASE PROTOCOL v6.2 (SUMMARY)

Full protocol in `~/.claude/rules/TACO-subagent.md`. This section is the
one-paragraph summary for reference.

```
FASE 0 ──► HEALTH GATE (cargo check + touring doctor) ── BLOQUEIA
FASE 1 ──► SCOUT (parallel agents) ──► sequential-thinking PROCESSA
FASE 2 ──► ARCHITECT (parallel) ──► sequential-thinking PROCESSA
FASE 3 ──► CONTEXT7 best practices ──► DECISÃO
FASE 4 ──► DECOMPOSE (sequential-thinking) ──► subtasks
FASE 4.5 ► PRE-IMPL AUDIT ── Auditor blocks FALSE_POSITIVEs
FASE 5 ──► ENGINEERS (parallel/sequential per DAG)
FASE 6 ──► POST-IMPL AUDIT (parallel)
FASE 7 ──► DOCUMENTAÇÃO completa
```

**Phase 0 health gate is NON-NEGOTIABLE** — if `cargo check --workspace` or
`touring doctor -j` fails, no subsequent phase runs.

**Symbol Verification Table** (CONSTITUTIONAL, from TACO-subagent.md):

| Role | Field | Categories |
|------|-------|-----------|
| Scouter | `cited_symbols` | `found` / `found_via_grep` / `not_found` |
| Architect | `symbol_verification` | `verified_existing` / `to_be_created` / `unverified_planned` |
| Engineer | `symbol_verification` | `imported_existing` / `created_this_subtask` / `modified_existing` |
| Auditor | `vgp_cross_verification` | re-execute CLI on ≥ 50% upstream sample |
| Scriber | `documented_symbols` | `verified_existing` / `planned_future` / `deprecated_removed` |

Anti-padrões: `BLOCKED_INVENTED_SYMBOL` (composite=0.0) — never cite a symbol
without `touring index find` evidence or explicit `to_be_created` justification.

---

## PART VI — HARD RULES SUMMARY

Full rules in `~/.claude/CLAUDE.md` and `~/.claude/rules/`.

| # | Rule | Summary |
|---|------|---------|
| REGRA #0 | POTENCIALIZAR | Zero orphan pub symbols; integrate or remove |
| REGRA #11 | GIT PROIBIDO | Never `git` — Touring is source of truth |
| REGRA #12 | DISK HYGIENE | `target/` is cache; profile.dev defensive + safe-clean.sh |
| REGRA #13 | SKILL HYGIENE | SKILL.md < 500L, name ≤ 64 chars, description ≤ 1024 |
| REGRA #14 | AGENTIC PARADIGM | All code via `taco-forge perfect-*`; no Write/Edit in code files directly |
| REGRA #15 | SYMBOL VERIFICATION | Every cited symbol needs CLI evidence or `to_be_created` |
| REGRA #16 | CLAUDE.MD GUARD | No bloat — soft limit 300L, hard limit 400L |

---

## PART VII — FILE INDEX

### Constitution Documents

| File | Purpose |
|------|---------|
| `docs/CONSTITUTION-v8.md` | **This file** — master constitution |
| `docs/RFC-001-activity-event-catalog.md` | Activity log schema (D9.1) |
| `docs/RFC-002-parcer-profile-schema.md` | PARCER contract (D9.2) |
| `docs/RFC-003-path-boundaries-contract.md` | VGP L5 path boundaries (D9.3) |
| `docs/RFC-004-entity-identity-registry.md` | Entity identity schema (D9.4) |
| `docs/RFC-005-seven-layer-validation-pipeline.md` | VGP 7-layer pipeline (D9.5) |
| `docs/RFC-001-activity-event-catalog.md` | Activity log schema |

### PARCER Profiles

| File | Agent | Lines |
|------|-------|-------|
| `~/.claude/agents/touring-scouter.parcer.yaml` | touring-scouter | 78L |
| `~/.claude/agents/touring-architect.parcer.yaml` | touring-architect | 78L |
| `~/.claude/agents/touring-engineer.parcer.yaml` | touring-engineer | 79L |
| `~/.claude/agents/touring-auditor.parcer.yaml` | touring-auditor | 76L |
| `~/.claude/agents/touring-scriber.parcer.yaml` | touring-scriber | 70L |

### Reference Implementation

| File | Purpose |
|------|---------|
| `crates/touring-activity/src/event.rs` | EventAction, Actor, EventId, Event, projection_hash |
| `crates/touring-activity/src/store.rs` | Append-only store with seq enforcement |
| `crates/touring-activity/schemas/event.schema.json` | JSON Schema (draft-07) |
| `crates/touring-generator/src/validate/boundary.rs` | L5 PathBoundary validator |
| `crates/touring-generator/src/validate/pipeline.rs` | 7-layer validation pipeline |
| `crates/touring-generator/src/plan/contracts.rs` | Contracts, PathBoundaries, TaskKind, BoundaryEnforcement, EntityIdRef |
| `crates/touring-identity/src/types.rs` | EntityId, EntityKind, Criterion, Entity, EntityRelation, MatchKind, Resolution |
| `~/.claude/rules/TACO-subagent.md` | Full TACO phase protocol |
| `~/.claude/rules/VP-Scout.md` | VP-Scout v1.1 with 7 chains |
| `~/.claude/rules/touring-cli-index.md` | CLI command ranks Tier 1-9 |
| `~/.claude/rules/touring-rebuild.md` | Touring binary + daemon lifecycle |

---

## PART VIII — MASTER PLAN REFERENCE

**Source**: `~/.claude/plans/2026-05-08-touring-v8-master-plan.md`

### Three Horizons Status

| Horizon | Strategies | Effort | Status |
|---------|-----------|--------|--------|
| H1 (Quick Wins) | S1 + S2 + S3 | 6d | ✅ DONE |
| H2 (Foundation) | S4 + S5 + S6 | 19d | ✅ DONE |
| H3 (Strategic) | S7 + S8 + S9 | 47d | S7 ✅ S8 ✅ **S9 IN PROGRESS** |
| **TOTAL** | 9 strategies | **72d + 13d contingency** | S1–S8 DONE · S9 pending |

### Constitution v8.0 Delivery Targets

S9 (Constitution v8.0) contains 12 deliverables (D9.1–D9.12). The first
five RFCs (D9.1–D9.5) are complete ✅. This document (D9.6) is the master
constitution synthesizing them. The remaining seven (D9.7–D9.12) are pending.

---

## APPENDIX A — Relationship to ESAA

ESAA specifies that agents operate under explicit behavioral contracts rather
than implicit instruction following. The ESAA gap (line 98 of master plan) was
"PARCER profiles — subagents are .md, not 6-dim PARCER YAML". S3 closed this gap.
The remaining ESAA gaps (immutability invariant, single-writer, replay verify)
are addressed by RFC-001 (Activity Log) and RFC-005 (7-layer pipeline).

All 5 TACO subagents implement ESAA's PARCER contract. No deviation from the
6-dimension structure is permitted without RFC amendment and this document
being updated to reflect the amendment.

---

## APPENDIX B — Schema Drift Register

Known schema drift items (to be fixed in D9.7 audit):

| Item | Location | Description | Fix |
|------|----------|-------------|-----|
| BD-001 | `event.schema.json` lines 24-37 | `BoundaryViolation` not in EventAction enum | Add to JSON Schema enum |
| BD-002 | `event.rs:27` | `BoundaryViolation` comment references S4/D4.6 but enum missing from schema | Coordinate with RFC-001 |

---

## CHANGE LOG

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-05-09 | Initial draft (Constitution v8.0) — synthesized from RFC-001 through RFC-005 |

---

**CONSTITUTION v8.0 — The definitive TACO reference document**
**S9 Horizon (H3) · Constitution v8.0 Draft · 2026-05-09**