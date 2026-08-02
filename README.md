# Touring

> **The agentic code harness. Open, typed, auditable.**

[![version: 30.3.0](https://img.shields.io/badge/version-30.3.0-blue)]()
[![license: tiered](https://img.shields.io/badge/license-tiered-green)]()
[![rustc: 1.80+](https://img.shields.io/badge/rustc-1.80%2B-orange)]()
[![tier: free | standard | premium | enterprise](https://img.shields.io/badge/tier-4%20tiers-purple)]()

Touring is the **open-source, code-native, agent-first infrastructure** for
the next generation of code-generating agents. It is a Cargo workspace
(`crates/*`) of **42 crates**, wired into:

- **218 lifecycle hooks** (PreToolUse / PostToolUse / Session* / Task* / Hook* / CLI* / Neural* / RL*)
- **120 CLI commands** + **88 MCP tools** + 1 binary
- **5 RFCs** + **Constitution v8.0** as the master contract
- **License tier model**: `free → standard → premium → enterprise` (additive)
- **7 quality gates**: cargo check + clippy + test + e2e + cycles + orphans + TDG
- **9 P3-NO-OP audit patterns** closed in 2026-06; tree in harmony

## Quick start (5 minutes)

```bash
# 1. Install (Linux x86_64 / aarch64, macOS, Windows via WSL2)
curl -fsSL https://touring.dev/install.sh | sh

# 2. Verify
touring --version
touring doctor          # 6/6 OK expected
touring index rebuild   # one-time index build for current project

# 3. First query
touring ast overview src/main.rs
touring status -j       # composite health score visible
```

## What Touring is

| You want | Touring gives you |
|----------|-------------------|
| Code intelligence at scale | `touring ast`, `touring wiring`, `touring tantivy` (Tantivy v5 schema) |
| Agent harness for Claude Code | 218 hooks + CEG X0..X9 sandbox + 5 RFCs |
| Code generation | 36 kinds via `touring-generator` (VGP pipeline) |
| Polyglot (Rust/Python/TS/Go/...) | `touring-code::polyglot` (14 languages via tree-sitter) |
| Inference sandbox | `inferlets` (WASM, 11 runtimes) + Z3 SMT solver for proofs |
| RL/learned routing | `touring-intelligence::rl` (LinUCB + 8 arms + 25 dims) |
| Per-project deployment | `touring-foundation::bin::resource-monitor` (W12) |
| Commercial tiers | `touring-license` (Free / Standard / Premium / Enterprise) |

## What Touring is NOT

- **NOT an editor.** Touring is harness infrastructure; pair it with your editor.
- **NOT a single binary per language.** Touring is one Rust binary + 88 MCP tools + the Claude Code MCP server.
- **NOT a hosted service.** Touring is local-first; deploys per-project or system-wide.
- **NOT a code-generation product.** Touring is the *substrate* for code-generating products.

## Architecture (4 layers, 1 line of sight)

```
+================================================================+
|  L4 SURFACE  —  CLI · MCP · hooks · dashboards                 |
+================================================================+
                          ↓
+================================================================+
|  L3 ORCHESTRATION  —  workflows · agents · tasks · RL routing  |
+================================================================+
                          ↓
+================================================================+
|  L2 INTELLIGENCE  —  code · storage · reasoning · learning     |
+================================================================+
                          ↓
+================================================================+
|  L1 INFRASTRUCTURE  —  types · error · alloc · config · ids   |
+================================================================+
```

**Acyclic.** No back edges. 13 target productive crates (foundation, code,
storage, intelligence, bindings, hooks, hooks-shared, hooks-prediction,
server, server-reasoning, server-session, server-visual, orchestration)
+ 11 auxiliaries + 12 compat shims. See `plan.md` Section I for the
full architecture.

## Where to next

- 📘 **[Getting started](docs/landing/index.md)** — the 5-minute install + first query
- 🏛️ **[Constitution v8.0](docs/CONSTITUTION-v8.md)** — the master contract
- 📐 **[RFCs](docs/)** — 5 foundational RFCs (activity, PARCER, boundaries, identity, validation)
- 📊 **[Premium refactor plan](docs/plans/touring-premium-refactor-2026/00-INDEX.md)** — the 16-wave roadmap
- 🛡️ **[Security model](docs/explanation/architecture.md)** — CEG + Landlock + rlimit + cgroup
- 🍳 **[Cookbook](docs/how-to/)** — recipes for common tasks

## Stability + License

| Bucket | Stability | Tier | Examples |
|--------|-----------|------|----------|
| Core | 🔒 locked (3) | free | foundation, code, storage, intelligence, hooks, server |
| Internal | ✅ stable (2) | free | simd, rkyv, analysis, cortex, assists, offensive |
| Experimental | 🧪 experimental (1) | free | cognitive, learning, antt, ast, ast-polyglot |
| Compat shim | ✅ stable (2) | free | python, wasm, capnp-server, web, web-server |
| Auxiliary | ✅ stable (2) | free | loom-proofs, integration-tests |

**License tiers** (additive precedence via `touring-license`):

| Tier | Cost | Support | Capabilities |
|------|------|---------|--------------|
| **Free** | $0 | Community (best-effort) | All public APIs |
| **Standard** | $99/seat/yr | Discord + email (48h SLA) | + `jwt-verify`, + `tier-standard` features |
| **Premium** | $499/seat/yr | Private Slack + monthly review (24h SLA) | + `tier-premium` features (MCTS-gated, conformal routing) |
| **Enterprise** | Custom | Dedicated engineer + 99.9% SLA (4h, 24/7) | + `tier-enterprise` features (multi-tenant, on-prem) |

## Contributing

Read [CONSTITUTION-v8.md](docs/CONSTITUTION-v8.md) first. Then
[CONTRIBUTING.md](CONTRIBUTING.md). Every PR passes the
7-gate contract (cargo + clippy + test + e2e + cycles + orphans + TDG).

## Credits

- **Constitution v8.0** — TACO orchestrator + Gabriel Gadea (2026-05-09)
- **Touring 30.x** — the `touring-orchestration` working group + Gabriel Gadea
- **5 RFCs** — TACO working group (2026-04 to 2026-05)
- **12-audit suite** — the S9 RFC-100 verification (2026-05-09)
- **CAH roadmap closure** — 86.0% conformance, 35/37 CONFORME (2026-06-03)

---

_Touring 30.3.0 | daemon: healthy | workspace index: 2,147 files / 52,824 symbols (rebuilt 2026-06-06) | RL: LinUCB + 8 arms + 25 dims | hooks: 218 | e2e: 0.83 → target 0.90_
<!-- index/symbol counts are the per-workspace figures; refresh via `touring index rebuild $PWD`. Crate/LOC metrics: `docs/sync_metrics.py`. -->


_Generated 2026-06-04 by the TACO orchestrator (W1 of the upgrade plan)._
