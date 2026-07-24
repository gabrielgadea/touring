# Wave C Wiring Completion — Session Report

> **Data**: 2026-04-20 (late) → 2026-04-21 | **Status**: ✅ COMPLETO
> **Autor**: TACO (Claude Code em modo Touring) | **Daemon**: v30.3.0
> **Contexto**: Fecha o ciclo edit → análise → cascade queue → decomposição adaptativa

---

## Executive Summary

Todos os 6 deliverables D1–D6 do plano `2026-04-20-wave-c-wiring-plan.md` foram
implementados e validados. O ciclo end-to-end entre `post_edit`, `api_cascade_bridge`,
`CascadeQueue`, `GranularityBandit` e `TaskDecomposer` está completo e testado.

---

## Arquitetura — Callback Injection vs Shared State

**Decisão central**: O ciclo usa **callback injection** (query pattern) ao invés de
**shared state** entre `touring-hooks` e `touring-server`. Reasons:

| Aspect | Callback Injection | Shared State |
|--------|-------------------|--------------|
| Coupling | Loose (JSON over socket) | Tight (same process) |
| Failure isolation | Crash in one doesn't corrupt the other | Shared memory corruption risk |
| Serialization | Natural boundary (daemon ↔ server) | Requires careful lock discipline |
| cold-start | Each query is independent | Must initialize together |

`HookRuntime` (touring-hooks) e `TaskDecomposer` (touring-server) vivem em crates
desacoplados. A ponte é o **Unix socket daemon** com handlers registrados no
`hook_registry`.

---

## Deliverables Implementados

| D | Descrição | Arquivo | Status | Tests |
|---|-----------|---------|--------|-------|
| D1 | C2-post_edit inline wiring | `post_edit.rs:286-294` | ✅ | 4 e2e |
| D2 | GranularityBandit query adapter | `granularity_adapter.rs` | ✅ | 8 server |
| D3 | TaskDecomposer consome GranularityHint | `decomposer.rs` + `tools_analysis.rs` | ✅ | 8 server |
| D4 | Cascade queue bridge | `cascade_queue.rs` + `tools_analysis.rs:714-789` | ✅ | 6 lib + 4 e2e |
| D5 | CLI + MCP cascade observability | `cli/cascade.rs` + handlers | ✅ | registry |
| D6 | Integration E2E + docs | `wave_c_e2e.rs` + este doc | ✅ | 8 e2e |

---

## Ciclo End-to-End

```
┌─────────────────────────────────────────────────────────────────┐
│ post_edit hook (touring-hooks)                                   │
│                                                                  │
│  1. analyze_rust_edit(path, src, &cache)                        │
│     → AnalysisOutcome::FirstObservation | Diffed | NotRust      │
│                                                                  │
│  2. Se Diffed + plan.high_severity():                           │
│     → CascadeQueue::push(path, plan)  (D4)                      │
│                                                                  │
│  3. CLI / MCP: touring_decompose create --auto-decompose         │
│     → tools_analysis.rs: create_task_with_cila_and_hint()       │
│     → GranularityBandit query (D2+D3)                           │
│                                                                  │
│  4. MCP: touring_decompose drain_cascades (D4)                  │
│     → daemon socket → cli-cascade-queue-drain handler           │
│     → CascadeQueue::drain_fresh()                               │
│     → TaskDecomposer::add_subtask() para cada proposal         │
└─────────────────────────────────────────────────────────────────┘
```

---

## Hook Count Evolution

| Data | Hook Count | Delta | Observação |
|------|-----------|-------|-----------|
| 2026-04-12 (pre) | 113 | — | baseline |
| 2026-04-14 (rkyv) | 138 | +25 | Wave 3: rkyv-ipc + tantivy + wiring-community |
| 2026-04-18 | 138 | 0 | Wave 24: pre_task_scout wired (sem novo hook) |
| 2026-04-20 (D5) | 146 | +8 | Wave C: cascade queue handlers + granularity hint |

`ALL_DAEMON_HOOK_NAMES.len() == 146` (registrado em `hook_registry.rs:1017`)

---

## Test Coverage

| Suite | Tests | Status |
|-------|-------|--------|
| `touring-hooks --lib` | 3024 passed | ✅ |
| `touring-server` | 502 passed | ✅ |
| `touring-integration-tests --test wave_c_e2e` | 8 passed | ✅ |
| `touring-hooks --test post_edit_cascade_e2e` | 4 passed | ✅ |
| `touring-hooks --test cli_handlers_e2e` | 12 passed | ✅ |
| `touring-integration-tests` (full) | 12 passed | ✅ |

---

## D4 drain_cascades — Implementation Detail

O action `drain_cascades` em `tools_analysis.rs` foi implementado com **socket
direto** ao daemon (inline `UnixStream`) ao invés de usar `daemon_query` do modulo
`cli`, porque `daemon_query` é `pub(crate)` em `src/main.rs` e não exportado para
o módulo `lib`. O pattern é:

```rust
let request = serde_json::json!({
    "hook": "cli-cascade-queue-drain",
    "payload": {},
    "project_root": std::env::current_dir()...
});
// write JSON → \n → flush → read_to_end → parse JSON response
```

Retorna `{drained_count, subtasks_added, stale_evicted, target_task_id}`.

---

## Metrics — Gate Counters (D5 Observability)

```bash
touring gate-metrics -j | jq '{
  blast: .blast_inject_count,
  cascade_queue: .cascade_queue_*,   # se existir
  linucb: {manual:.linucb_route_manual_count, generator:.linucb_route_generator_count}
}'
```

---

## Lessons Learned

1. **Inline socket** vs `daemon_query`: `pub(crate)` em `main.rs` não é acessível do
   contexto `lib` do touring-server. Solução: inline direct socket em `tools_analysis.rs`.

2. **`std::env::var("UID")` vs `libc::getuid()`**: `libc` crate não estava em
   dependências de touring-server.UID vem de ambiente com fallback "1000" (valor
   típico para primeiro usuário Unix).

3. **CascadeQueue TTL**: 1h é suficiente para sessões típicas; items stale são
   evictados automaticamente em `drain_fresh()`.

4. **hook_registry count**: cada handler adicionado requer update em 3 pontos
   simultâneos (2 arrays + 1 count assertion) para evitar test breakage.

---

## Pending Items (from 01-suggestions.md)

| Item | Status | Complexity |
|------|--------|------------|
| Speculative Validation via SMT/Concolic | ❌ PENDENTE | L4+ |
| Auto-Cura: Concolic SMT + AST Surgery | ❌ PENDENTE | L4+ |
| rkyv IPC SagaOrchestrator | ✅ IMPLEMENTADO — PLN2 DistributedSagaCoordinator (2026-04-21) | L3 |
| DSPy guided by CRDT | ❌ PENDENTE | L4 |
| Telemetry Predictive Circuit Breaker | ❌ PENDENTE | L3 |
| RL feedback in offensive motor | 🔶 PARCIAL — ciclo quebrado (2026-04-21) | L3 |

---

## Files Changed

```
crates/touring-hooks/src/
  + shared/cascade_queue.rs       (D4, novo)
  + shared/mod.rs                 (D4, +pub mod cascade_queue)
  + hook_runtime.rs               (D1, +api_cascade_cache field)
  + post_edit.rs                 (D1, +cascade push)
  src/cli_handlers.rs            (D5, +cascade handlers)
  src/hook_registry.rs           (D5, +8 entries → 146 total)

crates/touring-server/src/
  + reasoning/granularity_adapter.rs  (D2, novo)
  + cli/cascade.rs                   (D5, novo)
  + cli/mod.rs                       (D5, +pub mod cascade)
  src/server/tools_analysis.rs        (D3+D4, +drain_cascades action)
  src/reasoning/decomposer.rs         (D3, +create_task_with_cila_and_hint)

crates/touring-integration-tests/tests/
  + wave_c_e2e.rs                (D6, 8 tests)

docs/
  + 2026-04-20-wave-c-wiring.md (este arquivo)
  + 2026-04-20-wave-c-wiring-plan.md (plano)
```

**Total: ~500+ linhas de código novo, 3024+9+12+8 = 3053+ testes passando.**

---

*Report gerado em 2026-04-21 pelo TACO v6.2 — Wave C wiring completion.*
