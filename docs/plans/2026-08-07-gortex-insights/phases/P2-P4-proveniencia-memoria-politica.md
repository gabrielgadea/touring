---
type: PhaseReport
title: "P2–P4 — S1 proveniência de aresta, S4 memória ACO, B7 delta negativo, A7b compactação"
description: "Quatro insights implementados com prova executada: proveniência por tier + tabela de não-resolvidos, evaporação do feromônio, cláusula de viés metodológico e compactação condicional. Inclui um flaky pré-existente corrigido e dois arquivos-fantasma provados por execução."
plan_id: 2026-08-07-gortex-insights
tags: [implementacao, wiring-proveniencia, memoria-aco, viés-metodologico, compactacao, regra-21]
timestamp: 2026-08-07T23:10:00-03:00
okf_version: "0.1"
---

# P2–P4 — Implementação

Continuação de [P1](P1-implementacao.md) (V4, S5, S3, S2). Estratégia:
[strategy-2026-08-07-implementacao.md](../strategy-2026-08-07-implementacao.md).

## Achado do OUTER — dois arquivos-fantasma, provados por execução

| Arquivo | Linhas | Prova |
|---|---|---|
| `crates/touring-hooks-core/src/knowledge_wiring.rs` | 879 | erro de sintaxe injetado → `cargo check -p touring-hooks-core` **verde** |
| `crates/touring-analysis/src/e2e/schema_guard.rs` | 346 | `e2e/mod.rs` faz `pub use touring_foundation::schema_guard` e o comentário diz *"orphaned on disk"* |

Consequência concreta: o S1 parecia ter **dois** sítios simétricos de `INSERT` com
`'ast_read'` literal (C08 clássico). Um deles é um cadáver — o compilador nunca o vê.
Espelhar a correção nele teria consumido tempo e produzido a ilusão de simetria.
Deleção fica para o Gabriel (REGRA #11 — os arquivos estão versionados).

## S1 — Proveniência de aresta ✅

**O defeito**: `wiring_map.contract_source` = `'ast_read'` em **77.679 de 77.679** linhas.
Uma coluna de proveniência com um único valor é uma coluna que não existe — e a
consequência era diagnóstica: uma aresta descoberta por **casamento de nome puro** (F9,
capado em 4 produtores, deliberadamente lossy) era indistinguível de uma cujo `use` o
resolvedor mapeou a um arquivo real. Um palpite silenciava um símbolo possivelmente morto.

**Tiers, mapeados aos call sites reais** (`WiringOrigin`, ordenado por força de evidência):

| Tier | Nasce em | Evidência |
|---|---|---|
| `ast_declared` | `register_pub_symbol` | forte |
| `ast_resolved` | `index.rs:664` — `use` resolvido a um arquivo | forte |
| `ast_inferred` | `index.rs:690` — F9, casa **só por nome**, cap 4 | **heurística** |
| `text_matched` | extratores regex (linguagens sem AST) | fraca |
| `unresolved` | `index.rs:664` ramo `else` — antes **vazio** | ausência |

**A metade que não existia**: o `else` do resolvedor. Quando o caminho do módulo não
mapeava, o call site simplesmente **desaparecia**. Agora vai para `wiring_unresolved`, e
`name_only_candidates` reporta a contagem separada — o número que o Gortex chama de
*"honest handling"*.

**Contenção — o risco que inverteria o objetivo**: se a linha unresolved usasse o
`module_file` real, contaria como **consumidor** e apagaria órfãos verdadeiros. Tabela
separada torna isso estruturalmente impossível, não apenas improvável. O teste
`unresolved_imports_never_change_the_orphan_set` grava linhas unresolved **nomeando os
mesmos símbolos** dos produtores e exige igualdade de **conjunto** antes/depois.

Compatibilidade: `record_consumer` delega com `AstResolved` (77.558 de 77.679 são
`rust_import`); só o call site F9 passa `AstInferred`. Linhas históricas `'ast_read'` leem
de volta como `AstResolved` — inventar fraqueza para um valor conhecido seria o mesmo
pecado ao contrário.

**Exposto em**: `touring wiring orphans -j` ganha `resolution` com
`name_only_candidates`, `heuristic_edges`, `top_unresolved_modules` e `origin_breakdown`.

**Prova**: 10 testes (`provenance_tests`), incluindo contenção, idempotência de rebuild,
round-trip lossless do enum e ordenação por evidência.

## S4 — O feromônio passa a evaporar ✅

**O defeito medido**: `outcome_reward` existe, é lido por `case_value`, e **11 de 7360**
entradas o têm (0,15%). Pior: não era opção de ordenação, então a única coluna que diz
quais lições funcionaram não conseguia ordenar a listagem que as mostra. Sem `importance`
não havia como despriorizar lixo de teste (`purpose-test-key-zx9` apareceu entre os 15
primeiros de um recall real); sem supersessão, uma lição corrigida seguia guiando com o
mesmo peso da correção.

**Implementado**: colunas `importance` (1–5), `pinned`, `superseded_by` (ALTER idempotente,
mesmo padrão de `outcome_reward`); `touring memory store --importance --pinned
--supersedes`; recall filtra superseded e ordena `pinned > importance > relevância`; `list`
aceita `--sort reward|importance` e passa a reportar `corpus.{total,with_reward}`.

**Disciplina do NULL** (a mesma que o `reward` já tinha): ausente ≠ zero. Uma entrada não
pontuada não foi julgada — `NULLS LAST` a coloca abaixo das pontuadas **sem** ordená-la
como se tivesse falhado.

**Supersessão é aposentadoria, não deleção**: a entrada antiga fica na tabela apontando
para a nova, e some do surfacing.

**Federação**: cada coluna é resolvida por PRAGMA por conexão. Um `memory.db` de outro
projeto sem as colunas continua recallável — o teste
`a_legacy_db_without_the_columns_still_recalls` existe porque o modo de falha aqui é o
pior possível: resultado **vazio em silêncio**.

**Prova**: 6 testes (`pheromone_decay_tests`).

## B7 — Zero resultado negativo é sinal de viés ✅

A cláusula mais afiada do protocolo de eval do Gortex, adotada no harness 50-dim:
`QualityReport.methodology_warnings`. Três sinais independentes:

1. **Nenhum resultado negativo** em ≥ 10 dims medidas — uma varredura ampla que não acha
   nada é mais provavelmente uma medição quebrada que um código perfeito.
2. **Maioria das dims não medida** com composite publicado assim mesmo.
3. **Evidência truncada** alimentando o veredito — a marca do S5, que existia por dimensão
   e não tinha voz no nível do relatório.

**Advisory por construção**: não move `composite`, não cria blocker. Uma suspeita não é uma
medição. Mas aparece no formato compact (`⚑ methodology:`) — um caveat que só chega ao JSON
é um caveat que o leitor do resumo nunca vê, e este existe justamente para aparecer ao lado
de um número bom.

**Prova**: 7 testes, incluindo `the_warning_is_advisory_and_never_moves_the_score`.

## A7b — Compactação condicional ✅

`touring compact [<domain>|all] [--force] [--dry-run]`. Dois limiares que **ambos** precisam
valer: dead ratio > 25% **e** > 16 MiB recuperáveis (o 1 GiB do Gortex escalado — os DBs
daqui são duas ordens de grandeza menores). Razão alta sobre arquivo minúsculo não compra
nada; bytes absolutos dentro de arquivo saudável são folga normal.

A decisão é **sempre reportada**, inclusive quando é "não" (`below_threshold` com os
números). O contraste é o `wiring_map`, cujos fantasmas se acumularam até serem purgados
à mão.

**Prova**: 4 testes, incl. um que lê os PRAGMAs contra um SQLite real e verifica que
deletar linhas move páginas para a free list.

## REGRA #21 — falha alheia corrigida

`query_cache::tests::cache_metrics_advance_on_hit_and_miss` **falhava sob a execução
paralela** e passava isolado — assinatura de estado global compartilhado: o
`clear_all_drops_everything` apaga o cache do processo inteiro e podia cair entre o `put` e
o `get` do teste de métricas, transformando o hit esperado em miss. Chaves distintas não
bastam quando a operação do outro teste é "apague tudo".

Corrigido com um mutex de teste compartilhado (tolerante a poisoning, para que um pânico
não cascateie). **3 execuções consecutivas, 422/422 cada.**

## Gates executados

| Gate | Resultado |
|---|---|
| `cargo check --workspace --all-targets` | verde |
| `cargo clippy --workspace --all-targets -D warnings` | **exit 0**, 0 erros (restam MSRV-config e future-incompat de `proc-macro-error2`, ambas pré-existentes e de dependência) |
| testes dos 6 crates tocados | **0 falhas** (327 + 422 + 1442 + 386 + 389 + … ) |
| testes novos desta fase | **27** (10 S1 + 6 S4 + 7 B7 + 4 A7b) |

## Aberto

| # | Item | Nota |
|---|---|---|
| A2 | Contagem real de tokens | o conserto é instrumentar bytes reais, **não** trocar o divisor — `ctx_gain` multiplica contadores por constantes inventadas (30_000/20_000), então tokenizar isso daria precisão falsa sobre um número inventado |
| C1 | Emagrecer schema do `tools/list` | 33.161 B para 23 tools |
| A1 | MinHash/LSH Type-2 no F1.3 | substrato existe (`touring-simd/src/similarity/jaccard.rs`) |
| A5 · A6 | Reach precomputado · elisão de corpo | fora deste ciclo (custo alto, sem urgência medida) |
| A4 · A3 · B1 | hook `rewrite` · GCX1 · linter de `~/.claude` | exigem decisão do Gabriel |
