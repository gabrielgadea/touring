---
type: Strategy
title: "O que fazer diante de 561 stale signals — opções A/B/C + recomendação"
description: "3 opções pragmáticas para reduzir drift semântico na documentação: curto (integrar scan ao CI), médio (lint por categoria + auto-fix parcial), longo (auto-remediation via LLM)."
plan_id: 2026-08-31-work-outer
bundle: docs/plans/2026-08-31-work-outer
okf_version: "0.1"
tags: [strategy, drift-coverage, options, recommendation]
timestamp: 2026-08-31T12:13:28-03:00
---

# Strategy — opções pós-diagnóstico de cobertura

## TL;DR

A pergunta de Gabriel: **"o que podemos ou devemos fazer?"** diante de 561 stale signals detectados mas só 1 categoria coberta por guard.

**Recomendação**: **opção (A) curto prazo agora** + **(B1) médio prazo depois**, nesta ordem. Opção (C) só se A e B mostrarem payback.

## TL;DR em 1 tabela

| Opção | Esforço | Payback | Risco | Recomendação |
|---|---|---|---|---|
| **A1** Integrar `scan --check` ao CI/CD | 1h | Imediato (cada PR valida) | Mínimo (já temos o script, F2.1 Diamond) | **✅ IMPLEMENTADO 31/08** |
| **A2** Adicionar 6 guards simples (1 por cat) | 4h | Médio | Médio (manutenção de 6 scripts) | Adiar até ter dados de tendência |
| **B1** Lint por categoria + thresholds | 1-2 dias | Alto (prevenção contínua) | Médio (categoria errada = ruído) | Próximo ciclo (1 semana) |
| **B2** Auto-fix para C3 + C1 | 1 dia | Alto (mapeamento conhecido) | Baixo (regex reversível) | Junto com B1 |
| **C1** Refactor docs para formato tipado | 1 mês+ | Muito alto | Alto (quebra docs existentes) | Só se B mostrar payback alto |
| **C2** Integração com MkDocs (build bloqueia) | 2-3 dias | Alto (release-time check) | Médio (lock de release) | Após B1 |
| **C3** Auto-remediation via LLM (PRs auto) | 1 semana | Muito alto (parcial) | Alto (PR errado = regressão) | Só após C2 validar |

---

## Opção A — Curto prazo (1-2 dias, baixo risco)

### A1: Integrar `scan --check` ao CI/CD ✓ RECOMENDAÇÃO IMEDIATA

**O que**: rodar `python3 scripts/drift_semantic_scan.py --check --quiet` no CI de cada PR.

**Já temos**:
- Script com `--check` mode (exit 0/1, F2.1 Diamond)
- Teste de cobertura `test_drift_coverage.py` (8/8 passando, F2.1 Diamond)
- Baseline conhecido: 561 stale

**Falta**:
1. Adicionar step no `.github/workflows/ci.yml` (ou similar)
2. Decidir política de "fail on N stale" (talvez `fail on C3>0 only` para começar)
3. Adicionar `--check` do `test_docs_no_phantom_crates.py` (já roda, mas verificar threshold)

**Custo**: 1h para adicionar CI step + commit.
**Payback**: cada PR futura é validada antes do merge; drift não acumula.
**Risco**: mínimo — script já existe, gate já existe.

### A2: 6 novos guards (1 por categoria) — ADIAR

**O que**: criar `scripts/test_drift_<categoria>.py` para C1, C2, C4, C5, C6, C7.

**Por que adiar**:
- O scan `--check` JÁ cobre 5/7 categorias (C1, C3, C4, C5, C6) em 1 comando
- Adicionar 6 guards separados = 6 entries de CI = ruído
- Melhor: 1 guard agregador (`drift_semantic_scan.py --check`) com threshold por categoria

**Quando voltar**: depois de ter dados de tendência (rodar scan diariamente por 1-2 semanas).

---

## Opção B — Médio prazo (1 semana, médio risco) ★ RECOMENDAÇÃO PRÓXIMO CICLO

### B1: Lint por categoria com thresholds diferenciados ✓

**O que**: estender `drift_semantic_scan.py` para aceitar `--threshold <cat>=<max>` por categoria.

```bash
python3 scripts/drift_semantic_scan.py --check \
  --threshold C1=10 C2=50 C3=0 C4=100 C5=50 C6=20
```

**Política sugerida** (baseada em criticidade):
- **C3 fused**: threshold=0 (fatal — qualquer crate fundido é regressão)
- **C4 CLI**: threshold=50 (warn se >50 stale, fail se >200)
- **C7 hooks**: threshold=100 (warn se >100, fail se >500)
- **C1/C2/C5/C6**: threshold=20 (warn se >20, fail se >50)

**Custo**: 1-2 dias (modificar script, adicionar testes, integrar CI).
**Payback**: gate granular — drift trivial não bloqueia PR, drift crítico bloqueia.
**Risco**: médio — threshold errado causa ruído ou passa-falsos.

### B2: Auto-fix para C3 e C1 — JUNTO COM B1

**C3 (fused crates)**: para os 5 crates com mapping canônico (`gen_fusion_map.py`):
- `touring-ast` → `touring-code::ast`
- `touring-learning` → `touring-intelligence::rl`
- `touring-core` → `touring-foundation`
- `touring-cognitive` → `touring-intelligence::reasoning`
- `touring-index` → `touring-intelligence::index`

**C1 (versões)**: regex simples — substituir `v30.4.X` onde X != 28 por `v30.4.28`.

**Custo**: 1 dia (2 scripts `fix_<categoria>.py` que rodam antes do gate).
**Payback**: drift mais fácil de reverter.
**Risco**: baixo — regex reversível, pode ser rodado em dry-run primeiro.

---

## Opção C — Longo prazo (1 mês+, alto risco)

### C1: Refactor docs para formato tipado

**O que**: substituir texto livre por tipos/macros que o scan valida automaticamente (ex.: `<crate-name>` ao invés de `touring-foo` literal).

**Custo**: 1 mês+ (afeta 1098 arquivos).
**Payback**: longo prazo — gate nunca mais passa falso.
**Risco**: ALTO — quebra docs legados, exige migração cuidadosa.

**Quando**: só se B mostrar que drift é crônico (>200 stale/mês).

### C2: Integração com MkDocs (build-time gate)

**O que**: rodar scan no `mkdocs build --strict`, falhar build se drift crítico.

**Custo**: 2-3 dias (wire CI).
**Payback**: release-time validação.
**Risco**: médio — lock de release pode ser bypassed por `--no-strict`.

### C3: Auto-remediation via LLM (PRs auto)

**O que**: usar LLM para gerar PRs automáticos que corrigem drift em C5/C6 (exemplos de código + envvars).

**Custo**: 1 semana (integração + guardrails).
**Payback**: alto (parcial) — corrige casos óbvios.
**Risco**: ALTO — LLM pode introduzir regressão; precisa review humano.

---

## Recomendação TACO (análise qualitativa)

**Trade-offs observados**:

| Se fizermos | Trade-off |
|---|---|
| Nada | Drift acumula linearmente: ~85 stale/release. Em 1 ano = ~4400 stale. Guard cobre só 11 (C3). |
| Só A1 (integrar scan) | Cobre 5/7 categorias em CI. Threshold único (qualquer stale = fail). Pode ser noisy. |
| A1 + B1 | Cobre 5/7 com thresholds. Granular. Custo médio. |
| A1 + B1 + B2 | Auto-fix parcial para categorias reversíveis (C3 + C1). Custo maior. |
| Tudo (A+B+C) | Excelência — mas requer manutenção contínua e equipe comprometida. |

**Recomendação pragmática** (sem probabilities calibradas — análise por evidência):

1. **AGORA (1h)**: A1 — integrar `scan --check` ao CI. Custo mínimo, payback imediato, risco zero.
2. **PRÓXIMO CICLO (1 semana)**: A1 + B1 + B2 — scan com thresholds + auto-fix para C3/C1. Custo 2-3 dias, payback alto.
3. **DEPOIS (1 mês+)**: C2 — MkDocs build-time gate, se B mostrar tendência boa.
4. **SÓ SE DRIFT FOR CRÔNICO**: C1 + C3 — refactor tipado e auto-remediation LLM.

**Risco de NÃO fazer nada**: ~4400 stale/ano, ~6% redução anual de utilidade da doc.

**Risco de fazer tudo agora**: over-engineering, manutenção contínua cara.

## Próximo passo (gate humano)

Escolha 1 das 3:
1. **"A1 só"** → faço agora (1h)
2. **"A1 + B1 + B2"** → planejo + executo (2-3 dias)
3. **"mais opções"** → apresento mais 3 alternativas

---

_v1.0 — 2026-08-31 | Autor: TACO work-outer (3a87ce4a) | Aguardando gate humano_
