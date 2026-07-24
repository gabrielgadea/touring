# Changelog — touring-simd

All notable public API changes are documented here.

## [0.3.0] — 2026-04-20

### Added — GPU Compute Backend (feature: `gpu-compute`)

- `GpuResources` — `pub struct` with `pub device: wgpu::Device` and `pub queue: wgpu::Queue`. Previously `pub(crate)`, now exposed for cross-crate use by touring-cognitive.
- `get_gpu_resources()` — `pub fn` re-exported outside the feature gate so consumers can use it without enabling the feature themselves.
- `U4_DOT_SHADER` — WGSL compute shader for U4-dequantized dot product. `array<i32>` (WGSL has no `u8`), `dequant_meta` uniform (renamed from `meta`, which is a WGSL reserved keyword).
- `REDUCE_SHADER` — WGSL parallel reduction that stays entirely on GPU (fixed: original implementation did CPU copy-back). Staged buffer readback via `COPY_SRC → COPY_DST | MAP_READ`.
- `compute_dot_u4(input: &[f32], weights: &[u8], scale: f32) -> Result<f32>` — end-to-end GPU dot product for quantized inference.

### Changed

- `GpuResources` visibility: `pub(crate)` → `pub struct` (non-breaking within workspace, required for orphan-rule workaround in touring-cognitive).
- Re-export added: `pub use http_impl::{get_gpu_resources, GpuResources}` now lives outside the `#[cfg(feature = "gpu-compute")]` gate so the types are always available.

### WGSL Language Constraints (fixed)

| Constraint | Fix |
|-----------|-----|
| `u8` not a WGSL type | Use `array<i32>` with bitcast |
| `meta` is reserved keyword | Renamed to `dequant_meta` |
| Ternary `? :` not supported | Use `select(a, b, cond)` |
| `if` expression not supported | Use `var` + `if/else` block |
| `var x = 32` type inference | Annotate: `var x: u32 = 32` |
| `stride / 2` type mismatch | Use `stride >> 1` |

### Tests

- 283 tests (was 206). New tests cover `compute_dot_u4`, staging buffer pattern, and GPU resource management.

---

## [0.2.0] — 2026-03-30

### Stabilized Public API

The following re-exports constitute the stable public API surface of `touring-simd`.

> **Note**: This list is the authoritative reference for semver compatibility checks.
> Any change to these re-exports constitutes a breaking change requiring a major version bump.

### `similarity` module re-exports

| Symbol | Type | Description |
|--------|------|-------------|
| `CosineComputer` | struct | AVX-512/NEON vector computer for cosine similarity |
| `CosineSimilarity` | struct | Cosine similarity between two vectors |
| `JaccardComputer` | struct | Jaccard index computer for set similarity |
| `JaccardSimilarity` | struct | Jaccard similarity score |
| `TopKSearcher` | struct | Top-K nearest neighbor search |
| `TopKResult` | struct | Result from top-K search operation |
| `Similarity` | trait | Core similarity computation trait |

### `statistics` module re-exports

| Symbol | Type | Description |
|--------|------|-------------|
| `WilsonRanker` | struct | Wilson score confidence ranking |
| `DriftDetector` | struct | Kolmogorov-Smirnov two-sample drift detection |
| `DriftDetection` | struct | Drift detection result with statistics |

### `similarity::distance` re-exports

| Symbol | Type | Description |
|--------|------|-------------|
| `euclidean` | fn | L2 Euclidean distance between two vectors |
| `euclidean_batch` | fn | Batch Euclidean distance computation |
| `euclidean_batch_par` | fn | Parallel batch Euclidean distance |
| `manhattan` | fn | L1 Manhattan/city block distance |
| `manhattan_batch` | fn | Batch Manhattan distance |
| `manhattan_batch_par` | fn | Parallel batch Manhattan distance |
| `pearson_correlation` | fn | Pearson correlation coefficient |
| `pearson_batch` | fn | Batch Pearson correlation |
| `pearson_batch_par` | fn | Parallel batch Pearson correlation |
| `squared_euclidean` | fn | Squared Euclidean distance |
| `dot_product` | fn | Dot product of two vectors |
| `chebyshev` | fn | Chebyshev (L∞) distance between two vectors |
| `chebyshev_batch` | fn | Batch Chebyshev distance |
| `chebyshev_batch_par` | fn | Parallel batch Chebyshev distance |

### Feature-gated modules (NOT stable)

| Module | Feature gate | Status |
|--------|-------------|--------|
| `ann` | `ann` | Experimental — `HnswIndex` + `HnswConfig` now derive `serde::Serialize/Deserialize` |
| `buffer_pool` | — | Experimental |
| `financial` | — | Experimental |
| `quantization` | `quantization` | Experimental |
| `learning` | `learning-integration` | Experimental |
| `cortex` | `cortex-integration` | Experimental |

---

## [0.1.0] — 2025-01-01

Initial release.
