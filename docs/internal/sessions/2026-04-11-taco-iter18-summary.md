# TACO Iter18 — EC22 + EC23: bash failure signal + read_count em GraphFocusCtx

**Data**: 2026-04-11
**Iteração**: 18
**ECs implementados**: EC22, EC23
**Arquivos modificados**:
- `crates/touring-hooks/src/shared/signals.rs` (EC22)
- `crates/touring-server/src/graph_service.rs` (EC23)
**Resultado**: 0 erros cargo check, 74/74 testes signals, 0/0 server

---

## EC22 — bash failure signal em enrich_with_cognitive()

### Problema
`enrich_with_cognitive()` em `shared/signals.rs` usava `knowledge_ref()` para risk + gotchas
mas ignorava `recent_bash_outcomes()` — método sync disponível na mesma `KnowledgeSource`.

### Mudança
```rust
// EC22: Recent bash failures — surfaces past command failures as proactive context.
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

Adicionado dentro do bloco `if let Some(knowledge) = cognitive.knowledge_ref()`, após gotchas.

### Sinergia com EC21
| Path | EC | Resultado |
|------|-----|-----------|
| Async (`resolve_enriched`) | EC21 | `EnrichedCtx.bash_failures: Option<Vec<String>>` |
| Sync (`enrich_with_cognitive`) | EC22 | Signal `"⚡ last fail: cargo, ruff"` |

Ambos paths agora expõem bash failures.

---

## EC23 — read_count em GraphFocusCtx via adb.lookup()

### Problema
`AsyncFileKnowledgeDB::lookup()` — 0 callers de produção (confirmado por Scout 2).
`FileKnowledge.read_count` (pre-read hook hits, TABLE_FILE_KNOWLEDGE) nunca exposto em GraphFocusCtx.
`access_count` (TABLE_FILE_ACCESS_LOG) e `edit_count` (TABLE_EDIT_HISTORY) já existiam (EC18/EC20).

### 4-Step Pattern (EC18/EC20 → EC23)

**Step 1**: Campo `read_count: i64` adicionado a `GraphFocusCtx` após `edit_count`
**Step 2**: `Default::read_count = 0`
**Step 3**: Em `resolve_ctx()`:
```rust
// EC23: pre-read hook hit count from TABLE_FILE_KNOWLEDGE via adb.lookup().
let read_count: i64 = if let Some(ref adb) = self.async_knowledge {
    adb.lookup(file).await.ok().flatten().map(|k| k.read_count).unwrap_or(0)
} else {
    0
};
```
**Step 4**: Em `inject()`: `"read_count": ctx.read_count`

### Distinção access_count vs read_count
| Campo | Tabela | Origem |
|-------|--------|--------|
| `access_count` | TABLE_FILE_ACCESS_LOG | Access tracking hook |
| `read_count` | TABLE_FILE_KNOWLEDGE | Pre-read hook hits |
| `edit_count` | TABLE_EDIT_HISTORY | Post-edit hook |

### Impacto
`AsyncFileKnowledgeDB::lookup()` agora tem **1 caller de produção** (era 0).
GraphFocusCtx agora tem **11 campos** (era 10).

---

## Scouts Utilizados (Iter17/18)

| Scout | Findings | False Positives |
|-------|----------|-----------------|
| Scout 1 (ab58fc) | recent_edits=FP, bash_failures=REAL, pre_edit gap=REAL | 1 (recent_edits) |
| Scout 2 (a1f519) | lookup=REAL, get_relations=REAL, GraphFocusCtx gap=REAL | 0 |

---

## Validação

```
cargo check --workspace  → Finished (0 errors)
cargo test -p touring-hooks --lib signals → 74 passed, 0 failed
cargo test -p touring-server → ok (0 tests, no regression)
```
