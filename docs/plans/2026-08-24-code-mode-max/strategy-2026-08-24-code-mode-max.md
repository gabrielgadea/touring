---
okf_version: "1.0"
type: Strategy
title: "Code Mode Máximo — corrigir o que estava quebrado e ligar o que estava desligado"
description: "Auditoria por execução das 7 dimensões do code mode do Touring; 3 defeitos reais corrigidos, 2 hipóteses retiradas após verificação, e a afordância do sandbox nos ADW finalmente utilizável"
tags: [code-mode, adw, sandbox, observabilidade, afordancia]
timestamp: 2026-08-24T15:30:00-03:00
plan_id: 2026-08-24-code-mode-max
scope: /home/gabrielgadea/projects/touring
---

# Estratégia — Code Mode Máximo (24/08/2026)

## O diagnóstico, e por que ele exigiu execução

O programa de code mode (bundle `2026-08-23-code-mode-best-practices`) fechou
10 fases W0–W9 e três do ciclo 2. Todos os phase reports dizem `done`, e o DAG
canônico marca 10 de 11 subtasks `completed`. Nada disso era falso — e nada
disso bastava para responder se o code mode **funciona**.

A auditoria (`audit_code_mode.py`, 7 dimensões) foi escrita para perguntar
apenas coisas verificáveis por leitura do fonte e por execução. Duas vezes ela
me devolveu um veredito errado antes de me devolver o certo, e vale registrar
por quê:

1. `adw:code-mode` reportou **0/0 specs** porque eu apontei para um
   `adw-library/` na raiz do workspace, que não existe. Zero-de-zero é
   indistinguível de "nenhum fluxo usa code mode" — o mesmo *ausência de sinal
   lida como zero* que a Lei L2 proíbe.
2. `counter:code_mode_runs_count` reportou o sítio errado porque procurei a
   chamada em `cli/run.rs`, e ela mora um nível abaixo, no executor
   compartilhado. O defeito existia — mas não era o que eu tinha descrito.

**Corolário**: um auditor precisa ser auditado antes de servir de prova. As
duas correções do instrumento vieram antes de qualquer correção do sistema.

## Os três defeitos reais

### D-1 · O KPI de economia media exatamente a rota que o programa substitui

`ctx_execute_impl` tem duas linhas adjacentes com destinos diferentes:

```rust
record_code_mode_run(bytes_elided);   // counter do PROCESSO
journal_run(...);                     // arquivo em DISCO
```

O `touring run` — o canal de code mode **sem MCP**, o que o programa inteiro
existe para promover — é um CLI efêmero. O counter morria com ele; o journal
sobrevivia. Medido: **163 execuções no `run_journal.jsonl` contra
`code_mode_runs_count = 0`** lido do daemon, que é de onde o `gate-metrics` lê.
A única chamada que "contava" era a da rota MCP (`ctx_execute_tools.rs:508`),
que roda no processo longo.

O efeito não é cosmético: a W4 e a tese de affordance (`U(a) = P·V − C(tokens)`)
dependem desse KPI para decidir promoção/demoção. Ele reportava zero economia
para o canal barato e toda a economia para o caro.

**Correção**: `CtxExecuteOutput` passa a expor `bytes_elided`, e o adaptador CLI
retransmite ao daemon pelo hook novo `cli-code-mode-run` — espelhando o que a
C2-W0 já fazia com as sub-chamadas do `--orchestrate`, contabilizadas no
daemon quando chegam pelo socket. Fail-open por construção: perder um counter
nunca pode custar a execução do usuário.

**Prova**: após deploy, `runs 2 → 4` em duas execuções, `bytes_elided: 781`.

### D-2 · A afordância do sandbox nos ADW existia e quebrava quem a usasse

`run_code_node` aceita `sandbox = true` e roteia o comando por `touring run`.
**Zero de 24 nós `code`** a usavam. A causa não era desconhecimento — eram dois
defeitos encadeados que a tornavam inutilizável:

| # | Defeito | Consequência |
|---|---|---|
| a | o `timeout_ms` do nó não era propagado | nós declaram 300 000 ms; o `touring run` usa 30 000 por default (teto 120 000) → abortava |
| b | o output vira o JSON do run | `NEW_FINDINGS_RE`/`METRIC_RE`/`VERDICT_RE` casam `^MARCADOR=`; dentro do JSON a linha é `  "stdout": "NEW_FINDINGS=5\n",` → **nenhum casa** |

O (b) é o mais perigoso porque falha em silêncio e para o lado errado: marcador
ausente é *unknown*, nunca zero, então um `loop` exauriria `max_iters` em vez de
convergir, e todo gate sob contrato leria "unparseable" = REJECT.

**Correção**: o timeout é propagado com clamp ao teto e **aviso explícito** no
output do nó (nunca silencioso); `_unwrap_sandbox_output` devolve o
stdout/stderr do programa e anexa o `retrieval_hint` da W1 quando houve spill —
o nó passa a saber onde está a saída completa em vez de recebê-la cortada.

**Prova**: dois artefatos em disco. Em `sbprova` (antes) o output do nó é o JSON
cru; em `marcador` (depois) é `NEW_FINDINGS=7`, e o regex casa.

Com isso, `sandbox = true` foi ligado em **14 nós de leitura** — que ganham
sandbox, CEG, journal, counters e colheita de snippets sem mudar o que fazem.

### D-3 · Corte cego destruindo o locator que a W1 entregou

Cinco nós faziam `touring <master> | head -c 1500`. O runner já implementa a
Lei L4 (`SUMMARY_LIMIT = 2000` inline + `full_ref` para o output completo em
disco), então o `head -c` cortava **antes** do runner ver, destruindo o artefato
completo. O custo desse padrão está documentado no próprio runner: o
`critic-panel` contou **zero** verdicts porque o summary truncou 14 718 bytes,
enquanto os três críticos haviam retornado REJECT.

Removido dos cinco cortes de comando único. Mantido onde é orçamento **por
fonte** (`feature` compõe três saídas) ou `echo` curto (`hotfix`).

## As duas hipóteses retiradas

Registro porque a disciplina de retirar vale tanto quanto a de corrigir:

- **stderr concatenado no stdout** (`adw.py:1479`) parecia o mesmo defeito que a
  W0 corrigiu no `touring run`. Não é: a linha 28 documenta que os contratos
  podem vir de "stdout **ou** stderr, own line". É design.
- **`; true` mascarando exit code** em 5 nós. Em todos, `on_pass == on_fail` —
  são nós informativos que seguem adiante por construção. Redundante, não
  perigoso.

## O princípio que organizou as três correções

**Enforcement mora no executor, não no anúncio** (D8). Nas três, a correção foi
no executor — o relay no adaptador CLI, o desembrulho no runner, o cap do
runner substituindo o corte da spec — e não num pedido para que o autor da
próxima spec se lembre. A quarta instância do dia do mesmo padrão: a afordância
existia, estava desligada, e ninguém tinha exercido o caminho que a quebrava.

## O que fica aberto

- `mem-vazio` (recall degradado em projeto sem corpus) segue registrado, não
  mascarado.
- O detector de escrita da auditoria enxerga sintaxe de shell (`>`, `rm`,
  `sed -i`) mas não escrita feita **dentro** de um programa — foi assim que ele
  aprovou o `arm_marker`, que escreve o marcador do flow guard. Revertido e
  documentado no ponto; o detector segue com essa limitação conhecida.
