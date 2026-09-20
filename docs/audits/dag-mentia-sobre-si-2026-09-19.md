---
type: AuditReport
title: O DAG mentia sobre si — censo, quatro defeitos e as duas pendências reais
description: >
  Um censo do DAG vivo perguntou se os 554 subtasks abertos eram trabalho ou
  registro. Quase todos eram registro, e a razão eram quatro defeitos de "ponta
  que não se encontra". Este documento registra a medição, as correções e o que
  deliberadamente NÃO foi corrigido.
tags: [dag, decompose, scout, code-mode, medição, REGRA-0, REGRA-21]
timestamp: 2026-09-19
---

# O DAG mentia sobre si

## 1. A pergunta

*"Verifique o que existe de fato de tarefa pendente no touring e se são só
registros ou se de fato estão pendentes."*

## 2. O censo (antes)

| medida | valor |
|---|---|
| tasks / subtasks no DAG | 381 / 1.427 |
| subtasks abertos | 554, em 148 tasks |
| destes, espelho do to-do do Claude Code | 78 — **74 no mesmo slot `::scout`** (95%) |
| abertos sob task JÁ declarada concluída | 59 |
| tasks sem toque há mais de 90 dias | 57 de 148 |
| reivindicações vivas (`claimed_by` com lease válido) | **0** de 70 |
| tasks terminais com `archived_at` NULL | 381 de 381 |

Amostra verificada por execução (o trabalho descrito existe em disco?):
`actor_yield` em 5 arquivos, `memory recall -j` saindo 0, os dois `PATCHES.md`
vendorizados, os dois relatórios de cross-audit (337 e 311 linhas), versão viva
já em 30.4.62 contra um subtask que pedia "release 30.4.47". Todos **feitos**,
todos registrados como pendentes.

## 3. Os quatro defeitos

Nenhum deles é uma função errada. Todos são **duas pontas que nunca se
encontram** — o padrão que revisão por arquivo não pega e só a consulta
agregada revela.

### D1 — `finalize` não arquivava, e ninguém reclamava

- `cli_decompose_finalize` gravava `status = 'finalized'`, nunca `archived_at`.
- `CheckpointStore::archive_completed_tasks` carimbava só onde
  `status = 'completed'` — valor que o finalize **nunca** produz.
- `hook_registry` testava `finalize_result.contains("\"archived\":true")` num
  payload que jamais emitiu esse campo: ramo morto desde a primeira linha.

**Correção**: o finalize carimba e declara (`archived`, `archived_at`); os
estados terminais saem de `task_lifecycle::TERMINAL_TASK_STATUSES` /
`terminal_status_sql_list()`, lidos pelas três rotas.

### D2 — a rotina de arquivamento não tinha chamador

`archive_completed_tasks` e `list_archived`: **zero** chamadores de produção,
só os próprios testes. Mesmo com o predicado corrigido, nada a invocaria.

**Correção** (REGRA #0 — capacidade órfã se liga, não se apaga):
`touring decompose archive [--older-than-secs N] [--dry-run]`, handler
`cli-decompose-archive`.

### D3 — o fechador conhecia 2 dos 3 estágios

O scaffolder escrevia `scout → implement → validate`; o `task-completed`
nomeava `::validate` e `::implement` como literais. O `scout` ficava aberto em
todo mirror já concluído: **74 linhas**.

**Correção**: `MIRROR_SCAFFOLD_STAGES` é a lista única; `close_scaffold_stages`
a percorre, e `hook_registry` chama a função em vez de soletrar estágios.

### D4 — o scout se alimentava do próprio rastro

O corpus TF-IDF indexa descrições de decompose; os tickets que o scout cria
são decomposes. Cada ciclo arquivava um ticket que o ciclo seguinte encontrava
e contava como descoberta nova — o ticket `task_1788296582280254749` trazia
como amostra `decomp:task_1787922622533751850`, o ticket anterior.

**Correção em duas camadas**: `is_scout_ticket` tira o ticket do corpus
(`collect_decompose_table`), e `scout_perpetuo` desconta o eco do yield
(`own_echoes`, sempre reportado — filtro silencioso é indistinguível de scout
quebrado).

## 4. As duas pendências reais

### P1 — `index rebuild --wait`

Implementado como espera **de cliente**: `--wait` faz poll de `index status`
até a geração deixar de ser `building`, com `--poll-secs` e
`--wait-timeout-secs` (0 = indefinido). O default segue **síncrono**, de
propósito: mudar isso alteraria o contrato de todo script que hoje chama
`index rebuild`, e a dor original (rebuild segurando o ator) já foi resolvida
por `actor_yield` e pelo `index status` servido fora do ator. O `--wait` existe
para o caso que sobrou: o exit 79 de um rebuild passado do orçamento, que hoje
manda o operador poll na mão.

### P2 — `code_mode_reuse` em 0,135 contra piso 0,20

Duas camadas, e é importante não confundi-las.

**A régua era cega.** O KPI contava `entry.harvest.is_some()` — o `--harvest`
explícito —, que aparece em **0 de 16.205** linhas do journal, enquanto o
executor já havia enrolado **369 corpos** na escada de trust. A correção lê
`snippet_stats::ladder_totals` e publica `ladder_enrolled` e `ladder_reused`.

**O comportamento não existe.** Dos 369 corpos, **364 rodaram exatamente uma
vez**; há 9 re-execuções em 8.572 runs v2. Portanto:

> Corrigir a régua **não** leva o KPI a 0,20 — e não deve. Enrolar não é
> reusar. O numerador conta re-execução de corpo já conhecido + script
> persistente; a primeira vez de qualquer coisa não é reuso dela.

O piso segue FAIL, agora dizendo a verdade: a persistência funciona, a
**descoberta** não — ninguém encontra o bloco na hora de escrever o próximo. O
próprio comentário do piso já nomeava isso ("abaixo dele o problema é de
DESCOBERTA, não de persistência"). A afordância que fecha esse laço — oferecer
o bloco existente no instante do run — é uma wave própria, não um remendo, e
está deliberadamente **fora** desta rodada.

## 5. O que NÃO foi feito, e por quê

- **Não fechei os 59 + 74 registros legados com UPDATE cego.** Reescrever
  histórico para o painel ficar bonito é o oposto de medir. A rota legítima
  existe agora (`decompose archive`, e o fechador corrigido) e roda sob decisão
  de Gabriel, com `--dry-run` antes.
- **Não mudei o default do `index rebuild`** (ver P1).
- **Não inflei o numerador do reuse** com os 369 enrolamentos, o que teria
  levado o ratio a ~0,28 e feito o gate passar sem uma linha a mais reusada.

## 6. Os três itens que eu havia marcado como "não verificados"

| item | veredito |
|---|---|
| R2-24 (mortes silenciosas do daemon) | **obsoleto** — o fix de cgroup da rodada 9 está em `daemon_spawn` (`systemd-run`) e há 0 mortes nos últimos 2.000 registros do journal; o subtask condicional ("na próxima morte") perdeu o objeto |
| lotes P5.L1-L3 do cross-audit 14/09 | **feitos** — F1 (`refresh_file_wiring`), F2 (patch ASCII do bash) e F5 (F2.5 `UNVERIFIED`, em `verifications/f2_5_dep_cves.rs`) presentes; meu primeiro instrumento procurou F5 no arquivo errado e quase reportou um item fechado como aberto |
| os 7 `RETOMAR-AQUI` | 2 fechados, 1 **pausado por decisão de Gabriel** (proxima-geracao), 4 com fases restantes declaradas. Nenhum é defeito; são planos, e um deles não deve andar sem ordem |

## 7. Gates

`cargo check --workspace --all-targets` verde · `clippy --workspace
--all-targets -D warnings` verde · testes dos crates tocados: foundation 572,
intelligence 532, hooks-prediction 192, cli 1.529, server-reasoning 118,
hook-handlers 700, dispatch 1.338, server 1.636, hooks (suíte completa) verde ·
`dag_lifecycle_closure_e2e` 6/6 · `test_scout_perpetuo` 10/10.

As correções de D1 e D3 foram provadas **por mutação**: com o bug de volta, o
teste falha; restaurado, verde. A primeira tentativa da mutação de D3 passou —
o teste percorria a mesma constante que o código, tautologia — e a asserção foi
reancorada no banco.
