# Touring Crates Exponential Upgrade — Implementation Plan

> **STATUS: COMPLETE** — All phases implemented, 1,336 tests passing, cross-audit PASS (5/5 gates). Date: 2026-03-25.

> ~~For agentic workers: REQUIRED SUB-SKILL~~ (plan fully executed)

**Goal:** Upgrade touring-learning, touring-ast, and touring-cognitive to best-practice Rust, close the cognitive loop with touring-hooks, and unlock exponential synergies between all crates. **ACHIEVED.**

**Architecture:** 26 tasks organized in 6 phases. Phase 1 (Quick Wins) and Phase 2 (Error/Persistence) can start immediately. Phase 3 (Cognitive Integration) is the critical path that connects the dormant CognitiveRuntime to HookRuntime. Phase 4 (Medium-Term) adds higher-order predictions, parallel GoT, and observability. Phase 5 (Deep Improvements) adds real embeddings, streaming MCTS, and real-time intelligence pipeline. Phase 6 (Cross-Crate Integration Tests) validates the full loop end-to-end.

**Tech Stack:** Rust 1.75+, tree-sitter, petgraph, rkyv, tokio, rusqlite, touring-simd, touring-core

**Workspace root:** `/home/gabrielgadea/.claude/rust/`

**Build & test commands:**
```bash
# Build all crates
cargo build --workspace

# Test single crate
cargo test -p touring-cognitive

# Test with features
cargo test -p touring-learning --features burn-transformer,ast-features,hnsw-working-memory

# Clippy (deny all — workspace lint)
cargo clippy --workspace --all-targets

# Bench
cargo bench -p touring-cognitive --bench semantic_graph
```

---

## File Structure Map

### Modified Files (by phase)

| Phase | Crate | File | Change |
|-------|-------|------|--------|
| 1 | touring-cognitive | `Cargo.toml` | Add `touring-simd` dependency |
| 1 | touring-cognitive | `src/semantic_graph.rs` | Replace manual cosine with `touring_simd::CosineComputer::cosine()` |
| 1 | touring-learning | `src/clustering/cosine.rs` | Replace manual cosine with `touring_simd::CosineComputer::cosine()` |
| 1 | touring-learning | `src/memory/recall.rs` | Replace manual cosine with `touring_simd::CosineComputer::cosine()` |
| 1 | touring-learning | `Cargo.toml` | Already has `touring-simd` — verify it's used |
| 1 | touring-cognitive | `src/focus_cache.rs` | Replace `Mutex` with `RwLock` |
| 2 | touring-cognitive | `Cargo.toml` | Add `thiserror` dependency |
| 2 | touring-cognitive | `src/error.rs` | **Create** — `CognitiveError` enum with thiserror |
| 2 | touring-cognitive | `src/lib.rs` | Add `pub mod error;` export |
| 2 | touring-cognitive | `src/persistence.rs` | Replace serde_json with rkyv |
| 2 | touring-cognitive | `src/semantic_graph.rs` | Return `CognitiveError` instead of `String` |
| 2 | touring-cognitive | `src/bridge.rs` | Return `CognitiveError` instead of `String` |
| 3 | touring-hooks | `Cargo.toml` | Add `touring-cognitive` dependency |
| 3 | touring-hooks | `src/cognitive_bridge.rs` | **Create** — `impl KnowledgeSource for FileKnowledgeDB` |
| 3 | touring-hooks | `src/lib.rs` | Add `pub mod cognitive_bridge;` |
| 3 | touring-hooks | `src/runtime.rs` | Add `CognitiveRuntime` field, wire to pre/post hooks |
| 4 | touring-cognitive | `src/session_predictor.rs` | Add bigram/trigram higher-order transitions |
| 4 | touring-cognitive | `src/got.rs` | Refactor to use `tokio::spawn` for parallel children |
| 4 | touring-ast | `src/watcher.rs` | Add integration tests |
| 4 | touring-hooks | `src/metrics.rs` | Add prometheus-style counters |
| 4 | touring-cognitive | `src/semantic_graph.rs` | Add graph compaction |
| 5 | touring-cognitive | `src/persistence.rs` | Already rkyv from Phase 2 — add mmap |
| 5 | touring-cognitive | `src/mcts.rs` | Add streaming search (background task) |
| 5 | touring-ast | `src/incremental_pipeline.rs` | Add dependency-graph invalidation |
| 6 | touring-hooks | `tests/cognitive_integration.rs` | **Create** — end-to-end integration test |

---

## Phase 1 — Quick Wins (Q1, Q5, Q7)

### Task 1: Replace manual cosine in touring-cognitive with touring-simd

**Files:**
- Modify: `crates/touring-cognitive/Cargo.toml`
- Modify: `crates/touring-cognitive/src/semantic_graph.rs`
- Test: `cargo test -p touring-cognitive`

- [ ] **Step 1: Add touring-simd dependency to touring-cognitive**

In `crates/touring-cognitive/Cargo.toml`, add under `[dependencies]`:

```toml
touring-simd = { path = "../touring-simd" }
```

- [ ] **Step 2: Find the manual cosine_similarity function in semantic_graph.rs**

```bash
grep -n "cosine_similarity\|fn cosine" crates/touring-cognitive/src/semantic_graph.rs
```

- [ ] **Step 3: Replace manual cosine with touring_simd::CosineComputer**

Replace the manual `cosine_similarity` function body with:

```rust
use touring_simd::similarity::cosine::CosineComputer;
use touring_simd::similarity::traits::CosineSimilarity;

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let computer = CosineComputer::new();
    computer.cosine(a, b)
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p touring-cognitive
```
Expected: ALL PASS

- [ ] **Step 5: Run clippy**

```bash
cargo clippy -p touring-cognitive --all-targets
```
Expected: 0 errors

- [ ] **Step 6: Commit**

```bash
git add crates/touring-cognitive/Cargo.toml crates/touring-cognitive/src/semantic_graph.rs
git commit -m "perf(cognitive): replace manual cosine with touring-simd CosineComputer"
```

---

### Task 2: Replace manual cosine in touring-learning clustering

**Files:**
- Modify: `crates/touring-learning/src/clustering/cosine.rs`
- Test: `cargo test -p touring-learning`

- [ ] **Step 1: Find the manual cosine implementation**

```bash
grep -n "cosine_similarity\|fn cosine" crates/touring-learning/src/clustering/cosine.rs
```

- [ ] **Step 2: Replace with touring_simd::CosineComputer**

touring-learning already depends on touring-simd. Replace the manual function body:

```rust
use touring_simd::similarity::cosine::CosineComputer;

// Inside wherever cosine is computed:
let computer = CosineComputer::new();
let similarity = computer.cosine(a, b);
```

Preserve the existing function signature — only change the internal implementation.

- [ ] **Step 3: Run tests**

```bash
cargo test -p touring-learning
```
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add crates/touring-learning/src/clustering/cosine.rs
git commit -m "perf(learning): replace manual cosine with touring-simd in clustering"
```

---

### Task 3: Replace manual cosine in touring-learning recall.rs

**Files:**
- Modify: `crates/touring-learning/src/memory/recall.rs`
- Test: `cargo test -p touring-learning`

- [ ] **Step 1: Find the manual cosine in recall.rs**

```bash
grep -n "cosine_similarity\|fn cosine\|dot_product" crates/touring-learning/src/memory/recall.rs
```

- [ ] **Step 2: Replace with touring_simd::CosineComputer**

Same pattern as Task 2 — replace the function body, preserve the signature.

- [ ] **Step 3: Run tests**

```bash
cargo test -p touring-learning
```
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add crates/touring-learning/src/memory/recall.rs
git commit -m "perf(learning): replace manual cosine with touring-simd in recall"
```

---

### Task 4: Replace Mutex with RwLock in FocusCache

**Files:**
- Modify: `crates/touring-cognitive/src/focus_cache.rs`
- Test: `cargo test -p touring-cognitive`

- [ ] **Step 1: Read current FocusCache implementation**

Read `crates/touring-cognitive/src/focus_cache.rs` fully. Note: it uses `Mutex<FocusCacheInner>` but the `get_or_compute` method has a double-checked pattern (release lock before compute, re-acquire after).

- [ ] **Step 2: Replace Mutex with RwLock**

```rust
use std::sync::RwLock;

pub struct FocusCache {
    inner: RwLock<FocusCacheInner>,
}
```

Update all `self.inner.lock().unwrap()` calls:
- Read-only paths (cache hit check): `self.inner.read().unwrap()`
- Write paths (cache insert, stats update): `self.inner.write().unwrap()`

The `get_or_compute` pattern becomes:
1. `read()` → check cache → if hit, clone and return
2. Drop read lock → compute value
3. `write()` → insert result → return

- [ ] **Step 3: Run tests**

```bash
cargo test -p touring-cognitive
```
Expected: ALL PASS

- [ ] **Step 4: Run bench to verify no regression**

```bash
cargo bench -p touring-cognitive --bench semantic_graph 2>&1 | head -30
```

- [ ] **Step 5: Commit**

```bash
git add crates/touring-cognitive/src/focus_cache.rs
git commit -m "perf(cognitive): replace Mutex with RwLock in FocusCache for read concurrency"
```

---

## Phase 2 — Error Handling & Persistence (Q2, Q5)

### Task 5: Create CognitiveError enum with thiserror

**Files:**
- Create: `crates/touring-cognitive/src/error.rs`
- Modify: `crates/touring-cognitive/src/lib.rs`
- Test: `cargo test -p touring-cognitive`

- [ ] **Step 1: Write the error module**

Create `crates/touring-cognitive/src/error.rs`:

```rust
//! Error types for the touring-cognitive crate.

use thiserror::Error;

/// Errors that can occur in the cognitive engine.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum CognitiveError {
    #[error("graph operation failed: {0}")]
    Graph(String),

    #[error("persistence failed: {0}")]
    Persistence(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("node not found: {0}")]
    NodeNotFound(String),

    #[error("prediction failed: {0}")]
    Prediction(String),
}

/// Convenience type alias.
pub type CognitiveResult<T> = std::result::Result<T, CognitiveError>;
```

- [ ] **Step 2: Add thiserror to Cargo.toml if not present**

Check `crates/touring-cognitive/Cargo.toml` — if `thiserror` is missing, add:

```toml
thiserror = { workspace = true }
```

- [ ] **Step 3: Export from lib.rs**

Add to `crates/touring-cognitive/src/lib.rs`:

```rust
pub mod error;
pub use error::{CognitiveError, CognitiveResult};
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p touring-cognitive
```
Expected: ALL PASS (no callers yet — this is additive)

- [ ] **Step 5: Commit**

```bash
git add crates/touring-cognitive/src/error.rs crates/touring-cognitive/src/lib.rs crates/touring-cognitive/Cargo.toml
git commit -m "feat(cognitive): add CognitiveError enum with thiserror"
```

---

### Task 6: Migrate persistence.rs from serde_json to CognitiveError

**Files:**
- Modify: `crates/touring-cognitive/src/persistence.rs`
- Test: `cargo test -p touring-cognitive`

- [ ] **Step 1: Replace Result<_, String> with CognitiveResult in persistence.rs**

Change `GraphPersistence::save` and `load` return types:

```rust
use crate::error::{CognitiveError, CognitiveResult};

pub fn save(&self, snapshot: &GraphSnapshot) -> CognitiveResult<usize> {
    // ...
    .map_err(|e| CognitiveError::Serialization(e.to_string()))?;
    // ...
    .map_err(CognitiveError::Io)?;
}

pub fn load(&self) -> CognitiveResult<Option<GraphSnapshot>> {
    // ...
    .map_err(CognitiveError::Io)?;
    // ...
    .map_err(|e| CognitiveError::Serialization(e.to_string()))?;
}
```

- [ ] **Step 2: Update callers of save/load**

```bash
grep -rn "\.save(\|\.load(" crates/touring-cognitive/src/ --include="*.rs"
```

Update each caller to handle `CognitiveError` instead of `String`.

- [ ] **Step 3: Run tests**

```bash
cargo test -p touring-cognitive
```
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add crates/touring-cognitive/src/persistence.rs crates/touring-cognitive/src/semantic_graph.rs
git commit -m "refactor(cognitive): replace Result<_,String> with CognitiveError in persistence"
```

---

### Task 7: Migrate GraphPersistence to rkyv for 5-10x speedup

**Files:**
- Modify: `crates/touring-cognitive/src/persistence.rs`
- Modify: `crates/touring-cognitive/src/semantic_graph.rs` (add rkyv derives)
- Test: `cargo test -p touring-cognitive`

- [ ] **Step 1: Add rkyv derives to MemoryNode, SemanticEdge, NodeType, EdgeType, GraphSnapshot**

In `semantic_graph.rs`, add to each struct/enum:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]  // rkyv 0.7 validation
```

Note: `serde_json::Value` in `MemoryNode.metadata` is NOT rkyv-serializable. Convert to `String` for rkyv and deserialize back on load. Or keep metadata as `String` (JSON text) in the rkyv layer.

- [ ] **Step 2: Create a rkyv-compatible snapshot struct**

In `persistence.rs`:

```rust
/// rkyv-serializable snapshot (metadata stored as JSON string).
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct RkyvSnapshot {
    pub nodes: Vec<RkyvNode>,
    pub edges: Vec<RkyvEdge>,
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct RkyvNode {
    pub id: String,
    pub label: String,
    pub node_type: u8,  // 0=Symbol, 1=File, 2=Concept, 3=Session
    pub embedding: Vec<f32>,
    pub metadata_json: String,
    pub last_accessed: f64,
    pub access_count: u64,
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct RkyvEdge {
    pub from_id: String,
    pub to_id: String,
    pub edge_type: u8,
    pub weight: f64,
    pub created_at: f64,
}
```

- [ ] **Step 3: Implement conversion between GraphSnapshot and RkyvSnapshot**

Add `impl From<&GraphSnapshot> for RkyvSnapshot` and `impl From<RkyvSnapshot> for GraphSnapshot`.

- [ ] **Step 4: Replace save/load with rkyv**

```rust
pub fn save(&self, snapshot: &GraphSnapshot) -> CognitiveResult<usize> {
    let rkyv_snap = RkyvSnapshot::from(snapshot);
    let bytes = rkyv::to_bytes::<_, 65536>(&rkyv_snap)
        .map_err(|e| CognitiveError::Serialization(e.to_string()))?;
    // Atomic write: temp file + rename
    let tmp = self.path.with_extension("rkyv.tmp");
    std::fs::write(&tmp, &bytes).map_err(CognitiveError::Io)?;
    std::fs::rename(&tmp, &self.path).map_err(CognitiveError::Io)?;
    Ok(bytes.len())
}

pub fn load(&self) -> CognitiveResult<Option<GraphSnapshot>> {
    let bytes = match std::fs::read(&self.path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(CognitiveError::Io(e)),
    };
    let archived = rkyv::check_archived_root::<RkyvSnapshot>(&bytes)
        .map_err(|e| CognitiveError::Serialization(e.to_string()))?;
    let rkyv_snap: RkyvSnapshot = archived.deserialize(&mut rkyv::Infallible)
        .map_err(|e| CognitiveError::Serialization(format!("{e:?}")))?;
    Ok(Some(GraphSnapshot::from(rkyv_snap)))
}
```

- [ ] **Step 5: Keep backward compat — try rkyv first, fallback to JSON**

In `load()`, if rkyv deserialization fails, try `serde_json::from_str` as fallback for existing JSON files. Log a warning suggesting re-save.

- [ ] **Step 6: Run tests**

```bash
cargo test -p touring-cognitive
```
Expected: ALL PASS

- [ ] **Step 7: Run bench**

```bash
cargo bench -p touring-cognitive --bench semantic_graph 2>&1 | head -30
```

- [ ] **Step 8: Commit**

```bash
git add crates/touring-cognitive/
git commit -m "perf(cognitive): migrate GraphPersistence from serde_json to rkyv (5-10x speedup)"
```

---

## Phase 3 — Cognitive Integration (THE CRITICAL PATH)

### Task 8: Add touring-cognitive dependency to touring-hooks

**Files:**
- Modify: `crates/touring-hooks/Cargo.toml`

- [ ] **Step 1: Add dependency**

In `crates/touring-hooks/Cargo.toml` under `[dependencies]`:

```toml
touring-cognitive = { path = "../touring-cognitive" }
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check -p touring-hooks
```
Expected: OK (no circular deps — hooks depends on cognitive which depends on learning and ast, hooks already depends on both)

- [ ] **Step 3: Commit**

```bash
git add crates/touring-hooks/Cargo.toml
git commit -m "build(hooks): add touring-cognitive dependency for cognitive loop integration"
```

---

### Task 9: Implement KnowledgeSource for FileKnowledgeDB

**Files:**
- Create: `crates/touring-hooks/src/cognitive_bridge.rs`
- Modify: `crates/touring-hooks/src/lib.rs`
- Test: `cargo test -p touring-hooks`

This is the KEY TASK that connects the dormant CognitiveRuntime to the live HookRuntime.

- [ ] **Step 1: Write the failing test**

Create `crates/touring-hooks/src/cognitive_bridge.rs`:

```rust
//! Bridge between FileKnowledgeDB and touring-cognitive's KnowledgeSource trait.
//!
//! This module implements the KnowledgeSource trait for FileKnowledgeDB,
//! enabling the CognitiveRuntime to draw on accumulated hook knowledge
//! (file relations, bash outcomes, gotchas, co-edits, risk scores).

use touring_cognitive::bridge::{
    BashOutcomeRecord, CoEditPair, EditRecord, FileRelation, FileRisk,
    GotchaRecord, KnowledgeSource,
};
use crate::knowledge::FileKnowledgeDB;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_db() -> (TempDir, FileKnowledgeDB) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = FileKnowledgeDB::new(&db_path).unwrap();
        (dir, db)
    }

    #[test]
    fn test_knowledge_source_file_relations() {
        let (_dir, db) = make_db();
        // FileKnowledgeDB stores relations via record_file_relation
        db.record_file_relation("src/a.py", "src/b.py", "imports").unwrap();
        db.record_file_relation("src/a.py", "src/c.py", "imports").unwrap();

        let ks: &dyn KnowledgeSource = &db;
        let relations = ks.file_relations();
        assert_eq!(relations.len(), 2);
        assert_eq!(relations[0].source_path, "src/a.py");
    }

    #[test]
    fn test_knowledge_source_file_risk() {
        let (_dir, db) = make_db();
        let ks: &dyn KnowledgeSource = &db;
        let risk = ks.file_risk("nonexistent.py");
        assert_eq!(risk.risk_score, 0.0);
    }

    #[test]
    fn test_knowledge_source_file_count() {
        let (_dir, db) = make_db();
        let ks: &dyn KnowledgeSource = &db;
        assert_eq!(ks.file_count(), 0);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p touring-hooks cognitive_bridge
```
Expected: FAIL — `KnowledgeSource` not implemented for `FileKnowledgeDB`

- [ ] **Step 3: Implement KnowledgeSource for FileKnowledgeDB**

Read `crates/touring-hooks/src/knowledge.rs` to understand the existing `FileKnowledgeDB` API. Then implement:

```rust
impl KnowledgeSource for FileKnowledgeDB {
    fn file_relations(&self) -> Vec<FileRelation> {
        // Map FileKnowledgeDB::get_all_relations() -> Vec<FileRelation>
        // The exact method name needs to be read from knowledge.rs
        // Typical pattern: self.conn.prepare("SELECT ...").query_map(...)
        todo!("Map from FileKnowledgeDB's internal relation storage")
    }

    fn recent_bash_outcomes(&self, limit: usize) -> Vec<BashOutcomeRecord> {
        todo!("Map from FileKnowledgeDB::get_recent_outcomes()")
    }

    fn coedit_pairs(&self) -> Vec<CoEditPair> {
        todo!("Map from FileKnowledgeDB's co-edit tracking")
    }

    fn gotchas_for_file(&self, file_path: &str) -> Vec<GotchaRecord> {
        todo!("Map from FileKnowledgeDB::get_gotchas(file_path)")
    }

    fn recent_edits(&self, limit: usize) -> Vec<EditRecord> {
        todo!("Map from FileKnowledgeDB's edit history")
    }

    fn file_risk(&self, file_path: &str) -> FileRisk {
        todo!("Map from FileKnowledgeDB::compute_risk()")
    }

    fn dependents_of(&self, file_path: &str) -> Vec<String> {
        todo!("Map from FileKnowledgeDB::get_dependents()")
    }

    fn file_count(&self) -> usize {
        todo!("Map from FileKnowledgeDB stats")
    }

    fn relation_count(&self) -> usize {
        todo!("Map from FileKnowledgeDB stats")
    }
}
```

**CRITICAL:** Read `knowledge.rs` to find the actual method names. The `todo!()` above are placeholders — each must be replaced with actual calls to `FileKnowledgeDB` methods. If a method doesn't exist (e.g. `coedit_pairs`), add a new SQL query to `FileKnowledgeDB`.

- [ ] **Step 4: Add module to lib.rs**

Add to `crates/touring-hooks/src/lib.rs`:

```rust
pub mod cognitive_bridge;
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p touring-hooks cognitive_bridge
```
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add crates/touring-hooks/src/cognitive_bridge.rs crates/touring-hooks/src/lib.rs
git commit -m "feat(hooks): implement KnowledgeSource for FileKnowledgeDB — cognitive bridge"
```

---

### Task 10: Wire CognitiveRuntime into HookRuntime

**Files:**
- Modify: `crates/touring-hooks/src/runtime.rs`
- Test: `cargo test -p touring-hooks`

- [ ] **Step 1: Read runtime.rs fully to understand HookRuntime::new()**

```bash
# Read the full file in sections
```

- [ ] **Step 2: Add CognitiveRuntime field to HookRuntime**

Add to `HookRuntime` struct:

```rust
use touring_cognitive::{CognitiveRuntime, GraphPersistence};

pub struct HookRuntime {
    // ... existing fields ...

    /// Cognitive engine — semantic graph + predictor + MCTS + knowledge integration.
    /// Initialized lazily on first cognitive query.
    pub cognitive: Option<CognitiveRuntime>,
}
```

- [ ] **Step 3: Initialize CognitiveRuntime in HookRuntime::new()**

After the existing initialization code:

```rust
// Cognitive engine initialization
let graph_path = data_dir.join("cognitive_graph.rkyv");
let persistence = std::sync::Arc::new(GraphPersistence::new(graph_path));
let knowledge_arc: std::sync::Arc<dyn touring_cognitive::bridge::KnowledgeSource> =
    std::sync::Arc::new(/* clone or Arc of the knowledge db — needs careful design */);
let mut cognitive = CognitiveRuntime::new_with_knowledge(persistence, knowledge_arc);
cognitive.feed_edit_history();
```

**Design decision:** `FileKnowledgeDB` uses `rusqlite::Connection` which is NOT `Clone`. Options:
- A) Create a second `FileKnowledgeDB` instance pointing to the same SQLite file (WAL mode supports concurrent readers)
- B) Wrap `FileKnowledgeDB` in `Arc` and use the same instance for both `HookRuntime.knowledge` and `CognitiveRuntime.knowledge`
- C) Create a `ThreadSafeKnowledgeDB` (already exists in the codebase!) that wraps in `Arc<Mutex<FileKnowledgeDB>>`

Check if `ThreadSafeKnowledgeDB` implements `KnowledgeSource` or can be adapted.

- [ ] **Step 4: Add convenience method for cognitive queries**

```rust
impl HookRuntime {
    /// Resolve enriched cognitive context for a tool invocation.
    /// Returns None if cognitive engine is not initialized.
    pub async fn resolve_cognitive_context(
        &self,
        tool_name: &str,
        file_path: Option<&str>,
        query_hint: &str,
    ) -> Option<touring_cognitive::EnrichedCtx> {
        let cognitive = self.cognitive.as_ref()?;
        Some(cognitive.resolve_enriched(tool_name, file_path, query_hint).await)
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p touring-hooks
```
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add crates/touring-hooks/src/runtime.rs
git commit -m "feat(hooks): wire CognitiveRuntime into HookRuntime — closing the cognitive loop"
```

---

### Task 11: Feed post-hook outcomes into CognitiveRuntime

**Files:**
- Modify: `crates/touring-hooks/src/post_tool_rl.rs` (or wherever post-hook processing occurs)
- Test: `cargo test -p touring-hooks`

- [ ] **Step 1: Read post_tool_rl.rs to understand the post-hook flow**

- [ ] **Step 2: After existing RL reward processing, feed SessionPredictor**

```rust
// After QTable/LinUCB update:
if let Some(ref cognitive) = runtime.cognitive {
    cognitive.predictor().record(touring_cognitive::ToolInvocation {
        tool_name: tool_name.to_string(),
        timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        success: outcome.success,
    });
    // Update semantic graph access count for the file
    if let Some(fp) = file_path {
        let _ = cognitive.graph().touch_node(fp);
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p touring-hooks
```
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add crates/touring-hooks/src/post_tool_rl.rs
git commit -m "feat(hooks): feed post-hook outcomes into CognitiveRuntime predictor"
```

---

### Task 12: Inject enriched cognitive context in pre-hooks

**Files:**
- Modify: `crates/touring-hooks/src/pre_read.rs` (and/or `pre_edit.rs`, `pre_bash.rs`)
- Test: `cargo test -p touring-hooks`

- [ ] **Step 1: Read pre_read.rs to understand context injection flow**

- [ ] **Step 2: Add cognitive enrichment to pre-hook context**

Before returning the hook response, if cognitive is available:

```rust
// In the pre-hook handler, after existing context building:
if let Some(ref cognitive) = runtime.cognitive {
    // Note: resolve_enriched is async — need a tokio runtime handle
    // Since hooks are short-lived, use block_on or a shared runtime
    let enriched = tokio::runtime::Handle::current()
        .block_on(cognitive.resolve_enriched(tool_name, file_path, query));

    if !enriched.is_empty() {
        // Append enriched context (risk, gotchas, predicted co-edits)
        if let Some(risk) = enriched.risk_score {
            context.push_str(&format!("\n⚠️ Risk score: {:.2}", risk));
        }
        if let Some(ref gotchas) = enriched.gotchas {
            for g in gotchas {
                context.push_str(&format!("\n⚡ Gotcha: {}", g));
            }
        }
        if let Some(ref related) = enriched.related_files {
            context.push_str(&format!("\n📎 Likely co-edits: {}", related.join(", ")));
        }
        if let Some(predicted) = &enriched.base.predicted_tool {
            context.push_str(&format!("\n🔮 Predicted next: {}", predicted));
        }
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p touring-hooks --features pre-hooks
```
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add crates/touring-hooks/src/pre_read.rs crates/touring-hooks/src/pre_edit.rs
git commit -m "feat(hooks): inject enriched cognitive context in pre-hooks"
```

---

## Phase 4 — Medium-Term Improvements (M1, M2, M4, M6, M7)

### Task 13: Higher-order Markov in SessionPredictor

**Files:**
- Modify: `crates/touring-cognitive/src/session_predictor.rs`
- Test: `cargo test -p touring-cognitive`

- [ ] **Step 1: Write failing test for bigram prediction**

```rust
#[test]
fn test_bigram_prediction() {
    let sp = SessionPredictor::new();
    // Record pattern: Read -> Edit -> Read -> Edit -> Read -> Edit
    for _ in 0..10 {
        sp.record(ToolInvocation { tool_name: "Read".into(), timestamp_ms: 0, success: true });
        sp.record(ToolInvocation { tool_name: "Edit".into(), timestamp_ms: 0, success: true });
    }
    // After Read, should strongly predict Edit
    let predictions = sp.predict_top_k(3);
    assert_eq!(predictions[0].0, "Edit");
    // Bigram bonus: Read->Edit pattern should boost Edit's score
}
```

- [ ] **Step 2: Run test to verify it fails or has low confidence**

- [ ] **Step 3: Add bigram transition tracking**

Add to `SessionPredictor`:

```rust
/// Bigram transitions: (tool_a, tool_b) -> (tool_c -> count)
bigram_transitions: RwLock<HashMap<(String, String), HashMap<String, u64>>>,
```

In `record()`, update both unigram and bigram transitions. In `predict_next()`, combine:
- `score = 0.4 * unigram_prob + 0.6 * bigram_prob` (if bigram context available)
- `score *= (0.5 + 0.5 * q_value)`

- [ ] **Step 4: Run tests**

```bash
cargo test -p touring-cognitive
```
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/touring-cognitive/src/session_predictor.rs
git commit -m "feat(cognitive): add bigram transitions to SessionPredictor for higher-order prediction"
```

---

### Task 14: Parallel GoT children via tokio::spawn

**Files:**
- Modify: `crates/touring-cognitive/src/got.rs`
- Test: `cargo test -p touring-cognitive`

- [ ] **Step 1: Read got.rs fully**

Note the comment about sequential await due to lifetime constraints. We need to use `Arc<GotEngine>` for `'static` bounds.

- [ ] **Step 2: Refactor GotEngine to be Arc-shareable**

Change the `heuristic_fn` to be `Arc<dyn Fn(&str) -> f64 + Send + Sync>` instead of a closure with lifetime bounds. This enables `tokio::spawn`.

- [ ] **Step 3: Replace sequential children evaluation with parallel**

```rust
use tokio::task::JoinSet;

async fn explore_children(&self, parent_id: NodeId, depth: u32, max_depth: u32) -> Vec<ThoughtResult> {
    let children = self.get_children(parent_id);
    let mut join_set = JoinSet::new();

    for child in children {
        let engine = self.clone(); // requires Clone or Arc
        join_set.spawn(async move {
            engine.evaluate_node(child, depth + 1, max_depth).await
        });
    }

    let mut results = Vec::new();
    while let Some(Ok(result)) = join_set.join_next().await {
        results.push(result);
    }
    results
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p touring-cognitive
```
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/touring-cognitive/src/got.rs
git commit -m "perf(cognitive): parallel GoT children via tokio::spawn + JoinSet"
```

---

### Task 15: Add tests for watcher.rs

**Files:**
- Modify: `crates/touring-ast/src/watcher.rs`
- Test: `cargo test -p touring-ast watcher`

- [ ] **Step 1: Write integration tests for FileWatcher**

Add to the bottom of `watcher.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::time::Duration;

    #[test]
    fn test_watcher_creation() {
        let dir = TempDir::new().unwrap();
        let watcher = FileWatcher::new(dir.path());
        assert!(watcher.is_ok());
    }

    #[test]
    fn test_watcher_detects_file_create() {
        let dir = TempDir::new().unwrap();
        let mut watcher = FileWatcher::new(dir.path()).unwrap();

        // Create a file
        let file_path = dir.path().join("test.py");
        std::fs::write(&file_path, "print('hello')").unwrap();

        // Wait for debounce (100ms + margin)
        std::thread::sleep(Duration::from_millis(300));

        let events = watcher.poll_events();
        assert!(!events.is_empty(), "Should detect file creation");
    }

    #[test]
    fn test_watcher_detects_file_modify() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.py");
        std::fs::write(&file_path, "v1").unwrap();

        let mut watcher = FileWatcher::new(dir.path()).unwrap();

        std::fs::write(&file_path, "v2").unwrap();
        std::thread::sleep(Duration::from_millis(300));

        let events = watcher.poll_events();
        assert!(!events.is_empty(), "Should detect file modification");
    }
}
```

**Note:** Adapt the test to the actual FileWatcher API (read the file to confirm method names).

- [ ] **Step 2: Run tests**

```bash
cargo test -p touring-ast watcher
```
Expected: ALL PASS

- [ ] **Step 3: Commit**

```bash
git add crates/touring-ast/src/watcher.rs
git commit -m "test(ast): add integration tests for FileWatcher"
```

---

### Task 16: Add observability metrics export

**Files:**
- Modify: `crates/touring-hooks/src/metrics.rs`
- Test: `cargo test -p touring-hooks metrics`

- [ ] **Step 1: Read current metrics.rs**

- [ ] **Step 2: Add prometheus-style counter struct**

```rust
use std::sync::atomic::{AtomicU64, Ordering};

/// Runtime metrics counters for observability.
pub struct RuntimeMetrics {
    pub hook_invocations: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub rl_updates: AtomicU64,
    pub cognitive_queries: AtomicU64,
    pub average_hook_latency_us: AtomicU64,
}

impl RuntimeMetrics {
    /// Export metrics as JSON for external consumption.
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "hook_invocations": self.hook_invocations.load(Ordering::Relaxed),
            "cache_hits": self.cache_hits.load(Ordering::Relaxed),
            "cache_misses": self.cache_misses.load(Ordering::Relaxed),
            "rl_updates": self.rl_updates.load(Ordering::Relaxed),
            "cognitive_queries": self.cognitive_queries.load(Ordering::Relaxed),
            "avg_hook_latency_us": self.average_hook_latency_us.load(Ordering::Relaxed),
        })
    }
}
```

- [ ] **Step 3: Wire metrics into HookRuntime**

Increment counters at appropriate points (pre-hook dispatch, cache hit/miss, RL update, cognitive query).

- [ ] **Step 4: Run tests**

```bash
cargo test -p touring-hooks metrics
```
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/touring-hooks/src/metrics.rs crates/touring-hooks/src/runtime.rs
git commit -m "feat(hooks): add prometheus-style observability metrics"
```

---

### Task 17: Graph compaction in SemanticGraph

**Files:**
- Modify: `crates/touring-cognitive/src/semantic_graph.rs`
- Test: `cargo test -p touring-cognitive`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_compact_removes_stale_and_merges_redundant() {
    let p = Arc::new(GraphPersistence::new(PathBuf::from(":memory:")));
    let graph = SemanticGraph::new(p);

    // Add nodes with low access count and old timestamps
    for i in 0..10 {
        let node = MemoryNode {
            id: format!("stale_{i}"),
            label: format!("Stale {i}"),
            node_type: NodeType::File,
            embedding: vec![],
            metadata: serde_json::json!(null),
            last_accessed: 0.0,  // epoch = very old
            access_count: 0,
        };
        graph.add_node(node).unwrap();
    }

    // Add a fresh node
    let fresh = MemoryNode {
        id: "fresh".into(),
        label: "Fresh".into(),
        node_type: NodeType::File,
        embedding: vec![],
        metadata: serde_json::json!(null),
        last_accessed: chrono::Utc::now().timestamp() as f64,
        access_count: 5,
    };
    graph.add_node(fresh).unwrap();

    let removed = graph.compact(5); // keep top 5 by recency
    assert!(removed >= 5); // at least 5 stale nodes removed
    assert!(graph.node_count() <= 6); // at most 6 remain
}
```

- [ ] **Step 2: Run test to verify it fails**

- [ ] **Step 3: Implement compact()**

```rust
impl SemanticGraph {
    /// Remove nodes with lowest (access_count * recency) score.
    /// Keeps at most `max_nodes` nodes. Returns count of removed nodes.
    pub fn compact(&self, max_nodes: usize) -> usize {
        let mut graph = self.graph.write().unwrap();
        let node_count = graph.node_count();
        if node_count <= max_nodes {
            return 0;
        }

        // Score each node: access_count * temporal_decay
        let now = chrono::Utc::now().timestamp() as f64;
        let mut scored: Vec<(NodeIndex, f64)> = graph
            .node_indices()
            .map(|idx| {
                let node = &graph[idx];
                let age = (now - node.last_accessed).max(0.0);
                let decay = (-age / DECAY_HALF_LIFE_SECS * std::f64::consts::LN_2).exp();
                let score = (node.access_count as f64 + 1.0) * decay;
                (idx, score)
            })
            .collect();

        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let to_remove = node_count - max_nodes;
        let mut removed = 0;
        for (idx, _score) in scored.iter().take(to_remove) {
            // Remove from DashMap index first
            if let Some(node) = graph.node_weight(*idx) {
                self.index.remove(&node.id);
            }
            graph.remove_node(*idx);
            removed += 1;
        }
        removed
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p touring-cognitive
```
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/touring-cognitive/src/semantic_graph.rs
git commit -m "feat(cognitive): add graph compaction to SemanticGraph (remove stale nodes)"
```

---

## Phase 5 — Deep Improvements (D2, D3)

### Task 18: Streaming MCTS (background search task)

**Files:**
- Modify: `crates/touring-cognitive/src/mcts.rs`
- Create: `crates/touring-cognitive/src/mcts_streaming.rs`
- Modify: `crates/touring-cognitive/src/lib.rs`
- Test: `cargo test -p touring-cognitive mcts`

- [ ] **Step 1: Write the streaming MCTS wrapper**

Create `crates/touring-cognitive/src/mcts_streaming.rs`:

```rust
//! Streaming MCTS — background search that refines results with each new event.
//!
//! Wraps MCTSEngine in a tokio task that continuously improves its search
//! tree. Callers can request the best-so-far result at any time.

use crate::mcts::{MCTSConfig, MCTSEngine, MCTSResult};
use std::sync::Arc;
use tokio::sync::{watch, Mutex};

/// Handle to a streaming MCTS search running in the background.
pub struct StreamingMCTS {
    /// Latest best result (updated continuously).
    result_rx: watch::Receiver<Option<MCTSResult>>,
    /// Shutdown signal.
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

impl StreamingMCTS {
    /// Start a background MCTS search. Returns a handle for reading results.
    pub fn spawn(
        config: MCTSConfig,
        expand_fn: Arc<dyn Fn(u64) -> Vec<u64> + Send + Sync>,
        reward_fn: Arc<dyn Fn(u64) -> f64 + Send + Sync>,
        root_state: u64,
    ) -> Self {
        let (result_tx, result_rx) = watch::channel(None);
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let engine = MCTSEngine::new(config);
            let mut rollouts_done = 0;
            let batch_size = 10;

            loop {
                // Check shutdown
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }

                // Run a batch of rollouts
                let result = engine.search(root_state, &expand_fn, &reward_fn);
                rollouts_done += batch_size;

                // Publish latest result
                let _ = result_tx.send(Some(result));

                // Yield to other tasks
                tokio::task::yield_now().await;
            }
        });

        Self { result_rx, shutdown_tx }
    }

    /// Get the best result found so far. Returns None if no search has completed yet.
    pub fn best_so_far(&self) -> Option<MCTSResult> {
        self.result_rx.borrow().clone()
    }

    /// Stop the background search.
    pub fn stop(self) {
        let _ = self.shutdown_tx.send(());
    }
}
```

- [ ] **Step 2: Add export to lib.rs**

```rust
pub mod mcts_streaming;
pub use mcts_streaming::StreamingMCTS;
```

- [ ] **Step 3: Write tests**

- [ ] **Step 4: Run tests**

```bash
cargo test -p touring-cognitive mcts
```
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/touring-cognitive/src/mcts_streaming.rs crates/touring-cognitive/src/lib.rs
git commit -m "feat(cognitive): add StreamingMCTS for background continuous search"
```

---

### Task 19: Dependency-graph invalidation in IncrementalPipeline

**Files:**
- Modify: `crates/touring-ast/src/incremental_pipeline.rs`
- Test: `cargo test -p touring-ast incremental`

- [ ] **Step 1: Read incremental_pipeline.rs fully**

- [ ] **Step 2: Add invalidation method**

```rust
impl IncrementalPipeline {
    /// Invalidate cached parse trees for files that depend on `changed_file`.
    /// Uses the SymbolIndex dependency graph to find affected files.
    pub fn invalidate_dependents(&mut self, changed_file: &str, index: &SymbolIndex) {
        let dependents = index.reverse_dependencies(changed_file);
        for dep in dependents {
            self.invalidate(&dep);
        }
        tracing::debug!(
            changed = changed_file,
            invalidated = dependents.len(),
            "invalidated dependent parse caches"
        );
    }
}
```

**Note:** Check if `SymbolIndex` has a `reverse_dependencies` method. If not, it needs to be added using the `BlastRadius` logic already in `graph.rs`.

- [ ] **Step 3: Write test**

- [ ] **Step 4: Run tests**

```bash
cargo test -p touring-ast incremental
```
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/touring-ast/src/incremental_pipeline.rs
git commit -m "feat(ast): add dependency-graph invalidation to IncrementalPipeline"
```

---

### Task 20: Wire burn_transformer train_step to OnlineRL

**Files:**
- Modify: `crates/touring-learning/src/rl/burn_transformer.rs`
- Modify: `crates/touring-learning/src/online_rl.rs`
- Test: `cargo test -p touring-learning --features burn-transformer`

- [ ] **Step 1: Read burn_transformer.rs to understand train_step signature**

- [ ] **Step 2: Read online_rl.rs to find where experiences are processed**

- [ ] **Step 3: Add an optional neural net update path in OnlineRL**

When `burn-transformer` feature is enabled, after the experience buffer reaches a threshold (e.g., 32 samples), call `train_step` with a batch. This activates the dormant neural net.

```rust
#[cfg(feature = "burn-transformer")]
fn maybe_train_neural(&mut self) {
    if self.buffer.len() >= 32 {
        let batch = self.buffer.sample(32);
        // Convert to burn tensors and train
        self.transformer.as_mut().map(|t| t.train_step(&batch));
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p touring-learning --features burn-transformer
```
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/touring-learning/src/rl/burn_transformer.rs crates/touring-learning/src/online_rl.rs
git commit -m "feat(learning): wire burn_transformer train_step to OnlineRL engine"
```

---

### Task 21: Add tracing::instrument to critical methods

**Files:**
- Modify: `crates/touring-cognitive/src/semantic_graph.rs`
- Modify: `crates/touring-cognitive/src/session_predictor.rs`
- Modify: `crates/touring-cognitive/src/bridge.rs`
- Modify: `crates/touring-cognitive/src/mcts.rs`
- Test: `cargo test -p touring-cognitive`

- [ ] **Step 1: Add #[tracing::instrument] to methods that lack it**

For each file, add `#[tracing::instrument(skip(self))]` to public methods that perform significant work:

- `SemanticGraph::add_node`, `add_edge`, `find_similar`, `compact`
- `SessionPredictor::predict_next`, `predict_top_k`
- `CognitiveRuntime::resolve_enriched`, `populate_from_knowledge`
- `MCTSEngine::search`, `search_with_rl`

- [ ] **Step 2: Run tests**

```bash
cargo test -p touring-cognitive
```
Expected: ALL PASS

- [ ] **Step 3: Commit**

```bash
git add crates/touring-cognitive/src/
git commit -m "observability(cognitive): add tracing::instrument to all critical methods"
```

---

## Phase 6 — Cross-Crate Integration Tests

### Task 22: End-to-end cognitive loop integration test

**Files:**
- Create: `crates/touring-hooks/tests/cognitive_integration.rs`
- Test: `cargo test -p touring-hooks --test cognitive_integration`

- [ ] **Step 1: Write the integration test**

```rust
//! Integration test: end-to-end cognitive loop.
//!
//! Validates: hook event → knowledge capture → graph update → prediction → context injection

use tempfile::TempDir;
use touring_hooks::{FileKnowledgeDB, HookRuntime};

#[test]
fn test_cognitive_loop_end_to_end() {
    let dir = TempDir::new().unwrap();
    let runtime = HookRuntime::new(dir.path()).expect("runtime init");

    // Phase 1: Simulate post-hook capturing knowledge
    runtime.knowledge.record_file_read("src/main.rs").unwrap();
    runtime.knowledge.record_file_read("src/utils.rs").unwrap();
    runtime.knowledge.record_file_relation("src/main.rs", "src/utils.rs", "imports").unwrap();

    // Phase 2: Simulate post-edit recording
    runtime.knowledge.record_edit("src/main.rs", "replace", None).unwrap();

    // Phase 3: Verify cognitive engine received knowledge
    if let Some(ref cognitive) = runtime.cognitive {
        assert!(cognitive.graph().node_count() > 0, "graph should have nodes from knowledge");
    }

    // Phase 4: Verify enriched context includes risk + co-edits
    // (requires tokio runtime for async resolve_enriched)
    let rt = tokio::runtime::Runtime::new().unwrap();
    if let Some(ctx) = rt.block_on(runtime.resolve_cognitive_context("Edit", Some("src/main.rs"), "editing main")) {
        // Should have some enrichment from the recorded knowledge
        assert!(!ctx.is_empty() || ctx.base.predicted_tool.is_some(),
            "enriched context should contain predictions or knowledge");
    }
}
```

**Note:** Adapt method names to actual `FileKnowledgeDB` API. Read `knowledge.rs` for correct signatures.

- [ ] **Step 2: Run test**

```bash
cargo test -p touring-hooks --test cognitive_integration
```
Expected: ALL PASS

- [ ] **Step 3: Commit**

```bash
git add crates/touring-hooks/tests/cognitive_integration.rs
git commit -m "test(hooks): add end-to-end cognitive loop integration test"
```

---

### Task 23: Cross-crate benchmark — cognitive-enhanced vs baseline hook latency

**Files:**
- Create: `crates/touring-hooks/benches/cognitive_overhead.rs`
- Test: `cargo bench -p touring-hooks --bench cognitive_overhead`

- [ ] **Step 1: Write benchmark**

```rust
use criterion::{criterion_group, criterion_main, Criterion};
use tempfile::TempDir;
use touring_hooks::HookRuntime;

fn bench_hook_with_cognitive(c: &mut Criterion) {
    let dir = TempDir::new().unwrap();
    let runtime = HookRuntime::new(dir.path()).unwrap();

    // Seed some knowledge
    for i in 0..100 {
        let _ = runtime.knowledge.record_file_read(&format!("src/file_{i}.rs"));
    }

    c.bench_function("pre_hook_with_cognitive", |b| {
        b.iter(|| {
            let _response = runtime.build_pre_read_context("src/file_42.rs");
        });
    });
}

criterion_group!(benches, bench_hook_with_cognitive);
criterion_main!(benches);
```

- [ ] **Step 2: Add to Cargo.toml**

```toml
[[bench]]
name = "cognitive_overhead"
harness = false
```

- [ ] **Step 3: Run benchmark**

```bash
cargo bench -p touring-hooks --bench cognitive_overhead
```

Target: < 5ms per pre-hook invocation with cognitive enrichment.

- [ ] **Step 4: Commit**

```bash
git add crates/touring-hooks/benches/cognitive_overhead.rs crates/touring-hooks/Cargo.toml
git commit -m "bench(hooks): add cognitive overhead benchmark for pre-hook latency"
```

---

### Task 24: Transfer learning cross-session persistence

**Files:**
- Modify: `crates/touring-hooks/src/runtime.rs`
- Test: `cargo test -p touring-hooks`

- [ ] **Step 1: Save CognitiveRuntime state on session end**

Add to `HookRuntime`:

```rust
/// Save cognitive state for cross-session transfer.
pub fn save_cognitive_state(&self) -> Result<(), touring_cognitive::CognitiveError> {
    if let Some(ref cognitive) = self.cognitive {
        let snapshot = cognitive.graph().snapshot();
        cognitive.graph().persistence().save(&snapshot)?;
        tracing::info!("saved cognitive graph for next session");
    }
    Ok(())
}
```

Wire this into the session-stop hook handler.

- [ ] **Step 2: Verify load-on-startup already works**

`CognitiveRuntime::new_with_knowledge` → `GraphPersistence::load` should load the saved state. Verify by:
1. Run a session that captures knowledge
2. Save state
3. Create new runtime — verify graph is populated from saved state

- [ ] **Step 3: Run tests**

```bash
cargo test -p touring-hooks
```
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add crates/touring-hooks/src/runtime.rs
git commit -m "feat(hooks): save/load cognitive state for cross-session transfer learning"
```

---

### Task 25: Predictive prefetch via GoT + FocusCache

**Files:**
- Modify: `crates/touring-cognitive/src/bridge.rs`
- Test: `cargo test -p touring-cognitive`

- [ ] **Step 1: Add prefetch method to CognitiveRuntime**

```rust
impl CognitiveRuntime {
    /// Predict top-N likely next files and prefetch their graph context.
    pub fn prefetch_predicted(&self, current_file: &str, top_n: usize) {
        let knowledge = match &self.knowledge {
            Some(k) => k,
            None => return,
        };

        // Get co-edit predictions
        let coedits = knowledge.coedit_pairs();
        let predicted: Vec<String> = coedits
            .iter()
            .filter(|c| c.file1 == current_file || c.file2 == current_file)
            .map(|c| if c.file1 == current_file { &c.file2 } else { &c.file1 })
            .take(top_n)
            .cloned()
            .collect();

        // Prefetch each predicted file's graph context into FocusCache
        for file in &predicted {
            let graph = self.graph.clone();
            let file = file.clone();
            self.focus_cache.get_or_compute(&file, || {
                // Compute graph neighborhood for this file
                let neighbors = graph.get_neighbors(&file, 3);
                serde_json::to_string(&neighbors).unwrap_or_default()
            });
        }

        tracing::debug!(
            current = current_file,
            prefetched = predicted.len(),
            "prefetched graph context for predicted files"
        );
    }
}
```

- [ ] **Step 2: Call prefetch in post-hook after recording outcome**

In the post-hook flow, after recording the tool outcome:

```rust
if let Some(ref cognitive) = runtime.cognitive {
    cognitive.prefetch_predicted(file_path, 3);
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p touring-cognitive
```
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add crates/touring-cognitive/src/bridge.rs
git commit -m "feat(cognitive): predictive prefetch via CoEditPredictor + FocusCache"
```

---

### Task 26: Migrate Python indexers to Rust

**Files:**
- Create: `crates/touring-hooks/src/indexer.rs`
- Modify: `crates/touring-hooks/src/lib.rs`
- Test: `cargo test -p touring-hooks indexer`

This task replaces `touring_graph_indexer.py` and `touring_batch_indexer.py`.

- [ ] **Step 1: Read the Python indexers to understand their functionality**

```bash
head -50 /home/gabrielgadea/.claude/hooks/touring_graph_indexer.py
head -50 /home/gabrielgadea/.claude/hooks/touring_batch_indexer.py
```

- [ ] **Step 2: Create Rust indexer module**

```rust
//! Graph and batch indexer — Rust replacement for touring_graph_indexer.py
//! and touring_batch_indexer.py.
//!
//! Uses touring-ast's IncrementalPipeline + SymbolStore + SymbolIndex
//! to maintain a live index of the codebase's symbols and relations.

use touring_ast::{IncrementalPipeline, SharedPipeline, SymbolIndex, SymbolStore};
use crate::knowledge::FileKnowledgeDB;
use std::path::Path;

/// Index a single file into the symbol store and knowledge DB.
pub fn index_file(
    pipeline: &SharedPipeline,
    store: &SymbolStore,
    knowledge: &FileKnowledgeDB,
    file_path: &Path,
) -> Result<usize, String> {
    // 1. Parse file via incremental pipeline
    let result = pipeline.process_file(file_path)
        .map_err(|e| format!("parse failed: {e}"))?;

    // 2. Store symbols
    let symbols = result.symbols();
    let count = store.upsert_symbols(file_path, symbols)
        .map_err(|e| format!("store failed: {e}"))?;

    // 3. Extract and record file relations (imports)
    for imp in result.imports() {
        let _ = knowledge.record_file_relation(
            &file_path.display().to_string(),
            &imp.target,
            "imports",
        );
    }

    Ok(count)
}

/// Batch index all files matching patterns in a directory.
pub fn index_directory(
    pipeline: &SharedPipeline,
    store: &SymbolStore,
    knowledge: &FileKnowledgeDB,
    dir: &Path,
    extensions: &[&str],
) -> Result<usize, String> {
    let mut total = 0;
    for entry in ignore::WalkBuilder::new(dir).build() {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if extensions.contains(&ext) {
                match index_file(pipeline, store, knowledge, path) {
                    Ok(n) => total += n,
                    Err(e) => tracing::warn!(path = %path.display(), err = %e, "index failed"),
                }
            }
        }
    }
    Ok(total)
}
```

**Note:** Adapt to actual API methods after reading the pipeline and store source.

- [ ] **Step 3: Add module to lib.rs**

- [ ] **Step 4: Write tests**

- [ ] **Step 5: Run tests**

```bash
cargo test -p touring-hooks indexer
```
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add crates/touring-hooks/src/indexer.rs crates/touring-hooks/src/lib.rs
git commit -m "feat(hooks): Rust graph+batch indexer replacing Python touring_graph_indexer.py"
```

---

## Dependency Graph (DAG)

```
Phase 1 (parallel):
  T1 ─┐
  T2 ─┤─→ all independent, run in parallel
  T3 ─┤
  T4 ─┘

Phase 2 (sequential within, parallel with Phase 1):
  T5 → T6 → T7

Phase 3 (sequential, depends on T5-T7):
  T8 → T9 → T10 → T11 → T12

Phase 4 (parallel, depends on Phase 3):
  T13 ─┐
  T14 ─┤
  T15 ─┤─→ all independent
  T16 ─┤
  T17 ─┘

Phase 5 (parallel, depends on Phase 3):
  T18 ─┐
  T19 ─┤─→ independent
  T20 ─┤
  T21 ─┘

Phase 6 (depends on ALL above):
  T22 → T23 → T24 → T25 → T26
```

**Critical path:** T5 → T6 → T7 → T8 → T9 → T10 → T11 → T12 → T22

---

## Validation Checklist

After ALL tasks complete, verify:

```bash
# Full workspace build
cargo build --workspace

# Full workspace tests
cargo test --workspace

# Clippy (must be 0 errors — workspace deny all)
cargo clippy --workspace --all-targets

# Cognitive loop works end-to-end
cargo test -p touring-hooks --test cognitive_integration

# Performance not regressed
cargo bench -p touring-hooks --bench cognitive_overhead
cargo bench -p touring-cognitive --bench semantic_graph

# Hooks binary still works
cargo build -p touring-hooks --release
./target/release/touring-hook --help
```
