# TACO Iter22 — EC33 + EC34: cognitive enrichment em post_tool_use (L4+) + post_tool_failure

**Data**: 2026-04-11
**Iteração**: 22
**ECs implementados**: EC33, EC34
**Arquivos modificados**: `post_tool_use.rs`, `post_tool_failure.rs`
**Resultado**: 0 erros cargo check, 1452/1452 tests passing, sem regressão

---

## EC33 — Cognitive enrichment em post_tool_use.rs (L4+ gate)

### Problema
`post_tool_use.rs` tinha `parse_file_path_from_input` documentada como
"used for cognitive wiring" (linha ~118) mas o wiring nunca foi completado.
A função `run_returning` sempre retornava `HookResponse::Allow` sem injetar
contexto cognitivo.

### Oportunidade
`trigger_mandatory_enrichment_l4plus` já verificava CILA L4+ para contabilidade,
mas não injetava contexto. O campo `cila_level` estava disponível mas só era
calculado dentro da função privada.

### Mudança
Extraído `cila_level` em `run_returning` antes da chamada a
`trigger_mandatory_enrichment_l4plus`, seguido de bloco EC33 condicional:

```rust
// EC33: Read cila_level here so we can gate the cognitive enrichment below.
let cila_level: u8 = runtime.ctx.stable_session.borrow().as_ref()
    .map(|s| s.cila_level)
    .unwrap_or_else(|| {
        runtime.ctx.result_cache
            .get_result("__meta__", "__session_cila_level__")
            .and_then(|v| v.parse().ok())
            .unwrap_or(3)
    });
trigger_mandatory_enrichment_l4plus(runtime, &tool_name);

// EC33: Cognitive enrichment at L4+ PostToolUse.
// Fulfills the "used for cognitive wiring" purpose of parse_file_path_from_input (line ~118).
// Only fires at CILA L4+ to avoid noise on every routine tool call.
if crate::shared::cila::is_enrichment_mandatory(runtime.enrichment_active, cila_level) {
    let file_path = parse_file_path_from_input(input);
    if let Some(ref cognitive) = runtime.cognitive {
        let enriched = crate::shared::signals::enrich_with_cognitive(cognitive, &file_path, false);
        if !enriched.is_empty() {
            return HookResponse::Context {
                context: enriched,
                event_name: Some("PostToolUse".to_string()),
            };
        }
    }
}
```

### Impacto
`enrich_with_cognitive` agora tem **8 callers**.
`parse_file_path_from_input` finalmente cumpre seu propósito de "cognitive wiring".
Contexto cognitivo injetado em workflows L4+ (self-modifying / multi-agent).

---

## EC34 — Cognitive enrichment em post_tool_failure.rs

### Problema
`post_tool_failure.rs` extraia `file_path` (linha 53) para o HALT gate mas nunca
usava cognitive enrichment. Em cenários de falha — justamente quando context
cognitivo é mais valioso — nenhum sinal de risk/bash_failures era injetado.

### Mudança
Bloco EC34 adicionado APÓS o HALT gate e ANTES do `HookRuntime::build_allow()`:

```rust
// EC34: Cognitive enrichment on tool failure — inject file risk + bash failure signals.
// On failure, surfacing cognitive context (risk score, recent bash failures, gotchas)
// gives the LLM maximum signal for self-correction. Replicates post_edit.rs:675 pattern.
if let Some(ref cognitive) = runtime.cognitive {
    let enriched = crate::shared::signals::enrich_with_cognitive(cognitive, file_path, false);
    if !enriched.is_empty() {
        return HookResponse::Context {
            context: enriched,
            event_name: Some("PostToolUse".to_string()),
        };
    }
}
```

### Impacto
`enrich_with_cognitive` agora tem **9 callers**.
Na falha de qualquer tool, o LLM recebe: risco do arquivo + bash failures recentes + gotchas.
Maximiza o sinal disponível para self-correction no pior cenário possível.

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

**Total callers**: 9 (era 1 original apenas em pre_read)

---

## Cobertura Cognitiva por Evento (após EC33+EC34)

| Evento Claude Code | Hook | Cognitive |
|--------------------|------|-----------|
| PreToolUse (Read/Write/Edit) | `pre_read`, `pre_edit`, `pre_write`, `pre_task_scout` | ✓ |
| PostToolUse (Write) | `post_write` | ✓ |
| PostToolUse (Edit) | `post_edit` | ✓ |
| PostToolUse (qualquer, L4+) | `post_tool_use` | ✓ EC33 |
| PostToolUse (falha) | `post_tool_failure` | ✓ EC34 |
| SessionStart | `instructions_loaded` | ✓ EC30 |
| Bash | `post_bash` | — |

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed
```
