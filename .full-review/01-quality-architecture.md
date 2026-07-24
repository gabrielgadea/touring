# Phase 1: Code Quality & Architecture — Consolidated

> Detail: `01a-code-quality.md` (F1.1–F1.6) · `01b-architecture.md` (F1.7–F1.12). Both agents Read real source + ran Touring CLI; baseline corrections noted.

## Verdict

**Structurally elite, mechanically indebted.** 0 Critical · 7 High · 10 Medium · 8 Low. The macro-architecture is best-in-class (verified, not asserted); the real work is mechanical (size, disk-orphan hygiene, doc drift, one dep pin).

## Code Quality (F1.1–F1.6)

| # | Sev | Finding | Evidence | Fix |
|---|-----|---------|----------|-----|
| Q1 | High | 12 dead-on-disk files | 4 hooks-shared A5 orphans + 8 dead cortex handlers (`self_reflection.rs`, `reasoning_advanced.rs`…), `external_module_refs=0` | ✅ **DONE 2026-06-20** — removed **7,235 LOC**, `cargo check --workspace` green (rc=0, 26.11s); backup `/tmp/dead-files-2026-06-20.tgz`. Gabriel: `git rm` to commit. |
| Q2 | High | Monolithic CLI handler `cli_decompose_create` | `touring-cli/src/cli/handlers/decompose.rs` — 203-LOC fn, CC=388, F1.4=0.563 (parse+inline-SQL+format mixed) | Extract parse/query/format; table-dispatch |
| Q3 | High | God-struct `GeneratorContext` | `touring-generator/src/core/context.rs:` 35 fields, 229 fns, 76-method impl, 4509 LOC | Split by responsibility (SRP) |
| Q4 | High | 27 source files >2000 LOC (154 >800) | `find crates/*/src` | Carve to ≤800 via existing `#[path]` pattern |
| Q5 | Medium | Coverage on prod hot paths low | TDG coverage 0.40–0.52 on hot files | Add behavior tests (enabler for Q2–Q4) |

**Verified-clean (do NOT regress / not real defects):**
- **Error handling Diamond:** `deny(clippy::unwrap_used)` in **48/48** lib crates; 0 real prod unwraps (raw 4064 = `#[cfg(test)]`).
- **Markers noise:** panic! 321 → **3 real** (1 documented design panic `candle_embedder.rs:372`); todo!/unimplemented! → **0 real** outside dead code.
- **`gate_metrics.rs` "dup" = NOT duplication** — byte-identical but the hooks-shared copy is uncompiled dead-on-disk (no `mod` decl; re-exports kernel's at `lib.rs:51`).

## Architecture (F1.7–F1.12)

| # | Sev | Finding | Evidence | Fix |
|---|-----|---------|----------|-----|
| A1 | High | `cargo-deny bans` FAILS (2 errors) | `touring-harness-mcp/Cargo.toml:21` pins `schemars="0.8"` vs workspace 1.2.1 (rmcp 1.2) → duplicate schemars/schemars_derive | `schemars = { workspace = true }` (1-line) |
| A2 | High | ARCHITECTURE.md drift (self-detected) | `sync_metrics.py --check` → "DRIFT…stale"; doc 45 crates/532k vs real 44+benches/544k | `--sync` + wire `--check` into CI |
| A3 | High | 231 `Result<_,String>` (RBP-03 incomplete) | bindings 51, hook-runtime 42 lead | thiserror typed errors on consumer-observed APIs |
| A4 | Medium | 5 shim dirs on disk (50 dirs vs 45 members) | A2-fusion leftovers | Remove (flag for git rm) |
| A5 | Medium | `touring-server` 70.9k mega-crate | CLI 48% / MCP 20% / shared 32%; `daemon_client` seam leaks once | Split cli-app + mcp (justified, **not urgent**) |
| A6 | Medium | Wiring/orphan DB has 57 phantom-path entries | stale `crates/touring-telemetry/...` (absorbed W3.6, dir gone) | Rebuild/prune wiring DB; Cadeia-7 staleness |

**Orphan triage (corrects raw 4823):** sample n=60 → **~45% cross-crate public API + ~48% intra-crate-used pub + ~7% (~320) genuinely dead**. Report the ~320, not 4823.

**Verified-elite:** 0 dependency cycles (Tarjan); true **zero-dep kernel** (`touring-foundation` deps `=[]`, 34 pub mods, cross-crate consumed); clean layering, no inversion (via `cargo metadata`); real typestate pipeline (Draft→…→Committed); type-driven storage (`SymbolId(u32)` newtype, per-module typed errors); move-utils-down playbook (dissolved A5 cycle); USP `[workspace.lints]` (clippy::all=deny + 8 RBP-11 ratchets). LICENSE-APACHE present (dual-license OK).

## Meta-finding (the quality engine itself)

`touring-quality --workspace` = 0.59 "Unranked" **disagrees with** `touring-elite` = Diamond because the 50-dim engine is **unfaithful at workspace scope**: F1.1 sums CC over the whole dir; F1.2 penalizes 25k aggregate short-ids; F2.1/F2.4 false-positive on its OWN detector source. **Action: per-function CC + self-exclusion + per-file-then-aggregate workspace mode.** (Per-FILE scoring IS faithful.)

## Critical issues for Phase 2 (Security & Performance) context

- **Hook response-path tail**: prior review flagged post_edit/post_write E2E scan on the response path (F1+F2 perf). Perf agent: verify if still synchronous or now offloaded.
- **`touring_file_ops` path traversal** (prior SEC-01): security agent verify if canonicalize/root-containment guard exists today.
- **cargo-deny advisories = OK** (no CVEs) but **bans FAIL** (schemars) — supply-chain hygiene item, not a CVE.
- **406 `unsafe` blocks** — security/concurrency agent: are they documented (`# Safety`) and necessary?
- **`candle_embedder.rs:372` documented design panic** — confirm it's truly unreachable in prod.
- Big async hot paths to profile: `hook_runtime.rs` (3102), `cortex/handlers/enrichment.rs` (2747), `server/tools_infra.rs` (2298).
