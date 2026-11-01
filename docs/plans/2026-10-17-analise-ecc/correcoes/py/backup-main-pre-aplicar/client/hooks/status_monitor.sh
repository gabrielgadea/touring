#!/bin/bash
# =============================================================================
# Claude Code Context Monitor - StatusLine Script
# Reads official JSON input from Claude Code statusLine API
# Fields: model, cost, context_window (with current_usage), workspace
# =============================================================================

# 08/10/2026 (Gabriel: "autocompactar aos 65%"): auto-compaction fires at
# CLAUDE_AUTOCOMPACT_PCT_OVERRIDE of the window; the checkpoint warning arms CHECKPOINT points
# before it, PER SESSION (check_context.sh speaks once per cycle). Replaces CONTEXT_THRESHOLD (95)
# and the global ~/.claude/context_alert flag, which a 65% compaction would never have reached.
AUTOCOMPACT=${CLAUDE_AUTOCOMPACT_PCT_OVERRIDE:-65}
CHECKPOINT=${CONTEXT_CHECKPOINT_PCT:-$((AUTOCOMPACT - 10))}
ALERT_DIR="${HOME}/.claude/touring/context_alert"

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
    printf "${DIM}jq not installed${RESET}\n"
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
# Diag: WIN_SRC ∈ {model,json,fallback}, WAS_FORCED bool — indica se OVERRIDE ativo.
# Quando WAS_FORCED=true + janela 1M, Claude Code pode ter um clamp interno ativo
# (CLAUDE_CODE_DISABLE_UNKNOWN_MODEL_WINDOW_ENFORCEMENT=1 em ~/.claude/settings.json desabilita).
WIN_SRC="fallback"
WAS_FORCED="false"
JSON_CTX_RAW=$(echo "$input" | jq -r '.context_window.context_window_size // empty' 2>/dev/null)

# 1. Acordo natural: se o JSON JÁ reporta 1M, usar diretamente (sem override).
if [ "$JSON_CTX_RAW" = "1000000" ]; then
    CONTEXT_SIZE=1000000
    WIN_SRC="json"
else
    # 2. JSON reporta outro valor (200k, vazio): tentar match por nome — se matcha, forçar 1M.
    case "$MODEL$MODEL_ID" in
        *MiniMax*|*M3*|*m3*|*minimax*|*opus\[1m\]*|*claude-opus-4-1*|*opus-4-1*|*\ \(1M\)*|*"1M"*) CONTEXT_SIZE=1000000; WIN_SRC="model"; WAS_FORCED="true" ;;
        *) CONTEXT_SIZE="$JSON_CTX_RAW"; WIN_SRC="json" ;;
    esac
fi

# 3. Fallback final: se mesmo assim vazio (modelo desconhecido + JSON vazio), 200k default.
if [ -z "$CONTEXT_SIZE" ]; then
    CONTEXT_SIZE=200000
    WIN_SRC="fallback"
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

# Format tokens (K for thousands)
if [ "$CURRENT_TOKENS" -ge 1000000 ] 2>/dev/null; then
    TOKENS_FMT="$(echo "scale=1; $CURRENT_TOKENS / 1000000" | bc 2>/dev/null || echo "$((CURRENT_TOKENS / 1000))K")M"
elif [ "$CURRENT_TOKENS" -ge 1000 ] 2>/dev/null; then
    TOKENS_FMT="$((CURRENT_TOKENS / 1000))K"
else
    TOKENS_FMT="${CURRENT_TOKENS}"
fi

# Format capacity (sempre K ou M para janelas grandes)
if [ "$CONTEXT_SIZE" -ge 1000000 ] 2>/dev/null; then
    SIZE_FMT="$((CONTEXT_SIZE / 1000000))M"
elif [ "$CONTEXT_SIZE" -ge 1000 ] 2>/dev/null; then
    SIZE_FMT="$((CONTEXT_SIZE / 1000))K"
else
    SIZE_FMT="$CONTEXT_SIZE"
fi

# Uso absoluto X/YM (ancla o tamanho real da janela no indicador)
USAGE_ABS="${TOKENS_FMT}/${SIZE_FMT}"

# Format cost
if [ "$(echo "$COST > 0" | bc 2>/dev/null)" = "1" ]; then
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

# Format total window (WINDOW_FMT) — used/total display, mirrors TOKENS_FMT formatting
if [ "$CONTEXT_SIZE" -ge 1000000 ] 2>/dev/null; then
    WINDOW_FMT="$(echo "scale=1; $CONTEXT_SIZE / 1000000" | bc 2>/dev/null || echo "$((CONTEXT_SIZE / 1000))K")M"
elif [ "$CONTEXT_SIZE" -ge 1000 ] 2>/dev/null; then
    WINDOW_FMT="$((CONTEXT_SIZE / 1000))K"
else
    WINDOW_FMT="${CONTEXT_SIZE}"
fi

# Per-session checkpoint flag: armed at CHECKPOINT, removed (with its delivered marker) once the
# context is back under it — that is, after the compaction — so the next cycle warns again.
SESSION=$(echo "$input" | jq -r '.session_id // empty' 2>/dev/null | tr -cd 'A-Za-z0-9_-')
if [ -n "$SESSION" ]; then
    if [ "$CONTEXT_INT" -ge "$CHECKPOINT" ]; then
        echo "$CONTEXT_INT" > "${ALERT_DIR}/${SESSION}.pct"
    else
        rm -f "${ALERT_DIR}/${SESSION}.pct" "${ALERT_DIR}/${SESSION}.delivered"
    fi
fi

# Output tiers (08/10/2026): green < CHECKPOINT, yellow CHECKPOINT (save state to disk), red at
# AUTOCOMPACT (the compaction is imminent). Display shows used/total + percent of the FULL window.
# echo -e (NOT printf) para não interpretar '%' dentro de ${CONTEXT_PCT_DISPLAY}.
if [ "$CONTEXT_INT" -ge "$AUTOCOMPACT" ]; then
    echo -e "${BOLD}${BG_RED}${BRIGHT_WHITE} COMPACT ${RESET} ${BRIGHT_RED}${bar}${RESET} ${BOLD}${RED}${TOKENS_FMT}/${WINDOW_FMT} (${CONTEXT_PCT_DISPLAY})${RESET} ${DIM}│${RESET} ${BRIGHT_CYAN}${MODEL}${RESET}${WIN_BADGE}${COST_SEG}"
elif [ "$CONTEXT_INT" -ge "$CHECKPOINT" ]; then
    echo -e "${BOLD}${YELLOW}SALVE${RESET} ${BRIGHT_YELLOW}${bar}${RESET} ${BOLD}${YELLOW}${TOKENS_FMT}/${WINDOW_FMT} (${CONTEXT_PCT_DISPLAY})${RESET} ${DIM}│${RESET} ${BRIGHT_CYAN}${MODEL}${RESET}${WIN_BADGE}${COST_SEG}"
else
    echo -e "${BRIGHT_GREEN}${bar}${RESET} ${GREEN}${TOKENS_FMT}/${WINDOW_FMT} (${CONTEXT_PCT_DISPLAY})${RESET} ${DIM}│${RESET} ${BRIGHT_CYAN}${MODEL}${RESET}${WIN_BADGE}${COST_SEG}"
fi
