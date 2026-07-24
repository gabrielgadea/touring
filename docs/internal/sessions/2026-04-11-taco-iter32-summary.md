# TACO Iter32 — EC47+EC48: with_orphan_count + to_json_line wired

**Data**: 2026-04-11
**Iteração**: 32
**ECs implementados**: EC47 + EC48
**Arquivos modificados**: `pre_edit.rs`, `post_edit.rs`
**Resultado**: 0 erros cargo check, 1452 tests passing (touring-hooks), sem regressão

---

## EC47 — `AnalysisInsights::with_orphan_count()` wired em `run_returning` (pre_edit.rs)

### Problema
`AnalysisInsights::with_orphan_count()` tinha 0 callers. O campo `orphan_count` em
`AnalysisInsights` era inicializado como 0 em `from_report()` e nunca atualizado.
O contexto de pre_edit sempre reportava `"orphans=0"` ao LLM mesmo com milhares de orphans.

### Mudança

```rust
// EC47: First caller of with_orphan_count() — extracts orphan_count from
// wiring dimension metrics so AnalysisInsights.to_context_string() reports
// the real orphan count instead of the default 0.
let orphan_count: usize = health.dimensions.iter()
    .find(|d| d.name == "wiring")
    .and_then(|d| d.metrics.get("orphan_count"))
    .and_then(|v| v.as_u64())
    .unwrap_or(0) as usize;
base.with_quality_trend(&trend).with_orphan_count(orphan_count)
```

**Impacto**: LLM recebe `"orphans=N"` real no contexto de pre_edit (não mais 0).

---

## EC48 — `MetricsDashboard::to_json_line()` wired em `post_edit` (post_edit.rs)

### Problema
`MetricsDashboard::to_json_line()` tinha 0 callers. Docstring: "Suitable for log
ingestion, streaming pipelines, and time-series storage." Mas nunca emitido.

### Mudança

```rust
let dashboard = health.to_dashboard();
// EC48: First caller of to_json_line() — emits the dashboard as NDJSON for
// log ingestion and time-series storage (per its docstring purpose).
tracing::debug!(target: "touring_metrics", dashboard = %dashboard.to_json_line(), "post_edit health dashboard");
let alerts = dashboard.alerts_below(0.8);
```

**Integração**: Usa `tracing::debug!` com `target: "touring_metrics"` — permite filtrar
por target nos logs de produção. NDJSON compatível com qualquer log ingestion pipeline.

**Impacto**: `to_json_line()` tem agora 1 caller real. Dashboard emitido como NDJSON
estruturado a cada post_edit que analisa saúde de código.

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed, 1 ignored
```
