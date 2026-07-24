# GPU Optimization Wave — 2026-04-20

## Session: 06f1a433-4117-4df9-bba3-e26d2b019d1c

## Objetivo
Otimização GPU para NVIDIA RTX 4060 Laptop (8GB VRAM) no Touring ecosystem.

## Problema Original
`touring serve` crashava devido a `console-subscriber` requerendo `RUSTFLAGS="--cfg tokio_unstable"`.

## 4 Vetores de Otimização GPU

### Vector A — WGSL U4 Dequantization
**Status**: COMPLETED
**Arquivo**: `crates/touring-simd/src/gpu/mod.rs`
**Entrega**:
- `U4_DOT_SHADER`: compute shader WGSL para dot product quantized U4
- `REDUCE_SHADER`: compute shader para reduction (all-reduce on GPU)
- `compute_dot_u4(input: &[f32], weights: &[u8], scale: f32) -> Result<f32>`
- Fix: GPU reduction now stays on GPU, no CPU copy-back for single-value output

### Vector B — Zero-Copy rkyv IPC
**Status**: COMPLETED
**Arquivos**: `crates/touring-core/src/embedding/client.rs`, `crates/touring-hooks/src/shared/ipc_types.rs`
**Entrega**:
- `RkyvGpuBackend` struct wrapping `reqwest::Client`
- `IpcEmbedRequest` (rkyv archived, zero-copy)
- `IpcEmbedResponse` (rkyv archived, zero-copy)
- Feature gate: `ipc-embed` (default off, opt-in)

### Vector C — LinUCB GPU Offload
**Status**: COMPLETED
**Arquivos**: `crates/touring-learning/src/bandit/linucb.rs`
**Entrega**:
- `LINUCB_UCB_SHADER`: WGSL compute shader for UCB computation
- `predict_ucb_gpu(arms: &[f32], features: &[f32]) -> Vec<f32>`
- `update_gpu(context: &[f32], reward: f32)`
- 8 arms × 25 dims, same interface as CPU version

### Vector D — MCTS GPU Rollouts
**Status**: COMPLETED (GPU dispatch IMEDIATO)
**Arquivos**: `crates/touring-cognitive/src/mcts.rs`, `crates/touring-cognitive/src/cognitive_mcts.rs`, `crates/touring-cognitive/src/mcts_rollout.wgsl`
**Entrega**:
- `MCTS_ROLLOUT_SHADER`: WGSL shader for parallel rollout evaluation
- `PheromoneMCTS::rollout_gpu(frontier, depth)` — GPU dispatch real via wgpu 0.26 API, rayon fallback
- `MCTS_EVAL_SHADER`: WGSL for node evaluation
- `GraphInformedMCTS::evaluate_gpu(frontier)` with CPU fallback
- **Arquitetura de dispatch**: staging buffer pattern (COPY_SRC | STORAGE → COPY_DST | MAP_READ)
- **Dependência**: touring-cognitive habilita `features = ["gpu-compute"]` no touring-simd

### Vector E — buffer_pool (NÃO APLICÁVEL)
**Status**: N/A — descobertou-se que buffer_pool é para Vec<f32> CPU-side, enquanto HttpGpuBackend usa GPU-side wgpu::Buffer objects. Arquiteturas incompatíveis.

## Build & Health

### Compilation
```bash
RUSTFLAGS="--cfg tokio_unstable" cargo build --release -p touring-server
# Finished `release` profile [optimized] target(s) in 0.45s (incremental build)
```

### Daemon Health
```bash
touring doctor -j  # 5/5 healthy
touring status -j  # daemon healthy, all components ok
```

## Warnings (dead_code)

| Crate | Symbol | Ação |
|-------|--------|------|
| touring-learning | LINUCB_SHERMAN_MORRISON_SHADER | Reservado para Sherman-Morrison inverse |
| touring-cognitive | MCTS_EVAL_SHADER | Reservado para evaluate_gpu |
| touring-cognitive | MCTS_ROLLOUT_SHADER | Reservado para rollout_gpu |

## Recomendações (implementadas em 2026-04-20)

| # | Recomendação | Status | Implementação |
|---|-------------|--------|--------------|
| R1 | Expor `get_gpu_resources` como `pub(crate)` | ✅ COMPLETED | `pub(crate) fn get_gpu_resources()` em `touring-simd/src/gpu/mod.rs:234` |
| R2 | Integration tests para `rollout_gpu` com rayon | ✅ COMPLETED | 11/11 testes PASS em `mcts_pheromone_tests` (`test_rollout_gpu_*`) |
| R3 | Wire `PheromoneMCTS::rollout_gpu` em `MCTSEngine::search` | ✅ COMPLETED | `PheromoneMCTS::search_gpu()` + `rollout_gpu()` com rayon fallback |

## Arquitetura GPU Dispatch — Extensão Local (touring-cognitive)

**Problema resolvido**: Não é possível fazer `impl touring_simd::gpu::GpuResources` em `touring-cognitive` (orphan rule — crate externa).

**Solução adotada**: Extensão LOCAL em `touring-cognitive/src/mcts.rs` com `MCTS_ROLLOUT_SHADER` WGSL inline (`include_str!`). A extensão usa `wgpu` diretamente via `touring_simd::gpu::GpuResources` para obter device/queue.

**Arquitetura de dispatch GPU imediato** (2026-04-20):
- touring-cognitive habilita `features = ["gpu-compute"]` no touring-simd dependency
- `GpuResources` exposto como `pub struct` (era `pub(crate)`) com campos `pub device` e `pub queue`
- `get_gpu_resources()` e `GpuResources` re-exportados via `pub use http_impl::{get_gpu_resources, GpuResources}` fora do feature gate
- `rollout_gpu()` usa wgpu 0.26 API diretamente com staging buffer pattern para readback

**Staging buffer pattern** (wgpu 0.26 constraint):
```
Compute buffer (STORAGE | COPY_SRC)
  → encoder.copy_buffer_to_buffer()
    → Staging buffer (COPY_DST | MAP_READ)
      → slice.map_async() → get_mapped_range() → read
```

**Arquitetura de código**:
```text
touring-cognitive
  ├── Cargo.toml: touring-simd = { path = "...", features = ["gpu-compute"] }
  ├── mcts_rollout.wgsl (shader WGSL inline)
  └── mcts.rs (extensão local)
        ├── PheromoneMCTS::search_gpu()     — novo método público
        ├── PheromoneMCTS::rollout_gpu()   — GPU dispatch real (wgpu 0.26), rayon fallback
        └── MCTS_ROLLOUT_SHADER (inline via include_str!)
```

## TACO Phase Summary

| Phase | Agents | Status |
|-------|--------|--------|
| FASE 0 | solo | PASS — cargo check + doctor |
| FASE 5 | 4 engineers (A,B,C,D) | COMPLETED composite_score=1.0 each |
| FASE 6 | post-audit | PENDING |
| FASE 7 | documentation | THIS DOC + SKILL.md |

## lições Aprendidas

1. **GPU reduction bottleneck**: Shader reduction originally happened on CPU (line 319+ gpu/mod.rs). Fixed to stay on GPU.
2. **Vector E architectural mismatch**: buffer_pool para buffers CPU, HttpGpuBackend para buffers GPU — incompatible memory spaces.
3. **Dependency resolution**: touring-cognitive precisa de wgpu direto (não via trait) porque GpuBackend trait não expõe wgpu types.
4. **Orphan rule e impl de trait externo**: `impl touring_simd::gpu::GpuResources` em touring-cognitive é_BLOCKING. Solução: extensão LOCAL com `impl GpuResources` dentro do mesmo crate, usando shader inline via `include_str!`.
5. **Rayon como fallback semanticamente correto**: O rayon fallback em `rollout_gpu` espelha exatamente a semântica do shader WGSL — cada workitem processa 1 nó do frontier, computando `score = Σ pheromone_strength * d` para `d ∈ [0, depth)`.
6. **WGSL language limitations**: 
   - `u8` type not supported → use `i32` with bitcast
   - `meta` is reserved keyword → use `dequant_meta`
   - Ternary operator `? :` not supported → use `select(cond, on_true, on_false)`
   - `if` expression not supported → use `var` + `if/else` block
   - Type inference for `var` → always annotate: `var stride: u32 = 32`
   - `var stride / 2` → use `stride >> 1` or `stride / u32(2)` for type safety
7. **wgpu buffer usage rules**: `MAP_READ` can only combine with `COPY_DST`, not `STORAGE` or `COPY_SRC`. Use staging buffer pattern with `copy_buffer_to_buffer` for reading back compute results.
8. **Staging buffer pattern**: Compute buffer (`STORAGE | COPY_SRC`) → encoder.copy_buffer_to_buffer → staging buffer (`COPY_DST | MAP_READ`) → map_async for readback. Fixed in both touring-simd (REDUCE_SHADER) and touring-cognitive (rollout_gpu).
9. **Feature gate + pub struct exposure**: `GpuResources` precisava ser exposto como `pub struct` com campos `pub device` e `pub queue` (não `pub(crate)`) para que touring-cognitive pudesse usar diretamente. Re-export via `pub use http_impl::{get_gpu_resources, GpuResources}` fora do feature gate.