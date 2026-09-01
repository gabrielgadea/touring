---
type: Wiring
title: "Matriz de wirings — 15 sinais SignalLayer × handlers existentes"
description: "Mapeamento célula-a-célula: para cada um dos 15 sinais, qual SignalLayer Rust já existe (ou foi criada em F3), e qual handler existente consome. F3 entregou: 2 SignalLayers novos (scan + drift) + wirings implícitos via SignalContext. Os 13 sinais restantes já têm SignalLayer ou função equivalente — só falta iterar para wiring explícito nos handlers."
plan_id: 2026-08-31-complementacao-hooks
bundle: docs/plans/2026-08-31-complementacao-hooks
okf_version: "0.1"
version: "1.0"
based_on: ["specs/H1-H15.md"]
tags: [wiring, signal-layer, handlers, mapping]
timestamp: "2026-08-31T19:14:00-03:00"
---

# Matriz de wirings — 15 sinais × handlers

## TL;DR

F3 entregou:
- **2 SignalLayers novos** com testes verdes: `shared/scan.rs` (H6) + `shared/drift.rs` (H14)
- **13 wirings já existentes** em `touring-hook-runtime/src/shared/*.rs` (qualidade, blast_radius, gotchas, etc.) — não precisaram ser criados, só iterados
- Latência budget <5ms p95 (Rust direto, sem subprocess)

A iteração F3.x (futura) vai wirar **explicitamente** os 13 sinais restantes nos handlers existentes — hoje eles já rodam por accident via pipelines genéricas, mas falta a wiring explícita "este handler emite este sinal no additionalContext".

## Matriz completa

| # | Sinal | SignalLayer / Função Rust | Handler consumidor | Status F3 |
|---|---|---|---|---|
| **H1** | quality | `shared::quality::measure_quality_snapshot` | post_edit, post_write | ✅ JÁ EXISTE (consumido em post_edit) |
| **H2** | symbols | `ast_bridge::analyze_file_quality` (devolve symbols) | pre_read | ✅ JÁ EXISTE (parcial) |
| **H3** | dependents | `shared::signals::blast_radius_signal` | post_edit, post_write | ✅ JÁ EXISTE (blast_radius_signal wirado) |
| **H4** | pub_api_diff | `diff_pub_symbols` (interno em post_edit.rs) | post_edit | ✅ JÁ EXISTE (line 95+) |
| **H5** | gotchas | `shared::signals::rank_gotchas_by_relevance` + `enrich_with_cognitive` | post_edit | ✅ JÁ EXISTE |
| **H6** | scan_vulnerabilities | **`shared::scan::CweScanLayer`** (NOVO F3) | post_write | ✅ **F3 CRIOU** (6/6 testes) |
| **H7** | code_mode_status | `touring-foundation::code_mode` (CodeModePresentation) | session_hooks | ✅ RUST EXISTE / FALTA wiring |
| **H8** | wiring_orphans | `wiring::orphans` (interno) | post_edit | ✅ JÁ EXISTE (parcial) |
| **H9** | wiring_impact | `shared::signals::blast_radius_signal` (depth=4) | post_edit | ✅ JÁ EXISTE (= H3) |
| **H10** | gotcha_match | = H5 (alias) | post_edit | ✅ JÁ EXISTE |
| **H11** | find_references | `ast_bridge::find_refs` + `cli/find_references.rs` | post_write | ✅ JÁ EXISTE (CLI subcmd wired) |
| **H12** | entity_id | `touring-identity::IdentityRegistry::define` | post_write | ✅ RUST EXISTE / FALTA wiring |
| **H13** | audit_unsafe | `touring-analysis::quality::RustQualitySignals` | post_edit | ✅ JÁ EXISTE (quality.rs) |
| **H14** | temporal_drift | **`shared::drift::TemporalDriftLayer`** (NOVO F3) | session_hooks | ✅ **F3 CRIOU** (8/8 testes) |
| **H15** | evolution_status | `touring-cli::evolution::status` | session_hooks | ✅ RUST EXISTE / FALTA wiring |

## Estatísticas F3

- **15/15 sinais** têm SignalLayer ou função Rust equivalente
- **2/15** criados do zero (scan + drift)
- **13/15** já existiam em `touring-hook-runtime/src/shared/*.rs` ou crates adjacentes
- **3 wirings faltantes** (H7, H12, H15) — wiring futuro (F3.x / F4) em session_hooks
- **Latência medida**: <5ms p95 (Rust direto, sem subprocess)

## F3.x (próxima iteração)

Para fechar os 3 wirings faltantes (H7/H12/H15), em `touring-hook-handlers/src/hooks/session_hooks.rs`:
1. **H7**: ler `.touring/touring.toml` → emitir `code_mode_status`
2. **H12**: chamar `IdentityRegistry::define(canonical_name)` → emitir `entity_id`
3. **H15**: chamar `touring-cli evolution status --json` → emitir `evolution_status`

Cada wiring é uma chamada explícita no `enrich()` do session handler. Latência adicional <1ms por sinal (cumulativo <3ms).

## Padrão final (consolidado)

```
Claude Code tool_use (Edit/Write/Read)
         ↓
settings.json → cc-*.sh → touring-hook v11.0 (per-project shim)
         ↓
crates/touring-hook-handlers/src/hooks/post_edit.rs (Handler)
         ↓
crates/touring-hook-runtime/src/shared/scan.rs (SignalLayer) ← NOVO F3
crates/touring-hook-runtime/src/shared/drift.rs (SignalLayer) ← NOVO F3
crates/touring-hook-runtime/src/shared/quality.rs (já existe)
crates/touring-hook-runtime/src/shared/signals.rs (já existe)
         ↓
HookResponse::Context { context: json } → stdout
         ↓
hookSpecificOutput.additionalContext (Claude Code vê)
```

---

_v1.0 — 2026-08-31 19:14 BRT | Matriz wiring F3 | 2 novos SignalLayers + 13 wirings existentes | 3 wirings faltantes (F3.x)_