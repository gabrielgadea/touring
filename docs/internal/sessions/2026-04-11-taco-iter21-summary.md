# TACO Iter21 — EC31 + EC32: enrich_with_cognitive em pre_task_scout + pre_edit

**Data**: 2026-04-11
**Iteração**: 21
**ECs implementados**: EC31, EC32
**Arquivos modificados**: `cli_handlers_scout.rs`, `pre_edit.rs`
**Resultado**: 0 erros cargo check, 1452/1452 tests passing, sem regressão

---

## EC31 — enrich_with_cognitive em cli_pre_task_scout

### Problema
`cli_pre_task_scout` em `cli_handlers_scout.rs` recebia `_rt: &mut HookRuntime` mas
nunca acessava `_rt.cognitive`. O parâmetro era usado apenas para `project_root`
e `quick_quality_context`, deixando os sinais cognitivos fora do contexto de pre-task scout.

### Mudança
Bloco adicionado após `quick_quality_context` e ANTES do "Store in cache":

```rust
// EC31: Cognitive enrichment — inject bash failure / file risk signals into pre-task scout.
// Replicates instructions_loaded.rs:61 / post_write.rs:210 pattern.
// file_path is the absolute path from payload; cognitive uses it for file_risk().
if let Some(ref cognitive) = _rt.cognitive {
    let enriched = crate::shared::signals::enrich_with_cognitive(cognitive, file_path, false);
    if !enriched.is_empty() {
        findings.push('\n');
        findings.push_str(&enriched);
    }
}
```

### Impacto
`enrich_with_cognitive` agora tem **6 callers** (era 5: post_edit, post_write, pre_write, pre_read, instructions_loaded).
`HookRuntime.cognitive` agora usado em **pre-task scout** — bash failures + file risk surfaceados no PreToolUse.

---

## EC32 — enrich_with_cognitive em pre_edit.rs

### Problema
`pre_edit.rs:run_returning` tinha acesso a `runtime.cognitive` mas nunca chamava
`enrich_with_cognitive`. O pre_edit já coletava signals de blast radius, quality,
PII, CILA, mas não incluía sinais cognitivos (file risk, bash failures, gotchas).

### Mudança
Bloco inserido após o PII scan (linha ~214) e antes do "Signal 15b + B1-store":

```rust
// ── EC32: Cognitive enrichment — inject file risk and bash failure signals ──
// Replicates post_edit.rs:675 / post_write.rs:210 / pre_write.rs:309 pattern.
// Always runs regardless of CILA gate — cognitive signals are pre-computed and cheap.
if let Some(ref cognitive) = runtime.cognitive {
    let enriched = crate::shared::signals::enrich_with_cognitive(cognitive, file_path, false);
    if !enriched.is_empty() {
        if !context.is_empty() { context.push_str(" | "); }
        context.push_str(&enriched);
    }
}
```

### Impacto
`enrich_with_cognitive` agora tem **7 callers** (era 6 após EC31).
`pre_edit` agora é o hook mais completo: blast_radius + quality + CILA + PII + cognitive.

---

## Estado Acumulado de enrich_with_cognitive callers

| Arquivo | EC | file_path | predictions |
|---------|-----|-----------|-------------|
| `pre_read.rs:375` | original | file_path | `true` |
| `post_edit.rs:675` | EC22 | file_path | `false` |
| `post_write.rs:210` | EC22 | file_path | `false` |
| `pre_write.rs:309` | EC22 | file_path | `false` |
| `instructions_loaded.rs:61` | EC30 | `""` | `false` |
| `cli_handlers_scout.rs` | EC31 | file_path | `false` |
| `pre_edit.rs:216` | EC32 | file_path | `false` |

**Total callers**: 7 (era 1 original apenas em pre_read)

---

## Hooks com cobertura cognitiva completa (após EC31+EC32)

| Hook | Cognitive | Blast | Quality | PII | CILA |
|------|-----------|-------|---------|-----|------|
| `pre_read` | ✓ EC22 | ✓ | ✓ | — | ✓ |
| `pre_edit` | ✓ **EC32** | ✓ | ✓ | ✓ | ✓ |
| `pre_write` | ✓ EC22 | ✓ | — | — | ✓ |
| `post_edit` | ✓ EC22 | — | — | — | — |
| `post_write` | ✓ EC22 | — | — | — | — |
| `instructions_loaded` | ✓ EC30 | — | — | — | — |
| `pre_task_scout` | ✓ **EC31** | — | ✓ | — | — |

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed
EC31 composite_score          → 1.0 (engineer agent)
```
