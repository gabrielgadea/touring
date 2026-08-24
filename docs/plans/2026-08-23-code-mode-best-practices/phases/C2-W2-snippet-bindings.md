---
type: PhaseReport
title: "Ciclo 2 — W2: w3b, bindings `snippet_*` com composição real, ciclo e credencial"
description: "Snippets ≥ provisional viram funções do sandbox; a composição snippet→snippet funciona (onde a referência dá ReferenceError), e quatro recusas barram uma biblioteca inconsistente sem derrubar o run."
tags: [code-mode, snippets, bindings, composition, w3b, S-3.4]
timestamp: 2026-08-24T16:30:00-03:00
plan_id: 2026-08-23-code-mode-best-practices
status: done
---

# C2-W2 — w3b / S-3.4: bindings `snippet_*`

> Backlog canônico (analise, `task_1787534694944647530::w3b`, claimed
> `owner=sess-2f2d716c`). Spec S-3.4: *"snippets trusted viram funções
> `snippet_<name>` no SDK Python; grafo de dependência com detecção de ciclo
> ANTES de injetar (R2); gate de segredos (P12)"*. Só foi possível porque
> [C2-W1](/phases/C2-W1-snippet-ladder-elo.md) fez a biblioteca sair de zero.

## O que a implementação de referência erra — e o que este porte faz

| Ponto | `ai-code-mode-snippets` | Aqui |
|---|---|---|
| Composição | snippet vê só `external_*` → `ReferenceError`; o system prompt **ensina** um exemplo que não funciona | o corpo é função real do módulo, enxerga os globais → `snippet_a` chama `snippet_b` |
| Ciclo | não existe grafo; recursão explode em runtime | DFS antes de injetar; erro traz o **caminho** `a → b → a` |
| Nome | `-`/`.`/`/` viram `SyntaxError` no `eval` | sanitização determinística, e **colisão é recusada** (nunca "o último vence") |
| Credencial | varre `inputSchema` por palavra | varre o corpo por **palavra atribuída a literal** — ler do ambiente nunca acusa |
| Corte | `maxSnippetsInContext: 5`, silencioso | `MAX_BINDINGS = 20`, e o cortado é **nomeado** em `omitted_over_cap` |
| Badge | visível ao modelo (o detalhe bom) | idem — na docstring de cada binding, com o nº de execuções |

## Desenho

`snippet_bindings.rs` (novo, lógica pura) + `snippet_preamble()` no `run.rs`.
Gate de entrada: `>= Provisional` e `#lang:python` — a doutrina do módulo de
trust (*"suggesters offer only >= Provisional"*), e o preâmbulo **é** Python,
então um corpo bash como função Python seria `SyntaxError` na primeira chamada.

**Fail-open por desenho**: biblioteca inconsistente não aborta o run — o
programa do usuário pode nem usar snippets, e derrubá-lo por causa de um
snippet alheio seria o gate bricando a sessão (invariante do CEG). Sem
bindings, quem dependia deles recebe `NameError` e o `snippet_bindings.error`
explica o porquê.

## A lacuna que só a sonda em produção pegou

A primeira versão guardava os corpos como strings e usava uma primitiva de
avaliação dinâmica. Funcionou — e a sonda mostrou
`[CEG WARNING] Forbidden calls detected: exec (code injection)` em **todo** run
orquestrado. Dois custos, nenhum visível em teste unitário:

1. Sob `TOURING_CEG_FORBIDDEN_ENFORCE=1` o run seria **bloqueado** — uma
   regressão latente, invisível até alguém ligar a política estrita.
2. Um aviso de *code injection* que aparece sempre é um aviso que se aprende a
   ignorar — o gate perde significado justamente quando importa.

Reescrito para emitir **funções Python reais**. Isso trouxe de brinde a
composição (corpo real enxerga globais) e um custo próprio, assumido e
declarado: virar corpo de função é ser indentado, e indentar um literal de
string multi-linha mudaria o texto em silêncio — daí a quarta recusa,
`MultilineLiteral`, com erro que ensina a saída.

Detalhe recursivo: o teste `preamble_uses_no_exec_…` reprovou a **primeira**
correção, porque o comentário que eu escrevera no preâmbulo continha a palavra
proibida. O scanner lê o preâmbulo como leria código do usuário.

## Sonda de aceitação — binário vivo

| Sonda | Resultado |
|---|---|
| binding executa | `['a', 'b']`, exit 0, `forbidden_calls: []`, **stderr vazio** |
| **composição binding→binding** | `snippet_sonda_compoe("de-dentro")` → `snippet_sonda_normaliza` → `de-dentro:['a', 'b']` — o `ReferenceError` da referência |
| ciclo plantado | `available: []` + `error: snippet_ciclo_a → snippet_ciclo_b → snippet_ciclo_a`; **run do usuário exit 0** (fail-open provado) |
| credencial plantada | `available: []` + erro nomeando `client_secret` e ensinando `os.environ` |

Fixtures de sonda removidas ao final (escada e memórias em 0).

## Gates

- 15 testes novos em `snippet_bindings` + 1 em `run` (escopo orchestrate/python).
- `clippy -D warnings` **0**; gate P0 50-dim **passou** (a primeira versão foi
  barrada por F2.4: o teste do detector de credenciais escrevia uma credencial
  literal no fonte — hoje as fixtures são montadas em runtime).
- `wiring orphans` 2450 (baseline 2516), 0 órfãos da wave.
- Deploy `update-touring` ×2 (ciclo sonda→fix→redeploy).

## Consequência

`d2` (a diretiva que a W3 deixou aberta esperando os bindings) fecha com esta
fase. O ciclo do code mode está inteiro: colher → medir → **compor**.
