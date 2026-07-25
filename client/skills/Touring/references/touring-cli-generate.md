# Touring CLI — Code Generation & L7-B Features

> **Module**: 6/7 | **Version**: v4.27 | **Touring**: v30.3.0
> **Series**: Touring CLI Reference (consulta sob demanda) — `~/.claude/skills/Touring/references/touring-cli-*.md`
> **Index** (auto-load): `~/.claude/rules/touring-cli-index.md` (CLI RANKS Tier 5)
>
> **Last update**: Wave C (v4.27) added `RenderShape` budget constraint system + touring-assists framework (10 handlers) to code generation pipeline.

touring-generator pipeline (24 subcommands, typestate Draft→Verified→Rendered→Speculated→Committed) + L7-B sandboxing primitives (Inferlets WASM, async Jobs, mpatch fuzzy patch preview, MCP tools).

---

## 17c. L7-B Inferlets WASM (v3.5 — 2026-04-10)

Observability + execução de WASM inferlets sandboxed via `InferletService` + `touring-wasm::WasmCacheManager`.
Requer feature `touring-hooks/inferlets-wasm` habilitada. Build step:
`cargo build --target wasm32-unknown-unknown --release -p inferlets` + copy para
`inferlets/wasm_bytes/libinferlets.wasm`.

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring inferlets list [-j]` | `cli-inferlets-list` | Lista inferlet pools carregados. Retorna `{count, loaded_inferlets: [AlwaysSuccess, Memory, Pattern, Classifier]}` |
| `touring inferlets run <name> [<input>]` | `cli-inferlets-run` | Executa inferlet por nome. Names válidos: `always_success`, `memory`, `pattern`, `classifier`. Retorna `{status, inferlet, result}` com `PluginResult { output, fuel_consumed, success }` |

**Fuel budgets observados**: always_success≈10864, memory≈24777, pattern≈23659, classifier≈30455.

## 17d. L7-B Spawn & Poll Jobs (v3.5 — 2026-04-10)

Async background worker primitives — Claude Code pode spawnar long-running tasks sem bloquear
o tool call, depois polling independente. Implementado via `DashMap<String, JobState>` singleton
com tokio::task::JoinHandle. **execve semantics** (no shell) para segurança contra command injection.

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring jobs spawn <program> [args...]` | `cli-jobs-spawn` | Spawnar worker em background. Retorna `{job_id, status: "spawned", tool_name, program}`. Sem shell — args passados como argv literals. |
| `touring jobs poll <job_id>` | `cli-jobs-poll` | Consultar status. Retorna `{status: running\|completed\|failed\|not_found, result?, error?, started_at_secs, finished_at_secs?, duration_secs?}`. Transição automática Running→terminal quando JoinHandle finaliza. |
| `touring jobs list` | `cli-jobs-list` | Listar todos os jobs do registry. Retorna `{job_count, jobs: [{job_id, status, started_at_secs, is_terminal}]}` |
| `touring jobs drop <job_id>` | `cli-jobs-drop` | Remover job do registry. Se Running, aborta o JoinHandle. Retorna `{dropped: bool, job_id}`. Completa o lifecycle SPAWN→POLL→LIST→DROP. |

**Lifecycle validado E2E**: echo spawn → completed → listed → dropped=true → not_found.

## 17e1. mpatch (L7-B Fuzzy Patch Preview — 2026-04-25)

Feature-gated by `mpatch-fuzzy` in `touring-hooks`. Provides dry-run fuzzy patch
preview for the pre_write hook and plan_commit pipeline. Hook handler:
`cli-mpatch-preview`.

| Comando | Descrição |
|---------|-----------|
| *(hook only)* | `cli_mpatch_preview(rt, payload)` — payload: `{"file": "...", "patch": "...", "dry_run": bool}`. Returns `{matched, method, confidence, preview?, error}` |

**Types** (`crates/touring-hooks/src/shared/mpatch_preview.rs`):

```rust
pub struct PatchPreview {
    pub matched: bool,
    pub method: PatchMethod,  // Exact | Whitespace | Fuzzy
    pub confidence: f32,      // 0.0–1.0
    pub preview: String,      // resulting content after patch
}
```

**Feature gate**: `mpatch-fuzzy = ["dep:mpatch"]` in `touring-hooks/Cargo.toml`.
Stub impl returns `{"matched": false, "error": "mpatch-fuzzy feature is not enabled"}` when feature is off.

**E2E tests** (feature-gated, `mpatch-fuzzy`):
`test_cli_mpatch_preview_exact`, `test_cli_mpatch_preview_fuzzy`,
`test_cli_mpatch_preview_missing_file`, `test_cli_mpatch_preview_feature_off`
in `cli_handlers_e2e.rs`. Integration with plan_commit pipeline is tested via
touring-generator `simd-fuzzy` feature (lines 494–523 in `typestate.rs`).

## 17e. L7-B MCP Tools (v3.5 — 2026-04-10)

4 novos MCP tools expostos via `#[tool]` macro (rmcp) em `touring-server::server::TouringServer`.
Consumíveis por Claude Code via `mcp__touring__*` namespace.

| MCP Tool | Rust Method | Params | Descrição |
|----------|-------------|--------|-----------|
| `touring_spawn_worker` | `TouringServer::spawn_worker` | `JobsSpawnParams { tool_name, program, args }` | Spawn background worker — returns job_id |
| `touring_poll_worker` | `TouringServer::poll_worker` | `JobsPollParams { job_id }` | Poll job status — returns JSON state |
| `touring_list_jobs` | `TouringServer::list_jobs` | `JobsListParams {}` (empty) | List registry entries |
| `touring_drop_job` | `TouringServer::drop_job` | `JobsDropParams { job_id }` | Drop + abort handle |

## 18. Generate (touring-generator pipeline — 24 subcommands)

Implementado em `touring-server/src/cli/generate.rs`. Pipeline typestate: Draft → Verified → Rendered → Speculated → Committed.

| Comando | Descrição |
|---------|-----------|
| `touring generate list-kinds [-j]` | Lista os 30 GeneratorKind com template names |
| `touring generate render <kind> [--vars '{}'] [-j]` | Render template com variáveis |
| `touring generate plan <kind> [-j]` | Scaffold de GeneratorPlan JSON |
| `touring generate verify --symbol <name> [-j]` | VGP symbol verification |
| `touring generate plan-submit --plan-file <path> [-j]` | Pipeline completo (verify→commit) |
| `touring generate plan-validate --plan-file <path> [-j]` | Valida plan JSON contra schema |
| `touring generate plan-verify --plan-file <path> [-j]` | VGP batch verify do plan |
| `touring generate plan-render --plan-file <path> [-j]` | Render sem commit |
| `touring generate plan-speculate --plan-file <path> [-j]` | Shadow validation |
| `touring generate plan-commit --plan-file <path> [-j]` | Atomic commit |
| `touring generate plan-status --plan-file <path> [-j]` | Status do plan registry |
| `touring generate plan-export --format json\|yaml\|toml [-j]` | Export plan |
| `touring generate plan-diff --plan-file <a> --other <b> [-j]` | Diff entre plans |
| `touring generate plan-critique --plan-file <path> [-j]` | Análise crítica do plan |
| `touring generate plan-suggest --intent "<text>" [-j]` | Sugestão de plan |
| `touring generate plan-recall --query "<text>" [-j]` | Recall de plans similares |
| `touring generate plan-history --plan-file <path> [-j]` | Histórico de execuções |
| `touring generate plan-replay --plan-file <path> [-j]` | Re-executa plan |
| `touring generate plan-rollback --plan-file <path> [-j]` | Rollback via rkyv snapshot |
| `touring generate template-list [-j]` | Lista 29 templates Tera |
| `touring generate template-validate --template-file <path> [-j]` | Valida template |
| `touring generate template-test --template <name> [--vars '{}'] [-j]` | Testa template com vars |
| `touring generate schema-dump [-j]` | JSON Schema do GeneratorPlan |
| `touring generate capacity [-j]` | Capacity limits atuais |

**Skill Claude Code**: `~/.claude/skills/touring-generator/SKILL.md` — auto-invocação quando code generation é necessária.

**Session/Decompose integration**: Cada `plan-submit` auto-cria touring session + touring decompose task.

---

## RenderShape Budget (Wave B v4.26.0)

Budget constraint system for generator output width/indent. `touring-generator/src/shape.rs` — 169 LOC.

```rust
pub struct RenderShape {
    pub budget: u16,   // max columns
    pub indent: u8,    // current indentation level
    pub width: u16,    // remaining width
}

impl RenderShape {
    pub fn indent(&mut self) { self.indent += 1; self.width = self.budget - (self.indent * 2) as u16; }
    pub fn dedent(&mut self) { self.indent = self.indent.saturating_sub(1); self.width = self.budget - (self.indent * 2) as u16; }
}
```

Prevents generator output from exceeding column budgets. All 30 GeneratorKind templates use RenderShape. Overflow emits G-200 diagnostic.

---

## touring-assists Framework (Wave C v4.27.0)

10 assist handlers for refactor-as-CLI operations. Produces SourceChange artifacts committed via Applier.

| Handler | Purpose | RFC-100 |
|---------|---------|---------|
| `add_missing_match_arms` | Enum variant arms | A-100 |
| `auto_import` | Unresolved symbol import | A-101 |
| `auto_wire` | Orphan pub → consumer (offensive against 199.832 orphans) | A-102 |
| `change_visibility` | pub ↔ pub(crate) ↔ pub(super) ↔ private | A-103 |
| `convert_to_guarded_return` | `if cond { body } else { return; }` → early-return | A-104 |
| `extract_function` | Extract block to new function | A-105 |
| `generate_impl` | `impl Trait for Type` skeleton | A-106 |
| `inline_call` | Replace call with body | A-107 |
| `merge_imports` | Collapse adjacent `use` statements | A-108 |
| `move_module_to_file` | `mod foo { }` → `mod foo;` + `foo.rs` | A-109 |

CLI: `touring assist list-kinds | applicable <file>:<line>:<col> | apply <kind> <file> <range>`
MCP: `assist_apply` tool. Full reference: [touring-cli-assists.md](touring-cli-assists.md).

---

**Outros módulos**: [overview](touring-cli-overview.md) | [hooks](touring-cli-hooks.md) | [intelligence](touring-cli-intelligence.md) | [tasks](touring-cli-tasks.md) | [rl-quality](touring-cli-rl-quality.md) | [meta](touring-cli-meta.md)
