# Bidirectional Task Flow — Claude Code ↔ Touring

**Data:** 2026-04-13
**Autor:** Claude Code (Cognitive Orchestrator) + Gabriel Gadea
**Status:** IMPLEMENTADO (B1-B5 concluídos) | **Testes:** pending (B6)
**Refinement level:** L3 (Refactoring cross-cutting)

---

## 1. Motivação

Antes desta mudança, o fluxo era **unidirecional**: Claude Code criava `TaskCreate` → hook `task-sync-post-create` espelhava no Touring como DAG + 3 subtasks `scout/implement/validate`. O Touring era espelho passivo.

**Lacuna identificada pelo usuário**: *"a lógica precisa ser inversa, a criação de uma tarefa no Touring deve provocar a criação de uma tarefa no Claude Code com toda riqueza de informações do Touring"*.

**Escopo adotado**: **bidirecional**, não inverso. Preservamos CC → Touring (canal de mirror) e adicionamos Touring → CC (canal de digest enriquecido). Agentes externos, scouts, MCTS e evolution drift podem originar tasks que o CC adota voluntariamente.

---

## 2. Arquitetura

### 2.1 Dois canais, uma tabela

| Canal | Origin | Mecanismo | mirrored_to_cc inicial |
|---|---|---|---|
| **CC → Touring** (existente) | `claude-code` | Hook `task-sync-post-create` (PostToolUse) | `1` (auto-mirrored) |
| **Touring → CC** (novo) | `touring-cli`, `external-agent`, etc. | `touring decompose create --origin=X` + digest injection | `0` (aguarda adoção) |

### 2.2 Invariantes anti-loop

| Invariante | Enforcement |
|---|---|
| Toda task tem `origin` explícito | `DEFAULT 'claude-code'` + CLI flag `--origin` |
| CC-originated nunca é re-sugerida ao CC | `digest` filtra `mirrored_to_cc = 0` |
| CC adotando Touring task NÃO duplica DAG | Hook detecta `external_ref` → `cli_decompose_mark_mirrored` em vez de `cli_decompose_create` |

### 2.3 Fluxos

**Fluxo A — CC-originated (unchanged)**:
```
CC.TaskCreate(subject="X")
  → hook task-sync-post-create
    → cli_decompose_create(origin="claude-code", mirrored_to_cc=1)
    → auto-subtasks scout/implement/validate
  → digest NÃO re-sugere (filtro mirrored_to_cc=0 falha)
```

**Fluxo B — Touring-originated (novo)**:
```
(scout/MCTS/drift/agent) → touring decompose create intent "refatorar X" --origin=external-agent
  → row: {origin: "external-agent", mirrored_to_cc: 0, status: "created"}

[next session]
  → hook instructions-loaded (session_start)
    → task_digest::digest_pending_tasks(runtime)
    → SELECT task_id, description, origin WHERE mirrored_to_cc=0 AND status IN ('created','active','ready') LIMIT 5
    → additionalContext: "Touring tasks (N pending): [id] desc (origin) ... Adopt via TaskCreate(external_ref=<id>)"

  → CC lê contexto, decide adotar:
    → CC.TaskCreate(subject="refatorar X", external_ref="task_178...")
    → hook task-sync-post-create detecta external_ref
      → cli_decompose_mark_mirrored(task_id="task_178...")
      → UPDATE mirrored_to_cc=1
      → SKIP cli_decompose_create + auto-subtasks (já existem)
  → digest para de sugerir essa task (mirrored_to_cc=1 agora)
```

---

## 3. Schema delta

`task_decompositions` ganha 2 colunas via `ALTER TABLE ADD COLUMN` idempotente
(in `ensure_decompose_tables` em `cli_handlers.rs`):

```sql
ALTER TABLE task_decompositions ADD COLUMN origin TEXT NOT NULL DEFAULT 'claude-code';
ALTER TABLE task_decompositions ADD COLUMN mirrored_to_cc INTEGER NOT NULL DEFAULT 1;
CREATE INDEX IF NOT EXISTS idx_task_origin_mirror ON task_decompositions(origin, mirrored_to_cc);
```

**Retrocompat**: colunas com `DEFAULT` não quebram queries antigas nem `SELECT *`.

---

## 4. CLI

```bash
# Nova flag --origin (default: touring-cli quando chamado via CLI)
touring decompose create intent "refactor X" --origin=touring-cli
touring decompose create bug "fix leak" --origin=external-agent

# Consultar tasks pendentes de adoção
sqlite3 .../knowledge.db "SELECT task_id, description, origin FROM task_decompositions WHERE mirrored_to_cc=0 LIMIT 5"
```

---

## 5. Arquivos afetados

| Arquivo | Mudança | LOC delta |
|---|---|---|
| `crates/touring-hooks/src/cli_handlers.rs` | `ensure_decompose_tables`: 2 ALTER + index. `cli_decompose_create`: aceita `origin` e deriva `mirrored_to_cc`. Novo handler `cli_decompose_mark_mirrored`. | +30 |
| `crates/touring-server/src/cli/decompose.rs` | `create` subcmd: parse `--origin=<val>` flag, strip do description, propaga no payload. | +12 |
| `crates/touring-hooks/src/lifecycle/task_create.rs` | Detecta `tool_input.external_ref`, ramifica em adoption path vs standard path. Subtask IDs declarados antes do branch para uso pelo `persist_task_creation`. | +25 refactor |
| `crates/touring-hooks/src/task_digest.rs` | **NOVO** módulo: `digest_pending_tasks(runtime) -> Option<String>`. Query + format (<5ms). | +84 |
| `crates/touring-hooks/src/lib.rs` | Registra `pub mod task_digest;`. | +1 |
| `crates/touring-hooks/src/instructions_loaded.rs` | Chama `task_digest::digest_pending_tasks` após enrichment cognitive, injeta no `additionalContext`. | +7 |

**Total**: ~160 LOC, 6 arquivos tocados, zero breaking changes.

### 5.1 Decisão arquitetural (vs. plano original)

Plano original propunha `lifecycle/touring_task_digest.rs` + alteração em `hook_registry.rs` + novo `mod` em `lifecycle.rs` façade. Isso criaria **dependência forte do B5 em D10** (façade clean).

Decisão final: módulo **standalone** `crates/touring-hooks/src/task_digest.rs` chamado a partir do hook existente `instructions-loaded`. Vantagens:
- ✅ Zero dependência da modularização em curso (D5-D10)
- ✅ Zero modificação de `hook_registry.rs` (reaproveita hook existente)
- ✅ Não aumenta contagem de hooks (`ALL_DAEMON_HOOK_NAMES.len()` permanece 124)
- ✅ Latência preservada (<5ms total no `instructions-loaded`)

---

## 6. Testes planejados (B6 — pendente)

```rust
#[test]
fn test_external_ref_triggers_adoption_path() {
    // Setup: CC-originated task + external task in DB
    // Act: fire hook with external_ref=external_task_id
    // Assert: mirrored_to_cc=1, no duplicate DAG, no auto-subtasks created
}

#[test]
fn test_digest_skips_cc_originated() {
    // Setup: 2 CC tasks (mirrored_to_cc=1) + 1 external task (mirrored_to_cc=0)
    // Act: digest_pending_tasks(runtime)
    // Assert: Some(digest) contains only external task, not CC ones
}

#[test]
fn test_loop_breaking_e2e() {
    // Setup: external task created
    // Act: digest → simulate CC.TaskCreate(external_ref=...) → hook
    // Assert: second digest returns None (task was mirrored, filtered out)
}
```

---

## 7. Casos de uso habilitados

| Origem | Cenário |
|---|---|
| **Evolution drift** | `touring evolution drift` detecta queda em métrica → cria task `"investigate edit_success_rate drop"` → digest surface no próximo session start |
| **Scout proativo** | Agente scout encontra 32 orphan symbols com impacto alto → cria task `"wire GotchaEntry to consumers"` → CC adota com contexto pré-computado |
| **MCTS planner** | MCTS expande árvore de decisão → materializa melhor caminho como task `"refactor X via approach A"` → CC implementa |
| **External agent (Kazuba/Kimi)** | Agente remoto detecta issue em PR → `curl touring decompose create ... --origin=external-agent` → CC vê no próximo session e adota |
| **CLI manual (Gabriel)** | Gabriel pode criar tasks diretamente via CLI enquanto codifica, CC vê no próximo turn |

---

## 8. Rollback e segurança

- **Idempotência**: `ALTER TABLE ADD COLUMN` silencia erro "duplicate column" (runs cleanly em DB pré-existente OU nova)
- **Fail-safe**: se schema `origin`/`mirrored_to_cc` ausente, `digest_pending_tasks` retorna `None` sem panic
- **Backward compat**: queries antigas `SELECT ... FROM task_decompositions` continuam funcionando; novas colunas têm `DEFAULT`
- **Zero git**: toda migration é in-place via ALTER; rollback requer apenas `ALTER TABLE DROP COLUMN` (SQLite 3.35+)

---

## 9. Observações finais

### 9.1 O que NÃO está neste escopo

- **TTL de tasks não-adotadas**: tasks com `mirrored_to_cc=0` acumulam indefinidamente. GC (`touring decompose gc --stale-days=7`) fica em ticket futuro.
- **Priorização de digest**: atualmente ordem é `ORDER BY created_at ASC LIMIT 5`. Ranking por `quality_score` / `cila_level` / `urgency` fica em Pln3.
- **Enriquecimento rico**: digest atual é texto simples. Plano original previa blast radius + memory recall + wiring suggest por task, mas essa computação (>5ms) excede o budget do `instructions-loaded`. Fica em hook dedicado `touring-task-digest-enrich` (Pln3).

### 9.2 Métrica de sucesso

✅ **Técnica**:
- `cargo test -p touring-hooks --lib` passa 2771+ testes pós-implementação
- `cargo clippy -D warnings` 0 novos warnings
- Hook `instructions-loaded` latência média <5ms (antes: ~3ms, delta: +1 SQLite query indexada)

⏳ **Funcional** (aguarda B6):
- Ciclo Touring→CC→Touring fecha sem loop em teste E2E
- Task CC-originated NÃO aparece no digest
- Task adotada (external_ref) marca `mirrored_to_cc=1` e sai do digest

---

*Implementado em 2026-04-13 sobreposto à modularização `lifecycle.rs` (D5-D10) em curso. Escopo: 6 arquivos, ~160 LOC. Nenhum hook novo registrado — integração via hook existente `instructions-loaded`.*
