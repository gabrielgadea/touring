#!/usr/bin/env bash
# check_context.sh — PostToolUse hook: context-window usage alert.
#
# S42 (plan 2026-09-25): emit alert via hookSpecificOutput.additionalContext (JSON stdout)
# instead of writing to stderr with exit 0. Stderr output is invisible to the model —
# only the user sees it in the terminal. The additionalContext field is injected into the
# model's context on the next turn.
#
# Contract: PostToolUse (*). Exit 0 always.

# Consume stdin (required by hook protocol).
cat > /dev/null

FLAG_FILE="${HOME}/.claude/context_alert"

# Skip if project-level context monitor is active (avoids duplicates).
if [[ -n "${CLAUDE_PROJECT_DIR:-}" && -f "${CLAUDE_PROJECT_DIR}/.claude/hooks/validation/context_monitor.py" ]]; then
    exit 0
fi

if [[ -f "${FLAG_FILE}" ]]; then
    CONTEXT_PCT=$(cat "${FLAG_FILE}" 2>/dev/null || echo "?")
    THRESHOLD="${CONTEXT_THRESHOLD:-90}"
    MSG_ESC="Contexto em ${CONTEXT_PCT}% (threshold: ${THRESHOLD}%). Considere /compact."
    MSG_ESC=$(printf '%s' "${MSG_ESC}" | sed 's/\\/\\\\/g; s/"/\\"/g')
    printf '{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"%s"}}\n' "${MSG_ESC}"
fi

exit 0
