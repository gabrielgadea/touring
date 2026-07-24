# THSF Conformance Suite v1.0

A drop-in test harness for verifying that any implementation of the
**Touring Holonic Symbiosis Framework** honors the normative contracts
defined in THSF-SPEC-v1.0 and RFC-001..004.

This suite ships **fixtures + runner + reference assertions** that are
implementation-agnostic — run them against the Python reference impl
(`holon.py`) or any future Rust/Go/Zig/etc. reimplementation.

---

## 1. What it proves

| Gate | Proves | RFC |
|---|---|---|
| Manifest validation — happy path | Parser accepts P0..P2 conformant manifests | RFC-001 §8 |
| Manifest validation — error codes | Parser emits the correct `thsf-manifest-NNN` diagnostic | RFC-001 §5.3 |
| CRDT merge — LWW tie-break | Two writers at same timestamp converge deterministically | RFC-003 §3.1 |
| CRDT merge — G-Set union | `a ∪ b = b ∪ a` (commutative) | RFC-003 §3.2 |
| CRDT merge — PN-Counter | `Σinc − Σdec` across actors | RFC-003 §3.3 |
| Grow-only invariant | No DELETE or UPDATE on G-Set or PN-Counter | RFC-003 §5.2 |
| Transport equivalence | Same capability invoked via `cli` vs. other adapters returns identical observable output | THSF-SPEC §9 I6 |

If every gate passes, the implementation is **Profile P3 conformant**
(see THSF-SPEC §8).

---

## 2. Layout

```
docs/thsf/conformance/
├── README.md                  ← this file
├── fixtures/                  ← 12 canonical manifest fixtures
│   ├── minimal-p0.toml
│   ├── p1-cli.toml
│   ├── p2-full.toml
│   ├── p1-wasm-hashed.toml
│   ├── missing-name.toml          → thsf-manifest-001
│   ├── bad-name-uppercase.toml    → thsf-manifest-003
│   ├── cli-no-cmd.toml            → thsf-manifest-004
│   ├── adapter-cmd-semicolon.toml → thsf-manifest-005
│   ├── path-traversal.toml        → thsf-manifest-006
│   ├── duplicate-names-a.toml     ↘ thsf-manifest-007
│   ├── duplicate-names-b.toml     ↗
│   └── unknown-top-level.toml     → thsf-manifest-008
├── tests/
│   └── test_conformance.py    ← language-neutral runner (harness only;
│                                 points to pluggable impls)
└── SPEC.md                    ← short ontology (mirrors THSF-SPEC §3)
```

---

## 3. Running against the reference implementation

```bash
cd /path/to/this/suite
# Uses the Python reference (~/.claude/tools/holon/holon.py by default):
python3 tests/test_conformance.py --impl=reference

# Or point at your own implementation (see §5 for the protocol):
python3 tests/test_conformance.py --impl=./my_impl_adapter.py
```

Exit codes:

| Code | Meaning |
|---|---|
| 0 | All gates passed |
| 1 | One or more gates failed (details on stderr) |
| 2 | Invocation error (bad `--impl`, missing fixtures) |

---

## 4. Running inside a CI pipeline

The fixtures are static TOML; the runner is a single Python 3.11+ file
with zero deps beyond stdlib. A minimal GitHub Actions job looks like:

```yaml
- uses: actions/setup-python@v5
  with: { python-version: "3.11" }
- run: python3 docs/thsf/conformance/tests/test_conformance.py
```

Any other CI system works identically — no package install required.

---

## 5. Adapter protocol for alternative implementations

To test a non-Python implementation, write a thin Python adapter that
exposes three callables:

```python
# my_impl_adapter.py
def parse_manifest(path: str) -> dict:
    """Return a JSON-serializable dict OR raise ManifestError.
    Exceptions MUST carry .code and .path attributes matching RFC-001.
    """

def crdt_open(db_path: str, actor_id: str):
    """Return a store object with .lww_set/.lww_get/.gset_add/.gset_members/
    .pn_increment/.pn_decrement/.pn_value methods matching RFC-003 API."""

def handshake_check(offerer_cap, requirer_req) -> bool:
    """Return True iff the offer satisfies the requirement per RFC-002 §4."""
```

The runner imports your adapter dynamically via `--impl=<path>` and
runs the same assertion suite against it. No other changes required.

---

## 6. Version compatibility

| Suite version | Matches THSF-SPEC | Matches RFC-001 | Matches RFC-002 | Matches RFC-003 | Matches RFC-004 |
|---|---|---|---|---|---|
| v1.0.0 | ≥ 1.0.0 | ≥ 1.0.0 | ≥ 1.0.0 | ≥ 1.0.0 | ≥ 1.0.0 |

The suite itself follows semver: a MAJOR bump only happens when a
normative change to any of the above RFCs makes prior fixture
expectations invalid. MINOR bumps add new gates; PATCH bumps fix
fixture or doc bugs without changing assertions.

---

## 7. Extending the suite

To add a new gate:

1. Drop a fixture (if needed) under `fixtures/`.
2. Add a test function to `tests/test_conformance.py` under the
   appropriate layer (manifest / crdt / transport).
3. Update the RFC that the new gate enforces with a cross-reference.
4. Bump this README's §6 table if the gate requires a new RFC minimum.

The adapter protocol (§5) MUST stay stable across MINOR bumps so
existing reimplementations continue to pass.

---

## 8. Attribution

Canonical fixtures + reference assertions originate from the
**Touring Holonic Symbiosis Framework** reference implementation at
`~/.claude/tools/holon/tests/`. They are redistributed here under the
same open-source terms for any downstream THSF implementation to
validate its conformance.
