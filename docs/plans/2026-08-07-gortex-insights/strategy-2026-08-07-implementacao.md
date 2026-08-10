---
type: Strategy
title: "Estratégia de implementação dos insights gortex — fases P2..P7"
description: "Consolidação OUTER: 4 itens já entregues (P1), 9 implementáveis restantes ordenados por alavanca÷custo, 3 itens de governança separados para decisão do Gabriel. Cada fase com prova executada e contenção de risco declarada."
plan_id: 2026-08-07-gortex-insights
tags: [estrategia, gortex, wiring-proveniencia, memoria-aco, tokens, minhash]
timestamp: 2026-08-07T22:25:00-03:00
okf_version: "0.1"
---

# Estratégia — implementação dos insights gortex

Entrada: [exploration-gortex.md](exploration-gortex.md) (2 rodadas + verificação, 21 ações).
Já entregue: [phases/P1-implementacao.md](phases/P1-implementacao.md) — **V4, S5, S3, S2**.

## Diagnóstico de partida (executado 22:20)

| Sinal | Valor |
|---|---|
| `touring doctor` | 6/6 ok |
| composite 50-dim | **0,9386 — Platinum**, `blockers: []` |
| warnings | F1_1, F1_2, F1_3, F4_5 |
| orphans | 4244 (baseline travada em 5109) |
| CCE ledger | `converged: True`, 18 findings, 5 rodadas, 6 lentes |

## Achado do OUTER que muda o plano

**`crates/touring-hooks-core/src/knowledge_wiring.rs` (879 linhas) não é compilado.**

`lib.rs:66` faz `pub use touring_storage::knowledge_wiring;` e não existe `mod
knowledge_wiring`. Prova executada: erro de sintaxe injetado no arquivo → `cargo check -p
touring-hooks-core` **verde** → arquivo restaurado.

Consequência direta para o S1: o que parecia um par simétrico C08 (mesmos 3 `INSERT` com
`'ast_read'` literal em dois crates) é **um sítio real e um cadáver**. Espelhar a correção
no cadáver seria trabalho sobre código que o compilador nunca vê — e é exatamente o tipo de
falso-simétrico que a matriz de decisão manda procurar. O arquivo entra como dívida REGRA #0
da fase P2.

## Ordenação — alavanca ÷ custo

| Fase | Item | Por que aqui | Custo |
|---|---|---|---|
| **P2** | **S1** proveniência de aresta | única cláusula de convergência aberta (`orphans_base`); particiona órfão-real vs falha-de-resolvedor | baixo |
| **P3** | **S4** memória ACO | feromônio com 0,15% de reward populado — falha na essência declarada | baixo-médio |
| **P4** | **B7** + **A7b** | cláusula de delta negativo + compactação condicional; ambos são política, não algoritmo | baixo |
| **P5** | **A2** contagem de tokens | tese fundadora medida por divisor inventado | médio |
| **P6** | **C1** schema do `tools/list` | 33.161 B em toda sessão, 2,2× o alvo do facade | médio |
| **P7** | **A1** MinHash/LSH | F1.3 em Warn há 3 fases; Type-2 é invisível ao detector atual | médio-alto |

**A5** (reach precomputado) e **A6** (elisão de corpo) ficam fora deste ciclo: custo alto e
sem urgência medida — o `wiring impact` responde hoje sem reclamação de latência, e A6
depende de A2 para ser comprovável.

## Contenção de risco declarada — P2

Gravar `unresolved` no `wiring_map` tem um modo de falha que inverteria o objetivo: se a
linha usasse o `module_file` real, ela contaria como **consumidor** e apagaria órfãos
verdadeiros — eu fabricaria o oposto do defeito que quero medir.

Contenção: `module_file = 'unresolved::<module_path>'`. Nunca casa com um produtor por JOIN
(não é um path `.rs`), é contável por prefixo, e exige caminho de escrita dedicado porque o
gate `is_indexable_module_file` corretamente o rejeitaria.

**Teste que prova a contenção** (sem ele, S1 é regressão silenciosa na própria métrica que
pretende melhorar): gravar N linhas unresolved e exigir que `orphan_symbols()` devolva
**exatamente o mesmo conjunto** de antes.

## Tiers de proveniência — mapeados aos call sites reais

| Tier | Origem no código | Evidência |
|---|---|---|
| `ast_declared` | `register_pub_symbol` — símbolo extraído do AST do produtor | forte |
| `ast_resolved` | `index.rs:664` — `use` resolvido a um arquivo real | forte |
| `ast_inferred` | `index.rs:690` — F9 casa **só por nome**, capado a 4 produtores | **heurística** |
| `text_matched` | extratores regex para linguagens sem AST | fraca |
| `unresolved` | `index.rs:664` ramo `else` — hoje **vazio**, o dado é descartado | ausência |

Compatibilidade: `record_consumer` delega para `record_consumer_with_origin` com
`AstResolved` (o caso dominante — 77.558 de 77.679 linhas são `rust_import`). Só o call site
F9 migra para `AstInferred` explícito. O default reflete o que os callers legados de fato
fazem; a mentira removida é a heurística contada como AST resolvido.

## Fora deste ciclo — exigem decisão do Gabriel

| # | Item | Por que é decisão dele |
|---|---|---|
| A4 | hook mode `rewrite` / `deny` | muda o comportamento das ferramentas da sessão dele |
| A3 | GCX1 como formato de wire | dependência de spec de terceiro |
| B1 | linter de `CLAUDE.md`/`rules` | mexe em `~/.claude/`, território do operador |

## Critério de saída

`loop_converged.py` exit 0. Por fase: `cargo check` + `clippy -D warnings` + testes dos
crates tocados + prova executada específica do item (REGRA #21 — 0 falhas).
