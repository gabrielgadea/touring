# REGRA #17 — Entity Identity Determinism (CONSTITUTIONAL)

> **Version**: v1.0 | **Date**: 2026-05-09 | **Authority**: Gabriel Gadea | **Source**: RFC-004 Entity Identity Registry
> **Auto-load stub**: `~/.claude/rules/entity-identity.md` (8L pointer)
> **This file is loaded on demand** when working on `touring-identity` crate or RFC-004 features.

---

## Princípio

Entity identity is **deterministic**, not emergent. An EntityId is derived from canonical name + admission criteria, NOT from memory address, creation order, or process ID. This is the core of semantic determinism in TACO.

---

## Formal Definition

| Property | Requirement |
|---|---|
| **Canonical name** | Stable, never renamed post-admission |
| **Admission criteria** | Defined in RFC-004 Criterion type |
| **Derivation function** | `EntityId::derive(canonical_name, admission_criteria)` — pure, total |
| **No temporal coupling** | EntityId does NOT encode creation timestamp or sequence number |
| **Revision stability** | Same inputs ALWAYS produce same EntityId across sessions |

---

## Violations

| Anti-padrão | Detection |
|---|---|
| EntityId derived from uuid or rand | VGP cross-check: RFC-004 defines deterministic derivation |
| EntityId encodes creation order | Cadeia 4 homonimia: check EntityKind definition |
| Runtime mutation of EntityId fields | Immutable struct invariant |

---

## RFC-004 Reference

- Entity types: `EntityId`, `EntityKind`, `Criterion`, `MatchKind`, `Resolution`
- Schema: `RFC-004-entity-identity-registry.md`
- Implementation: `touring-identity/src/types.rs` (verified 2026-05-09)
