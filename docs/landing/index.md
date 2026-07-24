# Touring Docs — Home

> **The agentic code harness.** Open, typed, auditable.

Welcome to the Touring documentation site. Touring is a Rust workspace of
36 crates (~428k LOC) that provides the **substrate for code-generating
agents**: code intelligence, lifecycle hooks, RL/learned routing, code
generation, polyglot AST, and a 7-gate quality contract.

## 🏁 New here? Start in 5 minutes

1. **Install** — `curl -fsSL https://touring.dev/install.sh | sh`
2. **Verify** — `touring doctor` (expect 6/6 OK)
3. **Index your project** — `touring index rebuild`
4. **First query** — `touring ast overview src/main.rs`
5. **Watch the metrics** — `touring status -j`

## 📚 Documentation by kind (Diátaxis)

| Kind | Path | Purpose |
|------|------|---------|
| **Tutorials** (learning) | [`tutorials/`](tutorials/) | Step-by-step narratives; first 5 minutes |
| **How-to guides** (tasks) | [`how-to/`](how-to/) | Recipes for specific problems |
| **Reference** (info) | [`reference/`](reference/) | API / CLI / config / hooks / MCP reference |
| **Explanation** (understanding) | [`explanation/`](explanation/) | Architecture / RFCs / ADRs / design notes |

## 🏛️ The contract (read first)

- **[CONSTITUTION-v8.md](../CONSTITUTION-v8.md)** — the master document
- **[5 foundational RFCs](../)** — Activity, PARCER, Boundaries, Entity Identity, Validation

## 🏗️ Architecture at a glance

Touring is a **4-layer onion** (L1 infrastructure → L2 intelligence →
L3 orchestration → L4 surface). The architecture is **acyclic** and the
substrate is **connected** (zero cycles, zero new orphans in 2026-06 audit).

```
+----------------------------------------------------------------+
|  L4 SURFACE  —  CLI · MCP · hooks · dashboards                 |
+----------------------------------------------------------------+
                              ↓
+----------------------------------------------------------------+
|  L3 ORCHESTRATION  —  workflows · agents · tasks · RL routing |
+----------------------------------------------------------------+
                              ↓
+----------------------------------------------------------------+
|  L2 INTELLIGENCE  —  code · storage · reasoning · learning    |
+----------------------------------------------------------------+
                              ↓
+----------------------------------------------------------------+
|  L1 INFRASTRUCTURE  —  types · error · alloc · config · ids    |
+----------------------------------------------------------------+
```

## 🎯 What you can do with Touring

| If you are a... | Use Touring for... |
|-----------------|---------------------|
| **Agent developer** | 198 lifecycle hooks + CEG X0..X9 sandbox + 5 RFCs |
| **Library author** | Code intelligence (AST + wiring + Tantivy) + 36 codegen kinds |
| **Platform team** | Per-project deployment + 4 license tiers + resource monitoring |
| **Researcher** | Z3 SMT solver + RL bandit + transcript miner for agent traces |
| **Commercial user** | Premium tier with private Slack + 24h SLA + dedicated engineer |

## 📊 Current state (2026-06-04)

| Metric | Value |
|--------|------:|
| Crates | 36 (13 target productive + 11 aux + 12 compat shims) |
| LOC | ~428k |
| Hooks | 198 |
| CLI commands | 120+ |
| MCP tools | 88 |
| License tiers | 4 (free / standard / premium / enterprise) |
| Tests | 4,008 / 4,009 PASS |
| E2E composite | 0.83 (target 0.90) |
| Cycles (Tarjan SCC) | 9 (target 0 — W3 of upgrade plan) |
| CAH conformance | 86.0% (35/37 CONFORME — yesterday's closure) |

## 🛣️ Roadmap

The 47→13 residual plan (this directory's parent) was upgraded 2026-06-03
to elevate Touring to a **Premium Elite Market product** for the
agentic-code infrastructure category. The 8-wave upgrade roadmap (W1-W8)
runs in parallel with the `touring-premium-refactor-2026` master plan (W0-W15).

| Wave | Scope | Effort |
|------|-------|-------:|
| W1 | Foundational README + Brand Layer | 1-2 ed |
| W2 | Module Boundary Audit | 2-4 ed |
| W3 | Cycle Elimination (9→0) | 4-8 ed |
| W4 | Orphan Convergence (6,367 → ≤2,000) | 4-8 ed |
| W5 | Test Coverage Push (≥90% in Core) | 4-8 ed |
| W6 | Doc Coverage Tooling | 2-4 ed |
| W7 | Cookbook Expansion (13 recipes) | 2-4 ed |
| W8 | Whitepaper + Commercial Positioning | 2-4 ed |

## 🤝 Get involved

- **Issues + Discussions** — GitHub (public)
- **Discord** — community support
- **Slack (Premium)** — private support channel
- **Email (Enterprise)** — dedicated engineer

## 📜 License + tier

Touring is open source with a 4-tier commercial model. The license is
**additive**: `tier-enterprise` ⊇ `tier-premium` ⊇ `tier-standard` ⊇ `tier-free`.
See [`touring-license`](../touring-license.md) for the full schema.

---

_Crafted 2026-06-04 by the TACO orchestrator (W1.2 of the upgrade plan)._
