# TACO Iter36 — EC52: to_json_pretty + to_health_diff wired no pipeline pre→post_edit

**Data**: 2026-04-11
**Iteração**: 36
**EC implementado**: EC52
**Arquivos modificados**: `crates/touring-hooks/src/pre_edit.rs`, `crates/touring-hooks/src/post_edit.rs`
**Resultado**: 0 erros cargo check, 1452 tests passing (touring-hooks), sem regressão

---

## EC52 — `CodeHealthReport::to_json_pretty()` + `CodeHealthReport::to_health_diff()` wired no pipeline pre→post_edit

### Problema
Dois métodos em `CodeHealthReport` tinham 0 callers em produção:
- `to_json_pretty()` — serializa o report completo como JSON pretty-printed
- `to_health_diff()` — produz `HealthDiff` tipado comparando dois snapshots de saúde

O E5 block em `post_edit.rs` fazia comparação manual de saúde usando apenas o
`composite_score` como float — incapaz de identificar QUAIS dimensões degradaram
(apenas filtrava `d.score < 0.8`, sem comparação real com o valor anterior).

### Mudança

**pre_edit.rs** — após o B1-store existente, adicionada segunda entrada de cache:

```rust
// EC52: Also store full health JSON so post_edit can call to_health_diff()
let json_snap_key = format!("__pre_edit_health_json__:{}", rel_path);
runtime.ctx.result_cache.cache_result(
    "pre_edit", &json_snap_key, health.to_json_pretty(),  // EC52: first caller
);
```

**post_edit.rs** — E5 block reescrito para usar `to_health_diff()` quando JSON disponível:

```rust
// Rich path: full pre-report JSON → to_health_diff() for typed HealthDiff
let (delta, degraded_dims) = if let Some(pre) = runtime.ctx.result_cache
    .get_result("pre_edit", &json_key)
    .and_then(|json| serde_json::from_str::<touring_analysis::CodeHealthReport>(&json).ok())
{
    // EC52: First production caller of to_health_diff()
    let hd = pre.to_health_diff(post);
    let dims: Vec<String> = hd.dimensions.degraded.iter()
        .filter_map(|name| post.dimensions.iter().find(|d| &d.name == name))
        .map(|d| format!("{}:{:.2}", d.name, d.score))
        .collect();
    (hd.score_delta, dims)
} else {
    // Fallback: float-only path (JSON cache cold on first run)
    ...
};
```

### Design decisions
- **Backward compat**: fallback ao float-only quando JSON cache não disponível (primeira
  execução após deploy) — zero regressão para sessões existentes
- **Rich path**: `to_health_diff()` usa a comparação real pre→post para identificar
  dimensões que DEGRADARAM (não apenas dimensões abaixo de 0.8)
- **Cache key**: `__pre_edit_health_json__:{rel_path}` — paralela à chave float existente
- `serde_json::from_str::<CodeHealthReport>` — desserialização segura (CodeHealthReport: Deserialize)

### Impacto
- `to_json_pretty()` tem agora **1 caller real em produção** (era 0) — via `pre_edit.rs`
- `to_health_diff()` tem agora **1 caller real em produção** (era 0) — via `post_edit.rs`
- O E5 regression detector passa a usar diff tipado (dimensões realmente degradadas)
  em vez de heurística de limiar (score < 0.8) — mais preciso, menos falsos positivos

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed, 1 ignored
```
