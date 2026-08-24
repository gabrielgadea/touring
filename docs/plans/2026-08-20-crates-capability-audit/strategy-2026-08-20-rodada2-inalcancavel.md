---
okf_version: "1.0"
type: Strategy
title: "Rodada 2 — o que nem o binário alcança"
description: "Segunda rodada de exploração dos crates: capacidade compilada fora por feature gate, superfície MCP real, catálogo de hooks, proveniência do wiring, adoção de fragmentos e completude das 50 dims"
plan_id: 2026-08-20-crates-capability-audit
tags: [adw, loop-engineering, feature-gates, mcp, hooks, wiring-provenance, fragments]
timestamp: 2026-08-20T02:45:00-03:00
---

# Rodada 2 — o que nem o binário alcança

Ligado a [index](./index.md) · rodada 1 em [strategy-2026-08-20-capability-reach.md](./strategy-2026-08-20-capability-reach.md)

## A diferença entre as duas rodadas

A rodada 1 mediu capacidade inalcançável **pelos fluxos** — comandos que existem no
binário e nenhum spec chama. A rodada 2 desce dois níveis: capacidade inalcançável
**pelo binário** (compilada fora) e inalcançável **pela camada de composição**
(peças publicadas que nenhum fluxo monta).

## Lente A — capacidade compilada fora

`update-touring` roda `cargo build --release --workspace`, **sem `--features`**. Resolvi
o fecho transitivo de features a partir dos `default` de cada membro e cruzei com todos
os sítios `cfg(feature = …)` do workspace.

| medida | valor |
|---|---:|
| features declaradas | 262 |
| ativações resolvidas no build | 305 |
| sítios `cfg(feature)` **ativos** | 1.266 |
| sítios `cfg(feature)` **mortos** | **199** (13,6%) |

Os de maior consequência:

| feature morta | sítios | o que fica fora |
|---|---:|---|
| `touring-offensive:cvc5` | 55 | solver SMT — a execução concólica do `concolic.rs`/`solver.rs` |
| `mpatch-fuzzy` (4 crates) | 25 | aplicação de patch difusa |
| `acp-protocol` (3 crates) | 24 | Agent Client Protocol |
| `touring-intelligence:gpu-compute` | 11 | computação em GPU |
| `touring-hooks-saga:saga` | 10 | 2PC distribuído — **mas `touring saga status` responde** `{"transactions":[]}`: o verbo existe com a engine desligada |
| `touring-code:incremental-salsa` | 5 | reanálise incremental |
| `touring-storage:candle-bge` | 4 | embeddings locais |

**Módulos inteiros fora do binário** (`#![cfg(...)]` na primeira linha):

- `server/tools_new.rs` (357L) + `server/tools_status.rs` (151L) — feature `mcp-curated`
- `touring-intelligence/src/rl/semantic/quantized_bert.rs` (605L) — `semantic-embeddings`

Dois detritos: **`mcp-legacy` não gateia sítio algum** (flag morta) e
`crates/touring-cli/src/cli/mpatch.rs` existe **sem nenhum verbo `mpatch`** no `--help`.

> **Correção (E5, 20/08)**: o fato está certo, a insinuação de que o módulo é órfão
> estava errada. `cli-mpatch-preview` **está no registry de handlers** (3 sítios em
> `hook_registry.rs`), é invocável via `touring-hook cli-mpatch-preview`, e o stub
> `#[cfg(not(feature = "mpatch-fuzzy"))]` responde
> `{"error":"mpatch-fuzzy feature is not enabled","matched":false}` — capacidade
> opcional que **declara** estar desligada é bom desenho, não código morto. Ausência
> de verbo de topo é escolha, não esquecimento. Fica a decisão de produto (ligar os
> 25 sítios de `mpatch-fuzzy`), sem plano datado que a autorize como em E4.

## Lente B — a superfície MCP real

O handshake do servidor vivo (`tools/list` sobre stdio) expõe **23 ferramentas**.
As regras auto-carregadas falam em "85 MCP tools" e "99 MCP tools".

Três delas ficam fora por `mcp-curated`: `touring_tdg`, `touring_cortex_classify`,
`touring_hook_metrics`. E `touring mcp-overhead` responde `{"tools":[],"total_tokens":0}`
— não consigo distinguir "sem dados ainda" de "nunca alimentado"; fica registrado como
não-medido, não como defeito.

## Lente C — hooks que nunca disparam

`touring-hook` declara **40** eventos; o `settings.json` registra **33**.

**7 nunca disparam** — e são justamente os de auditoria e aprendizado:
`pii-scan` · `qa-syntax` · `session-audit` · `session-insights` · `session-end` ·
`classify` · `metrics`.

`pii-scan` nunca disparar significa que a detecção de PII **não roda em sessão alguma**.

## Lente D — inferlets (correção de leitura)

Minha primeira leitura disse "registro vazio" porque olhei só o cabeçalho da tabela.
O real: **14 módulos** no crate, **25 instalados** em `~/.claude/touring/inferlets/`,
**46 no pool** do runtime — `dependency_diff`, `find_circular_imports`,
`flaky_test_pattern_detector`, `tdg_grade_distribution`, `top_n_complex_files`,
`unused_pub_symbols`, `composite_health_trend`.

Estão instalados e funcionam. **Nenhum spec ADW chama `touring inferlets run`** — o
Reflexo #8 (Compute-in-Code para agregação em ≥3 arquivos) não tem realização em fluxo.

## Lente E — a proveniência do wiring

O grafo em que a cláusula `orphans_base` do gate de convergência repousa:

| origem | linhas | % |
|---|---:|---:|
| `ast_inferred` (heurística) | 63.527 | **82,1%** |
| `ast_declared` | 11.949 | 15,4% |
| `ast_resolved` | 1.389 | 1,8% |
| `ast_read` | 406 | 0,5% |
| `scip_resolved` | 89 | 0,1% |

**Apenas 1,9% das arestas vêm de um import resolvido de fato.** A memória
`wiring-64pct-heuristico` registra 2.981 arestas resolvidas; hoje são **1.478**
(`ast_resolved` + `scip_resolved`). Não afirmo regressão sem investigar o método de
contagem — mas a memória precisa ser reconciliada com esta medição.

Os 2.376 órfãos concentram-se em: `touring-intelligence` 460 · `touring-bindings` 321 ·
`touring-server` 235 · `touring-foundation` 188 · `touring-hooks-shared` 114.
Por kind: `method` 1.042 · `function` 1.012 · `const` 193.

## Lente F — o melhor instrumento, montado por ninguém

`touring-quality score crates/touring-ceg --format json` devolve as **50 dimensões
completas**: 44 Pass, 3 Warn, 3 NotApplicable, composite 0,9549 (Diamond) — com
evidência real por dimensão (`"LOC-weighted over 43 files: 0.834 (worst
enforce_linux.rs=0.550, p10=0.680)"`). N/A entra como 1.0 só no display e o composite
o **exclui** — desenho honesto, ao contrário do `repo-score` da rodada 1.

É o instrumento mais forte do sistema. E:

> **0 de 16 fragmentos publicados são compostos por qualquer um dos 9 specs da library.**

`gate-quality50` · `gate-rust` · `converge` · `critic-panel` · `audit-pack` ·
`plan-pack` · `phase-close` · `human-approve` · `conflict-guard` · `prior-art` ·
`recall-pack` · `diagnose-pack` · `fanout-lenses` · `graph-pack` · `skilling-pack` ·
`worker-critic-pair` — nenhum `[[use]]` em spec algum. O kit inteiro é órfão.

Some-se a rodada 1: os specs também não chamam `touring-quality` diretamente. Então a
harness de 50 dimensões não é alcançada **por via nenhuma** a partir de um fluxo.

## Lente G — geração

`touring generate list-kinds` expõe **36 GeneratorKind** com templates `.tera`.
Alcançado só por script, e só na forma `generate verify`.

## Deliverables da rodada 2

| # | entrega | lente | tam |
|---|---|---|---|
| **E1** | Compor `gate-quality50` nos specs de `audit`/`feature`/`bugfix` — o instrumento mais forte passa a ter caminho | F | **S** |
| **E2** | Registrar os 7 hooks inertes, começando por `pii-scan` e `qa-syntax` | C | **S** |
| **E3** | Reconciliar o `--features` do `update-touring`: decidir explicitamente, por feature, ligar ou remover (199 sítios mortos) | A | **M** |
| **E4** | Remover `mcp-legacy` (flag morta) e decidir `mcp-curated`: ligar (traz `touring_tdg`) ou apagar os 508L | A/B | **S** |
| **E5** | Dar verbo ao `mpatch` ou remover `cli/mpatch.rs` — REGRA #0, nunca meio-termo | A | **S** |
| **E6** | Corrigir as regras auto-carregadas: "85/99 MCP tools" → **23** medidos | B | **S** |
| **E7** | Nó `code` de `touring inferlets run` no lugar das agregações em shell dos specs | D | **M** |
| **E8** | Reconciliar a memória `wiring-64pct-heuristico` com a medição de hoje (1.478, não 2.981) | E | **S** |
| **E9** | Atacar órfãos onde concentram: `touring-intelligence` (460) via `assist auto_wire` | E | **M** |
| **E10** | Decidir `cvc5`: ligar (habilita concólico) ou remover `touring-offensive` do build | A | **M** |

## Sequenciamento

```
E1 ─→ E7            (compor primeiro, depois trocar agregação por inferlet)
E2 (independente)
E4 ─→ E3 ─→ E10     (decidir as flags mortas → política de features → caso caro)
E5 (independente)
E6 ─→ E8            (corrigir o declarado, depois reconciliar a memória)
E9 (independente, depende de E3 só se auto_wire estiver atrás de feature)
```

Acíclico. E1/E2/E4/E5/E6 são S.

## Riscos

| risco | prob | impacto | mitigação |
|---|---|---|---|
| Ligar features mortas quebrar o build (E3/E10) | ALTA | ALTO | uma feature por vez, `cargo check` entre cada; `cvc5` exige binário externo — medir antes |
| `pii-scan` registrado gerar falso positivo em massa (E2) | MÉDIA | MÉDIO | rodar em modo observação e contar antes de deixar bloquear |
| Compor `gate-quality50` reprovar fluxos hoje verdes (E1) | ALTA | BAIXO | é o objetivo; piso Gold 0.80 é o já declarado no gate de convergência |
| Remover `mcp-curated` apagar tool que alguém usa (E4) | BAIXA | MÉDIO | os 3 nomes não aparecem em `tools/list` há builds — ninguém pode estar usando |

## O que esta rodada NÃO mediu

- Se os 46 inferlets do pool produzem resultado correto (existem ≠ funcionam).
- Cobertura de mutação (`mutation-test` segue sem rodar — caro).
- `touring-intelligence` (80k LOC, maior concentração de órfãos) não foi lida por dentro;
  só medida por fora.
- Se as 23 tools MCP cobrem o que os 167 nomes `touring_*` do fonte sugerem.
