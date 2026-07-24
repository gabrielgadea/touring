# TACO Iter17/18 — EC22: bash failure signal em enrich_with_cognitive()

**Data**: 2026-04-11  
**Iteração**: 17-18  
**EC implementado**: EC22  
**Arquivo modificado**: `crates/touring-hooks/src/shared/signals.rs`  
**Resultado**: 0 erros cargo check, 74/74 testes passing

---

## Contexto

`enrich_with_cognitive()` em `shared/signals.rs` é a função central de enriquecimento de contexto
cognitivo nos hooks de resposta síncronos. Ela já consumia `knowledge_ref()` para:
- `file_risk()` → signal de risco
- `gotchas_for_file()` → signal de gotchas

Porém, ignorava completamente `recent_bash_outcomes()` da trait `KnowledgeSource`.

EC21 (Iter16) adicionou `bash_failures` ao `EnrichedCtx` async em `resolve_enriched()`.
EC22 completa o quadro no caminho síncrono — `enrich_with_cognitive()` consumida por `post_edit.rs:675`.

## Falsos Positivos Evitados (FASE 1)

| Target | Verdict | Evidência |
|--------|---------|-----------|
| `recent_edits()` | FALSE POSITIVE | Já wired em bridge.rs:391, pre_edit.rs:341, post_write.rs:758 |
| `resolve_cognitive_context()` | Complexidade L3 | Async, requer refactor de hooks |

## Mudança EC22

```rust
// EC22: Recent bash failures — surfaces past command failures as proactive context.
// Complements EnrichedCtx.bash_failures (EC21 async path) on the sync signal path.
let recent_failures: Vec<String> = knowledge
    .recent_bash_outcomes(3)
    .into_iter()
    .filter(|o| !o.success)
    .map(|o| o.command_short.clone())
    .collect();
if !recent_failures.is_empty() {
    signals.push(format!("⚡ last fail: {}", recent_failures.join(", ")));
}
```

Adicionado dentro do bloco `if let Some(knowledge) = cognitive.knowledge_ref()`, após o bloco de gotchas.
Utiliza `⚡` (U+26A1) — mesmo ícone usado para gotchas, indicando aviso operacional.

## Impacto

`enrich_with_cognitive()` é consumida por:
- `post_edit.rs:675`: `enrich_with_cognitive(cognitive, file_path, false)` → resultado vai para `issues`
- Outros callers que usam cognitive enrichment

`recent_bash_outcomes(3)` via `KnowledgeSource` (sync) — agora tem novo caller de produção.

## Sinergia EC21 + EC22

| Path | EC | Resultado |
|------|-----|-----------|
| Async (`resolve_enriched`) | EC21 | `EnrichedCtx.bash_failures: Option<Vec<String>>` |
| Sync (`enrich_with_cognitive`) | EC22 | Signal `"⚡ last fail: cargo, ruff"` no output |

Ambos paths agora expõem bash failures — cobertura completa.

## Validação

```
cargo check --workspace  → Finished (0 errors)
cargo test -p touring-hooks --lib signals → 74 passed, 0 failed
```
