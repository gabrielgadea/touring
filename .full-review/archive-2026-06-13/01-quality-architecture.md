# Phase 1: Code Quality & Architecture Review

> Touring workspace · 2026-06-13 · agents: comprehensive-review:code-reviewer (1A) + architect-review (1B)
> Full detail: `01a-code-quality.md` · `01b-architecture.md`

## ⚠ Ground-truth corrections (the review corrected the scope's headline numbers — FACT)

These supersede the raw greps in `00-scope.md` and are the honest baseline going forward:

| Scope claim | Corrected reality | Evidence |
|---|---|---|
| ~3,686 prod `unwrap()` | **~124 prod** unwrap (rest is `#[cfg(test)]`); `unwrap_or_default` already used 935× | per-crate test/prod split |
| ~375 prod `panic!` | **~16 prod**; several `unimplemented` are antipattern-detector strings | split |
| "weak lint policy: 8/46 crate roots" | **NOT weak** — `Cargo.toml:586` `[workspace.lints.clippy] all = deny`, inherited by 37/38 crates via `workspace = true`. clippy `-D warnings` = 0 | `Cargo.toml:586-627` |

**Implication**: the prototype-era robustness debt was largely paid down. The elite gap is no longer "stop panicking" — it is **raising the enforced ceiling** and **structural** (crate topology, public-API governance, doc-truth).

## Code Quality Findings (1A) — 0 Critical / 3 High / 7 Medium / 6 Low

- **[High] Lint ceiling stops at `clippy::all`** — no `pedantic`, `unwrap_used`, `expect_used`, `missing_docs`; `indexing_slicing = allow` (panic risk). `Cargo.toml:586-627`. The CEG already denies unwrap (`touring-ceg/gateway/mod.rs:43`) — ratchet that policy outward via `[workspace.lints]`.
- **[High] 195 `pub fn cli_*` handlers** duplicate the same parse→execute→envelope prelude (~90% copy). A `CliHandler` trait collapses them.
- **[High] Genuinely-dangerous prod unwraps in untrusted-input/daemon paths** — `touring-server/src/cli/assist.rs:256` (`.parse().unwrap()` on user-supplied line:col), `touring-dispatch/src/daemon.rs:1226` (daemon-singleton blast radius). These are the ~124 that matter; the elite move is `#![deny(clippy::unwrap_used)]` so no NEW ones appear.
- **[Med] `touring-hooks-core/src/knowledge.rs` (~3,149 prod LOC)** — the one true god-file: 5 concerns + 327L/234L DDL functions. Split into schema/relations/bash_outcomes/edit_history.
- **[Med] JSON output boilerplate copy-pasted across ~61 files** — no shared `json_envelope` helper.
- **[Med] 370 `.map_err(|e| format!(...))`** stringly-typed errors — thiserror `#[from]` collapses ~60%.
- **[Med] 73 `allow(dead_code)`** (down from 91); ~10 bare/undocumented are genuine REGRA #0 candidates (`cross_agent_ledger.rs:101,213,231`).
- **[Med] 244 prod `eprintln!`** bypass `tracing` (827 tracing calls) — unfilterable by `RUST_LOG`.

**Biggest lever (1A):** lift the lint ceiling — `unwrap_used`/`expect_used`/`missing_docs`/`indexing_slicing` into `[workspace.lints]`, ratcheting `deny` outward from the already-clean CEG. Converts "auditable" from claim to enforced, regression-proof invariant.

## Architecture Findings (1B) — 3 High / 3 Med-High / 3 Med / 1 Low-Med

- **A1 [High] `touring-server` is the next monolith** — 67.9k LOC (13.6%, largest). Internally two independent products: `cli/` (89 files) + `server/` MCP surface. Ships **194 `#[tool]` macros** (not the documented 164) + ~120 CLI cmds in one compilation unit.
- **A2 [High] Permanent shim layer from a half-finished fusion** — `touring-{ast,learning,cognitive,antt,wasm}` are 6–12-LOC re-export shims yet still have 13–15 live consumers each; old+new names coexist (touring-learning shim ⟷ touring-intelligence 64.3k). Contributor trap.
- **A3 [High] ARCHITECTURE.md describes an architecture that no longer exists** — lists 38 crates + ~15 phantom dirs (touring-core/-index/-vfs/-semantics…); still says `touring-hooks 127,575 LOC`. (Session G-1 fixed the *metrics table*, not the *crate map*.) `sync_metrics.py` gates counts, not topology.
- **A4 [Med-High] `touring-foundation` is a god-kernel** — fan-in 20 (highest), 21.7k LOC; sentinel/embedding/failover/conflict/telemetry mixed with true kernel (schema/config/error).
- **A5 [Med-High] Data layer entangled into the hooks plane** — `FileKnowledgeDB` (4.5k) + tantivy in `touring-hooks-core`; CRDT graph under `touring-intelligence/src/rl/memory/`; DDL in foundation. `touring-storage` exists but doesn't own the primary stores. Can't depend on "Touring's index" without the whole hooks plane.
- **A6 [Med-High] `mcp-curated`/`mcp-legacy` are dead feature flags** (both gate 0 blocks). The 194-tool MCP surface — Touring's most important public API — has no curation gate, no versioning, no stable contract.
- **A7 [Med] IoC seam (touring-contracts) correct but ad-hoc** — only 2 consumers; the boundaries that would benefit most (hooks→intelligence, generator→LLM) remain hard-wired.
- **A8 [Med] Structurally LLM-less** — `LlmProvider` exists but only `NoopLlm` implements it, and the trait lives in touring-generator not touring-contracts.

**Biggest lever (1B):** split `touring-server` → `touring-cli-app` + `touring-mcp`, and make `mcp-curated` a real `#[cfg]` gate (A1+A6). Removes the next monolith, decouples two products' build times, and gives Touring its first semver-governable public API — exactly what the B-W1/B-W3 public-release waves require.

## Critical Issues for Phase 2 Context (security/performance)

1. **194 MCP `#[tool]` macros with no curation/versioning gate (A6)** — every tool is an attack/abuse surface; security must assess input validation + capability scoping per family, and the gap between 194 exposed vs 22 intended.
2. **~124 prod unwraps incl. untrusted-input paths (1A #3)** — `assist.rs:256` parses user line:col with `.unwrap()`; daemon paths panic-on-singleton. Security: panic = DoS vector on the daemon. Performance: panic-unwind cost + daemon restart.
3. **`unsafe` (424 sites) + landlock/sandbox/supervised-exec (CEG) + touring-offensive cvc5** — the CEG is the security crown jewel; verify the X0-X9 typestate actually enforces capability deny-by-default and landlock is wired on Linux. Performance: sandbox/dry-run overhead on hot hook paths.
4. **Data-layer entanglement (A5)** — tantivy + sqlite + CRDT on the hooks plane → assess concurrency (locking, write amplification), and whether hook latency (pre_read/pre_edit on every tool call) is bounded.
5. **`touring-foundation` fan-in 20 god-kernel (A4)** — a perf regression or panic here blasts the whole workspace.
