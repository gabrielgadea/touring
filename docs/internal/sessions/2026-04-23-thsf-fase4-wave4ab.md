# THSF Fase 4 Waves 4A + 4B — Foundations + Proof-of-life

> **Data**: 2026-04-23  |  **Status**: 4A + 4B ENTREGUES ✅  |  **Próximo**: 4C (scale out para 3 capabilities)
>
> **Resumo**: WIT package `holon:core@0.1.0` + primeiro WebAssembly Component
> (`spec-version.wasm`, 62 KB) + host runner (`holon-wasm-runner`) + smoke test
> 3/3 PASS. Pipeline `wit-bindgen → wasm32-wasip2 → wasmtime 42` validado
> end-to-end. Latência subprocess-path: **P50 9.5 ms** (cold fork+exec).

---

## 1. Escopo entregue

### Wave 4A — Foundations

| ID | Deliverable | Status | Evidência |
|---|---|---|---|
| 4A.0 | Pre-flight (WASI 0.3 vs 0.2, toolchain) | ✅ | Rebaixado para WASI 0.2 (opção 1 pragmática) após descoberta que `wasm32-wasip3` não existe como target Rust stable |
| 4A.1 | `touring-wasm/wit/holon-core.wit` WIT package | ✅ | 60 linhas, 1 package + 2 interfaces + 1 world |

### Wave 4B — Proof-of-life

| ID | Deliverable | Status | Evidência |
|---|---|---|---|
| 4B.1 | `holon-spec-version.wasm` (primeiro component) | ✅ | 62 KB, exporta `spec-version` capability |
| 4B.2 | `holon-wasm-runner` (host driver) | ✅ | 327 linhas Rust, embeds wasmtime 42 |
| 4B.3 | Smoke test integration | ✅ | 3/3 tests PASS |

---

## 2. Arquitetura

```
/home/gabrielgadea/.claude/rust/
├── crates/                                          # main workspace (host)
│   └── touring-wasm/
│       └── wit/
│           └── holon-core.wit                       # Fase 4 authoritative WIT
│
└── holon-wasm-components/                           # SEPARATE workspace — out of `crates/`
    ├── Cargo.toml                                   # virtual workspace
    ├── spec-version/
    │   ├── Cargo.toml                               # → wasm32-wasip2 target
    │   └── src/lib.rs                               # wit-bindgen::generate!
    │
    ├── runner/
    │   ├── Cargo.toml                               # → host target
    │   ├── src/main.rs                              # wasmtime::component::*
    │   └── tests/smoke_spec_version.rs
    │
    └── target/
        ├── wasm32-wasip2/release/holon_spec_version.wasm   # 62 KB
        └── release/holon-wasm-runner                        # host bin
```

### Por quê separar dos `crates/`?

Decisão arquitetural (INFERENCE 0.9):

- Componentes compilam para `wasm32-wasip2`, não host — incluí-los no workspace principal exigiria hacks de `forced-target` e poluiria `cargo build --workspace`.
- Lockfile isolado → updates de `wit-bindgen` não propagam para 5k+ testes.
- Futuros componentes (blast-radius, quality-gate) adicionam-se como crates irmãs sem tocar no workspace Touring.

O WIT é authoritative em `crates/touring-wasm/wit/` (uma única fonte); as componentes referenciam via path relativo.

---

## 3. Ground truth: WASI 0.3 em 2026-04

**Premissa Gabriel (decisão A, sessão anterior)**: insistir em WASI 0.3 para async nativo.

**Realidade observada** [FACT 1.0]:

- `rustup target list | grep wasm` → apenas `wasm32-unknown-unknown` e `wasm32-wasip1`. `wasm32-wasip2` tem que ser instalado manualmente (sucesso). `wasm32-wasip3` **não existe** como target Rust stable em 2026-04.
- `wasmtime 42.0.2` crate tem módulo `p3` para WASI 0.3, mas é APIs do host-side (linker bindings), não um target compile.
- Conclusão: componentes hoje **obrigatoriamente** compilam para WASI 0.2 (`wasm32-wasip2`); WASI 0.3 async nativo depende de novo target Rust ainda não liberado.

**Decisão registrada**: Wave 4 inteira usa **WASI 0.2 component model** (stable). WASI 0.3 fica como *wave 4F* futura quando `wasm32-wasip3` estabilizar.

---

## 4. Decisões técnicas críticas

### 4.1 `wit-bindgen` versioning

- Workspace (components): `wit-bindgen = "0.35"` — versão estável; gera bindings em `wasm32-wasip2`.
- Runner (host): **sem bindgen** — uso direto de `wasmtime::component::Val` + `instance.get_export(..)` para portabilidade entre wasmtime 42/43/44.

Custo: `val_to_serde` tem CC=34 (match de 18 variantes de `Val`). **Aceito** — é boilerplate de API, não lógica complexa; e isola o runner de futuras evoluções do `bindgen!` macro.

### 4.2 WasiView API (wasmtime-wasi 42)

Divergiu significativamente da versão 24 que eu conhecia:

```rust
// wasmtime-wasi 42:
pub trait WasiView: Send {
    fn ctx(&mut self) -> WasiCtxView<'_>;
}
pub struct WasiCtxView<'a> {
    pub ctx: &'a mut WasiCtx,
    pub table: &'a mut ResourceTable,
}
```

Consequência: o método `table()` separado foi removido; implementador guarda ambos campos e retorna view unificado. `add_to_linker_sync` vive em `wasmtime_wasi::p2::`.

### 4.3 Imports implícitos via `std`

Um componente "sem imports" construído para `wasm32-wasip2` **ainda** linka `wasi:io/error@0.2.6` porque o `std` Rust o usa via panic hooks + allocator. Consequência: runner obrigatoriamente chama `p2::add_to_linker_sync` mesmo para componentes triviais.

Lesson [FACT 1.0]: `wasm32-wasip2` ≠ "free WASI". Qualquer std-linked component precisa de WASI runtime host.

---

## 5. Resultados de validação

### 5.1 Runner output (manual smoke)

```bash
$ holon-wasm-runner holon_spec_version.wasm list
["spec-version"]

$ holon-wasm-runner holon_spec_version.wasm invoke spec-version '{}'
{"ok":{"duration-ms":0,"exit-code":0,"logged":false,"stderr":[],
       "stdout":[123,34,115,112,101,99,95,118,101,114,115,105,
                 111,110,34,58,34,48,46,49,46,48,34,125]}}
# decodifica para: {"spec_version":"0.1.0"}

$ holon-wasm-runner holon_spec_version.wasm invoke does-not-exist '{}'
{"err":{"case":"unknown-capability","payload":"does-not-exist"}}
```

### 5.2 Integration tests (cargo test)

```
running 3 tests
test invoke_unknown_capability_returns_err_variant ... ok
test list_capabilities_returns_spec_version       ... ok
test invoke_spec_version_returns_version_bytes    ... ok
test result: ok. 3 passed; 0 failed; 0 ignored
```

### 5.3 Latência (N=100, subprocess cold path)

| Percentil | Valor |
|---|---|
| min  | 8.42 ms |
| p50  | **9.50 ms** |
| p95  | 11.37 ms |
| p99  | 11.84 ms |
| mean | 9.66 ms |
| stddev | 0.76 ms |

### 5.4 Comparação com transports existentes (contexto D3.4)

| Transport | P50 | Notas |
|---|---|---|
| capnp RPC (Rust) | **9 μs** | persistent UDS |
| capnp RPC (Python) | 44 μs | pycapnp overhead ~35μs |
| WASM (subprocess cold) | **9 500 μs** (9.5 ms) | fork+exec+wasmtime init+component load |
| fs.subprocess (`holon discover`) | 45 ms | Python startup + walk |

**Insight**: WASM subprocess é ~4.7× mais rápido que fs.subprocess (`holon discover`), mas ~1000× mais lento que capnp. Isso **é esperado** — WASM entrega **portabilidade + sandbox isolation**, não latência. Em consumer embeddings (long-running process + pooling allocator + component pré-instanciado), latência de invoke cai para sub-ms.

---

## 6. Próximos passos

### Wave 4C — Scale out (próxima sessão)

- 4C.1 `blast-radius.wasm` — port de touring-ast lógica essencial para `wasm32-wasip2`. Desafio: eliminar deps de filesystem.
- 4C.2 `quality-gate.wasm` — port de touring-analysis quality signals.
- 4C.3 `wac plug` composition — compor 2 capabilities em 1 `.wasm`.

### Wave 4D — Integration + pilot `konverter`

- 4D.1 Manifest v2 schema (`transport = "wasm"` + `wasm_component` path).
- 4D.2 `holon invoke` WASM adapter.
- 4D.3 Pilot real em `konverter` (menor atrito).
- 4D.4 Bench ampliado (CLI vs capnp vs WASM comparativo).

### Wave 4E — Docs + exit

- Consolidar Fase 4 completa.

---

## 7. Arquivos tocados / criados nesta sessão

| Path | Status | Propósito |
|---|---|---|
| `crates/touring-wasm/wit/holon-core.wit` | CREATE | WIT package authoritative |
| `holon-wasm-components/Cargo.toml` | CREATE | Virtual workspace (out of main `crates/`) |
| `holon-wasm-components/spec-version/Cargo.toml` | CREATE | First component crate |
| `holon-wasm-components/spec-version/src/lib.rs` | CREATE | wit-bindgen + Guest impl |
| `holon-wasm-components/runner/Cargo.toml` | CREATE | Host runner crate |
| `holon-wasm-components/runner/src/main.rs` | CREATE | wasmtime 42 component driver |
| `holon-wasm-components/runner/tests/smoke_spec_version.rs` | CREATE | 3 integration tests |
| Artefatos compilados: `holon_spec_version.wasm` (62 KB), `holon-wasm-runner` (host bin) | BUILT | |

**Alterações em `crates/` Touring workspace**: **zero** (apenas adicionado `wit/holon-core.wit` em touring-wasm; código Rust do workspace inalterado).

---

## 8. Invariantes THSF preservadas

- `autonomy_guarantee=true`: ✅ (nenhum projeto foi invadido)
- Reversibilidade: `rm -rf /home/gabrielgadea/.claude/rust/holon-wasm-components/ && rm /home/gabrielgadea/.claude/rust/crates/touring-wasm/wit/holon-core.wit` remove Fase 4 completamente.
- Zero deps externas que quebrem builds existentes: componentes em workspace isolado, não tocam workspace principal.
- Touring workspace `cargo build` inalterado: confirmado (não recompilamos touring-wasm em nenhum momento).

---

*Fase 4 progresso: 2/5 waves entregues. Restam Waves 4C + 4D + 4E para completar o COMBO C (WASM Woven Holarchy) do plano mestre.*
