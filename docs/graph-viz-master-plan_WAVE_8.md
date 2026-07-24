---
name: graph-viz-wave-8
description: Wave 8 (Optional Investments) — Deliverables D10, D11, D12, D34, D35, D36, D39, D40, D41, D46, D48
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

# Wave 8 — Optional Investments (GATED BY DEMAND)

**Target**: TBD | **Data**: 2026-05-02

> **⚠️ ALL WAVE 8 DELIVERABLES ARE OPTIONAL**. Ship only if Gabriel validates demand.

---

## TIER A — Web + Visualization

### D10 — Web UI lite (Axum + dot.wasm) 🔴 POSTERGADO (XL)

**Scope**: Axum 0.8 + Svelte + WebGL + dot.wasm

**Endpoints**:
- `GET /` — landing
- `GET /graph?type=workspace|blast|wiring&symbol=X&format=svg` — server-side dot render
- `GET /api/v1/{find|impact|cycles}` — REST mirror
- `WebSocket /events` — health/quality streaming

**Risk**: HIGH — XL effort, scope creep altíssimo

**Pré-requisito**: Gabriel valida demanda antes de começar

---

### D11 — Edge Bundling (FDEB) 🔴 POSTERGADO (L)

**Scope**: Holten/van Wijk Force-Directed Edge Bundling em `visual/bundling.rs`

**Algorithm**:
- 20 control points per edge
- 6 iterações typical
- Compatibility: angular + scale + position + visibility

**Risk**: MEDIUM — non-trivial, hyperparams custosos

**Ship**: Wave 8.A isolada (~1 semana)

---

### D12 — gvpr-inspired Filter DSL 🔴 POSTERGADO (L)

**Scope**: DSL parser via chumsky + query → petgraph filter chain

**Operators**: WHERE, SELECT, GROUP BY, count/avg/max

**Risk**: MEDIUM — alto commitment, users podem preferir shell+jq

**Ship**: Wave 8.B isolada (~1 semana)

---

## TIER B — Backend Infrastructure

### D34 — Postgres backend opcional 🔴 POSTERGADO (M)

**Scope**: `PostgresBackend` além de `SqliteBackend`

**CLI**: `touring config storage=postgres --url <DSN>`

**Risk**: MEDIUM — schema migration + Docker em CI

---

### D35 — Cloudflare Workers Edge 🔴 POSTERGADO (XL)

**Scope**: touring-wasm targeting Cloudflare Workers + D1 backend

**Risk**: VERY HIGH — Edge debugging painful

---

## TIER C — Paradigm Shifts

### D36 — Bidirectional file ↔ graph sync engine 🔴 POSTERGADO (L)

**Scope**: `Projector` trait graph → file, edits-via-graph API

**Risk**: VERY HIGH — paradigm shift massivo

> **NOTA**: Touring é file-centric por design. Bidir sync é contra a filosofia.

---

### D39 — AI-Native Knowledge Layer (MVKL) 🔴 SPIKE ONLY (XL+)

**Scope**: Multi-Resolution Knowledge Layer (L0 file index, L1 parsed defs, L2 semantic graph)

**Risk**: VERY HIGH — multi-quarter investment

**Spike**: 1 semana para avaliar viabilidade. Se ROI negativo → documented and postponed indefinitely.

---

### D40 — Content-Addressed Definition Store (Unison) 🔴 WONTFIX

**Recomendação**: NÃO IMPLEMENTAR.

**Justification**:
- Paradigm shift muito grande para Touring file-centric design
- Thread escolheu Option C (Multi-Resolution) sobre Option B
- Use case overlap pequeno: Gabriel edita files via Claude Code, não via graph editing
- Investment XL com ROI incerto

---

### D41 — Code Graph Model integration (NeurIPS 2025) 🔴 SPIKE ONLY (XL)

**Scope**: 512x context compression via graph attention masking

**Spike outputs**:
- Feasibility report
- Cost estimate
- Alternative: SCIP export já planejado

---

## TIER D — Multi-User / Multi-Agent

### D46 — `.claude/plugins/touring/` plugin system 🔴 POSTERGADO (L)

**Scope**:
- Per-project custom commands/agents/rules
- `<workspace>/.claude/plugins/touring/.claude-plugin/plugin.json`

**Risk**: MEDIUM — scoped mas requires D30 (YAML rules) + D44 (commands)

---

### D48 — Multi-agent compatibility files 🔴 POSTERGADO (XS)

**Scope**: `.specify/.serena/.jules/.gemini` mirrors

**Risk**: LOW — apenas se Gabriel adopt outros agents

---

## SUMMARY TABLE

| ID | Deliverable | Size | Risk | Status |
|----|-------------|------|------|--------|
| D10 | Web UI | XL | HIGH | Postergado |
| D11 | FDEB | L | MEDIUM | Postergado (8.A) |
| D12 | Filter DSL | L | MEDIUM | Postergado (8.B) |
| D34 | Postgres backend | M | MEDIUM | Postergado |
| D35 | Cloudflare Workers | XL | VERY HIGH | Postergado |
| D36 | Bidir sync | L | VERY HIGH | Contra filosofia |
| D39 | MVKL | XL+ | VERY HIGH | Spike only |
| D40 | Unison-store | XL | VERY HIGH | WONTFIX |
| D41 | CGM integration | XL | RESEARCH | Spike only |
| D46 | Plugin system | L | MEDIUM | Postergado (gated) |
| D48 | Multi-agent files | XS | LOW | Postergado (gated) |

**TOTAL WAVE 8 LOC (se todos shippados)**: ~14.580 LOC — mas todos opcionais