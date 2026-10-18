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
# 17/10/2026 — the silence had three causes and one face.  `[[ -f $FLAG ]] || exit 0` left the model
# without a warning whether the context was fine, the status line never rendered (headless runs and
# subagents have none: the sensor does not exist) or it had stopped rendering (the sensor is stale).
# Absent signal is not zero.  status_monitor.sh now writes a heartbeat (<session>.seen = "<epoch> <pct>")
# on EVERY render, and every call of this hook leaves a line in the journal with `silent_reason`:
#   abaixo        sensor alive (fresh heartbeat), no flag: the context is under the checkpoint
#   sem_sensor    no heartbeat at all: the status line never rendered for this session
#   obsoleto      the heartbeat is older than CONTEXT_SENSOR_STALE_SECS (default 900): it stopped rendering
#   entregue      flag armed and already delivered this cycle (the by-design silence)
#   monitor_projeto  a registered project monitor owns the alert
#   sem_sessao    the payload carries no readable session_id (jq missing, or no field)
# Journalling never changes what is injected.  A silent line is written when the reason CHANGES or every
# CONTEXT_JOURNAL_EVERY seconds (default 900) — not once per tool call.
#
# Journal: ~/.claude/touring/logs/context_check_journal.jsonl (same family as skill_suggestion_journal.jsonl).
#
# Contract: PostToolUse (*). Exit 0 always; injects only when this session has an undelivered flag.

INPUT=$(cat)
AUTO="${CLAUDE_AUTOCOMPACT_PCT_OVERRIDE:-65}"
STALE_SECS="${CONTEXT_SENSOR_STALE_SECS:-900}"
JOURNAL_EVERY="${CONTEXT_JOURNAL_EVERY:-900}"
ALERT_DIR="${HOME}/.claude/touring/context_alert"
JOURNAL="${HOME}/.claude/touring/logs/context_check_journal.jsonl"
printf -v NOW '%(%s)T' -1

# journal <said:true|false> <silent_reason|""> <session> <pct|""> <sensor_age|""> [detail]
# Every field is a fixed token, a sanitised session id or digits: nothing here needs JSON escaping.
journal() {
    local said="$1" reason="$2" session="$3" pct="$4" age="$5" detail="${6:-}"
    local reason_json="null" pct_json="null" age_json="null" detail_json=""
    [[ -n "$reason" ]] && reason_json="\"$reason\""
    [[ -n "$pct" ]] && pct_json="$pct"
    [[ -n "$age" ]] && age_json="$age"
    [[ -n "$detail" ]] && detail_json=",\"detail\":\"$detail\""
    mkdir -p "${JOURNAL%/*}" 2>/dev/null || return 0
    printf '{"ts":"%s","session":"%s","said":%s,"silent_reason":%s,"pct":%s,"sensor_age_s":%s%s}\n' \
        "$(date -Iseconds)" "$session" "$said" "$reason_json" "$pct_json" "$age_json" "$detail_json" \
        >> "$JOURNAL" 2>/dev/null || true
}

# journal_silent: dedupes by (session, reason) so a quiet session leaves a handful of lines, not one per call.
journal_silent() {
    local reason="$1" session="$2" pct="$3" age="$4" detail="${5:-}"
    local state="${ALERT_DIR}/${session}.silent" last_reason="" last_at=0
    if [[ -r "$state" ]]; then
        read -r last_reason last_at < "$state" 2>/dev/null || true
    fi
    [[ "$last_at" =~ ^[0-9]+$ ]] || last_at=0
    if [[ "$last_reason" == "$reason" ]] && (( NOW - last_at < JOURNAL_EVERY )); then
        return 0
    fi
    mkdir -p "$ALERT_DIR" 2>/dev/null || true
    printf '%s %s\n' "$reason" "$NOW" > "$state" 2>/dev/null || true
    journal false "$reason" "$session" "$pct" "$age" "$detail"
}

SESSION=$(printf '%s' "$INPUT" | jq -r '.session_id // empty' 2>/dev/null | tr -cd 'A-Za-z0-9_-')
if [[ -z "$SESSION" ]]; then
    if command -v jq >/dev/null 2>&1; then
        detail="payload sem session_id"
    else
        detail="jq ausente: session_id ilegível"
    fi
    journal_silent sem_sessao "_nosession" "" "" "$detail"
    exit 0
fi

FLAG="${ALERT_DIR}/${SESSION}.pct"
DELIVERED="${ALERT_DIR}/${SESSION}.delivered"
SEEN="${ALERT_DIR}/${SESSION}.seen"

# The status line's last heartbeat: age in seconds and the percentage it saw ("" when there is none / it is unreadable).
SENSOR_AGE=""
SEEN_PCT=""
SENSOR_DETAIL=""
if [[ -r "$SEEN" ]]; then
    seen_at=""
    read -r seen_at SEEN_PCT < "$SEEN" 2>/dev/null || true
    if [[ "$seen_at" =~ ^[0-9]+$ ]]; then
        SENSOR_AGE=$(( NOW - seen_at ))
        (( SENSOR_AGE < 0 )) && SENSOR_AGE=0
    else
        SENSOR_DETAIL="heartbeat ilegível"
    fi
    [[ "$SEEN_PCT" =~ ^[0-9]+$ ]] || SEEN_PCT=""
fi

# Defer only to a project monitor that is REGISTERED. The file existing is not the monitor
# running: analise kept context_monitor.py on disk, unregistered, and no session there ever
# received the alert.
if [[ -n "${CLAUDE_PROJECT_DIR:-}" ]]; then
    for settings in "${CLAUDE_PROJECT_DIR}/.claude/settings.json" "${CLAUDE_PROJECT_DIR}/.claude/settings.local.json"; do
        if [[ -f "$settings" ]] && grep -q 'context_monitor\.py' "$settings"; then
            journal_silent monitor_projeto "$SESSION" "$SEEN_PCT" "$SENSOR_AGE"
            exit 0
        fi
    done
fi

if [[ ! -f "$FLAG" ]]; then
    # No flag: under the checkpoint, OR nobody is measuring. Only a fresh heartbeat licenses "under".
    if [[ -n "$SENSOR_DETAIL" ]]; then
        journal_silent obsoleto "$SESSION" "$SEEN_PCT" "" "$SENSOR_DETAIL"  # a heartbeat that cannot be read proves nothing
    elif [[ -z "$SENSOR_AGE" ]]; then
        journal_silent sem_sensor "$SESSION" "" ""
    elif (( SENSOR_AGE > STALE_SECS )); then
        journal_silent obsoleto "$SESSION" "$SEEN_PCT" "$SENSOR_AGE"
    else
        journal_silent abaixo "$SESSION" "$SEEN_PCT" "$SENSOR_AGE"
    fi
    exit 0
fi
if [[ -f "$DELIVERED" ]]; then
    journal_silent entregue "$SESSION" "$(head -n1 "$FLAG" | tr -cd '0-9')" "$SENSOR_AGE"
    exit 0
fi

PCT=$(head -n1 "$FLAG" | tr -cd '0-9')
: > "$DELIVERED"
journal true "" "$SESSION" "$PCT" "$SENSOR_AGE"

MSG="Contexto em ${PCT:-?}% — a autocompactação dispara aos ${AUTO}%. Grave agora o estado em disco (RETOMADA do bundle, \`touring decompose\`, nó no grafo) para que a retomada não dependa do resumo. Se a próxima tarefa for distinta desta, prefira /clear e reconstrução pelo disco a /compact."
jq -cn --arg m "$MSG" '{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $m}}'
exit 0
