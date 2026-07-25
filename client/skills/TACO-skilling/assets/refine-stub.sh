#!/usr/bin/env bash
# refine-stub.sh — hybrid refinement stub embedded by TACO-skilling.
#
# Every skill TACO-skilling generates carries a copy of this stub in its
# scripts/ directory. It records local usage telemetry and points refinement
# back to the central engine in ~/.claude/skills/TACO-skilling/ — it never
# duplicates that engine. One engine, every skill (Rule #3 / REGRA #13).
#
# Usage:
#   refine-stub.sh log [note]   append a usage record to <skill>/.usage.log
#   refine-stub.sh refine       print how to invoke the central refine engine
#   refine-stub.sh stats        summarize recorded usage (default)
#
# Exit codes: 0 ok | 2 usage error.

set -eu

SKILL_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SKILL_NAME="$(basename "$SKILL_DIR")"
USAGE_LOG="${SKILL_DIR}/.usage.log"
ENGINE="${HOME}/.claude/skills/TACO-skilling"

cmd="${1:-stats}"

case "$cmd" in
  log)
    note="${2:-}"
    printf '%s\t%s\t%s\n' \
      "$(date -Iseconds 2>/dev/null || date)" \
      "${CLAUDE_SESSION_ID:-unknown}" \
      "$note" >> "$USAGE_LOG"
    echo "refine-stub: logged usage of ${SKILL_NAME}"
    ;;
  refine)
    echo "To refine '${SKILL_NAME}', run TACO-skilling in REFINE mode:"
    echo "  ask Claude:  refine the ${SKILL_NAME} skill"
    echo "  engine:      ${ENGINE}/SKILL.md"
    if [ -f "$USAGE_LOG" ]; then
      echo "  telemetry:   ${USAGE_LOG} ($(wc -l < "$USAGE_LOG" | tr -d ' ') records)"
    else
      echo "  telemetry:   none recorded yet"
    fi
    ;;
  stats)
    if [ -f "$USAGE_LOG" ]; then
      n="$(wc -l < "$USAGE_LOG" | tr -d ' ')"
      echo "${SKILL_NAME}: ${n} usage record(s)"
      if [ "$n" -gt 0 ]; then
        echo "  first: $(head -n1 "$USAGE_LOG" | cut -f1)"
        echo "  last:  $(tail -n1 "$USAGE_LOG" | cut -f1)"
      fi
    else
      echo "${SKILL_NAME}: no usage recorded yet"
    fi
    ;;
  -h|--help)
    grep '^#' "$0" | sed 's/^#\{1,\} \{0,1\}//'
    ;;
  *)
    echo "usage: refine-stub.sh {log [note]|refine|stats}" >&2
    exit 2
    ;;
esac
