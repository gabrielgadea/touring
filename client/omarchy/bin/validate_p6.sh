#!/usr/bin/env bash
# validate_p6.sh — Gate P6: Memória (router ~/Work, second brain, timer)
# Roda no Omarchy após configurar ~/Work e o second brain.
# Uso: validate_p6.sh [--json]
set -euo pipefail

SCRIPT_DIR="$(cd "${BASH_SOURCE[0]%/*}" && pwd)"
OMARCHY_ROOT="${SCRIPT_DIR%/*}"
_PHASE="P6"
_JSON_MODE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json)   _JSON_MODE=1; shift ;;
        --help|-h)
            printf 'Uso: validate_p6.sh [--json]\n'
            printf 'Gate P6: ~/Work router, AREA.md, blind test, second brain, timer.\n'
            exit 0 ;;
        *) printf 'Opção desconhecida: %s\n' "$1" >&2; exit 1 ;;
    esac
done

# shellcheck source=_validate_lib.sh
source "$SCRIPT_DIR/_validate_lib.sh"

WORK_DIR="$HOME/Work"
AREAS=("touring" "antt-detran" "conteudo" "pessoal")

# ── 1. ~/Work/CLAUDE.md + 4 AREA.md existem e cada AREA.md <= 80 linhas ──────
if [[ -f "$WORK_DIR/CLAUDE.md" ]]; then
    check_ok "work-claude-md" "$WORK_DIR/CLAUDE.md exists"
else
    check_fail "work-claude-md" "$WORK_DIR/CLAUDE.md not found"
fi

for area in "${AREAS[@]}"; do
    area_file="$WORK_DIR/$area/AREA.md"
    if [[ -f "$area_file" ]]; then
        line_count=$(wc -l < "$area_file")
        if [[ "$line_count" -le 80 ]]; then
            check_ok "area-md-${area}" "$area_file exists ($line_count lines)"
        else
            check_fail "area-md-${area}" "$area_file has $line_count lines (max 80)"
        fi
    else
        check_fail "area-md-${area}" "$area_file not found"
    fi
done

# ── 2. Links markdown do router resolvem ─────────────────────────────────────
if [[ -f "$WORK_DIR/CLAUDE.md" ]]; then
    bad_links=0
    while IFS= read -r link; do
        # Strip leading ./
        link="${link#./}"
        if [[ ! -e "$WORK_DIR/$link" ]]; then
            check_fail "router-link-broken" "$link not found"
            bad_links=$(( bad_links + 1 ))
        fi
    done < <(grep -oP '\[.*?\]\(\K[^)]+' "$WORK_DIR/CLAUDE.md" 2>/dev/null \
        | grep -v '^http' | grep -v '^#' || true)
    if [[ "$bad_links" -eq 0 ]]; then
        check_ok "router-links" "all markdown links in CLAUDE.md resolve"
    fi
fi

# ── 3. Teste cego (blind.json) ────────────────────────────────────────────────
BLIND_JSON="$OMARCHY_ROOT/tests/blind.json"
if [[ -f "$BLIND_JSON" ]]; then
    if command -v claude >/dev/null 2>&1; then
        expected_q=$(python3 -c "import json;print(len(json.load(open('$BLIND_JSON'))))" 2>/dev/null || printf '0')
        seen_q=0
        while IFS= read -r q && IFS= read -r expect; do
            seen_q=$(( seen_q + 1 ))
            if [[ "$q" == *"PLACEHOLDER"* ]] || [[ "$expect" == *"PLACEHOLDER"* ]]; then
                check_warn "blind-test-placeholder" "question not yet edited: $q"
                continue
            fi
            # `</dev/null` NAO e' decoracao: sem isso o `claude` herda o stdin do
            # laco — a process substitution com as perguntas — e CONSOME as linhas
            # restantes. O laco entao roda UMA vez e o gate reporta "ok blind-test"
            # como se tivesse avaliado as tres. Um check que parece testar N e
            # testa 1, sem dizer. Medido 2026-08-23 na primeira execucao real.
            result=$(cd "$WORK_DIR" && claude -p "$q" --permission-mode plan </dev/null 2>/dev/null || printf '')
            if printf '%s' "$result" | grep -qi "$expect"; then
                check_ok "blind-test" "Q: $q → found '$expect'"
            else
                check_fail "blind-test" "Q: $q → '$expect' not in response"
            fi
        done < <(python3 -c "
import json, sys
data = json.load(open('$BLIND_JSON'))
for item in data:
    print(item.get('q','').replace(chr(10),' '))
    print(item.get('expect','').replace(chr(10),' '))
" 2>/dev/null || true)
        # Fail-closed: o numero de perguntas avaliadas tem de bater com o gabarito.
        if [[ "$seen_q" -ne "$expected_q" ]]; then
            check_fail "blind-test-coverage" "evaluated $seen_q of $expected_q question(s) in blind.json"
        else
            check_ok "blind-test-coverage" "evaluated all $expected_q question(s)"
        fi
    else
        check_fail "missing-tool" "claude"
    fi
else
    check_fail "blind-json-exists" "$BLIND_JSON not found"
fi

# ── 4. curl -s 127.0.0.1:7777/brain/ com >= 3 <h2 ───────────────────────────
if need_tool curl; then
    brain_html=$(curl -s 'http://127.0.0.1:7777/brain/' 2>/dev/null || printf '')
    # `x=$(...) || x=0` — ver nota da classe em validate_p0.sh:53.
    h2_count=$(printf '%s' "$brain_html" | grep -c '<h2') || h2_count=0
    if [[ "$h2_count" -ge 3 ]]; then
        check_ok "second-brain-h2" "$h2_count <h2> tags at 127.0.0.1:7777/brain/"
    else
        check_fail "second-brain-h2" "$h2_count <h2> tags (expected >= 3)"
    fi
fi

# ── 5. timer routine-brain-moc listado ───────────────────────────────────────
if command -v systemctl >/dev/null 2>&1; then
    if systemctl --user list-timers --all 2>/dev/null | grep -q 'routine-brain-moc'; then
        check_ok "timer-brain-moc" "routine-brain-moc listed in systemctl"
    else
        check_fail "timer-brain-moc" "routine-brain-moc not found in systemctl --user list-timers"
    fi
else
    need_tool systemctl
fi

finish "$_PHASE"
