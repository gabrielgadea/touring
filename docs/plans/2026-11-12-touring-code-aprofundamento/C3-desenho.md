# C3 — Um caminho de escrita para "arquivo indexado" (nota de desenho)

Arquivos: `crates/touring-code/src/ast/graph/mod.rs`, `crates/touring-code/src/ast/store.rs`;
acompanham `touring-server/src/graph_service.rs` (3 sítios de chave). Termo novo no `GLOSSARY.md`:
**chave de módulo** (*module key*).

## Escritores hoje (enumerados por grep em `crates/`, fora de testes)
- Mapas em memória (`symbols`, `file_to_symbols`, `dependencies`, `reverse_deps`): `index_file` (mod.rs:157),
  `index_files` (:258), `remove_file` (:210), `clear` (:148), `SymbolStore::load_into_index` (store.rs:643).
  Os campos são `pub`: só fixtures de teste escrevem neles fora desses cinco.
- SQLite, `symbols`: `UPSERT_SYMBOL_SQL` em 4 sítios (`upsert_symbol`, `bulk_index`, `replace_file_symbols`,
  `apply_change_set`). `dependencies`: INSERT literal em 2 (`upsert_dependency`, `bulk_index`).
  `dependency_symbols`: 3 jeitos (por aresta, reconstrução total via `json_each`, backfill do `ensure_schema`).
  DELETE do arquivo: 2 cópias (`remove_file`, `replace_file_symbols`).
- Chamadores externos (grep): `index_file` ×4 prod (server) · `remove_file` ×1 prod (graph_service) ·
  `load_into_index` ×9 · `replace_file_symbols` ×10 (hook-runtime ×5, cli ×5) · `persist_to` ×1 ·
  `get_reverse_deps` ×1 (`tools_infra.rs:1510`). `incremental_pipeline.rs` chama só `diff_symbols`/`apply_change_set`: intocado.

## Gramática das chaves (escrita uma vez, em `graph/mod.rs`)
- **chave de arquivo**: o caminho como o chamador indexou. Usada por `symbols[..].file_path`, `file_to_symbols`,
  `dependencies` (chave externa e `DependencyEdge.from`), SQL `symbols.file_path`, `dependencies.from_file`.
- **chave de módulo**: o texto que um import nomeia (`ImportInfo.module_path`: `utils`, `crate::a::b`). Usada por
  `DependencyEdge.to`, chaves de `reverse_deps`, SQL `dependencies.to_file`.
- `module_key(file_path) -> &str` (`pub(crate)`): a chave de módulo sob a qual os importadores de um arquivo são
  arquivados = o stem do arquivo (convenção que já existia em 5 cópias: `remove_file` + 3 sítios de `graph_service` + o
  comentário de `store`). É heurística (Rust `crate::a::b` ≠ stem `b`); fica documentada, não redesenhada.

## Interface (assinaturas reais)
Memória (`impl SymbolIndex`):
- `pub fn index_file(&mut self, file_path: &str, source: &str, lang: Lang) -> AstResult<()>` — assinatura mantida; passa a SUBSTITUIR a contribuição anterior do arquivo (reindexar).
- `pub fn index_files(&mut self, files: &[(&str, &str, Lang)]) -> AstResult<()>` — extração paralela mantida; fusão pelo mesmo escritor.
- `pub fn remove_file(&mut self, file_path: &str)` — assinatura mantida.
- `pub fn importers_of(&self, file_path: &str) -> &[String]` — substitui as 3 cópias do stem em `graph_service.rs`.
- Escritor único, `pub(crate)`: `insert_symbol(&mut self, SymbolLocation)` (`symbols` + `file_to_symbols`) e
  `insert_edge(&mut self, DependencyEdge)` (`dependencies` + `reverse_deps`); privado: `withdraw(&mut self, file_path)`.
  Invariantes: `file_to_symbols[f]` lista os nomes de `symbols[*]` com `file_path == f`; `reverse_deps[m]` lista os
  `from` das arestas com `to == m`; `withdraw` poda chave que ficou vazia; `index → remove` devolve o índice ao estado anterior.
- `load_into_index` (store) chama `insert_symbol`/`insert_edge` na ordem das linhas (ordem de `symbols[nome]` preservada).
SQLite (`impl SymbolStore`, privados): `put_symbol_row(&Connection, &SymbolLocation)`,
`put_edge_row(&Connection, &DependencyEdge)` (aresta + `dependency_symbols` dessa aresta),
`delete_file_rows(&Connection, &str)` (símbolos + `dependency_symbols` + `dependencies`, sem confiar só no CASCADE).
`upsert_symbol`, `upsert_dependency`, `bulk_index`, `replace_file_symbols`, `apply_change_set`, `remove_file` passam por eles;
transações via `rusqlite::Transaction` (rollback no drop). API pública de store inalterada.
`get_reverse_deps(file)` casa `to_file IN (file, module_key(file))`.

## Suspeitas e defeitos (RED→GREEN antes de consertar; resultado final no relatório)
(i) colisão de stem em `remove_file`; (ii) `get_reverse_deps` cego a dado extraído de verdade; (iii) `dependency_symbols`
órfã sem `PRAGMA foreign_keys`; (iv) reindexar em memória acumula duplicata; (v) transação de `bulk_index` /
`replace_file_symbols` vaza aberta em erro; (vi) `remove_file` deixa chaves vazias em `reverse_deps`.

## O que sai
Os 4 literais extras de `UPSERT_SYMBOL_SQL`, o INSERT duplicado de `dependencies`, a reconstrução total de
`dependency_symbols`, o DELETE duplicado, as 3 cópias de merge em memória, as 3 cópias do stem em `graph_service.rs`.

## Fora do escopo (relatado, não tocado)
`SymbolIndex::blast_radius` e os leitores `signals.rs:305`/`pre_read.rs:1409` consultam `reverse_deps` por CAMINHO
de arquivo enquanto o escritor grava por chave de módulo: mudar isso muda o que gates e hooks dizem. Fica para decisão do lead.
