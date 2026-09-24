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

# `tem <padrao> <alvo...>` — procura no MÓDULO, não num arquivo.
#
# Cross-audit 21/09/2026: `decompose.rs` e `kpi.rs` foram divididos em
# submódulos, e metade destas verificações passou a FALHAR com o comportamento
# intacto — elas apontavam para um caminho, e o código tinha mudado de arquivo
# dentro do mesmo módulo. Um diretório é aceito como alvo e varrido.
#
# ⚠ LIMITE CONHECIDO, registrado no relatório da auditoria: estas 13
# verificações provam que o TEXTO existe no disco, nunca que o comportamento
# acontece. Um `grep` casa um comentário tão bem quanto o código. Substituí-las
# por verificação executável é item próprio, ainda aberto.
tem() {
  local padrao="$1"; shift
  grep -rqF -- "$padrao" "$@"
}

# Módulos que a divisão espalhou: o alvo é o conjunto, não o arquivo.
MOD_DECOMPOSE=(
  "$R/crates/touring-cli/src/cli/handlers/decompose.rs"
  "$R/crates/touring-cli/src/cli/handlers/decompose/"
)
MOD_KPI=(
  "$R/crates/touring-cli/src/cli/kpi.rs"
  "$R/crates/touring-cli/src/cli/kpi/"
)

# ── Rodada 1 (19/09) — os quatro defeitos de "ponta que não se encontra" ──────

# D1: o finalize arquiva e DECLARA, e os terminais vêm de uma lista só
tem 'archived_at = COALESCE(archived_at, ?2)' "${MOD_DECOMPOSE[@]}" || fail "finalize não carimba archived_at"
tem '"archived": archived' "${MOD_DECOMPOSE[@]}" || fail "finalize não declara archived"
ok "D1 finalize arquiva e declara"
grep -q 'pub const TERMINAL_TASK_STATUSES' \
  "$R/crates/touring-foundation/src/task_lifecycle.rs" || fail "lista terminal única"
grep -q 'terminal_status_sql_list' \
  "$R/crates/touring-server-reasoning/src/reasoning/persistence.rs" || fail "retenção usa a lista única"
ok "D1 fonte única dos estados terminais"

# D2: a retenção tem rota
tem 'pub fn cli_decompose_archive' "${MOD_DECOMPOSE[@]}" || fail "handler archive"
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
grep -q 'fn generation_state' \
  "$R/crates/touring-server/src/cli/index.rs" || fail "predicado puro do --wait"
grep -q 'enum EstadoGeracao' \
  "$R/crates/touring-server/src/cli/index.rs" || fail "--wait distingue 'nao sei' de 'selou'"
ok "P1 index rebuild --wait"
grep -q 'pub fn ladder_totals' \
  "$R/crates/touring-intelligence/src/rl/memory/snippet_stats.rs" || fail "agregado da escada"
tem 'ladder_enrolled' "${MOD_KPI[@]}" || fail "KPI separa enrolado"
tem 'ladder.reused' "${MOD_KPI[@]}" || fail "numerador usa re-execução"
# O `- 1` e o UNICO lugar onde "enrolar nao e reusar" existe como codigo, e ate
# 21/09 nada o guardava: remove-lo passava com 1911 testes verdes.
grep -q 'fn o_agregado_conta_reexecucao_e_nao_enrolamento' \
  "$R/crates/touring-intelligence/src/rl/memory/snippet_stats.rs" || fail "teste do agregado no banco"
ok "P2 régua do reuse lê a escada e separa enrolar de reusar"

# ── Rodada 2 (20/09) — o aperfeiçoamento ─────────────────────────────────────

grep -q 'get("review_required")' \
  "$R/crates/touring-cli/src/cli/decompose.rs" || fail "add lê review_required do payload"
ok "F2 review_required tem rota"
tem 'pub use crate::cli::decompose::cli_decompose_add' "${MOD_DECOMPOSE[@]}" || fail "duplicata virou re-export"
ok "F2 uma só implementação de cli_decompose_add"
tem 'pub fn cli_decompose_reconcile_stages' "${MOD_DECOMPOSE[@]}" || fail "handler reconcile"
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
tem 'fn enrolling_a_body_is_not_reusing_it' "${MOD_KPI[@]}" || fail "teste que enrolar != reusar"
ok "testes de prova presentes"

# ── A narrativa em disco ─────────────────────────────────────────────────────
test -f "$R/docs/audits/dag-mentia-sobre-si-2026-09-19.md" || fail "relatório da rodada 1"
ok "narrativa registrada"

echo "COMPLETO: entregáveis das duas rodadas verificados no disco"
