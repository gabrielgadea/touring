# ADR — Wave 1 Cognitive Integration Roadmap

> **Status**: Proposed | **Date**: 2026-04-14 | **Scope**: W1 (P0 #1, #2)
> **Depends on**: `2026-04-14-crates-inventory-ranking.md`, W0 Quick-Wins (delivered same day)

## Context

The crates inventory identifies 3 crates with >15 composite ROI score, all in the **cognitive layer**:

| # | Crate | Score | Value proposition |
|---|---|---|---|
| 1 | `candle-core + candle-nn + candle-transformers` | 15.5 | In-process GGUF Q4_K_M embedding inference — zero-latency RAG, kills external API dependency |
| 2 | `mentedb-cognitive` | 15.4 | U-Curve attention + Delta-Aware Serving + Belief Propagation — 90.7% token reduction in 20-turn loops, 0% stale returns |
| 3 | `moka` | 8.4 | W-TinyLFU eviction — already integrated in 4 crates, requires weigher-based refactor |

W0 (delivered 2026-04-14) shipped the **skeleton**: workspace deps declared,
feature gates wired, trait abstractions in place. This ADR plans the **full
integration** for a dedicated next session.

## Decision

### D1. Keep the Embedder trait as the integration boundary

`touring-learning::semantic::Embedder` is the sole dyn-safe surface. All
consumers (`touring-ast::file_heat`, `touring-cortex::palace`,
`touring-hooks::post_read`) import `Box<dyn Embedder>` — never the concrete
`CandleEmbedder` or `MockEmbedder`. Feature gates must not leak past this
boundary.

Rationale: swap-friendly between real inference (candle) and fallback
(mock) without touching call sites; CI paths stay lightweight.

### D2. CandleEmbedder loads quantized GGUF directly from disk

Model distribution: `HF_HUB_CACHE` (default `~/.cache/huggingface/hub/`).

Target models (ordered by size/quality tradeoff):

| Model | Dims | Q4_K_M size | Expected P50 latency (batch=1) | Use case |
|---|---|---|---|---|
| `bge-micro-v2` | 384 | ~26 MB | < 2 ms | session-level signals, fast |
| `bge-small-en-v1.5` | 384 | ~42 MB | < 4 ms | production default |
| `nomic-embed-text-v1.5` | 768 | ~96 MB | < 10 ms | high-recall semantic search |

Loader signature (to implement):
```rust
impl CandleEmbedder {
    pub fn load_gguf(
        gguf_path: impl AsRef<Path>,
        tokenizer_path: impl AsRef<Path>,
        device: Device,
    ) -> Result<Self, CandleError> { /* ... */ }
}
```

Forward pass: tokenize → `BertModel::forward` → mean pool over last hidden
state → L2 normalize → `Vec<f32>` of length `dims`.

### D3. Plug candle embeddings directly into `EmbeddingU4::from_f32`

Pipeline: `text -> CandleEmbedder::embed(&str) -> Vec<f32> -> EmbeddingU4::from_f32(&v)`.

No intermediate quantization buffer needed — `from_f32` is the accepted sink
and already handles global min/max scaling + 4-bit packing.

Downstream consumers (`ann_search_u4`, `SemanticRecall`) operate exclusively
on `EmbeddingU4` values; they are indifferent to the source backend.

### D4. mentedb-cognitive goes behind `cognitive-memory` feature gate

Do NOT add to default features — the crate is young (v0.3.2) and
unproven in production. Gate guarantees:

- Workspace builds without the dep unless `--features cognitive-memory` used
- Supply-chain review of mentedb's transitive deps happens during gate enablement
- Downstream can opt-in per-crate (touring-cortex is the natural home for
  Palace Hierarchy bridge)

Integration layer: `touring-cortex::palace::MentedbBridge` implements the
existing `PalaceStore` trait; mentedb becomes a drop-in backend for 5-tier
memory (Working/Episodic/Semantic/Procedural/Archival).

### D5. Load model lazily per daemon actor

Each project actor holds a `OnceLock<Arc<CandleEmbedder>>`. First use
triggers load; subsequent calls are zero-cost pointer clones. Model weights
live in anonymous mmap (`memmap2` already in workspace) so the RSS cost is
paid once and shared across actors.

Device selection: `Device::Cpu` default; `Device::Cuda(0)` if
`TOURING_EMBEDDINGS_DEVICE=cuda` env var is set AND candle was built with
`--features cuda`.

## Consequences

### Positive
- Zero external network calls for embeddings (privacy + latency)
- `SemanticRecall` upgrades from FxHash heuristic to true cosine similarity
- Token budget drops ~90% on long sessions (via mentedb Delta-Aware Serving)
- EmbeddingU4 quantization pipeline unchanged — all downstream code works

### Negative / Mitigations
- **+200MB** compile time when `semantic-embeddings` is active — mitigated
  by `Swatinem/rust-cache@v2` in CI (already configured)
- **Model file distribution** — user must pre-cache models. Mitigation:
  bootstrap script `scripts/fetch_embeddings.sh` runs `huggingface-cli
  download` on first `touring doctor` invocation
- **mentedb immaturity risk** — mitigated by feature gate (off by default)
  + integration-test-only activation until v1.0

## Implementation plan (next session)

### Phase 1 — Real CandleEmbedder (L3, ~3h)
- [ ] Replace `candle_embedder::stub()` with `load_gguf()` implementation
- [ ] Add `tokenizers = "0.21"` to workspace deps (feature-gated under `semantic-embeddings`)
- [ ] Write integration test that loads `bge-micro-v2` from `tempfile::tempdir`
- [ ] Criterion bench: real forward pass vs MockEmbedder baseline
- [ ] Document model fetch script

### Phase 2 — Wire into file_heat signal (L2, ~2h)
- [ ] `touring-ast::file_heat` gains optional `semantic_embedder: Option<Arc<dyn Embedder>>`
- [ ] When present, file_digest_signal produces an embedding-augmented digest
- [ ] Backward compatible: None path preserves current AST-only behavior

### Phase 3 — mentedb-cognitive bridge (L4, ~4h)
- [ ] Add `mentedb-cognitive = { version = "0.3", optional = true }` to workspace
- [ ] Declare `cognitive-memory` feature in touring-cortex
- [ ] Implement `MentedbBridge: PalaceStore` adapter
- [ ] Integration test: 20-turn conversation produces ≥ 80% token reduction

### Phase 4 — Benchmark & decide on default
- [ ] Baseline: P50/P95 latency of current FxHash-based SemanticRecall
- [ ] Measure: P50/P95 with real CandleEmbedder + EmbeddingU4 quantization
- [ ] Go/no-go: enable `semantic-embeddings` by default only if P50 stays < 5ms

## Alternatives considered

### A1. ONNX Runtime via `ort` crate
**Rejected**: ONNX has quality issues with GGUF models, requires a separate
runtime dep (~300MB), and candle's quantization support is superior.

### A2. Direct tokenizers + serde safetensors (no candle)
**Rejected**: would re-implement 80% of candle-transformers bert module.
Candle's `BertModel` is well-tested and <1000 LOC to use.

### A3. mentedb as default (no feature gate)
**Rejected**: crate v0.3.2 has not been audited under load. Gate first, then
promote to default after 30 days of daemon exposure in integration suite.

## References

- Original ranking: `docs/analyses/2026-04-14-crates-inventory-ranking.md`
- Supply-chain policy: `deny.toml` (must accept candle/mentedb licenses before activation)
- Skeleton lands: `crates/touring-learning/src/semantic/` (W0 deliverable)
- Bench baseline: `crates/touring-learning/benches/embedding_u4.rs` (W0 deliverable)
