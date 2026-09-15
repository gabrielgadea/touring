---
type: LoopBundle
title: "Análise exaustiva de trailhq/Graft — bundle"
description: "Bundle OKF da análise de 12-14/09/2026 em oito rodadas: plano, relatório analysis.md v9 (32 seções), fechamentos P1-P6, R1-R7, R3-1..R3-7, E1-E4, B1-B3, Z0-Z5, Y0-Y5 e S1-S11, bancada de busca, diagnóstico OUTER, abstracts tipados e log."
plan_id: 2026-09-12-graft-analysis
task_id: task_1789257125505670436
task_id_round2: task_1789259381190412574
task_id_round3: task_1789261519392597087
task_id_round4: task_1789263655655762620
task_id_round5: task_1789266884554123368
task_id_round6: task_1789298251281030189
task_id_round7: task_1789320476116312329
task_id_round8: task_1789378432436695877
artifact_url: https://claude.ai/code/artifact/740de4b8-a816-4155-880b-cf37e371f73a
content_blake2b: 15b394e30f7644530e8c519eeeeb1dd4
tags: [competitive-analysis, graft, code-graph]
timestamp: 2026-09-13T23:46:24.950820-03:00
---

## Documentos

| Documento | Papel |
|---|---|
| [`analysis.md`](/analysis.md) | O relatório (v9, 32 seções): rodadas 1-3 (§0-§26), rodada 4 (§27), rodada 5 (§28), rodada 6 (§29), rodada 7 (§30), rodada 8 (§31) |
| [`plan.md`](/plan.md) | Plano da rodada 1: DAG task_1789257125505670436, P1-P6 |
| [`log.md`](/log.md) | Registro cronológico dos fechamentos de fase das oito rodadas |
| [`diagnostics/touring-20260912T220504.md`](/diagnostics/touring-20260912T220504.md) | Diagnóstico OUTER da rodada 3 |
| [`phases/P1.md`](/phases/P1.md) · [`P2`](/phases/P2.md) · [`P3`](/phases/P3.md) · [`P4`](/phases/P4.md) · [`P5`](/phases/P5.md) · [`P6`](/phases/P6.md) | Rodada 1 |
| [`phases/R1.md`](/phases/R1.md) · [`R2`](/phases/R2.md) · [`R3`](/phases/R3.md) · [`R4`](/phases/R4.md) · [`R5`](/phases/R5.md) · [`R6`](/phases/R6.md) · [`R7`](/phases/R7.md) | Rodada 2 |
| [`phases/R3-1.md`](/phases/R3-1.md) · [`R3-2`](/phases/R3-2.md) · [`R3-3`](/phases/R3-3.md) · [`R3-4`](/phases/R3-4.md) · [`R3-5`](/phases/R3-5.md) · [`R3-6`](/phases/R3-6.md) · [`R3-7`](/phases/R3-7.md) | Rodada 3 |
| [`phases/E1.md`](/phases/E1.md) · [`E2`](/phases/E2.md) · [`E3`](/phases/E3.md) · [`E4`](/phases/E4.md) | Rodada 4 (DAG task_1789263655655762620) |
| [`phases/B1.md`](/phases/B1.md) · [`B2`](/phases/B2.md) · [`B3`](/phases/B3.md) | Rodada 5 (DAG task_1789266884554123368): I16 medido, I13 entregue, I12 regra |
| [`phases/Z0.md`](/phases/Z0.md) · [`Z1`](/phases/Z1.md) · [`Z2`](/phases/Z2.md) · [`Z3`](/phases/Z3.md) · [`Z4`](/phases/Z4.md) · [`Z5`](/phases/Z5.md) | Rodada 6 (DAG task_1789298251281030189): I16 como fase, 2A, 3A |
| [`phases/Y0.md`](/phases/Y0.md) · [`Y1`](/phases/Y1.md) · [`Y1b`](/phases/Y1b.md) · [`Y2`](/phases/Y2.md) · [`Y3`](/phases/Y3.md) · [`Y4`](/phases/Y4.md) · [`Y5`](/phases/Y5.md) | Rodada 7 (DAG task_1789320476116312329): memórias e regras buscáveis, busca medida, propagação |
| `bench/` | Conjuntos de perguntas (7), gerador de saco de palavras e grades do avaliador `examples/search_eval.rs` |
| `knowledge/*.json` | Abstracts tipados por fase |
| Página publicada | https://claude.ai/code/artifact/740de4b8-a816-4155-880b-cf37e371f73a |

## Estado

Sete rodadas. Rodada 7: `.claude/` e raízes-companheiras (rules, commands, agents, skills, memória do projeto e de `~`) indexadas sob `@companion/<nome>/<rel>`; `exclude_dirs` por projeto; o texto de markdown e os doc comments entram no tantivy pelo mesmo construtor em rebuild, hooks e reindex. Busca medida por `examples/search_eval.rs` sobre 75 perguntas: conteúdo 0 → 11 de 11, código 10 de 10 com gabarito corrigido, 63 de 75 ao vivo após o rebuild final (64 corrigido), unified igual ao tantivy. Releases 30.4.40-30.4.43 propagadas com prova 40/40; analise (geração 4, 17.558 purgados por motivo) e konverter (geração 2, 588) selados `complete`. Cinco defeitos só visíveis nos consumidores, corrigidos com teste: panic UTF-8, título gigante multiplicado por bloco, guarda de memória contando páginas de arquivo, prova do release herdando ledger, índice ausente em `wiring_map.consumer_file`. Nada commitado. Em aberto: `ViolationsDiff::counts` (produto), memória do rebuild em projeto grande, orçamento de 1.800 s do handler, projeto Work fora do registro.

Rodada 6: predicado de admissão único em touring_hooks_shared::index_policy (walker, varredura, escritores de hook, why); rebuild vivo purgou 5.867 arquivos por motivo e o store foi de 697.101 a 334.410 linhas; selo index_generation (partial explícito em status/find/search/why/doctor); transação por arquivo e passe único de kinds: frio = quente (0 diffs), kill -9 sem perder aresta (aceite i16_accept.sh); hybrid removido do unified; search fuzzy → search_rrf, unified em BM25 por bench (7/10 vs 4/10); reindex do tantivy limpa o índice inteiro. Três deploys, doctor 8/8. Nada commitado. Em aberto: .claude/ fora do índice por regra do walker; backend semântico só com bench de paráfrase; morte única do daemon da cópia sem causa.
