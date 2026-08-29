#!/bin/bash
# audit-plan-completion.sh — verificação determinística de que os entregáveis
# M0-M5 da estratégia paralelização-agentes (29/08/2026, aprovada por Gabriel)
# existem NO DISCO — spec, library, espelho de regra e contrato de KPI.
# Consumido pela cláusula cross_audit do loop_converged.py (exit 0 = completo).
set -euo pipefail
R=/home/gabrielgadea/projects/touring
H="$HOME"

fail() { echo "FALTA: $1"; exit 1; }
ok() { echo "  ok: $1"; }

# M0 — instrumentar antes de esperar adoção
grep -q 'SDK_HOOK_ALIASES' "$R/crates/touring-foundation/src/orchestrate_allowlist.rs" || fail "aliases na foundation"
ok "M0 aliases (foundation)"
grep -q '":par" if _par else' "$R/crates/touring-server/src/cli/run.rs" || fail ":par no prelude python"
ok "M0 :par (prelude py)"
grep -q 'code_mode_parallel_runs' "$R/docs/kpi/commitments.yaml" || fail "KPI parallel_runs no contrato"
grep -q 'adw_tiered_agent_share' "$R/docs/kpi/commitments.yaml" || fail "KPI tiered_agent_share no contrato"
ok "M0 KPIs (commitments.yaml)"
grep -q '_agent_journal_fields' "$H/.claude/skills/Touring/scripts/adw.py" || fail "tier/skill no journal do adw.py"
ok "M0 journal tier (adw.py)"

# M1 — painel cego no cross-audit (spec E library)
grep -q 'critic-panel' "$R/.touring/adw/cross-audit.toml" || fail "critic-panel no spec cross-audit"
grep -q 'critic-panel' "$H/.claude/skills/Touring/adw-library/cross-audit.toml" || fail "critic-panel na library"
ok "M1 critic-panel (spec + library)"

# M2 — diamante ground no strategy-loop (spec E library)
grep -q 'type = "parallel"' "$R/.touring/adw/strategy-loop.toml" || fail "diamante no spec strategy-loop"
grep -q 'type = "parallel"' "$H/.claude/skills/Touring/adw-library/strategy-loop.toml" || fail "diamante na library"
ok "M2 diamante ground (spec + library)"

# M3 — reflexo de delegação (executor + regra)
grep -q 'M3 delegação' "$R/crates/touring-cli/src/cli_suggester.rs" || fail "advisory M3 no suggester"
grep -q 'M3 delegação' "$H/.claude/rules/touring-decision-matrix.md" || fail "regra M3 na decision matrix"
ok "M3 delegação (executor + regra)"

# M4 — doutrina do custo
grep -q 'Quando paralelizar (e quando não)' "$R/docs/explanation/adw-flow-portfolio.md" || fail "doutrina M4"
ok "M4 doutrina (adw-flow-portfolio.md)"

# M5 — quadro fase→agente
grep -q 'M5 — Quadro de especialização' "$R"/docs/plans/2026-08-29-paralelizacao-agentes/strategy-*.md || fail "quadro M5"
ok "M5 quadro fase→agente (strategy doc)"

echo "audit-plan-completion: M0-M5 presentes em disco"
