---
type: Plan
title: Contexto por chamada — medir e reduzir o contexto que cada chamada relê (inspiração billion-context)
plan_id: 2026-10-17-contexto-por-chamada
timestamp: 2026-10-17T17:45:00Z
dag: task_1792258981432513400
aprovado_por: Gabriel («abra P1, P2 e P3 no DAG como frente nova», 17/10/2026)
tags: ["#kind:plan", "#status:active", "#domain:contexto", "#process:measurement"]
---

# Contexto por chamada

## Por que (medido em 17/10/2026, transcripts dos últimos 14 dias, dedup por `message.id`)

O custo e a degradação das sessões NÃO vêm do cache nem das saídas de ferramenta, e sim do tamanho do contexto que cada
chamada relê:

| | touring | analise |
|---|---|---|
| entrada total, 14 dias | 5,31 bi tokens | 14,78 bi tokens |
| acerto de cache, Claude principal | 99,1% | 99,1% |
| contexto por chamada, Claude principal (mediana / p90 / máx, mil tokens) | 431 / 749 / 966 | 415 / 681 / 966 |
| contexto por chamada, subagente Claude (mediana) | 336 | 289 |
| saídas de ferramenta (total · p95 · forma JSON) | 20 MB · 5,7 KB · 4% | 100 MB · 8 KB · 6% |
| sessões com ≥ 1 compactação | 6/28 | 26/167 |

Medição: `prototipos/medir_relevancia.py` e `prototipos/medir_contexto_por_chamada.py` (cópias dos scripts da sessão
efe418c2; o P1 os transforma em script versionado com testes). Linha de base de compactação: o resumo cita 53% dos arquivos
alterados (129 compactações, 17/09–08/10, CLAUDE.md global §Compact Instructions).

Origem das ideias: análise do `ranxianglei/billion-context` (memória `analise-billion-context-2026-10-17`). O proxy NÃO
será instalado (credencial e tráfego por código de auto-merge, toma o `settings.json`, desliga o autocompact, termos da
assinatura não verificados). Só as ideias entram.

## Nós (DAG `task_1792258981432513400`)

| Nó | O quê | Inspiração | Toca | Depende |
|---|---|---|---|---|
| P1 | KPI de contexto por chamada + atribuição de perda de cache (TTL ocioso, troca de modelo, troca de chave) | `cache-ledger` | script novo + `docs/kpi/commitments.yaml` | — |
| P2 | aviso de estado-em-disco a cada ~100K tokens de CRESCIMENTO desde o último checkpoint, no lugar do aviso único aos 55% | `compress.nudgeGrowthTokens` (50K, fixo) | `client/hooks/check_context.sh` | P1 |
| P3 | histórico compactado pesquisável: trechos do transcript no tantivy, escopo por projeto; o hook pós-compactação aponta a busca das lacunas | `search_context` / `decompress` | lado vivo `~/.claude/skills/loop-engineering/scripts/hooks/compaction_ledger.py` (o `client/` é espelho gerado) | P1 |
| JUIZ | ≥ 7 dias com P2 e P3 ligados × linha de base do P1, critérios fixados antes de abrir os dados | — | — | P1, P2, P3 |

## Regras da frente

- Linha de base ANTES de qualquer mudança; critérios de aceite de P2/P3/JUIZ pré-registrados antes de ligar.
- Contagem por `message.id` (o transcript grava um registro por bloco; contar registros infla ~2,3×).
- Escopo por projeto em tudo que indexa transcript (o analise tem material sensível; lição `cache-sem-escopo-entre-projetos`).
- Quanto a leitura de cache pesa na COTA da assinatura não é público (na API, 0,1× a entrada): o KPI mede volume e
  tamanho; a conversão em cota fica declarada como pergunta aberta.
