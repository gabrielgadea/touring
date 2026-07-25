# Touring CLI — Overview & Architecture

> **Module**: 1/7 | **Version**: v4.9 | **Touring**: v30.3.0 | **SCHEMA_VERSION**: 8
> **Series**: Touring CLI Reference (consulta sob demanda) — `~/.claude/skills/Touring/references/touring-cli-*.md`
> **Index** (auto-load): `~/.claude/rules/touring-cli-index.md` (CLI RANKS v5.0)
> **Skill master**: `~/.claude/skills/Touring/SKILL.md`

Arquitetura geral, padrão actor do daemon, tabela de dispatch, flags globais e formato de wire. Leia este módulo primeiro para entender como tudo se conecta.

---

## Overview

O Touring CLI é um cliente Unix socket que se comunica com o `touring-daemon` via `touring-server`. Cada comando CLI invoca um **hook handler** no daemon que consulta o `HookRuntime`.

### Arquitetura de 3 camadas

| Camada | Local | Responsabilidade |
|--------|-------|-----------------|
| **CLI Client** | `touring-server/src/cli/` | Parsing de args + `daemon_query()` |
| **Daemon Handler** | `touring-hooks/src/cli_handlers.rs` | Lógica via `HookRuntime` |
| **Dispatch Table** | `touring-hooks/src/hook_registry.rs` | Mapeia hook name → handler |

### Daemon Internals (Actor Pattern — 2026-04-12)

O daemon (`touring-hooks/src/daemon.rs`) usa **um actor por projeto** ao invés de `Arc<Mutex<HookRuntime>>` compartilhado:

- **`ProjectCommand` enum**: `RunHook { hook_name, payload, response: oneshot::Sender<String> }` + `Shutdown { done: oneshot::Sender<()> }`.
- **Thread OS dedicada** (`touring-project-actor`) executa `run_project_actor(runtime, cmd_rx)` — possui o `HookRuntime` e processa commands serialmente. Preserva `!Sync` do rusqlite **sem contenção de kernel Mutex**.
- **Bounded mpsc** de profundidade 128 por projeto. Producers bloqueiam em `send().await` sob backpressure ao invés de vazar FDs.
- **Panic-safe**: cada handler + E2E scan envolvidos em `std::panic::catch_unwind(AssertUnwindSafe(...))`. Panic não mata o actor — loga via `tracing::error!` e continua.
- **Accept loop backoff**: `100ms × 2^streak` cap 2s em erros transitórios (EMFILE/ENOBUFS).
- **Handler budgets**: 15s light / **300s heavy**. Heavy hooks: `cli-index-rebuild`, `cli-ast-blast`, `cli-ast-blast-cross-feature`, `cli-mcts-search`, `cli-session-start`, `cli-session-assess`, `cli-tantivy-reindex`, `cli-wiring-chains`, `cli-wiring-audit`, `cli-e2e`.
- **Semaphores timed-out**: global (64) + per-project (**56**, raised from 16) ambos com `timeout(REQUEST_TIMEOUT, acquire_owned())`. Backpressure fail-fast previne acúmulo de FDs sob hook storm.
- **Graceful shutdown**: `ProjectCommand::Shutdown { done }` roda WAL checkpoint + LinUCB + CRDT save dentro do actor (panic-guarded por step), acks via oneshot. `graceful_shutdown()` aguarda cada actor com timeout de 5s.

### Dispatch Architecture (v3.0)

O dispatch usa uma **command table** estática (`cli/common.rs::command_table()`) — single source of truth.
Cada comando é um `CommandDescriptor { name, description, error_policy, handler }`.
O `main.rs` itera pela tabela com `table.iter().find(|c| c.name == subcommand)` (CC=14, was 54).

### Global Flags

Todos os comandos suportam flags globais extraídas antes do dispatch:

| Flag | Efeito |
|------|--------|
| `-j`, `--json` | Output JSON puro (machine-readable, para pipelines com `jq`) |
| `-v`, `--verbose` | Verbose tracing para stderr |
| `--timeout <N>` | Timeout do socket daemon em segundos (default: 10) |

### Formato de comunicação

```rust
// CLI client envia:
{"hook": "cli-ast-find", "payload": {...}, "project_root": "/path/to/project"}

// Daemon responde:
{"success": true, "output": "{...json result...}"}
```

---

**Outros módulos**: [hooks](touring-cli-hooks.md) | [intelligence](touring-cli-intelligence.md) | [tasks](touring-cli-tasks.md) | [rl-quality](touring-cli-rl-quality.md) | [generate](touring-cli-generate.md) | [meta](touring-cli-meta.md)
