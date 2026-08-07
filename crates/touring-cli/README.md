# touring-cli — CLI Daemon-Side Query Handlers

> Handlers daemon-side que respondem a cada subcomando `touring …`. Crate de
> biblioteca (`touring_cli`) do workspace [Touring](https://github.com/gabrielgadea/touring);
> não produz binário próprio — é consumido através da fachada
> `touring-hooks → touring-dispatch → touring-cli`.

The **cli layer**, carved from `touring-dispatch` on 2026-06-10
(Wave C2, PoNR #4 — `~/.claude/plans/daemon-lib-rearch/data/wave_runtime_cli_manifest.md`).
This carve closed the elite goal: **no crate >15% of the workspace**
(dispatch 108.6k → 66.2k = 13.9%).

## Overview

Owns every daemon-side cli handler the dispatch table calls:

| Group | Modules |
|---|---|
| Handler tree | `cli/` (55 files — kpi, evolution, repo_score, repo_health, polyglot, scout, memory, decompose, mpatch, acp, saga, execute, viz, health, …) |
| Handler hubs | `cli_handlers{,_decompose,_entity,_index,_file_knowledge,_wiring_repair,_semantics,_session,_mcp,_mutation_test}` (the `#[path = "cli/handlers/*.rs"]` decls) |
| Suggestion engine | `cli_suggester` (PreToolUse classifier — best Touring command per (tool, input)) |
| E2E | `cli_e2e` (comprehensive code analysis handler) |
| Workflow Intelligence | `workflow/` (CEG Pln2 P8 stage/antipattern/advise — moved with its single consumer, cli_suggester) |

## Layering

```
touring-dispatch (hooks/ · lifecycle/ · hook_registry · daemon)
   ↓ depends on + re-exports at historical paths (288 downward call sites)
touring-cli (this crate)
   ↓ depends on
touring-hook-runtime (HookRuntime substrate)
   ↓
touring-hooks-core (knowledge, tantivy, engines) → leaves
```

Cross-crate consumers (touring-server's 22 `touring_hooks::cli_handlers_*`
imports) reach this crate through the double façade
touring-hooks → touring-dispatch → touring-cli, unchanged byte-for-byte.

## Wave C2 inversions (pre-carve, all ship-green)

- `prompt_enhance.rs` (2.3k) + `protocol/` (ACP shim) moved **down to touring-hook-runtime**
- `emit_b302_if_low_confidence_expansion` moved **down to touring-hooks-core::health_delta**
- 30 `maybe_*_hint_on_task_create` matchers + dispatcher → **NEW touring-hooks-core::generator_hints**
- `workflow/` (P8) moved **into this crate** with its single consumer

## Features

`tantivy-fts` (→core+runtime) · `acp-protocol` (→runtime) · `templates`
(→touring-orchestration) · `mpatch-fuzzy` (→core) · `ann-blast` (pure gate) —
all forwarded by touring-dispatch.

## Install

Crate interno do workspace: não é publicado no crates.io e não se instala
isoladamente. Consuma-o por path, como fazem os demais membros:

```toml
[dependencies]
touring-cli = { path = "../touring-cli" }
```

Requisitos herdados do workspace: **edition 2024**, **MSRV 1.95** (fixado em
`rust-toolchain.toml`; o gate de MSRV no CI falha abaixo disso).

Para obter o binário `touring` — que é quem exercita estes handlers — construa
o workspace a partir da raiz:

```bash
cargo build --release -p touring-server     # produz target/release/touring
```

Em uma máquina já provisionada, o pipeline canônico (build → install → restart
do daemon → verify) é `update-touring`; nunca reinicie o daemon com `pkill`
(REGRA #19 — use `touring daemon-ctl restart`).

## Usage

Este crate não tem API pública própria de aplicação: cada handler é despachado
pela tabela do `touring-dispatch` em resposta a um subcomando. O caminho de uso
real é a CLI:

```bash
touring index find <symbol>          # → cli/handlers/index.rs
touring memory recall "<topic>"      # → cli/memory.rs
touring decompose ready <task_id>    # → cli/decompose.rs
touring kpi -j                       # → cli/kpi.rs
touring e2e -j                       # → cli_e2e.rs
```

Como biblioteca, os handlers são invocados com um `HookRuntime` e um payload
JSON, devolvendo a resposta serializada:

```rust
use touring_cli::cli::memory;

let out: String = memory::cli_memory_list(&mut rt, &serde_json::json!({
    "limit": 50,          // default 20
    "sort": "access_count", // ou "last_accessed_at" / "key"
}));
```

## Tests

```bash
cargo test -p touring-cli --features "tantivy-fts,templates,mpatch-fuzzy,ann-blast,acp-protocol"
cargo clippy -p touring-cli --all-targets -- -D warnings
```

O conjunto de features acima é o que o CI exercita; rodar sem elas deixa os
módulos gated fora da compilação e, portanto, fora da cobertura.

## Contributing

O workspace segue gates de qualidade obrigatórios — nenhuma mudança entra com
falha conhecida em aberto:

1. `cargo check --workspace --all-targets` (só `--all-targets` compila os alvos de teste)
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test -p touring-cli` com as features acima
4. `touring-quality score crates/touring-cli --fail-below 0.80` (piso Gold)

Antes de editar um arquivo deste crate, consulte o raio de impacto —
`touring ast meta <file> --depth summary -j` e `touring ast blast <file>` — e
evite deixar símbolos `pub` sem consumidor: o gate de wiring
(`touring wiring orphans -j`) trata órfãos como débito, não como neutro.

Convenções e o protocolo completo estão em `CLAUDE.md` na raiz do workspace.

## License

Licenciado sob **MIT OR Apache-2.0**, à escolha de quem usa — veja
[`LICENSE-MIT`](../../LICENSE-MIT) e [`LICENSE-APACHE`](../../LICENSE-APACHE)
na raiz do repositório.
