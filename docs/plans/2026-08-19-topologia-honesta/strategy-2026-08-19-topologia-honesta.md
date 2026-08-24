---
okf_version: "1.0"
type: Strategy
title: "Topologia honesta e prova de comportamento"
description: "A estratégia da rodada 3: fazer o grafo dizer a verdade sobre si mesmo, e depois provar por execução que os fluxos de fato invocam agentes e fazem trabalho real."
plan_id: 2026-08-19-topologia-honesta
tags: ["#kind:strategy", "#domain:adw", "#domain:loop-engineering", "#process:outer", "#artifact:strategy"]
timestamp: 2026-08-19T07:35:00-03:00
sources:
  - "../2026-08-18-graph-engineering-flow-portfolio/sources/ — as quatro fontes primárias (3 transcrições + arXiv:2505.22954)"
  - "../2026-08-18-graph-engineering-flow-portfolio/o-juiz-e-o-arquivo.html — a análise da quarta fonte"
---

# Estratégia — Topologia honesta e prova de comportamento

## 1. Intento

Duas rodadas anteriores entregaram o **agent graph** (11 peças componíveis) e a
**integridade do juiz**. Esta rodada tem dois movimentos, e o segundo só apareceu
porque o Gabriel perguntou.

**Movimento A — o grafo diz a verdade sobre si.** Uma releitura integral das quatro
fontes com uma pergunta diferente (*o que ainda não foi implementado?*) rendeu sete
lacunas verificadas por `grep = 0`. A descoberta que organiza o plano: um spec carrega
duas topologias sobrepostas — a de **controle**, declarada (`on_pass`/`branches`), e a de
**dados**, real (`{{nodes.X.summary}}`) — e só a primeira existia como objeto. A
divergência entre elas produz `fake_waiting` e `dead_node` do mesmo instrumento.

**Movimento B — prova de comportamento.** Nada disso vale se os fluxos não rodam de
verdade. E não rodavam: cinco dos nós de escrita da library não declaravam
`permission_mode`, então cada invocação headless **pedia permissão a um humano ausente**
e devolvia `pass` por ter perguntado. A library carregava intenção, não evidência.

## 2. O que decidimos NÃO fazer

| Ideia | Fonte | Por que recusada |
|---|---|---|
| Spec não-persistente | Pocock | Ele apaga o spec porque o ticket é a fonte primária; aqui o bundle OKF **é** o ativo que compõe entre execuções |
| Cerimônia "desenhe antes de automatizar" | Isenberg | Vira burocracia; o que sobrevive é a promoção com execução registrada (T5.1) |
| Substituir o painel por pares worker↔crítico | Jay E | Não é lacuna, é topologia alternativa — virou fragmento, não substituição |
| Reproduzir o loop completo do DGM | Zhang et al. | ~2 semanas por run sobre um benchmark numérico barato que não temos; adotada a metade que carrega o ganho medido |

## 3. Os dois princípios que emergiram

**Onde a prova falta, o instrumento cala.** Acoplamento por efeito colateral é invisível
ao interpolador; um lint que acusasse esse par mandaria alguém paralelizar nós que de fato
dependem um do outro. Por isso `fake_waiting` e `dead_node` só falam quando **ambos** os
nós são provadamente read-only.

**Ausência de leitura não é leitura de ausência.** Escrevi esse guard no `judge_attest`
(`rubric_unreadable`) e o violei uma função depois: o ledger de promoção leu a ausência do
campo `synthesized` como lista vazia e promoveu um run **totalmente mockado** como
evidência de comportamento. O mesmo erro, duas vezes, em uma hora.

## 4. Como isto se fecha

Convergência medida por `loop_converged.py` (exit 0, 8 cláusulas), e comportamento provado
por execução real: fixtures genuinamente quebradas, agentes headless reais consertando-as,
gates verificando contra o filesystem. Detalhe das 11 entregas: [`plan.md`](/plan.md) ·
apresentação completa: [`plano.html`](/plano.html).
