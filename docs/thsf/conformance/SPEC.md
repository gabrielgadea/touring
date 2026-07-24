# THSF Conformance — Short SPEC

This is the minimum set of concepts a conformant THSF implementation
must honor. Full normative text is in `../THSF-SPEC-v1.0.md` and
`../rfcs/RFC-00{1..4}.md`; this document is the abridged contract for
implementers building the conformance runner adapter (§5 of README).

---

## Concept map

```
holarchy
├── holon (has a .holon/manifest.toml)
│   ├── identity { name, version, autonomy_guarantee, … }
│   ├── offers[name] → capability
│   │   ├── adapter ∈ { "cli", "capnp", "wasm" }
│   │   ├── adapter_cmd | capnp_socket | wasm_component   (exactly one)
│   │   ├── schema (optional — path to JSON Schema / WIT)
│   │   └── version (semver)
│   └── requires[name] → {optional, fallback, min_version}
└── CRDTStore (LWW, G-Set, PN-Counter) — per-actor, merge-safe
```

## Four layers (mirror of THSF-SPEC §3)

1. **Discovery** — filesystem glob for `.holon/manifest.toml`
2. **Handshake** — compute offer ⇄ require matches by capability name + semver
3. **Capability exchange** — invoke via chosen adapter (`cli`/`capnp`/`wasm`)
4. **Knowledge sync** — CRDT state stores (SQLite in reference impl)

## Invariants (see THSF-SPEC §9)

- Autonomy: holon builds & runs without `.holon/`
- Reversibility: `rm -rf */.holon/` is a safe reset
- No framework imports at runtime
- Idempotent operations across the stack
- Monotonic state (grow-only semantics on G-Set and PN-Counter)
- Transport equivalence: same capability returns same observable output
  regardless of adapter

## Adapter protocol (minimum surface)

Every conformance-runner adapter exposes:

| Callable | Contract |
|---|---|
| `parse_manifest(path) -> dict` | Parse manifest; raise error-with-`.code` on failure |
| `crdt_open(db_path, actor_id)` | Return store with `lww_*`, `gset_*`, `pn_*` methods |
| `handshake_check(offer, require) -> bool` | Optional; required for §handshake gates |
| `supports_grow_only_triggers: bool` | Adapter attribute; opts into grow-only gate |

Diagnostic codes follow `thsf-manifest-NNN` (RFC-001 §5.3),
`thsf-cap-NNN` (RFC-002 §9), `thsf-crdt-NNN` (RFC-003 §11),
`thsf-wasm-NNN` (RFC-004 §12). Codes are stable contracts — renaming a
code is a MAJOR breaking change.
