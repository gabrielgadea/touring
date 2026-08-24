#!/usr/bin/env bash
# validate_p7.sh — Gate P7: Rotinas + Herdr x ADW
# Roda no Omarchy após configurar routines_gen.py e os timers.
# Uso: validate_p7.sh [--json]
set -euo pipefail

SCRIPT_DIR="$(cd "${BASH_SOURCE[0]%/*}" && pwd)"
_PHASE="P7"
_JSON_MODE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json)   _JSON_MODE=1; shift ;;
        --help|-h)
            printf 'Uso: validate_p7.sh [--json]\n'
            printf 'Gate P7: timers, units válidas, validate-*.json, PR Claude, ADW spec.\n'
            exit 0 ;;
        *) printf 'Opção desconhecida: %s\n' "$1" >&2; exit 1 ;;
    esac
done

# shellcheck source=_validate_lib.sh
source "$SCRIPT_DIR/_validate_lib.sh"

WORK_DIR="$HOME/Work"
ADW_SPEC="$HOME/projects/touring/.touring/adw/omarchy-audit-2.toml"

# ── 1. Todo routine LOCAL do board tem timer; o board tem >= 5 rotinas ───────
# O criterio antigo era `>= 5 timers`, que confunde "5 rotinas ARMS" com "5
# timers". Uma delas (docs-drift) e' `runner = "cloud"` e por DESENHO nao gera
# unit — vira uma entrada em ~/Work/state/routines-cloud.md. O board correto
# produz 4 timers, entao o gate reprovava um estado correto. O contrato real,
# e mais forte: cada rotina local tem o seu timer instalado (nenhuma some em
# silencio) e o board tem as 5 rotinas. Medido 2026-08-23.
BOARD_TOML="$WORK_DIR/routines.toml"
ROUTINES_GEN="$SCRIPT_DIR/routines_gen.py"
if need_tool systemctl; then
    live_timers=$(systemctl --user list-timers --all --no-pager 2>/dev/null \
        | grep -o 'routine-[a-z0-9-]*\.timer' | sort -u) || live_timers=""
    if [[ -f "$BOARD_TOML" && -f "$ROUTINES_GEN" ]]; then
        board_json=$(python3 "$ROUTINES_GEN" board --toml "$BOARD_TOML" --json 2>/dev/null || printf '[]')
        total=$(printf '%s' "$board_json" | python3 -c "import json,sys; print(len(json.load(sys.stdin)))" 2>/dev/null || printf '0')
        locals_ids=$(printf '%s' "$board_json" | python3 -c \
            "import json,sys; print(' '.join(r['id'] for r in json.load(sys.stdin) if r.get('runner','local')=='local'))" \
            2>/dev/null || printf '')
        missing=""
        for rid in $locals_ids; do
            printf '%s\n' "$live_timers" | grep -qx "routine-$rid.timer" || missing="$missing $rid"
        done
        n_local=$(printf '%s' "$locals_ids" | wc -w)
        if [[ -n "$missing" ]]; then
            check_fail "routine-timers-installed" "local routine(s) without a timer:$missing"
        else
            check_ok "routine-timers-installed" "$n_local/$n_local local routine(s) have a live timer"
        fi
        if [[ "$total" -ge 5 ]]; then
            check_ok "routine-board-size" "$total routine(s) in $BOARD_TOML ($n_local local + $(( total - n_local )) cloud)"
        else
            check_fail "routine-board-size" "$total routine(s) in $BOARD_TOML (expected >= 5)"
        fi
    else
        check_fail "routine-board-exists" "$BOARD_TOML or routines_gen.py not found"
    fi

    # ── 2. systemd-analyze --user verify das units ────────────────────────────
    if command -v systemd-analyze >/dev/null 2>&1; then
        unit_files=("$HOME/.config/systemd/user"/routine-*.service)
        if [[ ${#unit_files[@]} -gt 0 ]] && [[ -f "${unit_files[0]}" ]]; then
            if systemd-analyze --user verify "${unit_files[@]}" >/dev/null 2>&1; then
                check_ok "systemd-units-valid" "all routine-*.service units valid"
            else
                check_fail "systemd-units-valid" "systemd-analyze --user verify failed"
            fi
        else
            check_fail "systemd-units-exist" "no routine-*.service files in ~/.config/systemd/user/"
        fi
    else
        need_tool systemd-analyze
    fi
fi

# ── 3. ~/Work/state/validate-*.json com < 26h ────────────────────────────────
STATE_DIR="$WORK_DIR/state"
if [[ -d "$STATE_DIR" ]]; then
    # Find any validate-*.json younger than 26h (93600 seconds)
    recent=$(find "$STATE_DIR" -name 'validate-*.json' -mmin -1560 2>/dev/null | head -1)
    if [[ -n "$recent" ]]; then
        check_ok "validate-json-recent" "$recent (< 26h old)"
    else
        # Check if any exists at all
        any=$(find "$STATE_DIR" -name 'validate-*.json' 2>/dev/null | head -1)
        if [[ -n "$any" ]]; then
            check_fail "validate-json-recent" "$any is older than 26h"
        else
            check_fail "validate-json-exists" "no validate-*.json in $STATE_DIR"
        fi
    fi
else
    check_fail "state-dir-exists" "$STATE_DIR not found"
fi

# ── 4. gh pr list --author app/claude >= 1 (warn se gh sem auth) ─────────────
if command -v gh >/dev/null 2>&1; then
    _prs="$(gh pr list --author 'app/claude' --limit 5 2>/dev/null)" || _prs=""
    pr_count=$(printf '%s' "$_prs" | grep -c . || true)
    if [[ "$pr_count" -ge 1 ]]; then
        check_ok "gh-claude-pr" "$pr_count PR(s) from app/claude"
    else
        # Check if gh is authenticated
        if gh auth status >/dev/null 2>&1; then
            check_warn "gh-claude-pr" "no PRs from app/claude yet (routine cloud not fired)"
        else
            check_warn "gh-claude-pr" "gh not authenticated; skipping PR check"
        fi
    fi
else
    need_tool gh
fi

# ── 5. anthropic-routine.token modo 600 ──────────────────────────────────────
TOKEN_FILE="$HOME/.config/omarchy/secrets/anthropic-routine.token"
if [[ -f "$TOKEN_FILE" ]]; then
    perms=$(stat -c '%a' "$TOKEN_FILE" 2>/dev/null || printf '')
    if [[ "$perms" == "600" ]]; then
        check_ok "routine-token-perms" "mode 600 OK"
    else
        check_fail "routine-token-perms" "$TOKEN_FILE mode $perms (expected 600)"
    fi
else
    check_warn "routine-token-exists" "$TOKEN_FILE not found (needed for /fire)"
fi

# ── 6. spec omarchy-audit-2.toml existe e adw lint exit 0 ───────────────────
if [[ -f "$ADW_SPEC" ]]; then
    if command -v touring >/dev/null 2>&1; then
        # `adw lint` resolve o spec em <proj>/.touring/adw/ a partir do CWD — e' um
        # comando PER-PROJECT. Rodado de ~/Work (como faz a rotina validate-all)
        # ele nao acha o spec e reprova um estado correto. Mesma classe do
        # e2e/symbol_count do P4. Medido 2026-08-23.
        if (cd "$HOME/projects/touring" && touring adw lint omarchy-audit-2) >/dev/null 2>&1; then
            check_ok "adw-omarchy-audit-2-lint" "touring adw lint exit 0"
        else
            check_fail "adw-omarchy-audit-2-lint" "touring adw lint omarchy-audit-2 failed"
        fi
    else
        need_tool touring
    fi
else
    check_fail "adw-spec-omarchy-audit-2" "$ADW_SPEC not found"
fi

# ── 7. Último journal ADW contém audit.txt e docs.txt ────────────────────────
# `adw-runs`, NAO `adw/runs`: o runner grava em <proj>/.touring/adw-runs/<run_id>/
# (verificado 2026-08-23 contra a run real `xaudit-gates-1787517521-...`). Com o
# caminho errado o check caia sempre no ramo "dir nao encontrada" e emitia warn —
# nunca poderia atestar nem reprovar coisa alguma.
ADW_RUNS="$HOME/projects/touring/.touring/adw-runs"
# Os journals do fan-out sao escritos por herdr_branch.sh em <journal_dir>, que o
# fragmento documenta como ~/Work/artifacts/adw-<ramo> (ou o `--var journal`
# passado na chamada) — NAO dentro do diretorio de run do ADW, que guarda o
# journal do runner. Procurar so' no run dir era procurar no lugar errado.
ADW_JOURNALS="$HOME/Work/artifacts"
if [[ -d "$ADW_RUNS" ]]; then
    # Find the most recent run directory
    latest_run=$(find "$ADW_RUNS" -mindepth 1 -maxdepth 1 -type d 2>/dev/null \
        | sort | tail -1)
    if [[ -n "$latest_run" ]]; then
        has_audit=$(find "$latest_run" "$ADW_JOURNALS" -name 'audit.txt' 2>/dev/null | head -1)
        has_docs=$(find "$latest_run" "$ADW_JOURNALS" -name 'docs.txt' 2>/dev/null | head -1)
        if [[ -n "$has_audit" ]] && [[ -n "$has_docs" ]]; then
            check_ok "adw-journal-artifacts" "audit.txt ($has_audit) and docs.txt ($has_docs)"
        else
            check_warn "adw-journal-artifacts" \
                "audit.txt=$(test -n "$has_audit" && printf found || printf missing) docs.txt=$(test -n "$has_docs" && printf found || printf missing) in $latest_run"
        fi
    else
        check_warn "adw-runs-exist" "no run directories in $ADW_RUNS"
    fi
else
    check_warn "adw-runs-dir" "$ADW_RUNS not found (run adw first)"
fi

# ── 8. servidor herdr sob systemd (S-7.5 / S-5.6) ────────────────────────────
# O botao --herdr e o fan-out exigem um servidor vivo: sem ele `herdr workspace
# create` sai 1 e o dispatch inteiro falha. Ate 23/08/2026 nada o subia — foi
# levantado a mao com nohup e morreria no primeiro reboot. `is-enabled` e' o que
# prova que sobrevive ao boot; `is-active` sozinho nao distingue "gerenciado" de
# "alguem rodou na mao".
if command -v systemctl >/dev/null 2>&1; then
    hs_enabled=$(systemctl --user is-enabled herdr-server.service 2>/dev/null || printf 'no')
    hs_active=$(systemctl --user is-active herdr-server.service 2>/dev/null || printf 'no')
    if [[ "$hs_enabled" == "enabled" && "$hs_active" == "active" ]]; then
        check_ok "herdr-server-managed" "herdr-server.service enabled and active"
    elif [[ "$hs_enabled" == "enabled" ]]; then
        check_fail "herdr-server-managed" "herdr-server.service enabled but $hs_active"
    else
        check_fail "herdr-server-managed" \
            "herdr-server.service not enabled — the --herdr button dies at the next reboot"
    fi
fi

finish "$_PHASE"
