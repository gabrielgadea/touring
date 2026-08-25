---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-25-code-mode-afordancia
tags: [loop, log]
timestamp: 2026-08-25T08:13:47.128275-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-25T09:01:08.675948-03:00 — P0 done

P0 reparo do caminho preferido: 4 defeitos corrigidos e provados vivos. (T0.1) gate.rs pedia Run(CmdScope::any()) com o token lexical apenas como rotulo, entao nenhum grant especifico podia cobrir o pedido — agora deriva o binario real da chamada (call_site_args + first_string_literal + command_name) e o perfil Sandboxed concede 22 binarios de inspecao. (T0.2) --brief elidia 24 de 30 linhas reportando truncated:false — campo elided_lines novo e truncated = upstream OR elided>0. (T0.3) o nudge code-mode-loop disparava sobre comandos que ja eram touring run, 6x nesta sessao. (T0.4) o G1 truncava cada comando da rajada a 240 chars e entregava programa cortado — agora e comando inteiro ou nenhum, com omissao declarada.

## 2026-08-25T09:03:36.305421-03:00 — P0 done

P0 reparo do caminho preferido: 4 defeitos corrigidos e provados vivos (gate.rs deriva o binario real em vez de pedir Run(any); --brief declara elided_lines; o nudge nao recorre sobre touring run; a fusao do G1 entrega comando inteiro ou nenhum). Refechado com --subtask apos o guard revelar que o fechamento anterior reportou dag_updated:true atualizando um subtask inexistente.

## 2026-08-25T09:20:36.089327-03:00 — P1 done

P1 executa a emenda do Gabriel: o nudge deixa de exortar e passa a ENTREGAR o programa. (a) o caminho de laco parou de traduzir para python com o corpo '# then your per-file op over files'; loop_code_mode_command e loop_glob foram REMOVIDAS (REGRA #0) e o laco viaja verbatim via bash_code_mode_command, que tambem perdeu a truncagem de 200 chars. (b) dois guards estruturais novos: 'todo nudge de bash carrega o comando do gatilho inteiro' na forma POSITIVA sobre 4 gatilhos, e 'nenhum nudge adia o trabalho' sobre o vocabulario de adiamento inteiro — o guard antigo so procurava '<' e por isso deixou passar um placeholder em prosa. (c) SDK do transporte injetado 1x por sessao no SessionStart (2324 bytes, byte-estavel, kill switch TOURING_SDK_SECTION_DISABLED=1) contra 1196 injecoes por chamada.

## 2026-08-25 14:30 — P2.3 calibração concluída

- 20 prompts reais / 8 sessões / 1.312 tool calls classificadas (T0-T4) com o
  classificador segment-aware ([calibration_p23.py](calibration_p23.py)).
- Matriz grep/cat/find VALIDADA; T1 fora (G6 cobre); fronteira T2/T3 medida
  (92% sem fronteira, 12/12 concordância humana).
- Fix cat-redirect em scan_class_of (27 escritas, 20,6% de erro) + teste
  provado por mutação; 398/398 touring-cli verde.
- Relatório: [phases/P2.3.md](phases/P2.3.md).

## 2026-08-25 15:00 — P2.4 efeito mensurável concluído

- Simulação por braços sobre 1.578 chamadas reais ([effect_p24.py](effect_p24.py)):
  native 2,30 MB / both 3,68 MB (+60%, nudge domina) / code 2,24 MB (−2,9%).
- Efeito das classes CONFIRMADO; valor do code = N→1 round-trips, não bytes.
- P1 (SDK 1×/sessão) confirmado quantitativamente. Relatório: [phases/P2.4.md](phases/P2.4.md).

## 2026-08-25T12:05:36.569033-03:00 — P2 done

TOURING_CODE_MODE por escopo (sessao->alias->projeto->default) deployado + secao de sessao declara apresentacao/origem/efeito (12 testes + guard D8) + politica padrao code neste workspace + calibracao 20 prompts/1312 calls (matriz validada, T1 fora, fronteira T2/T3 medida 12/12) + fix cat-redirect provado por mutacao (398/398) + efeito por bracos: both +60% bytes (nudge domina), code -2.9% bytes mas N->1 round-trips

## 2026-08-25T12:10:38.876041-03:00 — P2 completed

probe do fix de resolucao

## 2026-08-25T12:10:38.993750-03:00 — P2 completed

probe do fix de resolucao de subtask id

## 2026-08-25T12:11:06.811421-03:00 — P3 pending

probe resolver (idempotente, P3 segue pending)

## 2026-08-25T12:11:26.931848-03:00 — P3 pending

probe resolver idempotente

## 2026-08-25T12:12:17.144730-03:00 — P3 pending

probe mutacao

## 2026-08-25T12:12:17.262033-03:00 — P3 pending

probe restaurado

## 2026-08-25T13:31:59.017683-03:00 — P3 completed

T3-B first-wins-fold-the-rest deployado: turn_ledger por (projeto,sessao), turn_decide puro com janela 10s, ordem G6→T3-B→G1 (especificidade), close em post-tool-rl (qq tool) + denies G1/G6, counters t3 no gate-metrics, kill switch. 5 testes novos; 406 cli + 1321 dispatch + 484 foundation + clippy 0. PROVA DE ACEITACAO viva: batch de 3 greps → 3ª fundida com programa nao-escrito. Achado: CC dispara hooks serial-com-sobreposicao (nao all-pres-first) — P5/T3-A hold de 25ms equaliza.

## 2026-08-25 16:30 — P4 fechado

- A/B adotado: simulação P2.4 sobre corpus real (decisão registrada, revisão do Gabriel).
- Memória inflada prova:code-mode-eficiencia-medida SUBSTITUÍDA (mesma chave, #status:corrected).
- Telemetria viva: t3 counters expostos em gate-metrics -j.
- Relatório: [phases/P4.md](phases/P4.md).

## 2026-08-25T13:35:41.693881-03:00 — P4 completed

A/B por bracos sobre corpus real (1578 chamadas, 3 grandezas separadas): both +60% bytes (nudge domina), code -2.9% bytes mas N->1 round-trips (140 programas). Decisao registrada: simulacao adotada como evidencia (A/B ao vivo fica como opcao futura). Memoria inflada prova:code-mode-eficiencia-medida SUBSTITUIDA (mesma chave, #status:corrected). Telemetria t3 viva em gate-metrics.

## 2026-08-25 17:00 — CONVERGÊNCIA

`loop_converged.py --rust-full` → **unmet: []** (judge_intact, dag_done 5/5,
quality, no_p0_fail, measured_whole_scope, orphans_base, cargo_green).
Extras REGRA #21 do caminho: axis8 health_delta flaky → 49 testes serializados
em 6 arquivos + guard generalizado (2 regras nomeadas); teste obsoleto
gate_command_denies_a_subprocess_under_sandboxed atualizado para a verdade
pós-T0.1 (+ teste simétrico do lado Allow). **Plano CONCLUÍDO.**
