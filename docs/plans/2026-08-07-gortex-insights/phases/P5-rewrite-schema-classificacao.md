---
type: PhaseReport
title: "P5 — A4 hook rewrite, C1 schema slimming, e a classificação que salvou o S1 de si mesmo"
description: "Três entregas mais duas correções de causa-raiz descobertas ao provar o S1 em produção: a migração que nunca rodava e a migração que não era idempotente. Inclui o primeiro retrato honesto da proveniência do wiring."
plan_id: 2026-08-07-gortex-insights
tags: [implementacao, hook-rewrite, mcp-schema, proveniencia, migracao, regra-21]
timestamp: 2026-08-08T00:05:00-03:00
okf_version: "0.1"
---

# P5 — Implementação

Decisões do Gabriel nesta fase: **A4 = `rewrite`**, **C1 = aplicar nas tools caras**,
**fantasmas = remover**. Todas executadas.

## O que provar o S1 em produção revelou

O P2 entregou o S1 com 11 testes verdes. Rodar de verdade encontrou **dois defeitos que
nenhum teste unitário pegaria** — e um deles teria feito eu reportar um sucesso falso.

### Defeito 1 — a migração que nunca rodava

Primeiro rebuild após o S1: `name_only_candidates: 0`. Número limpo, resultado plausível.
`sqlite3` respondeu: **`no such table: wiring_unresolved`**.

`FileKnowledgeDB::new` só chama `ensure_schema()` quando `user_version < SCHEMA_VERSION`.
Um `CREATE TABLE IF NOT EXISTS` adicionado ali **sem bump de versão é no-op em todo DB já
migrado**. A tabela nunca nasceu, cada escrita morreu dentro de `let _ =`, e o contador
respondeu um zero perfeitamente inocente.

É a tese central do dossiê acontecendo dentro da correção que existe para eliminá-la — e a
terceira instância do padrão nesta sessão. Duas correções:

1. **Bump v8 → v9** (e a migração repetida em `migrate_schema`, não só em `ensure_schema` —
   mesma defesa que o FIX-5 de 13/04/2026 aplicou a `wiring_suggestions`).
2. **`name_only_candidates` virou `Option<i64>`**: `None` = não medido, `Some(0)` = medido e
   vazio. Um zero produzido por tabela ausente é indistinguível de um zero produzido por
   código limpo — exatamente a classe de mentira que este trabalho remove. Teste:
   `a_missing_table_reports_not_measured_never_zero`.

### Defeito 2 — a migração que não era idempotente

Com o bump, o daemon **recusou todo acesso**: `UNIQUE constraint failed: idx_wiring_unique`.
O bloco que marca símbolos de touring-hooks como `daemon_hook` transforma linhas produtoras
em linhas consumidoras que compartilham o literal `touring-daemon://dispatch`. Rodado duas
vezes sobre os mesmos dados, recria uma tupla que já existe.

Latente desde que foi escrito — invisível porque `SCHEMA_VERSION` nunca subia. **Uma
migração que roda exatamente uma vez é indistinguível de uma correta até o próximo bump.**
Corrigido com `UPDATE OR IGNORE` + o teste
`test_migrate_schema_survives_a_second_pass_over_populated_wiring`, que força a re-execução
sobre dados que colidem.

### Defeito 3 — o número novo repetindo o pecado velho

Com tudo funcionando, a primeira medida real: **7.197** call sites não resolvidos. Publicar
isso como "dívida do resolvedor" seria o mesmo colapso numa roupa nova — o topo da lista era
`super` (1.298), `serde` (531), `std::path` (403): um scope keyword e dois crates externos,
nenhum deles dívida de ninguém.

`UnresolvedClass` (bump v10) separa os três fatos:

| Classe | Significado | É dívida? |
|---|---|---|
| `scope_keyword` | `super::`, `self::`, `crate` — o resolvedor declina por desenho | não |
| `external` | crate de terceiro ou `std` — não há produtor a encontrar | não |
| `workspace_unresolved` | caminho para um crate DO workspace que não resolveu | **sim** |

A classificação roda no **call site**, com o mesmo `TOURING_CRATE_MAP` que a tentativa de
resolução usou — derivá-la no storage deixaria o veredito divergir da tentativa. E o ranking
`top_unresolved_modules_debt_only` filtra por dívida: uma lista de "conserte isto" encabeçada
por `serde` manda todo leitor caçar não-problemas.

## O primeiro retrato honesto do wiring

Medido após o rebuild completo (3.128 arquivos, 138 s):

| origem | linhas | leitura |
|---|---:|---|
| `ast_inferred` | **49.523** | casamento de **nome puro**, capado em 4 produtores |
| `ast_read` | 13.051 | legado, ainda não reescrito |
| `ast_declared` | 11.856 | produtores lidos do AST |
| `ast_resolved` | **2.981** | `use` efetivamente resolvido a um arquivo |

**64% das arestas do grafo são heurística.** Apenas ~4% são imports resolvidos de verdade.
Isso era literalmente invisível: as 77.679 linhas diziam `ast_read`, indistintamente. É o
achado de maior consequência da sessão — muda como qualquer número derivado do wiring
(órfãos, blast radius, integração) deve ser lido.

## A4 — hook mode `rewrite` ✅

`crates/touring-cli/src/hook_rewrite.rs`. Quando um comando tem **espelho exato** na
superfície touring, o hook troca via `hookSpecificOutput.updatedInput` — nada é persuadido,
nada é bloqueado, a chamada melhor simplesmente acontece.

**A barra de equivalência é a regra**: um rewrite só dispara se a saída for **byte-idêntica**.
Menos que isso é pior que persuasão — devolver silenciosamente algo *diferente* do pedido
corrompe as premissas do agente, e invisivelmente.

Verificado antes de entrar:
`cmp <(NO_COLOR=1 touring ast highlight F) <(cat F)` → **idêntico**.

O ganho não é o texto, é o caminho: a leitura passa a atravessar o índice em vez de passar
ao largo dele — o mesmo objetivo que o Gortex descreve como *"observe chamadas como
`graph_stats`, não leituras de arquivo"*.

**O candidato óbvio ficou de fora, de propósito**: `cat <file>` → `touring read <file>` é a
conversão de maior valor em tokens **e** uma violação de equivalência (`touring read` devolve
um relatório, não o arquivo). Permanece advisory até que um humano decida essa troca — mudar
*o que o chamador recebe* é decisão de política, não de hook. Teste:
`metadata_first_conversions_are_deliberately_absent`.

Modos: `TOURING_HOOK_MODE` ∈ {`rewrite` (default), `enrich`}; valor desconhecido degrada
para `enrich` — fail-safe, não fail-open. Contador `hook_rewrite_applied_count`: mede
chamadas de fato melhoradas, não nudges emitidos (um rewrite não depende de o modelo
obedecer, então emitido == seguido por construção).

**Prova**: 8 testes, incl. 9 formas compostas que **não** podem disparar (`|`, `>`, `&&`,
`$VAR`, glob, múltiplos arquivos, flags, `~`, aspas).

## C1 — schema do `tools/list` ✅

11 campos alias (`path`, `source`, `title`, `memoryType`, `content`, `topK`, `operation`,
`action`…) ganharam `#[schemars(skip)]`: somem do schema, **continuam aceitos** pelo serde.
Zero quebra de contrato — chamadas antigas seguem funcionando.

O risco de esconder é o que o próprio dossiê critica em `apply_curation` (*"esconder destrói
a descoberta"*). Contido por `HIDDEN_ALIAS_FIELDS` + `hidden_alias_capabilities()`, com um
teste anti-drift que conta os `#[schemars(skip)]` **no próprio fonte** e exige que cada um
esteja registrado — um campo escondido fora do registro seria invisível duas vezes.

Dois falsos positivos nos meus próprios testes, achados e corrigidos: `title` é *keyword* do
JSON Schema (a busca por substring acusava presença — homonímia, cadeia 4 do VP-Scout), e a
contagem de `#[schemars(skip)]` pegava as menções na própria prosa.

**Medido no handshake MCP real** (mesma metodologia que produziu o baseline 33.161 B):

| | tools | payload | delta |
|---|---:|---:|---:|
| baseline (rodada 2 do dossiê) | 23 | 33.161 B | — |
| após esconder 11 aliases | 23 | 32.556 B | −605 B (−1,8%) |
| após esconder 8 campos raros de `touring_decompose` | 23 | **31.706 B** | **−1.455 B (−4,4%)** |

`touring_decompose` sozinha: 2.947 → 2.030 B (**−31%**).

**A conclusão honesta**: o alvo do facade do Gortex é ≤15.000 B, e −4,4% não chega lá. Os
aliases não eram o peso — o custo está **distribuído** por 23 tools × ~15 campos. O
mecanismo está provado e é seguro (esconder ≠ remover); estendê-lo às 23 tools é
mecânico e daria a ordem de grandeza que falta. Reportar os −4,4% como se fossem a solução
seria o mesmo tipo de número conveniente que este trabalho existe para remover.

## Fantasmas removidos ✅

1.225 linhas que o compilador nunca via, confirmadas pelo próprio rebuild
(`stale_paths_sample` listou os dois como purgados):

- `crates/touring-hooks-core/src/knowledge_wiring.rs` (879 L)
- `crates/touring-analysis/src/e2e/schema_guard.rs` (346 L)

Backups em `scratchpad/ghost-*.bak`.

## Gates

| Gate | Resultado |
|---|---|
| `cargo check --workspace --all-targets` | 0 erros |
| `cargo clippy --workspace --all-targets -D warnings` | **exit 0** |
| testes novos desta fase | **17** (8 rewrite + 4 C1 + 5 classificação) |
| testes de regressão de migração | 3 (v8→v9, idempotência, segunda passada sobre dados) |
