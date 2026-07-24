# Phase 1A — Code Quality Review (Rust)

> **Target**: Touring Rust workspace `~/.claude/rust` (46 crates, ~498,697 src LOC, daemon v30.3.0)
> **Date**: 2026-06-13 | **Lens**: Rust-idiomatic, elite-of-market bar | **Mode**: read-only, evidence-cited
> **Method**: real greps + Python AST-region splitting + file reads + 5 parallel Explore agents. Daemon up (`doctor` 5/6 ok).
> **Confidence**: `FACT [1.0]` = measured/quoted · `INFERENCE [0.7-0.9]` = deduced from evidence.

---

## 0. Executive Reframe — the headline numbers are mostly TEST code

The scope's premise (~3,686 prod `unwrap()`, 375 `panic!`, 57 `unimplemented!`, "weak workspace lint policy") is **substantially refuted by prod/test region analysis**. Two corrections dominate this phase:

1. **The error markers are ~97% in test code.** Splitting every `.rs` at its `#[cfg(test)]` boundary (Python, all 46 crates) yields prod-path counts an order of magnitude smaller than the raw grep:

   | Marker | Raw (scope) | **PROD (this pass)** | TEST | %test |
   |---|---:|---:|---:|---:|
   | `.unwrap()` | ~3,686 | **~124** | 3,836 | 97% |
   | `.expect(` | ~4,537 | **~424** | 2,469 | 85% |
   | `panic!(` | 375 | **~16** | 299 | 95% |
   | `unimplemented!/todo!` | 57 | **~16** (many are detector strings) | 24 | — |
   | `unwrap_or_default` | — | **935** | 51 | — |

   `FACT [1.0]` — method: `crates/*/src/**/*.rs`, test region = lines at/after first `#[cfg(test)]` + files in `tests/`/`*_test.rs`/`tests.rs`. The heuristic can mis-bin a few prod sites that follow an early inline test helper, so true prod `unwrap` is **~124-300**, not thousands. The high `unwrap_or_default` count (935) shows the codebase **already prefers the safe pattern**.

2. **The workspace lint policy is NOT weak — it is Cargo-native and inherited.** `Cargo.toml:586` defines `[workspace.lints.clippy] all = { level = "deny", priority = -1 }`, and **37 of 38 crates with a `[lints]` section set `workspace = true`** (`FACT [1.0]`). The scope's "8 of 46 crate roots deny/forbid" measured the secondary `#![deny(...)]` *inner-attribute* mechanism and missed the primary one. `clippy::all = deny` is enforced workspace-wide; clippy `-D warnings` is already 0.

**Net**: the code-quality gap to elite is **not gross robustness** (that was the prototype-era story, now largely paid down). It is **(a) a handful of genuinely-dangerous prod unwraps in untrusted-input/daemon paths, (b) a lint *ceiling* that stops at `clippy::all` and never reaches `pedantic`/`unwrap_used`/`missing_docs`, (c) systemic CLI-handler + JSON + error-mapping duplication, and (d) two true god-files.**

---

## 1. Error-Handling Robustness

### 1.1 [High] Genuinely-dangerous prod unwraps in untrusted-input / daemon paths

Most prod unwraps are benign (`Mutex::lock().unwrap()`, Tarjan SCC invariants, infallible `serde_json::to_value`). The dangerous minority are panics reachable from **external input** or in the **daemon request path**:

- **`crates/touring-server/src/cli/assist.rs:256`** `FACT [1.0]` — `let line: usize = after_colon.parse().unwrap();` parses a user-supplied `path:line:col` spec. The `path:line:col` branch is guarded by an `is_ok()` on line 248, but the trailing single-colon branch (line 256) reaches `after_colon.parse().unwrap()` after only structural checks; a spec like `foo.rs:999999999999999999999` (overflow) or non-numeric tail panics the handler.
- **`crates/touring-dispatch/src/daemon.rs:1226`** `FACT [1.0]` — `serde_json::to_value(caps).unwrap()` in the ACP success-response path. `to_value` on a well-typed struct is effectively infallible, but a panic here is in the **singleton daemon** request loop — blast radius = all hooks in the session. Prefer `?`/`unwrap_or_else(|_| Value::Null)`.
- **`crates/touring-storage/src/embeddings/providers/fastembed.rs:175`**, **`crates/touring-intelligence/src/ann/monetary_parser.rs:89,128`**, **`crates/touring-intelligence/src/rl/semantic/candle_embedder.rs:378`** `FACT [1.0]` — `panic!` / `unwrap_or_else(|| panic!(...))` on model/regex init inside `Lazy::new`. These are startup, pre-validated-pattern contracts; acceptable but they abort the process on a corrupt model file instead of degrading.

**Verified benign (do NOT flag):** `crates/touring-hook-handlers/src/hooks/team_hooks.rs:121` — `state.get("subtasks").unwrap().as_array().unwrap()` is safe-by-construction: `has_pending` is computed 3 lines above with `.and_then(as_array).map(!is_empty).unwrap_or(false)`, proving the shape before the unwrap. `crates/touring-hook-runtime/src/hook_runtime.rs:361,370` + `inferlets.rs:151` are `Mutex::lock().unwrap()` (poison-only). `crates/touring-hook-runtime/src/wiring.rs:356-375` are Tarjan-SCC `.get().unwrap()` on indices the algorithm just inserted (infallible invariant).

**Elite pattern / fix**:
```rust
// assist.rs:256 — fallible parse with actionable error instead of panic
let line: usize = after_colon
    .parse()
    .map_err(|_| AssistError::BadSpec { spec: spec.to_string() })?;

// daemon.rs:1226 — never panic in the daemon loop
let body = serde_json::to_value(caps).unwrap_or_else(|e| {
    tracing::error!(error=%e, "caps serialization failed");
    serde_json::Value::Null
});
```

### 1.2 [Medium] `expect()` is the larger remaining surface (~424 prod)

Prod `expect(` (~424) outnumbers prod `unwrap` (~124) 3:1. Many carry good messages, but `expect` still panics. The CEG already documents the target invariant — `crates/touring-ceg/src/gateway/learn.rs:26`: *"No `.unwrap()` in production paths"* `FACT [1.0]`. Extend that contract from CEG to the L1 crates (`touring-foundation`, `touring-storage`) and the daemon path (`touring-dispatch`, `touring-server`).

### 1.3 [Low] Error-infra adoption is partial (thiserror 20/46, anyhow 13/46)

`FACT [1.0]` — 20 crates depend on `thiserror`, 13 on `anyhow`. Combined with 370 `.map_err(|e| format!(...))` sites (§4.4), the gap is **stringly-typed errors**: errors are built as `String` rather than typed enums with `#[from]`, which loses the error chain and forces re-formatting at every call site.

### 1.4 Recommended rollout strategy (`#![deny(clippy::unwrap_used)]`)

Already proven on the gateway (`crates/touring-ceg/src/gateway/mod.rs:43` carries `#![cfg_attr(not(test), deny(clippy::unwrap_used))]` `FACT [1.0]`). Ratchet outward in this order, lowest-risk first:
1. **L1 infra**: `touring-foundation`, `touring-storage`, `touring-identity`, `touring-contracts` (smallest prod-unwrap counts: 4/1/varies).
2. **Daemon path**: `touring-dispatch`, `touring-server` (singleton blast radius).
3. **Intelligence/code**: `touring-intelligence`, `touring-code`, `touring-cortex` (largest counts; do last, with `unwrap_or_default`/`?` sweeps).
Use `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]` so the 97% test unwraps stay legal.

---

## 2. Lint Policy Gap (corrected)

### 2.1 [High] The ceiling is `clippy::all` — elite repos go to `pedantic` + correctness sub-lints

`Cargo.toml:586-627` `FACT [1.0]`: the floor is `clippy::all = deny` (good), with deliberate relaxations: `indexing_slicing = "allow"` (line 594 — **re-introduces panic risk** on `vec[i]`/`map[k]`, relaxed 2026-04-20 for serde ergonomics) and a long test-harness allow-list. `[workspace.lints.rust]` only sets `unexpected_cfgs = "allow"`. There is **no `pedantic`, no `unwrap_used`, no `missing_docs`, no `panic`/`unwrap_in_result`** at workspace scope.

What an elite Rust repo (rustls, tokio, ripgrep) enforces, and the migration cost:

```toml
[workspace.lints.clippy]
all       = { level = "deny",  priority = -1 }
pedantic  = { level = "warn",  priority = -1 }   # opt-in selectively; ~hundreds of warns first pass
# correctness / robustness — the real value:
unwrap_used        = "warn"   # -> deny per-crate via ratchet (§1.4)
expect_used        = "warn"
panic              = "warn"   # in non-test
indexing_slicing   = "warn"   # REVERSE the 2026-04-20 allow; fix the ~dozen serde sites with .get()
missing_panics_doc = "warn"
[workspace.lints.rust]
missing_docs            = "warn"   # currently only 3 crates #![deny(missing_docs)]
unreachable_pub         = "warn"   # surfaces over-exposed pub (see §5.5)
```

**Migration cost** `INFERENCE [0.8]`: `pedantic` will emit hundreds of warnings (mostly `must_use_candidate`, `missing_errors_doc`, `module_name_repetitions`) — adopt as `warn` + triage, not `deny`. `unwrap_used`/`expect_used` as `warn` is near-free given §0 (only ~124/~424 prod sites). Reversing `indexing_slicing` requires fixing ~a dozen `Value["key"]` sites with `.get()` — bounded, 1-2 days. `missing_docs` is the biggest lift (rustdoc coverage was ~68% in the prior diagnostic).

### 2.2 [Low] Inner-attribute lints are inconsistent across crate roots

`FACT [1.0]`: only `#![forbid(unsafe_code)]` ×4, `#![deny(missing_docs)]` ×3, `#![warn(missing_docs)]` ×4, `broken_intra_doc_links` ×7 — applied ad-hoc. Once §2.1 moves these to `[workspace.lints]`, delete the per-crate duplicates to avoid drift. Note `touring-generator/src/lib.rs:28` has `deny(missing_docs)` **commented out** (prior in-loco "PARTIAL") — re-enable or move to workspace.

---

## 3. Complexity & Maintainability

Per-file prod/test split + god-function table (`FACT [1.0]`, via Explore agent + reads):

| File | LOC | Prod | God fns (>150L) | Max nest | Verdict |
|---|---:|---:|---|---:|---|
| `touring-hooks-core/src/knowledge.rs` | 4456 | 3149 | `ensure_schema` (296, 327L), `migrate_schema` (623, 234L) | 6 | **SPLIT** |
| `touring-generator/src/core/context.rs` | 4131 | 4131 | `append` (~7214, 246L) | 8 | adapter tower — accept |
| `touring-hook-handlers/src/hooks/pre_read.rs` | 3797 | 1124 | `run_returning_impl` (873, 239L), `build_db_context` (902, 160L) | 6 | cohesive orchestrator |
| `touring-hooks-shared/src/gate_metrics.rs` | 3186 | 2523 | `GateMetrics::default` (1248, 610L), `record_core_pinning` (1402, 420L), `capture` (1444, 267L) | 5 | metrics store — accept |
| `touring-cli/src/cli_suggester.rs` | 2651 | 1871 | `classify_bash` (485, 292L) | 5 | cohesive classifier |

### 3.1 [Medium] `knowledge.rs` — true low-cohesion god-file (the one real split)

`FACT [1.0]` — 3,149 prod LOC mixing **5 orthogonal concerns**: file-metadata CRUD, relations graph, bash-outcome tracking, edit history, gotcha/error patterns. Two DDL god-functions (`ensure_schema` 327L, `migrate_schema` 234L) are pure schema boilerplate. **Fix**: extract `schema.rs` (DDL), `relations.rs`, `bash_outcomes.rs`, `edit_history.rs`, leaving `knowledge.rs` as CRUD core. This is the highest-value maintainability split remaining post-monolith-decomposition.

### 3.2 [Low] The other four big files are large-but-cohesive (do NOT split)

- `context.rs` (8-deep nesting, 41 impls) is the **GeneratorContext cross-crate adapter tower** — its size is a deliberate closure-injection contract; fragmenting it would scatter the injection points. `INFERENCE [0.85]`.
- `gate_metrics.rs` god-functions are `Default::default()` initializing ~150 `AtomicU64` counters — observability surface, not algorithmic complexity. Accept.
- `pre_read.rs` is 70% test (only 1,124 prod LOC); `run_returning_impl` coordinates the 4-layer signal pipeline — long by orchestration, single responsibility.
- `cli_suggester.rs` `classify_bash` (292L) is regex-dense Bash classification; well-factored per tool type.

**Elite guard already exists**: `docs/file_size_gate.py` (BUDGET=5000) is wired in `ci.yml` `FACT [1.0]` — the ratchet that would have caught the old `lifecycle.rs` 19k regression. Consider lowering to 3000 for *prod* LOC (test files exempt) to force `knowledge.rs` and surface the next offenders early.

---

## 4. Duplication (the most systemic theme)

### 4.1 [High] 195 `pub fn cli_*` handlers share a ~identical prelude

`FACT [1.0]` — `grep 'pub fn cli_' = 195`. Sampled `cli/handlers/mutation_test.rs`, `decompose.rs`: every handler = parse `&Value` → execute → wrap in `success_envelope`/`failure_envelope` → return `String`. ~90% of each prelude is copy. **Fix**: a `trait CliHandler { type Req; type Res; fn parse(&Value)->Result<Req>; fn execute(Req)->Result<Res>; }` + one generic `dispatch::<H>()` collapses parse/envelope/error-map across all 195. `INFERENCE [0.85]` — this is the single largest LOC-reduction lever in the CLI layer.

### 4.2 [Medium] JSON output boilerplate copy-pasted across ~61 files

`FACT [1.0]` — `to_string_pretty|serde_json::to_string` appears in ~61 files across `touring-cli` + `touring-server` (e.g. `touring-server/src/cli/generate.rs` has ~16-25 hits alone). No shared `print_json`/`json_envelope` helper. **Fix**: `touring-contracts` (the existing leaf crate) gains `fn json_envelope(ok: bool, data: impl Serialize) -> String` + `fn emit_json<T: Serialize>(&T)`; replace ~60 inline sites. Also kills a class of `to_string_pretty(...).unwrap()` panics.

### 4.3 [Low] Hook registry is large (676 names) but clean

`FACT [1.0]` — `touring-dispatch/src/hook_registry.rs` builds ~676 hook-name entries; lookup is O(1) HashMap, no in-registry duplication. The duplication is upstream (195 individual handler fns, §4.1), not the registry. A declarative `register!` macro per handler family (`cli-learning-*`, `cli-wiring-*`) would shrink the registry definition.

### 4.4 [Medium] 370 `.map_err(|e| format!(...))` — stringly-typed error mapping

`FACT [1.0]` — 370 workspace sites (e.g. `touring-bindings/src/desktop/cli.rs:31,69`). A per-crate `thiserror` enum with `#[from]` collapses ~60%+ to plain `?`, restores the error chain, and removes ~220 LOC. Pairs with §1.3.

---

## 5. Code Smells / Anti-Patterns

### 5.1 [Medium] 73 `allow(dead_code/unused)` prod sites — REGRA #0 tension

`FACT [1.0]` — 73 sites (down from prior 91). Distribution: intelligence 8, server 7, foundation 7, cortex 5, generator 4. **Many are legitimately documented** (`touring-foundation/src/cgm/mod.rs:36` explains stable-API suppression; `touring-code/src/ast/store.rs:716` "consumed by observers"; `hnsw_working.rs:71,135` explain index-validity invariants). **The irony, confirmed**: a chunk of "hits" are the antipattern *detector* itself (`touring-analysis/src/quality/antipatterns.rs:20`, `touring-code/src/ast/quality.rs:425,432`, `touring-generator/src/core/context.rs:457,554` — Touring rejecting `#[allow(dead_code)]` in generated code). The genuine REGRA#0 candidates are the **bare, comment-less** suppressions — e.g. `touring-assists/src/handlers/auto_wire.rs:20,28`, `touring-cortex/src/types.rs:97`, `touring-cortex/src/pipeline.rs:182`, `touring-generator/src/source_change/{text_edit.rs:229, applier.rs:155}`, `touring-hooks-core/src/cross_agent_ledger.rs:101,213,231`. **Fix**: wire to consumers, or remove, or add a one-line rationale + `#[doc(hidden)]`.

### 5.2 [Medium] Stringly-typed generator-kind dispatch

`FACT [1.0]` — `touring-dispatch/src/lifecycle/shared.rs:42-167` has `classify_file_to_generator_kind`, `classify_yaml_to_generator_kind`, `classify_rust_to_generator_kind`, `suggest_generator_for_task_subject` all returning `&'static str` literals (`"ProtobufSchema"`, `"FuzzTarget"`, `"RustModule"`…) — while a `GeneratorKind` enum **already exists** in `touring-generator`. **Fix**: return `GeneratorKind`; the string is then `kind.as_str()` at the boundary only. Removes a whole class of typo-bugs and makes the match exhaustive-checked.

### 5.3 [Low] Boolean-param primitive obsession

`FACT [1.0]` — e.g. `touring-hooks-core/src/hooks/activity_hook.rs:142` `emit_pre_compact(project_root, linucb_saved: bool, got_snapshot_saved: bool)` — callsite can't tell the bools apart. **Fix**: a `CompactFlags` struct (field-init is self-documenting). Low frequency; opportunistic.

### 5.4 [Low] `.clone()` density not a problem in the big files

`FACT [1.0]` — prod-region clone counts in the 4 hottest files are low (≤8 each); `cli_suggester.rs` only 3. No avoidable-clone hot-path finding. (Note: workspace-wide clone density was not exhaustively scanned; the sampled hot files are clean.)

### 5.5 [Low] `pub` over-exposure — surfaced by `unreachable_pub`

`INFERENCE [0.7]` — adding `unreachable_pub = "warn"` (§2.1) would mechanically surface struct fields/fns marked `pub` that are only used in-crate. Not separately enumerated here; defer to the lint rollout.

---

## 6. Error Messages & Observability

### 6.1 [Medium] tracing is dominant but mixed with raw `eprintln!`/`println!`

`FACT [1.0]` — 827 prod `tracing::{info,warn,error,debug,trace}!` vs **244 prod `eprintln!`** + 702 `println!`. Many `println!` are legitimate CLI stdout (the product is a CLI), but **244 `eprintln!` in prod** is an observability smell: diagnostics that bypass the tracing subscriber can't be filtered by `RUST_LOG`, structured, or shipped to the gate-metrics layer. **Fix**: audit `eprintln!` → `tracing::warn!/error!`; reserve `eprintln!` for the pre-subscriber-init bootstrap window only. Pairs with the prior diagnostic's note that real errors were emitted at `tracing::debug!` (invisible without `RUST_LOG=debug`).

### 6.2 [Low] Stringly-typed errors reduce actionability

`FACT [1.0]` — the 370 `format!`-based errors (§4.4) produce flat strings with no `source()` chain and no machine-readable kind. Typed `thiserror` enums make errors matchable by callers and let the gate-metrics/RFC-100 layer classify failures. This is the observability twin of §1.3.

---

## 7. Per-Crate Error-Marker Heat Table (prod region)

`FACT [1.0]` — prod-only (region = before first `#[cfg(test)]`); top crates by prod `unwrap`. Test-region totals shown for contrast.

| Crate | prod unwrap | prod expect (approx) | notes |
|---|---:|---|---|
| touring-analysis | 17 | — | several are detector fixtures |
| touring-intelligence | 15 | high | largest test surface; ann/rl panics on model init |
| touring-code | 14 | — | parse-unwraps are test asserts |
| touring-server | 14 | high | daemon path; `assist.rs` parse-unwrap (§1.1) |
| touring-offensive | 13 | — | cvss `panic!` bound check (bug_bounty.rs:121) |
| touring-cortex | 12 | — | |
| touring-hook-runtime | 8 | — | mostly `Mutex::lock().unwrap()` (benign) + Tarjan invariants |
| touring-cli | 7 | — | |
| touring-foundation | 4 | — | L1 — prioritize for `unwrap_used` deny |
| touring-hooks-core | 4 | — | |
| touring-hook-handlers | 3 | — | team_hooks unwrap is safe-by-construction |
| touring-ceg | 1 | 0 | already `deny(unwrap_used)` |
| touring-dispatch | 1 | — | daemon.rs:1226 `to_value().unwrap()` |
| **WORKSPACE PROD TOTAL** | **~124** | **~424** | vs 3,836 / 2,469 in test |

---

## 8. Severity Summary

| Sev | # | Findings |
|---|---|---|
| **Critical** | 0 | — (no panic reachable from the daemon singleton on routine untrusted input that isn't guarded; the worst, `daemon.rs:1226`, is effectively-infallible `to_value`) |
| **High** | 3 | §1.1 dangerous untrusted-input/daemon unwraps · §2.1 lint ceiling at `clippy::all` (no pedantic/unwrap_used/missing_docs) · §4.1 195 cli_* handlers duplicate prelude |
| **Medium** | 7 | §1.2 ~424 prod expect · §3.1 knowledge.rs split · §4.2 JSON boilerplate ×61 · §4.4 370 format!-errors · §5.1 73 dead_code (bare ones) · §5.2 stringly-typed generator kinds · §6.1 244 eprintln |
| **Low** | 6 | §1.3 partial thiserror · §2.2 ad-hoc inner lints · §3.2 (accept big files) · §5.3 bool params · §5.4/5.5 clone/pub · §6.2 stringly errors |

**Biggest lever toward elite**: lift the lint *ceiling* — move `unwrap_used`/`expect_used`/`missing_docs`/`indexing_slicing` into `[workspace.lints]` and ratchet `deny` outward from the already-clean CEG. It is cheap (only ~124 prod unwraps), it converts the "auditable" claim from aspiration to enforced invariant, and it permanently prevents regression of the exact debt this review measured.
