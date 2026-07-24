# Touring Workspace v28 Analysis — Key Findings

## Session: 2026-03-28 | Task: Full workspace deep analysis + exponential enhancement

## Critical Findings

### P0 — Pre-existing Issues (NOT from our changes)
- touring-hooks: 592 unwrap() — hot paths (knowledge.rs:116, wiring.rs:100, shadow_v2.rs:77, post_edit:36, pre_edit:26, pre_read:22)
- touring-hooks: rusqlite API errors — pre-existing, blocking compilation of touring-server + touring-cortex
- touring-server: 20 Mutex + 9 RwLock — HIGHEST in workspace, blocking async
- touring-python: src/lib.rs EMPTY — PyO3 bindings not wired

### P1 — Issues Found & Status
1. **SIMD features NOT activated** → FIXED: touring-cortex (cortex-integration), touring-server + touring-learning (learning-integration)
2. **Cargo.toml duplicates** → FIXED: dev-deps deduplicated in touring-cortex + touring-server
3. **AnnIndex custom cosine** → ALREADY FIXED (v28.3.1): CosineComputer SIMD in use
4. **CoEditPredictor not wired** → FIXED: predict_coedit_files() added to GraphService
5. **blast_radius BUG** → ALREADY FIXED (S2): bridge.rs:411-420 computes from dependents_of()

### P1 — Known Issues (Not Fixed Due to Pre-existing Errors)
- touring-hooks rusqlite errors prevent touring-server/touring-cortex from compiling
- 592 unwrap() in touring-hooks remain — identified but not audited due to compilation blocking

### P2 — Insights
- moka/dashmap/lru: 3 different caches in touring-server — KEEP SEPARATE (different concurrency semantics)
- touring-simd::DriftDetector: ZERO cross-crate usage despite being statistically rigorous
- touring-wasm AsyncInferletPool: async/sync mismatch with H83 TypedEvaluateHandler
- touring-core embedding.rs: STUB (gpu-embeddings feature dead code)
- touring-rules (712 LOC): candidate for inlining into touring-hooks

## Test Coverage
- 9 compilable crates: ~1,563 lib + doc tests passing
- touring-server + touring-cortex + touring-hooks: pre-existing errors block compilation

## Changes Applied
- touring-cortex/Cargo.toml: touring-simd features = ["cortex-integration"]
- touring-server/Cargo.toml: touring-simd features = ["learning-integration"]  
- touring-learning/Cargo.toml: touring-simd features = ["learning-integration"]
- touring-cortex + touring-server: dev-deps duplicate entries removed
- graph_service.rs: CoEditPredictor::predict_coedit_files() added
- session_predictor.rs + semantic_graph.rs: warm_cache TODO comments (ARCH-2/ARCH-3)
