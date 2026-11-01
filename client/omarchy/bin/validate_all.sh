#!/usr/bin/env bash
# validate_all.sh — Roda P0..P8 (ou subconjunto), grava state/validate-<ts>.json
# Uso: validate_all.sh [--json] [--phases P0,P1,...]
# Exit: número de fases com falha (0 = tudo ok)
set -euo pipefail

SCRIPT_DIR="$(cd "${BASH_SOURCE[0]%/*}" && pwd)"
_PHASE="ALL"

# ── Arg parsing ───────────────────────────────────────────────────────────────
_JSON_MODE=0
OPT_PHASES=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --json)    _JSON_MODE=1; shift ;;
        --phases)  OPT_PHASES="$2"; shift 2 ;;
        --help|-h)
            printf 'Uso: validate_all.sh [--json] [--phases P0,P1,...]\n'
            printf 'Roda todos os validadores (P0..P8) ou subconjunto.\n'
            printf 'Grava ~/Work/state/validate-<ts>.json.\n'
            printf 'Exit = número de fases com falha.\n'
            exit 0 ;;
        *) printf 'Opção desconhecida: %s\n' "$1" >&2; exit 1 ;;
    esac
done

# shellcheck source=_validate_lib.sh
source "$SCRIPT_DIR/_validate_lib.sh"

# Determina quais fases rodar
ALL_PHASES=(P0 P1 P2 P3 P4 P5 P6 P7 P8)
RUN_PHASES=()

if [[ -n "$OPT_PHASES" ]]; then
    IFS=',' read -ra requested <<< "$OPT_PHASES"
    for p in "${requested[@]}"; do
        p=$(printf '%s' "$p" | tr '[:lower:]' '[:upper:]' | tr -d ' ')
        RUN_PHASES+=("$p")
    done
else
    RUN_PHASES=("${ALL_PHASES[@]}")
fi

# ── Rodar cada validador ──────────────────────────────────────────────────────
FAILED_PHASES=0
PHASE_RESULTS=()

for phase in "${RUN_PHASES[@]}"; do
    phase_lower=$(printf '%s' "$phase" | tr '[:upper:]' '[:lower:]')
    script="$SCRIPT_DIR/validate_${phase_lower}.sh"

    if [[ ! -f "$script" ]]; then
        check_fail "phase-script-missing" "$script"
        PHASE_RESULTS+=("{\"phase\":\"$phase\",\"ok\":false,\"error\":\"script not found\"}")
        FAILED_PHASES=$(( FAILED_PHASES + 1 ))
        continue
    fi

    phase_out=""
    phase_rc=0
    phase_out=$(bash "$script" --json 2>/dev/null) || phase_rc=$?

    if [[ "$phase_rc" -eq 0 ]]; then
        check_ok "phase-${phase}" "exit 0"
    else
        check_fail "phase-${phase}" "exit $phase_rc"
        FAILED_PHASES=$(( FAILED_PHASES + 1 ))
    fi

    if [[ -n "$phase_out" ]]; then
        PHASE_RESULTS+=("$phase_out")
    else
        PHASE_RESULTS+=("{\"phase\":\"$phase\",\"ok\":$([ "$phase_rc" -eq 0 ] && printf 'true' || printf 'false'),\"checks\":[]}")
    fi
done

# ── Gravar ~/Work/state/validate-<ts>.json ────────────────────────────────────
STATE_DIR="$HOME/Work/state"
mkdir -p "$STATE_DIR"
TS=$(date +%Y%m%dT%H%M%S)
STATE_FILE="$STATE_DIR/validate-${TS}.json"

python3 - "$STATE_FILE" "$FAILED_PHASES" "${PHASE_RESULTS[@]}" << 'PYEOF'
import sys, json, datetime

state_file = sys.argv[1]
failed = int(sys.argv[2])
phases_json = sys.argv[3:]

phases = []
for pj in phases_json:
    try:
        phases.append(json.loads(pj))
    except json.JSONDecodeError:
        phases.append({"raw": pj})

result = {
    "ts": datetime.datetime.utcnow().isoformat() + "Z",
    "phases": phases,
    "failed": failed
}
with open(state_file, 'w') as fh:
    json.dump(result, fh, indent=2)
print(f"Wrote {state_file}")
PYEOF

# ── Saída final ───────────────────────────────────────────────────────────────
# finish() emite JSON com {"phase","ok","checks"} (contrato) e retorna 0|1.
# O exit code é max(FAILED_PHASES, finish_rc) para preservar contagem real.
finish_rc=0
if [[ "$_JSON_MODE" -eq 1 ]]; then
    # JSON via lib finish — inclui FORCE_FAIL se FORCE_FAIL=1
    finish "$_PHASE" || finish_rc=$?
else
    # Modo humano: check_ok/check_fail já imprimiram linhas por fase acima.
    # Injeta FORCE_FAIL manualmente para modo não-JSON.
    if [[ "${FORCE_FAIL:-0}" == "1" ]]; then
        printf 'FAIL forced FORCE_FAIL=1 environment variable\n' >&2
        FAILED_PHASES=$(( FAILED_PHASES + 1 ))
    fi
    if [[ "$FAILED_PHASES" -eq 0 ]]; then
        printf 'ok ALL %d/%d phases passed\n' "${#RUN_PHASES[@]}" "${#RUN_PHASES[@]}"
    else
        printf 'FAIL ALL %d/%d phases failed\n' "$FAILED_PHASES" "${#RUN_PHASES[@]}" >&2
    fi
fi

exit_code=$(( FAILED_PHASES > finish_rc ? FAILED_PHASES : finish_rc ))
exit "$exit_code"
