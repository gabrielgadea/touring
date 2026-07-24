# RFC-001 — Manifest Schema

**Status**: NORMATIVE
**Version**: 1.0.0
**Date**: 2026-04-24
**Editor**: Gabriel Gadea
**Supersedes**: —
**Depends on**: THSF-SPEC-v1.0.0
**Canonical schema file**: `~/.claude/tools/holon/holon-manifest.schema.json`

---

## 1. Purpose

This RFC defines the binding schema for `.holon/manifest.toml` files — the
single point of truth for every holon's contract with the THSF holarchy.

Any implementation that discovers, validates, or invokes holons MUST reject
manifests that fail to validate against this schema.

---

## 2. File location and format

### 2.1 Location

- **MUST** be at `<project-root>/.holon/manifest.toml`.
- **MUST NOT** use any alternative path (no `holon.toml` at the project root,
  no `.thsf/`, no `manifest.yaml`).

Rationale: a single canonical path allows discovery to be a pure glob pass,
and lets the framework be removed by a single `rm -rf */.holon/`.

### 2.2 Format

- **MUST** be valid TOML 1.0.0.
- **MAY** carry a first-line comment pointing at the JSON Schema:

  ```toml
  # schema: https://gadea.local/thsf/holon-manifest.schema.json
  ```

  This comment is informative only (editors use it for completion); the
  validator ignores it.

### 2.3 Encoding

- **MUST** be UTF-8.
- **SHOULD** use LF line endings (CRLF accepted for Windows compatibility).

---

## 3. Top-level shape

A conformant manifest is a TOML document containing exactly one top-level
table named `holon`. Nothing outside `[holon]` is allowed.

```toml
[holon]
# identity, offers, requires, mediator — see §4
```

The `holon` table MUST contain `identity`. It MAY contain `offers`,
`requires`, and `mediator`.

```
holon: {
  identity: {...}   # REQUIRED
  offers:   {...}   # OPTIONAL
  requires: {...}   # OPTIONAL
  mediator: {...}   # OPTIONAL
}
```

---

## 4. Sections

### 4.1 `[holon.identity]` — REQUIRED

| Field | Type | Required | Constraint |
|---|---|---|---|
| `name` | string | **yes** | `^[a-z0-9][a-z0-9_-]*$` — kebab- or snake-case |
| `version` | string | no (default `"0.1.0"`) | semver 2.0 |
| `description` | string | no | free-form, one line recommended |
| `autonomy_guarantee` | bool | no (default unset) | `true` asserts build independence |

`name` uniqueness scope: across the entire holarchy reachable from the scan
root. Colliding names cause discovery to emit a `duplicate-name` diagnostic
and skip both manifests.

`autonomy_guarantee = true` is a statement by the author that the holon
compiles, tests, and runs with the `.holon/` directory deleted. It is NOT
automatically verified — implementations MAY add a conformance test that
temporarily renames `.holon/` and runs the build (see §8).

### 4.2 `[holon.offers.<cap-name>]` — OPTIONAL

For each capability offered, exactly one table named after the capability.
Capability names follow the same pattern as `identity.name`
(`^[a-z0-9][a-z0-9_-]*$`).

| Field | Type | Required | Constraint |
|---|---|---|---|
| `schema` | string | no (but RECOMMENDED) | relative path from `.holon/` to JSON Schema or `.wit` |
| `adapter` | enum | **yes** | one of `cli`, `capnp`, `wasm` |
| `adapter_cmd` | string | **yes if** `adapter=cli` | argv[0] resolved against holon root |
| `capnp_socket` | string | **yes if** `adapter=capnp` | absolute socket path, may use `$XDG_RUNTIME_DIR` |
| `wasm_component` | string | **yes if** `adapter=wasm` | relative path to `.wasm` file |
| `version` | string | no (default `identity.version`) | semver 2.0 |
| `content_hash` | string | no (SHOULD for `wasm`) | `sha256:<hex>` or `blake3:<hex>` |

**Exactly-one rule**: the presence of `adapter_cmd`, `capnp_socket`, and
`wasm_component` is mutually exclusive. A JSON Schema `oneOf` enforces this
(see §7).

### 4.3 `[holon.requires.<cap-name>]` — OPTIONAL

For each capability the holon wants from peers.

| Field | Type | Required | Constraint |
|---|---|---|---|
| `optional` | bool | no (default `false`) | if `true`, missing capability is not an error |
| `fallback` | string | no | free-form hint (e.g. `"native"`, `"no-op"`) |
| `min_version` | string | no | semver 2.0 floor |
| `max_version` | string | no | semver 2.0 ceiling (v1.1+) |

Compatibility check during handshake (RFC-002 §3):

```
for each requires.cap:
  find offerer o with o.offers[cap] defined
  pass iff semver(o.offers[cap].version).major == semver(cap.min_version).major
          and semver(o.offers[cap].version) >= semver(cap.min_version)
```

### 4.4 `[holon.mediator]` — OPTIONAL

Observability hints. Not used by the core framework but honored by
implementations that emit telemetry.

| Field | Type | Required | Constraint |
|---|---|---|---|
| `observability` | enum | no | `otlp`, `stdout`, or `none` |
| `log_path` | string | no | absolute filesystem path |

---

## 5. Validation rules

### 5.1 Syntactic

1. TOML parser MUST accept the document.
2. All required fields MUST be present.
3. All field types MUST match §4.
4. All string patterns MUST match their regex.

### 5.2 Semantic

1. Every `[holon.offers.X]` with `adapter=cli` MUST have `adapter_cmd`.
2. Every `[holon.offers.X]` with `adapter=capnp` MUST have `capnp_socket`.
3. Every `[holon.offers.X]` with `adapter=wasm` MUST have `wasm_component`.
4. `adapter_cmd` MUST NOT contain shell metacharacters listed in
   THSF-SPEC §7.3 (semicolon, ampersand, pipe, backtick, dollar-paren).
5. `adapter_cmd` MUST NOT contain `..` path segments.
6. Paths in `schema`, `wasm_component` MUST be relative and MUST NOT
   traverse upward out of the holon directory.

### 5.3 Diagnostic codes

When validation fails, implementations MUST emit a structured diagnostic:

```json
{
  "file": ".holon/manifest.toml",
  "line": 12,
  "column": 5,
  "code": "thsf-manifest-001",
  "message": "adapter=cli requires adapter_cmd",
  "severity": "error"
}
```

Reserved diagnostic codes:

| Code | Meaning |
|---|---|
| `thsf-manifest-001` | Required field missing |
| `thsf-manifest-002` | Type mismatch |
| `thsf-manifest-003` | Pattern mismatch |
| `thsf-manifest-004` | Exactly-one violation in offer adapter |
| `thsf-manifest-005` | Shell metacharacter in adapter_cmd |
| `thsf-manifest-006` | Path traversal detected |
| `thsf-manifest-007` | Duplicate holon name |
| `thsf-manifest-008` | Unknown top-level key (not `holon`) |

---

## 6. Examples

### 6.1 Minimal P0 (Discoverable only)

```toml
[holon.identity]
name = "docs-only-holon"
version = "0.1.0"
description = "Reference-only, no executable capabilities."
autonomy_guarantee = true
```

### 6.2 P1 offerer (cli adapter)

```toml
[holon.identity]
name = "traffic-graph"
version = "2.1.0"
autonomy_guarantee = true

[holon.offers.traffic-graph]
schema = "schemas/traffic-graph.json"
adapter = "cli"
adapter_cmd = "bin/traffic-graph"
version = "2.1.0"
```

### 6.3 P2 full participant (multi-adapter, requires)

```toml
[holon.identity]
name = "analise-geo-engine"
version = "1.5.2"
description = "EVTEA modeling for road concessions"
autonomy_guarantee = true

[holon.offers.monte-carlo-stochastic]
schema = "schemas/monte-carlo.json"
adapter = "cli"
adapter_cmd = "python3 -m analise.mef.monte_carlo"

[holon.offers.evtea-solve]
schema = "schemas/evtea.json"
adapter = "capnp"
capnp_socket = "$XDG_RUNTIME_DIR/holon/evtea.sock"
version = "1.0.0"

[holon.requires.symbol-index]
optional = true
fallback = "native"
min_version = "30.0.0"

[holon.requires.quality-gate]
optional = true
min_version = "1.0.0"

[holon.mediator]
observability = "otlp"
```

### 6.4 P1 offerer (wasm adapter, hashed)

```toml
[holon.identity]
name = "generator-health"
version = "0.1.0"
autonomy_guarantee = true

[holon.offers.generator-health]
schema = "schemas/generator-health.wit"
adapter = "wasm"
wasm_component = "dist/generator-health.wasm"
content_hash = "blake3:abc123...def789"
version = "0.1.0"
```

---

## 7. JSON Schema (excerpt)

The full schema is at
`~/.claude/tools/holon/holon-manifest.schema.json`. Key constraints:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://gadea.local/thsf/holon-manifest.schema.json",
  "title": "THSF Holon Manifest",
  "type": "object",
  "required": ["holon"],
  "additionalProperties": false,
  "properties": {
    "holon": {
      "type": "object",
      "required": ["identity"],
      "additionalProperties": false,
      "properties": {
        "identity": { "$ref": "#/$defs/identity" },
        "offers":   { "$ref": "#/$defs/offersMap" },
        "requires": { "$ref": "#/$defs/requiresMap" },
        "mediator": { "$ref": "#/$defs/mediator" }
      }
    }
  },
  "$defs": {
    "offer": {
      "oneOf": [
        { "required": ["adapter_cmd"],    "properties": { "adapter": { "const": "cli" } } },
        { "required": ["capnp_socket"],   "properties": { "adapter": { "const": "capnp" } } },
        { "required": ["wasm_component"], "properties": { "adapter": { "const": "wasm" } } }
      ]
    }
  }
}
```

---

## 8. Conformance tests

Every implementation of a manifest parser MUST pass the following
canonical test fixtures (which SHOULD live at
`~/.claude/tools/holon/tests/fixtures/`):

| Fixture | Expected result |
|---|---|
| `minimal-p0.toml` | parse OK, P0 profile |
| `p1-cli.toml` | parse OK, P1 profile |
| `p2-full.toml` | parse OK, P2 profile |
| `p1-wasm-hashed.toml` | parse OK, `content_hash` honored |
| `missing-name.toml` | error, code `thsf-manifest-001` |
| `bad-name-uppercase.toml` | error, code `thsf-manifest-003` |
| `cli-no-cmd.toml` | error, code `thsf-manifest-004` |
| `adapter-cmd-semicolon.toml` | error, code `thsf-manifest-005` |
| `path-traversal.toml` | error, code `thsf-manifest-006` |
| `duplicate-names.toml` | discovery warning, code `thsf-manifest-007` |

---

## 9. Extension points for future RFCs

RFCs that extend this schema in a backward-compatible way (MINOR bump) MAY
add:

- New keys inside `[holon.identity]` — reserved names: `homepage`,
  `license`, `authors`.
- New adapter types — MUST reserve a new enum value AND add a
  `oneOf` clause.
- New fields inside `[holon.offers.*]` and `[holon.requires.*]` — MUST
  default to benign values.
- New top-level tables under `[holon]` — reserved: `experimental`,
  `signature`, `metrics`.

Breaking changes MUST bump the spec MAJOR version and provide a migration
tool in `~/.claude/tools/holon/migrate/`.

---

## 10. Security reminders

- Never spawn `adapter_cmd` through a shell interpreter. Use `execve`-style
  argv lists exclusively (the reference implementation uses Python
  `subprocess.run(argv, shell=False)`).
- Validate `capnp_socket` paths against symlink-escape before connecting.
- Verify `content_hash` before loading a `.wasm` component.
- Treat all fields as untrusted input until validated against this schema.

See THSF-SPEC §7 for full threat model.

---

## 11. Version history

| Version | Date | Summary |
|---|---|---|
| 1.0.0 | 2026-04-24 | Initial (Fase 8 D8.2.a) |

---

*End of RFC-001. Next: RFC-002 (Capability IDs + Versioning).*
