---
name: graph-viz-dependencies
description: Dependency graph completo dos 49 deliverables do graph-viz-master-plan
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

# Dependency Graph — 49 Deliverables

**Verificação**: DAG (acyclic). Topological sort aplicado.

---

## TIER 0 — Independent (leaf dependencies)

```
D1  (graph --format)           — visual foundation
D5  (confidence tiers)          — independent, parallel to all
D4  (RRF search)                — independent base
D14 (GracefulChunker)            — independent base
D15 (ResourceGovernor)           — independent base
D16 (init --profile)            — independent UX
D17 (move detection)             — independent prep W5
D20 (rignore audit)             — independent
D21 (node types KB)             — independent
D28 (MCP overhead)              — independent
D32 (tier language UX)           — independent UX
D34 (Postgres backend)           — independent feature flag
```

---

## TIER 1 — Depends on Wave 1-2

```
D2  (--max-nodes/--reduce)       ← D1
D3  (viz encoding)               ← D1 + D2
D6  (graph flow A→B)             ← D1
D8  (snapshot)                   ← D1
D9  (clone detection)             ← D1
D13 (intent classification)       ← D4 + D5
D18 (checkpoint fingerprint)      ← D1
D29 (TouringFlowBuilder)          ← D1 + D8 + D9 + D31
D31 (semantic classification)     ← D21
D38 (cross-lang perf benchmarks)  ← D32
```

---

## TIER 2 — Depends on Waves 1-4

```
D7  (rename --plan)              ← D1 + D5 + D17
D19 (FailoverService)            ← D18
D22 (embeddings provider)         ← D18
D33 (conflict tier SLAs)         ← D19 + D37 + D38
D37 (Overlay Graph)               ← D8
```

---

## TIER 3 — Depends on Wave 4 stack

```
D23 (vector store)                ← D22
D24 (hybrid scoring)              ← D22 + D23
D25 (asymmetric embeddings)      ← D24 + D18
D26 (find_code super-tool)        ← D24
```

---

## TIER 4 — Depends on Wave 5 stack

```
D27 (Plugin DI)                   ← D22 + D23 + D24
D30 (YAML rule engine)           ← D29 + assists
```

---

## TIER 5 — Optional / Gated

```
D10 (Web UI)                     ← D1 + D2 + D3
D11 (FDEB bundling)               ← D1 + D3
D12 (Filter DSL)                 ← D1 + D3 + D21
D35 (Cloudflare Workers)         ← D34
D36 (Bidir sync)                 ← D37
D39 (MVKL)                       ← D31 + D37
D40 (Unison-store)               — WONTFIX
D41 (CGM)                        — spike only
D46 (plugin system)              ← D30 + D44
D48 (multi-agent)               — independent (gated)
```

---

## CRITICAL PATH (longest dependency chain)

```
CORE PATH (8 hops):
D1 → D4 → D22 → D23 → D24 → D25 → D26 → D27

ESTIMATED: 4-6 weeks core implementation
```

---

## CC INTEGRATION DEPENDENCY CHAIN (D42-D49)

```
D42 (cc-setup installer)
  ├─ uses D16 (profiles)
  ├─ includes D43 hook scripts (embedded)
  └─ auto-adds D45 (permissions)

D44 (Speckit commands) → includes D49 (handoffs)
D47 (multi-project registry) → consumed by D26 (find_code --project)
```

---

## VERIFICATION: TOPOLOGICAL SORT

```
Tier 0: [D1, D5, D4, D14, D15, D16, D17, D20, D21, D28, D32, D34]
Tier 1: [D2, D3, D6, D8, D9, D13, D18, D29, D31, D38]
Tier 2: [D7, D19, D22, D33, D37]
Tier 3: [D23, D24, D25, D26]
Tier 4: [D27, D30]
Tier 5: [D10, D11, D12, D35, D36, D39, D40, D41, D46, D48]
```

**ACYCLIC**: ✅ Verified — no circular dependencies