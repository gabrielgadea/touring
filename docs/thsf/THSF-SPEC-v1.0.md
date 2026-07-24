# THSF — Touring Holonic Symbiosis Framework

**Specification v1.0.0**
**Status**: DRAFT (Fase 8 D8.1) → STABLE after conformance suite pass
**Date**: 2026-04-24
**Editor**: Gabriel Gadea
**Reference implementations**: `~/.claude/tools/holon/holon.py` (CLI),
`~/.claude/rust/crates/touring-capnp-server/` (Cap'n Proto),
`~/.claude/rust/holon-wasm-components/` (WASM).

---

## 0. Abstract

THSF is a framework for **temporary, reversible coupling** between autonomous
software projects — *holons* — that need to discover, negotiate, and exchange
capabilities without a central broker, without a mandatory daemon, and without
lock-in to any specific LLM, RPC framework, or transport.

Four layers (Discovery → Handshake → Capability Exchange → Knowledge Sync) are
specified independently, so each holon picks the minimum subset it needs. A
holon that declares only Layer 1 is already conformant — it can be discovered
by peers even if it offers zero capabilities. A holon that implements all four
layers participates in the full symbiosis cycle.

The defining invariant is **autonomy**: every holon must build and run as if
THSF did not exist. `rm -rf */.holon/` removes 100% of the framework's
footprint from any project, with no code changes required.

---

## 1. Motivation

Modern software ecosystems need three properties that rarely coexist:

1. **Autonomy** — each project builds, tests, and deploys on its own terms.
2. **Integration** — projects share work, data, and capabilities.
3. **Reversibility** — coupling can be introduced and removed without
   rewriting either side.

Existing approaches force a choice:

| Approach | Autonomy | Integration | Reversibility |
|---|---|---|---|
| Monorepo | ✗ | ✓✓ | ✗ |
| Microservices (REST/gRPC) | ✓ | ✓ | ✗ (clients hard-code URLs) |
| Plugin systems (shared ABI) | ✗ | ✓ | ✗ |
| MCP / A2A (broker-based) | ✓ | ✓ | ~ (broker required) |
| **THSF** | **✓** | **✓** | **✓** |

THSF achieves all three by keeping the coupling **declarative** (TOML files),
**filesystem-native** (no runtime broker required), and **scoped** (no holon
imports any THSF code at build time).

---

## 2. Core Concepts

### 2.1 Holon

A **holon** is any software project (Rust crate, Python package, JS app, Go
binary, shell script tree, …) that declares a manifest at `.holon/manifest.toml`.

Every holon has an **identity** (unique `name` within the holarchy) and a
declaration of:
- what it **offers** (capabilities external holons can invoke)
- what it **requires** (capabilities from peers — may be `optional`)

### 2.2 Capability

A **capability** is a named, versioned unit of functionality with a typed
interface. Names are lowercase kebab-case (e.g. `symbol-index`,
`traffic-graph`, `generator-health`). Every capability declares:

- A **transport adapter**: `cli` (subprocess), `capnp` (Cap'n Proto over Unix
  socket), or `wasm` (WebAssembly component).
- A **JSON Schema** (or WIT interface, for WASM) that documents the request
  and response shape.
- A **semver version** so consumers can pin or negotiate compatibility.

### 2.3 Manifest

The manifest is the single source of truth about a holon's contract. It lives
at `<project-root>/.holon/manifest.toml`. The schema is RFC-001.

The presence of a manifest implies nothing about availability — the capability
may fail at runtime (binary missing, socket closed, component not built). The
manifest is a **declaration of intent**, not a guarantee.

### 2.4 Holarchy

A **holarchy** is a collection of holons sharing a filesystem root (or, in
Fase 7, a peer-to-peer network). THSF provides a deterministic discovery
algorithm that enumerates all holons reachable from a given root.

### 2.5 Symbiosis Cycle

A **symbiosis cycle** is one pass through the four layers:

```
Discovery → Handshake → Capability Exchange → Knowledge Sync
```

Each layer is optional for a holon; all four are optional for a holarchy. A
cycle runs on demand (CLI invocation) or on a schedule (systemd timer,
typically daily).

---

## 3. Architecture — Four Layers

### 3.1 Layer 1 — Discovery

**Purpose**: enumerate holons reachable from a filesystem root.

**Protocol**: recursive glob for `**/.holon/manifest.toml`, bounded by a
maximum depth (default 5), respecting `.gitignore`.

**Output**: deterministic list of `(path, manifest)` tuples sorted by `path`.

**Conformance**:
- **C1.1**: MUST produce the same output for repeated calls on an unchanged
  tree (idempotent).
- **C1.2**: MUST skip `.git/`, `.holon/state.db`, and any path matching
  `.gitignore`.
- **C1.3**: MUST tolerate malformed manifests by emitting a diagnostic and
  continuing (partial discovery beats total failure).
- **C1.4**: MUST complete in under 200 ms for a holarchy of ≤ 100 holons on
  an SSD.

**Reference impl**: `holon discover <root>` (holon.py:`HolonDiscovery.scan`).

### 3.2 Layer 2 — Handshake

**Purpose**: compute the bipartite graph of compatible offer↔require pairs.

**Input**: the list of holons from Layer 1.

**Algorithm**:
```
for each requirer r in holons:
  for each requirement q in r.requires:
    for each offerer o in holons (o ≠ r):
      if q.name ∈ o.offers.keys() and
         semver(o.offers[q.name].version) ≥ q.min_version:
          emit Handshake(requirer=r, offerer=o, capability=q.name)
```

**Output**: list of `Handshake` tuples — `(requirer, offerer, capability)`.

**Conformance**:
- **C2.1**: Handshake set MUST be deterministic (stable ordering).
- **C2.2**: If `q.optional = true` and no offerer exists, MUST NOT emit an
  error — the requirer falls back to its declared `fallback` strategy.
- **C2.3**: If `q.optional = false` (default) and no offerer exists, MUST
  emit `UnsatisfiedRequirement` diagnostic.
- **C2.4**: `handshakes_rejected` counter MUST be 0 in a healthy holarchy
  after discovery.

**Reference impl**: `holon symbiosis <root>` (holon.py:`HandshakeEngine.run`).

### 3.3 Layer 3 — Capability Exchange

**Purpose**: invoke a capability on the offering holon from the requiring
holon.

**Three transport adapters**, chosen per-capability by the offerer:

#### 3.3.1 Adapter: `cli`

- **Invocation**: subprocess spawn of `adapter_cmd` with JSON args appended
  to `argv` (execve semantics — no shell interposed).
- **Encoding**: JSON on stdin, JSON on stdout, errors on stderr.
- **Latency budget**: typically 10–100 ms (subprocess startup + work).
- **Use case**: polyglot interop with zero runtime deps; baseline.

#### 3.3.2 Adapter: `capnp`

- **Invocation**: Cap'n Proto RPC over Unix socket at `capnp_socket` path.
- **Encoding**: Cap'n Proto native wire format; zero-copy.
- **Latency budget**: 1–50 µs (in-process) to 100 µs (cross-process).
- **Use case**: performance-critical paths, promise pipelining.
- **Interface definition**: `.capnp` schema distributed out-of-band (RFC-004).

#### 3.3.3 Adapter: `wasm`

- **Invocation**: load `wasm_component` via Wasmtime, call exported function.
- **Encoding**: WIT-typed values, WASI 0.2 / Preview 3 semantics.
- **Latency budget**: 1–20 ms (component instantiation + execution).
- **Use case**: sandboxed pure functions, untrusted code, portable binaries.

**Conformance**:
- **C3.1**: A holon MAY declare the same capability under multiple transports
  — consumers select by preference (RFC-002 §4.3).
- **C3.2**: Errors MUST be reported in a transport-specific error envelope,
  but MUST preserve the original failure message.
- **C3.3**: The `touring-master` aggregator (Fase 2 reference) MUST honor
  `adapter` selection; it is a transport proxy, not a reimplementation.

**Reference impls**:
- `holon invoke` (all three transports, dispatch by `adapter` field)
- `holon-touring-adapter.py` (Python bridge)
- `generator_health_client.py` (pycapnp client, Fase 5 Wave G)

### 3.4 Layer 4 — Knowledge Sync

**Purpose**: share stateful information (lessons, patterns, counters,
rewards) across holons without central authority.

**Protocol**: Conflict-free Replicated Data Types (CRDTs) persisted in
SQLite. Two standard types:

- **LWW-Register** (Last-Write-Wins) for single values. Tie-break by
  `(actor_id, timestamp)` lexicographic.
- **Grow-Only Set** (G-Set) for accumulating observations — never removes
  entries, always merges by union.

Fase 5 Wave I demonstrated a grow-only audit trail of `HealthDeltaEvent`s as a
canonical Layer 4 application.

**Conformance**:
- **C4.1**: Merge MUST be commutative, associative, and idempotent.
- **C4.2**: Every write MUST carry an `actor_id` (unique per holon session)
  and a monotonic `timestamp_ms`.
- **C4.3**: Reads MUST return the full merged state; they MUST NOT reveal
  per-actor state except via dedicated audit APIs.
- **C4.4**: Clock skew up to ±1 hour MUST NOT produce inconsistent merges.

**Reference impl**: `~/.claude/tools/holon/holon.py::CRDTStore` (Fase 1 baseline);
`touring-hooks::health_delta_audit` (Fase 5 Wave I).

---

## 4. Topology × Layer Matrix

Seven topologies are recognized (labeled T1–T7). Each combo (A–G) in the
THSF Master Plan maps to a specific topology set:

| Topology | Name | Typical scale | Reference |
|---|---|---|---|
| T1 | Filesystem Single-Host | 10–100 holons | Fase 1 baseline |
| T2 | Typed Pair (Cap'n Proto 1:1) | 2 holons | Fase 3 |
| T3 | Ocap Graph (OCapN) | 10–∞, hostile net | Fase 6 (research) |
| T4 | CRDT Mesh | 10–100 holons | Fase 1 + Fase 5 Wave I |
| T5 | WASM Component Graph | 1:N sandboxed | Fase 4 |
| T6 | libp2p DHT (multi-host) | 100+ hosts | Fase 7 (deferred) |
| T7 | Typed Federation (capnp + schema registry) | 10–100 holons, perf crit | Fase 3 + Fase 5 |

**Matrix** (✓ = layer applies to topology):

| Topology | Discovery | Handshake | Exchange | Knowledge Sync |
|---|---|---|---|---|
| T1 | ✓ (FS) | ✓ | ✓ (`cli`) | ~ (SQLite local) |
| T2 | ✓ | ✓ | ✓ (`capnp`) | — |
| T3 | ✓ (DHT or FS) | ✓ | ✓ (OCapN) | ✓ (eventual) |
| T4 | ✓ | — | — | ✓ (CRDT) |
| T5 | ✓ | ✓ | ✓ (`wasm`) | — |
| T6 | ✓ (DHT) | ✓ (gossip) | ✓ (libp2p) | ✓ (GossipSub) |
| T7 | ✓ (FS or registry) | ✓ (typed) | ✓ (`capnp`) | ✓ (schema CRDT) |

### 4.1 Combo Mapping

| Combo | Topologies | Layers active | Fase |
|---|---|---|---|
| A — FS Baseline | T1 + T4 | 1, 2, 3(cli), 4 | Fase 1 ✅ |
| B — OCapN Symbiosis | T3 + T4 | 1, 2, 3(ocapn), 4 | Fase 6 (research) |
| C — WASM Woven | T5 + T1 | 1, 2, 3(wasm) | Fase 4 ✅ |
| D — P2P Knowledge | T6 + T4 | 1, 2, 3(libp2p), 4 | Fase 7 (deferred) |
| E — Typed Federation | T2 + T7 | 1, 2, 3(capnp) | Fase 3 ✅ |
| F — Hybrid FS+WASM+CRDT | T1 + T4 + T5 | 1, 2, 3(all), 4 | Fase 5 ✅ |
| G — Layered Polyglot Stack | meta | all | Fase 8 (this doc) |

---

## 5. Manifest Schema (Normative)

The canonical schema is defined in **RFC-001**. Summary:

```toml
# schema: holon-manifest.schema.json

[holon.identity]
name = "my-holon"              # REQUIRED, unique per holarchy, kebab-case
version = "1.2.3"              # OPTIONAL semver, defaults to 0.1.0
description = "One-line text"  # OPTIONAL
autonomy_guarantee = true      # OPTIONAL, MUST be true for conformance

[holon.offers.capability-name]
schema = "schemas/cap.json"    # JSON Schema (or .wit for wasm)
adapter = "cli"                # one of: cli | capnp | wasm
adapter_cmd = "bin/run.sh"     # REQUIRED when adapter=cli
capnp_socket = "$XDG_RUNTIME_DIR/holon/foo.sock"  # when adapter=capnp
wasm_component = "dist/foo.wasm"                   # when adapter=wasm
version = "0.1.0"              # OPTIONAL, defaults to identity version

[holon.requires.capability-name]
optional = true                # default false
fallback = "native"            # hint for graceful degradation
min_version = "0.1.0"          # semver floor

[holon.mediator]               # OPTIONAL, observability hints
observability = "otlp"         # "otlp" | "stdout" | "none"
log_path = "/var/log/holon.log"
```

**Validation**: every manifest MUST validate against
`holon-manifest.schema.json` (JSON Schema Draft 2020-12). Malformed manifests
are discovered but NOT handshaked.

---

## 6. Versioning & Compatibility

THSF uses **strict semver 2.0** at three levels:

### 6.1 Spec version

This document is `THSF-SPEC-v1.0.0`. Spec versions follow:

- **MAJOR** bump: breaking changes to manifest schema, wire protocols, or
  discovery algorithm.
- **MINOR** bump: additive features (new transports, new CRDT types) that do
  not break existing conformant implementations.
- **PATCH** bump: clarifications, typo fixes, examples.

### 6.2 Holon identity version

The `version` under `[holon.identity]` is controlled by the holon author. It
tracks the holon's own evolution, not THSF evolution.

### 6.3 Capability version

Each offered capability has its own `version`. Consumers use `min_version`
in their `[holon.requires.*]` block. Compatibility rule:

> Offerer capability version `O`. Consumer `min_version = M`.
> Handshake succeeds iff `O.major == M.major AND O ≥ M`.

Major version bumps are breaking — consumers MUST declare a new
`min_version` or suffer rejection.

### 6.4 Schema version

The `schema` field of an offer points to a JSON Schema file. When the schema
changes in a backward-incompatible way, the capability's `version` MUST be
bumped MAJOR. Tooling can hash the schema (Fase 3 D3.2 uses blake3) to detect
silent drift.

---

## 7. Security Model

### 7.1 Scope

THSF Layer 1–3 on a single host assumes **mutual trust between holons** on
the same filesystem (the user runs all of them). It is NOT a general-purpose
multi-tenant framework.

Security-hardened topologies:

- **T5 (WASM)**: Wasmtime sandbox isolates untrusted components; only
  explicit `wasi:http` or `wasi:filesystem` capabilities grant access.
- **T3 (OCapN)**: object capability model prevents ambient authority; only
  holders of a reference can invoke.
- **T6 (libp2p)**: peer IDs are ed25519 public keys; transport is Noise or
  TLS 1.3.

### 7.2 Threat model

| Threat | In-scope? | Mitigation |
|---|---|---|
| Malicious manifest (e.g. `rm -rf /`) | ✗ | User-reviewed before install |
| Tampered `.wasm` component | ✓ (T5) | Component hash pinning |
| CRDT poisoning by compromised actor | ✗ | Actor authn out of scope |
| DoS via huge manifest | ✓ | Parser bounds + timeout |
| Path traversal in `adapter_cmd` | ✓ | Spec forbids `..` segments |

### 7.3 Normative requirements

- **S1**: Implementations MUST reject manifests with `adapter_cmd` containing
  shell metacharacters (semicolon, ampersand, pipe, backtick, dollar-paren).
- **S2**: `adapter_cmd` MUST be resolved against the holon's own directory
  — never against `$PATH`.
- **S3**: Discovery MUST NOT follow symlinks that escape the scan root.
- **S4**: WASM components SHOULD be pinned by content hash
  (`sha256:<hex>` or `blake3:<hex>`) in a dedicated `content_hash` field
  (RFC-004 §3).

---

## 8. Conformance Profiles

Implementations declare one of four profiles:

### 8.1 Profile P0 — Discoverable

Minimum conformant implementation. Satisfies:
- C1.1 through C1.4 (Discovery)

A P0 holon is visible in `holon discover` but offers no capabilities. Useful
for registering documentation projects or placeholder modules.

### 8.2 Profile P1 — Offerer

P0 plus:
- At least one `[holon.offers.*]` entry
- Adapter endpoint reachable at invocation time

### 8.3 Profile P2 — Full Participant

P1 plus:
- All declared `[holon.requires.*]` entries either (a) have a matching offerer
  in the holarchy, or (b) are marked `optional = true`
- Handshake validation passes (C2.1–C2.4)

### 8.4 Profile P3 — Knowledge-Sharing

P2 plus:
- Layer 4 enabled: participates in at least one CRDT state store
- Merge conformance (C4.1–C4.4) verified

### 8.5 Conformance suite

`holon doctor --profile P<N>` MUST run the profile's checks and emit a JSON
report. Exit 0 = pass, 1 = domain failure, 2 = invocation error.

The canonical suite lives at `~/.claude/tools/holon/tests/` and is
reproducible via `pytest -v`. Reference implementation passes 37/37 tests
as of Fase 5 completion.

---

## 9. Invariants (Inviolable)

1. **Autonomy**: every holon MUST build and test as if THSF did not exist.
   `autonomy_guarantee = true` MUST be honest.
2. **Reversibility**: `find / -type d -name .holon -exec rm -rf {} +`
   MUST leave every project functional.
3. **No framework imports**: holons MUST NOT depend on `holon.py` or any
   THSF runtime library in their production build graph.
4. **Idempotence**: every THSF operation (discovery, symbiosis, handshake,
   invocation, sync) MUST be idempotent. Repeated runs on unchanged state
   MUST produce identical output.
5. **Monotonic state**: Layer 4 state MUST be grow-only or LWW-resolved —
   never destructive on merge.
6. **Transport equivalence**: the same capability name across different
   transports MUST have identical observable semantics (same inputs → same
   outputs, modulo latency).

---

## 10. Reference Implementations & Ecosystem

### 10.1 Core CLI (`holon`)

- **Path**: `~/.claude/tools/holon/holon.py` (Python 3.11+, zero deps
  beyond stdlib)
- **Subcommands**: `init`, `discover`, `symbiosis`, `invoke`, `doctor`,
  `stats`
- **Tested**: 37/37 pytest cases as of 2026-04-24

### 10.2 Cap'n Proto Server (Layer 3, `capnp`)

- **Path**: `~/.claude/rust/crates/touring-capnp-server/`
- **Schemas**: `holon-core.capnp` (Fase 3), `holon-generator.capnp` (Fase 5)
- **Bench**: P50 = 9 µs (Rust), 44 µs (Python), 29 µs (generator-health)

### 10.3 WASM Components (Layer 3, `wasm`)

- **Path**: `~/.claude/rust/holon-wasm-components/`
- **Components**: `spec-version`, `blast-radius`, `quality-gate`,
  `generator-health`
- **Compose**: `wac compose` aggregates components; runner executes
  composed graph

### 10.4 Touring Self-Integration

- **Aggregator manifest**: `~/.claude/rust/.holon/manifest.toml`
  (`touring-master`, 10+ offered capabilities)
- **Bridge**: `~/.claude/tools/holon/holon_touring_adapter.py`
- **Dashboard**: `holon stats --touring`

### 10.5 Template repositories

Fase 8 delivers three standalone templates:

- `~/projects/templates/holon-rust-template/`
- `~/projects/templates/holon-python-template/`
- `~/projects/templates/holon-ts-template/`

Each template is self-contained: it has its own build system, a minimal
`.holon/manifest.toml` exposing one trivial capability, and a README
explaining how to customize.

---

## 11. Evolution Policy

### 11.1 Changes to this spec

Changes MUST follow the RFC process (RFC-000 defines the process itself, to
be added in v1.1). Until then, changes MUST be:

1. Proposed as a pull-request-style markdown doc with clear `before/after`
2. Reviewed by the editor (Gabriel Gadea) with a 72-hour comment window
3. Labeled with proposed version bump (MAJOR / MINOR / PATCH)

### 11.2 Deprecation

Features MAY be deprecated but MUST NOT be removed before:
- One MINOR cycle of warning via `holon doctor` diagnostics
- One MAJOR cycle of dual-support

### 11.3 Experimental features

Experimental features MUST live under a `[holon.experimental.*]` TOML table
and MUST be documented with a `stability: experimental` field. They MAY be
removed at any time without a deprecation cycle.

---

## 12. Non-Goals

THSF explicitly does NOT aim to:

- Replace service meshes (Istio, Linkerd) for high-volume microservice RPC.
- Provide end-user authentication or authorization (use OIDC/OAuth2
  elsewhere).
- Act as a workflow engine (Temporal, Airflow remain appropriate for those
  use cases).
- Enforce language-level type safety (each transport has its own type
  system: JSON Schema, Cap'n Proto, WIT).
- Mandate adoption — every feature is opt-in.

---

## 13. Acknowledgments

THSF synthesizes ideas from:

- **Arthur Koestler**, *The Ghost in the Machine* (1967) — the holon concept.
- **Carl Hewitt**, Actor Model (1973) — inspiration for Layer 3.
- **Mark Miller** et al., *E* language (late 1990s) — object capabilities,
  promise pipelining (Fase 3 + Fase 6).
- **Marc Shapiro** et al., *CRDTs* (2011) — Layer 4 merge semantics.
- **Bytecode Alliance** — WASI 0.2 Component Model (Fase 4).
- **Spritely Institute** — Goblins + OCapN (Fase 6 research).
- **Cap'n Proto** (Kenton Varda) — wire format (Fase 3).
- **libp2p** (Protocol Labs) — multi-host transport (Fase 7).

---

## 14. Appendices

### A. Reserved capability names

The following capability names are reserved for future THSF extensions and
MUST NOT be used for user-defined capabilities:

- `holon.*` (meta-capabilities)
- `thsf.*` (framework internals)
- `conformance.*` (test harness)

### B. Glossary

| Term | Definition |
|---|---|
| **Holon** | Self-contained project participating in THSF via `.holon/manifest.toml` |
| **Holarchy** | Set of holons reachable from a filesystem root (Fase 1) or P2P network (Fase 7) |
| **Capability** | Named, versioned, typed unit of functionality (`symbol-index`, `traffic-graph`, …) |
| **Offer** | Capability a holon exposes for peers to invoke |
| **Require** | Capability a holon wants from peers (may be `optional`) |
| **Handshake** | Match between an Offer and a Require, passing semver compatibility |
| **Adapter** | Transport implementation: `cli`, `capnp`, or `wasm` |
| **Symbiosis** | One complete run of Discovery → Handshake → Exchange → Sync |
| **Autonomy Guarantee** | Declaration that the holon builds/runs without THSF |
| **Mediator** | Optional observability sink declared in the manifest |
| **Actor ID** | Unique identifier for a holon's writes in Layer 4 CRDTs |

### C. Version history

| Version | Date | Summary |
|---|---|---|
| 1.0.0 | 2026-04-24 | Initial public specification (Fase 8 D8.1) |

### D. References to other RFCs

- **RFC-001** — Manifest Schema (normative)
- **RFC-002** — Capability IDs + Versioning (normative)
- **RFC-003** — CRDT Semantics + Merge Protocol (normative)
- **RFC-004** — WIT Interfaces Standard (normative for `adapter=wasm`)

### E. External resources

- JSON Schema Draft 2020-12: https://json-schema.org/draft/2020-12/
- Cap'n Proto language ref: https://capnproto.org/language.html
- WASI 0.2 Component Model: https://component-model.bytecodealliance.org/
- Automerge CRDT library: https://automerge.org/
- libp2p specs: https://github.com/libp2p/specs
- Spritely Goblins: https://spritely.institute/goblins/

---

*End of THSF Specification v1.0.0.*

*For the executable plan that produced this specification, see
`docs/2026-04-23-THSF-master-plan.md`. For the Fase 8 session report, see
`docs/2026-04-24-thsf-fase8-spec-publica.md`.*
