# `cli_handlers` Family — Decomposition Inventory & Blueprint

> **Master Plan A.W2.P1.T4** (N01 hotspot). Inventory + dispatch map + target
> module design for decomposing the 18,839-LOC `cli_handlers` family. Measured in
> loco 2026-06-04. This is the **blueprint** — no code moved yet; it gates A.W2.P2/P3.

## 1. Current state (FACT [1.0])

- **194** `pub fn cli_*` handlers across **16 files**, **18,839 LOC** total.
- Largest: `cli_handlers.rs` **9,077 LOC** (~143 core handlers, frozen by file-size gate at ≤9,500).
- The core file mixes ~30 domains behind a single string-match dispatch.

| File | LOC | Domain |
|------|----:|--------|
| `cli_handlers.rs` | 9,077 | core dispatch + ~30 domains |
| `cli_handlers_decompose.rs` | 2,665 | decompose DAG |
| `cli_handlers_index.rs` | 1,246 | index |
| `cli_handlers_mcp.rs` | 1,114 | MCP tool bridge |
| `cli_handlers_semantics.rs` | 700 | resolve-def / find-refs / rename (N07 fixed) |
| `cli_handlers_repo_score.rs` | 605 | repo score |
| `cli_handlers_evolution.rs` | 578 | evolution |
| `cli_handlers_repo_health.rs` | 417 | repo health |
| `cli_handlers_kpi.rs` | 414 | kpi |
| `cli_handlers_scout.rs` | 409 | scout |
| `cli_handlers_polyglot.rs` | 385 | polyglot |
| `cli_handlers_session.rs` | 318 | session |
| `cli_handlers_file_knowledge.rs` | 314 | file-knowledge |
| `cli_handlers_mutation_test.rs` | 254 | mutation test |
| `cli_handlers_wiring_repair.rs` | 180 | wiring repair |
| `cli_handlers_entity.rs` | 163 | entity |

## 2. Core domains (handlers in `cli_handlers.rs`, by count)

```
decompose 10 · wiring 9 · ast 9 · saga 7 · viz 6 · gotcha 6 · tantivy 5 ·
memory 5 · workflow 4 · plugin 4 · jobs 4 · suggestion/suggest 6 · health 3 ·
granularity 3 · ssr 2 · skip 2 · search 2 · mpatch 2 · learning 2 · inferlets 2 ·
hook 2 · gate 2 · cognitive 2 · cascade 2 · acp 2 · (world/tokio/status/… singletons)
```

Dispatch today: string `match` (e.g. `"wiring.cycles" => cli_wiring_cycles`).

## 3. Target design — `enum CliCommand` + `src/cli/`

Group the ~30 domains into **9 cohesive modules** under `crates/touring-hooks/src/cli/`,
each < 1,500 LOC, with a typed dispatch enum replacing the string match:

```rust
pub enum CliCommand {
    Wiring(WiringCmd),      // wiring.rs   ← wiring(9) + suggestion/suggest(6) + wiring_repair
    Ast(AstCmd),            // ast.rs      ← ast(9) + search(2) + ssr(2) + skip(2)
    Intelligence(IntelCmd), // intel.rs    ← gotcha(6) + cognitive(2) + memory(5) + file_knowledge
    Decompose(DecomposeCmd),// decompose.rs (satellite already exists, 2,665 LOC)
    Saga(SagaCmd),          // saga.rs     ← saga(7) + cascade(2)
    Viz(VizCmd),            // viz.rs      ← viz(6)
    Search(SearchCmd),      // tantivy.rs  ← tantivy(5)
    Runtime(RuntimeCmd),    // runtime.rs  ← jobs(4) + inferlets(2) + workflow(4) + mpatch(2)
    Rl(RlCmd),              // rl.rs       ← learning(2) + gate(2) + health(3) + granularity(3)
    Plugin(PluginCmd),      // plugin.rs   ← plugin(4) + acp(2) + hook(2)
    // singletons (status/doctor/tokio/world) → misc.rs
}
```

`cli_handlers.rs` becomes a **< 200-LOC façade**: `pub use cli::*;` + the typed
dispatch (`CliCommand::from_str(verb).dispatch(rt, payload)`).

## 4. Phased execution (risk-managed — W8 pivot model)

1. **A.W2.P2 (zero-risk satellites)** — move the small standalone satellites
   (`kpi` 414, `evolution` 578, `repo_score` 605, `repo_health` 417, `polyglot`
   385, `scout` 409) into `src/cli/`. Each is already a separate file; this is a
   path move + re-export. `cargo test -p touring-hooks` after each.
2. **A.W2.P2 (medium)** — move `decompose` (2,665) + `index` (1,246) into `cli/`,
   resolving re-exports.
3. **A.W2.P3 (core split)** — carve `cli_handlers.rs` (9,077) into
   `cli/{wiring,ast,intel,saga,viz,tantivy,runtime,rl,plugin,misc}.rs` + `cli/dispatch.rs`
   via `taco-forge perfect-edit` per extraction. Façade ends < 200 LOC.
4. **A.W2.P4 (validation)** — `wiring orphans` 0 new; clippy `-D warnings` 0;
   `cargo test --workspace`; `file_size_gate.py --check` confirms `cli_handlers.rs`
   below its (now lowered) whitelist cap.

## 5. Risks

| Risk | Prob/Impact | Mitigation |
|------|:-----------:|------------|
| SCC in `touring-hooks` blocks clean module split | HIGH/HIGH | Façade `pub use` (W8 pivot); keep within crate, no new crate |
| 194 call-sites break on rename | MED/HIGH | Keep `pub fn cli_*` names; only relocate + re-export; `cargo check` per move |
| Dispatch string→enum changes behavior | MED/HIGH | Map every existing arm 1:1; regression test of `from_str` table |

**DoD A.W2**: `cli_handlers.rs` residual < 200 LOC; typed `enum CliCommand`;
0 regression; `file_size_gate.py --check` green with lowered cap.

## 6. Status (2026-06-05) — substantial milestone, A.W2.P4 remaining

**Done (validated):**
- Core `cli_handlers.rs`: **9,077 → 6,051 LOC** (−3,026, **−33.3%**).
- **23 cohesive modules** in `src/cli/` (~5,400 LOC): kpi, evolution, repo_score,
  repo_health, polyglot, scout, wiring, viz, gotcha, saga, tantivy, plugin, jobs,
  cognitive, granularity, cascade, health, inferlets, gate, learning, workflow,
  acp, **memory**.
- Extraction is a **path-move + façade `pub use`** (every `pub fn cli_*` name and
  the string-match dispatch preserved → 0 call-site churn, `hook_registry.rs`
  untouched). Deterministic extractor: `scripts/cli_domain_extract.py`.
- Gates: `cargo check` 0 errors across **default / acp-protocol / all-features /
  workspace**; `clippy` 0 (default + all-features, excl. 1 pre-existing
  `daemon.rs:1308` + 3 `touring-offensive` crate warnings); **4,013 tests pass**;
  `file_size_gate.py --check` green (cap ratcheted **9,500 → 6,100**).
- Recovery + hardening this wave: repaired a build broken by a rate-limited
  subagent (acp cfg-gate + `workflow.rs` private-helper imports); cleared 8 clippy
  issues; fixed a **real functional bug** in `session_hooks.rs` (the ES2/ES3
  HarnessContract attest + cross-agent ledger were dead code after
  `emit_context_for_event` — a `-> !` `process::exit(0)` — now the emit runs last);
  registered gotcha `clippy --fix strips cfg-gated imports`.

**Remaining — A.W2.P4** (to hit the literal DoD `< 200 LOC` + typed dispatch):
- 63 `pub fn cli_*` still in core: decompose(10), ast(9), suggest/suggestion(7),
  ssr/skip/search/mpatch/hook(10), + ~27 singletons (status, doctor, world, tokio,
  mcts, predict, prove, prompt, profile, harness, graph, flywheel, query, metadata,
  incremental, shadow, session, rl, pre, mcp, …).
- Each needs a new `DOMAINS` config in `cli_domain_extract.py` plus per-domain
  shared-helper promotion (`pub(crate)`) and cfg-gate fixes — the same friction
  paid for `memory` (4 symbols) and `acp`/`workflow`.
- Final step: replace the string-match dispatch with a typed `enum CliCommand`
  (1:1 arm map + `from_str` regression test), shrinking the façade < 200 LOC.
- Best executed in a fresh-context session with parallel engineer subagents
  (this wave hit subagent rate-limiting + context saturation).
