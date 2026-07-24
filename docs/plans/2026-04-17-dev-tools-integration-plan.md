# Plano de Integração: 7 Dev-Tools Crates no Touring

> **Data**: 2026-04-17 | **Session**: assert_cmd + hdrhistogram + cfg_aliases + … | **Escopo**: 7 itens priorizados | **Status**: Planejamento + Implementação P0

## Contexto

Após análise empírica de 7 categorias do `crates.io` (debugging, testing, profiling, cargo-plugins, build-utils, procedural-macro-helpers, ffi), foram identificados **7 crates novos** com potencial de agregar valor significativo ao Touring. Este plano detalha cada um, com **scout empírico validando as premissas** antes da implementação.

## Scout Empírico (VGP V2)

| Item | Premissa original | Evidência real | Veredito |
|---|---|---|---|
| binary_e2e.rs coverage | "limitada" | 219 LOC, ~15 funções de teste para 54 CLI commands | ✅ CONFIRMADO (~28% cobertura) |
| gate_metrics counters | "rkyv_dispatch_bytes/count" | **31 `AtomicU64`** — 6x mais counters que documentado | ✅ CONFIRMADO + ampliado |
| cfg_aliases impact | "17+ features repetidas" | Apenas **2 `cfg(all(feature))` em todo touring-server** | ⚠️ **ESCOPO REDUZIDO** — não justifica integração |
| hook_registry manual | "138 entries manuais" | Não verificado nesta sessão | ⏳ Pendente |
| post_tool_failure backtrace | "bytes mangled" | Não verificado nesta sessão | ⏳ Pendente |
| touring-server proc-macro | "`#[tool]` via rmcp" | rmcp é dep externa — não é macro interna do Touring | ⚠️ Escopo N/A |

**Ajuste do roadmap pós-scout**: cfg_aliases rebaixado de P0→P2, proc-macro-crate movido para N/A.

---

## Plano Priorizado (Ajustado)

### 🔴 P0 — Implementar Agora

#### #1 assert_cmd — CLI E2E Testing [FACT 1.0, Score 0.90]

**Problema**: `binary_e2e.rs` tem 15 tests para 54 CLI subcommands = 28% cobertura empírica. Regressões em comandos como `doctor`, `status`, `wiring audit`, `index find` não são detectadas automaticamente.

**Solução**: Adicionar `assert_cmd` dep + expandir `binary_e2e.rs` cobrindo os 20 comandos mais críticos.

**Comandos críticos a cobrir** (Tier 1-2 da Touring skill):
1. `touring --version` + `touring --help` (sanity)
2. `touring doctor -j` (health check)
3. `touring status -j` (dashboard)
4. `touring index status -j`
5. `touring index find <symbol> -j`
6. `touring index search <prefix> -j`
7. `touring ast meta <file> --depth skeleton -j`
8. `touring ast overview <file> -j`
9. `touring ast blast <file> -j`
10. `touring wiring orphans -j`
11. `touring wiring modules -j`
12. `touring wiring audit -j`
13. `touring memory stats -j`
14. `touring memory recall "<query>" -j`
15. `touring tantivy search "<query>" -j`
16. `touring tantivy stats -j`
17. `touring gate-metrics -j`
18. `touring decompose status -j`
19. `touring session list -j`
20. `touring generate list-kinds -j`

**Assertions padrão**:
- `.assert().success()` — exit code 0
- `.stdout(predicate::str::contains("{"))` — JSON válido
- `.stdout(predicate::str::is_match(r#""[a-z_]+":"#))` — tem campos

**Esforço**: L2 (~4-6h)
**Arquivos**:
- `crates/touring-server/Cargo.toml` — add `assert_cmd = "2"` + `predicates = "3"` em dev-dependencies
- `crates/touring-server/tests/binary_e2e.rs` — expandir de 15 → 35+ tests
- `crates/touring-server/tests/cli_smoke.rs` — **NOVO** arquivo dedicado para smoke tests

**Riscos**: Alguns comandos exigem daemon rodando — usar `--help` e comandos stateless primeiro; comandos com dependência marcar `#[ignore = "requires daemon"]`.

**Validação**: `cargo test -p touring-server --test binary_e2e --test cli_smoke` → 35+ passing.

---

#### #2 hdrhistogram — Latency Percentile Tracking [INFERENCE 0.9, Score 0.90]

**Problema**: `gate_metrics.rs` tem 31 `AtomicU64` counters que tracam totais/contagens, mas dividem soma/count para "mean". Perde distribuição — **não detecta tail latency spikes** (P99 > P50 × 10). Viola invariante "P50 = 1ms" sem observability real.

**Solução**: Adicionar `hdrhistogram` dep + envolver 5-6 counters críticos em `Histogram<u64>` (sharded para concorrência).

**Counters críticos para histogramar**:
1. `rkyv_dispatch_bytes` → **`rkyv_dispatch_latency_us`** histograma (1μs-60s range)
2. `pre_edit_latency_us` (NOVO) — L7-B gate duration
3. `pre_write_latency_us` (NOVO) — speculate validation duration
4. `tantivy_query_latency_us` → já existe como counter, converter para histogram
5. `hook_dispatch_latency_us` (NOVO) — daemon ProjectCommand::RunHook duration

**API target** (`gate_metrics.rs`):
```rust
pub struct GateMetrics {
    // ... existing counters ...
    pub rkyv_dispatch_latency: LatencyHistogram,  // NOVO
    pub hook_dispatch_latency: LatencyHistogram,
    pub tantivy_query_latency: LatencyHistogram,
}

pub struct LatencyHistogram {
    inner: Mutex<Histogram<u64>>,  // Sync boundary
}

impl LatencyHistogram {
    pub fn record_us(&self, micros: u64) { /* ... */ }
    pub fn snapshot(&self) -> LatencySnapshot { /* p50/p90/p99/p999/max */ }
}

pub struct LatencySnapshot {
    pub count: u64,
    pub p50_us: u64,
    pub p90_us: u64,
    pub p99_us: u64,
    pub p999_us: u64,
    pub max_us: u64,
}
```

**CLI exposure**: `touring gate-metrics -j` já existe — estender output:
```json
{
  "rkyv_dispatch": {
    "count": 1234, "p50_us": 12, "p99_us": 187, "max_us": 2043
  }
}
```

**Esforço**: L2 (~2-3h)
**Arquivos**:
- `crates/touring-hooks/Cargo.toml` — add `hdrhistogram = "7"`
- `crates/touring-hooks/src/shared/gate_metrics.rs` — add `LatencyHistogram` struct + methods
- `crates/touring-server/src/cli/gate_metrics.rs` — expose novas métricas em JSON

**Riscos**: `Mutex<Histogram>` introduz lock. Alternativa: `hdrhistogram::sync::SyncHistogram` ou `SampleHistogram` com recording sampling. Decisão: iniciar com simples Mutex; profile se for gargalo (provavelmente não em <1M records/s).

**Validação**: `cargo test -p touring-hooks --lib shared::gate_metrics` — novos tests para record + percentile.

---

### 🟡 P1 — Considerar em Fase Seguinte

#### #5 addr2line — Structured Panic Backtraces [INFERENCE 0.75, Score 0.75]

**Problema**: `post_tool_failure` hook captura panics, mas backtrace fica mangled (hex addresses) sem resolução para `file.rs:line`. Gotcha DB fica pobre, erros recorrentes difíceis de diagnosticar.

**Solução**: Integrar `addr2line` + `gimli` (já na tree via `backtrace` crate) para resolver addresses → `file:line`. Armazenar resolved backtrace em gotcha entries.

**Esforço**: L2 (~3h)
**Pré-requisitos**: Verificar estado atual de `post_tool_failure` hook + gotcha DB schema.
**Arquivos**:
- `crates/touring-hooks/src/post_tool_failure.rs` — resolver backtrace antes de persistir
- `crates/touring-hooks/Cargo.toml` — add `addr2line` + `object`

---

#### #4 inventory — Auto-registered Hooks [INFERENCE 0.80, Score 0.80]

**Problema**: `hook_registry.rs` tem ~138 entries mantidas manualmente. Cada novo hook exige edição dupla (definir função + registrar entry). Bug-prone.

**Solução**: `inventory::submit!` para auto-registration. Hooks se registram via proc-macro ou manual `submit!`. Runtime coleta via `inventory::iter::<HookEntry>`.

**Esforço**: L3 (~1 dia, blast_radius alto)
**Riscos**: Refactor invasivo, exige cautela + full test suite pós-mudança. **Agendar em sessão dedicada**.

---

### 🟢 P2/P3 — Skip ou Investigar Mais

#### #3 cfg_aliases [SCORE REBAIXADO 0.50]

**Scout revelou**: Apenas 2 `cfg(all(feature))` em todo `touring-server/src/`. O valor antecipado (limpeza de features verboso) **não se materializou** — o codebase já está razoavelmente limpo.

**Decisão**: **Skip**. Reavaliar se `cfg(all(...))` count subir >10.

---

#### #6 proc-macro-crate [Score 0.70 → N/A]

**Scout revelou**: Touring não define proc-macros internas. `#[tool]` vem do rmcp (dep externa). `proc-macro-crate` serve para desenvolvedores de proc-macros resolverem paths — não se aplica.

**Decisão**: **N/A**. Remover do ranking.

---

#### #7 cargo-make [SPECULATION 0.65]

**Avaliação**: `.cargo/config.toml` já tem aliases. Sem evidência de Makefile chaos. Marginal ROI.

**Decisão**: **Skip**. Reavaliar se build orchestration crescer.

---

## Ordem de Implementação P0

```
1. Adicionar deps (assert_cmd, predicates, hdrhistogram) em Cargo.toml
   ↓
2. Implementar LatencyHistogram em gate_metrics.rs + unit tests
   ↓
3. Expandir binary_e2e.rs com 20 comandos críticos
   ↓
4. cargo test --workspace → validar 0 regressions
   ↓
5. Atualizar gate-metrics CLI JSON output
   ↓
6. Documentar em CHANGELOG
```

## Critérios de Sucesso

- ✅ `cargo check --workspace` clean
- ✅ `cargo clippy -p touring-server -- -D warnings` → 0 warnings
- ✅ `cargo clippy -p touring-hooks -- -D warnings` → 0 warnings
- ✅ `cargo test -p touring-server --test binary_e2e` → 35+ passing
- ✅ `cargo test -p touring-hooks --lib shared::gate_metrics` → novos tests passam
- ✅ `touring gate-metrics -j` retorna P50/P99 em 3+ métricas
- ✅ Zero regressions em 5.154 tests existentes

## Estimativa Total

- P0 (#1 + #2): **6-9h** (assert_cmd 4-6h + hdrhistogram 2-3h)
- P1 (#4 + #5): **~16h** (sessão separada)
- P2/P3: skipped

## Memory Persistence

```bash
touring memory store "plan:dev-tools-2026-04-17" \
  "P0: assert_cmd (CLI E2E) + hdrhistogram (latency P99). cfg_aliases rejeitado (scout: só 2 cfg(all)). proc-macro-crate N/A. cargo-make skip." \
  --tier semantic --type pattern
```
