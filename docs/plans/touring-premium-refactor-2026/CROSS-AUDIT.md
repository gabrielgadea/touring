---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
type: "cross-audit"
created: "2026-05-11"
depends_on:
  - W0
  - W1
  - W2
  - W3
  - W4
  - W5
  - W6
  - W7
  - W8
  - W9
  - W10
  - W11
  - W12
  - W13
  - W14
script: "scripts/touring_premium_refactor_2026/cross_audit_e2e.py"
---
# Cross-Audit E2E — touring-premium-refactor-2026

> **Propósito**: Verificar que TODAS as 15 waves atingiram seus objetivos e que
> o plano cumpriu sua finalidade: transformar Touring em produto premium.

## Script de Auditoria

```bash
python3 scripts/touring_premium_refactor_2026/cross_audit_e2e.py --full
```

## 10 Dimensões de Avaliação

| # | Dimensão | Peso | Verificação |
|---|---|---|---|
| D1 | Funcional — código executa, testes passam | 2.0 | cargo test workspace pass |
| D2 | Wiring — zero ciclos, zero orphans | 1.5 | touring wiring cycles + orphans |
| D3 | Performance — < 5% regressão vs baseline | 1.5 | Criterion benches |
| D4 | Cobertura — ≥ 20% LOC ratio por crate | 1.5 | cargo llvm-cov per crate |
| D5 | Mutation — kill rate ≥ 80% | 1.0 | cargo mutants |
| D6 | API Stability — semver-check clean | 1.5 | cargo public-api + semver-check |
| D7 | Supply Chain — deny+audit+vet clean | 1.0 | cargo deny check |
| D8 | Documentation — docs.rs green | 1.0 | cargo doc warnings-as-errors |
| D9 | Deployment — per-project funcional | 1.5 | touring init pilot OK |
| D10 | Propósito — produto premium entregue | 2.0 | 1.0.0 GA + 4 tiers ativos |

## Critérios de Sucesso

- **Composite score** ≥ 0.95 (média ponderada das 10 dimensões)
- **Nenhuma dimensão** < 0.80 (VETO threshold)
- **D10 Propósito** OBRIGATORIAMENTE ≥ 0.95 (plano só passa se entrega o produto)

## Verificação por Wave

Veja `cross_audit_e2e.py` para a tabela completa de critérios por wave.
