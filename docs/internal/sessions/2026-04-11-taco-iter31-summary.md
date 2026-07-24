# TACO Iter31 — EC47: with_orphan_count wired em pre_edit AnalysisInsights

**Data**: 2026-04-11
**Iteração**: 31
**EC implementado**: EC47
**Arquivo modificado**: `crates/touring-hooks/src/pre_edit.rs`
**Resultado**: 0 erros cargo check, 1452 tests passing (touring-hooks), sem regressão

---

## EC47 — `AnalysisInsights::with_orphan_count()` wired em `run_returning`

### Problema
`AnalysisInsights::with_orphan_count()` existia com **0 callers** fora de lib.rs (apenas
re-exportado). O método é um builder que define `orphan_count` em `AnalysisInsights` —
o campo que aparece em `to_context_string()` como `"Analysis: ... | orphans=N"`.

Em `pre_edit.rs`, `AnalysisInsights::from_report()` inicializava `orphan_count = 0`
(hardcoded no construtor). Mesmo que `health.dimensions` contivesse o `orphan_count`
real (extraído de `analyze_wiring` → `HealthDimension.metrics["orphan_count"]`),
esse valor **nunca era transferido** para `AnalysisInsights`.

Resultado: o LLM recebia `"orphans=0"` no contexto de pre_edit mesmo quando o wiring
tinha centenas de orphan pub symbols.

### Mudança

**pre_edit.rs** — bloco de construção de `AnalysisInsights` em `run_returning`:

```rust
// G3: Build AnalysisInsights enriched with quality trend from temporal DB.
let insights = {
    let base = touring_analysis::AnalysisInsights::from_report(&health);
    let trend = touring_analysis::quality_trend(conn, 5);
    // EC47: First caller of with_orphan_count() — extracts orphan_count from
    // wiring dimension metrics so AnalysisInsights.to_context_string() reports
    // the real orphan count instead of the default 0.
    let orphan_count: usize = health.dimensions.iter()
        .find(|d| d.name == "wiring")
        .and_then(|d| d.metrics.get("orphan_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    base.with_quality_trend(&trend).with_orphan_count(orphan_count)
};
```

**Design decisions**:
- Extração via `d.metrics.get("orphan_count").and_then(|v| v.as_u64())` — zero panic,
  graceful quando wiring feature não habilitada (retorna 0)
- `.unwrap_or(0) as usize` — safe cast, dimensão ausente → 0 (sem wiring data)
- Chained com `with_quality_trend` — mantém idioma builder existente
- `HealthDimension.metrics` é `serde_json::Value` — extração via `.get()` + `.as_u64()`

### Semântica do `to_context_string()` corrigida

Antes do EC47:
```
"Analysis: health=0.72 (degraded) | trend=Stable | orphans=0"
```

Após EC47 (com wiring ativo e 19451 orphans no projeto):
```
"Analysis: health=0.72 (degraded) | trend=Stable | orphans=19451"
```

O LLM recebe agora o contexto real de orphan pub symbols durante pre_edit hooks.

### Cadeia de dados

```
AnalysisPipeline::run_wiring()
  → analyze_wiring(conn)
  → WiringReport { orphan_count: N, ... }
  → HealthDimension { metrics: {"orphan_count": N, ...}, ... }

run_returning() em pre_edit:
  health.dimensions.iter().find("wiring").metrics["orphan_count"]
  → with_orphan_count(N)                                           ← EC47
  → AnalysisInsights { orphan_count: N, ... }
  → to_context_string() → "orphans=N"  ← real value
```

### Impacto
`AnalysisInsights::with_orphan_count()` tem agora **1 caller real** (era 0).
O contexto de pre_edit do LLM passa a incluir o orphan count real do projeto,
não mais o default 0. Sinal de wiring health agora fluye completamente até o LLM.

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed, 1 ignored
```
