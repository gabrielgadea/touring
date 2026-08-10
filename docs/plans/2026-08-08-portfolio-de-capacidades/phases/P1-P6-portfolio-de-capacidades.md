---
okf_version: "1.0"
type: PhaseReport
title: "P1–P6 — Portfólio de Capacidades: prior-art por propósito"
description: >
  Índice global de artefatos keyed por propósito (não por nome), com contrato
  anti-âncora de três seções, afordâncias no explore/ADW/hook, veredito
  persistido, re-rank semântico opt-in, e conserto do find-code fabricado.
tags: [portfolio, prior-art, bm25, bilingue, afordancia, regra-0, regra-21]
timestamp: 2026-08-08T15:40:00-03:00
plan_id: 2026-08-08-portfolio-de-capacidades
---

# P1–P6 — Portfólio de Capacidades

## O que foi entregue

| Fase | Entrega | Onde |
|---|---|---|
| **P1** | Minerador de propósito (2 níveis + herança de bundle) · índice global · `touring portfolio` com contrato de 3 seções | `touring-foundation/src/portfolio/{mod,lexicon,query,store,feedback}.rs` · `touring-server/src/portfolio/miner.rs` · `touring-server/src/cli/portfolio.rs` |
| **P2** | Lente `portfolio` no `explore` (inescapável pelo contrato CCE) · nó de prior-art em `feature.toml` e `chore.toml` | `~/.claude/skills/Touring/scripts/explore_until_dry.py` · `client/skills/Touring/adw-library/` |
| **P3** | Veredito persistido (append-only JSONL) reaplicado como evidência no refresh | `portfolio/feedback.rs` |
| **P4** | Re-rank semântico opt-in (`TOURING_PORTFOLIO_SEMANTIC=1`) que também dá o **primeiro implementador** ao `EmbeddingStore` órfão da ACO | `touring-server/src/portfolio/semantic.rs` |
| **P5** | Injeção de prior-art no PreToolUse de `Write` (arquivo novo), com intento derivado do conteúdo | `touring-cli/src/cli_suggester.rs` |
| **P6** | `find-code` parou de fabricar resultados; `KeywordSearch` como seam injetável; `BackendStatus` no retorno | `touring-storage/src/hybrid_search/hybrid/pipeline.rs` · `touring-server/src/{tools/search_tools.rs,cli/find_code.rs}` |

## Medições (FACT [1.0])

**Corpus**: 4.068 artefatos indexados em **187 ms** — 3.952 scripts, 89 skills, 19 módulos, 8 ADWs; 10 com propósito herdado do bundle.

**Os dois exemplos do pedido**:

| intento | 1º resultado | observação |
|---|---|---|
| `gerar um PDF profissional` | `~/.claude/skills/literature-review/scripts/generate_pdf.py` | trouxe também `gerar_pdf_premium.py` (analise), que documenta *por que* WeasyPrint e não wkhtmltopdf — a "estratégia já utilizada" que o pedido queria; e os scripts de `pdf-anthropic` por **herança de bundle** |
| `gerar mapa geográfico` | `~/projects/analise/scripts/process_analysis/gerar_mapas_bamin.py` | desambigua corretamente mapa-artefato vs. mapear-verbo |
| `treinar rede neural convolucional` | *(vazio)* | devolve `prior_art: []` e **declara** a ausência com o tamanho do corpus |

**Antes** (mesmo intento, ferramentas existentes): `touring search-tools "gerar PDF profissional"` → `No matching tool`; a versão em inglês devolvia `touring generate verify --symbol <name>` — a mesma resposta genérica que dava para "generate a map".

### Duas granularidades, e o ajuste que a medição exigiu

O pedido nomeia "scripts, **funções, classes, símbolos**". A primeira versão
indexava só artefatos; a segunda passou a minerar também `def`/`class` (Python)
e `pub fn`/`struct`/`trait`/`enum` (Rust) documentados.

Medido logo depois: **a granularidade fina degradou a consulta de artefato.**
Com o piso de prosa do módulo (20 chars), stubs como `main` — docstring
`"Command-line interface."` (23 chars) — entravam no corpus e **ultrapassavam**
os artefatos reais de PDF, porque a normalização por comprimento do BM25 favorece
documentos curtos: um one-liner genérico bate um parágrafo preciso.

Dois ajustes, ambos medidos:

| ajuste | efeito |
|---|---|
| `MIN_SYMBOL_PURPOSE_LEN = 40` (símbolo exige mais prosa que módulo) | 9.826 → 6.795 símbolos; 3.031 stubs fora |
| `SYMBOL_WEIGHT = 0.8` (o artefato é a unidade acionável; o símbolo é detalhe) | desempate a favor do que se pode executar |

Resultado: `gerar um PDF profissional` voltou a devolver só artefatos, e
`função que converte HTML em markdown` mantém a classe
`ANTTHtmlToMarkdownConverter` em 2º — que é exatamente o grão que o pedido queria.
Travado em `an_artifact_outranks_a_symbol_of_equal_relevance`,
`a_symbol_still_wins_when_it_is_genuinely_more_relevant` e
`stub_docstrings_do_not_enter_the_corpus`.

**Corpus final**: 10.863 artefatos (3.952 scripts · 6.795 símbolos · 89 skills ·
19 módulos · 8 ADWs), minerados em **236 ms**.

## Decisões de desenho e por quê

1. **A chave estava errada, não o motor.** 96% dos 3.881 scripts `.py` têm prosa de propósito (média 411 chars) num campo que nenhum índice lia. Indexar esse campo resolveu o que embeddings não resolveriam.

2. **Léxico bilíngue simétrico.** Corpus majoritariamente em inglês, intentos frequentemente em português. Ambos os lados são normalizados para o mesmo termo canônico ("mapa"→`map`, "gerar"→`generate`), então eles se encontram no meio. Tabela curada, offline, determinística — termos fora dela passam intactos (PDF, SIMD, tantivy).

3. **Herança de bundle não é detalhe.** Os 8 scripts de `pdf-anthropic` — o exemplo canônico do pedido — não têm docstring. Minerar só scripts teria perdido exatamente o caso pedido.

4. **Piso por cobertura de termos, não por score.** Um piso absoluto de BM25 escondia o candidato certo quando o termo aparecia em todos os documentos (IDF≈0). Cobertura é invariante ao tamanho do corpus. Registrado no teste `a_term_common_to_every_document_still_returns_its_candidates`.

5. **Lacuna é afirmação de ausência** e por isso usa comparação por prefixo, não exata: dizer "nenhum candidato menciona professional" quando um diz "professionally formatted" é uma afirmação falsa. Teste: `a_gap_never_fires_on_a_morphological_variant`.

6. **Uma única implementação de BM25.** O `tool_catalog` passou a delegar ao `text_rank` compartilhado; tokenizadores seguem distintos (catálogo inglês vs. portfólio bilíngue).

## REGRA #0 — órfãos potencializados

- `touring_intelligence::rl::aco::template_library::EmbeddingStore`: **0 implementadores** antes; agora `FastEmbedSimilarity` implementa ele e o `SemanticScorer` do portfólio com o mesmo corpo. Teste estrutural: `one_body_serves_both_traits`.

## REGRA #21 — falha encontrada e corrigida

`touring find-code search` devolvia corpus fabricado (`doc_kw_1..5` + `doc_sem_1..5`, scores constantes, idêntico para qualquer consulta, inclusive em português). Ambas as pernas eram `synthetic_*`; a semântica chegava a calcular o embedding real e **descartá-lo**. Dois testes existentes (`test_search_no_rerank`, `test_search_with_rerank`) asseguravam `!results.is_empty()` — **codificavam o bug**. Reescritos para o contrato real.

Também removido o `InMemoryVectorStore::default()` vazio que os dois call-sites construíam por chamada: um store vazio wirado fazia o relatório dizer "consultados, sem correspondência" quando nunca houve corpus.

**Fora de escopo, declarado**: popular um corpus de embeddings sobre os ~270k símbolos do índice é subsistema próprio (download de modelo, embedding em lote, persistência, refresh) — não foi construído. O `find-code` hoje é honesto e aponta `touring tantivy search` / `touring portfolio`.

## Segurança — injeção de template nas ADWs (achado colateral)

O review automatizado apontou o nó que eu tinha acabado de adicionar ao
`chore.toml`. O achado era válido e o padrão era **pré-existente**: 26 comandos
em 7 dos 9 specs interpolavam `{{vars.X}}` dentro da string de script de
`["bash","-c", …]`. Como `adw.py:363` renderiza por elemento de argv e chama
`subprocess.run(list)` sem `shell=True`, um valor renderizado **dentro** do
script é parseado pelo bash — um ticket com aspas e separadores escapa.

Corrigido para parâmetros posicionais em toda a library **e** nas cópias
instanciadas em `.touring/adw/`:

```toml
command = ["bash", "-c", "touring portfolio \\"$1\\" --top 3", "--", "{{vars.task}}"]
```

Provado por execução, não por afirmação: a forma antiga injetou, a nova não, o
valor continua chegando literal, e `write_set` mantém o word-split com o
separador virando argumento literal.

Duas exceções deliberadas: `verify_cmd` (é um comando por contrato) e
`write_set` (`$1` sem aspas — preserva o split, neutraliza execução).

Guard estrutural sobre **todos** os specs de uma vez —
`test_adw.py::test_no_data_variable_is_interpolated_inside_a_shell_script` —
porque o conserto por arquivo foi exatamente o que deixou o padrão se espalhar
por sete specs. Suíte ADW: 41/41.

## Uso

```bash
touring portfolio refresh                       # minera (~190ms, 4k artefatos)
touring portfolio "gerar um PDF profissional"   # 3 seções + veredito exigido
touring portfolio status                        # cobertura do índice
touring portfolio verdict "gerar PDF" --choice extend --why "cobre 80%, falta hifenização"
export TOURING_PORTFOLIO_SEMANTIC=1             # re-rank semântico (baixa modelo no 1º uso)
```

## Referências

- Estratégia: `/strategy-2026-08-08-portfolio-de-capacidades.md`
- Diagnóstico: `/diagnostics/touring-20260808T120605.md`
- Ledger CCE: `/cce-ledger.json`
- Lente externa: Context7 `/websites/rs_tantivy_tantivy` — `QueryParser::set_field_boost`
