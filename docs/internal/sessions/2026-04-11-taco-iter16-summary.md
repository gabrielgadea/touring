# TACO Iter16 — EC21: bash_failures in EnrichedCtx

**Data**: 2026-04-11  
**Iteração**: 16  
**EC implementado**: EC21  
**Arquivo modificado**: `crates/touring-cognitive/src/bridge.rs`  
**Resultado**: 0 erros cargo check, 1/1 testes passing

---

## Contexto

`EnrichedCtx` é o struct de contexto cognitivo enriquecido retornado por `CognitiveRuntime::resolve_enriched()`.
Ele era populado com `risk_score`, `related_files`, `gotchas` e `dependent_count` — mas ignorava o método
`KnowledgeSource::recent_bash_outcomes()`, que estava declarado na trait mas nunca chamado em produção.

## Mudança EC21

### Campo adicionado a `EnrichedCtx`

```rust
/// Recent failed bash commands for proactive risk awareness.
#[serde(skip_serializing_if = "Option::is_none")]
pub bash_failures: Option<Vec<String>>,
```

Segue o padrão `Option<Vec<String>>` + `skip_serializing_if = "Option::is_none"` já estabelecido
pelos campos `gotchas` e `related_files`. Adicionado na posição lógica após `dependent_count`.

### `is_empty()` atualizado

```rust
pub fn is_empty(&self) -> bool {
    self.base.is_empty()
        && self.risk_score.is_none()
        && self.related_files.is_none()
        && self.gotchas.is_none()
        && self.dependent_count.is_none()
        && self.bash_failures.is_none()   // EC21
}
```

### População em `resolve_enriched()`

```rust
// EC21: Expose recent bash failures for proactive risk awareness.
let bash_failures: Option<Vec<String>> = {
    let failures: Vec<String> = knowledge
        .recent_bash_outcomes(5)
        .into_iter()
        .filter(|o| !o.success)
        .map(|o| o.command_short.clone())
        .collect();
    if failures.is_empty() { None } else { Some(failures) }
};
```

Parâmetro `5` → últimos 5 outcomes do banco. Filtra por `!o.success` → apenas falhas.
Mapeia `command_short` (campo compacto do `BashOutcomeRecord`).

## False Positives Evitados

- **EC21-A** (`edit_count` em `cli_e2e.rs`): FALSE POSITIVE — já implementado nas linhas 567-694 do EC19b.

## Impacto

`KnowledgeSource::recent_bash_outcomes()` agora tem **1 caller de produção** (era 0 antes desta iteração).
O método estava presente desde a implementação da trait, sem consumidor real até EC21.

`EnrichedCtx` agora carrega contexto de falhas recentes de bash — útil para o cognitive engine
ajustar recomendações quando há comandos falhando no ambiente.

## Validação

```
cargo check --workspace  → Finished (0 errors)
cargo test -p touring-cognitive → 1 passed, 0 failed
```

---

## Acumulado TACO Loop (Iter13–16)

| Iter | EC | Arquivo | Descrição |
|------|----|---------|-----------|
| 13 | EC13 | graph_service.rs | `coedit_files` em GraphFocusCtx |
| 14 | EC14a/b | pre_write.rs + wiring_status | co-edit signal + memory_count |
| 15 | EC15a | cli_wiring_status.rs | `memory_count` em wiring modules JSON |
| 15 | EC15b | cli_e2e.rs | bash_count + edit_count em knowledge metrics |
| 16 | EC18 | graph_service.rs + async_knowledge.rs | `access_count` em GraphFocusCtx |
| 16 | EC19a | pre_write.rs | Signal 12: co-edit neighbors |
| 16 | EC19b | cli_e2e.rs | `access_count` + `knowledge_activity` block |
| 16 | EC20 | async_knowledge.rs + graph_service.rs | `edit_count_for_file` + `edit_count` em GraphFocusCtx |
| 16 | EC21 | bridge.rs | `bash_failures` em `EnrichedCtx` |
