# Touring Integrations & Performance Layers

> Deep technical detail on Touring integrations: GPU compute, StringZilla SIMD, ACP/HyperGraph, rkyv IPC, supply-chain, Rust deep analysis, dynamic quality stack. Consult when working on, debugging, or extending these layers.

## Table of Contents

- [Rust Deep Analysis (Wave 4)](#rust-deep-analysis-wave-4)
- [Dynamic Quality Stack (Waves 5.1 → 19)](#dynamic-quality-stack-waves-51--19)
- [StringZilla Performance Layer](#stringzilla-performance-layer)
- [GPU Optimization (Vectors A-D)](#gpu-optimization-vectors-a-d)
- [ACP Protocol + HyperGraph (v4.9)](#acp-protocol--hypergraph-v49)
- [rkyv Zero-Copy IPC](#rkyv-zero-copy-ipc)
- [Supply-Chain & Observability](#supply-chain--observability)

---

## Rust Deep Analysis (Wave 4)

Three `touring ast` subcommands expose Rust-specific semantic depth via `syn` 2.0, `prettyplease`, and `cargo_metadata`. **All three are pure library calls** — no daemon round-trip.

| Command | Backed by | Output |
|---|---|---|
| `touring ast rust-semantic <file.rs>` | `syn` + `RustSemanticReport` | generics, trait bounds, lifetimes, derives, where clauses, unsafe+async counts, `semantic_complexity` ∈ [0,1] |
| `touring ast format-rust <file.rs>` | `prettyplease` + `format_rust_code` | rustfmt-clean output without invoking `rustfmt` binary |
| `touring ast workspace-info [<dir>]` | `cargo_metadata` + `WorkspaceInfo` | workspace packages, features per crate, dependents_of, packages_with_feature |

### Library APIs

```rust
use touring_ast::{format_rust_code, WorkspaceInfo};
use touring_ast::rust_semantic::RustSemanticReport;
use touring_ast::{TracedAstError, AstResultExt};        // span-trace errors
use touring_analysis::quality::RustQualitySignals;      // health bridge

let report    = RustSemanticReport::from_source(src)?;     // syn visit
let signals   = RustQualitySignals::from_report(&report);  // health score
let formatted = format_rust_code(src)?;                    // prettyplease
let ws        = WorkspaceInfo::load(".")?;                 // cargo metadata
let callers   = ws.dependents_of("touring-core");          // cross-crate blast radius
```

### When to Use

| Goal | Tool |
|---|---|
| Match existing generics when extending a module | `ast rust-semantic` |
| Emit edit output with rustfmt-clean formatting | `ast format-rust` or `surgery::format_rust_code` |
| Decide target crate before `Write` | `ast workspace-info` + `WorkspaceInfo::dependents_of` |
| Check if a feature flag already exists | `WorkspaceInfo::packages_with_feature` |
| Propagate span context with an error | `AstError::traced()` or `.traced()?` |
| Convert tree-sitter quality report → RL-ready signal | `RustQualitySignals::health_score()` |

### Test Infrastructure

- **rstest**: parametric `#[case]` tables (58 cases in `tests/parametric_multilang.rs`)
- **hdrhistogram**: P50/P95/P99/P99.9 latency guards (5 tests in `tests/latency_p99_guard.rs`)
- **pprof + criterion**: flamegraph CPU profiles (`benches/flamegraph_profile.rs`)
- **tracing-error**: `SpanTrace` capture on `AstError` via `.traced()`

### Critical Files

- `crates/touring-ast/src/rust_semantic.rs` — syn visitor (12 tests)
- `crates/touring-ast/src/surgery.rs::format_rust_code` (5 tests)
- `crates/touring-ast/src/wiring.rs::WorkspaceInfo` (5 tests)
- `crates/touring-ast/src/error.rs::TracedAstError` (5 tests)
- `crates/touring-analysis/src/quality/rust_semantic.rs` (8 tests)

---

## Dynamic Quality Stack (Waves 5.1 → 19)

System converting per-edit deltas into RL rewards + hint surfaces.

### Health Delta Bridge (cross-edit memory)

```bash
# Aggregate (7 counters + threshold)
touring health-delta status

# Per-path state (regression/improvement streak + hints)
touring health-delta status <path>
# {"file_path":"...","regression_streak":3,"improvement_streak":0,
#  "warning_hint":"⚠ regression streak: 3 consecutive declines on ... — review",
#  "improvement_hint":null,"alert_threshold":3}

# Reset after deliberate refactor checkpoint
touring health-delta reset <path>
```

### Architecture

- **W9**: `shared::health_delta` singleton (DashMap, OnceLock)
- **W10**: wired in `pre_edit::compose_edit_context` (record) + `post_edit run_returning` (compute + reward)
- **W11**: multi-lang dispatch (Rust = syn, Python/TS/TSX/JS = tree-sitter)
- **W13**: streak tracking — alert at `regression_streak == 3`; recovery on improvement
- **W14**: hint surfacing in `pre_edit` (Signal 13) + `pre_read::collect_index_signals` (weight 1.5)
- **W15**: `pre_write` wiring + CLI `touring health-delta`
- **W16**: `touring status -j` dashboard + 2 MCP tools `touring_health_delta_{status,reset}`
- **W19**: Generator integration via `HealthDeltaRecordFn` / `HealthDeltaComputeFn` closures in `Speculated::commit()`

### Wave 12 (2026-04-27) — B-301 6-dim TDG + B-302 PatchExpansion

#### B-301 RefactorRequired — promoted to 6-dimension TDG

Wave 11 emit-site (`pre_edit.rs:964-997`) recomputed quality_score locally from `avg_complexity / 20.0` (1-dim). Wave 12 dissolves the anonymous block that scoped `tdg` and consumes `tdg.composite` directly — capturing all 6 weighted dimensions:

```rust
// pre_edit.rs (Wave 12)
const B301_BLAST_THRESHOLD: usize = 20;
const B301_QUALITY_THRESHOLD: f64 = 0.40;
if blast_count > B301_BLAST_THRESHOLD && tdg.composite < B301_QUALITY_THRESHOLD {
    let finding = BlastWarning::RefactorRequired {
        file: file_path.to_string(),
        quality_score: tdg.composite,    // 6-dim
        blast_radius: blast_count,
    };
    tracing::warn!(
        code = %diag.code, ...,
        grade = %tdg.grade_letter(),     // A+..F now in event
        "B-301 RefactorRequired: high blast ({blast_count}) + low TDG composite ({:.2}, grade {})",
        tdg.composite, tdg.grade_letter()
    );
}
```

| Threshold | Old (1-dim) | New (6-dim) |
|-----------|-------------|-------------|
| `complexity = 1.0`, others = 0 | composite_proxy = 1.0 → silent | composite = 0.20 → fires |
| All dims = 0.3 | proxy = 0.3 → fires | composite = 0.30 → fires |
| All dims = 1.0 | proxy = 1.0 → silent | composite = 1.0 → silent |

#### B-302 PatchExpansion — new RFC-100 code

Wires the orphan `PatchComplexityDelta::compute()` (Wave P1.5) into production via mpatch fuzzy preview:

```rust
// pre_write.rs (Wave 12)
#[cfg(feature = "mpatch-fuzzy")]
pub fn emit_b302_if_low_confidence_expansion(
    file: &str,
    source: &str,
    preview: &PatchPreview,
) -> Option<PatchComplexityDelta> {
    const B302_CONFIDENCE_THRESHOLD: f32 = 0.7;
    let delta = PatchComplexityDelta::compute(source, &preview.preview, preview);
    if delta.is_expansion() && delta.confidence < B302_CONFIDENCE_THRESHOLD {
        // emit BlastWarning::PatchExpansion via tracing::warn!
        record_diagnostic_b302_emitted();
        Some(delta)
    } else {
        None
    }
}
```

**Production wiring**: `cli_mpatch_preview` (cli_handlers.rs:5122) calls helper; response JSON gains `b302_diagnostic` field (object when gate fires, `null` otherwise — backward compat).

```bash
# Live observability
touring gate-metrics -j | jq .diagnostic_b302_emitted_count
touring synergy --with-metrics -j | jq '.wired_pairs[] | select(.consumer | contains("B-302"))'
```

**Threshold matrix**:

| `is_expansion()` | `confidence < 0.7` | B-302 fires? |
|-------------------|--------------------|--------------|
| true | true | ✅ YES |
| true | false (high confidence) | ❌ NO |
| false (contraction) | true | ❌ NO |
| false (contraction) | false | ❌ NO |

**Severity**: `Warning` (patch is viable; merece review). NOT `Error` because dry-run already confirmed apply works.

**Critical files**:
- `crates/touring-core/src/diagnostic.rs` — `B_302_PATCH_EXPANSION` const
- `crates/touring-analysis/src/blast_radius/warning.rs` — `BlastWarning::PatchExpansion` variant
- `crates/touring-hooks/src/pre_write.rs::emit_b302_if_low_confidence_expansion`
- `crates/touring-hooks/src/cli_handlers.rs::cli_mpatch_preview` (response wiring)
- `crates/touring-hooks/src/shared/gate_metrics.rs` — `diagnostic_b302_emitted_count`
- `crates/touring-server/src/cli/synergy.rs` — 2 WIRED_PAIRS + 1 WIRED_PAIR_METRICS entries
- `~/projects/touring/docs/2026-04-27-wave12-b301-b302.md` — session report

### Counters in gate-metrics

```bash
touring gate-metrics -j | jq '{
  record: .health_delta_record_count,
  compute: .health_delta_compute_count,
  regression: .health_delta_regression_count,
  improvement: .health_delta_improvement_count,
  outstanding: .health_delta_outstanding,
  streak_alert: .health_delta_streak_alert_count,
  recovery: .health_delta_recovery_count,
  cache_hit: .query_cache_hit_count,
  cache_miss: .query_cache_miss_count,
  cache_ratio: .query_cache_hit_ratio,
  cache_invalidate: .query_cache_invalidate_count
}'
```

### Quality Gate Fusion

- **W6**: `QualityGateAdapter::detect_language()` — 8 languages
- **W7**: `.with_semantic_threshold(f32)` triggers syn-backed `RustQualitySignals::health_score()` as 4th gate
- **W8**: `wave5_workflow::rust_workflow_hint` emits `health=X.XX` + reward damper when `health < 0.75`. Envelope `[-0.10, +0.10]`

### Query Result Cache

5 hot paths cached (moka 4096 cap, 60s TTL):

| Handler | Tier | Use case |
|---|---|---|
| `cli_index_find` (W17) | TIER 1 | VGP verification |
| `cli_tantivy_search` (W17) | TIER 3 | BM25 ranked search |
| `cli_ast_meta` (W18) | TIER 1 | File metadata first |
| `cli_ast_blast` (W18) | TIER 1 | Blast radius pre-edit |
| `cli_index_search` (W18) | TIER 8 | Prefix lookup |

`invalidate_by_path(path)` called in `post_edit` + `post_write` after success. Stale window = 0 for recently edited files. Live hit ratio: 0.58+.

---

## StringZilla Performance Layer

Eight hotspot optimizations across 4 crates using StringZilla SIMD-accelerated string routines. Zero interface changes.

| ID | Crate | File | Technique | Gain |
|----|-------|------|-----------|------|
| T0.1 | touring-antt | `reranker.rs` | AhoCorasick replaces 8× `.contains()` | ~8× |
| T0.2 | touring-hooks | `pre_tool_validator.rs` | `StaticPrefixPattern` replaces regex for 29/30+ patterns | ~15× |
| T0.3 | touring-hooks | `async_knowledge.rs` | `memmem::Finder` + `OnceLock` replaces SQL LIKE | Eliminates full-scan |
| T1.1 | touring-analysis | `quality/complexity.rs` | `RangeUtf8NewlineSplits` replaces `str.lines()` | 3-5× |
| T1.3 | touring-analysis | `quality/fast_hash.rs` | `stringzilla::hash` AES-NI as blake3 pre-filter | Skips 90%+ blake3 |
| T2.1 | touring-generator | `core/context.rs` | BK-tree O(log N) + `sz_edit_distance` | ~2125× |
| T3.1 | touring-hooks | `cli_handlers_index.rs` | `utf8_case_insensitive_find` for `--ignore-case` | O(N) SIMD |
| T3.3 | touring-hooks | `cli_handlers_suggest.rs` | `LazyLock<AhoCorasick>` for 18 skill patterns | 18× |

### Key Invariants

- **`StaticPrefixPattern` vs `DangerousPattern`**: prefix-based patterns use `starts_with` O(m); regex only for the single catch-all. 85%+ of validator branches never touch regex.
- **`fast_content_hash` is pre-filter only**: uses `stringzilla::hash` (AES-NI polynomial, NOT cryptographic). `quick_content_changed()` calls it first; if hashes match, blake3 is skipped. blake3 remains authoritative.
- **BK-tree O(log N)**: `BkTreeFuzzyAdapter::top_k()` is lazy-seeded — built on first call. Edit distance via `sz_edit_distance` (feature `simd-fuzzy`). Previously O(N×m×n) brute-force.
- **AhoCorasick are `LazyLock`**: built once, zero subsequent overhead. Pattern sets: `ANTT_PATTERNS` (8), `TECHNICAL_KEYWORDS` (N), `SKILL_PATTERNS` (18).

### CLI New Capability

```bash
# Case-insensitive symbol lookup (T3.1)
touring index find HookRuntime --ignore-case
# Finds: HookRuntime, hookruntime, HOOKRUNTIME, etc.
```

### Cross-Audit E2E Tests

| Test file | Tests | Crate |
|-----------|-------|-------|
| `tests/stringzilla_e2e.rs` | 13 | touring-hooks |
| `tests/reranker_e2e.rs` | 10 | touring-antt |
| `tests/stringzilla_quality_e2e.rs` | 13 | touring-analysis |
| `tests/bktree_e2e.rs` | 10 | touring-generator |
| **Total** | **46** | 4 crates |

---

## GPU Optimization (Vectors A-D)

Four GPU optimization vectors for NVIDIA RTX 4060 Laptop (8GB VRAM). Feature-gated: `gpu-compute` (wgpu/WGSL) and `ipc-embed` (rkyv IPC).

### Vectors Summary

| Vector | Crate | File | New Symbols | Status |
|--------|-------|------|-------------|--------|
| A — WGSL U4 Dequantization | touring-simd | `src/gpu/mod.rs` | `U4_DOT_SHADER`, `REDUCE_SHADER`, `compute_dot_u4()` | COMPLETED |
| B — rkyv IPC | touring-core | `src/embedding/client.rs` | `RkyvGpuBackend`, `IpcEmbedRequest`, `IpcEmbedResponse` | COMPLETED |
| C — LinUCB GPU | touring-learning | `src/bandit/linucb.rs` | `LINUCB_UCB_SHADER`, `predict_ucb_gpu()`, `update_gpu()` | COMPLETED |
| D — MCTS GPU | touring-cognitive | `src/mcts.rs` | `MCTS_ROLLOUT_SHADER`, `MCTS_EVAL_SHADER`, `rollout_gpu()` | COMPLETED |
| E — buffer_pool | N/A | N/A | N/A | N/A (arch mismatch) |

### Vector A — WGSL U4 Dequantization (touring-simd)

GPU compute for quantized U4 embedding vectors. GPU reduction stays on GPU (no CPU copy-back).

```rust
pub const U4_DOT_SHADER: &str;       // WGSL compute shader
pub const REDUCE_SHADER: &str;        // GPU all-reduce
pub fn compute_dot_u4(input: &[f32], weights: &[u8], scale: f32) -> Result<f32>
```

### Vector B — rkyv IPC (touring-core)

Zero-copy IPC for embedding requests/responses via rkyv. Feature `ipc-embed` (default off).

```rust
pub struct RkyvGpuBackend { reqwest_client: reqwest::Client }
pub struct IpcEmbedRequest;  // rkyv archived, zero-copy
pub struct IpcEmbedResponse; // rkyv archived, zero-copy
```

### Vector C — LinUCB GPU Offload (touring-learning)

GPU-accelerated contextual bandit (8 arms × 25 dims).

```rust
pub const LINUCB_UCB_SHADER: &str;
pub fn predict_ucb_gpu(arms: &[f32], features: &[f32]) -> Vec<f32>;
pub fn update_gpu(context: &[f32], reward: f32);
```

### Vector D — MCTS GPU Rollouts (touring-cognitive)

GPU parallel rollout evaluation for MCTS. **GPU dispatch is IMMEDIATE** — not future.

```rust
pub fn PheromoneMCTS::rollout_gpu(frontier, depth) -> Result<Vec<f32>>;
pub fn PheromoneMCTS::search_gpu(...) -> Option<MCTSResult>;
pub fn GraphInformedMCTS::evaluate_gpu(frontier) -> Result<Vec<f32>>;
```

**Architecture (IMMEDIATE)**: touring-cognitive enables `features = ["gpu-compute"]` on touring-simd. `GpuResources` exposed as `pub struct` with `pub device` + `pub queue`. `rollout_gpu()` uses wgpu 0.26 directly with staging buffer pattern. Shader inline via `include_str!("mcts_rollout.wgsl")`.

**Staging buffer pattern (wgpu 0.26)**:
- Compute buffer: `STORAGE | COPY_SRC`
- Staging buffer: `COPY_DST | MAP_READ`
- Readback: `encoder.copy_buffer_to_buffer()` → `slice.map_async()` → `get_mapped_range()`

### Key Lessons

1. **GPU reduction bottleneck**: shader reduction originally on CPU. Fixed to stay on GPU.
2. **GpuBackend trait limitation**: doesn't expose wgpu types. touring-cognitive uses direct wgpu.
3. **Feature gate discipline**: GPU features opt-in (`gpu-compute`, `ipc-embed`) to avoid binary bloat.
4. **WGSL convention**: `@group(0) @binding(N)` for all shader bindings.
5. **Orphan rule**: `impl touring_simd::gpu::GpuResources` in touring-cognitive is BLOCKING. Use local extension with `impl GpuResources` inside same crate.
6. **Rayon as semantic fallback**: `rollout_gpu` rayon fallback mirrors WGSL semantics — each work-item processes 1 frontier node.
7. **WGSL language limitations**:
   - `u8` not supported → use `i32` with bitcast
   - `meta` is reserved → use `dequant_meta`
   - Ternary `? :` not supported → use `select(cond, on_true, on_false)`
   - `if` expression not supported → use `var` + `if/else` block
   - Always annotate `var` types: `var stride: u32 = 32`
   - Use `stride >> 1` or `stride / u32(2)` for type safety
8. **wgpu buffer rules**: `MAP_READ` only with `COPY_DST`, not `STORAGE`/`COPY_SRC`. Use staging buffer.
9. **Feature gate + pub struct**: `GpuResources` needs `pub struct` with `pub device` + `pub queue` (not `pub(crate)`) for cross-crate use. Re-export via `pub use http_impl::{get_gpu_resources, GpuResources}` outside feature gate.

### Verification

```bash
touring index find "U4_DOT_SHADER" -j
touring index find "LINUCB_UCB_SHADER" -j
touring index find "MCTS_ROLLOUT_SHADER" -j

cd /home/gabrielgadea/projects/touring
RUSTFLAGS="--cfg tokio_unstable" cargo check --workspace
```

---

## ACP Protocol + HyperGraph (v4.9)

### F3 — ACP (Agent Client Protocol) Shim

ACP is the editor↔agent wire protocol from Zed Industries (similar to LSP but optimized for AI agents). Opt-in shim layer `protocol/acp.rs` over the existing daemon socket: with `acp-protocol` feature enabled, daemon peeks bytes and routes:

- ACP messages: parsed as ACP `Message` envelope
- Legacy JSON: pass-through unchanged

```rust
pub const PROTOCOL_VERSION: &str = "acp-1.0";

// JSON-RPC 2.0 envelope
pub struct Message {
    pub jsonrpc: String,        // "2.0"
    pub id: String,             // request correlation
    pub method: String,          // e.g. "wiring.impact"
    pub params: serde_json::Value,
    pub correlation_id: Option<String>,
}

pub struct Response { id: String, result: Option<serde_json::Value>, error: Option<ResponseError> }
pub struct Capabilities { pub version: String, pub impact_analysis: bool, pub cycle_detection: bool, ... }

// Error taxonomy
pub mod errors {
    pub const E_INVALID_MESSAGE: i32 = -32700;
    pub const E_METHOD_NOT_FOUND: i32 = -32601;
    pub const E_INVALID_PARAMS: i32 = -32602;
    pub const E_INTERNAL_ERROR: i32 = -32603;
    pub const E_SERVER_BUSY: i32 = -32000;
    pub const E_CAPABILITY_NOT_NEGOTIATED: i32 = -32001;
}

// Helpers
pub fn detect_acp_payload(bytes: &[u8]) -> bool;
pub fn parse_message(json: &str) -> Option<Message>;
pub fn serialize_response(resp: &Response) -> Result<String, serde_json::Error>;
```

### F4 — HyperGraph (petgraph artificial node pattern)

`crates/touring-hooks/src/wiring/hypergraph.rs` implements hypergraphs via petgraph DiGraph: each hyperedge is an **artificial node** connected to its members via directed edges. Resolves petgraph's dyadic limitation for N-ary relations.

| Use Case | Hyperedge type |
|---|---|
| `#[cfg(all(feature = "X", feature = "Y"))]` | `FeatureGateHyperedge` |
| `use foo::{A, B, C}` | `MultiImportHyperedge` |
| Symbol used by multiple consumers | Custom via `HyperGraph::add_hyperedge()` |

```rust
use hypergraph::{HyperGraph, FeatureGateHyperedge, MultiImportHyperedge};

let mut hg: HyperGraph<&str> = HyperGraph::new();
let a = hg.add_node("module_a");
let b = hg.add_node("module_b");
let edge = hg.add_hyperedge(&[a, b], "feature_gate");

hg.hyperedges_for(a)     // Vec<NodeIndex> — hyperedges containing 'a'
hg.members_of(edge)       // Vec<NodeIndex> — members of hyperedge
hg.node_count()           // real nodes
hg.hyperedge_count()      // artificial nodes
hg.graph()                // &DiGraph for advanced petgraph algorithms
```

**FeatureGateHyperedge** — extracts features from `#[cfg(all(feature = "X"))]`:
```rust
let gate = FeatureGateHyperedge::new(
    r#"all(feature = "simd", feature = "gpu")"#,
    "touring_hooks::semantic_search",
);
// gate.features == vec!["simd", "gpu"]
```

**MultiImportHyperedge** — extracts symbols from `use foo::{A, B, C}`:
```rust
let m = MultiImportHyperedge::new("use foo::{A, B, C}", "module_x");
// m.imported_symbols == vec!["A", "B", "C"]
```

### Critical Files

- `crates/touring-hooks/src/protocol/acp.rs` — ACP shim (7 tests)
- `crates/touring-hooks/src/protocol/mod.rs` — re-export when `acp-protocol` enabled
- `crates/touring-hooks/src/wiring/hypergraph.rs` — HyperGraph wrapper (6 tests)

---

## rkyv Zero-Copy IPC

Zero-copy wire protocol on the daemon socket. Feature `rkyv-ipc` is in **default features** of `touring-hooks` + `touring-server` since 2026-04-14 — active in any standard build. Daemon peek-byte dispatch: `R` → rkyv, `{` → JSON (compat with legacy clients). Both directions framed (request + response).

| Build | Command |
|---|---|
| Standard (rkyv default ON) | `cargo build --release -p touring-server` |
| Daemon binaries (rkyv default ON) | `cargo build --release -p touring-hooks` |
| Opt-out (rare — interop testing) | `cargo build --release --no-default-features --features <minimal-set> -p touring-server` |

| Runtime control | Effect |
|---|---|
| `touring <cmd>` (default) | rkyv when feature on |
| `TOURING_RKYV_IPC=0 touring <cmd>` | Force JSON (hot rollback) |

| Observability | Command |
|---|---|
| All rkyv counters | `touring gate-metrics -j \| jq '{rkyv_dispatch_count, rkyv_parse_error_count, rkyv_response_count, rkyv_mean_bytes}'` |
| Healthy signal | `parse_error_count == 0` AND `dispatch_count > 0` |

### Critical Files

- `crates/touring-rkyv/src/ipc.rs` — `IpcRequest` / `IpcResponse` + framing
- `crates/touring-rkyv/tests/ipc_roundtrip.rs` — 10 tests (roundtrip + bytecheck)
- `crates/touring-rkyv/benches/ipc_vs_json.rs` — criterion (small/large/response)
- `crates/touring-hooks/tests/rkyv_ipc_e2e.rs` — 3 E2E via real UnixStream
- `crates/touring-hooks/src/daemon.rs` — `handle_connection_async` (peek byte)
- `crates/touring-server/src/cli/mod.rs` — `parse_daemon_response` (dual-path)
- `docs/2026-04-14-rkyv-ipc-rollout.md` — rollout playbook + benchmarks

**Bench gains** (criterion `--quick`): serialize 4-37×, parse 35-34800×.

---

## Supply-Chain & Observability

Four health gates:

| Gate | Command | Config |
|---|---|---|
| Supply-chain audit | `cargo deny check` | `deny.toml` (43 skips, 3 advisory ignores) |
| Test runner | `cargo nextest run --profile ci` | `.config/nextest.toml` |
| Line coverage ≥ 75% | `cargo llvm-cov --workspace --lcov` | `docs/ci-template.yml` job `coverage` |
| Unused deps | `cargo machete` | root-level run |

### Optional Feature Flags (touring-server)

| Feature | Activation | Use |
|---|---|---|
| `console` | `--features console` (rustflag `--cfg tokio_unstable` global via `.cargo/config.toml`) | `tokio-console http://127.0.0.1:6669` |
| `otlp` | `--features otlp` + `OTEL_EXPORTER_OTLP_ENDPOINT=http://<collector>:4317` | Jaeger/Tempo/any OTLP collector |
| `dhat-heap` | `--no-default-features --features dhat-heap,wasm-plugins,l7b-alpha,...` | `dhat-heap.json` + https://nnethercote.github.io/dh_view/dh_view.html |
| `heap-profile` | default ON — `MALLOC_CONF=prof:true touring profile heap-dump --output /tmp/heap.pb.gz` | jemalloc_pprof heap dump |

### Loom Proofs

```bash
RUSTFLAGS="--cfg loom" cargo test -p touring-loom-proofs --release
# → 3 passed: concurrent fetch_add, Release/Acquire publication, Mutex-protected map
```

Isolated crate `touring-loom-proofs/` (zero deps on touring; bypasses hyper-util).

### Compile-Time Invariants (`static_assertions`)

- `touring-simd/src/quantization.rs` → `EmbeddingU4: Send+Sync+Clone` + `size=40` (detects drift in serde/rkyv)
- `touring-hooks/src/shared/job_registry.rs` → `JobState: Send` (guards DashMap cross-thread)

### Critical Files

- `deny.toml` — supply-chain policy
- `.config/nextest.toml` — test runner
- `.cargo/config.toml` — tokio_unstable + aliases
- `crates/touring-server/src/telemetry_init.rs` — composable subscriber setup
- `crates/touring-loom-proofs/` — concurrency proofs
- `docs/ci-template.yml` — CI pipeline
- `docs/2026-04-14-supply-chain-and-observability.md` — session report

---

## touring-assists Crate (v4.27.0 / Wave C)

Refactor-as-CLI framework modeled after rust-analyzer's `ide-assists`. 10 handlers analyze code context, produce `SourceChange` artifacts, applied atomically via `Applier::commit()`.

### 10 Assist Handlers

| Handler | Purpose | RFC-100 |
|---------|---------|---------|
| `add_missing_match_arms` | Suggests arms for unhandled enum variants | A-100 |
| `auto_import` | Inserts `use` for unresolved symbols via index lookup | A-101 |
| `auto_wire` | Wires orphan pub symbols to best consumer (offensive against 199.832 orphans) | A-102 |
| `change_visibility` | Cycles pub ↔ pub(crate) ↔ pub(super) ↔ private | A-103 |
| `convert_to_guarded_return` | `if cond { body } else { return; }` → early-return guard | A-104 |
| `extract_function` | Extracts block to new function (Rust + JS/TS via tree-sitter) | A-105 |
| `generate_impl` | Generates `impl Trait for Type` skeleton | A-106 |
| `inline_call` | Inverse of extract — replaces call site with body | A-107 |
| `merge_imports` | Collapses adjacent `use` statements with shared prefix | A-108 |
| `move_module_to_file` | Converts `mod foo { ... }` to `mod foo;` + new `foo.rs` | A-109 |

### CLI Commands

```bash
touring assist list-kinds                      # list 10 handlers
touring assist applicable <file>:<line>:<col>  # applicable assists at cursor
touring assist apply <kind> <file> <range>     # apply assist (emits SourceChange)
```

### Architecture

```
AssistHandler = fn(&mut Assists, &AssistContext) -> Option<()>
Assist        = AssistId + Label + Group + Target + LazySourceChange
Assists       = accumulator with add/add_with_group/add_group methods
AssistCatalog = registry mapping AssistId → AssistHandler
```

Key modules: `assist.rs` (Assist, AssistTarget, LazySourceChange), `assists.rs` (Assists accumulator), `context.rs` (AssistContext), `catalog.rs` (AssistCatalog registry).

### SourceChange Integration

Each handler produces a `SourceChange` (from `touring-generator::source_change`). The applier commits atomically:

```rust
let result = applier.commit(&source_change, &mut files, path_for);
assert!(matches!(result, ApplyResult::Committed { .. }));
```

### Tests

14 E2E tests in `crates/touring-assists/tests/e2e_assist_pipeline.rs` — all PASS. Validates assist → LazySourceChange → SourceChange.evaluate() → Applier.commit() pipeline.

---

## touring-vfs Crate (v4.27.0 / Wave C)

Virtual filesystem layer for multi-file edit sessions. 7 modules:

| Module | Purpose |
|--------|---------|
| `lib.rs` | VfsState actor, Arc-cloned snapshots for concurrent sessions |
| `abs_path.rs` | Absolute path normalization (to_file_path, to_path) |
| `file_id.rs` | FileId allocation (FileIdAllocator trait) |
| `file_set.rs` | FileSet: manages file_id → path mappings |
| `overlay.rs` | Overlay: in-memory text edits layered over real files |
| `vfs.rs` | Vfs trait object + VfsSnapshot for serialization |
| `watcher.rs` | Filesystem watcher for external changes |

Used by the Applier to track in-memory file state during transactional multi-file commits.

---

## touring-incremental-salsa Crate (v4.27.0 / Wave C)

Salsa 0.18 incremental computation framework. 5 `#[salsa::input]` fields, 11 tests PASS.

```rust
// Example input struct
#[salsa::input]
struct CompilationOutput {
    #[id]
    file_id: FileId,
    stdout: String,
    stderr: String,
    duration_ms: u64,
}
```

Provides `Database` trait for memoized queries with revision tracking. Integrates with touring-generator for plan caching.

---

## touring-core::profile — Hotpath RAII Instrumentation (v4.25.0 / Wave A)

`touring-core/src/profile/` — RAII guards + background worker for measuring code block latency.

### Commands

```bash
touring profile query <file>       # query profile data for file
touring profile dump [--output f] # dump all profile data to JSON
touring profile heap-dump         # jemalloc_pprof heap profile
touring profile flamegraph        # CPU flamegraph via perf+jeprof
```

### Library API

```rust
use touring_core::profile::{Profile, measure_block, start_background_worker};

// RAII guard pattern
let _guard = measure_block("expensive_operation", &[("key", "value")]);
// On drop: records duration_ms, calls background worker upsert

// Background worker: batched upserts to Tantivy index
start_background_worker("profile", Duration::from_secs(5));
```

`Profile` struct: `file_id`, `block_name`, `duration_ms`, `timestamp`, `tags: Vec<(String,String)>`.

---

## touring ssr — Semantic Structural Rewrite (v4.26.0 / Wave B)

Pattern-based code transformation with VGP verification gate.

```bash
touring ssr --pattern 'old_fn' --replacement 'new_fn' [--path <file>]
touring ssr --pattern 'Foo' --replacement 'Bar' --dry-run
```

### How it works

1. Parse `pattern` with tree-sitter to find AST node
2. VGP gate: `touring index find <symbol>` to verify target exists
3. Build `TextEdit` (Indel stack) for the replacement
4. Applier commits atomically or returns errors

### Pattern syntax

`==>>` separator: `match pattern ==>> replacement`. The `==>>` operator distinguishes SSR from simple text search.

---

## RenderShape Budget (v4.26.0 / Wave B)

Budget constraint system for generator output width/indent. `touring-generator/src/shape.rs` — 169 LOC.

```rust
pub struct RenderShape {
    pub budget: u16,      // max columns
    pub indent: u8,        // current indentation level
    pub width: u16,        // remaining width
}

impl RenderShape {
    pub fn indent(&mut self) { self.indent += 1; self.width = self.budget - (self.indent * 2) as u16; }
    pub fn dedent(&mut self) { self.indent = self.indent.saturating_sub(1); self.width = self.budget - (self.indent * 2) as u16; }
}
```

Prevents generator output from exceeding specified column budgets. Used by all 30 GeneratorKind templates.

---

## CharClasses — Multi-language State Machine (v4.26.0 / Wave B)

Classifies characters for multi-language tokenization. CharClass ∈ {Code, StringLit, Comment, RawString, DocComment}.

```rust
pub enum CharClass { Code, StringLit, Comment, RawString, DocComment }

pub fn char_class(c: char, prev: CharClass) -> CharClass {
    match (c, prev) {
        ('"', Code) => StringLit,
        ('/', StringLit) => Comment,
        ('r', Comment) if next == '"' => RawString,
        _ => Code,
    }
}
```

25/25 tests PASS. Used by touring-assists for correct region detection in assist handlers.

---

## SourceChange — Transactional Multi-File Edits (v4.26.0 / Wave B)

`touring-generator/src/source_change/` — atomic cross-file edits with two-phase commit.

### Two-Phase Protocol

```
Phase 1 (shadow_validate): dry-run all text edits, verify files readable, check fs permissions
Phase 2 (commit): apply all edits atomically — rollback on ANY failure
```

### Key Types

```rust
pub struct SourceChange {
    edits: BTreeMap<FileId, TextEdit>,
    fs_edits: Vec<FileSystemEdit>,
    snippet: Option<SnippetEdit>,
}

pub enum FileSystemEdit {
    CreateFile { path: PathBuf, content: String },
    OverwriteFile { path: PathBuf, content: String },
    DeleteFile { path: PathBuf },
    MoveFile { from: PathBuf, to: PathBuf },
    MoveDir { from: PathBuf, to: PathBuf },
}
```

### Applier

```rust
pub struct Applier { /* ... */ }

impl Applier {
    pub fn shadow_validate(&self, change: &SourceChange) -> ApplyResult;
    pub fn commit<F>(&self, change: &SourceChange, files: &mut BTreeMap<FileId, String>, path_for: F) -> ApplyResult;
}

pub enum ApplyResult {
    Valid,
    Committed { files_written: usize, fs_ops: usize },
    Invalid { errors: Vec<ApplyError> },
    RolledBack { errors: Vec<ApplyError>, partial_writes: usize },
}
```

11 tests in `crates/touring-generator/tests/source_change_tests.rs` — all PASS. 14 E2E tests in `crates/touring-assists/tests/e2e_assist_pipeline.rs` — all PASS.

---

## SkipContext — Region Markers (v4.25.0 / Wave A)

`// touring:skip-region` ... `// touring:skip-end` markers for regions that should be excluded from analysis/editing.

### Commands

```bash
touring skip list <file>    # list all skip regions in file
touring skip validate <file> # validate skip region syntax
```

### W-115 Diagnostic

Post-edit hook emits `W-115` when an edit overlaps a skip region. Prevents accidental modification of generated/to-be-generated content.

```rust
// Example skip region
fn_generated_code!();
fn_too_complex_to_edit();

// touring:skip-region
deprecated_code();
// touring:skip-end
```

---

## RFC-100 Diagnostic Codes (Waves A/B/C)

| Code | Name | Severity | Description |
|------|------|----------|-------------|
| Q-220 | NonIdempotentFormat | warning | Edit produces different output on re-application |
| Q-310 | RegionFrozen | warning | Edit target is in a skip-region |
| W-115 | SkippedRegionWritten | warning | Edit overlaps skip-region marker |
| S-100 | SsrApplied | info | SSR applied successfully |
| S-101 | SsrRejected | warning | SSR not applicable or ambiguous |
| S-102 | SsrAmbiguous | info | Multiple SSR patterns — user choice required |
| G-200 | ShapeOverflow | warning | Generated output exceeds RenderShape budget |
| SC-100 | SourceChangeValid | info | SourceChange shadow-validate passed |
| SC-101 | SourceChangeCommitted | info | SourceChange committed successfully |
| SC-102 | SourceChangeRolledBack | warning | SourceChange failed, rolled back |
| F-200 | FormatPreserveDivergence | info | format-rust --preserve produced different output |
| A-100..A-109 | AssistApplied/Rejected/Ambiguous | info/warning | Per-handler assist codes (see touring-assists) |

All diagnostics emit via `miette` with source snippet context. Counter `diagnostic_<code>_emitted_count` tracks emission rate.
