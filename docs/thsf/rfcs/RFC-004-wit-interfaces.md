# RFC-004 — WIT Interfaces Standard

**Status**: NORMATIVE (for holons declaring `adapter = "wasm"`)
**Version**: 1.0.0
**Date**: 2026-04-24
**Editor**: Gabriel Gadea
**Depends on**: THSF-SPEC-v1.0.0, RFC-001, RFC-002
**Canonical WIT**: `~/.claude/rust/crates/touring-wasm/wit/holon-core.wit`
**Reference implementations**:
- `~/.claude/rust/holon-wasm-components/spec-version/`
- `~/.claude/rust/holon-wasm-components/blast-radius/`
- `~/.claude/rust/holon-wasm-components/quality-gate/`
- `~/.claude/rust/holon-wasm-components/generator-health/` (Fase 5 Wave H)

---

## 1. Purpose

Defines the WIT (WebAssembly Interface Type) contract that every THSF
`adapter = "wasm"` capability MUST implement. Based on the WASI 0.2
Component Model (Preview 2 stable; Preview 3 forward-compatible).

---

## 2. Toolchain (informative)

- **WIT tooling**: `wit-bindgen` 0.35+ (Rust host + guest), `wit-parser`
  for validation.
- **Target**: `wasm32-wasip2` (stable since Rust 1.82).
- **Runtime**: Wasmtime 25+ (host), supporting WASI 0.2.
- **Composition**: `wac` (WebAssembly Composition) 0.6+ for `wac compose`
  / `wac plug`.

These versions are recommendations. The canonical WIT must be consumable
by any conforming WASI 0.2 implementation.

---

## 3. Canonical world: `holon-component`

The `holon:core@0.1.0` package defines the world every component MUST
export:

```wit
package holon:core@0.1.0;

/// Per-call invocation envelope (byte-equivalent to capnp InvokeRequest).
interface types {
    record invoke-request {
        capability: string,
        args: list<u8>,
        requester: string,
        timeout-ms: u32,
    }

    record invoke-response {
        exit-code: s32,
        stdout: list<u8>,
        stderr: list<u8>,
        duration-ms: u32,
        logged: bool,
    }

    variant invoke-error {
        unknown-capability(string),
        invalid-args(string),
        internal(string),
    }
}

interface capabilities {
    use types.{invoke-request, invoke-response, invoke-error};

    list-capabilities: func() -> list<string>;
    invoke: func(request: invoke-request) -> result<invoke-response, invoke-error>;
}

world holon-component {
    export capabilities;
}
```

### 3.1 Field semantics

| Field | Purpose |
|---|---|
| `invoke-request.capability` | The capability ID being called (must match one of `list-capabilities()` output) |
| `invoke-request.args` | Opaque byte payload — each capability documents its own encoding (JSON MVP, CBOR in v1.1) |
| `invoke-request.requester` | Actor ID (RFC-003 §4) of the caller — used for audit |
| `invoke-request.timeout-ms` | Host-advised budget. Component SHOULD respect; host WILL enforce via `store.set_epoch_deadline` |
| `invoke-response.exit-code` | Unix-style status (0 = success, nonzero = domain failure) |
| `invoke-response.stdout` | Primary output (capability-specific JSON/CBOR) |
| `invoke-response.stderr` | Diagnostic messages (MAY be empty) |
| `invoke-response.duration-ms` | Execution time as measured inside the component (host also tracks) |
| `invoke-response.logged` | Whether the component internally logged the call (for audit) |

### 3.2 Error variants

| Variant | When to use |
|---|---|
| `unknown-capability(msg)` | `request.capability` not in `list-capabilities()` |
| `invalid-args(msg)` | `request.args` failed schema validation |
| `internal(msg)` | Unexpected runtime error — caller SHOULD retry or escalate |

Components MUST NOT trap (unreachable, out-of-bounds, etc.) for ordinary
domain errors — only `invoke-error` is the correct path. Traps are
reserved for genuine bugs (undefined behavior).

---

## 4. Lifecycle (informative)

### 4.1 Component build

```bash
# In the component crate's directory:
cargo component build --release --target wasm32-wasip2
# Produces: target/wasm32-wasip2/release/<name>.wasm
```

Equivalent via plain cargo (no `cargo-component` dep):

```bash
cargo build --release --target wasm32-wasip2
wasm-tools component new \
  target/wasm32-wasip2/release/<name>.wasm \
  -o target/wasm32-wasip2/release/<name>.component.wasm
```

### 4.2 Component instantiation (host-side, Rust)

```rust
use wasmtime::{Config, Engine, Store};
use wasmtime::component::{Component, Linker};
use wasmtime_wasi::p2::{WasiCtx, WasiCtxBuilder, WasiCtxView, add_to_linker_sync};

let mut config = Config::new();
config.wasm_component_model(true);
let engine = Engine::new(&config)?;

let component = Component::from_file(&engine, "path/to/component.wasm")?;

let mut linker: Linker<WasiCtxView> = Linker::new(&engine);
add_to_linker_sync(&mut linker)?;

let ctx = WasiCtxBuilder::new().build();
let mut store = Store::new(&engine, WasiCtxView::new(ctx));

let instance = linker.instantiate(&mut store, &component)?;
// Call exported functions via bindgen-generated wrappers.
```

### 4.3 Compose (aggregation)

Multiple components implementing `holon-component` MAY be composed into a
single aggregate via `wac compose`:

```bash
wac compose \
  --plug blast-radius=blast-radius.component.wasm \
  --plug quality-gate=quality-gate.component.wasm \
  --plug generator-health=generator-health.component.wasm \
  aggregate.wac \
  -o aggregate.component.wasm
```

The aggregate component exposes `list-capabilities()` returning the union
of all plugged components, and dispatches `invoke()` by capability name.

---

## 5. Capability-specific payloads

### 5.1 MVP: JSON

For v1.0.0, every capability encodes `args` and `stdout` as UTF-8 JSON.
The schema of each is documented in the capability's `schema` file (per
RFC-001 §4.2).

**Example** — `blast-radius` capability:

Request payload (`args`):
```json
{"file_path": "src/foo.rs", "max_depth": 3}
```

Response payload (`stdout`):
```json
{
  "file_path": "src/foo.rs",
  "direct_dependents": 5,
  "transitive_dependents": 23,
  "paths": ["src/bar.rs", "src/baz.rs", "..."]
}
```

### 5.2 Future: CBOR (v1.1)

For payloads exceeding ~64 KB, JSON encoding overhead becomes measurable.
RFC-004a (future) will specify CBOR as an alternative encoding, opt-in via
a `content-encoding` field in the request.

Until v1.1 ratified, implementations MUST use JSON.

### 5.3 Empty payloads

Capabilities that take no arguments or return no data MUST still encode
an empty JSON object: `args = [123, 125]` (bytes for `{}`).

---

## 6. Content hashing & integrity

### 6.1 Recommendation

Every `.wasm` component SHOULD be pinned by content hash in its
manifest:

```toml
[holon.offers.generator-health]
adapter = "wasm"
wasm_component = "dist/generator-health.wasm"
content_hash = "blake3:7f83b165..."
version = "0.1.0"
```

### 6.2 Hash verification

Before instantiation, the host MUST:
1. Read the `.wasm` file.
2. Compute its blake3 (or sha256) hash.
3. Compare against `content_hash` in the manifest.
4. Refuse to instantiate if mismatch. Emit `thsf-wasm-integrity` diagnostic.

### 6.3 Hash format

Format: `<algo>:<hex>` where `<algo>` is `blake3` or `sha256`.
BLAKE3 is PREFERRED (faster, same security properties, wider adoption
in modern toolchains).

---

## 7. Resource limits

Hosts MUST apply the following defaults unless overridden by the holon's
manifest:

| Limit | Default | Override field |
|---|---|---|
| Memory | 16 MiB | `[holon.offers.X.wasm_limits.memory_mib]` |
| Fuel (deterministic time) | 100 M units | `[holon.offers.X.wasm_limits.fuel]` |
| Epoch deadline | `timeout-ms` from request | (no override) |
| Table size | 1024 entries | `[holon.offers.X.wasm_limits.table_entries]` |
| Instances | 1 per call | (no override) |

Components exceeding limits trap and the host returns
`invoke-error::internal("<limit> exceeded")`.

---

## 8. WASI capability allowances

### 8.1 Default — pure functions

By default, components have **no WASI access**. No filesystem, no clocks
(beyond monotonic within the call), no network, no stdin. This models
the canonical "pure function" use case (generator-health formatter).

### 8.2 Opt-in

A component MAY request WASI capabilities via its manifest:

```toml
[holon.offers.my-cap.wasi]
clocks = true        # wasi:clocks
random = true        # wasi:random
stdio = false        # wasi:stdio (default false)
filesystem = []      # list of read-only mount points (host paths)
http = false         # wasi:http (future, WASI 0.3)
```

The host MUST honor the manifest and grant ONLY the declared capabilities.
Capabilities not listed MUST return `-1` / errno / `unknown-capability`
at runtime.

### 8.3 Security rationale

WASI's capability model means a component that declares `filesystem = []`
cannot read arbitrary files, even if bugs exist — the host never exposes
the handle. This is THSF's primary isolation story (vs `adapter = "cli"`,
which has full process ambient authority).

---

## 9. Testing & conformance

### 9.1 Conformance harness

Every `wasm` capability MUST pass:

1. **Load test**: component instantiates in < 20 ms.
2. **List test**: `list-capabilities()` returns the declared IDs.
3. **Invoke happy path**: calling each capability with a valid payload
   returns `Ok(invoke-response)` with `exit_code == 0`.
4. **Invoke error path**: calling with malformed args returns
   `Err(invoke-error::invalid-args(...))` — MUST NOT trap.
5. **Timeout test**: a capability that busy-loops past `timeout-ms` MUST
   be terminated by the host epoch deadline.
6. **Determinism test**: same input → same output (important for caching
   + audit replay).

The canonical harness lives at:
`~/.claude/rust/holon-wasm-components/runner/tests/`.

### 9.2 CI gate

CI for a `wasm` holon SHOULD include:

```yaml
- cargo build --release --target wasm32-wasip2
- wasm-tools validate target/wasm32-wasip2/release/<name>.wasm
- wasm-tools component new ...  # verify component form
- cargo test -p <host-harness>   # integration tests via wasmtime
- blake3sum <name>.wasm > <name>.wasm.blake3  # regenerate hash
```

---

## 10. Examples — four reference components

### 10.1 `spec-version`

Trivial proof-of-life. Exports one capability `spec-version` that
returns the WIT package version.

```rust
impl Guest for Component {
    fn list_capabilities() -> Vec<String> {
        vec!["spec-version".to_string()]
    }
    fn invoke(req: InvokeRequest) -> Result<InvokeResponse, InvokeError> {
        if req.capability != "spec-version" {
            return Err(InvokeError::UnknownCapability(req.capability));
        }
        let body = br#"{"spec_version":"0.1.0"}"#.to_vec();
        Ok(InvokeResponse {
            exit_code: 0,
            stdout: body,
            stderr: vec![],
            duration_ms: 0,
            logged: false,
        })
    }
}
```

### 10.2 `blast-radius`

Pure function: takes `{file_path, max_depth}`, returns direct + transitive
dependent lists. Deterministic for a fixed input snapshot.

### 10.3 `quality-gate`

Pure function: takes source bytes + language hint, returns quality signals
(unwrap count, antipattern count, health score ∈ [0, 1.2]).

### 10.4 `generator-health` (Fase 5 Wave H)

Pure function formatter: takes a health-delta snapshot JSON, returns
structured analysis (summary, alerts, clamped health_score). 157 KB WASM.

---

## 11. WIT package evolution

### 11.1 Package versions

The `holon:core` package is versioned independently of individual
components. Current: `0.1.0`. MAJOR bump rules:

- Change to existing record shape → MAJOR
- Add new required field → MAJOR
- Add new method to `capabilities` interface → MAJOR (bindings change)
- Add new world → MINOR (existing components unaffected)
- Add new interface alongside → MINOR

### 11.2 Add new canonical packages

THSF MAY add additional canonical WIT packages (e.g. `holon:streaming` for
streaming capabilities in v1.2). Each package has its own version track.

The `holon:generator@0.1.0` package (Fase 5) is the first such addition,
targeting real-time health-delta subscription over Cap'n Proto — but also
exposable via WIT-bindings in future WASI Preview 3.

---

## 12. Diagnostics

| Code | Meaning | Severity |
|---|---|---|
| `thsf-wasm-001` | Component file not found | error |
| `thsf-wasm-002` | Invalid component (wasm-tools validate failed) | error |
| `thsf-wasm-003` | Wrong world (component doesn't export `holon-component`) | error |
| `thsf-wasm-004` | `content_hash` mismatch — file tampered or stale | error |
| `thsf-wasm-005` | Capability listed but invoke returned `unknown-capability` | error |
| `thsf-wasm-006` | Component trapped instead of returning `invoke-error` | error |
| `thsf-wasm-007` | Resource limit exceeded (memory / fuel / epoch) | warning |
| `thsf-wasm-008` | WASI capability requested but not in manifest | error |
| `thsf-wasm-integrity` | Alias of `thsf-wasm-004` used in host logs | error |

---

## 13. Known limitations

### 13.1 No async in v1.0

WASI 0.2 lacks native async. Current canonical world is synchronous —
long-running capabilities MUST complete within `timeout-ms` or fail. Async
support depends on WASI 0.3 (expected late 2025 / early 2026) and will be
addressed in RFC-004a.

### 13.2 No streaming

Request/response envelopes are one-shot. Streaming capabilities (e.g.
real-time subscribe) MUST use `adapter = "capnp"` until WIT gains
streams (WASI Preview 3).

### 13.3 No shared memory

Components cannot share memory. All data crosses the boundary by copy.
For large payloads (>1 MiB), consider `adapter = "capnp"` which supports
zero-copy via shared `list<u8>` references.

---

## 14. Version history

| Version | Date | Summary |
|---|---|---|
| 1.0.0 | 2026-04-24 | Initial (Fase 8 D8.2.d) |

---

*End of RFC-004. See also: RFC-001 (manifest), RFC-002 (versioning),
RFC-003 (CRDT). For session notes on Fase 4 WASM delivery, see
`docs/2026-04-24-thsf-fase4-final.md`.*
