#!/usr/bin/env bash
# validate_p3.sh — Gate P3: agent harness (Claude Code + kit restaurado)
# Roda no Omarchy após restaurar o kit e fazer login no Claude.
# Uso: validate_p3.sh [--json]
set -euo pipefail

SCRIPT_DIR="$(cd "${BASH_SOURCE[0]%/*}" && pwd)"
_PHASE="P3"
_JSON_MODE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json)   _JSON_MODE=1; shift ;;
        --help|-h)
            printf 'Uso: validate_p3.sh [--json]\n'
            printf 'Gate P3: Claude Code, skills, MCP touring, .claude.json seguro.\n'
            exit 0 ;;
        *) printf 'Opção desconhecida: %s\n' "$1" >&2; exit 1 ;;
    esac
done

# shellcheck source=_validate_lib.sh
source "$SCRIPT_DIR/_validate_lib.sh"

# ── 1. claude -p "print exactly: OK" --permission-mode plan imprime OK ────────
if need_tool claude; then
    claude_out=$(claude -p "print exactly: OK" --permission-mode plan 2>/dev/null || printf '')
    if printf '%s' "$claude_out" | grep -q '^OK$'; then
        check_ok "claude-plan-mode" "output: OK"
    else
        check_fail "claude-plan-mode" "expected 'OK', got: $(printf '%s' "$claude_out" | head -1)"
    fi
fi

# ── 2. find ~/.claude/skills -xtype l | wc -l == 0 ───────────────────────────
if [[ -d "$HOME/.claude/skills" ]]; then
    broken_links=$(find "$HOME/.claude/skills" -xtype l 2>/dev/null | wc -l)
    if [[ "$broken_links" -eq 0 ]]; then
        check_ok "no-broken-skill-symlinks" "0 broken symlinks in ~/.claude/skills"
    else
        check_fail "no-broken-skill-symlinks" "$broken_links broken symlink(s) in ~/.claude/skills"
    fi
else
    check_fail "skills-dir-exists" "$HOME/.claude/skills not found"
fi

# ── 3. hooks_runnable.py ~/.claude/settings.json exit 0 ──────────────────────
HOOKS_RUNNABLE="$SCRIPT_DIR/hooks_runnable.py"
SETTINGS="$HOME/.claude/settings.json"
if [[ -x "$HOOKS_RUNNABLE" ]] || [[ -f "$HOOKS_RUNNABLE" ]]; then
    if [[ -f "$SETTINGS" ]]; then
        if python3 "$HOOKS_RUNNABLE" "$SETTINGS" >/dev/null 2>&1; then
            check_ok "hooks-runnable-settings" "hooks_runnable.py exit 0"
        else
            check_fail "hooks-runnable-settings" "hooks_runnable.py exit nonzero"
        fi
    else
        check_fail "settings-json-exists" "$SETTINGS not found"
    fi
else
    check_warn "hooks-runnable" "pending-artifact bin/hooks_runnable.py"
fi

# ── 4. claude mcp list contém touring ────────────────────────────────────────
if command -v claude >/dev/null 2>&1; then
    if claude mcp list 2>/dev/null | grep -qi 'touring'; then
        check_ok "mcp-touring-listed" "touring in claude mcp list"
    else
        check_fail "mcp-touring-listed" "touring not found in claude mcp list"
    fi
else
    check_warn "mcp-touring-listed" "claude not available (see check above)"
fi

# ── 5. ls ~/.claude/skills | wc -l >= 170 ────────────────────────────────────
if [[ -d "$HOME/.claude/skills" ]]; then
    skill_count=$(ls "$HOME/.claude/skills" 2>/dev/null | wc -l)
    if [[ "$skill_count" -ge 170 ]]; then
        check_ok "skills-count" "$skill_count skills in ~/.claude/skills"
    else
        check_fail "skills-count" "$skill_count skills (expected >= 170)"
    fi
fi

# ── 6. ~/.claude.json modo 600 ───────────────────────────────────────────────
CLAUDE_JSON="$HOME/.claude.json"
if [[ -f "$CLAUDE_JSON" ]]; then
    perms=$(stat -c '%a' "$CLAUDE_JSON" 2>/dev/null || printf '')
    if [[ "$perms" == "600" ]]; then
        check_ok "claude-json-perms" "mode 600 OK"
    else
        check_fail "claude-json-perms" "mode $perms (expected 600)"
    fi
else
    check_fail "claude-json-exists" "$CLAUDE_JSON not found"
fi

finish "$_PHASE"
