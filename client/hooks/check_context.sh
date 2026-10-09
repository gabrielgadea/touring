#!/usr/bin/env bash
# check_context.sh — PostToolUse hook: ONE checkpoint warning per compaction cycle.
#
# 08/10/2026 (Gabriel: "autocompactar aos 65%"): auto-compaction fires at
# CLAUDE_AUTOCOMPACT_PCT_OVERRIDE (65) of the window, so the warning has to come BEFORE it.
# status_monitor.sh arms ~/.claude/touring/context_alert/<session>.pct at the checkpoint
# (autocompact − 10, or CONTEXT_CHECKPOINT_PCT); this hook speaks once and marks
# <session>.delivered. The status line removes both when the context falls back under the
# checkpoint — after the compaction — which re-arms the next cycle.
#
# History: S42 (2026-09-25) moved the alert from stderr (invisible to the model) to
# hookSpecificOutput.additionalContext. Until 08/10 the flag was ONE global file
# (~/.claude/context_alert): with several live sessions the first to cross the threshold lit it
# for all, the Stop hook of any session consumed it, and the warning repeated on every tool call.
#
# Contract: PostToolUse (*). Exit 0 always; silent unless this session has an undelivered flag.

INPUT=$(cat)
AUTO="${CLAUDE_AUTOCOMPACT_PCT_OVERRIDE:-65}"
SESSION=$(printf '%s' "$INPUT" | jq -r '.session_id // empty' 2>/dev/null | tr -cd 'A-Za-z0-9_-')
[[ -n "$SESSION" ]] || exit 0

ALERT_DIR="${HOME}/.claude/touring/context_alert"
FLAG="${ALERT_DIR}/${SESSION}.pct"
DELIVERED="${ALERT_DIR}/${SESSION}.delivered"

# Defer only to a project monitor that is REGISTERED. The file existing is not the monitor
# running: analise kept context_monitor.py on disk, unregistered, and no session there ever
# received the alert.
if [[ -n "${CLAUDE_PROJECT_DIR:-}" ]]; then
    for settings in "${CLAUDE_PROJECT_DIR}/.claude/settings.json" "${CLAUDE_PROJECT_DIR}/.claude/settings.local.json"; do
        if [[ -f "$settings" ]] && grep -q 'context_monitor\.py' "$settings"; then
            exit 0
        fi
    done
fi

[[ -f "$FLAG" && ! -f "$DELIVERED" ]] || exit 0
PCT=$(tr -cd '0-9' < "$FLAG")
: > "$DELIVERED"

MSG="Contexto em ${PCT:-?}% — a autocompactação dispara aos ${AUTO}%. Grave agora o estado em disco (RETOMADA do bundle, \`touring decompose\`, nó no grafo) para que a retomada não dependa do resumo. Se a próxima tarefa for distinta desta, prefira /clear e reconstrução pelo disco a /compact."
jq -cn --arg m "$MSG" '{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $m}}'
exit 0
