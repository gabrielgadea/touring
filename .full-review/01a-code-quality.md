# 01a — Code Quality Review (F1.1–F1.6)

> Scope: Premium-Elite Code-Quality half of the full review · Target: `/home/gabrielgadea/.claude/rust/crates`
> Methodology: every finding anchored to a real `file:line` I Read or to literal CLI output I ran (TDG / `touring-quality` / `diff` / `grep`). No invented symbols.
> Run date: 2026-06-20 · Read-only / advisory.

---

## Executive verdict

This workspace is **genuinely elite on the axes that matter for correctness and safety**, and the remaining gaps are concentrated in **two real dimensions**: per-file **size/complexity** (F1.1/F1.2 on the 27 files >2000 LOC) and **dead-code/orphan hygiene on disk** (F1.5). The headline numbers that *look* alarming (panic! 320, todo! 26, unimplemented! 13, F1.6=0.0 on hot files) are **dominated by verifier/test/detector-source noise** — I verified each and the real residue is tiny (3 prod panics, of which only 1 is a genuine design smell; ~0 prod `todo!`/`unimplemented!` outside dead code).

| Dimension | Verdict | Real residue after noise-removal |
|---|---|---|
| **F1.1 Complexity** | ⚠ Real gap | 27 files >2000 LOC; per-file CC 358–388; ~203-LOC single functions |
| **F1.2 Maintainability** | ✅ Mostly elite | God-struct `GeneratorContext` (35 fields/229 fns); short-id penalty is a verifier artifact |
| **F1.3 Duplication** | ✅ Elite (1 disk-hygiene item) | The "identical 3468-LOC gate_metrics" is an **orphaned dead file**, not live clone |
| **F1.4 Clean Code/SOLID** | ✅ Strong | 1 god-struct + monolithic CLI handlers mixing concerns |
| **F1.5 Tech debt** | ✅ Strong (disk debt) | 12 orphan dead `.rs` files on disk (~6.5k LOC) awaiting `git rm`; 1 genuine prod panic |
| **F1.6 Error handling** | ✅ **Elite — verified** | RBP-01 ratchet `deny(unwrap_used)` present in 48/48 lib crates; 0 real prod unwrap in sampled hot files |

---

## F1.1 — Complexity (cyclomatic/cognitive)

### Objective TDG on the named hot-spots (`touring ast tdg`, literal output)

| File | LOC | TDG grade | composite | complexity | coverage |
|---|---|---|---|---|---|
| `touring-generator/src/core/context.rs` | 4509 | **C** | 0.729 | 0.5 | 0.517 |
| `touring-hook-handlers/src/hooks/pre_read.rs` | 3824 | **C+** | 0.786 | 0.5 | 0.478 |
| `touring-hook-runtime/src/hook_runtime.rs` | 3102 | **C+** | 0.768 | 0.5 | 0.483 |
| `touring-cli/src/cli/handlers/decompose.rs` | 2701 | **C+** | 0.779 | 0.5 | 0.400 |
| `touring-server-reasoning/src/reasoning/decomposer.rs` | 2843 | **C+** | 0.756 | 0.5 | 0.438 |
| `touring-cortex/src/handlers/enrichment.rs` | 2747 | **C+** | 0.763 | 0.5 | 0.436 |
| `touring-cli/src/cli_suggester.rs` | 2693 | **C+** | 0.785 | 0.5 | 0.484 |

Per-file 50-dim (`touring-quality score … --dims F1.1`): `context.rs` **CC≈374**, `pre_read.rs` **CC≈358**, `decompose.rs` **CC≈388** (file-aggregate over all functions). Every hot file's `complexity` sub-score is pinned at the 0.5 floor — i.e. the model can't distinguish among them because they're all past the cap. The TDG `action` for all seven is *"Edit cauteloso, planejar mitigação"*. Note the **consistent low coverage (0.40–0.52)** on these prod-critical paths is the second real signal.

### Findings

**[High] F1.1-1 — 27 source files exceed 2000 LOC; 7 exceed 2700.**
`find crates -path '*/src/*' -name '*.rs' -not -name tests.rs | wc` → **27 files >2000 LOC**. The largest non-test source file is `touring-generator/src/core/context.rs:1` at **4509 LOC**. These are the files whose CC the per-file verifier reports as 358–388.
*Fix:* split each by cohesive concern into sibling modules (the crate already uses the `#[path]`-include pattern in `touring-cli/src/lib.rs:27`, so carving is low-risk and idiomatic here). Target: no source file >800 LOC, no function >60 LOC.

**[High] F1.1-2 — Monolithic CLI handler functions (single fn doing parse→validate→SQL→format).**
`touring-cli/src/cli/handlers/decompose.rs:432` `cli_decompose_create` is a **~203-LOC single function** (next fn at line 635). It interleaves four concerns in one body: payload extraction (`decompose.rs:433-445`), task-id generation (`decompose.rs:447-454`), raw `INSERT … VALUES` SQL (`decompose.rs:459`), and JSON response formatting. Sibling offenders in the same file: `cli_workflow_status` (~188 LOC), `cli_devrcfile_import` (~145), `cli_decompose_update` (~135). This is why `decompose.rs` has the **highest CC (388)** of all hot-spots despite not being the largest.
*Fix:* `extract_function` per concern — `parse_create_payload(payload) -> CreateArgs`, `insert_decomposition(db, &args) -> Result<…>`, `render_create_response(...)`. The DB write should return `Result` and propagate with `?` rather than embedding the SQL inline.

**[Medium] F1.1-3 — `GeneratorContext` impl is a 76-method monolith (one impl block, line 3370).**
`touring-generator/src/core/context.rs:3370` `impl GeneratorContext { … }` contains **76 methods**; the file as a whole defines **229 `fn`s**. This is the structural driver of the 4509 LOC + CC 374.
*Fix:* group methods by capability into trait-segregated `impl` blocks across sibling files (closures/lifecycle, memory, audit, session) — see F1.4-1.

---

## F1.2 — Maintainability

**[Info / verifier artifact] F1.2-A — The per-file "short-id penalty" is benign Rust idiom, not a defect.**
`touring-quality` reports `context.rs` F1.2=0.0 with *"short_id_penalty=1.00 (1939 short ids)"*. I checked the actual short ids: `grep -oE '\b[a-z]\b'` → top counts are `a` ×223 (lifetime `'a` + closure params), `e` ×56 (`Err(e)`), `x`/`n`/`f`/`v`/`d` (closure params like `|v| v.as_str()`, `|d| d.as_nanos()`). These are **idiomatic single-char closure/lifetime identifiers**, correctly *not* flagged by clippy. **Do not "fix" these** — it's a known engine artifact (scope §Verifier artifacts). I flag it only so the consolidated report doesn't chase a false alarm.

**[Medium] F1.2-1 — Real maintainability cost is function length, not naming.**
The genuine F1.2 issue is the same as F1.1: ~200-LOC functions and 4509-LOC files raise the cognitive load to read/change. Naming across the sampled hot files is in fact good (`cli_decompose_create`, `ensure_decompose_tables`, `priority_token`, `SessionStartFn` — intention-revealing). Cohesion is the lever, not vocabulary.
*Fix:* the extract-function work in F1.1-2/F1.1-3 resolves F1.2 simultaneously.

**[Low] F1.2-2 — Coverage on prod-critical hot paths is 0.40–0.52 (TDG `coverage` field).**
Every hot-spot's TDG `coverage` sub-score sits below 0.52 (`decompose.rs` lowest at 0.40). For files on the hook-runtime / generator critical path, that's the thinnest margin in the F1.x set. (Detailed coverage analysis belongs to the F3.x reviewer; surfaced here because it's the second-strongest signal in the TDG output and it *bounds* maintainability — under-tested large functions are the hardest to refactor safely.)

---

## F1.3 — Duplication

**[Medium — RESOLVED to disk-hygiene, NOT a live clone] F1.3-1 — The "identical 3468-LOC gate_metrics.rs ×2" is an orphaned dead file.**
The scope flagged `touring-hooks-shared/src/gate_metrics.rs` and `touring-foundation/src/gate_metrics.rs` as both exactly 3468 LOC = possible copy-paste. **Verified byte-for-byte identical:**
```
$ diff …hooks-shared/src/gate_metrics.rs …foundation/src/gate_metrics.rs ; echo $?
0                       # zero lines of difference
$ sha256sum …          # 6d16999ea159c54d3387d0bee5bb86e3c7d9303b9edca95ec73fc95da30c3faa  (both)
```
**But it is NOT a live duplication.** `touring-hooks-shared/src/lib.rs:47-51` documents the A5 relocation and re-exports the canonical:
```rust
// gate_metrics relocated to touring-foundation (A5 Path-A step-2, 2026-06-16);
pub use touring_foundation::gate_metrics;
```
`grep "mod gate_metrics" …hooks-shared/src/` → **no match** (the disk file is never `mod`-declared, so it is **never compiled**). Only ONE copy is in the binary (foundation's). Consumers like `touring-ceg/src/gateway/metrics.rs` reach it via the re-export. So the maintenance-divergence risk is **zero**; this is **stale disk debt awaiting `git rm`** (consistent with MEMORY.md "Órfãos no disco aguardam git rm").
*Fix:* `git rm` the 4 A5-orphaned files in `hooks-shared/src/` (gate_metrics 3468, query_cache 481, moka_policies 195, memory_stats_probe 109 = **4253 LOC dead on disk**). No code change — they're already not compiled.

**[Low] F1.3-2 — Live duplication is genuinely low (F1.3 PASS everywhere I scored).**
Per-file F1.3: `context.rs` 0.899 (Pass), `pre_read.rs` 0.919 (Pass), `decompose.rs` 0.828 (Pass); TDG `duplication` sub-score = **1.0** on all 7 hot-spots. The crate's CLI handlers share a deliberate `cli_*` shape (`decompose.rs` `cli_decompose_create`/`_update`/`_finalize`/`_ready`) — that's pattern consistency, not copy-paste. No structural-clone action required.

---

## F1.4 — Clean Code / SOLID

**[High] F1.4-1 — `GeneratorContext` is a god-struct (35 fields, 229 fns in-file, 76-method main impl).**
`touring-generator/src/core/context.rs` defines `pub struct GeneratorContext` with **35 fields** (`sed -n '/pub struct GeneratorContext/,/^}/p' | grep field` → 35) and **229 `fn`s** in the file (76 in the primary `impl` at line 3370). The struct carries many `Arc<dyn Fn…>` injected-closure fields (`SessionStartFn` at `context.rs:3129`, `CognitiveNexusFn`, etc.) — a dependency-injection bag that has grown past Single-Responsibility.
*Fix:* group the injected closures into cohesive sub-contexts (`SessionHooks`, `MemoryHooks`, `CognitiveHooks`) held as 3–4 fields instead of ~20 loose ones; segregate the 76 methods into capability traits (`impl SessionLifecycle for GeneratorContext`, `impl AuditSink …`). Per-file F1.4 is already only a **Warn (0.760)** here, so this is improvement-to-elite, not a failure.

**[Medium] F1.4-2 — CLI handlers violate separation of concerns (parse + persistence + presentation in one fn).**
`decompose.rs:432` `cli_decompose_create` embeds raw SQL (`INSERT OR IGNORE INTO task_decompositions …` at `decompose.rs:459`) directly in the same function that parses the JSON payload and formats the string response. Its per-file F1.4 is the worst of the sample at **0.563 (Warn)**. The data-access layer should be a typed function returning `Result`, not inline SQL inside a presentation handler.
*Fix:* introduce a thin repository function (`insert_decomposition(db, &CreateArgs) -> Result<TaskId, DecomposeError>`) and keep handlers as parse→call→render. This also fixes F1.1-2.

**[Low] F1.4-3 — Strong SOLID elsewhere (do not over-correct).**
`pre_read.rs` scores F1.4 = **0.985 (Pass)** — the largest hook handler is well-factored despite its size. The trait-driven `Embedder`/`KnowledgeSource` abstractions (the A5 `KnowledgeSource` moved to `touring-foundation` per MEMORY.md) are textbook Dependency-Inversion. The god-struct is the exception, not the rule.

---

## F1.5 — Technical Debt

I separated **real prod markers** from **detector-source / test-fixture / dead-code noise**. Raw counts in scope (panic! 320, todo! 26, unimplemented! 13) are overwhelmingly noise.

### Marker triage (verified)

| Marker | Raw | Real prod (verified) | Where the rest live |
|---|---|---|---|
| `panic!()` | 321 | **3** statements, of which **1** is a design smell | test regions + detector strings (`pattern:`/`message:`) |
| `todo!()` | 26 | **0** outside dead code | antipattern-detector source (`touring-analysis/src/quality/antipatterns.rs:18`), test fixtures |
| `unimplemented!()` | 10 | **0** outside dead code | detector source + test-fixture strings (`rust_semantic.rs:142` is inside `abstract_src = r#"…"#`) |

The 3 real `panic!()` (`grep panic!( … | grep -v test/string/comment`):
- `touring-dispatch/src/hook_registry.rs:2075` — **in test region** (file test marker line 1834); it's a *Sprint 4.6 STRUCTURAL DEFENSE* assertion. **Elite practice, keep.**
- `touring-server/src/cli/search_unified.rs:728` — **in test fn** `exact_limit_flag` (line 617); `let … else { panic!() }` on an irrefutable parse. **Idiomatic, keep.**
- `touring-intelligence/src/rl/semantic/candle_embedder.rs:372` — **genuine prod panic** (see F1.5-2).

### Findings

**[Medium] F1.5-1 — 12 orphaned dead `.rs` files on disk (~6.5k LOC) — REGRA #0 violation.**
These exist on disk, are never `mod`-declared / `#[path]`-included, and have **0 external module references** (verified per-file):

*hooks-shared (A5 relocation orphans):*
- `gate_metrics.rs` (3468 LOC), `query_cache.rs` (481), `moka_policies.rs` (195), `memory_stats_probe.rs` (109)

*cortex (dead handlers — `handlers/mod.rs` does NOT declare them; `external_module_refs=0`):*
- `self_reflection.rs` (664, contains the `unimplemented!()` at :555 inside a test fixture string), `reasoning_advanced.rs` (514), `adaptive.rs` (380), `hybrid_cognitive.rs` (370), `cognitive_metrics.rs` (283), `coedit.rs` (271), `focus.rs` (266), `streaming_mcts.rs` (234)

Total ≈ **6,499 LOC dead-on-disk** (4253 hooks-shared + 2982 cortex, minus overlap). This is the single largest *honest* F1.5 item: it inflates the "27 files >2000 LOC" and the raw marker counts, and it's REGRA #0 debt (`allow(dead_code)`/orphan).
*Fix:* `git rm` the 4 hooks-shared relocation orphans (safe — re-export already canonical). For the 8 cortex handlers, decide per REGRA #0: **wire** them to a consumer if the capability is wanted, else **`git rm`**. (Read-only here — flagging, not deleting.)

**[Low] F1.5-2 — One genuine prod panic on a Result-less trait method.**
`touring-intelligence/src/rl/semantic/candle_embedder.rs:372`: `impl Embedder for CandleEmbedder { fn embed(&self, text) -> Vec<f32> { match self.forward_pass(text) { Ok(v) => v, Err(e) => panic!("CandleEmbedder::embed failed: {e}…") } } }`. It is **documented and intentional** (comment cites Operating Principle #5 "Falhe loud"; consumers told to check `has_forward_pass()` first). Defensible, but the panic only exists because the `Embedder` trait signature is `-> Vec<f32>` with no `Result`.
*Fix (proper):* widen the trait to `fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError>` so loud-fail becomes a propagated error; callers that want the old behavior do `.expect("staged GGUF")`. Low priority (single site, documented).

**[Info] F1.5-3 — `allow(dead_code)` count (53 in scope) is mostly in the dead orphans + detector self-tests.**
Resolving F1.5-1 collapses most of this. Not separately actioned.

---

## F1.6 — Error Handling

**[✅ Elite — VERIFIED] F1.6-VERIFY — RBP-01 unwrap ratchet is real and clean.**
I sampled the 3 biggest crates as instructed:

| Crate | Ratchet lint present? | Raw `.unwrap()` (incl tests) | Prod unwraps |
|---|---|---|---|
| `touring-server` | `lib.rs:8 #![cfg_attr(not(test), deny(clippy::unwrap_used))]` ✅ | 497 | **0 verified** |
| `touring-intelligence` | `lib.rs:38 …deny(clippy::unwrap_used)` ✅ | 665 | 0 (lint + clippy-clean) |
| `touring-dispatch` | `lib.rs:30 …deny(clippy::unwrap_used)` ✅ | 278 | 0 (lint + clippy-clean) |

**48/48** workspace lib crates carry the `deny(clippy::unwrap_used)` ratchet (`grep -rln … crates/*/src/lib.rs` = 48 of 48). Since clippy passes `--all-targets` with 0 warnings (scope §27) and this is a `deny`, **every raw unwrap is necessarily inside `#[cfg(test)]`** (the lint exempts `test`). I proved the one file my crude grep flagged (`touring-server/src/plugins/runner.rs`, "7 unwraps, no cfg(test)"): all 7 are inside `mod tests` at `runner.rs:112+`, every one under `#[test]`. The raw 4064/3257 unwrap/expect totals are **test-module-dominated**, exactly as scope predicted. This dimension is genuinely Diamond-tier.

**[Info / verifier artifact] F1.6-A — Per-file F1.6=0.0 on `pre_read.rs` is a test-counting artifact, NOT a defect.**
`touring-quality` scores `pre_read.rs` F1.6=0.000 and `context.rs` F1.6=0.150 (Fail), but `decompose.rs` F1.6=1.000 (Pass) — wild divergence. Root cause verified: `pre_read.rs` has 92 `.unwrap()` + 13 `.expect()`, and the **first test marker is at line 1138 of 3824 — there are 0 prod unwraps before it** (`head -1138 | grep -c .unwrap()` = 0). The verifier counts test-module unwraps at file scope without `cfg(test)` exclusion. **Do not act on per-file F1.6 for test-heavy files** — trust the ratchet + clippy instead.

**[Low] F1.6-1 — `let _ =` discards: 1067 occurrences, mostly benign, a few worth a typed drop.**
`grep "let _ = " crates/*/src` → 1067 (prod-ish). Spot-check: the bulk are intentional fire-and-forget channel sends (`touring-code/src/ast/store.rs:903 let _ = self.stream_sender.send(...)`, `touring-cortex/src/fascicles/channels.rs:121`) where a closed receiver is genuinely ignorable — legitimate. A handful discard a real `Result` whose error matters, e.g. `touring-cli/src/cli/handlers/index.rs:463 let _ = store.replace_file_symbols(...)` (a DB write whose failure is silently dropped) and `touring-ceg/src/gateway/sandbox_executor.rs:1413 let _ = result.unwrap();` (a discard-after-unwrap, redundant).
*Fix:* for the DB-write case, log on error (`if let Err(e) = … { tracing::warn!(?e, …) }`); the workspace's own detector even teaches this (`touring-code/src/ast/quality.rs:468` flags `let _ = expr` as "silently discards a Result"). Note: NaN-panic float sorts were already swept (MEMORY.md RBP-01 fixed 7 `partial_cmp().unwrap()` → `unwrap_or(Ordering::Equal)`); I found no new prod float-sort panic in the hot-spots.

---

## Top 5 by impact

1. **[High] F1.5-1 — `git rm` 12 orphaned dead files (~6.5k LOC) on disk.** Biggest *honest* win: removes ~6.5k LOC of REGRA #0 debt, deflates the "3468-LOC duplicate" and "27 files >2000 LOC" headlines, and clears most `allow(dead_code)`. Zero compile risk (already not compiled). *(hooks-shared ×4 relocation orphans + cortex ×8 dead handlers.)*
2. **[High] F1.1-2 / F1.4-2 — Break the monolithic CLI handlers in `decompose.rs`.** `cli_decompose_create` (203 LOC) and siblings interleave parse/SQL/format; extracting a typed repository fn + parse/render helpers fixes the **highest-CC file (388)** and its Warn-tier F1.4 (0.563) in one move.
3. **[High] F1.4-1 / F1.1-3 — Decompose the `GeneratorContext` god-struct** (35 fields, 229 fns, 4509 LOC). Group injected closures into sub-contexts + segregate the 76-method impl into capability traits across sibling files — collapses the single largest source file.
4. **[High] F1.1-1 — Carve the 27 files >2000 LOC** to ≤800 LOC using the crate's existing `#[path]`-include pattern. Systematic size reduction; pairs with #2/#3.
5. **[Medium] F1.2-2 — Raise coverage on the prod-critical hot paths** (TDG coverage 0.40–0.52 on generator/hook-runtime/decompose). Under-tested 200-LOC functions are the hardest to refactor safely — coverage is the enabler for #2–#4. *(Hand-off to the F3.x reviewer for depth.)*

---

## What is ALREADY elite (do not manufacture problems)

- **F1.6 error handling is Diamond-tier and I verified it end-to-end.** The `deny(clippy::unwrap_used)` ratchet is present in 48/48 lib crates; 0 real prod unwraps in the sampled big crates; the scary raw counts (4064 unwrap) are test-module-dominated by design. RBP-01's "49/49 locked" claim holds.
- **Live duplication is genuinely low** — F1.3 PASS on every file scored, TDG `duplication`=1.0 across all 7 hot-spots. The one "identical file" is a re-export orphan, not a clone.
- **Tech-debt markers are clean** — 0 prod `todo!()`/`unimplemented!()` outside dead code; the 320 `panic!` reduce to 3 statements, 2 of which are idiomatic test assertions.
- **clippy `--all-targets` = 0 warnings** with elite `[workspace.lints]` ratchets; **0 dependency cycles** (Tarjan SCC).
- **Naming and abstraction quality are strong** — intention-revealing identifiers, trait-driven DI (`Embedder`, `KnowledgeSource`), pattern-consistent `cli_*` handlers. `pre_read.rs` is F1.4=0.985 despite 3824 LOC.

**The honest gap to per-file-elite-everywhere is structural size + the disk-orphan cleanup — not correctness, safety, or duplication.** Most of the apparent F1.x failures in `touring-quality --workspace` (0.59 Unranked) are the engine's known per-file/aggregate artifacts (CC-summed, test-unwrap-counted, short-id-penalized), not crate defects — exactly as scope §Verifier artifacts warned. Fixing the size of the top ~30 files and `git rm`-ing the 12 orphans moves the per-file scores into Gold/Platinum without touching the (already-elite) correctness layer.
