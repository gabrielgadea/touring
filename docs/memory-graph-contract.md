---
type: Contract
title: Contrato do Grafo de Memória — padrão mínimo (7 cláusulas)
description: O padrão de Gabriel (30/08/2026) como contrato executável — o que a máquina já aplica, o que passa a aplicar, e a escada de enforcement. Derivável por outros projetos (analise deriva daqui).
plan_id: 2026-08-30-grafo-memoria-padrao-minimo
tags: [contract, memory, graph, learning]
timestamp: 2026-08-30T01:10:00-03:00
okf_version: "0.1"
---

# Contrato do Grafo de Memória (padrão mínimo, 30/08/2026)

> Diretiva de Gabriel via sessão `analise-e0`: o grafo de 7 cláusulas construído
> no projeto `analise` (10 nós · 35 arestas · 98 facetas · comunidade 4→13) é o
> **padrão básico mínimo** de memória em qualquer lugar. Este documento é o
> contrato canônico no Touring; consumidores externos derivam daqui.

## As 7 cláusulas → predicados (estado medido em 30/08)

| # | Cláusula | Predicado executável | Executor HOJE | Furo confirmado |
|---|---|---|---|---|
| 1 | Chave determinística `<tipo>:<slug>:<AAAA-MM-DD>` | regex de forma; id derivável (REGRA #17) | convenção apenas | nenhum gate valida a forma |
| 2 | Corpo denso (número + método + o refutado) | — | humano | deliberadamente FORA do gate v1: densidade não se mede barato sem virar teatro de len() |
| 3 | Facetas do vocabulário canônico, provadas por leitura | `Facet::from_str_ci` (enum fechado de 7, aliases governados — `tags.rs`) | parser existe | **faceta desconhecida morre em silêncio**: `store`→`stored`, índice descarta, `query`→`total=0` (gotcha medido no analise; o WARN prometido no doc cobre VALORES, não facetas) |
| 4 | Arestas tipadas, id determinístico `{src}\|{rel}\|{dst}` | `cli_memory_link` + `LinkRel` (`extends\|relates-to\|exemplifies\|supersedes\|generated-by`) | superfície completa | nada as exige; **o grau não vale nada no recall** (nó com 17 arestas = nó órfão) |
| 5 | Procedência `generated-by` → fase/DAG | `LinkRel::GeneratedBy` | vocabulário existe | **nenhum produtor estrutural**: `loop_phase_close` grava a memória da fase e NÃO cria a aresta |
| 6 | Nó-ponte para o sem-faceta (sessão/run_id/bundle) | recall textual | ✓ funciona | convenção — ok assim |
| 7 | Leitura de volta é parte do gravar | `memory query` lendo `total` (nunca `count`=shown) | superfícies existem | sem aviso quando a faceta consultada não existe (mesmo furo da cláusula 3) |

## O que a máquina JÁ sabe fazer e o padrão ainda não usa

1. **`supersedes` é executável, não anotação**: `rlm.rs::store_rich` com
   `supersedes` RETIRA a entrada antiga (`superseded_by`, nunca delete). A
   aresta certa tem efeito de estado — usar ao corrigir um nó, não regravar.
2. **As arestas já viajam no recall**: `attach_one_hop_links` anexa o 1-hop de
   cada entrada servida — o payload do recall já carrega o grafo; só o re-rank
   o ignora.
3. **Utility por nó existe**: o credit loop (`memory credit`) grava
   `outcome_reward` na entrada servida — feromônio de CONTEÚDO. O de ESTRUTURA
   (grau/procedência) é o que falta (P2 abaixo).
4. **MOC/communities derivam do grafo**: `moc.rs` (STRONG_FACETS =
   domain/purpose/artifact/process/lang) já detecta comunidades emergentes —
   consomem facetas e arestas; melhoram sozinhos quando o contrato sobe.
5. **Código gera memória**: `sync-tags`/codetags (post_write/edit). O mesmo
   harvest pode gerar ARESTAS (import → `relates-to`) — P5, medido antes.
6. **Hyper-Extract tem ids determinísticos** (`entity_id`,
   `relation_id = {src}|{type}|{dst}` — alinhado à cláusula 4)… e está
   **degenerado na prática**: o `knowledge/P1.json` da wave de ONTEM tem
   1 entidade e 0 relações, porque ninguém passa `--abstract`. O "mesmo grafo
   escrito duas vezes" da diretiva é pior: uma cópia está oca (P4 reconcilia).

## A proposta executável (fases; enforcement no executor, escada medida)

Princípio D8: o contrato vale o que o executor aplica. Princípio das waves de
29/08: **instrumentar antes de bloquear** — advisory com KPI primeiro, block
depois de dias de série (a mesma escada do code-mode arm e do
policy_discrimination).

| Fase | Entrega | Executor | Tamanho |
|---|---|---|---|
| **P1 fail-loud** | `memory store` responde `ignored_facets:[…]` + warning quando `#token:` não é faceta canônica; `memory query` com faceta desconhecida → erro que ENSINA as 7 (nunca `total=0` mudo) | handler do store (espelho `memory_tags`) + handler do query | S |
| **P2 feromônio estrutural** | re-rank do recall ganha o eixo de grau DENTRO da classe: `(classe_valor, é_traço, bucket_estrutura)` onde bucket 0 = tem `generated-by` E ≥2 arestas, 1 = alguma aresta, 2 = órfão — usando o 1-hop que `attach_one_hop_links` JÁ carrega | `rerank_by_case_value` (cli/memory.rs) | S |
| **P3 contrato visível** | resposta do `store` (tier semantic, kinds lesson/decision/diagnostico) ganha `contract:{key_shape,faceted,linked,provenance}` advisory + KPI `touring.memory.graph_contract_share` | handler do store + braço kpi + commitment | S |
| **P4 uma fonte, duas projeções** | `loop_phase_close.py` cria as arestas `generated-by` (memória-da-fase → `loop:<task>:P<n>`, → `decomp:<task>`) via `memory link`, e `knowledge/P<n>.json` passa a ser PROJEÇÃO do mesmo material (nós+arestas reais, nunca mais 0 relações) | loop_phase_close.py (skill loop-engineering) | M |
| **P5 arestas derivadas** | `touring memory suggest-links`: pares co-servidos no recall ≥N vezes → sugestão `relates-to` marcada `derived` (nunca automática; primeiro a régua `edge_density`) | comando novo + case_ledger como fonte | M |

KPIs da família: `graph_contract_share` (P3) · `edge_density` (arestas/nó novo,
janela) · `provenance_share` (nós com `generated-by` / nós novos). Bloqueio só
depois de série durável — o gate que nasce bloqueando vira rota contornada.

## Adenda medida (30/08, troca com `analise-e0`)

- **`--supersedes` NÃO reaponta arestas** (provado por código rlm.rs:586-593 —
  só `memory_entries.superseded_by` é tocada — e por sonda viva:
  `links(nó-novo)=0`, `links(nó-antigo)=1`). Corrigir nó com arestas =
  supersede + reapontar as arestas NA MESMA LEVA (id determinístico torna
  idempotente). Candidato **P1.5**: o handler do store reaponta
  `memory_links` quando `supersedes` está presente.
- **Dois pontos de morte muda para tags**: `parse_tag` Err → `tracing::warn`
  no LOG do daemon (rlm.rs:604), nunca na resposta; faceta não-canônica que
  parseia Ok morre depois, no filtro por `Facet` da consulta. O P1 eleva
  ambos à resposta — com a emenda da peer: `ignored_facets` nomeia a faceta
  canônica mais próxima (`#classe`→`#domain|#process`;
  `#adw`→`#process:adw-<nome>`).
- **Método das 3 sondas para provar faceta** (da peer, medido): `total>0`
  sobre valor não-único é ambíguo entre faceta casada e fallback textual
  (`*` NÃO é wildcard — cai para busca de texto). A prova exige valor único +
  controle negativo (faceta absurda → 0) + controle positivo na mesma sonda.

## Regras de uso imediato (valem já, sem esperar as fases)

1. Chave: `<tipo>:<slug>:<AAAA-MM-DD>`; corrigir nó = `--supersedes <key-velha>`
   (retire executável) **+ reapontar as arestas na mesma leva** (ver adenda),
   nunca regravar variante.
2. Toda memória de fase/lição: facetas das 7 + `link --rel generated-by` para
   `loop:<task_id>` ou `decomp:<task_id>` na mesma leva do store.
3. Depois de gravar: `memory query "#faceta:valor"` e exigir o nó em `total`
   (nunca `count`).
4. O que não tem faceta (sessão, run_id, bundle) vive no CORPO de um nó-ponte.

## Procedência deste contrato

Diretiva: mensagem cross-session `analise-e0` (30/08); grafo de origem: bundle
`~/projects/analise/docs/plans/2026-08-29-cinco-decisoes-do-pipeline/`, DAG
`task_1788046247478962143`. Grounding por execução nesta sessão: `tags.rs`
(enum fechado + from_str_ci), `rlm.rs:535` (supersedes retire),
`cli/memory.rs:189-213` (re-rank sem grau) e `:653` (1-hop no payload),
`knowledge/P1.json` degenerado (1 nó, 0 arestas). Bundle:
`docs/plans/2026-08-30-grafo-memoria-padrao-minimo/`.
