---
type: AuditReport
title: "Cross-audit — portfólio de capacidades por propósito"
description: "Auditoria cruzada de fidelidade-ao-propósito de tudo implementado em 08/08/2026, com evidência executada por fase."
tags: [cross-audit, portfolio, wiring, enriquecimento-de-contexto]
plan_id: 2026-08-08-portfolio-de-capacidades
timestamp: 2026-08-08T21:00:00-03:00
---

# Cross-audit — 08/08/2026

Escopo: tudo que esta sessão implementou — o **portfólio de capacidades por
propósito** (P1–P6), o conserto do `find-code`, a blindagem das ADWs contra
injeção de template, a doutrina de enriquecimento de contexto nas três skills, e
o que a própria auditoria encontrou e corrigiu.

A pergunta desta auditoria não é "quebra?" — é **"cumpre o propósito
documentado?"**, e a resposta só vale acompanhada do comando executado.

---

## Veredito

**APROVADO com 8 achados, todos corrigidos e provados em execução.**

| Gate | Resultado | Comando |
|---|---|---|
| Suíte completa do workspace | **15.431 passaram · 0 falharam** (270 suites) | `cargo test --workspace --no-fail-fast` → `TEST=0` |
| Clippy `-D warnings` | **0** | `cargo clippy --workspace --all-targets -- -D warnings` → `CLIPPY=0` |
| Prova E2E do portfólio | **45/45** | `python3 docs/audits/prove_portfolio_e2e.py` |
| Guards de injeção ADW | **41/41** | `pytest ~/.claude/skills/Touring/scripts/test_adw.py` |
| Doctor | **6/6 ok** | `touring doctor -j` |
| Gate de links OKF | **CLEAN (5 docs)** | `loop_doc_link_gate.py --bundle …` |

O achado mais importante não estava no código novo: estava numa correção que **eu
mesmo tinha declarado pronta** (F7). Ela cobria 1 dos 5 sítios que gravam a mesma
aresta. Está na seção F8.

---

## Fase 1 — MAP

Superfície implementada (3.752 linhas de Rust novo, fora testes):

| Camada | Arquivo | L | Papel |
|---|---|---|---|
| foundation | `text_rank.rs` | 213 | o único BM25 do crate (`tool_catalog` delega) |
| foundation | `portfolio/mod.rs` | 314 | `CapabilityKind`, `Evidence`, `Verdict`, `SemanticScorer` |
| foundation | `portfolio/lexicon.rs` | 317 | normalização bilíngue simétrica pt↔en |
| foundation | `portfolio/query.rs` | 625 | ranking + contrato anti-âncora (3 seções + veredito) |
| foundation | `portfolio/store.rs` | 260 | índice em disco, escrita atômica |
| foundation | `portfolio/feedback.rs` | 244 | `verdicts.jsonl` append-only (o feromônio) |
| server | `portfolio/miner.rs` | 922 | 8 extratores de propósito + herança por bundle |
| server | `portfolio/semantic.rs` | 157 | fastembed opcional, gated por env |
| server | `portfolio/keyword.rs` | 148 | implementa `KeywordSearch` (o seam do `find-code`) |
| server | `cli/portfolio.rs` | 538 | `query · refresh · status · verdict · inspect · history` |

Pontos de acoplamento onde o enriquecimento entra **por construção**, não por
lembrança: catálogo de lentes do CCE · nós `recall`/`prior_art` das ADWs ·
`PreToolUse` do hook antes de um `Write`.

---

## Fase 2 — PURPOSE AUDIT

O propósito declarado: *antes de criar, explorar o que já existe e apresentar
como portfólio — sem limitar a LLM a ele*. Provado por execução:

```
$ touring portfolio "gerar um PDF profissional"
corpus : 10863 artefatos indexados
  1. generate_pdf        (25.10) …professionally formatted PDFs…
  2. gerar_pdf_premium   (21.40) …WeasyPrint (não wkhtmltopdf), porque só ele
                                  oferece hifenização pt-BR real (pyphen)…
  3. pdf [skill]         (21.01) · 4-5 scripts do bundle [propósito herdado]
```

O item 2 é a prova de que a chave certa é **propósito**, não identificador: o
artefato explica *por que* WeasyPrint venceu wkhtmltopdf — exatamente a decisão
que a LLM repetiria do zero. Nenhuma busca por nome o encontraria.

Contra-prova (E4/E5 — ausência exibida, corpus declarado):

```
$ touring portfolio "treinar rede neural convolucional"
corpus : 10863 artefatos indexados
── prior art ──  (nenhum)
```
Uma resposta magra se lê como magra, não como autoridade.

Composição do corpus: `script 3952 · symbol 6795 · skill 89 · module 19 · adw 8`
(`touring portfolio status -j`), 10 com propósito herdado do bundle.

---

## Fase 3 — DEBT SCAN

- `rl/aco/template_library.rs`: 376 linhas de infraestrutura semântica **sem
  nenhum chamador**. Potencializado (REGRA #0): `FastEmbedSimilarity` implementa
  o `EmbeddingStore` órfão *e* o `SemanticScorer` novo — um corpo serve os dois
  (`one_body_serves_both_traits`).
- Dois testes do `hybrid_search` afirmavam `!results.is_empty()` contra o corpus
  fabricado: **passavam porque o defeito existia**. Reescritos para o contrato
  real. Um teste que defende o bug é, ele mesmo, um achado.

---

## Fase 4 — HARMONY

`touring doctor -j` → 6/6 `ok`. Índice: 78.143 linhas de wiring,
`kind_unknown=0`, `non_rust=0`, `abs_paths=0`.

Aqui apareceu o fio que puxou o F7 e depois o F8: o contador ficou verde **sem
que o defeito tivesse sido corrigido** — ver abaixo.

---

## Fase 5 — FIX & POTENTIALIZE

Oito achados. Nenhum resolvido removendo capacidade.

| # | Achado | Correção (potencializa) |
|---|---|---|
| **F1** | o veredito prometia gravar em memória/learning e nunca gravava | espelho best-effort para `cli-memory-store` + `cli-learning-reward`; o JSONL local segue canônico (portfólio é global, memória é por projeto) e a indisponibilidade é **impressa**, não escondida — provado nos dois estados: `memória institucional: indisponível (log local é canônico)` com o daemon ocupado no rebuild, e `memória institucional: ok · reward RL: emitido` fora dele |
| **F2** | `with_keyword_backend` sem nenhum implementador em produção | `PortfolioKeyword`; `load()` devolve `None` em índice vazio para `is_unwired()` continuar verdadeiro |
| **F3** | 7 símbolos `pub` sem consumidor | viraram `portfolio inspect` e `portfolio history` — superfícies de depuração reais |
| **F4** | cache do hook nunca invalidado (`OnceLock` em daemon longevo) | invalidação por mtime |
| **F5** | banner de licença virava o "intento" derivado | filtro de boilerplate que **pula e continua procurando** |
| **F6** | skills sem a doutrina de enriquecimento | E1–E9 em `Touring`, `loop-engineering` e `TACO-cross-audit` |
| **F7** | reexport intra-crate não seguido | `follow_intra_crate_reexport` |
| **F8** | **o F7 cobria 1 de 5 sítios** | `definer_module` + guard estrutural — abaixo |

### F8 — a assimetria C08 que a própria auditoria produziu

`touring doctor` tinha voltado a `ok` com `kind_unknown=0`. Isso era
**mascaramento, não conserto**:

```
module_file  crates/touring-storage/src/hybrid_search/mod.rs   ← ainda a fachada
symbol_kind  extern                                            ← era 'unknown'
```

`backfill_unknown_consumer_kinds()` reclassifica `unknown → extern` quando não
existe produtor algum para aquele nome, significando "reexport de crate externa".
Para `KeywordSearch` isso é falso: o símbolo é definido no próprio workspace. O
contador que revelou o defeito parou de revelá-lo.

A causa real: `record_consumer_from_path` (hook) recebeu o salto; o
`index rebuild` — que grava a **maioria** das linhas — não. Cinco sítios de
produção gravam a mesma aresta e resolviam a mesma pergunta de formas diferentes:

| Sítio | Antes | Depois |
|---|---|---|
| `touring-cli/…/index.rs:677` (rebuild) | módulo resolvido cru | `definer_module` |
| `touring-hook-handlers/…/post_read.rs:299` | módulo resolvido cru | `definer_module` |
| `touring-hook-handlers/…/post_read.rs:307` | fallback `crate::` cru | `definer_module` |
| `touring-hook-runtime/…/wiring.rs:633` (reexport aninhado) | submódulo cru | `definer_module` |
| `touring-hook-runtime/…/wiring.rs:691` | F7 (só aqui) | `definer_module` |

Três exceções ficaram **documentadas no próprio guard**: chave de pacote Go
(`go:<path>`, não é arquivo), passe F9 de method-dispatch (vem de linha de
produtor) e o reparo de wiring (vem de linha de produtor órfã).

O guard `record_consumer_sites_resolve_the_definer` varre `crates/*/src/**` e
falha se um sítio novo gravar consumidor sem passar pelo helper. **Ele achou 3
dos 5 sítios sozinho** — eu tinha visto 2.

`follow_intra_crate_reexport` (o salto cru) passou a **privado**: `definer_module`
é a única porta, então nenhum sítio pode contornar o ponto único que o guard
protege. Não é redução de escopo — é o invariante que o F8 existe para criar.

Custo: `ModuleFacts` (definições + reexports por arquivo) com uma única regex
compilada e cache invalidado por mtime — a mesma lição E9 que esta auditoria
levantou no F4, aplicada ao próprio conserto. O rebuild ficou **mais rápido**:
146 s → **134 s** (a regex por símbolo era mais cara que o salto que o cache paga).

**Medido em produção** (`update-touring` + `touring index rebuild --dir $PWD`,
3.152 arquivos, 67.851 símbolos, `errors=0`):

| Métrica | Antes | Depois |
|---|---:|---:|
| `KeywordSearch` — produtor / consumidor | `pipeline.rs` / **`mod.rs`** | `pipeline.rs` / **`pipeline.rs`** |
| linhas de consumo sem produtor no mesmo módulo | 960 | **361** (−62%) |
| órfãos estruturais (consulta SQL) | 5.399 | **5.177** |
| `orphan_count` (`touring wiring orphans -j`) | 4.235 | **4.006** |
| `symbol_kind='extern'` | 15 | 12 |
| `kind_unknown` | 0 | 0 |

222 símbolos deixaram de ser reportados como órfãos porque **nunca foram**
órfãos: o consumidor existia e estava sendo creditado à fachada. É a REGRA #0 ao
contrário — o medidor é que estava reduzindo o escopo do código.

---

## Fase 6 — E2E PROOF

```
$ python3 docs/audits/prove_portfolio_e2e.py
45/45 checks passaram
```

45 asserções cobrindo: propósito minerado por extrator, herança por bundle,
ranking bilíngue, ausência exibida, corpus declarado, veredito exigido,
`inspect`/`history`, injeção de template ADW (forma antiga injeta — controle;
forma blindada não injeta e o valor chega íntegro) e **invariante exit 0** em 12
entradas de borda (sem argumento, só stopwords, unicode, metacaracteres de shell,
subcomando desconhecido, arquivo inexistente).

Lente `portfolio` viva no catálogo automático do CCE — rodada ao vivo:

```
institutional 12 · antistaleness 12 · quality 4 · portfolio 5
```

Nenhuma rodada conta como "seca" sem que **toda** lente tenha rodado, então o
prior-art-por-propósito é estruturalmente inescapável.

---

## Hipóteses refutadas (registradas para não voltarem)

- CRLF **não** quebra a extração de docstring.
- A blindagem das ADWs preservou a semântica: em `arm_marker`, `scope` aparece
  duas vezes e mapeia para `$1` nas duas; aridade consistente nos 16 comandos
  transformados.
- `proptest_parser_fuzz` com `signal: 15` **não** era defeito de código: passa
  isolado em 3,20 s; era pressão de recursos (`target/` a 303 GB, disco 96%).
  `safe-clean.sh incremental` liberou 63,55 GB.

---

## O que não foi executado

Nada nesta auditoria ficou `UNVERIFIED`. Onde uma medição substituiu uma
suposição, a suposição está registrada acima como refutada.
