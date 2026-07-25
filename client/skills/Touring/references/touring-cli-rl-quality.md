# Touring CLI — RL & Quality / Observability

> **Module**: 5/7 | **Version**: v4.9 | **Touring**: v30.3.0
> **Series**: Touring CLI Reference (consulta sob demanda) — `~/.claude/skills/Touring/references/touring-cli-*.md`
> **Index** (auto-load): `~/.claude/rules/touring-cli-index.md` (CLI RANKS Tier 1, 2, 6)

RL feedback loop (suggest, shadow, mcts, learning), quality observability (evolution, gotcha, flywheel, incremental, e2e), gate metrics + Predictive Wave Counters, rkyv zero-copy IPC.

---

## 6. MCTS (Monte Carlo Tree Search)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring mcts search [root_state]` | `cli-mcts-search` | Busca MCTS multi-caminho |

## 7. Shadow / Speculative

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring shadow validate` | `cli-shadow-validate` | Validação especulativa |

## 8. Suggest (RL-guided)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring suggest next [query]` | `cli-suggest-next` | Próxima ação recomendada |
| `touring suggest skill [query]` | `cli-suggest-skill` | Recomendação de skill |

## 9. Learning / RL (LinUCB + Bandit)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring learning status` | `cli-learning-status` | Status do RL engine |
| `touring learning reward <tool> <val> [context]` | `cli-learning-reward` | Injeta reward signal manual (RL feedback) |

## 12. Flywheel (Component Health)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring flywheel status` | `cli-flywheel-status` | Status de componentes |

## 13. Incremental (Parser Cache)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring incremental status` | `cli-incremental-status` | Cache hit rate do parser |

## 14. Gotcha (Pitfall Database)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring gotcha list [--file F]` | `cli-gotcha-list` | Lista de pitfalls conhecidos |
| `touring gotcha add <pattern> <description> [--severity S]` | `cli-gotcha-add` | Adiciona pitfall (severity: low/medium/high) |
| `touring gotcha match <file_path>` | `cli-gotcha-match` | Encontra gotchas para um arquivo |
| `touring gotcha stats` | `cli-gotcha-stats` | Estatísticas de gotchas (total, resolved, unresolved) |

## 16. Evolution (Drift + Insights)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring evolution drift` | `cli-evolution-drift` | Detecção de drift + self-correction (PLN2 P4.3) |
| `touring evolution insights` | `cli-evolution-insights` | Insights de padrões |
| `touring evolution tools` | `cli-evolution-tools` | Efetividade de ferramentas |

**P4.3 Output Schema** (`touring evolution drift -j`):
```json
{
  "detected": bool,
  "alert_level": "none|degraded|structural",
  "self_correction_applied": bool,
  "degrading_metrics": [...],
  "summary": { "bash_success_rate", "edit_trend_pct", ... }
}
```
- `degraded`: `inject_reward("evolution:drift_detected", severity)`
- `structural`: `tracing::warn!` + RL injection + 3+ metrics degrading

## 17. E2E Analysis (v3.2)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring e2e [--depth quick\|standard\|deep] [-j]` | `cli-e2e` | Análise E2E completa: index+AST+wiring+quality+knowledge+learning+evolution+memory. Retorna score composto 0–1 com status por fase. |

**Depth levels**: `quick` (~50ms, index+wiring only), `standard` (~500ms, +AST+quality+learning), `deep` (~2s, todos os arquivos + evolution + memory).

## 17b. L7-B Gate Metrics (v3.5 — 2026-04-10)

Observability para a enrichment gate CILA-adaptativa (pre_edit/pre_write/post_tool_use).
Lock-free `AtomicU64` counters no `touring-hooks::shared::gate_metrics` module.

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring gate-metrics [-j]` | `cli-gate-metrics` | Snapshot dos contadores do enrichment gate: `pre_edit_fast_path`, `pre_edit_full`, `pre_edit_fast_ratio`, `pre_write_fast_path`, `pre_write_full`, `pre_write_fast_ratio`, `post_tool_l4_mandatory`, `total_invocations`, **rkyv** (`rkyv_dispatch_count`, `rkyv_dispatch_bytes`, `rkyv_mean_bytes`, `rkyv_parse_error_count`, `rkyv_response_count` — Wave 3 D1) |

**Uso**: `touring gate-metrics` (human) ou `touring status -j \| jq .gate_metrics` (aggregado no status dashboard).

**rkyv counters interpretation** (Wave 3, 2026-04-14):
- `rkyv_dispatch_count` — successful inbound rkyv parses (after `check_archived_root`)
- `rkyv_dispatch_bytes` — cumulative body bytes; `rkyv_mean_bytes = bytes/count`
- `rkyv_parse_error_count` — frames rejected (magic/truncated/length/bytecheck). **0** under steady load
- `rkyv_response_count` — outbound rkyv responses emitted (matches dispatch_count 1:1 ideally)

**RFC-100 Diagnostic Counters** — emitted by structured RFC-100 sites:

| Counter | Subsystem | Wave | Semantics |
|---------|-----------|------|-----------|
| `diagnostic_wiring_finding_emitted_count` | wiring | v4.12 S4 | W-100/W-103 emitted by `cli_wiring_orphans` |
| `diagnostic_tdg_emitted_count` | quality | v4.12 S4 | Q-201/Q-202 emitted by `cli_ast_tdg` (D/F grade) |
| `diagnostic_b302_emitted_count` | blast | **v4.24 W12** | **B-302 PatchExpansion emitted by `pre_write::emit_b302_if_low_confidence_expansion` (typically called from `cli_mpatch_preview` when mpatch fuzzy expand + confidence < 0.7)** |

**Helpers** (record from any emission site):
```rust
use touring_hooks::shared::gate_metrics::{
    record_diagnostic_wiring_finding_emitted,
    record_diagnostic_tdg_emitted,
    record_diagnostic_b302_emitted,    // Wave 12
};
```

**Cheatsheet — RFC-100 prevalence**:
```bash
touring gate-metrics -j | jq '{
  wiring_findings: .diagnostic_wiring_finding_emitted_count,
  tdg_grade_d_or_f: .diagnostic_tdg_emitted_count,
  b302_patch_expansion: .diagnostic_b302_emitted_count
}'
```

**Predictive Wave Counters (2026-04-20)** — 9 new AtomicU64 fields added by D5 (telemetria preditiva):

| Counter | Subsystem | Semantics |
|---------|-----------|-----------|
| `blast_inject_count` | AST Blast | Blast-radius hints successfully injected into pre_edit context |
| `blast_timeout_count` | AST Blast | Blast calls exceeding budget (P99 guard — alert if > 0) |
| `blast_mutation_count` | AST Blast | Blast results that triggered a mutation suggestion |
| `linucb_route_manual_count` | LinUCB Router | Requests routed to manual (non-generator) path |
| `linucb_route_generator_count` | LinUCB Router | Requests routed to generator pipeline |
| `linucb_route_hint_count` | LinUCB Router | Routing decisions influenced by workflow hints |
| `mcts_shadow_run_count` | MCTS Shadow | Shadow rollouts executed speculatively |
| `mcts_shadow_timeout_count` | MCTS Shadow | Shadow rollouts killed by timeout guard |
| `mcts_shadow_deadlock_detected_count` | MCTS Shadow | Deadlock patterns caught by shadow analysis |

**Cheatsheet** — observar saúde do subsistema preditivo:
```bash
# Snapshot dos 9 counters preditivos
touring gate-metrics -j | jq '{
  blast: {inject: .blast_inject_count, timeout: .blast_timeout_count, mutation: .blast_mutation_count},
  linucb: {manual: .linucb_route_manual_count, generator: .linucb_route_generator_count, hint: .linucb_route_hint_count},
  mcts: {runs: .mcts_shadow_run_count, timeout: .mcts_shadow_timeout_count, deadlock: .mcts_shadow_deadlock_detected_count}
}'

# Alert rule: qualquer *_timeout_count ou deadlock_detected_count > 0 = investigar
touring gate-metrics -j | jq 'if (.blast_timeout_count > 0 or .mcts_shadow_timeout_count > 0 or .mcts_shadow_deadlock_detected_count > 0) then "ALERT: P99 guard triggered" else "OK" end'
```

**Referência completa**: `~/projects/touring/docs/2026-04-20-predictive-wave.md`

## 17b1. rkyv IPC Runtime Switch (Wave 3, 2026-04-14) — **DEFAULT ON**

`rkyv-ipc` é DEFAULT FEATURE em `touring-hooks` + `touring-server` desde
2026-04-14. Toda build padrão já habilita o protocolo zero-copy. Env var
controla path em runtime sem rebuild.

| Var | Effect | When to use |
|---|---|---|
| `TOURING_RKYV_IPC` (unset / `1` / `true`) | rkyv path active — **default state** | Always (production) |
| `TOURING_RKYV_IPC=0` | Forces JSON path on the same binary | Hot rollback / interop with legacy daemon |
| `TOURING_RKYV_IPC=false` | Same as `0` | Same as `0` |

**Build commands**:
```bash
# Standard build — rkyv-ipc is in default features, no flag needed.
cargo build --release -p touring-server
cargo build --release -p touring-hooks   # daemon + hook binaries

# Opt-out (raro — apenas para legacy interop testing)
cargo build --release --no-default-features --features <minimal-set> -p touring-server
```

**Bypass example**: `TOURING_RKYV_IPC=0 touring index find Foo`

**Production validation playbook**: see `crates/touring-rkyv/docs/2026-04-14-rkyv-ipc-rollout.md` (or workspace `docs/`).

---

**Outros módulos**: [overview](touring-cli-overview.md) | [hooks](touring-cli-hooks.md) | [intelligence](touring-cli-intelligence.md) | [tasks](touring-cli-tasks.md) | [generate](touring-cli-generate.md) | [meta](touring-cli-meta.md)
