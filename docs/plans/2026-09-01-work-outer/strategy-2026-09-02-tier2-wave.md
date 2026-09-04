---
type: Strategy
title: "Wave TIER-2 do signal layer: F10 · F9 · S6 · S8 · S9 · S4"
description: "Estratégia da wave ordenada por Gabriel em 02/09/2026: post-bash em PostToolUseFailure, numerador do hooks_complement por origem, qualidade do conteúdo proposto no pre_edit, CweScanLayer wired com detector único, badge semântico .rs e cascata de API antes do edit. DAG task_1788318067626657742."
tags: [strategy, signal-layer, hooks, tier-2, tdd, kpi]
timestamp: 2026-09-02T00:05:00-03:00
plan_id: 2026-08-31-complementacao-hooks
---

# Wave TIER-2 — sinais ativos (fase 2) + fechamento do F0.3

Bundle canônico: `../2026-08-31-complementacao-hooks/` (strategy sinais-ativos §TIER-2). DAG: `task_1788318067626657742`
(6 subtasks; S4 depende de S9). Lei L2: `loop_converged.py --task task_1788318067626657742 --scope .` exit 0 é o único "pronto".

## OUTER (evidência)

- `strategy-loop` rodou (recall ‖ diagnose → explore 2 rodadas dry). Ledger `.touring-explore/tier-2-signal-layer-*.ledger.json`.
- Terreno verificado por grep/leitura (não inferido):
  - `post_bash.rs` lê `validated.stdout` do `tool_response`; o próprio comentário do handler registra que `PostToolUse` só dispara em sucesso. `PostToolUseFailure` entrega `error: "Exit code N\n…"`, `is_interrupt`, `duration_ms` (docs Claude Code) — hoje sem registro no `settings.json`.
  - `HookCallEntry {ts, hook_name, duration_ms, success}` sem origem; escritores: `sdk_signal_mirror::record` (via `post_bash`) e o template Python do `--orchestrate` (`run.rs:303`). `hooks_complement_from` (`kpi.rs:556`) divide TODAS as entregas por `post-bash` despachado.
  - `pre_write::quality_baseline_signals` (privado) usa `ast_bridge::analyze_file_quality`; `pre_edit` só tem `compose_quality_evolution` sobre o arquivo ATUAL em disco.
  - `CweScanLayer` (`touring-hook-runtime/src/shared/scan.rs:207`) implementa `SignalLayer` e **não está em nenhum pipeline** (órfão, REGRA #0); `touring-server/src/cli/scan.rs:54 detect_cwes_in_source` "espelha" `detect_cwes` (duplicado).
  - `RustSemanticReport::semantic_complexity()` (`rust_semantic.rs:164`), `public_api_surface()` (:211), `diff_api_surfaces` (:512); `plan_api_cascade` (`api_cascade.rs:194`) hoje só no caminho pós-edit (`api_cascade_bridge::analyze_rust_edit`, cache por path).

## Fases (INNER, cada uma sob TDD red→green→revert-proof→green)

| # | Fase | Desenho | Arquivos | Prova |
|---|---|---|---|---|
| F10 | `post-bash` em `PostToolUseFailure` | handler aceita payload de falha: `error` vira saída, exit code extraído de `Exit code N`; `success=false` alimenta nota de falha, Pensieve e sinal temporal. Registro novo no `settings.json` (ordem explícita de Gabriel) | `post_bash.rs`, schema do payload, `settings.json` | teste com payload `PostToolUseFailure`; trace vivo de um Bash que falha |
| F9 | numerador por origem | `HookCallEntry.origin: Option<String>` (`serde(default)`, legado = `null`), `MirrorOrigin {PostBash, Sdk}`; `record(..., origin)`; template Python grava `"origin":"sdk"`; KPI expõe `mirror_deliveries_by_origin` e `post_bash_delivery_ratio` = só `post_bash` ÷ despachos | `sdk_signal_mirror.rs`, `sdk.rs`, `post_bash.rs`, `run.rs`, `kpi.rs` | testes de serde legado, de `record`, de ratio com mistura de origens; template renderizado contém `origin` |
| S6 | qualidade do conteúdo proposto no `pre_edit` | `quality_baseline_signals` → `shared/quality_signal.rs`; `QualityBaselineLayer::for_edit(rel, source, old, new)` aplica o edit em memória e mede; `pre_write` passa a usar o compartilhado | `pre_write.rs`, `pre_edit.rs`, novo shared | edit que leva uma função a CC>10 produz `quality: CC>10` |
| S8 | `CweScanLayer` wired + detector único | `detect_cwes` + tipos movem para `touring-code` (leaf); hook-runtime e `touring-server/cli/scan.rs` delegam; layer entra em `pre_write` (conteúdo) e `pre_edit` (texto proposto, linhas relativas) | `scan.rs` ×2, `touring-code`, pipelines | segredo/`unwrap` no `new_string` gera `[CWE-…]`; `detect_cwes_in_source` == `detect_cwes` em fixtures |
| S9 | badge semântico `.rs` | `RustPreview::{for_write,for_edit}` parse syn uma vez (antes/depois), guard 100KB; `RustSemanticBadgeLayer` emite `[rust] semantic 0.31 · pub API 7 (+1)` | novo `shared/rust_preview.rs`, pipelines | Write `.rs` gera badge; Edit que adiciona `pub fn` mostra `+1` |
| S4 | cascata de API antes do edit ★ | `ApiCascadePreviewLayer::for_edit(preview, refs)`: `diff_api_surfaces` → `plan_api_cascade(changes, graph do after)` + `find_references` cross-file; CILA ≥ 2; `[cascade] pub fn f(...) muda — N call sites: a.rs:12 …` | novo `shared/api_cascade_preview.rs`, `pre_edit.rs` | edit só na assinatura (A2 não dispara) com call sites no índice → sinal `[cascade]` |

## Gates por fase

`cargo test -p <crate> --features pre-hooks,post-hooks` (handlers) · `cargo clippy -p <crate> --all-targets -- -D warnings` ·
`loop_phase_close.py --task task_1788318067626657742 --phase <F>` · ao fim: `cargo check --workspace`, `touring e2e -j`,
`loop_converged.py`. Deploy só com `scripts/propagate-release.sh` (nova versão) — decisão de Gabriel ao fim da wave.

## Riscos

- F10: schema de validação pode rejeitar payload sem `tool_response` → o teste vermelho é exatamente esse caso.
- S8: mover `detect_cwes` muda dois crates; a prova de igualdade em fixtures cobre a regressão.
- S9/S4: syn duplo por edit `.rs` — guard 100KB + CILA; medir `elapsed_ms` do `pre-edit` no trace após a wave.
