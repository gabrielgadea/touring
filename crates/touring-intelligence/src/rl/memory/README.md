# `rl/memory` — subsistema de memória do Touring

Memória durável, recuperável e aprendida do agente: armazenamento em tiers,
recall federado híbrido (SQL FTS5 + ANN + TF-IDF, fusão RRF, value-rank
Memento), palace paths, e — desde 2026-08-12 — a **hashtag library**: cada
memória carrega facetas `#facet:value` consultáveis.

## Description

| Módulo | Papel |
|---|---|
| `rlm` | Store SQLite canônico (`memory_entries`), tiers, schema + migrações idempotentes |
| `tags` | **Hashtag library**: 7 facetas (kind/purpose/lang/domain/process/artifact/status), parser+linter, DDL single-source (`TAG_TABLES_DDL`), helpers SQL compartilhados, link graph tipado (REGRA #17) |
| `codetag` | Scanner de `#tags:` em código → snippet memories (value = o bloco ancorado); tombstones quando a âncora some |
| `moc` | Comunidades emergentes (label propagation determinístico) + Maps of Content em OKF markdown |
| `recall` | SemanticRecall + fusão RRF |
| `palace` | Memory palace paths (wing/room/closet/drawer) |
| `tier_manager` | Política de tiers e manutenção |
| `working` | Working memory LRU |
| `pattern_cluster` | HNSW lazy clustering de padrões |
| `crdt_graph` | Grafo semântico CRDT (delta sync, rkyv) |

## Install

Parte do crate `touring-intelligence` (workspace `~/projects/touring`) —
sem instalação separada: `cargo build -p touring-intelligence`. O CLI é o
binário `touring` (crate `touring-server`); deploy via `update-touring`.

## Usage

```bash
touring memory store "<key>" "<valor>" --tag "#kind:lesson" --tag "#domain:wiring"
touring memory query "#kind:snippet #lang:python"
touring memory recall "diorama #artifact:map"
touring memory sync-tags --file <arquivo>   # codetags → snippet memories
touring memory link <src> <dst> --rel extends && touring memory links <key>
touring memory moc <tópico> [--out path] && touring memory communities
touring memory backfill-tags [--dry-run]
```

Guia completo (facetas, convenção codetag, MOCs):
`docs/memory-hashtag-library.md` na raiz do workspace.

## Contributing

Contratos que toda contribuição deve preservar:

- **Dois write paths, um schema**: `RlmMemory` (in-process) e os handlers RPC
  (`cli_memory_*` em `touring-hook-runtime`) compartilham as helpers de
  `tags.rs` — um segundo DDL hand-written é o anti-padrão "cinco sítios".
- **Trust ordering de tags**: `explicit > code_sync > auto > backfill`.
- **Código é fonte da verdade para snippets**: remover o codetag remove a
  memória (tombstone via `sync_file`).
- **Determinismo** (REGRA #17): ids de link `{src}|{rel}|{dst}`, keys de
  snippet `snippet:{relpath}#L{line}`, label propagation em chaves ordenadas.

## Tests

```bash
cargo test -p touring-intelligence --lib rl::memory   # 172 testes
cargo test -p touring-intelligence --doc              # doctests
```

E2E live (daemon): `docs/plans/2026-08-11-memory-hashtag-library/validate_f{1,2,3}_e2e.sh`.

## License

Mesma licença do workspace Touring (raiz do repositório).
