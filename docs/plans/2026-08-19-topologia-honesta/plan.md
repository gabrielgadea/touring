---
okf_version: "1.0"
type: Plan
title: "Topologia Honesta — 11 entregas em 5 trilhas"
description: "Plano de implementação das lacunas remanescentes das quatro fontes: grafo de dados vs grafo de controle, crítica gated por brief, mapa que propaga resoluções, e o arquivo de variantes."
plan_id: 2026-08-19-topologia-honesta
dag: task_1787118866702163488
tags: ["#kind:plan", "#domain:adw", "#domain:loop-engineering", "#status:done"]
timestamp: 2026-08-19T02:55:00-03:00
---

# Plano — Topologia Honesta

Apresentação completa, com raciocínio, diagramas e riscos: [`plano.html`](/plano.html).
Este arquivo é o contrato: entregas, tamanhos, dependências e gates.

## Entregas

| ID | Entrega | Tam. | Depende de | Prova da lacuna |
|---|---|---|---|---|
| T1.0 | Extrator do grafo de dados (`flow_dataflow`) | M | — | `grep -c dataflow adw.py` → 0 |
| T1.1 | Lint `fake_waiting` | M | T1.0 | `grep -c fake_waiting adw.py` → 0 |
| T1.2 | Lint `dead_node` | S | T1.0 | `grep -c dead_node adw.py` → 0 |
| T1.3 | `adw explain --cost` | S | T1.0 | `grep -c "explain --cost" adw.py` → 0 |
| T2.1 | Lint `critique_without_brief` | S | — | `critic-panel.toml` sem nó a montante |
| T2.2 | Fragmento `worker-critic-pair` | S | — | `ls fragments/` → ausente |
| T3.1 | Resolução propaga ao mapa pai | S | — | `grep -c write_back decompose.rs` → 0 |
| T3.2 | `frontier` nomeia protótipo ausente sob névoa | S | — | só vocabulário em `TICKET_SUBTYPES` |
| T4.1 | Fragmento `graph-pack` (knowledge graph) | S | — | `ls fragments/` → ausente |
| T5.1 | Promoção exige execução registrada | M | — | política inexistente |
| T6.1 | Arquivo de variantes + seleção ponderada | L | T1.0, T5.1 | proposto na rodada 2 |

## DAG

`task_1787118866702163488` — 11 subtarefas, validado sem ciclos.
`touring decompose ready task_1787118866702163488` reporta **7 prontas** (T1.0, T2.1, T2.2, T3.1, T3.2, T4.1, T5.1)
e 4 bloqueadas (T1.1/T1.2/T1.3 por T1.0; T6.1 por T1.0+T5.1).

## Trilhas paralelas

`T1` é a única cadeia com dependência real. `T2`, `T3`, `T4` e `T5` são independentes entre si e
de `T1`. `T6` é gated por aprovação humana explícita.

## Gate de aceitação

```bash
loop_converged.py --task task_1787118866702163488 --scope /home/gabrielgadea/projects/touring --rust-full
judge_attest.py && pytest test_adw.py && touring adw lint <specs>
cargo test -p touring-cli          # só se T3 executar
sync-client-skills.py --check && touring e2e -j
```

## Decisões — TODAS APROVADAS 19/08/2026, todas implementadas

_(mantidas abaixo como registro do que foi decidido)_

### Original

1. Aprovar T1-T5 (nove entregas, S/M, sem mudança arquitetural).
2. Decidir T6.1 — a única `L`, a única que muda arquitetura.
3. Decidir T3 separadamente — única trilha que toca Rust e o esquema do banco.
