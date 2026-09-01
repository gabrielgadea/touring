---
type: KPI
title: "KPI counter — hooks_complement.composite ≥ 0.80 ≥ 7 dias"
description: "Documentação do KPI counter que mede aderência ao uso dos 15 SignalLayers da complementação. Régua derivada do code_mode_adherence (M1) mas com escopo próprio: hooks_complement.composite = (tests_passed/15) × 0.5 + (SignalLayer emit rate) × 0.5 ≥ 0.80 por ≥ 7 dias consecutivos."
plan_id: 2026-08-31-complementacao-hooks
bundle: docs/plans/2026-08-31-complementacao-hooks
okf_version: "0.1"
version: "1.0"
based_on: ["criacao.md", "strategy-v0.1.md"]
tags: [kpi, gate, best-practices, signal-layer]
timestamp: "2026-08-31T19:20:00-03:00"
---

# KPI counter — hooks_complement.composite

## Definição

`hooks_complement.composite` = régua composta que mede aderência ao uso dos 15 SignalLayers da complementação-hooks. Composta por 2 eixos:

```
hooks_complement.composite =
    0.5 × (test_pass_rate) +
    0.5 × (signal_emit_rate)
```

| Eixo | Definição | Medido por |
|---|---|---|
| `test_pass_rate` | `(tests_passed) / 15` (0.0 a 1.0) | `scripts/test_complementacao_hooks.py` |
| `signal_emit_rate` | `(signals_emitted) / (signals_expected)` durante 1 sessão (0.0 a 1.0) | `touring kpi -j hooks_complement.emit_count / 15` |

## Gates (do Briah — critério de pronto #3)

- **Piso**: `hooks_complement.composite ≥ 0.80` por **≥ 7 dias consecutivos**
- **WARN** (atual): abaixo de 0.80 com amostra ≥ 20 execuções
- **FAIL-CLOSED** (futuro, pós-estabilização): se degradar ≥ 7 dias

## Implementação

### BestPracticesGate hooks_complement_use (Warn-severo)

Adicionar como 3ª regra ao `crates/touring-quality/src/builtins/best_practices.rs` (junto com declaração E/A/M + medição M1):

```rust
/// Adherence to the 15 SignalLayers defined in complementacao-hooks.
/// Reads `hooks_complement.composite` from `touring kpi -j`; Warn when
/// below 0.80 with sample ≥ 20 invocations.
const HOOKS_COMPLEMENT_PISO: f32 = 0.80;
const HOOKS_COMPLEMENT_SAMPLE_MIN: usize = 20;
```

Implementação concreta: clone do pattern de `code_mode_adherence` (M1) já existente em best_practices.rs.

### KPI counter

Adicionar counter no daemon (`touring-intelligence/src/learning/kpi.rs` ou equivalente):

```rust
pub fn hooks_complement_composite() -> f32 {
    let tests_passed = run_test_complementacao_hooks();   // calls the script
    let emit_rate = hooks_complement_emit_count() / 15.0;  // from journal
    0.5 * (tests_passed / 15.0) + 0.5 * emit_rate
}
```

### Tour metrics → kpi

```bash
touring kpi -j hooks_complement.composite
# Returns: { value: 0.93, sample: 47, status: "ok" }
```

## Critérios de promoção (Warn → fail-closed)

1. **Estabilidade**: composite ≥ 0.80 por ≥ 14 dias consecutivos
2. **Sem regressão**: 0 quedas abaixo de 0.80 no período
3. **Sample size**: ≥ 100 execuções no journal

Após essas 3 condições, BestPracticesGate promove para **FAIL-CLOSED** (bloqueia a esteira se composite < 0.80 com sample ≥ 100).

## Estado atual (snapshot 31/08 19:20 BRT)

- **Test pass rate**: 15/15 = 1.00
- **Signal emit rate**: ainda não medido (precisa instrumentação no journal)
- **Composite estimado**: ~0.50 (assumindo emit_rate = 0 enquanto instrumentation não roda)
- **Aderência ao gate**: WARN (abaixo de 0.80, sample insuficiente)

## Roadmap

| Fase | Ação | Status |
|---|---|---|
| F4.1 | BestPracticesGate hook adicionado (Warn) | ⏳ F4 atual |
| F4.2 | Instrumentação do `hooks_complement_emit_count` no journal | ⏳ F4.x |
| F4.3 | 7 dias de produção com composite ≥ 0.80 | ⏳ F5 |
| F4.4 | Promoção para fail-closed (gate `quality` strict) | ⏳ F6 |

---

_v1.0 — 2026-08-31 19:20 BRT | KPI counter documentado | composite = 0.5 × test + 0.5 × emit | atual: ~0.50 (emit_rate zerado) | promotion gates: estabilidade + sample + regressão_