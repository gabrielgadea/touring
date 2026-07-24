# Second Synergy Wave — 4 Cross-Subsystem Integrations

**Date**: 2026-04-25 | **Session**: TACO L4+ | **Skill**: Touring v4.14.0

## Objetivo

Potencializar 4 gaps identificados por VP-Scout após a primeira Synergy Wave (v4.12.0).
Todos os building blocks já existiam; apenas as conexões faltavam.

## Sumário Executivo

| ID | Synergy | Files Modified | Tests Added |
|----|---------|---------------|-------------|
| N2 | Wiring Audit + RFC-100 Diagnostics | `cli/wiring.rs` | 2 |
| N5 | Session Summary + Health Delta | `cli_handlers.rs` | 1 |
| N6 | Instructions Loaded + Cognitive Metrics | `instructions_loaded.rs` | 3 |
| N7 | Wiring Status + HyperGraph Orphan Fns | `cli_handlers.rs` | 1 |
| **TOTAL** | | **3 files** | **7 tests** |

## Resultados FASE 6

- `cargo check --workspace`: EXIT:0
- `touring-hooks` --lib: **3217 PASS, 0 failed**
- `touring-server` --lib + integration: **408+2=410 PASS, 0 failed**
- Total: **3627 PASS, 0 failed**
- Orphan baseline: **9106** (preservado — zero novos orphans)
- Hook Registry: **172** (sem novos handlers — wiring in-process)

## Detalhes por Synergy

### N2 — Wiring Audit inclui RFC-100 Diagnostics

**Arquivo**: `crates/touring-server/src/cli/wiring.rs`
**Função**: `run_audit()` (linha 94)

Gap: `touring wiring audit` chamava `cli-wiring-orphans` com `json!({})` — sem
`"diagnostics": true`. RFC-100 W-100/W-103 diagnostics nunca apareciam no output de auditoria.

Fix:
```rust
// Antes:
let orphans_raw = daemon_query("cli-wiring-orphans", serde_json::json!({}))?;

// Depois:
let orphans_raw = daemon_query("cli-wiring-orphans", serde_json::json!({"diagnostics": true}))?;
```

Extraído do resultado e incluído em `rfc100_diagnostics: {count, findings}` no JSON de audit.

**Por que**: `touring wiring audit` é o comando de auditoria completa. Sem `diagnostics:true`,
os W-100/W-103 structured findings nunca chegavam ao output — tornando RFC-100 invisível
no workflow de auditoria principal.

### N5 — Session Summary + Health Delta

**Arquivo**: `crates/touring-hooks/src/cli_handlers.rs`
**Função**: `cli_session_summary()` (linha 5774)

Gap: `cli_session_summary` retornava apenas `{file_path, summaries, count}` — sem
informação de health_delta. O sistema de regression streak (W13, W15) existia mas
não era surfaceado no contexto de sessão por arquivo.

Fix:
```rust
let health_delta_str = crate::health_delta::status_json(Some(file_path));
let health_delta: serde_json::Value =
    serde_json::from_str(&health_delta_str).unwrap_or(serde_json::Value::Null);
serde_json::json!({
    "file_path": file_path,
    "summaries": summaries,
    "count": count,
    "health_delta": health_delta,  // novo campo
})
```

**Por que**: `health_delta::status_json` existia desde Wave 15/16 mas nunca era
consultado no contexto de sessão por arquivo — padrão "building block órfão".

### N6 — Instructions Loaded + Cognitive Metrics

**Arquivo**: `crates/touring-hooks/src/instructions_loaded.rs`
**Função**: `push_cognitive_parts()` (linha 138)

Gap: `push_cognitive_parts` chamava `enrich_with_cognitive()` (file risk + bash failures)
mas nunca surfaceava o estado do cognitive runtime em si. LLM não sabia se o
enrichment estava ativo ou em cold-start.

Fix:
```rust
let predictor_state = if runtime.learning.predictor.is_some() { "ready" } else { "inactive" };
parts.push(format!("cognitive=active, predictor={predictor_state}"));
```

**Por que**: O cognitive graph é inicializado em 5/5 daemon components, mas o LLM nunca
recebia confirmação disso no additionalContext da sessão — informação que afeta
confiança nas sugestões de routing e enrichment.

### N7 — Wiring Status conecta HyperGraph Orphan Pub Fns

**Arquivo**: `crates/touring-hooks/src/cli_handlers.rs`
**Função**: `cli_wiring_status()` (linha 682)

Gap: `hypergraph_cycle_detection` e `build_multi_import_hypergraph` em `wiring.rs`
eram pub fns com zero callers fora do arquivo — orphan pub symbols. HyperGraph foi
introduzido em v4.9.0 mas nunca integrado ao workflow de status.

Fix:
```rust
let (hg_count, hg_labels) = crate::wiring::hypergraph_cycle_detection(db);
let (_hg, multi_imports) = crate::wiring::build_multi_import_hypergraph(db);
obj.insert("hypergraph_cycles", json!({"count": hg_count, "detail": hg_labels}));
obj.insert("multi_import_hyperedges", json!({"count": multi_imports.len()}));
```

`touring wiring status -j` agora inclui:
```json
{
  "hypergraph_cycles": {"count": N, "detail": [...]},
  "multi_import_hyperedges": {"count": M}
}
```

**Por que**: `hypergraph_cycle_detection` + `build_multi_import_hypergraph` existiam
desde v4.9.0 mas ficavam invisíveis — callers = 0. Wired em `cli_wiring_status`
sem novo daemon handler (in-process call direto ao `FileKnowledgeDB`).

## FP Evitado

- **N3 (api_cascade)**: VP-Scout Chain 3 confirmou `plan_api_cascade` já wired
  via `api_cascade_bridge.rs` + `post_edit.rs:307`. Falso positivo evitado.

## Lições Aprendidas

1. **Orphan pub fns em wiring.rs**: `hypergraph_*` fns foram entregues em v4.9.0
   mas não conectadas ao CLI — típico padrão "building block without wiring"
2. **`diagnostics: true` gap**: Chamadas ao daemon com `json!({})` perdem flags
   opcionais — sempre verificar se handlers têm parâmetros enriquecidos disponíveis
3. **Instructions_loaded granularity**: `push_cognitive_parts` tinha informação de
   file risk mas não de graph health — duas categorias distintas de cognitive context
4. **Session summary como pivot point**: `cli_session_summary` é ponto natural para
   agregar múltiplas fontes (summaries + health_delta + gotchas) — extensível

## Touring CLI Changes

Nenhuma nova CLI command adicionada. `touring wiring audit` agora inclui `rfc100_diagnostics`.
`touring wiring status` agora inclui `hypergraph_cycles` + `multi_import_hyperedges`.
`cli_session_summary` agora inclui `health_delta` por arquivo.
Session-start additionalContext agora inclui `cognitive=active, predictor=ready|inactive`.
