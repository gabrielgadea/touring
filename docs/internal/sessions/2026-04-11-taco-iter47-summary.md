# TACO Iter47 — EC63: 4 anotações vestigiais em 4 arquivos + wiring de 3 constantes

**Data**: 2026-04-11
**Iteração**: 47
**EC implementado**: EC63
**Arquivos modificados**:
- `crates/touring-cortex/src/context.rs`
- `crates/touring-learning/src/data/errors.rs`
- `crates/touring-learning/src/data/checkpoint.rs`
- `crates/touring-cortex/src/handlers/test_generation.rs`
**Resultado**: 0 erros cargo check, 0 warnings, 720 tests (touring-cortex) + 791 tests (touring-learning), sem regressão

---

## EC63 — 4 clusters em 4 arquivos

### 1. `context.rs` — `CortexContext::from_input` (linha 60)

**Status antes**: `#[allow(dead_code)]` com doc "Used in tests as a convenience constructor"

**Análise VP-Scout**: A função tem 13 callers de produção em `pipeline.rs` (linhas 536, 657, 691, 729, 861, 877, 927, 967, 999, 1220, 1252, 1307, 1343) e mais callers em `cross_audit.rs`. O comment era enganoso — não é apenas "test helper". Anotação 100% vestigial.

**Ação EC63**: Removida anotação + doc atualizado para refletir realidade.

### 2. `errors.rs` — `TelemetryError` (linha 16)

**Status antes**: `#[allow(dead_code)]` em todo o enum

**Análise VP-Scout**: `TelemetryError` é:
- Re-exportado em `data/mod.rs`: `pub use telemetry::{TelemetryError, ...}`
- Usado como tipo `Result<T>` em `telemetry.rs`
- Construído diretamente: `TelemetryError::NotFound(...)`, `TelemetryError::Io`

Anotação completamente vestigial.

**Ação EC63**: Removida anotação.

### 3. `checkpoint.rs` — `CheckpointData` enum (linha 35)

**Status antes**: `#[allow(dead_code)]` com doc "All fields are populated via serde deserialization even when not read directly by code"

**Análise VP-Scout**: `CheckpointData` é:
- Construído via serde: `let data: CheckpointData = serde_json::from_str(&data)?;`
- Passado para `build_graph`: `fn build_graph(&self, name: &str, data: CheckpointData)`
- Todos os 5 variants são matchados em `build_graph` (`AcoEvolution`, `SessionSummary`, `Pln2Phase`, `DspyComplete`, `Generic`)

O comment antigo estava correto sobre serde, mas incompleto — o enum TAMBÉM é matchado em código de produção. Anotação vestigial.

**Ação EC63**: Removida anotação + doc atualizado para documentar tanto o papel de serde quanto o de `build_graph`.

### 4. `test_generation.rs` — 3 constantes: `MIN_BUDGET`, `MIN_CONFIDENCE`, `MCTS_ROLLOUTS` (linhas 38, 42, 46)

**Status antes**: 3 constantes com `#[allow(dead_code)]`, nunca usadas. Comentário: "These will be used when H102 integrates with the full MCTS pipeline."

**Análise VP-Scout**: As constantes têm semântica clara e pontos naturais de uso em `execute()`:

| Constante | Valor | Ponto de uso natural |
|-----------|-------|---------------------|
| `MIN_BUDGET` | `50` | Guard no início de `execute()` — budget check |
| `MCTS_ROLLOUTS` | `24` | Substituir `8` hardcoded em `cases.truncate()` |
| `MIN_CONFIDENCE` | `0.60` | Gate no output — skip se confidence baixa |

**Mudanças em `generate_test_cases()`**:
```rust
// Antes (hardcoded):
cases.truncate(8);

// Depois (EC63):
cases.truncate(MCTS_ROLLOUTS); // EC63: Keep top N test cases (MCTS_ROLLOUTS budget)
```

**Mudanças em `execute()`**:
```rust
fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
    // EC63: Guard — skip if context budget is too low to inject test suggestions.
    if ctx.context_budget_remaining < MIN_BUDGET {
        return HandlerResult::skip(self.name());
    }

    // ... extract_target ...

    // Generate test cases (capped at MCTS_ROLLOUTS by generate_test_cases)
    let test_cases = self.generate_test_cases(symbol);

    // Calculate confidence: fraction of MCTS_ROLLOUTS budget utilised.
    let confidence = (test_cases.len() as f64 / MCTS_ROLLOUTS as f64).min(1.0);

    // EC63: Guard — skip if confidence is below threshold (too few test cases generated).
    if confidence < MIN_CONFIDENCE {
        return HandlerResult::skip(self.name());
    }

    // Build test suite
    let test_suite = self.build_test_suite(symbol, &test_cases);
    // ...
```

**Impacto comportamental**:
- `MIN_BUDGET`: Handler H102 não poluirá contextos já quase esgotados
- `MCTS_ROLLOUTS`: Aumenta cap de 8 → 24 casos, permitindo cobertura mais rica
- `MIN_CONFIDENCE`: Garante que só contextos com ≥ 60% do budget de rollouts preenchidos são injetados

---

## Validação

```
cargo check --workspace     → Finished (0 errors, 0 warnings)
cargo test -p touring-cortex --lib  → 720 passed, 0 failed, 0 ignored
cargo test -p touring-learning --lib → 791 passed, 0 failed, 0 ignored
```
