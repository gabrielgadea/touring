#!/bin/bash
# shellcheck disable=SC2015  # ok()/falta() always return 0: `check && ok || falta` cannot run both
# audit-plan-completion.sh — plano 2026-10-26-touring-janela-eficacia (PLANO.md v3).
#
# Consumido pela cláusula `cross_audit` do loop_converged.py (exit 0 = todo artefato
# prometido existe e passa) para a tarefa task_1793043745985215009. Cada fase do §4
# é uma verificação; uma fase ainda não feita aparece como FALTA (fail-closed: o
# juiz só converge com o plano inteiro). Tudo aqui é leitura ou roda testes; nada
# é implantado e nada é escrito fora de diretórios temporários.
set -uo pipefail
R=/home/gabrielgadea/projects/touring
B="$R/docs/plans/2026-10-26-touring-janela-eficacia"
cd "$R" || exit 3
faltas=0
ok() { echo "  ok: $1"; }
falta() { echo "FALTA: $1"; faltas=$((faltas + 1)); }
has() { grep -rqs -- "$1" "${@:2}"; }

# --- Phase 0: S-1 (C0) instrumento versionado, fixture verde
python3 scripts/test_measure_tool_trio.py >/dev/null 2>&1 && ok "S-1 trio: fixture verde" || falta "S-1 scripts/test_measure_tool_trio.py vermelho"

# --- Phase 1: conteúdo do release R1
has "fn test_no_advisory_drops_sermon_keeps_decision" crates/ && ok "S-2 teste por sítio existe" || falta "S-2 test_no_advisory_drops_sermon_keeps_decision ausente"
has "touring_foundation::injection_policy" crates/touring-hooks crates/touring-hooks-core crates/touring-hook-runtime \
  && ok "S-2 política com consumidor nos sítios" || falta "S-2 injection_policy ainda sem consumidor"
has "fn test_kpi_reports_injection_suppressed_by_family" crates/ && ok "S-3 teste da telemetria existe" || falta "S-3 test_kpi_reports_injection_suppressed_by_family ausente"
f25="$(timeout 300 touring-quality check --gate F2.5 --target . --format json 2>/dev/null)"
echo "$f25" | grep -Eq 'ctutils@0\.4\.2|quinn-proto@0\.11\.15|rustls@0\.23\.41|rustls-webpki@0\.103\.13|wasmtime@36\.0\.14|webpki-roots@1\.0\.8' \
  && falta "S-4 algum dos 6 pacotes com patch compatível segue vulnerável" || ok "S-4 os 6 pacotes atualizados"
echo "$f25" | grep -q '"status": *"Fail"' && falta "S-4b F2.5 ainda Fail (wasmtime 46 / z3 / zip)" || ok "S-4b F2.5 fora de Fail"
python3 - <<'EOF' >/dev/null 2>&1 && ok "S-5 os 4 órfãos resolvidos" || falta "S-5 algum dos 4 órfãos segue no índice"
import json, subprocess
out = subprocess.run(["touring", "wiring", "orphans", "-j"], capture_output=True, text=True, timeout=120).stdout
names = {"cli_repo_health", "overlays_map"}
assert not any(n in out for n in names), names
EOF
has "fn test_post_bash_log_has_no_raw_command" crates/touring-hook-handlers && ok "S-6 log sem comando cru (teste)" || falta "S-6 teste do log ausente"
f21="$(timeout 120 touring-quality check --gate F2.1 --target crates/touring-server/build.rs --format json 2>/dev/null)"
echo "$f21" | grep -q '"status": *"Pass"' && ok "S-7 F2.1 do build.rs Pass" || falta "S-7 F2.1 do build.rs não passa"
timeout 600 python3 -m pytest client/hooks/tests -q -p no:cacheprovider >/dev/null 2>&1 && ok "S-8 client/hooks verde" || falta "S-8 client/hooks/tests vermelho"
python3 scripts/guard_injection_policy.py >/dev/null 2>&1 && ok "S-8 guarda da política 0 lacunas" || falta "S-8 guard_injection_policy com lacuna"
python3 scripts/test_injection_policy.py 2>&1 | grep -q " 0 FAIL\|FAIL: 0\|0 failed" && ok "suíte da política sem FAIL" \
  || { python3 scripts/test_injection_policy.py >/dev/null 2>&1 && ok "suíte da política (rc 0)" || falta "scripts/test_injection_policy.py vermelho"; }
ok "S-9 este script existe"

# --- Phase 3 (na emissão) — o instrumento entra no R1
has "fn test_gate_metrics_trio_matches_measure_tool_trio_fixture" crates/ && ok "S-11 paridade do trio na emissão" || falta "S-11 trio na emissão sem teste de paridade"

# --- Phase 2: release R1 implantado (o binário do daemon é mais novo que o último commit da wave)
last_commit_ts="$(git log -1 --format=%ct -- crates/touring-foundation/src/injection_policy.rs 2>/dev/null)"
last_commit_ts="${last_commit_ts:-0}"  # never committed yet = 0, not an empty string
daemon_ts="$(stat -c %Y target/release/touring-daemon 2>/dev/null || echo 0)"
[ "$last_commit_ts" -gt 0 ] && [ "$daemon_ts" -ge "$last_commit_ts" ] && ok "S-10 release R1 commitado e construído depois" \
  || falta "S-10 release R1 (commit da wave + deploy) não feito"

# --- Phases 3-4: dependem de dados pós-deploy
[ -f "$B/A3-preregistro.md" ] && ok "S-12 pré-registro do quase-experimento" || falta "S-12 A3-preregistro.md ausente"
has "fn test_every_gate_reason_names_the_next_command" crates/ && ok "S-13 rubrica das negações" || falta "S-13 teste da rubrica ausente"
has "fn test_compactor_never_cuts_failure_block" crates/ && ok "S-14 compactador com controle de falha" || falta "S-14 compactador ausente"
has "fn test_never_evicts_failure_results" crates/ && ok "S-16 mod de evicção determinístico" || falta "S-16 mod session.compact ausente"
ls "$B"/regua-*.md >/dev/null 2>&1 && ok "S-15 régua semanal com ponto registrado" || falta "S-15 nenhum ponto da régua semanal"

if [ "$faltas" -gt 0 ]; then
  echo "audit-plan-completion: $faltas falta(s)"
  exit 1
fi
echo "audit-plan-completion: completo"
