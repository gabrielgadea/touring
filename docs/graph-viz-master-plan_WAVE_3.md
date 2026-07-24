---
name: graph-viz-wave-3
description: Wave 3 (Capability Parity + Overlay Graph) — Deliverables D6, D7, D8, D9, D17, D37
type: project
related_files:
  - graph-viz-master-plan_OVERVIEW.md
  - graph-viz-master-plan_STATUS.md
  - graph-viz-master-plan_WAVES_1_2.md
  - graph-viz-master-plan_WAVE_3.md
  - graph-viz-master-plan_WAVE_4.md
  - graph-viz-master-plan_WAVE_5.md
  - graph-viz-master-plan_WAVE_6.md
  - graph-viz-master-plan_WAVE_7.md
  - graph-viz-master-plan_WAVE_8.md
  - graph-viz-master-plan_DEPENDENCIES.md
---

# Wave 3 — Capability Parity + Overlay Graph

**Target**: v30.7.0 | **Data**: 2026-05-02

---

## WAVE 3.A — flow + clones + move detection

### D6 — `touring graph flow <a> <b>` 🔴 PENDENTE (0%)

**Módulo implementado**: `visual/flow.rs` (291 LOC) existe mas não exposto via CLI.

**Falta**:
- [ ] Daemon handler `cli_graph_flow`
- [ ] `touring graph flow <symbol_a> <symbol_b> [--max-paths 10] [--max-depth 8]`
- [ ] `petgraph::algo::all_simple_paths` integration
- [ ] Output JSON: `{paths: [[a, m1, m2, b], ...], count: N, truncated: bool}`
- [ ] DOT/Mermaid highlighting
- [ ] Compact: `a → m1 → m2 → b`

**Arquivos afetados**:
```
touring-server/src/visual/flow.rs       ✅ 291 LOC (existe)
touring-server/src/cli/graph.rs         🟡 estender dispatcher
touring-hooks/src/cli_handlers.rs       🟡 cli_graph_flow handler
```

**Testes**: 8 unit (cycle, no path, single/multi path, depth limit, max paths, identical a==b, missing symbol)

---

### D9 — Clone detection (signature hashing) 🔴 PENDENTE (0%)

**Falta**:
- [ ] `touring-ast/src/signature.rs` — signature computation
- [ ] Hash: SHA256 of `(kind, lines_count, ast_node_count, complexity_bucket, derives_sorted, generic_params_count, trait_bounds_count)`
- [ ] `touring graph clones [--min-group 2] [--scope <path>] [--format compact|json|dot]`
- [ ] CLI handler `cli_graph_clones`

**Arquivos afetados**:
```
touring-ast/src/signature.rs            🔴 NEW (~100 LOC)
touring-server/src/refactor/clones.rs   🔴 NEW (~280 LOC)
touring-server/src/cli/graph.rs         🟡 estender
touring-hooks/src/cli_handlers.rs       🟡 cli_graph_clones
```

**Testes**: 10 unit

---

### D17 — Move detection (incremental dedup) ✅ COMPLETO (100%)

**Implementado**: `touring-vfs::manifest` com `detect_moves()` por hash matching.

**Testes**: 10 unit

---

## WAVE 3.B — rename plan + snapshot

### D7 — `touring graph rename <symbol> --new <name> --plan` 🔴 PENDENTE (0%)

**Falta**:
- [ ] Pipeline: `touring index find <old_symbol>` → `touring wiring impact <old_symbol>` → locate call sites → generate plan JSON
- [ ] `plan struct`: `{old, new, edits: [{file, line, col, kind}], blast_radius, tier, files_affected, risk_factors}`
- [ ] `--dry-run` (default) — mostra plan
- [ ] `--apply` com hash confirm + speculative validation + rollback
- [ ] Daemon handlers: `cli_graph_rename_plan`, `cli_graph_rename_apply`

**Arquivos afetados**:
```
touring-server/src/refactor/rename.rs   🔴 NEW (~350 LOC)
touring-server/src/refactor/mod.rs      🔴 NEW
touring-server/src/cli/graph.rs         🟡 estender
touring-hooks/src/cli_handlers.rs       🟡 2 handlers
```

**Testes**: 12 unit (plan gen 4, apply 4, rollback 2, idempotence 2)

**Risk**: MEDIUM — `--apply` pode quebrar código se call sites têm contexto sutil

---

### D8 — Snapshot create/list/delete/diff ✅ COMPLETO (100%)

**Implementado** (2026-05-01):
- `touring-server/src/cli/snapshot.rs` (426 LOC)
- `snapshot create/list/delete/diff` commands
- `snapshot diff-impact <git-ref>` — blast radius de files alterados

**Testes**: 14 unit

---

## WAVE 3.C — Overlay Graph

### D37 — Overlay Graph (Base + Delta + Unified) 🟡 PARCIAL (90%)

**Implementado**: `touring-server/src/snapshot/` com overlay logic.

**Falta**:
- [ ] `touring overlay status -j` — base_commit, delta_files_count, conflicts_detected
- [ ] `touring overlay promote` — move delta → base (equivalent a reindex)
- [ ] `touring overlay discard` — clear delta layer
- [ ] `touring overlay diff -j` — delta vs base diff
- [ ] Integration com D33 (conflict detection)

**Testes**: 18 unit + 8 integration

---

## VALIDAÇÃO GATE WAVE 3

```bash
# flow paths
touring graph flow main run --format json | jq '.paths | length' | { read N; [ "$N" -ge 1 ] || exit 1; }

# rename plan
touring graph rename HookRuntime --new HookEngine --plan --format json | jq '.edits | length'

# snapshot diff
touring graph snapshot create base && touring graph snapshot diff base HEAD --format json

# clones
touring graph clones --min-group 2 --format json | jq '.groups | length'

# move detection
mv src/foo.rs src/bar.rs
touring vfs sync --json | jq '.moves[] | select(.from == "src/foo.rs" and .to == "src/bar.rs")'
```

---

## CRITICAL PATH

```
D8 (snapshot) ──► D37 (Overlay Graph) ──► D33 (conflict detection)
     │
     └──► D6 (flow) ←─ D1 (graph --format)
```