#!/bin/bash
# =============================================================================
# Claude Code Context Monitor - StatusLine Script
# Reads official JSON input from Claude Code statusLine API
# Fields: model, cost, context_window (with current_usage), workspace, rate_limits
# =============================================================================

# 08/10/2026 (Gabriel: "autocompactar aos 65%"): auto-compaction fires at
# CLAUDE_AUTOCOMPACT_PCT_OVERRIDE of the window; the checkpoint warning arms CHECKPOINT points
# before it, PER SESSION (check_context.sh speaks once per cycle). Replaces CONTEXT_THRESHOLD (95)
# and the global ~/.claude/context_alert flag, which a 65% compaction would never have reached.
AUTOCOMPACT=${CLAUDE_AUTOCOMPACT_PCT_OVERRIDE:-65}
CHECKPOINT=${CONTEXT_CHECKPOINT_PCT:-$((AUTOCOMPACT - 10))}
ALERT_DIR="${HOME}/.claude/touring/context_alert"
# 18/10/2026: franchise quota. Red band and the one-per-window alert share the 5 h threshold (85), yellow starts at 70.
QUOTA_ALERT=${QUOTA_ALERT_PCT:-85}
QUOTA_WARN=${QUOTA_WARN_PCT:-70}

mkdir -p "$ALERT_DIR"

# ANSI colors
RESET='\033[0m'
BOLD='\033[1m'
DIM='\033[2m'
GREEN='\033[32m'
YELLOW='\033[33m'
RED='\033[31m'
BRIGHT_GREEN='\033[92m'
BRIGHT_YELLOW='\033[93m'
BRIGHT_RED='\033[91m'
BRIGHT_CYAN='\033[96m'
BRIGHT_WHITE='\033[97m'
BG_RED='\033[41m'
WHITE='\033[37m'

input=$(cat)

# Diagnóstico: se STATUSLINE_DEBUG_LOG=1, persistir payload bruto para inspeção.
if [ "${STATUSLINE_DEBUG_LOG:-0}" = "1" ]; then
    DEBUG_FILE="${HOME}/.claude/statusline_last.json"
    printf '%s' "$input" > "$DEBUG_FILE" 2>/dev/null
fi

# Cota da franquia → ~/.claude/touring/rate_limits.json, para a sessão consultá-la antes de um
# fanout (`rate_limits_snapshot.py --show`; o agent_model_gate a cita na recusa). Só quando o
# payload a traz — ausência nunca apaga a última leitura —, em background para não atrasar a
# renderização. O gravador mora ao lado deste script (o symlink em ~/.claude/hooks é resolvido).
case "$input" in
    *'"rate_limits"'*)
        printf '%s' "$input" | python3 "$(dirname "$(readlink -f "$0")")/rate_limits_snapshot.py" >/dev/null 2>&1 &
        ;;
esac

if ! command -v jq &> /dev/null; then
    printf '%bjq not installed%b\n' "$DIM" "$RESET"
    exit 0
fi

# Extract from official JSON schema
MODEL=$(echo "$input" | jq -r '.model.display_name // "claude"' 2>/dev/null)
MODEL_ID=$(echo "$input" | jq -r '.model.id // ""' 2>/dev/null)

# Model-driven window detection (PRIORIDADE sobre JSON value — Claude Code
# às vezes envia context_window_size=200000 mesmo para opus[1m]).
# Janela real:
#   MiniMax-M3 · opus[1m] · claude-opus-4-1 · display com "(1M)" ou "1M" → 1.000.000
#   demais (Sonnet/Haiku) → valor do JSON se presente, senão 200.000 default
# WAS_FORCED bool — indica se OVERRIDE ativo (o selo WIN_BADGE abaixo o mostra).
# Quando WAS_FORCED=true + janela 1M, Claude Code pode ter um clamp interno ativo
# (CLAUDE_CODE_DISABLE_UNKNOWN_MODEL_WINDOW_ENFORCEMENT=1 em ~/.claude/settings.json desabilita).
WAS_FORCED="false"
JSON_CTX_RAW=$(echo "$input" | jq -r '.context_window.context_window_size // empty' 2>/dev/null)

# 1. Acordo natural: se o JSON JÁ reporta 1M, usar diretamente (sem override).
if [ "$JSON_CTX_RAW" = "1000000" ]; then
    CONTEXT_SIZE=1000000
else
    # 2. JSON reporta outro valor (200k, vazio): tentar match por nome — se matcha, forçar 1M.
    case "$MODEL$MODEL_ID" in
        *MiniMax*|*M3*|*m3*|*minimax*|*opus\[1m\]*|*claude-opus-4-1*|*opus-4-1*|*\ \(1M\)*|*"1M"*) CONTEXT_SIZE=1000000; WAS_FORCED="true" ;;
        *) CONTEXT_SIZE="$JSON_CTX_RAW" ;;
    esac
fi

# 3. Fallback final: se mesmo assim vazio (modelo desconhecido + JSON vazio), 200k default.
if [ -z "$CONTEXT_SIZE" ]; then
    CONTEXT_SIZE=200000
fi

# WIN_BADGE — aparece SOMENTE quando OVERRIDE ativo (forçamos 1M contra JSON que dizia diferente).
if [ "$WAS_FORCED" = "true" ]; then
    WIN_BADGE=" ${DIM}[📐1M-unenforced]${RESET}"
else
    WIN_BADGE=""
fi
COST=$(echo "$input" | jq -r '.cost.total_cost_usd // 0' 2>/dev/null)
USAGE=$(echo "$input" | jq '.context_window.current_usage' 2>/dev/null)

# Calculate context percentage from current_usage (official method)
if [ "$USAGE" != "null" ] && [ -n "$USAGE" ]; then
    CURRENT_TOKENS=$(echo "$USAGE" | jq '(.input_tokens // 0) + (.cache_creation_input_tokens // 0) + (.cache_read_input_tokens // 0)' 2>/dev/null)
    CURRENT_TOKENS=${CURRENT_TOKENS:-0}
else
    CURRENT_TOKENS=0
fi

# Calculate percentage (with 1-decimal precision for display, integer for tier compare)
if [ "$CONTEXT_SIZE" -gt 0 ] 2>/dev/null; then
    CONTEXT_INT=$((CURRENT_TOKENS * 100 / CONTEXT_SIZE))
    CONTEXT_DECI=$((CURRENT_TOKENS * 1000 / CONTEXT_SIZE))   # 3-digit: 247 = 24.7%
    CONTEXT_DECI_INT=$((CONTEXT_DECI / 10))                   # floor(decimals)
    CONTEXT_DECI_REM=$((CONTEXT_DECI % 10))                   # first decimal digit
    CONTEXT_PCT_DISPLAY="${CONTEXT_DECI_INT}.${CONTEXT_DECI_REM}%"
else
    CONTEXT_INT=0
    CONTEXT_PCT_DISPLAY="0.0%"
fi

# One-decimal millions in pure bash, truncated like bc's `scale=1`. bc is not installed on this machine, and the old
# `$(… | bc || echo "$((n / 1000))K")M` fallback printed a 1M window as "1000KM" (17/10/2026).
fmt_millions() { printf '%d.%dM' "$(($1 / 1000000))" "$((($1 % 1000000) / 100000))"; }

# Format tokens (K for thousands)
if [ "$CURRENT_TOKENS" -ge 1000000 ] 2>/dev/null; then
    TOKENS_FMT="$(fmt_millions "$CURRENT_TOKENS")"
elif [ "$CURRENT_TOKENS" -ge 1000 ] 2>/dev/null; then
    TOKENS_FMT="$((CURRENT_TOKENS / 1000))K"
else
    TOKENS_FMT="${CURRENT_TOKENS}"
fi


# Format cost
if awk -v c="$COST" 'BEGIN { exit !(c + 0 > 0) }' 2>/dev/null; then
    COST_FMT="\$$(LC_NUMERIC=C printf '%.2f' "$COST" 2>/dev/null || echo "$COST")"
else
    COST_FMT=""
fi

# Progress bar (20 blocks — 5% cada; granularidade adequada para janelas 1M)
bar=""
filled=$((CONTEXT_INT / 5))
if [ "$filled" -gt 20 ]; then filled=20; fi
for ((i=0; i<20; i++)); do
    if [ $i -lt $filled ]; then bar+="█"; else bar+="░"; fi
done

# Cost segment (only if > $0)
COST_SEG=""
if [ -n "$COST_FMT" ]; then
    COST_SEG=" ${DIM}│${RESET} ${WHITE}${COST_FMT}${RESET}"
fi

# Franchise quota segment (18/10/2026): "5h 60% ↻1h04 · 7d 50%". The 5 h and 7 d windows come from the payload's
# `rate_limits` (Pro/Max only, after the session's first reply; each window may be missing alone). ABSENT IS NOT
# ZERO: a payload without a numeric reading for a window shows nothing for it — never "0%". The percentage is
# floored and capped at 100; a negative or non-number is junk and dropped. `resets_at` (epoch) only decorates the
# 5 h window and only while it is in the future and plausible for a 5 h window (<= 10 h).
printf -v QUOTA_NOW '%(%s)T' -1
quota_color() {  # $1 = integer percentage → the band's colour
    if [ "$1" -ge "$QUOTA_ALERT" ]; then printf '%s' "$RED"
    elif [ "$1" -ge "$QUOTA_WARN" ]; then printf '%s' "$YELLOW"
    else printf '%s' "$GREEN"; fi
}
fmt_countdown() {  # $1 = seconds left (> 0), rounded UP to the minute: 1h04, 42m
    local mins=$((($1 + 59) / 60))
    if [ "$mins" -ge 60 ]; then printf '%dh%02d' $((mins / 60)) $((mins % 60)); else printf '%dm' "$mins"; fi
}
QUOTA_FIELDS=$(echo "$input" | jq -r '
    def pct: if type == "number" and . >= 0 then (if . > 100 then 100 else . end | floor) else "-" end;
    def epoch: if type == "number" and . > 0 then floor else "-" end;
    (.rate_limits | objects) as $r
    | [ ((($r.five_hour | objects | .used_percentage) // null) | pct),
        ((($r.five_hour | objects | .resets_at) // null) | epoch),
        ((($r.seven_day | objects | .used_percentage) // null) | pct) ]
    | join(" ")' 2>/dev/null)
read -r Q5_PCT Q5_RESET Q7_PCT <<< "$QUOTA_FIELDS"
QUOTA_SEG=""
QUOTA_PARTS=""
if [[ "$Q5_PCT" =~ ^[0-9]{1,3}$ ]]; then
    QUOTA_PARTS="$(quota_color "$Q5_PCT")5h ${Q5_PCT}%${RESET}"
    if [[ "$Q5_RESET" =~ ^[0-9]{1,12}$ ]] && [ "$Q5_RESET" -gt "$QUOTA_NOW" ] && [ $((Q5_RESET - QUOTA_NOW)) -le 36000 ]; then
        QUOTA_PARTS+=" ${DIM}↻$(fmt_countdown $((Q5_RESET - QUOTA_NOW)))${RESET}"
    fi
fi
if [[ "$Q7_PCT" =~ ^[0-9]{1,3}$ ]]; then
    QUOTA_PARTS="${QUOTA_PARTS:+$QUOTA_PARTS ${DIM}·${RESET} }$(quota_color "$Q7_PCT")7d ${Q7_PCT}%${RESET}"
fi
if [ -n "$QUOTA_PARTS" ]; then
    QUOTA_SEG=" ${DIM}│${RESET} ${QUOTA_PARTS}"
fi

# Format total window (WINDOW_FMT) — used/total display, mirrors TOKENS_FMT formatting
if [ "$CONTEXT_SIZE" -ge 1000000 ] 2>/dev/null; then
    WINDOW_FMT="$(fmt_millions "$CONTEXT_SIZE")"
elif [ "$CONTEXT_SIZE" -ge 1000 ] 2>/dev/null; then
    WINDOW_FMT="$((CONTEXT_SIZE / 1000))K"
else
    WINDOW_FMT="${CONTEXT_SIZE}"
fi

# Per-session checkpoint flag: armed at CHECKPOINT, removed (with its delivered marker) once the
# context is back under it — that is, after the compaction — so the next cycle warns again.
SESSION=$(echo "$input" | jq -r '.session_id // empty' 2>/dev/null | tr -cd 'A-Za-z0-9_-')
if [ -n "$SESSION" ]; then
    printf -v NOW '%(%s)T' -1
    # Heartbeat (17/10/2026): written on EVERY render, armed or not. check_context.sh can then tell "under the
    # checkpoint" (fresh heartbeat, no flag) from "nobody is measuring" (no heartbeat: headless run or
    # subagent, the status line never renders) and "the sensor went quiet" (stale heartbeat). An absent flag
    # alone means none of those — absent signal is not zero.
    printf '%s %s\n' "$NOW" "$CONTEXT_INT" > "${ALERT_DIR}/${SESSION}.seen"
    if [ "$CONTEXT_INT" -ge "$CHECKPOINT" ]; then
        # Line 1 is the percentage (check_context.sh reads only that); line 2 stamps the render that armed it.
        printf '%s\nts=%s\n' "$CONTEXT_INT" "$NOW" > "${ALERT_DIR}/${SESSION}.pct"
    else
        rm -f "${ALERT_DIR}/${SESSION}.pct" "${ALERT_DIR}/${SESSION}.delivered"
    fi
    # Quota marker, same directory and the same arm/clear cycle (check_context.sh delivers it once per window). Line 1
    # is the 5 h percentage, line 2 `resets=<epoch>` — the window's identity, so a NEW window announces itself even
    # when no render happened under the threshold in between. Only a numeric 5 h reading moves it: a render that
    # carries none must not read as "the window reset" (absent is not zero).
    if [[ "$Q5_PCT" =~ ^[0-9]{1,3}$ ]]; then
        if [ "$Q5_PCT" -ge "$QUOTA_ALERT" ]; then
            QUOTA_KEY=$Q5_RESET
            [[ "$QUOTA_KEY" =~ ^[0-9]{1,12}$ ]] || QUOTA_KEY="-"
            printf '%s\nresets=%s\n' "$Q5_PCT" "$QUOTA_KEY" > "${ALERT_DIR}/${SESSION}.quota"
        else
            rm -f "${ALERT_DIR}/${SESSION}.quota" "${ALERT_DIR}/${SESSION}.quota.delivered"
        fi
    fi
fi

# Output tiers (08/10/2026): green < CHECKPOINT, yellow CHECKPOINT (save state to disk), red at
# AUTOCOMPACT (the compaction is imminent). Display shows used/total + percent of the FULL window.
# echo -e (NOT printf) para não interpretar '%' dentro de ${CONTEXT_PCT_DISPLAY}.
if [ "$CONTEXT_INT" -ge "$AUTOCOMPACT" ]; then
    echo -e "${BOLD}${BG_RED}${BRIGHT_WHITE} COMPACT ${RESET} ${BRIGHT_RED}${bar}${RESET} ${BOLD}${RED}${TOKENS_FMT}/${WINDOW_FMT} (${CONTEXT_PCT_DISPLAY})${RESET} ${DIM}│${RESET} ${BRIGHT_CYAN}${MODEL}${RESET}${WIN_BADGE}${COST_SEG}${QUOTA_SEG}"
elif [ "$CONTEXT_INT" -ge "$CHECKPOINT" ]; then
    echo -e "${BOLD}${YELLOW}SALVE${RESET} ${BRIGHT_YELLOW}${bar}${RESET} ${BOLD}${YELLOW}${TOKENS_FMT}/${WINDOW_FMT} (${CONTEXT_PCT_DISPLAY})${RESET} ${DIM}│${RESET} ${BRIGHT_CYAN}${MODEL}${RESET}${WIN_BADGE}${COST_SEG}${QUOTA_SEG}"
else
    echo -e "${BRIGHT_GREEN}${bar}${RESET} ${GREEN}${TOKENS_FMT}/${WINDOW_FMT} (${CONTEXT_PCT_DISPLAY})${RESET} ${DIM}│${RESET} ${BRIGHT_CYAN}${MODEL}${RESET}${WIN_BADGE}${COST_SEG}${QUOTA_SEG}"
fi
