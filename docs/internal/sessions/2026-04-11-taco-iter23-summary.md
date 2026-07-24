# TACO Iter23 — EC35 + EC36 + EC37: cognitive enrichment em post_bash + pre_bash + bash_failures em GraphFocusCtx

**Data**: 2026-04-11
**Iteração**: 23
**ECs implementados**: EC35, EC36, EC37
**Arquivos modificados**: `post_bash.rs`, `pre_bash.rs`, `async_knowledge.rs`, `graph_service.rs`
**Resultado**: 0 erros cargo check, 330 tests passing (touring-hooks + touring-server), sem regressão

---

## EC35 — Cognitive enrichment em post_bash.rs

### Problema
`post_bash.rs` captura resultados de bash (sucesso/falha, temporal context, Pensieve, ACO)
mas nunca injetava contexto cognitivo ao LLM — justamente após a execução de comandos, quando
bash failures recentes + risco de arquivo são mais valiosos para auto-correção.

### Mudança
Bloco EC35 pré-computa `cog_enriched: Option<String>` ANTES do bloco temporal, depois
injeta em AMBOS os caminhos: temporal (quando `temporal_ctx = Some(...)`) e fallback
(quando temporal_ctx é None).

```rust
// EC35: Pre-compute cognitive enrichment for the failure path.
let cog_enriched: Option<String> = if !outcome.success {
    runtime.cognitive.as_ref().and_then(|cog| {
        let path_ref = outcome.file_context.as_deref().unwrap_or("");
        let enriched = crate::shared::signals::enrich_with_cognitive(cog, path_ref, false);
        if enriched.is_empty() { None } else { Some(enriched) }
    })
} else {
    None
};
```

**Design decisions**:
- Somente em `!outcome.success` — evita overhead em comandos bem-sucedidos
- `outcome.file_context.as_deref().unwrap_or("")` — file-specific risk quando disponível; `""` retorna recent_bash_failures globais
- Mergeado em AMBOS os paths do match temporal_ctx para cobertura total

### Impacto
`enrich_with_cognitive` agora tem **10 callers** (era 9 após EC34).
Falhas de bash agora recebem contexto cognitivo (risco + bash failures recentes + gotchas).

---

## EC36 — Cognitive enrichment em pre_bash.rs

### Problema
`pre_bash.rs` constrói contexto CILA-budgeted com Pensieve warnings mas não injetava sinais
cognitivos. Comandos como `cargo build`, `pytest`, `python scripts/foo.py` se beneficiam de
contexto cognitivo ANTES de serem executados (prevenção vs. diagnóstico pós-falha).

### Mudança
Shadow-rebinding da variável `merged` com cognitive enrichment injetado ANTES do `match merged`
final (que aplica truncate CILA). Pattern espelha `pre_edit.rs:EC32`.

```rust
// EC36: Cognitive enrichment — inject file risk and bash failure signals into pre-bash context.
let merged = if let Some(ref cognitive) = runtime.cognitive {
    let path_ref = file_ctx.as_deref().unwrap_or("");
    let enriched = crate::shared::signals::enrich_with_cognitive(cognitive, path_ref, false);
    if enriched.is_empty() {
        merged
    } else {
        match merged {
            Some(m) => Some(format!("{m}\n{enriched}")),
            None => Some(enriched),
        }
    }
} else {
    merged
};
```

**Design decisions**:
- Shadow rebinding Rust (`let merged = ...`) — clean injection sem reestruturar o bloco
- `file_ctx.as_deref().unwrap_or("")` — `file_ctx` já extraído em linha 97 do comando
- CILA budget (`truncate_str`) aplicado pelo `match merged` existente — sem duplicação
- Dispara para comandos sem path (`""`) via `recent_bash_outcomes` global

### Impacto
`enrich_with_cognitive` agora tem **11 callers**.
Contexto cognitivo disponível PRE-execução de bash — previne em vez de apenas corrigir.

---

## EC37 — `bash_failures: i64` em GraphFocusCtx

### Problema
`GraphFocusCtx` tinha `access_count` (leituras), `edit_count` (edições), `read_count`
(pre-read hits), mas NENHUM sinal de falha de execução por arquivo. O LLM recebia via
`graph_ctx` a frequência de leitura/edição mas não sabia quantos comandos bash falharam
naquele arquivo — signal crítico para risk assessment.

### Mudança

**async_knowledge.rs**: novo método `bash_failures_for_file()`:
```rust
pub async fn bash_failures_for_file(&self, file_path: &str) -> Result<i64, AsyncKnowledgeError> {
    // SELECT COUNT(*) FROM bash_outcomes WHERE file_context = ?1 AND success = 0
}
```

**graph_service.rs** — 4 mudanças atômicas:
1. Campo `bash_failures: i64` em `GraphFocusCtx` struct (field #16)
2. `bash_failures: 0` em `Default` impl
3. Query em `resolve_ctx()` via `adb.bash_failures_for_file(file).await.unwrap_or(0)`
4. `"bash_failures": ctx.bash_failures` em `inject()` output

### Impacto
`GraphFocusCtx` agora tem **16 campos** (era 15).
Todas as 26 MCP tool responses que incluem `graph_ctx` passam a expor `bash_failures`.
LLM recebe sinal: "esse arquivo teve N comandos bash falhando" → risk-aware decisions.

---

## Estado Acumulado de enrich_with_cognitive callers

| Arquivo | EC | Trigger | file_path | predictions |
|---------|-----|---------|-----------|-------------|
| `pre_read.rs:375` | original | sempre | file_path | `true` |
| `post_edit.rs:675` | EC22 | sempre | file_path | `false` |
| `post_write.rs:210` | EC22 | sempre | file_path | `false` |
| `pre_write.rs:309` | EC22 | sempre | file_path | `false` |
| `instructions_loaded.rs:61` | EC30 | session_start | `""` | `false` |
| `cli_handlers_scout.rs` | EC31 | PreToolUse | file_path | `false` |
| `pre_edit.rs:216` | EC32 | sempre | file_path | `false` |
| `post_tool_use.rs` | EC33 | L4+ only | file_path | `false` |
| `post_tool_failure.rs` | EC34 | tool failure | file_path | `false` |
| `post_bash.rs` | EC35 | bash failure | file_ctx\|"" | `false` |
| `pre_bash.rs` | EC36 | sempre | file_ctx\|"" | `false` |

**Total callers**: 11 (era 1 original apenas em pre_read)

---

## Estado Acumulado de GraphFocusCtx fields

| Campo | EC | Source | Table |
|-------|-----|--------|-------|
| `focused_file` | original | params | — |
| `imports` | original | SymbolIndex | dependencies |
| `imported_by` | original | SymbolIndex | reverse_deps |
| `blast_radius_count` | original | len(imported_by) | — |
| `neighbor_files` | original | imports ∪ imported_by | — |
| `confidence_modifier` | original | blast_radius_count | — |
| `source` | original | enum | — |
| `coedit_files` | GS-EC11 | AsyncFileKnowledgeDB | TABLE_FILE_COEDITS |
| `access_count` | EC18 | AsyncFileKnowledgeDB | TABLE_FILE_ACCESS_LOG |
| `edit_count` | EC20 | AsyncFileKnowledgeDB | TABLE_EDIT_HISTORY |
| `read_count` | EC23 | AsyncFileKnowledgeDB | TABLE_FILE_KNOWLEDGE.read_count |
| `relation_count` | EC24 | AsyncFileKnowledgeDB | TABLE_FILE_RELATIONS |
| `line_count` | EC25 | AsyncFileKnowledgeDB | TABLE_FILE_KNOWLEDGE.line_count |
| `symbol_count` | EC25 | AsyncFileKnowledgeDB | TABLE_FILE_KNOWLEDGE.symbol_count |
| `file_notes` | EC28 | AsyncFileKnowledgeDB | TABLE_FILE_KNOWLEDGE.notes |
| `bash_failures` | EC37 | AsyncFileKnowledgeDB | TABLE_BASH_OUTCOMES (success=0) |

**Total campos**: 16 (era 1 campo original apenas em pre_read)

---

## Cobertura Cognitiva por Evento (após EC35+EC36)

| Evento Claude Code | Hook | Cognitive |
|--------------------|------|-----------|
| PreToolUse (Read/Write/Edit) | `pre_read`, `pre_edit`, `pre_write`, `pre_task_scout` | ✓ |
| PreToolUse (Bash) | `pre_bash` | ✓ EC36 |
| PostToolUse (Write) | `post_write` | ✓ |
| PostToolUse (Edit) | `post_edit` | ✓ |
| PostToolUse (qualquer, L4+) | `post_tool_use` | ✓ EC33 |
| PostToolUse (falha) | `post_tool_failure` | ✓ EC34 |
| PostBash (falha) | `post_bash` | ✓ EC35 |
| SessionStart | `instructions_loaded` | ✓ EC30 |

**Cobertura**: 100% dos hook events relevantes têm cognitive enrichment.

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks -p touring-server --lib → 330 passed, 0 failed
```
