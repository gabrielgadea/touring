# RFC-002 — Capability IDs + Versioning

**Status**: NORMATIVE
**Version**: 1.0.0
**Date**: 2026-04-24
**Editor**: Gabriel Gadea
**Depends on**: THSF-SPEC-v1.0.0, RFC-001

---

## 1. Purpose

Defines the canonical **naming**, **versioning**, and **compatibility
resolution** rules for capabilities across a THSF holarchy. Without these
rules, the handshake algorithm (Layer 2) cannot produce deterministic
results, and consumers cannot safely depend on a capability's surface.

---

## 2. Capability identifiers

### 2.1 Syntax

A **capability ID** is a lowercase kebab- or snake-case string matching:

```
^[a-z0-9][a-z0-9_-]*$
```

Examples of valid IDs: `symbol-index`, `traffic_graph`, `evtea-solve`,
`u4-dot-product`, `generator-health`.

Examples of **invalid** IDs: `SymbolIndex` (uppercase), `symbol.index`
(dot), `-symbol` (leading hyphen), `123symbol` (starting digit is OK
but `123` alone is reserved), `` (empty).

### 2.2 Uniqueness scope

A capability ID is unique **within a single offerer**, not across the
holarchy. Two different holons MAY offer a capability named
`symbol-index` — consumers distinguish via the handshake tuple
`(offerer_name, capability_name, version)`.

### 2.3 Reserved prefixes

The following prefixes are reserved for framework use and MUST NOT be used
by user holons:

| Prefix | Owner | Purpose |
|---|---|---|
| `holon.*` | THSF core | Meta-capabilities (e.g. `holon.identity`) |
| `thsf.*` | THSF core | Internal framework services |
| `conformance.*` | Conformance suite | Test harnesses |
| `_experimental.*` | Any holon | Local experiments (see §5.4) |

---

## 3. Semantic versioning

### 3.1 Rule

Capability versions MUST follow **Semantic Versioning 2.0.0**:
`MAJOR.MINOR.PATCH[-prerelease][+build]`.

### 3.2 Bump semantics (binding)

| Change | Bump | Example |
|---|---|---|
| Add optional request field with default | MINOR | `{name}` → `{name, lang?="rust"}` |
| Add optional response field | MINOR | add `warnings: []` to response |
| Rename field (even aliasing) | MAJOR | `path` → `file_path` |
| Change field type | MAJOR | `line: u32` → `line: u64` |
| Tighten validation (regex, range) | MAJOR | `name: string` → `name: matches /^[a-z]+$/` |
| Loosen validation | MINOR | `status: "ok"` → `status: "ok"|"retry"` |
| Add new method/endpoint | MINOR | add `batch_query` alongside `query` |
| Remove method/endpoint | MAJOR | delete `legacy_query` |
| Fix bug without changing surface | PATCH | return correct value for edge case |
| Change documentation only | PATCH | clarify ambiguity in docstring |

### 3.3 Pre-1.0.0 rule

Before a capability reaches `1.0.0`, any MINOR bump MAY contain breaking
changes (standard semver exception for `0.y.z`). Authors SHOULD document
breaking changes in a `CHANGES.md` alongside the schema.

### 3.4 Prerelease tags

Prerelease capabilities MUST use a tag like `0.2.0-alpha.1`,
`1.0.0-rc.2`. Handshake MUST NOT select prerelease versions unless the
consumer's `min_version` explicitly includes a prerelease tag.

---

## 4. Handshake compatibility algorithm

### 4.1 Inputs

- Offerer capability version: `O = MAJOR_O.MINOR_O.PATCH_O`
- Consumer `min_version`: `M = MAJOR_M.MINOR_M.PATCH_M`
- Consumer `max_version` (optional, default `+∞`): `X`

### 4.2 Decision

```
handshake_compatible(O, M, X):
  if O.major != M.major:                return false     # major mismatch
  if semver_lt(O, M):                   return false     # below floor
  if X != +∞ and not semver_lt(O, X):   return false     # above ceiling
  if O.is_prerelease and not M.is_prerelease:
                                        return false     # prerelease opt-in
  return true
```

### 4.3 Transport selection

When a capability is offered under multiple transports (§3.3 of THSF-SPEC),
the consumer MUST follow this preference order:

1. `capnp` — when latency matters (<1ms paths)
2. `wasm` — when isolation matters (untrusted code)
3. `cli` — fallback, always works

A consumer MAY override via `[holon.requires.cap-name.prefer_adapter]`:

```toml
[holon.requires.symbol-index]
min_version = "30.0.0"
prefer_adapter = "capnp"
```

### 4.4 Multiple offerers

If multiple holons offer a matching capability, ordering rules:

1. Prefer offerer with higher version (exact match > newer minor > newer patch)
2. Break ties by offerer `name` lexicographic
3. Emit `multiple-offerers` diagnostic with the full list so operators can
   disambiguate explicitly via a `prefer_offerer` hint

---

## 5. Capability lifecycle

### 5.1 Introduction

A new capability MUST be introduced at version `0.1.0` with a JSON Schema
(or WIT) describing the interface. The capability SHOULD be marked
`stability: experimental` for at least one release cycle before promotion
to `1.0.0`.

### 5.2 Stabilization

A capability reaches `1.0.0` when:
1. Its interface has been stable for ≥ 1 MINOR release cycle of the
   containing holon.
2. At least one external consumer exists (evidenced by a `requires` entry
   somewhere in the holarchy).
3. A conformance test fixture exists in the offerer's test suite.

### 5.3 Deprecation

To deprecate a capability:
1. Bump its version with a `-deprecated.<YYYY-MM-DD>` prerelease tag.
2. Emit a `capability-deprecated` diagnostic whenever the capability is
   invoked.
3. Remove it no earlier than one full MAJOR version bump later.

### 5.4 Removal

Removing a capability requires:
1. MAJOR version bump of the offering holon.
2. Entry in the holon's `CHANGES.md` with migration instructions.
3. No `requires` edge pointing at the removed capability in the current
   holarchy (verified by `holon doctor`).

### 5.5 Experimental capabilities

Experimental capabilities MUST live under names prefixed with
`_experimental.`. They are excluded from handshake by default — consumers
must opt in explicitly:

```toml
[holon.requires._experimental.new-thing]
optional = true
experimental_ack = true   # required for opt-in
```

---

## 6. Schema evolution

### 6.1 Schema ↔ version binding

Each capability version points to exactly one JSON Schema (or WIT) via the
`schema` field in the offerer's manifest. The schema hash SHOULD be
computed at release time and included in a sidecar:

```
schemas/
  my-cap.json
  my-cap.json.blake3   # contains: <hex>  <filename>
```

### 6.2 Detecting silent drift

`holon doctor` MUST:
1. Hash every `schema` file it encounters.
2. Compare against the declared sidecar hash (if present).
3. Emit `schema-drift` diagnostic if hashes mismatch.

This catches the case where an author edits a schema without bumping the
capability version.

### 6.3 Cross-holon schema reuse

Two holons MAY point `schema` at the same file (shared schema repo). In
that case the schema's version MUST be tracked separately — e.g.:

```toml
schema = "../../shared-schemas/traffic-graph.json"
schema_version = "2.1.0"
```

---

## 7. Cross-references

### 7.1 Referencing other capabilities

An offer's JSON Schema MAY reference types defined in another capability's
schema via `$ref` with a URN:

```json
{
  "$ref": "urn:thsf:cap:traffic-graph:types.json#/$defs/Corridor"
}
```

URN format: `urn:thsf:cap:<cap-id>:<schema-file>#/<json-pointer>`.

Implementations resolve URNs by:
1. Searching the current holarchy for a holon offering `<cap-id>`.
2. Reading its `schema` file.
3. Applying the JSON Pointer.

### 7.2 Circular references

Capabilities MUST NOT form circular schema dependencies. The handshake
engine MUST detect cycles and emit `schema-cycle` diagnostic.

---

## 8. Example — full lifecycle walkthrough

### 8.1 v0.1.0 — initial experimental release

```toml
[holon.offers._experimental.fancy-analysis]
schema = "schemas/fancy-analysis.json"
adapter = "cli"
adapter_cmd = "bin/fancy"
version = "0.1.0"
```

### 8.2 v0.2.0 — field rename (breaking in 0.x)

```toml
# schemas/fancy-analysis.json:
# "inputFile" renamed to "input_file"

[holon.offers._experimental.fancy-analysis]
version = "0.2.0"   # breaking in 0.x per §3.3
```

### 8.3 v1.0.0 — promoted out of experimental

```toml
[holon.offers.fancy-analysis]   # dropped _experimental. prefix
schema = "schemas/fancy-analysis.json"
adapter = "cli"
adapter_cmd = "bin/fancy"
version = "1.0.0"
```

### 8.4 v1.1.0 — additive change

```toml
# schemas/fancy-analysis.json:
# added optional "verbosity": "normal" | "detailed"

[holon.offers.fancy-analysis]
version = "1.1.0"   # MINOR bump, backwards-compat
```

### 8.5 v2.0.0 — breaking change

```toml
# schemas/fancy-analysis.json:
# "input_file" now requires absolute path (was relative)

[holon.offers.fancy-analysis]
version = "2.0.0"   # MAJOR — consumers at min_version="1.x" reject
```

---

## 9. Diagnostics

| Code | Meaning | Severity |
|---|---|---|
| `thsf-cap-001` | Invalid capability ID syntax | error |
| `thsf-cap-002` | Reserved prefix used by user holon | error |
| `thsf-cap-003` | Invalid semver in version field | error |
| `thsf-cap-004` | min_version > max_version | error |
| `thsf-cap-005` | No compatible offerer found (non-optional requires) | error |
| `thsf-cap-006` | Multiple offerers matched — disambiguate | warning |
| `thsf-cap-007` | Schema hash mismatch (silent drift) | warning |
| `thsf-cap-008` | Experimental capability required without ack | error |
| `thsf-cap-009` | Circular schema dependency | error |
| `thsf-cap-010` | Deprecated capability invoked | warning |
| `thsf-cap-011` | Prerelease version selected without opt-in | error |

---

## 10. Consumer-side version pinning

Consumers SHOULD pin minimum versions liberally and avoid maximum versions
unless a known incompatibility exists:

```toml
# GOOD — allows the offerer to evolve within major
[holon.requires.symbol-index]
min_version = "30.0.0"

# OK — pins to a known-bad-free range
[holon.requires.broken-capability]
min_version = "1.2.3"
max_version = "1.3.0"   # 1.3.1 known to have bug X

# BAD — over-constrains, breaks on patches
[holon.requires.fragile]
min_version = "1.0.0"
max_version = "1.0.0"   # essentially pinned; no security updates
```

---

## 11. Version history

| Version | Date | Summary |
|---|---|---|
| 1.0.0 | 2026-04-24 | Initial (Fase 8 D8.2.b) |

---

*End of RFC-002. Next: RFC-003 (CRDT Semantics).*
