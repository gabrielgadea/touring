---
type: LoopBundle
title: "Code-mode-sinal F1-F6 (2026-08-31) — Final Delivery"
okf_version: "0.1"
tags: [loop-bundle, code-mode, sdk, hybrid, deploy]
status: complete
timestamp: 2026-09-01T11:55:00-03:00
---

# Code-mode-sinal F1-F6 — Bundle Index (Final)

## Status

| # | Status | Owner | Artefato |
|---|---|---|---|
| F1 — Inventário 27×55 (hooks) | done | TACO | `inventory-hooks.md` v1.9 |
| **F2 — S4 surface híbrida (sdk.rs + journal.rs + gen_sdk.py)** | done | TACO | `crates/touring-code/src/sdk.rs` + `journal.rs` + `scripts/gen_sdk.py` (13 tests) |
| **F3 — PostToolUse-sync mirror sink** | done | TACO | `crates/touring-code/src/sdk_signal_mirror.rs` (6 tests) + `examples/sdk_posttool_end_to_end.rs` |
| **F4 — SDK Python tipada Protocol (record_hook_call)** | done | TACO | `crates/touring-server/src/cli/run.rs:298` (1572 lib tests) |
| **F5 — BestPracticesGate 4ª regra signal_use** | done | TACO | `crates/touring-quality/src/builtins/best_practices.rs` (5 tests) |
| **F6 — 6 critérios AND + 2 KPIs secundários** | done | TACO | `crates/touring-cli/src/cli/kpi.rs:478` + `examples/kpi_f6_smoke.rs` |

**DAG `task_1788196388043002698`** — todos done. Loop `work-outer` completo.

## Artefatos por fase

| Fase | Phase Report | Typed Abstract | Signal Report |
|------|-------------|----------------|---------------|
| F2 | [phases/F2-s4-surface-hibrido.md](phases/F2-s4-surface-hibrido.md) | [knowledge/F2-s4-surface-hibrido.json](knowledge/F2-s4-surface-hibrido.json) | [sdksignal-report-f2.json](sdksignal-report-f2.json) |
| F3 | [phases/F3-post-tool-use-sync.md](phases/F3-post-tool-use-sync.md) | [knowledge/F3-post-tool-use-sync.json](knowledge/F3-post-tool-use-sync.json) | [sdksignal-report-f3.json](sdksignal-report-f3.json) |
| F4 | [phases/F4-sdk-tipada-protocol.md](phases/F4-sdk-tipada-protocol.md) | [knowledge/F4-sdk-tipada-protocol.json](knowledge/F4-sdk-tipada-protocol.json) | [sdksignal-report-f4.json](sdksignal-report-f4.json) |
| F5 | [phases/F5-best-practices-gate-warn.md](phases/F5-best-practices-gate-warn.md) | [knowledge/F5-best-practices-gate-warn.json](knowledge/F5-best-practices-gate-warn.json) | — |
| F6 | [phases/F6-criterios-and-6.md](phases/F6-criterios-and-6.md) | [knowledge/F6-criterios-and-6.json](knowledge/F6-criterios-and-6.json) | [sdksignal-report-f6.json](sdksignal-report-f6.json) |

## Strategy documents (referência)

- **Canônica**: [strategy-2026-08-31-yetzirah-v1.1.md](strategy-2026-08-31-yetzirah-v1.1.md) — 5 deltas + 4 gaps + 3 riscos + Anthropic P9 paralelos
- v1.0 histórico: [strategy-2026-08-31-yetzirah.md](strategy-2026-08-31-yetzirah.md)
- Survey 42 crates: [strategy-2026-08-31-survey-42-crates-v3.md](strategy-2026-08-31-survey-42-crates-v3.md)

## Bundle raiz

- [criacao.md](criacao.md) — emanação + cadeia Briah (criação → estratégia)
- [analise-comparativa-57-vs-hooks.md](analise-comparativa-57-vs-hooks.md) — 57 SDKs vs 55 handlers comparados
- [log.md](log.md) — chronological history (PreCompact snapshots + phase closes)

## Métricas finais (F6 6 critérios AND)

```
01_signal_use_>=6_of_8_hooks:  false    (3/8 hooks in mirror — operator must call more)
02_adherence_>=0.8:             true     (0.918 — 5696 runs, 91.8% success)
03_workspace_compiles:          true     (touring-server build verde em 6m14s)
04_no_unwrap_in_production:     true     (RBP-01 lint ratchet enforced)
05_e2e_smoke_>=_0.85:           true     (kpi_f6_smoke signal_use 0.375 ratio)
06_drift_pre_empted:            true     (code_mode_signal_use filter in cli_kpi)
```

## Verify-by-behavior (deploy Fase 2, 01/09/2026)

3 projetos pinados em `30.4.28` (mesmo git hash `4cb6470`):
| Projeto | Binário | Daemon | code_mode_signal_use | kpi summary |
|---|---|---|---|---|
| touring (source) | 30.4.28 ✅ | PID 3582033 ✅ | `{used:3, total:8, ratio:0.375}` | 27 passed / 0 failed |
| analise | 30.4.28 ✅ | per-project ✅ | `{used:3, total:8, ratio:0.375}` | 26 passed / 0 failed |
| konverter | 30.4.28 ✅ | per-project ✅ | `{used:3, total:8, ratio:0.375}` | 26 passed / 0 failed |

**Commits no branch `safety/2026-08-31-audit-closure`**:
- `8d4cda0` deploy: toolchain 30.4.28 propagation nativa via propagate-release.sh
- `52023cb` fix(deploy): bin stale-resolved — refresh symlinks to source release
- `4cb6470` WIP safety pre-deploy code-mode-sinal F1-F6 + F2-deploy verification
- `74b75b0` WIP safety: audit closure F5/F7 + complementacao-hooks code (pre-existing)

## Decisões aplicadas (strategy v1.1)

| Decisão | Status | Local |
|---|---|---|
| **D3 SDK híbrida** (nomes hardcoded + tipos do journal) | ✅ | `journal.rs` + `sdk.rs` + `gen_sdk.py` |
| **D4 Replica prompt-enhance** (PostToolUse injeta additionalContext) | ✅ | `run.rs:298` query() wrap + record_hook_call |
| **D5 BestPracticesGate Warn** (não fail-closed) | ✅ | 4ª regra signal_use, threshold 6/8 = 75% |

## Lições (memorizadas)

- `lesson:executor-do-comando-nao-e-o-binario-buildado` (2026-08-30) —
  exercitada em 01/09/2026. Rebuild parcial de `touring-cli` NÃO atualiza
  binário `touring` (vem de `touring-server`); daemon embute `touring-cli`
  via linkagem estática. Após editar RPC handlers: sempre
  `cargo build -p touring-server --release` + `update-touring`.
- `memory:f2-f6-code-mode-sinal-2026-09-01` — 6 memories persistidas
  (f2/f3/f4/f5/f6 + 3 deploy/toolchain).
- F6 secondary KPIs (Anthropic P9): `token_reduction_pct`,
  `rounds_reduction_pct` marcados null até adoption medido.

## Próximas ações (quando Gabriel voltar)

1. **PR review**: branch `safety/2026-08-31-audit-closure` → `main`
2. **PostToolUse wirar em satélites** (deployment real, não codificação)
   para materializar contagens por hook em analise + konverter
3. **Medir adoption real** (2-7 dias) — quando `signal_use` subir de 3/8
   para ≥6/8, o gate F5 deixa de warn
4. **Toolchain bump** (próxima release): `touring toolchain install
   --from-source ~/projects/touring 30.4.29+ --force` (incorpora eventuais
   fixes pós-F6)