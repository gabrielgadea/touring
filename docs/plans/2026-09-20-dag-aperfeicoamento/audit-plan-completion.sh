#!/bin/bash
# audit-plan-completion.sh — verificação determinística de que os entregáveis
# das duas rodadas do DAG (19-20/09/2026) existem NO DISCO, não na narrativa.
#
# Consumido pela cláusula `cross_audit` do loop_converged.py (exit 0 = completo).
# Essa cláusula respondia N/A em toda rodada porque nenhum bundle trazia este
# script — uma das oito cláusulas do juiz nunca era avaliada. Este arquivo é a
# correção: a partir daqui o veredito cobre também os ARTEFATOS, não só os gates.
set -euo pipefail
R=/home/gabrielgadea/projects/touring
H="$HOME"

fail() { echo "FALTA: $1"; exit 1; }
ok() { echo "  ok: $1"; }

# ── Rodada 1 (19/09) — os quatro defeitos de "ponta que não se encontra" ──────

# D1: o finalize arquiva e DECLARA, e os terminais vêm de uma lista só
grep -q 'archived_at = COALESCE(archived_at, ?2)' \
  "$R/crates/touring-cli/src/cli/handlers/decompose.rs" || fail "finalize não carimba archived_at"
grep -q '"archived": archived' \
  "$R/crates/touring-cli/src/cli/handlers/decompose.rs" || fail "finalize não declara archived"
ok "D1 finalize arquiva e declara"
grep -q 'pub const TERMINAL_TASK_STATUSES' \
  "$R/crates/touring-foundation/src/task_lifecycle.rs" || fail "lista terminal única"
grep -q 'terminal_status_sql_list' \
  "$R/crates/touring-server-reasoning/src/reasoning/persistence.rs" || fail "retenção usa a lista única"
ok "D1 fonte única dos estados terminais"

# D2: a retenção tem rota
grep -q 'pub fn cli_decompose_archive' \
  "$R/crates/touring-cli/src/cli/handlers/decompose.rs" || fail "handler archive"
grep -q '"cli-decompose-archive"' \
  "$R/crates/touring-dispatch/src/hook_registry.rs" || fail "archive no registry"
grep -q 'Archive {' "$R/crates/touring-server/src/cli/decompose.rs" || fail "subcomando archive"
ok "D2 retenção alcançável (handler + registry + CLI)"

# D3: o fechador deriva da MESMA lista do scaffolder
grep -q 'pub const MIRROR_SCAFFOLD_STAGES' \
  "$R/crates/touring-foundation/src/task_lifecycle.rs" || fail "lista de estágios"
grep -q 'pub fn close_scaffold_stages' \
  "$R/crates/touring-hook-handlers/src/hook_decompose_bridge.rs" || fail "fechador"
grep -q 'close_scaffold_stages(rt, task_id, success)' \
  "$R/crates/touring-dispatch/src/hook_registry.rs" || fail "hook chama o fechador"
grep -q 'for stage in MIRROR_SCAFFOLD_STAGES' \
  "$R/crates/touring-hook-handlers/src/hook_decompose_bridge.rs" || fail "scaffolder itera a lista"
ok "D3 scaffolder e fechador na mesma lista"

# D4: o scout não relê o próprio rastro (duas camadas)
grep -q 'pub fn is_scout_ticket' \
  "$R/crates/touring-foundation/src/task_lifecycle.rs" || fail "predicado do ticket"
grep -q 'is_scout_ticket' \
  "$R/crates/touring-hooks-prediction/src/tfidf_retriever.rs" || fail "corpus filtra o ticket"
grep -q 'def is_own_echo' \
  "$H/.claude/skills/Touring/scripts/scout_perpetuo.py" || fail "contador desconta o eco"
grep -q 'own_echoes' \
  "$H/.claude/skills/Touring/scripts/scout_perpetuo.py" || fail "eco é reportado"
ok "D4 laço do scout cortado nas duas camadas"

# P1/P2: as duas pendências reais
grep -q 'fn wait_for_generation_seal' \
  "$R/crates/touring-server/src/cli/index.rs" || fail "--wait do rebuild"
grep -q 'fn generation_is_building' \
  "$R/crates/touring-server/src/cli/index.rs" || fail "predicado puro do --wait"
ok "P1 index rebuild --wait"
grep -q 'pub fn ladder_totals' \
  "$R/crates/touring-intelligence/src/rl/memory/snippet_stats.rs" || fail "agregado da escada"
grep -q 'ladder_enrolled' "$R/crates/touring-cli/src/cli/kpi.rs" || fail "KPI separa enrolado"
grep -q 'ladder.reused' "$R/crates/touring-cli/src/cli/kpi.rs" || fail "numerador usa re-execução"
ok "P2 régua do reuse lê a escada e separa enrolar de reusar"

# ── Rodada 2 (20/09) — o aperfeiçoamento ─────────────────────────────────────

grep -q 'get("review_required")' \
  "$R/crates/touring-cli/src/cli/decompose.rs" || fail "add lê review_required do payload"
ok "F2 review_required tem rota"
grep -q 'pub use crate::cli::decompose::cli_decompose_add' \
  "$R/crates/touring-cli/src/cli/handlers/decompose.rs" || fail "duplicata virou re-export"
ok "F2 uma só implementação de cli_decompose_add"
grep -q 'pub fn cli_decompose_reconcile_stages' \
  "$R/crates/touring-cli/src/cli/handlers/decompose.rs" || fail "handler reconcile"
grep -q '"cli-decompose-reconcile-stages"' \
  "$R/crates/touring-dispatch/src/hook_registry.rs" || fail "reconcile no registry"
grep -q 'ReconcileStages {' \
  "$R/crates/touring-server/src/cli/decompose.rs" || fail "subcomando reconcile"
ok "F3 reconciliação de estágios alcançável"
grep -q 'cli-decompose-update' \
  "$R/crates/touring-dispatch/src/daemon_yield_tests.rs" || fail "decisão do heavy registrada"
ok "F4 decisão sobre heavy documentada em teste"

# ── Os testes que provam tudo isso existem ───────────────────────────────────
test -f "$R/crates/touring-hooks/tests/dag_lifecycle_closure_e2e.rs" || fail "e2e do ciclo de vida"
grep -q 'fn reconcile_refuses_to_guess_while_a_sibling_is_still_open' \
  "$R/crates/touring-hooks/tests/dag_lifecycle_closure_e2e.rs" || fail "controle negativo do reconcile"
grep -q 'fn the_corpus_excludes_the_scouts_own_tickets' \
  "$R/crates/touring-hooks-prediction/src/tfidf_retriever.rs" || fail "teste do corpus"
grep -q 'fn enrolling_a_body_is_not_reusing_it' \
  "$R/crates/touring-cli/src/cli/kpi.rs" || fail "teste que enrolar != reusar"
ok "testes de prova presentes"

# ── A narrativa em disco ─────────────────────────────────────────────────────
test -f "$R/docs/audits/dag-mentia-sobre-si-2026-09-19.md" || fail "relatório da rodada 1"
ok "narrativa registrada"

echo "COMPLETO: entregáveis das duas rodadas verificados no disco"
