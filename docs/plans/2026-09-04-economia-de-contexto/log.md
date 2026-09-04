---
type: LoopLog
title: "Economia de Contexto — registro de execução"
description: "O que foi executado do plano em 04/09/2026, com o que a execução corrigiu do próprio plano."
plan: 2026-09-04-economia-de-contexto
timestamp: 2026-09-04
okf_version: 1
---

# Registro de execução — 04/09/2026

## Entregue

| fase | subtarefa | estado | evidência |
|---|---|---|---|
| P0 | S-0.1 régua `context_budget` | **feito** | `crates/touring-cli/src/cli/context_budget.rs`, 10 testes + sonda viva |
| P0 | S-0.2 fonte sem instrumentação nova | **feito** | lê o transcript do próprio Claude Code; funciona retroativamente |
| P1 | S-1.1 dedup por conteúdo | **feito** | `repeat_reference` + `emitted_content`, 5 testes |
| P2 | S-2.1 hint por evidência | **feito** | `symbol_bearing_edits`, 13 testes |
| P2 | S-2.2 expurgo do rastro superado | **feito** | `unresolved_failures`, 5 testes |
| P3 | S-3.1 uma escala CILA | **feito** | `touring-foundation/src/cila.rs` + guard cruzado em 256 níveis |
| P3 | S-3.2 `LESSON_BUDGET` sob a escala | **feito** (escopo corrigido) | ver abaixo |
| P3 | S-3.3 orçamento por turno | **feito** | `touring-hooks-shared/src/turn_budget.rs`, 6 testes |
| P4 | S-4.1 classe de custo no manifesto | **feito** | `flow_manifests.json` + gate + 3 guards |
| P4 | S-4.2 default cobra só classe A | **feito** | `work-outer.enforced_classes = ["A"]` |
| P4 | S-4.3 classe B (interação como gate) | **desenhado, não construído** | a classe existe no contrato; falta o executor que materializa o registro do turno |
| P5 | S-5.1 medir os tool results | **feito** | Bash 62,7% + Read 28,0% = 90,7% |
| P6 | S-6.1 verificar o CUR | **feito — hipótese confirmada** | CUR = 0,971; nada a corrigir |

## O que a execução corrigiu do plano

1. **S-3.2 tinha escopo errado.** O plano listava três constantes do
   `cli_suggester` como "orçamentos próprios". A leitura mostrou que só
   `LESSON_BUDGET` é orçamento de INJEÇÃO; `BUDGET` (rajada python-inline) e
   `G1_BODY_BUDGET` dimensionam o **programa que o remédio entrega**, e
   encolhê-los cortaria a correção em vez do ruído. O grep os agrupou; a leitura
   os separou. Quem alcança os 61% da injeção é S-3.3.

2. **A referência do dedup podia custar mais que o bloco.** A primeira forma
   tinha 144 B; `past-lessons` — a família com 86,4% de repetição — tem bloco
   médio de **48 B**. Deduplicar ali teria piorado exatamente o pior caso. O
   teste pegou, e o remédio é uma guarda aritmética: só elide quando a
   referência é menor.

3. **Os dois localizadores do F2.4 não eram iguais.** Colapsar duplicação exige
   ler os DOIS produtores; colapsei para o mais fraco e o teste pegou.

4. **A régua reportava 12× a mais.** Nove testes sintéticos verdes e o número
   ainda confiantemente errado: mensagens de usuário com `content` STRING — a
   forma comum de um prompt real — não contavam como turno. Só a sonda contra
   dados REAIS pegou.

## REGRA #21 — falhas pré-existentes fechadas no caminho

| falha | causa-raiz | correção |
|---|---|---|
| `e2e_test.py::test_phase_close_and_gate` | fixture `bundle` inexistente: um ERRO de coleta que contava como cobertura sem nunca executar | fixture criada |
| `test_cognicao_formal::mais_ausentes` | `chaves - set(...)` itera um SET: ordem por `PYTHONHASHSEED`, ranking diferente para a mesma entrada | desempate estável pela ordem do ESQUEMA |
| `test_doc_link_gate::mundos` | estratégia do Landlock com 0 elos causais onde o rito exige ≥5 | cadeia derivada da própria prosa, 6 elos |

## Não feito, e por quê

- **S-4.3 (classe B)**: o contrato existe e é testado; falta o executor que
  deriva o registro do turno. Construí-lo sem medir seria repetir D4.
- **Deploy**: nada propagado. `update-touring` muda o comportamento de toda
  sessão CC — aguarda ordem de Gabriel.
- **P5 ação**: a fração LIDA dos tool results segue não medida, então o digest
  ainda não é decidível. O volume está medido; o critério, não.
