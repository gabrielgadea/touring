---
name: graph-viz-wave-1-2
description: Wave 1 (Visual Foundation) + Wave 2 (Rich Encoding & Search) — Deliverables D1-D5, D13-D16, D43-D45
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

# Wave 1 + Wave 2 — Visual Foundation + Rich Encoding & Search

**Target**: v30.5.0 | **Data**: 2026-05-02

---

## WAVE 1 — Visual Foundation (D1 + D2)

### D1 — `touring graph <subcmd> --format dot|mermaid|json` ✅ PARCIAL (82%)

**Status**: Módulos visuais implementados (dot.rs, mermaid.rs, types.rs) mas CLI incompleto.

**Implementado**:
- `touring-server/src/visual/dot.rs` (132 LOC)
- `touring-server/src/visual/mermaid.rs` (92 LOC)
- `touring-server/src/visual/types.rs` (em mod.rs)
- `touring-server/src/visual/mod.rs` (403 LOC) — exports

**Falta**:
- [ ] `--format svg` pipe para `dot -Tsvg` (detectar dot no PATH)
- [ ] `--include-orphans` flag
- [ ] `--include-tests` flag
- [ ] Handler `cli_graph_file` estendido com GraphData enriquecido
- [ ] Handler `cli_graph_god_nodes` com encoding
- [ ] Handler `cli_graph_shortest_path` com encoding
- [ ] Handler `cli_graph_communities` com encoding

**Arquivos afetados**:
```
touring-server/src/visual/dot.rs       ✅ 132 LOC
touring-server/src/visual/mermaid.rs   ✅ 92 LOC
touring-server/src/cli/graph.rs        🟡 253 LOC (estender)
touring-hooks/src/cli_handlers.rs      🟡 4 handlers extension
```

**Testes**: 16 unit + 1 integration (roundtrip JSON→DOT→re-parse)

---

### D2 — `--max-nodes/--max-edges` + `--reduce` (auto-tred) ✅ COMPLETO (100%)

**Implementado**:
- `visual/cap.rs` (108 LOC) — BFS cap algorithm
- `visual/tred.rs` (148 LOC) — Aho/Garey/Ullman transitive reduction
- CLI flags em `graph.rs`

**Testes**: 6 unit (BFS cap, tred DAG, tred cycle warning, max_edges, ellipsis, --no-cap)

---

## WAVE 2 — Rich Encoding & Search Foundation (D3 + D4 + D5 + D13 + D14 + D15 + D16 + D43)

### D3 — `touring viz` top-level com encoding visual rico 🔴 PENDENTE (0%)

**Status**: Módulos `visual/encoding.rs` + `visual/theme.rs` + `visual/layout.rs` existem mas CLI `viz` não os consome.

**Implementado**:
- `visual/encoding.rs` (155 LOC) — node/edge styling
- `visual/theme.rs` (132 LOC) — TOML loader
- `visual/layout.rs` — layout auto-select
- `cli/viz.rs` (206 LOC) — dispatcher

**Falta**:
- [ ] `viz workspace` — clusters por crate, encoding por quality_score
- [ ] `viz blast <symbol>` — radial layout
- [ ] `viz wiring [--scope <crate>]` — integration_score color
- [ ] `viz cycles` — SCCs com circo
- [ ] `viz orphans` — órfãos classificados
- [ ] `viz feature <feature>` — gated symbols
- [ ] Theme TOML `~/.claude/touring/viz-theme.toml`
- [ ] Layout auto-select (dot/sfdp/circo/twopi)
- [ ] Tooltip embedding em DOT/SVG

**Testes**: 18 unit + 6 integration

---

### D4 — Reciprocal Rank Fusion (RRF) 🟡 PARCIAL (90%)

**Implementado**:
- `touring-search-fusion/src/hybrid/fusion.rs` (5.717 LOC) — RRF algorithm
- `touring-search-fusion/src/lib.rs` — module exports

**Falta**:
- [ ] CLI `touring search unified <query>` exposto
- [ ] CLI `touring search exact|fuzzy|bm25` exposto
- [ ] Output format with badges `[E]/[F]/[B]/[U]`

**Testes**: 8 unit (RRF arithmetic, backend fusion, tie-breaking, empty)

---

### D5 — Confidence tiers configuráveis 🔴 PENDENTE (0%)

**Falta**:
- [ ] Schema `~/.claude/touring/touring.toml` com `[blast]` + `[impact.depth]`
- [ ] `tier: "high"|"medium"|"low"` em blast/impact responses
- [ ] CLI `--tier` output formatter

**Testes**: 4 unit

---

### D13 — Intent classification + semantic weighting 🟡 PARCIAL (90%)

**Implementado**:
- `touring-search-fusion/src/intent.rs` — 6 QueryIntent types + heuristics
- Keyword-based detection (Understand/Debug/Implement/Refactor/Document/Explore)

**Falta**:
- [ ] CLI `--intent <type>` flag
- [ ] `--intent auto` (detect) default
- [ ] Boost factor configurable em `touring.toml [search.intent]`
- [ ] 20% boost wired AFTER hybrid+RRF+rerank

**Testes**: 12 unit

---

### D14 — GracefulChunker fallback chain ✅ COMPLETO (100%)

**Implementado**:
- `touring-core/src/chunker/graceful.rs` — `GracefulChunker<P, F>` trait
- `touring-core/src/chunker/error.rs` — typed errors (BinaryFileError, ParseError, etc.)
- `touring-core/src/chunker/mod.rs` — ChunkerSelector

**Testes**: 14 unit

---

### D15 — ResourceGovernor unificado 🟡 PARCIAL (80%)

**Implementado**:
- `touring-core/src/governor/` — ResourceGovernor struct
- `touring-server/src/cli/governor.rs` (596 LOC) — CLI commands

**Falta**:
- [ ] Integração com search (D4)
- [ ] Integração com chunker (D14)
- [ ] Integração com tantivy
- [ ] Memory pressure via `memory-stats` crate

**Testes**: 8 unit + 1 integration

---

### D16 — `touring init --profile` UX 🟡 PARCIAL (50%)

**Falta**:
- [ ] `~/.claude/touring/profiles/recommended.toml`
- [ ] `~/.claude/touring/profiles/quickstart.toml`
- [ ] `~/.claude/touring/profiles/airgapped.toml`
- [ ] `~/.claude/touring/profiles/ci.toml`
- [ ] CLI `touring init --profile <name> --list-profiles`

**Testes**: 5 unit

---

### D43 — PreToolUse Grep/Glob enrichment hook ✅ COMPLETO (100%)

**Implementado** (2026-05-01):
- `touring-hooks/src/pre_grep.rs` — PascalCase/snake_case/camelCase detection
- `touring-hooks/src/pre_glob.rs` — pattern enrichment
- P99 latency = **2ms** (25× margin vs spec 50ms)
- Counters `pre_grep_enrichment_count` + `pre_grep_zero_results_count` em gate-metrics
- Disable switch: `TOURING_DISABLE_PREGREP=1`

**Testes**: 39 tests E2E

---

### D45 — Bash(touring *) permission auto-add ✅ COMPLETO (100%)

**Implementado** (2026-05-01):
- 4 entries em `permissions.allow` em settings.json
- Idempotente (re-run não duplica)

---

## VALIDAÇÃO GATE WAVE 1+2

```bash
# Encoding
touring viz workspace --format svg --output /tmp/ws.svg
grep -q "fill=\"#a5d6a7\"" /tmp/ws.svg

# RRF + intent
touring search unified "how does authentication work" --format json | jq '.intent_detected'  # → "Understand"

# GracefulChunker fallback
echo -e "\xff\xfe\x00binary" > /tmp/binary
touring ast meta /tmp/binary --depth summary -j  # → BinaryFileError handled

# Profiles
touring init --profile quickstart --output /tmp/ws-test && [ -f /tmp/ws-test/touring.toml ]
```

---

## CRITICAL PATH DEPENDENCY

```
D1 (--format) ──┬─► D2 (--max-nodes/--reduce) ──► D3 (viz)
                │
                └─► D4 (RRF search) ──► D13 (intent boost) ──► D26 (find_code)
                                           │
                                           └─► D24 (hybrid scoring)
```