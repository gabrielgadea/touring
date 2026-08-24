#!/usr/bin/env bash
# _validate_lib.sh — biblioteca compartilhada dos validadores omarchy
# Source com: source "$(dirname "${BASH_SOURCE[0]}")/_validate_lib.sh"
# NÃO executar diretamente.
#
# Variáveis de controle (setar ANTES de source ou entre source e finish):
#   _PHASE     — identificador da fase (ex: "P0")
#   _JSON_MODE — 1 para output JSON (setada pelo script chamador ao parsear --json)
#
# Variáveis de ambiente:
#   FORCE_FAIL=1 — injeta check_fail "forced" em finish() (testa o contrato)

_OK_COUNT=0
_FAIL_COUNT=0
_WARN_COUNT=0
_JSON_MODE="${_JSON_MODE:-0}"
_PHASE="${_PHASE:-unknown}"
# Acumula registros como "STATUS<TAB>NAME<TAB>EVIDENCE<NEWLINE>"
_CHECKS_TSV=""
_FINISH_CALLED=0

# check_ok NAME [EVIDENCE]
check_ok() {
    local name="$1" evidence="${2:-}"
    _OK_COUNT=$(( _OK_COUNT + 1 ))
    _CHECKS_TSV+="ok	${name}	${evidence}
"
    if [[ "$_JSON_MODE" -eq 0 ]]; then
        printf 'ok %s %s\n' "$name" "$evidence"
    fi
}

# check_fail NAME [EVIDENCE]
check_fail() {
    local name="$1" evidence="${2:-}"
    _FAIL_COUNT=$(( _FAIL_COUNT + 1 ))
    _CHECKS_TSV+="fail	${name}	${evidence}
"
    if [[ "$_JSON_MODE" -eq 0 ]]; then
        printf 'FAIL %s %s\n' "$name" "$evidence" >&2
    fi
}

# check_warn NAME [EVIDENCE]
check_warn() {
    local name="$1" evidence="${2:-}"
    _WARN_COUNT=$(( _WARN_COUNT + 1 ))
    _CHECKS_TSV+="warn	${name}	${evidence}
"
    if [[ "$_JSON_MODE" -eq 0 ]]; then
        printf 'warn %s %s\n' "$name" "$evidence"
    fi
}

# need_tool NAME  →  check_fail "missing-tool" NAME se command -v falhar; retorna 1
need_tool() {
    local tool="$1"
    if ! command -v "$tool" >/dev/null 2>&1; then
        check_fail "missing-tool" "$tool"
        return 1
    fi
    return 0
}

# finish [PHASE]
#   Injeta FORCE_FAIL se FORCE_FAIL=1.
#   Imprime JSON se _JSON_MODE=1.
#   Retorna 0 se _FAIL_COUNT == 0, 1 caso contrário.
finish() {
    _FINISH_CALLED=1
    local phase="${1:-$_PHASE}"

    if [[ "${FORCE_FAIL:-0}" == "1" ]]; then
        check_fail "forced" "FORCE_FAIL=1 environment variable"
    fi

    if [[ "$_JSON_MODE" -eq 1 ]]; then
        # Criar tmpfile via python3 (sem precisar de mktemp/rm externos)
        local tmpf
        tmpf=$(python3 -c 'import tempfile; f=tempfile.NamedTemporaryFile(delete=False,suffix=".tsv"); print(f.name); f.close()')
        printf '%s' "$_CHECKS_TSV" > "$tmpf"
        local fail_count="$_FAIL_COUNT"
        python3 - "$phase" "$fail_count" "$tmpf" <<'PYEOF_INNER'
import sys, json, os
phase = sys.argv[1]
fail_count = int(sys.argv[2])
tmpf = sys.argv[3]
checks = []
with open(tmpf) as fh:
    for line in fh:
        line = line.rstrip('\n')
        if not line:
            continue
        parts = line.split('\t', 2)
        if len(parts) >= 2:
            checks.append({
                'name': parts[1],
                'status': parts[0],
                'evidence': parts[2] if len(parts) > 2 else ''
            })
os.unlink(tmpf)
result = {'phase': phase, 'ok': fail_count == 0, 'checks': checks}
print(json.dumps(result))
PYEOF_INNER
    fi

    if [[ "$_FAIL_COUNT" -gt 0 ]]; then
        return 1
    fi
    return 0
}

# ── EXIT trap — garante JSON mesmo se script abortar antes de finish() ────────
_lib_exit_trap() {
    local _rc=$?
    if [[ "$_FINISH_CALLED" -eq 0 ]]; then
        check_fail "script-aborted" "script exited rc=$_rc before finish()"
        finish "$_PHASE" || true
    fi
}
trap '_lib_exit_trap' EXIT
