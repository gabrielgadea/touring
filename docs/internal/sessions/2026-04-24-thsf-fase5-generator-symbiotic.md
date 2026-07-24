# THSF Phase 5 — COMBO F Generator Symbiotic

> **Data**: 2026-04-24 | **Status**: ENTREGUE ✅ | **Autor**: Claude (TACO)
>
> **Resumo**: Propagação real-time de `health-delta` entre holons via
> `GeneratorHealth` capnp RPC. Schema novo `holon:generator@0.1.0`
> (separado de `holon:core`). Producer = Touring daemon
> (`touring-hooks::health_delta::compute_signals_delta`). Consumer fan-out
> = `touring-capnp-server::GeneratorHealthImpl` via
> `tokio::sync::broadcast` hospedado em `touring-core::health_events`.
> **P50 = 29 µs** end-to-end — 34× sob o target de < 1 ms.

---

## 1. Decisões arquiteturais

Autorizado por Gabriel após apresentação de 3 eixos × 3–4 opções:

| Eixo | Escolha | Trade-off aceito |
|---|---|---|
| **Producer** | `A` — Touring-native | Reusa health_delta maduro (6 counters, streaks, recovery). Evita duplicar lógica em cada holon. |
| **Protocolo** | `3` — capnp subscribe real-time (variante **3b**: import direto touring-core) | ~50 µs latência. Quebra parcial do isolation rationale de Fase 3: capnp-server agora depende de `touring-core` (leaf crate, leve) — NÃO de `touring-hooks`. |
| **Scope WIT** | `Y` — novo package `holon:generator@0.1.0` | Sinaliza opt-in capability que depende de Touring como producer. `holon:core` permanece fundacional e inalterado. |

**Invariantes preservadas**:
- `autonomy_guarantee = true` em todos os holons.
- Reversibilidade total via `rm schemas/holon-generator.capnp` +
  `rm src/generator_health.rs` + revert do emitter em
  `health_delta.rs` + revert do launcher.
- `holon:core@0.1.0` schema untouched.
- Nenhum projeto externo (konverter/analise/claude-trading) foi tocado.

---

## 2. Topologia resultante

```
┌──────────────────────────┐         ┌────────────────────────────┐
│ touring-hooks            │         │ touring-capnp-server       │
│   health_delta.rs        │         │   generator_health.rs      │
│   compute_signals_delta  │         │   GeneratorHealthImpl      │
│            │             │         │            ▲               │
│            ▼             │         │            │ subscribe     │
│   touring_core::          │         │            │ fan-out       │
│   publish_health_event(e)│         │   broadcast::Receiver      │
└────────────┬─────────────┘         └────────────┬───────────────┘
             │                                    │
             │  tokio::sync::broadcast (cap=64)   │
             ▼                                    │
      ┌──────────────────────────────────────────┴──┐
      │ touring-core::health_events::SENDER         │
      │ OnceLock<broadcast::Sender<HealthDeltaEvent>>│
      └──────────────────────────────────────────────┘

Unix socket: $XDG_RUNTIME_DIR/holon/generator.sock
Schema file: schemas/holon-generator.capnp  (@0x8d59c3ccb270ce4b)
```

---

## 3. Entregáveis por wave

### Wave 5A — Broadcast infra (`touring-core`)

Novo módulo `touring-core/src/health_events.rs` (~210 linhas):

```rust
pub enum DeltaOutcome { Neutral, Improvement, Regression }
pub struct HealthDeltaEvent {
    pub file_path: String,
    pub old_health: f32, pub new_health: f32, pub delta: f32,
    pub outcome: DeltaOutcome,
    pub regression_streak: u32, pub improvement_streak: u32,
    pub timestamp_ms: u64,
}
pub fn publish(event: HealthDeltaEvent) -> usize;      // returns subscriber count
pub fn subscribe() -> broadcast::Receiver<HealthDeltaEvent>;
pub fn subscriber_count() -> usize;
```

- Singleton `OnceLock<broadcast::Sender<_>>`, capacidade 64 (~8 KB peak).
- Publish non-blocking infallible — zero subscribers = drop silencioso.
- 6/6 unit tests PASS.

### Wave 5B — Schema capnp (`touring-capnp-server`)

Novo file `schemas/holon-generator.capnp` (~110 linhas):

```capnp
@0x8d59c3ccb270ce4b;
enum DeltaOutcome { neutral, improvement, regression }
struct HealthDeltaEvent { filePath, oldHealth, newHealth, delta, outcome,
                         regressionStreak, improvementStreak, timestampMs }
struct SubscriptionFilter { pathPrefixes, minAbsDelta, regressionsOnly }
struct HealthDeltaCounters { ... 7 fields }
interface HealthDeltaListener { onDelta(event) }
interface SubscriptionHandle { close() }
interface GeneratorHealth {
    subscribe(listener, filter) -> (handle)
    getCounters() -> (counters)
    specVersion() -> (version)
}
```

`build.rs` estendido para compilar o novo schema. Bindings expostos via
`pub mod holon_generator_capnp`.

### Wave 5C — Server impl (`touring-capnp-server`)

Novo file `src/generator_health.rs` (~290 linhas) — `GeneratorHealthImpl`:

- `subscribe`: inscreve no broadcast, spawna `tokio::task::spawn_local`
  que faz fan-out filtrado para o listener do cliente. Lagged receivers
  tratados via `tracing::warn!` + continue. Dead listener (capnp Error
  disconnected) derruba a subscription.
- `SubscriptionHandleImpl`: flip em `Rc<RefCell<bool>>` ou drop do
  handle client-side termina a fan-out task.
- `getCounters`: subprocess shim `touring gate-metrics -j`, parseia 7
  counters `health_delta_*`.
- `specVersion`: retorna `"0.1.0"`.
- Filter: `regressions_only`, `min_abs_delta`, `path_prefixes`.

5/5 unit tests PASS (filter logic + spec_version).

### Wave 5D — Daemon launcher + publisher wire

**Publisher** (`touring-hooks/src/health_delta.rs`):
após os counter increments em `compute_signals_delta`, emite:

```rust
touring_core::publish_health_event(HealthDeltaEvent {
    file_path, old_health, new_health, delta, outcome,
    regression_streak, improvement_streak, timestamp_ms,
});
```

Só publica quando `delta` é definido (pula first-observation).

**Launcher** (`src/bin/touring_capnp.rs`):
- Dual-socket. Novo `TOURING_CAPNP_GENERATOR_SOCKET` env var (default:
  derivado do registry socket trocando `registry.sock` →
  `generator.sock`).
- Accept loop duplo via `tokio::select!`. Cada connection gera um
  `RpcSystem` próprio.
- Ctrl+C limpa ambos os sockets.

`--print-config` agora reporta ambos os paths:

```json
{
  "spec_version": "1.0.0",
  "socket_path": "/run/user/1000/holon/registry.sock",
  "generator_socket_path": "/run/user/1000/holon/generator.sock",
  "root": "/home/gabrielgadea"
}
```

**41/41 health_delta regression tests PASS** — zero regressão.

### Wave 5E — E2E integration tests

Novo file `tests/e2e_generator_health.rs` (~330 linhas), 5 tests:

| Teste | Coverage |
|---|---|
| `spec_version_roundtrips` | capnp RPC call básico + version string |
| `subscribe_receives_published_events` | fan-out simples publish→listener |
| `filter_regressions_only_excludes_improvements` | filter semântico |
| `filter_path_prefixes_limits_delivery` | filter path scoping |
| `subscribe_receives_within_1ms_budget` | latência < 50 ms smoke budget |

Padrão: in-process server + client via `LocalSet` + tempdir socket.
`CollectingListener` acumula eventos em `Rc<RefCell<Vec>>`; publish via
`touring_core::publish_health_event` (mesmo processo = atinge broadcast).

**5/5 PASS em 0.17 s.**

### Wave 5F — Bench real

Novo file `examples/bench_generator_health.rs` (~200 linhas).

Mede latência publisher → listener via hdrhistogram (1 µs → 10 s, 3 decimais).
500 measured + 50 warmup. Loop sincronizado (aguarda cada evento antes do
próximo publish) para isolar latência pura do path.

**Resultados**:

```
──────────────────────────────────────────────────────────
  min  =     9.02 µs
  P50  =    29.49 µs     ← 34× sob target < 1 ms
  P90  =    50.34 µs
  P99  =    67.33 µs
  P999 =    98.62 µs
  max  =    98.62 µs
──────────────────────────────────────────────────────────
  ✓ target met (P50 < 1 ms = 1000 µs)
```

**Comparação com benches anteriores** (do `docs/2026-04-23-thsf-fase3-d34-benchmark.md` + Wave 4D):

| Transport | P50 | Notas |
|---|---:|---|
| capnp.spec_version (Rust, D3.4 floor) | 9 µs | Protocol floor sem work |
| **GeneratorHealth.subscribe (Phase 5)** | **29 µs** | publish → broadcast → filter → RPC |
| capnp.list_holons (Rust, D3.4) | 51 µs | RPC + walkdir |
| wasm.subprocess (Phase 4) | 12 100 µs | fork+exec+wasmtime |
| fs.subprocess baseline | 48 500 µs | fork+Python |

Phase 5 fica **no mesmo patamar** do floor capnp Rust (~3× o floor puro),
absolutamente dentro do envelope esperado para um fan-out filtrado.

---

## 4. Arquivos tocados

| Path | Status | Propósito |
|---|---|---|
| `crates/touring-core/src/health_events.rs` | CREATE | Broadcast infra |
| `crates/touring-core/src/lib.rs` | EDIT | `pub mod health_events` + re-exports |
| `crates/touring-capnp-server/schemas/holon-generator.capnp` | CREATE | Novo WIT package |
| `crates/touring-capnp-server/build.rs` | EDIT | Compila 2º schema |
| `crates/touring-capnp-server/src/lib.rs` | EDIT | `pub mod holon_generator_capnp` + `pub mod generator_health` |
| `crates/touring-capnp-server/src/generator_health.rs` | CREATE | GeneratorHealthImpl |
| `crates/touring-capnp-server/src/bin/touring_capnp.rs` | EDIT | Dual-socket launcher |
| `crates/touring-capnp-server/Cargo.toml` | EDIT | Adiciona `touring-core` dep |
| `crates/touring-capnp-server/tests/e2e_generator_health.rs` | CREATE | E2E (5 tests) |
| `crates/touring-capnp-server/examples/bench_generator_health.rs` | CREATE | Bench latência |
| `crates/touring-hooks/src/health_delta.rs` | EDIT | Publisher wire |
| `rust/docs/2026-04-24-thsf-fase5-generator-symbiotic.md` | CREATE | Este relatório |

**Alterações em workspaces externos** (konverter/analise/claude-trading): **zero**.

---

## 5. Testes (totais)

| Camada | Tests | Estado |
|---|---:|---|
| touring-core::health_events | 6 | ✅ |
| touring-capnp-server::generator_health (unit) | 5 | ✅ |
| touring-capnp-server::e2e_generator_health | 5 | ✅ |
| touring-hooks::health_delta (regressão) | 41 | ✅ |
| **Total** | **57** | **57/57 PASS** |

Nenhum teste pré-existente quebrou.

---

## 6. Exit criteria Fase 5

| # | Critério | Evidência |
|---|---|---|
| 1 | Novo schema capnp compilado e isolado de `holon:core` | `holon-generator.capnp` + build.rs |
| 2 | Broadcast channel compartilhado, leaf dep | `touring-core::health_events` |
| 3 | Publisher in-place sem regressão | 41/41 tests PASS |
| 4 | Daemon lança duas RPC interfaces em sockets separados | `--print-config` dual |
| 5 | E2E subscribe + filter + close demonstrado | 5/5 E2E PASS |
| 6 | Latência real P50 < 1 ms medida | **29 µs** (bench) |
| 7 | Invasão cirúrgica: nenhum projeto externo tocado | git/fs inspection |
| 8 | Reversibilidade: arquivos listados em §4 podem ser revertidos | Path lista curta |
| 9 | Documentação completa | Este relatório |

**9/9 ✅.**

---

## 7. Riscos residuais / follow-ups opcionais

| ID | Risco / oportunidade | Severidade | Mitigação proposta |
|---|---|---|---|
| R1 | `LatencyListener` não-Send — limita bench a `current_thread` runtime | Baixa | Ok para bench; produção usa multi-thread mas fan-out é local. |
| R2 | `getCounters` usa subprocess fork → ~50 ms (contra ~30 µs de subscribe). | Baixa | Fallback path; consumers real-time usam `subscribe`. Doc registra. |
| R3 | Dropped subscription sem `close()` explícito → task bg continua até primeiro send falhar | Média | Detectar via `receiver_count()` + periodic sweep em futuro. |
| R4 | Filter aceita apenas 3 campos. Não há filtro por streak_alert. | Baixa | Extensível em minor bump do schema. |
| R5 | `alert_threshold` em `HealthDeltaCounters` é hard-coded 3 | Baixa | Reflete constante `STREAK_ALERT_THRESHOLD` em touring-hooks. |

**Follow-ups naturais** (não parte desta fase):
- Python pycapnp client consumindo `GeneratorHealth.subscribe` (análogo a D3.3).
- WASM component `generator-health/` para ambientes sem capnp.
- Persistência opcional de deltas em SQLite (Grow-only set) para audit trail.

---

## 8. Comandos úteis

```bash
# Build + run daemon (dual socket)
cargo build --release -p touring-capnp-server
./target/release/touring-capnp --print-config
./target/release/touring-capnp   # serves until Ctrl+C

# E2E tests
cargo test -p touring-capnp-server --test e2e_generator_health

# Regression suite (confirma zero regressão em touring-hooks)
cargo test -p touring-hooks --lib health_delta::

# Latency bench (produces P50/P99 numbers)
cargo run --release -p touring-capnp-server --example bench_generator_health

# Inspect counter snapshot via capnp (precisa de cliente capnp — Python follow-up)
# Alternativa Phase 5: CLI direto
touring gate-metrics -j | jq '{record:.health_delta_record_count, compute:.health_delta_compute_count}'
```

---

**Fase 5 COMBO F declarada COMPLETA.** 5/5 waves + 9/9 exit criteria.
Pronta para Gabriel revisar e decidir próxima fase THSF.
