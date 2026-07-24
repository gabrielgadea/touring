# Architecture — the *why* behind Touring

> An **explanation** (Diátaxis): understanding-oriented. It explains the ideas
> and trade-offs, not the exhaustive structure. For the detailed crate map see
> `ARCHITECTURE.md`; for catalogs see `docs/reference/`; for tasks see
> `docs/how-to/`. Master Plan D.W4.P4.

Touring is an **agentic code harness**: infrastructure that sits *around* a
coding model and makes its actions verifiable, safe, and self-improving. It is
deliberately a **kernel, not a distro** — the substrate (indexing, verification,
sandboxing, learning) rather than a polished end-user editor. This document
explains the four ideas that organize the codebase.

## The four layers

```
   ┌─────────────────────────────────────────────────────────────┐
   │ L4  LEARNING      RL (LinUCB + Q-table), memory, evolution,   │
   │                   gotchas, health-delta — the system improves │
   ├─────────────────────────────────────────────────────────────┤
   │ L3  EXECUTION     Code Execution Gateway X0..X9 — every code- │
   │                   bearing action is classified + sandboxed    │
   ├─────────────────────────────────────────────────────────────┤
   │ L2  GENERATION    VGP — symbols are verified against the index│
   │                   before code is generated (anti-hallucination)│
   ├─────────────────────────────────────────────────────────────┤
   │ L1  INTELLIGENCE  index / AST / wiring graph — the navigation │
   │                   substrate everything else queries           │
   └─────────────────────────────────────────────────────────────┘
```

Each layer only depends on the ones below it. You can use L1 (indexing,
blast-radius, wiring) without ever touching L3/L4 — which is exactly what the
`touring index`/`touring ast`/`touring wiring` commands do.

### L1 — Intelligence: index before inference

The foundational claim is that an agent should **navigate**, not guess. Touring
builds a local symbol index (no cloud round-trip) and a wiring graph of
producer→consumer relationships. From these come blast-radius analysis (what
breaks if I change this), orphan detection (pub symbols with no consumers), and
cycle detection (Tarjan SCC). The golden rule — *file metadata first* — is the
practical expression: read `blast_radius`/`quality_score` before editing.

### L2 — Generation: verify before you write (VGP)

The **Verified Generation Protocol** exists because the most expensive failure
mode of a coding agent is confidently referencing a symbol that does not exist.
Before code is generated, the symbols it cites are checked against the index. A
symbol that is not found is not "probably fine" — it is removed from the plan or
justified as `to_be_created`. This is the anti-hallucination spine, mirrored at
the human-process level by the Symbol Verification Table.

### L3 — Execution: deny-by-default sandbox (CEG)

The **Code Execution Gateway** is a ten-stage typestate pipeline (X0 CAPTURE →
X9 LEARN) that intercepts every code-bearing action — Bash, Write, generated
scripts — *before* it runs. Two stages are structurally unskippable: X3 (VGP)
and X5 (SANDBOX). Executed code never gets ambient authority; it declares the
capabilities it needs (filesystem read/write, network, subprocess, env) and a
profile resolves each to allow/deny/prompt, **deny winning ties**. On Linux this
is enforced by the kernel (landlock + rlimit); elsewhere it degrades *loud*, not
silent. The invariant, like the hooks, is fail-open: the gateway never blocks a
session, it records outcomes and warns. (Full pipeline:
`docs/reference/` and the CEG rule.)

### L4 — Learning: the system gets better between sessions

Outcomes feed reinforcement learning (a LinUCB contextual bandit plus a Q-table)
and a memory/gotcha store. A tool that worked earns reward; a repeated error
becomes a gotcha that is surfaced *before* the next attempt. The
`emit_context_for_event` at session start re-attests a constitutional digest so
later stages can compare pre- vs post-session behavior. This is the layer that
turns a static tool into one that converges.

## Process topology (why three binaries)

Touring runs as three distinct processes so that crashes and restarts are
contained and a single backend can serve many concurrent sessions:

| Process | Role | Multiplicity |
|---|---|---|
| `touring-daemon` | RPC backend; holds the index, RL state, socket | **one per user** (singleton via flock) |
| `touring` (CLI) | ephemeral read-mostly client over the socket (<10ms) | many, one-shot |
| `touring-hook` | ephemeral lifecycle handler (PreToolUse, etc.) | many, ≤5s, fail-open |

(A fourth, `touring serve` / `touring-mcp`, bridges stdio↔socket for MCP — one
per editor session.) The daemon is the single source of truth; clients and hooks
are stateless and disposable. This is why a hook can be slow-path-free: it asks
the already-warm daemon instead of recomputing.

## Cross-cutting principles

- **Fail-open everywhere on the interactive path.** Hooks and the gateway exit 0
  and degrade to no-ops on error. Breaking the user's session is never an
  acceptable failure mode.
- **Dogfooding ("the cure is within").** The same gates Touring offers — file
  metadata, blast radius, wiring integrity, file-size budgets, drift checks — are
  applied to Touring's own source (`docs/*.py` gates, `file_size_gate.py`,
  `sync_metrics.py`). Where they were *not* yet applied is tracked as debt.
- **Determinism over identity.** Entity identity is derived from canonical
  inputs, not creation order or memory addresses (RFC-004), so the same inputs
  always produce the same result across sessions.

## Known gaps (honesty first)

This is a maturing system, not a finished product. The honest current state:

- **Build from source only.** No prebuilt binary yet (~6 min release build);
  tracked as Master Plan B.W1.
- **Syntactic LSP.** Symbol resolution is index/AST-based; there is no
  cross-file type inference yet (Master Plan A.W4 — salsa-backed LSP).
- **No public extension ABI.** Hooks and generators are extended by editing the
  Rust crates and rebuilding; the stable contract is RFC-006 (planned, B.W3).
- **Single primary provider.** The model integration is Claude-centric; a
  provider abstraction (OpenAI/Ollama) is planned (B.W2 / E.W3).
- **Internal monolith debt.** Some modules remain large (e.g. `cli_handlers`
  decomposition is partial — Master Plan A.W2; `lifecycle.rs` — A.W3.P3). These
  do not affect correctness but are tracked by the file-size gate.

For the full diagnostic that grounds these gaps, see
`docs/2026-06-04-touring-diagnostico-elite-mercado.md`.

## Where to go next

- **`ARCHITECTURE.md`** — the detailed crate-by-crate structure and dependency
  layering (the *what*, kept current by `sync_metrics.py`).
- **`docs/reference/`** — generated catalogs: generators, MCP tools, hooks.
- **`docs/how-to/`** — task recipes (add a language, extend the generator, debug
  the CEG, build an MCP tool, run E2E).
- **`docs/tutorial/`** — `getting-started.md`, `first-hook.md`.
