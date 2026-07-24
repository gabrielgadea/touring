# TACO Iter20 — EC28 + EC29 + EC30: file_notes, task_metrics_count, cognitive enrichment

**Data**: 2026-04-11
**Iteração**: 20
**ECs implementados**: EC28, EC29, EC30
**Arquivos modificados**: `graph_service.rs`, `cli_handlers.rs`, `cli_e2e.rs`, `instructions_loaded.rs`
**Resultado**: 0 erros cargo check, 1452/1452 tests passing, sem regressão

---

## EC28 — file_notes em GraphFocusCtx (single roundtrip extension)

### Problema
`FileKnowledge.notes` (TABLE_FILE_KNOWLEDGE) nunca exposto em GraphFocusCtx.
Campo disponível no mesmo `adb.lookup()` já executado para EC23+EC25.

### Otimização: Single Roundtrip Extension (EC23+EC25 → EC28)
EC28 estende a tupla EC23+EC25 de 3 para 4 campos — sem nova roundtrip de DB:

```rust
// EC23+EC25+EC28: single adb.lookup() call extracts read_count, line_count, symbol_count,
// and notes from TABLE_FILE_KNOWLEDGE. One roundtrip for four fields.
let (read_count, line_count, symbol_count, file_notes): (i64, i64, i64, Option<String>) =
    if let Some(ref adb) = self.async_knowledge {
        adb.lookup(file)
            .await
            .ok()
            .flatten()
            .map(|k| {
                let notes = k.notes.map(|n| {
                    if n.len() > 500 {
                        // UTF-8-safe truncation via char_indices
                        let cut = n
                            .char_indices()
                            .map(|(i, _)| i)
                            .take_while(|&i| i <= 497)
                            .last()
                            .unwrap_or(0);
                        format!("{}…", &n[..cut])
                    } else {
                        n
                    }
                });
                (k.read_count, k.line_count, k.symbol_count, notes)
            })
            .unwrap_or((0, 0, 0, None))
    } else {
        (0, 0, 0, None)
    };
```

### Impacto
GraphFocusCtx agora tem **15 campos** (era 14 após EC24+EC25).
`file_notes: Option<String>` emitido em inject() para todos os 26 MCP tools.
UTF-8-safe truncation via `char_indices()` evita panic em strings multi-byte.

---

## EC29 — task_metrics_count em knowledge_activity (cli_handlers.rs + cli_e2e.rs)

### Problema
`TABLE_TASK_DECOMPOSITIONS` (schema_guard.rs) nunca aparecia no output do `touring e2e`
nem no `knowledge_activity` block do `touring status`. Dados de decomposição DAG invisíveis.

### Mudança
Query direta `SELECT COUNT(*) FROM task_decompositions` em ambos os arquivos,
seguindo o mesmo pattern das queries existentes:

```rust
// EC29: Task decomposition metrics count from task_decompositions table.
// No schema_guard constant exists — using raw literal consistent with knowledge.rs:2489.
let task_metrics_count: i64 = db
    .conn_ref()
    .query_row("SELECT COUNT(*) FROM task_decompositions", [], |r| r.get(0))
    .unwrap_or(0);
```

Adicionado ao `knowledge_activity` block em ambos os arquivos:
```json
"knowledge_activity": {
    "access_count": ...,
    "bash_count": ...,
    "edit_count": ...,
    "gotcha_count": ...,
    "coedit_pairs": ...,
    "relation_count": ...,
    "task_metrics_count": ...   // EC29 — novo
}
```

**Nota**: Não existe `schema_guard::TABLE_TASK_DECOMPOSITIONS` — raw literal usado
consistentemente com `knowledge.rs:2489` e `persistence.rs:81`.

---

## EC30 — Cognitive enrichment em instructions_loaded.rs

### Problema
`enrich_with_cognitive()` (shared/signals.rs — EC22) era usado em `post_edit.rs`
e `pre_write.rs`, mas nunca em `instructions_loaded.rs`. Na inicialização da sessão,
sinais cognitivos (bash failures, file risk) não eram injetados no contexto inicial do LLM.

### Mudança
Import adicionado:
```rust
use crate::shared::signals::enrich_with_cognitive;
```

Bloco de enriquecimento inserido em `run_returning()` após `gotcha_count` push,
antes do `if parts.is_empty()` check:

```rust
// EC30: Cognitive enrichment — surfaces bash failures and file risk signals at session start.
// Replicates post_edit.rs:674 pattern. Empty file_path skips file-risk block; recent_bash_outcomes still fires.
if let Some(ref cognitive) = runtime.cognitive {
    let enriched = enrich_with_cognitive(cognitive, "", false);
    if !enriched.is_empty() {
        parts.push(enriched);
    }
}
```

### Semântica de file_path=""
Passar `""` como file_path é seguro: `file_risk("")` retorna 0.0 (skip do bloco de risco),
mas `recent_bash_outcomes` ainda dispara e injeta sinais de falhas recentes no contexto.

### Impacto
`HookRuntime.cognitive` agora tem **3 callers** (era 2 — post_edit.rs e pre_write.rs).
Na inicialização da sessão, o LLM recebe sinais cognitivos junto com o project knowledge summary.

---

## Estado Acumulado de GraphFocusCtx

| Campo | Tabela | EC |
|-------|--------|-----|
| `focused_file` | — | original |
| `imports` | SymbolIndex | original |
| `imported_by` | SymbolIndex | original |
| `blast_radius_count` | SymbolIndex | original |
| `neighbor_files` | SymbolIndex | original |
| `confidence_modifier` | — | original |
| `source` | — | original |
| `coedit_files` | TABLE_FILE_COEDITS | EC11 |
| `access_count` | TABLE_FILE_ACCESS_LOG | EC18 |
| `edit_count` | TABLE_EDIT_HISTORY | EC20 |
| `read_count` | TABLE_FILE_KNOWLEDGE | EC23 |
| `relation_count` | TABLE_FILE_RELATIONS | EC24 |
| `line_count` | TABLE_FILE_KNOWLEDGE | EC25 |
| `symbol_count` | TABLE_FILE_KNOWLEDGE | EC25 |
| `file_notes` | TABLE_FILE_KNOWLEDGE | EC28 |

**Total**: 15 campos (era 7 original)

---

## knowledge_activity — Estado Acumulado

| Campo | Tabela | EC |
|-------|--------|----|
| `access_count` | TABLE_FILE_ACCESS_LOG | EC19b |
| `bash_count` | TABLE_BASH_OUTCOMES | EC19b |
| `edit_count` | TABLE_EDIT_HISTORY | EC19b |
| `gotcha_count` | TABLE_GOTCHAS | EC19b |
| `coedit_pairs` | TABLE_FILE_COEDITS | EC19b |
| `relation_count` | TABLE_FILE_RELATIONS | EC26 |
| `task_metrics_count` | task_decompositions | EC29 |

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed
```
