---
type: LoopBundle
title: "Gortex → Touring — exploração e extração de insights"
description: "Bundle OKF da exploração multi-rodada do zzet/gortex, com os insights transferíveis ranqueados por alavanca e as verificações executadas no Touring."
plan_id: 2026-08-07-gortex-insights
tags: [exploration, gortex, benchmarking-externo]
timestamp: 2026-08-07T18:10:00-03:00
okf_version: "0.1"
---

# Bundle — Gortex → Touring

Exploração de [`zzet/gortex`](https://github.com/zzet/gortex) — engine de code-intelligence
para agentes (Go, Apache-2.0, 1.1k★), o análogo mais direto do Touring que existe público —
com extração de insights de melhoria, refinamento e evolução.

## Documentos

| Documento | Conteúdo |
|---|---|
| [exploration-gortex.md](exploration-gortex.md) | **Dossiê principal** — 2 rodadas. **R1**: 13 insights em 3 tiers, 6 anti-adoções, 4 superioridades do Touring, a tese central. **R2**: código-fonte + prática de engenharia + trajetória de releases; 2 correções à R1, 2 novos tier-S, 3 novos tier-A, 7 novos tier-B |

## Implementação

| Fase | Entregue | Prova |
|---|---|---|
| [P1](phases/P1-implementacao.md) | V4 walker · S5 truncagem · S3 eval de retrieval · S2 baseline+epsilon | 2133 testes, R@k medido por tier |
| [P2–P4](phases/P2-P4-proveniencia-memoria-politica.md) | S1 proveniência · S4 feromônio · B7 viés · A7b compactação | 27 testes novos; flaky pré-existente corrigido |
| [P5](phases/P5-rewrite-schema-classificacao.md) | A4 `rewrite` · C1 schema · classificação da dívida · 2 causas-raiz de migração | 17 testes novos; 15.281 na suíte, 0 falhas |
| [P6](phases/P6-tokens-reais-e-clones-type2.md) | A2 tokens reais (bytes exatos + cl100k) · A1 MinHash/LSH Type-2 · hook PostCompact · paridade C08 · **5 defeitos** que só a medição expôs | 39 testes novos; 15.312 na suíte, 0 falhas |
| [strategy](strategy-2026-08-07-implementacao.md) | consolidação OUTER + contenção de risco declarada | diagnóstico 0,9386 Platinum |

**Aberto (desenho pronto, sem meia-implementação)**: A5, A6.
**Registrado sem ação**: A3 (GCX1), B1, B3–B6, B8–B14.

**Decisões resolvidas pelo Gabriel (08/08, "resolva tudo")**:

| # | Decisão | Resultado |
|---|---|---|
| 1 | Type-2 na nota do F1.3 | **entra**, via banda própria; `F1.3 = min(type1, combinado)`. F1.3 0,592 → 0,509; composite 0,939 → 0,936 (Platinum) |
| 2 | Registrar `pre-tool-use` | **não registrado** — mina desarmada (`envelope_as_tool_input`), recusa fundamentada em 3 fatos verificados |
| 3 | Disco 94% / `target/` 279 GB | `safe-clean sweep` executado após o deploy |

## Artefatos de exploração

| Artefato | Local |
|---|---|
| Ledger CCE (6 lentes, `converged: True`, 2 visitas externas) | `.touring-explore/gortex-code-intelligence-engine---insights-trans.ledger.json` |
| Lição R1 · Lição R2 (memory, tier semantic) | `lesson:gortex-external-benchmark-2026-08-07` · `lesson:gortex-round2-2026-08-07` |

## Resultado em uma linha

O Gortex tem resposta **medida** para as fraquezas que verifiquei no Touring — e várias são
baratas porque a infraestrutura já existe e está desligada: `wiring_map.contract_source` é
constante `ast_read` em 77.679 de 77.679 linhas (S1), e a memória não tem supersessão nem
peso, então o feromônio ACO nunca evapora (S4).

**Achado central da rodada 2**: a honestidade do Gortex é **prática sistêmica** — *todo
cálculo limitado anuncia o próprio limite* — com 5 instâncias observadas. E eu cheguei à
mesma regra hoje, sozinho, ao adicionar o anúncio de truncagem ao harness 50-dim. Design
convergente sob pressão idêntica.

## Cross-links

- Auditoria cruzada da mesma sessão: [/docs/audits/cross-audit-2026-08-07.md](../../audits/cross-audit-2026-08-07.md)
- Bundle da migração rkyv (onde `orphans_base` está travada): [/docs/plans/2026-08-07-rkyv-migration/index.md](../2026-08-07-rkyv-migration/index.md)
