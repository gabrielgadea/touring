---
type: PhaseReport
title: "Ciclo 2 — W1: w3b + o elo que populava a biblioteca de snippets"
description: "A escada de confiança da W3 nunca fora exercitada — tabela inexistente, 0 execuções. O executor passa a colher e reconhecer sozinho; a biblioteca gradua-se pelo uso."
tags: [code-mode, snippets, trust-ladder, affordance, w3b]
timestamp: 2026-08-24T14:05:00-03:00
plan_id: 2026-08-23-code-mode-best-practices
status: done
---

# C2-W1 — o elo `harvest → ladder` (pré-condição de w3b)

> Backlog canônico (analise, `task_1787534694944647530`): a ordem aprovada punha
> **w3b** como próximo. A W3 registrara que w3b era *"dependente de biblioteca
> populada — não half-baked"*. Na retomada, essa pré-condição foi **medida** e
> estava em zero. Gabriel decidiu (24/08, retomada): fechar o elo antes.

## O diagnóstico — duas rupturas, uma causa

| # | Ruptura | Evidência |
|---|---|---|
| **B1** | Nada chamava `record_execution` na prática | tabela `snippet_stats` **inexistente** em todos os DBs — `ensure_schema` só roda dentro dela, logo 0 execuções em produção. Único caller: `ceg_impls.rs:51`, alimentado apenas por `touring learning reward` digitado à mão |
| **B2** | A chave ensinada não casava com o predicado | o `harvest_hint` mandava `touring memory store <slug>` (slug livre); a ponte só aceitava `starts_with("snippet:")`. Medido: as memórias colhidas eram slug-livre, as prefixadas eram **todas** codetag-anchored — nenhuma vinha do code mode |

Causa comum: **extrator e verificador não vinham da mesma fonte** — o padrão já
registrado em `verificador-usa-menos-que-o-extrator`. O `harvest_hint` ensinava
um beco sem saída, e a W3 media trust de uma biblioteca que nunca enchia.

## O conserto — afordância, não pedido (D8)

| Peça | Onde | O quê |
|---|---|---|
| **A** | `snippet_stats.rs` | `SNIPPET_KEY_PREFIX` + `harvest_key()` + `is_snippet_key()` — **o par único** de quem cunha e quem reconhece. `ceg_impls.rs` passou a derivar dali |
| **B** | `snippet_stats.rs` | `code_sig()` (digest do corpo, normalização só cosmética) e `by_sig()` — a **própria coluna `sig_hash` da escada** vira o índice de reidentificação: zero schema novo para sair de sincronia |
| **C** | `run.rs` | `touring run … --harvest <slug>`: o **executor** persiste a memória (`snippet:<slug>`, tags `#kind:snippet` `#lang:<lang>` `#process:code-mode`) e matricula na escada, em um comando |
| **D** | `run.rs` | todo run reidentifica o corpo por digest e registra seu próprio outcome; devolve `snippet_trust` (badge) ao modelo |
| **E** | `run.rs` | o `harvest_hint` passou a ensinar `--harvest` (não mais o beco sem saída) e **some** quando o run já colheu |

Fail-open em todo o caminho: falha de contabilidade de snippet nunca altera o
resultado do programa que o usuário rodou.

## Sonda de aceitação — no binário vivo, 4 caminhos independentes

| Sonda | Resultado |
|---|---|
| Colheita `--harvest` | `snippet_trust: ○ untrusted`; hint **ausente** (correto — já colhido) |
| **Reuso ×10, zero reward manual** | `○ untrusted` → **`◐ provisional`** — a escada subiu sozinha |
| Hint em corpo novo | ensina `--harvest` ✓ · ensina `memory store` ✗ |
| Corpo desconhecido | `snippet_trust` ausente — não atribui a ninguém |

Estado final da biblioteca que estava em ZERO:
`snippet:sonda-normaliza | 12 | 12 | provisional` · `snippet:sonda-somar | 4 | 4 | untrusted`.

A sonda também corrigiu **a si mesma** duas vezes (log INFO do CEG misturado no
pipe; filtro de chave contra um `query` com limite 10) — antes de acusar o
código, provar o instrumento.

## Gates

- 8 testes novos (5 em `snippet_stats`, 3 em `run`); suítes dos 3 crates verdes.
- `clippy -D warnings` **0** em `touring-intelligence`, `touring-hook-runtime`, `touring-server`.
- `e2e` **0.8576 pass** (baseline 0.8584; Δ −0.0008, 0 fases falhas).
- `wiring orphans` **2450** (baseline 2516) — nenhum órfão desta wave, os 5 pub novos wired.
- `sync-client-skills --check` CLEAN (301 espelhados).
- Deploy via `update-touring` ×2 (ciclo sonda→fix→redeploy), daemon vivo.

## Teste que trava o contrato

`every_minted_key_is_recognised_by_the_predicate` — sobre as **formas** que um
slug assume, não sobre um caso. É a asserção positiva que impede o B2 de voltar
por um caminho ainda não coberto (a lição do
`every_derivable_nudge_carries_real_value_not_placeholder`).

## Consequência para w3b

Com a biblioteca populando-se por uso, `w3b` (bindings `snippet_*` dinâmicos no
SDK do orchestrate, com detecção de ciclo e gate de segredos) deixa de operar
sobre vazio — a pré-condição que a própria W3 declarou está satisfeita, e `d2`
pode fechar de verdade quando w3b entregar.
