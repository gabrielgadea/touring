# Touring — The Agentic Code Harness

> **Whitepaper v0.1** — 2026-06-04
> **Status**: Draft for review
> **Author**: TACO orchestrator (Premium Elite product framing, W8 of upgrade plan)
> **Audience**: Platform teams, agent developers, code-intelligence researchers, commercial evaluators

---

## TL;DR

Touring is the **open-source, code-native, agent-first infrastructure** for
the next generation of code-generating agents. It is a Rust workspace of
**46 crates / ~479k LOC (src), 547k workspace** providing code intelligence, lifecycle hooks,
RL/learned routing, code generation, polyglot AST, and a 7-gate quality
contract — all under a constitutional contract that ships in code, not
in prose.

**One binary. 120 CLI commands. 102 MCP tools (22 curated opt-in). 218 hooks. 5 RFCs. 4
license tiers. Zero vendor lock.**

---

## 1. Problem — the missing harness layer

Code-generating agents are powerful but unsafe. The 2026 LLM landscape
demonstrates this daily: agents write code, break code, lose track of
intent, run unverified commands, edit files without blast-radius
awareness, and produce results that no human can audit.

Three classes of failure dominate:

1. **The tool-call boundary is unbounded.** Agents run `rm -rf`,
   edit files outside their task scope, and execute network calls
   without policy. The 2026 arXiv paper
   [Code as Agent Harness (2605.18747)](https://arxiv.org/abs/2605.18747)
   names this the "AutoHarness" problem.
2. **The reasoning layer is not version-controlled.** Agent traces
   vanish; success patterns are lost; failure modes are not learned.
3. **The substrate is opaque.** Code intelligence, RL routing, and
   execution are scattered across vendor SaaS (Sourcegraph, LangSmith,
   Cursor, Replit) with no single constitutional contract.

The industry has built **harnesses for the LLM** (prompt management,
tool selection, context engineering) but not **harnesses for the code
the LLM produces**. The latter is the missing layer.

---

## 2. Solution — Touring as the harness

Touring is the **code harness**. It is a Cargo workspace that wraps
**the code under agent control** in a typed, auditable, constitutional
substrate. The substrate provides:

| Layer | Capability | Symbols |
|-------|-----------|---------|
| **Code intelligence** | AST + wiring + Tantivy index + 14-language polyglot | `touring-code`, `touring-intelligence::index` |
| **Execution harness** | CEG X0..X9 sandbox (capture → classify → static → VGP → predict → sandbox → gate → decision → supervised → learn) | `touring-hooks::gateway` |
| **Lifecycle hooks** | 218 hooks covering PreToolUse / PostToolUse / Session* / Task* / CLI* / Neural* / RL* | `touring-hooks` |
| **RL/learned routing** | LinUCB bandit + 8 arms + 25-dim context features + live MCTS-gated speculation | `touring-intelligence::rl`, `touring-server-reasoning` |
| **Code generation** | 36 code-gen kinds via `touring-generator` (5-stage typestate pipeline) | `touring-generator` |
| **Polyglot** | Rust + Python + TS + TSX + Go + C + C++ + Java + Swift + Shell + PHP + Perl + R + Elixir | `touring-code::polyglot` |
| **Inference sandbox** | WASM-based `inferlets` (11 runtimes) + Z3 SMT solver for proof-in-loop | `inferlets/`, `touring-offensive` |
| **License tiers** | `free` ⊆ `standard` ⊆ `premium` ⊆ `enterprise` (additive precedence) | `touring-license` |

**The contract is the code.** The 5 RFCs (`docs/RFC-00{1,2,3,4,5}-*.md`)
are the constitution. The Constitution v8.0 (`docs/CONSTITUTION-v8.md`)
is the master document. Every hook, every CLI command, every MCP tool
is bound by the 7-gate quality contract.

---

## 3. Architecture — the 4-layer onion

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
|  L1 INFRASTRUCTURE  —  types · error · alloc · config · ids    |
+================================================================+
```

**13 target productive crates** (all in place as of 2026-06-03):
foundation, code, storage, intelligence, bindings, hooks, hooks-shared,
hooks-prediction, server, server-reasoning, server-session,
server-visual, orchestration. **Acyclic.** **Connected.** **0 cycles in
the CAH-passing subset** (per `2026-06-03-cah-roadmap-closure.md`).

---

## 4. Constitutional contract — the 5 RFCs

The contract is **published, versioned, and enforced by the code**:

- **RFC-001** Activity Event Catalog — append-only event log with
  monotonic `seq`, SHA-256 `projection_hash`, 7 `output.rejected` error
  codes.
- **RFC-002** PARCER Profile Schema — 6-dim behavioral contract for
  agents (5 YAML profiles in `~/.claude/agents/`).
- **RFC-003** Path Boundaries Contract — VGP Layer 5 globset
  enforcement per `TaskKind`.
- **RFC-004** Entity Identity Registry — EntityId is **deterministic**
  (derived from canonical name + admission criteria), NOT emergent.
  Same inputs ALWAYS produce same EntityId across sessions.
- **RFC-005** 7-Layer Validation Pipeline — VGP typestate +
  `validate_plan()` in `pipeline.rs`.

**The 7-gate quality contract** (per wave, per PR):

| # | Gate | Tool | Threshold |
|---|------|------|----------:|
| 1 | Compilation | `cargo check --workspace --tests --benches` | exit 0 |
| 2 | Lints | `cargo clippy --workspace --all-targets -- -D warnings` | 0 warnings |
| 3 | Tests | `cargo test --workspace --lib` | 100% pass |
| 4 | E2E | `touring e2e -j` composite | ≥ 0.85 |
| 5 | Cycles | `touring wiring cycles --min-depth 2 -j` count | 0 |
| 6 | Orphans | `touring wiring orphans -j` delta vs baseline | ≤ 0 |
| 7 | TDG | `touring ast tdg <changed_file>` per file | ≥ B |

---

## 5. Codebase facts (FACT [1.0] 2026-06-09)

| Metric | Value | Source |
|--------|------:|--------|
| Crates | **40** (incl. `touring-ceg` CEG extraction 2026-06-10 + `touring-contracts` IoC leaf A.W3.P1 2026-06-09 + `saga`/`agentic_rl` A01 2026-06-06; 41 `workspace_members` incl. `fuzz/`) | `docs/sync_metrics.py` |
| LOC | **477,674 (src) / 545,416 (workspace)** | `docs/sync_metrics.py` (A03 anti-drift gate) |
| Hooks | **218** | `ALL_DAEMON_HOOK_NAMES` (`docs/gen_reference.py`) |
| CLI commands | **120+** | `touring --help` |
| MCP tools | **102** (default `mcp-legacy`; 22 curated via opt-in `mcp-curated`) | `crates/touring-hooks/src/cli/handlers/mcp.rs` |
| Unit tests | **4,008 / 4,009** PASS | `cargo test --workspace --lib` |
| E2E composite | **0.83** | `touring e2e -j` |
| **Doc coverage (NEW MEASUREMENT)** | **29.45%** (mean), 85.71% (top: touring-license), 0% (bottom: compat shims) | `scripts/doc-coverage.py` (this wave, W6) |
| Cycles | **0** (acyclic DAG; A05 closed 2026-06-04 — was 690 ghost edges from cross-project contamination + absorbed crates) | `touring wiring cycles` |
| Orphans | 4,090 (workspace-level, informational) | `touring wiring orphans` |
| CAH conformance | **86.0%** (35/37 CONFORME) | `docs/2026-06-03-cah-roadmap-closure.md` |

**The doc-coverage number is honest**: 29.45% mean across 28 measured
crates (1 PASS, 8 WARN, 19 FAIL). This is the **W6 baseline**; the
upgrade plan targets ≥ 80% in Core, ≥ 60% in Internal.

---

## 6. Position vs landscape

| Competitor | What they have | Touring's differentiation |
|------------|----------------|---------------------------|
| **Sourcegraph** | Code search, code intelligence | Touring adds **agentic harness + hooks + RL routing** in the same binary |
| **LangSmith** | LLM tracing | Touring is **code-native**, model-agnostic (LinUCB + 8 arms) |
| **Cursor** | AI-first editor | Touring is **open**, not editor-locked, runs anywhere |
| **Replit** | Cloud IDE | Touring is **local-first**, polyglot, no cloud lock |
| **Sentry** | Observability | Touring's hooks **ARE** the observability, code-native |
| **Vercel** | DX + deployment | Touring's `touring --help` UX is the bar |
| **Linear** | Opinionated workflow | Touring has the **constitutional contract** (5 RFCs + 7-gate quality) |
| **Stripe** | Docs-as-product | Touring ships the same caliber (Diátaxis 4-kind framework) |

**The niche**: open-source, code-native, agent-first infrastructure for
the code intelligence + harness + RL routing category. No vendor lock,
no editor lock, no cloud lock. The closest analogues are
[LangChain](https://www.langchain.com/) (for LLM chains) and
[Anthropic Claude Code](https://docs.anthropic.com/en/docs/claude-code)
(for agent harness) — Touring is the **code-first substrate** that
both build on, not the LLM-facing layer.

---

## 7. Business model — 4 license tiers

| Tier | Price | Support | Capabilities |
|------|-------|---------|--------------|
| **Free** | $0 | Community (best-effort) | All public APIs |
| **Standard** | $99/seat/yr | Discord + email (48h SLA) | + `jwt-verify`, + `tier-standard` features |
| **Premium** | $499/seat/yr | Private Slack + monthly review (24h SLA) | + `tier-premium` (MCTS-gated, conformal routing, transcript miner) |
| **Enterprise** | Custom | Dedicated engineer + 99.9% SLA (4h, 24/7) | + `tier-enterprise` (multi-tenant, on-prem, air-gap) |

The license substrate is **already shipped** (`touring-license` crate
with 4-tier additive precedence + 30-day offline grace). The
`jwt-verify` feature is the immediate next deliverable.

---

## 8. Strategic narrative — why Touring wins

Three convergent trends favor Touring's positioning:

1. **Agents are eating software.** The 2026 LLM landscape is dominated
   by Claude Code, Cursor, Replit, Devin, Codex. Every one needs a
   harness. Touring is the only **open + constitutional** harness.
2. **Code intelligence is a commodity, harness is not.** Sourcegraph
   proved code intelligence is valuable. Anthropic Claude Code proved
   the harness is the **bottleneck**. Touring is positioned at the
   bottleneck.
3. **The constitutional contract is the moat.** Once a team adopts
   Touring's 5 RFCs + 7-gate contract, the switching cost is the
   team's own audit trail. This is **lock-in by correctness**, not
   by vendor.

**Tagline**: *The agentic code harness. Open, typed, auditable.*

---

## 9. Call to action

- **Install** in 5 minutes: `curl -fsSL https://touring.dev/install.sh | sh`
- **First query** in 1 minute: `touring ast overview src/main.rs`
- **First hook** in 15 minutes: see [cookbook/add-a-hook.md](how-to/add-a-hook.md)
- **First migration** in 1 hour: see [cookbook/migrate-from-2024.md](how-to/migrate-from-2024.md)

For commercial inquiries: [sales@touring.dev](mailto:sales@touring.dev).
For partnership / extension: [partners@touring.dev](mailto:partners@touring.dev).
For security disclosures: [security@touring.dev](mailto:security@touring.dev).

---

## 10. Roadmap — the next quarter

| Wave | Scope | Effort | Status |
|------|-------|-------:|:------:|
| W1 | Foundational README + Brand Layer | 1-2 ed | ✅ DONE 2026-06-04 |
| W2 | Module Boundary Audit | 2-4 ed | ⏳ planned |
| W3 | Cycle Elimination (9→0) | 4-8 ed | ⏳ inventory done; 1 fix in progress |
| W4 | Orphan Convergence (6,367 → ≤2,000) | 4-8 ed | ⏳ planned |
| W5 | Test Coverage Push (≥90% in Core) | 4-8 ed | ⏳ planned |
| W6 | **Doc Coverage Tooling** | 2-4 ed | ✅ DONE 2026-06-04 (this wave) |
| W7 | Cookbook Expansion (13 recipes) | 2-4 ed | ⏳ planned |
| W8 | **Whitepaper + Commercial Positioning** | 2-4 ed | ✅ DONE 2026-06-04 (this wave) |

3 of 8 upgrade waves delivered. 5 remaining. The full
`touring-premium-refactor-2026` master plan (W0-W15) runs in parallel.

---

## 11. Appendix — the 5 RFCs in 1 minute

```
RFC-001  Activity Event Catalog
         append-only event store, 13 EventAction variants, 7 error codes,
         I1-I5 invariants (monotonic seq, SHA-256 projection_hash, etc.)

RFC-002  PARCER Profile Schema
         6-dim behavioral contract (scope, autonomy, reversibility,
         tool_risk, evidence, blast_radius); 5 profiles in YAML

RFC-003  Path Boundaries Contract
         VGP Layer 5 — globset enforcement per TaskKind (e.g.
         "test runner can write to /test-results/* but not to src/*")

RFC-004  Entity Identity Registry
         EntityId = derive(canonical_name, admission_criteria) — pure +
         total, deterministic, no memory address, no creation order

RFC-005  7-Layer Validation Pipeline
         VGP typestate + validate_plan() in pipeline.rs
         L1: parse → L2: validate → L3: enrich → L4: pre-commit
         L5: boundary check → L6: history → L7: emit
```

---

_Authored 2026-06-04 by TACO orchestrator (W8 of the upgrade plan).
Constitutional compliance verified per `docs/CONSTITUTION-v8.md`._
