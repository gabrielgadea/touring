---
type: Ticket
title: Fila do daemon — os hooks de todas as sessões são atendidos em série
description: Aberto no fechamento do S32 (plano de hooks do analise). O daemon atende as requisições de hook uma de cada vez; os 4 handlers religados pela S31 ocupam essa fila e atrasam hooks sem relação com eles.
plan_id: 2026-09-27-fila-do-daemon
origin: analise docs/plans/2026-09-25-auditoria-hooks-qualidade (S32 → S32b), canvas 6 opção D aprovada por Gabriel em 27/09/2026
tags: [daemon, hooks, latencia, fila]
timestamp: 2026-09-27T16:45:00-03:00
---

# Fila do daemon (S32b)

## O que foi medido

**Experimento controlado** (`s32_fila.py`, sessão sintética, 8 tentativas cada, relógio de parede do cliente):
um `pre-bash` sozinho leva **8 ms** (8/8); disparado 20 ms depois de um `pre-edit` de 215 ms, leva **214–215 ms**
(8/8). O segundo pedido herda o tempo restante do primeiro — atendimento em série.

**Tráfego real** (`~/.claude/touring/hook_trace.jsonl`, sessões reais, desde a S31 às 14:35 de 27/09): cada linha do
trace é gravada no fim da chamada com o `elapsed_ms` desde o início, então uma chamada ocupa
`[ts_ms − elapsed_ms, ts_ms]`. Das chamadas lentas (> 600 ms) dos 4 handlers religados, **82%** tinham um hook
pesado (> 500 ms) sobreposto, contra **7%** das rápidas (< 300 ms) — o controle está embutido na comparação. O
vizinho pesado mais comum é o `cli-suggest` do MESMO evento.

**Efeito sobre hooks sem relação** (`hook_latency_report --window 2h`, 27/09 ~16:40):

| hook | antes da S31 (p95) | depois (p95) | n |
|---|---|---|---|
| `pre-bash` | 4 ms | 118 ms | 748 |
| `ceg-observe` | 24 ms | 208 ms | 748 |
| `cli-suggest` | — | 565 ms | 1.306 |

Confundido em parte pela carga dos builds de 27/09 à tarde (releases 30.4.86 e 30.4.87).

**Custo por chamada, isolado** (20 chamadas iguais, binário 30.4.87, arquivo de 265 linhas): p95 pre-read 78 ms,
pre-edit 175, pre-edit-prevention 28, pre-write 104 — dentro do orçamento. A cauda é fila, não custo por chamada;
por isso o gating CILA (`should_enrich`), que corta trabalho por chamada, é a alavanca errada.

## Por que o S32 fechou mesmo assim

O critério do S32 (`hook_latency_report --window 2h --p95-max 800` sobre os 4 handlers) passou com rc 0 sobre toda a
população pós-S31: pre-edit n=174 p95 790 · pre-edit-prevention 170/615 · pre-read 115/707 · pre-write 93/483. A
margem do pre-edit é de 10 ms, e o teto não foi movido. O efeito sobre os outros hooks não está no critério do S32
— é este ticket.

## Passos

1. **Instrumento** — separar, por chamada, o tempo **em fila** do tempo **de trabalho** do handler, e o tempo por
   camada dentro do handler. Hoje não existe; o ponto exato onde o daemon serializa os pedidos de hook ainda não foi
   localizado no código (pergunta aberta 1).
2. **Medir** numa janela sem build: quanto da latência de cada hook é fila, e qual camada domina o trabalho.
3. **Encurtar** as camadas caras dos handlers (alavanca B) — sem mudança de arquitetura.
4. **Decisão de Gabriel** (L4): o daemon atender hooks somente-leitura em paralelo (alavanca A), decidida com os
   dados dos passos 1–3.

**Aceite proposto** (a confirmar no passo 2): `pre-bash` com p95 ≤ 30 ms numa janela sem build, com os 4 handlers
ligados.

## Perguntas abertas

1. Onde a serialização acontece — laço de aceitação único, trava global no despacho, ou banco com escritor único?
2. Quanto dos 118 ms do `pre-bash` é fila e quanto é a carga dos builds da tarde?
