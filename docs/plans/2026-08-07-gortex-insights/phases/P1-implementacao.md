---
type: PhaseReport
title: "P1 — Implementação dos refinamentos gortex (4 de ~13, com prova executada)"
description: "V4 contenção do walker, S5 marcador de truncagem, S3 eval de retrieval real, S2 protocolo de baseline. Cada um com prova executada; o que não foi feito está listado sem meia-implementação."
plan_id: 2026-08-07-gortex-insights
tags: [implementacao, symlink, truncagem, retrieval-eval, baseline, regra-21]
timestamp: 2026-08-07T21:45:00-03:00
okf_version: "0.1"
---

# P1 — Implementação

Quatro itens entregues **com prova executada**. O resto está aberto e listado no fim —
nenhuma meia-implementação.

## V4 — Contenção do walker de indexação ✅

**Arquivo**: `crates/touring-cli/src/cli/handlers/index.rs`

Helper module-level `inside_root(path, root) -> Option<PathBuf>`: canonicaliza e exige
`starts_with(root)`; o chamador alimenta um `HashSet` `visited` com o canônico devolvido.
Fecha três modos que `Path::is_dir()` deixava abertos (ele chama `fs::metadata`, que
**resolve** o link):

| Modo | Antes | Agora |
|---|---|---|
| Escape | `ln -s /etc proj/x` indexava conteúdo de fora no `symbols.db` | recusado |
| Ciclo | `a/link -> a` recursava até estourar a pilha | termina |
| Link quebrado | erro de leitura no meio da varredura | pulado |

Symlink **interno** continua sendo seguido — a guarda remove o escape, não a capacidade
(REGRA #0). Aplicada nos **dois** walkers (`walk` em `cli_index_rebuild` e `walk_dir` em
`cli_ast_modules`): guarda em 1 de 2 sítios simétricos é a forma de bug que a matriz de
decisão chama de C08. Arquivos também são guardados — um `.rs` symlinkado escapa igual.

**Prova**: 5 testes unitários + 1 E2E in-process (`rebuild_containment_e2e`) que roda o
`cli_index_rebuild` **real** com escape + ciclo + alias interno → **0,08 s**, canary de fora
não indexado, símbolos de dentro indexados.

**Gotcha crítico descoberto**: `touring index rebuild` é `daemon_query` — roda o binário do
**daemon**, não o recém-compilado. O daemon em execução (release de 15:04) **travou 120 s**
no mesmo cenário: o defeito antigo se manifestando ao vivo. Um teste no nível de CLI
testaria a build anterior em silêncio; por isso o E2E é in-process.

## S5 — Marcador de truncagem legível por máquina ✅

Campo `truncated: bool` em `DimScore` (`#[serde(default)]` para relatórios antigos), setado
em `score_scope_native` quando `dir_scan_overflow` dispara. Antes o fato só existia como
prosa dentro de `evidence` — nenhum consumidor podia ramificar sobre ele.

Nova cláusula `measured_whole_scope` no `loop_converged.py`: **recusa convergir sobre medida
parcial**; `N/A` quando `quality` é imensurável (não conta duas vezes a mesma causa raiz).

**Prova**: `"truncated": false` no JSON do `touring-quality`; 4 casos da cláusula
(`None`→N/A, `[]`→pass, 1 dim→fail, 2 dims→fail).

## S3 — Eval de retrieval real substituindo o tautológico ✅

**O que saiu**: 3 casos hardcoded, comentário dizendo *"Test RRF hybrid search"* enquanto
chamava `cli-index-find` (lookup exato por chave), gate passando com `accuracy >= 0.50` —
isto é, **errando 1 de 3**.

**O que entrou**: `bench/retrieval.json` com **82 casos** de ground truth verificado contra
`symbols.db` (só definições **únicas** em produção; 3 descartados por homonímia), 3 tiers
(`exact`/`concept`/`multi_hop` com semântica any-hit), consultando `cli-tantivy-search` — o
caminho ranqueado real —, com R@1/R@5/R@20/MRR overall e por tier, e a lista dos
`missed_case_ids`. Fail-**closed** se a fixture não carrega.

**Resultado medido**:

| tier | n | R@1 | R@5 | R@20 | MRR |
|---|---|---|---|---|---|
| overall | 82 | 0,805 | 0,841 | 0,866 | 0,822 |
| exact | 69 | 0,942 | 0,986 | 0,986 | 0,959 |
| **concept** | 10 | **0,100** | **0,100** | **0,300** | 0,123 |
| multi_hop | 3 | 0,000 | 0,000 | 0,000 | 0,000 |

**Confundidor meu, achado e corrigido antes de reportar**: as queries `concept` estavam em
**português** contra um corpus em inglês — a primeira medição deu `concept = 0,000`.
Reescritas em inglês, subiu para 0,100/0,300. Parte do zero era falha da minha fixture; mas
o tier `concept` segue **genuinamente fraco** (R@1 = 10%). O Gortex publica 25,4% no mesmo
tier.

**Prova**: 6 testes das métricas (any-hit, miss = `None` nunca 0, R@k cumulativo, hit além
de k não conta, conjunto vazio sem divisão por zero) + guard de boa-formação que **recusa uma
query `concept` que vaze o nome do símbolo** (seria uma query `exact` disfarçada, inflando o
tier com acerto lexical).

## S2 — Protocolo de baseline com epsilon ✅

Embutido no S3, que vira seu primeiro consumidor. `bench/retrieval-baseline.json` +
`TOURING_RETRIEVAL_EPSILON` (default 0,02) + flag `--update-baseline`. Status:
`pass` / `fail` / `baseline_missing` / `baseline_updated`. `baseline_missing` **nunca** é um
pass silencioso nem uma falha que bloqueia a primeira execução.

**Prova executada, os quatro caminhos**:

1. grava → `baseline_updated`
2. re-roda sem mudança → `pass`, `regressions=[]`
3. baseline adulterado para 0,99 → **`fail`** com `r_at_5 −0,149` e `mrr −0,168`
4. baseline = atual + 0,010 (< epsilon) → `pass`, ruído absorvido

Isto é o que faltava para `orphans_base`: regravar deixa de ser `rm` e passa a ser um comando
nomeado, com tolerância declarada e registro.

## Gates

- **2133 testes**, 0 falhas (382 quality + 313 cli + 1438 server)
- **clippy `-D warnings` limpo** — as 2 mensagens restantes são MSRV-config e
  future-incompat do `proc-macro-error2`, ambas pré-existentes e de dependência
- **6 dims P0 BLOCK no código novo**: `blockers: []`
- Blockers por arquivo em `index.rs` são **pré-existentes**: `F1_2` é o `cli_index_rebuild`
  que já era CC=60 (subiu para 63 com a guarda), `F3_1` lê `target/llvm-cov/lcov.info` que
  **eu apaguei na higiene de disco de hoje** (artefato ausente ≠ cobertura zero), `F3_11` é
  "sem README no escopo" de um arquivo único

## Não entregues — abertos, sem meia-implementação

| # | Item | Nota |
|---|---|---|
| S1 | Proveniência de aresta no wiring | `contract_source` segue constante `ast_read` |
| S4 | Colunas de memória (`importance`/`pinned`/`superseded_by`) | — |
| A1 | MinHash/LSH para F1.3 | — |
| A2 | Contagem real de tokens | **Descoberta relevante**: `touring-cortex/src/enrichment.rs:316` já tem encoder `cl100k_base` funcionando, e `mcp.rs:393` tem o comentário *"tiktoken-rs, see follow-up issue"* logo acima do `bytes/4`. **Mas** `ctx_gain` calcula `contadores × constante hardcoded (30_000/20_000)` — não há bytes reais para tokenizar. O conserto certo é instrumentar bytes reais no sítio de compressão, **não** trocar o divisor. Trocar o divisor daria precisão falsa sobre um número inventado |
| C1 | Emagrecer schema do `tools/list` | 33.161 B para 23 tools |
| A5 | Reach index precomputado | — |
| A6 | Elisão de corpo | — |
| A7b | Compactação condicional do DB | — |
| B7 | Cláusula de delta negativo | — |
