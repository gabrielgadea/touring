# REGRA #17 — Entity Identity Determinism (CONSTITUTIONAL pointer)

> **Auto-load stub** | **Full rule body**: `~/.claude/skills/Touring/references/entity-identity.md`

EntityId is **deterministic** (derived from canonical name + admission criteria), NOT emergent (no memory address, creation order, process ID). Same inputs ALWAYS produce same EntityId across sessions. RFC-004 defines `EntityId::derive(canonical_name, admission_criteria)` as pure + total. **Violations**: derive from uuid/rand, encode creation order, runtime mutation of EntityId fields. Implementation: `touring-identity/src/types.rs`.
