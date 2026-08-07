---
type: Strategy
title: Isolamento de loops por sessão — N sessões CC, um projeto
description: O marker do loop era keyed só por cwd, então sessões concorrentes no mesmo projeto compartilhavam um único loop. Escopo passa a ser (projeto, sessão).
plan_id: 2026-08-02-multi-session-loop-isolation
tags: [loop, multi-session, isolation, marker, adw]
timestamp: 2026-08-02T14:35:00-03:00
okf_version: "0.1"
---

# Isolamento de loops por sessão

Parte do [bundle](/index.md). Antecedente: [o loop como modus operandi padrão](/../2026-08-02-loop-default-modus-operandi/strategy-2026-08-02-loop-default-modus-operandi.md).

## 1. Problema (Gabriel, 02/08/2026)

> "às vezes existem mais de uma sessão do Claude Code abertas trabalhando no mesmo
> projeto, e o daemon considera os loops como se fossem de uma única sessão."

## 2. Causa-raiz

Não é o daemon — é o **marker**. `loop_marker.py::marker_path()` derivava o nome
de arquivo **só do cwd**:

```python
return MARKER_DIR / f"active-{_key(_resolve_cwd(cwd))}.json"   # sem dimensão de sessão
```

Todos os três hooks (`loop_outer_arm`, `loop_stop_guard`, `loop_snapshot`)
resolvem por esse caminho. Logo, N sessões no mesmo projeto = **um** loop.

### Sintomas derivados (todos consequência da mesma linha)

| # | Sintoma |
| --- | --- |
| S1 | Stop de B **bloqueado** pelo manifesto não cumprido de A |
| S2 | B **pega carona** nos artefatos de A: A completa, B encerra sem ter feito o próprio OUTER |
| S3 | `continuations` é **um contador só** — os 2-5 avisos de A esgotam a cota de B |
| S4 | A converge e **arquiva** o marker debaixo de B |
| S5 | `compliance.jsonl` grava só `cwd` → registros de sessões concorrentes leem como **um único flow errático**, corrompendo o KPI |
| S6 | PreCompact de A snapshota o estado que estiver no marker, possivelmente de B |

### Segundo defeito, mesma família (`adw.py`)

```python
run_id = resume_run or f"{spec.name}-{int(time.time())}"   # granularidade de 1 s
```

Duas sessões iniciando o mesmo ADW no mesmo segundo derivavam o **mesmo
`run_path`**: a perdedora morria no `flock` e, se adquirisse o lock depois,
`_resume_state` **replayaria o journal da outra sessão como estado próprio**.
Era latente enquanto ADW era invocado à mão — passou a ser material agora que o
OUTER determinístico é padrão e **toda** sessão dispara `strategy-loop`.

## 3. Decisões

| # | Decisão | Razão |
| --- | --- | --- |
| D1 | Chave passa a ser `active-<sha1(cwd)[:12]>-<sha1(sessão)[:8]>.json` | mantém a dimensão de projeto (defeito #2 de 02/07) e acrescenta a de sessão |
| D2 | Identidade: payload `session_id` → `CLAUDE_CODE_SESSION_ID` → `TOURING_SESSION_ID` | o CC exporta ambas em **todo** processo de hook, então `loop_stop_guard`/`loop_snapshot` — que não leem stdin — resolvem o MESMO id que o hook de arming gravou |
| D3 | Marker carimbado para **outra** sessão nunca é meu, mesmo com o path alcançado | defesa em profundidade contra `--marker` explícito, cópia manual, colisão de hash |
| D4 | Marker **sem** carimbo (pré-migração) é **reivindicado uma vez** pelo primeiro avaliador | descartá-lo desarmaria silenciosamente um loop vivo; "primeiro avaliador vence" é arbitrário mas limitado e estritamente melhor que o status quo onde TODOS compartilhavam |
| D5 | Sem identidade resolvível ⇒ comportamento pré-02/08 **exato** | não inventar identidade; nada regride onde a informação não existe |
| D6 | `compliance.jsonl` grava `session_id` | sem isso o KPI de aderência é inatribuível (S5) |
| D7 | `run_id` do ADW vira `<adw>-<epoch>-<sessão8>.<pid>` | sessão dá atribuição; pid garante unicidade mesmo para dois runs da **mesma** sessão no mesmo segundo |

## 4. Prova executada

- `test_flow_guard.py`: **41** testes (6 novos), incluindo
  `test_stop_of_session_b_is_not_held_by_session_a` — o sintoma S1 reproduzido e
  corrigido —, rejeição de carimbo alheio, reivindicação única e compatibilidade
  sem identidade.
- `test_adw.py` + explore + scout + factory: **76** testes (1 novo, colisão de `run_id`).
- `e2e_test.py`: **16/16**.
- **Ao vivo**, no marker real deste projeto:

```
ANTES : active-43224dc4d9af.json            (sem sessão, compartilhado)
DEPOIS: active-43224dc4d9af-242fb47a.json   (owner=506e269f-…, migrado)
outra sessão procura active-43224dc4d9af-31b49cf3.json → enxerga: False
```

O hash de projeto (`43224dc4d9af`) é idêntico nos dois: a dimensão de projeto
foi preservada, a de sessão foi acrescentada.

## 5. Escopo deliberadamente não alterado

- **Ledger CCE (`.touring-explore/<topico>.ledger.json`) segue por tópico, não por
  sessão** — e deve seguir. Ele é conhecimento acumulado do projeto; duas sessões
  explorando o mesmo tema **devem** somar rodadas em vez de duplicar trabalho. O
  que precisava de isolamento era o *gate*, não o *conhecimento*.
- **DAG (`touring decompose`) já é seguro**: por projeto no daemon, mas cada loop
  tem seu próprio `task_id`, então as cláusulas de convergência já eram
  task-scoped.
- **Markers órfãos de sessões mortas** não são adotados automaticamente: expiram
  pelo TTL de 24 h. Retomar trabalho de outra sessão continua sendo ato explícito
  (`loop_marker.py write --task <id>`), como a skill já documenta.
