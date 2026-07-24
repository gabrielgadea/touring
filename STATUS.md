# STATUS — Touring Workspace (v4.9.0 — 2026-04-24)

## WIRING ENHANCEMENT WAVE (v4.9.0) ✅ — F1+F2+F3+F4 COMPLETE

### F1: touring wiring impact <symbol> [--depth N] ✅
- CLI: `./target/release/touring wiring impact HookRuntime --depth 2` → 68 direct consumers, max depth 1
- Code: compute_impact + walk_consumers_bfs in wiring.rs, cli_wiring_impact in cli_handlers.rs
- E2E: 5 tests — test_cli_wiring_impact_* — ALL PASS

### F2: touring wiring cycles [--min-depth N] [--format json|text] ✅
- CLI: `./target/release/touring wiring cycles` → 4 cycles (cortex x2, cognitive x1, generator x1)
- Code: find_all_cycles with Tarjan's SCC in wiring.rs, cli_wiring_cycles in cli_handlers.rs
- E2E: 5 tests — test_cli_wiring_cycles_* — ALL PASS

### F3: ACP Protocol Layer (feature: acp-protocol) ✅
- Created: protocol/acp.rs (Message, Response, ResponseError, Capabilities)
- Feature flag: acp-protocol in touring-hooks/Cargo.toml
- 7 unit tests PASS

### F4: HyperGraph Wrapper (crates/touring-hooks/src/wiring/hypergraph.rs) ✅
- HyperGraph<N> using petgraph artificial node pattern
- FeatureGateHyperedge + MultiImportHyperedge types
- 6 unit tests PASS

### E2E Tests ✅
- 10 new E2E tests in cli_handlers_e2e.rs: wiring impact (5) + wiring cycles (5)
- All 10 pass (test_cli_wiring_{impact,cycles}*)

### F0 Health Gate
- cargo check --workspace: 0 errors
- touring doctor -j: daemon_socket status = ok

### Build
- cargo build --release -p touring-server: SUCCESS (4m18s)

## P2 — tree-sitter WASM + Incremental Parsing ✅

**F.1 (wasm feature)** — DONE:
- Workspace `tree-sitter = { version = "0.24", features = ["wasm"] }` — no version change, no links conflict

**C (benchmark + validation)** — DONE:
- Benchmark results (criterion --quick, 500-line synthetic Rust):
  - Full reparse: **483µs**
  - Incremental parse (cached tree, single-line edit): **70µs**
  - **Speedup: 6.9×** for cached-tree edits (exceeds 30% threshold)
  - Cache lookup: **108ns** (essentially free)
- `IncrementalParser::parse_incremental()` exists and correctly uses `parser.parse(source, Some(&old_tree))` — O(changed_range) not O(file)
- `IncrementalParser` LRU cache (moka, 128 trees) already in place
- **Integration complete**: `shared::reindex::reindex_file_with_old()` now uses pipeline in two tiers:
  1. `process_edit` — O(edit_region) when cached tree + old content available (hot path after first edit)
  2. `process_file` — O(file) on cache miss or no old content (populates tree cache)
  - New `extract_symbols_via_pipeline()` helper reduces CC from 48→11 for the extraction logic
  - `compute_edit_offsets()` diffs old/new content to derive byte offsets for `process_edit`
  - CC of `reindex_file_with_old`: 31 (down from 48 with inline chain)
  - 3150 lib + 90 E2E tests PASS

## P4 — Cross-Audit Follow-ups ✅ (2026-04-25)

**P4.3 (parse_incremental in post_edit)** — DONE:
- `post_edit.rs`: `old_source` captured at line 134 threaded through `phase1_tracking` → `reindex_file` → `reindex_file_with_old`
- `post_write.rs`: delegates to `reindex_file_with_old` (no old_content available — first edit populates cache)
- `touring-ast/incremental_pipeline.rs`: new `cached_tree()` + `has_cached_tree()` on `IncrementalPipeline`/`SharedPipeline`

**P4.3/P4.4 (parse_incremental integration)** — DONE:
- Tree cache population: every `reindex_file` call now populates cache via `process_file` (first-edit path) or `process_edit` (subsequent-edit path)
- Hot path: edit #2+ of same file → `process_edit` with cached tree → 6.9× speedup
- `old_source` from `post_edit` is passed through 3 function layers to enable the incremental path

## DOCUMENTATION UPDATED (v4.9.0)
- SKILL.md: v4.9.0 header + F1/F2 in TIER 3 + F3/F4 section added
- touring-cli-commands.md: v4.9 + F1/F2 spec in Wiring Intelligence table
- docs/2026-04-24-touring-wiring-enhancement-plan.md: status → IMPLEMENTED

## P2 — ACO/Metacognitive (PENDING)
7. **PipelineContext hardcoded values** — cortex_dispatcher.rs:256-268
   - current_threshold=64, vector_dim=None, memory_bound=false — all hardcoded
   - Should come from hook_runtime state

8. **MutableGeneratorGraph integration_score=0.024** — ACO graph not wired to hook_runtime

## IN PROGRESS
- Fix 1: touring-rkyv ipc.rs needless lifetime — DONE
- Fix 2: Engineer dispatched for FtrlLayer wiring
- Fix 3: Engineer dispatched for bandit clone fix
- Fix 4: Engineer dispatched for actor_critic clones + cache

### Cross-Audit (2026-04-25) ✅
- **HyperGraph tests**: 6/6 PASS (hypergraph module exported via `pub mod hypergraph` in wiring.rs)
- **ACP tests**: 7/7 PASS (with `--features acp-protocol`)
- **E2E F1+F2**: 10/10 PASS (test_cli_wiring_{impact,cycles}*)
- **E2E full suite**: 84/84 PASS (cli_handlers_e2e.rs)
- **Dead code**: `COMMIT_TIMEOUT`/`ROLLBACK_TIMEOUT` marked `#[allow(dead_code)]` (used in future BEGIN phase); `evict_expired_transactions_sync` is async-wrapped version for sync call paths
- **unused_mut**: Fixed in cli_handlers_e2e.rs:1344

## P1-P4: Crate Integration Plan (2026-04-25) — PENDING GABRIEL APPROVAL

**Reference**: `docs/2026-04-25-crate-analysis-mpatch-semantic-analyzer-tree-sitter.md`
**Plan**: `docs/2026-04-25-touring-crate-implementation-plan.md`

### Summary

| Crate | Priority | Approach | Sequence |
|-------|----------|----------|----------|
| **mpatch** | P1 (HIGH) | Incremental — add as optional dep, feature `mpatch-fuzzy`, integrate in touring-generator plan_commit pipeline | Week 1-2 |
| **tree-sitter** | P2 (HIGH) | Full — enable WASM feature, benchmark incremental parsing | Week 1 |
### P3: semantic-analyzer Spike — ✅ COMPLETE (ARCHIVE RECOMMENDED)

**Spike Result** (2026-04-25): `crates/touring-semantic-spike/`
- Rust 1.88 compatibility: **CONFIRMED** ✅ (compiles)
- API complexity: **HIGH** (State requires ExtendedExpression<I> trait bound)
- AST ownership model mismatch: tree-sitter → semantic-analyzer bridge is non-trivial

**Verdict**: Recommend ARCHIVE — semantic-analyzer expects compiler-frontend-owned AST, not tree-sitter input. Integration cost is high with uncertain payoff. syn-based `RustSemanticReport` already covers syntactic analysis. Full report: `docs/2026-04-25-semantic-analyzer-spike-report.md`

### Key Risks

- **semantic-analyzer**: Rust 1.88+ required (workspace minimum: 1.75) — spike may result in ARCHIVE
- **mpatch**: O(N×M) fuzzy scan performance needs benchmarking (>30% speedup threshold)
- **tree-sitter**: Already integrated — no new dependency, just feature opt-in + benchmark

### Open Questions (Waiting for Gabriel)

1. **Workspace MSRV**: Update to 1.88 for semantic-analyzer spike success?
2. **mpatch feature name**: `mpatch-fuzzy` or `fuzzy-patch` or `ai-patch`?
3. **Incremental parsing threshold**: 30% speedup minimum — acceptable?
4. **P4 TODO/FIXME audit**: Schedule now or defer to next sprint?

### P4 Cross-Audit Follow-ups

- E2E tests for ACP protocol integration ✅ — 3 E2E tests added (93/93 suite PASS)
- E2E tests for HyperGraph wiring integration ✅ — 4 E2E tests added (94/94 suite PASS)
- ACP shim wired into daemon socket dispatch ✅ (handle_acp_request_async + detect_acp_payload)
- HyperGraph wired into wiring analysis system ✅ (pub mod hypergraph exported, petgraph-based wiring uses HyperGraph via import)
- TODO/FIXME annotation audit across codebase (DEFERRED — file_todos table needs schema migration first; grep shows ~0 active TODO/FIXME in hooks codebase, only NOTE comments documenting decisions)

## NEXT
- Run cargo clippy after all fixes
- Create E2E tests for RL pipeline
- Update touring-learning/Cargo.toml if needed
- P4 TODO/FIXME DEFERRED — active TODOs are decision NOTES (not debt); file_todos table needs schema migration first