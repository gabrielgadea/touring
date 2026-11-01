---
type: ExplorationReport
title: S-5.6 — exploração P6 (compaction-assist) × P7 (rerank no explore)
description: Avaliação dos dois candidatos de exploração à luz do G2 medido. Veredito — P7: design aprovado, dobra em S-5.4 (rerank por P(sim), único uso validado pelo G2). P6: adiado para o aluno local — o state é a sessão inteira (LGPD/R2) e o Noul tem ECE 0,709 (sem τ).
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [exploracao, p6, p7, compaction, rerank, recall]
timestamp: 2026-10-01T03:00:00-03:00
okf_version: "0.1"
---

# S-5.6 — exploração P6/P7 (fog: unknown → resolvido)

Parte do [bundle](/index.md). Restrições MEDIDAS que governam os dois designs
[G2, F]: **(a)** Noul tem ECE 0,709 nos nossos dados — probabilidades servem
para ORDENAR, nunca para `P ≥ τ` sem recalibração; **(b)** latência real da
máquina: p50 ~370 ms por chamada; **(c)** Jev processa nos EUA — conteúdo de
transcript/memórias exige redação ou professor local (R2/R10 da estratégia);
**(d)** recall booleano (corte 0,5) acertou 86,7% — único uso validado junto com
o rerank do cookbook (top-1 5%→18%, top-10 38%→62%).

## P7 — rerank no explore (VERDE — dobra em S-5.4)

**O que é.** Estágio de re-ranking por P(sim) sobre os candidatos das lentes do
`touring explore` e do recall — exatamente o desenho do cookbook de re-ranking
(ordenar por `noul`, sem threshold).

**Por que passa no predicado de encaixe**: espaço fechável (os candidatos já
existem) · julgamento semântico · custo real hoje (ruído medido: 78,7% do recall
é irrelevante no G1; em modo slim, 47 candidatos e 0 injetados — S-3.4) · erro
tolerável (ranking, não gate) · dados são memórias do touring (repo público).

**Desenho (para a wave de S-5.4)**:
1. shortlist atual (BM25/RRF/embeddings, top-30) → 1 chamada com 30 Nouls
   (candidata dentro das instructions — anti-context-rot, padrão da doc);
2. **ordenar** por P(sim); cortar pelo orçamento CILA **por posição**, nunca por τ;
3. journal via adapter; `curated_recall_share` (S-3.1 + S-3.4) mede o efeito.
Custo [E]: ~30 × 400 tok ≈ 12 k tok ≈ US$ 0,0005 por recall; ~370 ms por chamada.

**Medição obrigatória antes de ligar**: replay do G1 (300 pares com dupla
referência) — a métrica é nDCG@10 e top-1, comparados ao RRF atual; ligar só se
top-1 do rerank Jev > top-1 do RRF no replay (a decisão se toma por número, não
por entusiasmo — a mesma disciplina do G2).

## P6 — compaction-assist (ADIADO para o aluno local)

**O que seria.** No PreCompact, o Jev ordenaria turnos/artefatos da sessão por
relevância para o objetivo corrente e manteria o top-K no snapshot.

**Por que adiar [F + I, 0,85]**:
1. **State = a sessão inteira** — transcripts carregam caminhos, dados de
   processos e conteúdo do `analise` (LGPD/R10): exige redação prévia robusta ou
   professor local. O Jev hospedado fica proibido até lá.
2. **O snapshot atual é determinístico e correto** (subtasks pendentes + estado) —
   o valor marginal do ranking semântico é pequeno contra o risco de dados.
3. Com o aluno local (W4.2) o mesmo design roda a 6–15 ms, sem egresso e sem
   redação — o aluno é o habitat natural do P6, não o Jev.

**Registro**: quando o aluno existir, P6 vira subtask com este desenho e a
restrição (a) aplicada (ordenação, não τ).

## Veredito

- **P7**: aprovado como design — vira o coração da wave S-5.4, com replay G1 como
  gate. Fog: **resolvido**.
- **P6**: adiado para o aluno local — fog: **resolvido (decisão: adiar)**.
