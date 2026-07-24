# Schema Freeze — touring-quality JSON contract v1 (pré-reforma)

> Capturado 2026-07-02 (W0.5.1). O contrato abaixo é o que os 10 consumers
> (ver `consumers-inventory.txt`) recebem hoje. A reforma bumpa para
> `schema_version: 2` em W7, com os consumers atualizados em lockstep.

## Contrato v1 (`score`/`check --format json`)

```json
{
  "target": "<path>",
  "dimensions": {
    "F1_1": {
      "value": 1.0,            // f32 0.0-1.0
      "status": "Pass",        // Pass (>=0.8) | Warn (0.5-0.8) | Fail (<0.5)
      "evidence": "<string>",
      "suggestions": ["<string>"],
      "latency_ms": 0           // hardcoded 0 hoje
    }
  },
  "composite": 1.0,             // média ponderada linear (Block 2.0 / Warn 1.5 / Advisory 1.0)
  "tier": "Diamond",            // >=0.95 Diamond / >=0.90 Platinum / >=0.80 Gold / >=0.70 Silver / >=0.60 Bronze / Unranked
  "blockers": [],
  "warnings": [],
  "suggestions": [],
  "total_latency_ms": 0,
  "schema_version": 1
}
```

## Mudanças planejadas para v2 (W3/W5)

1. `status` ganha `NotApplicable` (dim × linguagem não-aplicável) — **fora do denominador** do composite.
2. `composite` deixa de ser média linear → quality-gate condicional (worst-of P0 + mediana WARN); campo novo `gate_conditions: [{condition, pass}]`.
3. `dimensions.*.language` — linguagem real detectada (fim do rótulo rust em blob).
4. `dimensions.*.artifacts` — para dims de artefato, os paths resolvidos em disco (vazio = ausência → step-function).

## Consumers a atualizar em W7 (lockstep)

Ver `consumers-inventory.txt`: touring-{ceg,cortex,lsp,server} (Cargo deps), hooks
`touring-quality-block-all.sh` + `touring-quality-f2-5-block.sh`, `elite_aggregate.py`,
`harness_gate.py`, `lib_touring.py`, `settings.json` (hook wiring).

## Baseline de regressão

`baseline-scores.json` — 8 alvos × 50 dims (valores pré-reforma). Uso: diff pós-wave;
mudanças devem ser **explicáveis pelo changelog da wave** (correção esperada), nunca
colaterais silenciosas.
