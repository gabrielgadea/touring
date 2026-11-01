#!/usr/bin/env bash
# validate_p8.sh — Gate P8: Apps + Command Centre
# Roda no Omarchy após configurar connectors, web apps e CC.
# Uso: validate_p8.sh [--json]
set -euo pipefail

SCRIPT_DIR="$(cd "${BASH_SOURCE[0]%/*}" && pwd)"
_PHASE="P8"
_JSON_MODE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json)   _JSON_MODE=1; shift ;;
        --help|-h)
            printf 'Uso: validate_p8.sh [--json]\n'
            printf 'Gate P8: search-connectors, connectors.json, apps, novnc, CC, cc-serve.\n'
            exit 0 ;;
        *) printf 'Opção desconhecida: %s\n' "$1" >&2; exit 1 ;;
    esac
done

# shellcheck source=_validate_lib.sh
source "$SCRIPT_DIR/_validate_lib.sh"

WORK_DIR="$HOME/Work"

# ── 1. ~/.claude/skills/search-connectors/SKILL.md existe ────────────────────
SKILL_MD="$HOME/.claude/skills/search-connectors/SKILL.md"
if [[ -f "$SKILL_MD" ]]; then
    check_ok "search-connectors-skill" "$SKILL_MD exists"
else
    check_fail "search-connectors-skill" "$SKILL_MD not found"
fi

# ── 2. ~/Work/state/connectors.json com 3 entradas ok ────────────────────────
CONNECTORS_JSON="$WORK_DIR/state/connectors.json"
if [[ -f "$CONNECTORS_JSON" ]]; then
    ok_count=$(python3 -c "
import json, sys
data = json.load(open('$CONNECTORS_JSON'))
# Formato canônico do connectors_audit.sh: {'counts':…, 'connectors':[{status,…}]}.
# Conta da fonte (a lista), nunca do agregado 'counts' — que poderia divergir dela.
if isinstance(data, dict) and isinstance(data.get('connectors'), list):
    data = data['connectors']
if isinstance(data, list):
    print(sum(1 for c in data if isinstance(c, dict) and c.get('status') == 'ok'))
else:
    print(0)
" 2>/dev/null || printf '0')
    if [[ "$ok_count" -ge 3 ]]; then
        check_ok "connectors-ok-count" "$ok_count ok connectors"
    else
        check_fail "connectors-ok-count" "$ok_count ok (expected 3) in $CONNECTORS_JSON"
    fi
else
    check_fail "connectors-json-exists" "$CONNECTORS_JSON not found"
fi

# ── 3. >= 3 .desktop casando gmail|calendar|notion|routines ──────────────────
APPS_DIR="$HOME/.local/share/applications"
if [[ -d "$APPS_DIR" ]]; then
    _desk="$(find "$APPS_DIR" -name '*.desktop' -print0 2>/dev/null \
        | xargs -0r grep -liE 'gmail|calendar|notion|routines' 2>/dev/null)" || _desk=""
    desktop_count=$(printf '%s' "$_desk" | grep -c . || true)
    if [[ "$desktop_count" -ge 3 ]]; then
        check_ok "web-apps-desktop" "$desktop_count .desktop files matching apps"
    else
        check_fail "web-apps-desktop" "$desktop_count .desktop files (expected >= 3)"
    fi
else
    check_fail "apps-dir-exists" "$APPS_DIR not found"
fi

# ── 4. curl 127.0.0.1:8006 grep novnc (warn se Docker/VM desligados) ─────────
if need_tool curl; then
    novnc_out=$(curl -s 'http://127.0.0.1:8006' 2>/dev/null || printf '')
    if printf '%s' "$novnc_out" | grep -qi 'novnc'; then
        check_ok "windows-novnc" "noVNC found at 127.0.0.1:8006"
    else
        check_warn "windows-novnc" "noVNC not reachable at 127.0.0.1:8006 (Docker/VM may be off)"
    fi
fi

# ── 5. curl 127.0.0.1:7777 | grep -c 'data-widget=' == 3 ────────────────────
if command -v curl >/dev/null 2>&1; then
    cc_html=$(curl -s 'http://127.0.0.1:7777' 2>/dev/null || printf '')
    # `x=$(...) || x=0` — ver nota da classe em validate_p0.sh:53.
    widget_count=$(printf '%s' "$cc_html" | grep -c 'data-widget=') || widget_count=0
    if [[ "$widget_count" -eq 3 ]]; then
        check_ok "cc-widgets" "3 data-widget= elements at 127.0.0.1:7777"
    else
        check_fail "cc-widgets" "$widget_count data-widget= elements (expected 3)"
    fi
fi

# ── 6. systemctl --user is-active cc-serve == active ─────────────────────────
if command -v systemctl >/dev/null 2>&1; then
    cc_state=$(systemctl --user is-active cc-serve 2>/dev/null || printf 'inactive')
    if [[ "$cc_state" == "active" ]]; then
        check_ok "cc-serve-active" "cc-serve.service is active"
    else
        check_fail "cc-serve-active" "cc-serve.service is $cc_state (expected active)"
    fi
else
    need_tool systemctl
fi

finish "$_PHASE"
