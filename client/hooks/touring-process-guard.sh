#!/usr/bin/env bash
# touring-process-guard.sh — PreToolUse Bash hook: REGRA #19 enforcement.
#
# Copied verbatim from ~/.claude/hooks/touring-process-guard.sh into client/hooks/ (S48).
# Source of truth is now this file; ~/.claude/hooks/touring-process-guard.sh is a symlink.
#
# Canonical rule: ~/.claude/rules/touring-process-hygiene.md

set -euo pipefail

if [[ "${TOURING_PROCESS_GUARD_DISABLED:-0}" == "1" || "${TACO_FORGE_GUARD_DISABLED:-0}" == "1" ]]; then
  exit 0
fi

EVENT="${1:-unknown}"
trap 'rc=$?; if [[ "$rc" -ne 0 && "$rc" -ne 2 ]]; then exit 0; fi' EXIT

emit_block() {
  local pattern="$1" target="$2" suggestion="$3"
  printf '\n[touring-process-guard] BLOCK: %s\n' "$pattern" >&2
  printf '  Target:     %s\n' "$target" >&2
  printf '  Suggestion: %s\n' "$suggestion" >&2
  printf '  Rule:       ~/.claude/rules/touring-process-hygiene.md REGRA #19\n' >&2
  printf '  Bypass:     export TOURING_PROCESS_GUARD_DISABLED=1 (human-only)\n' >&2
  exit 2
}

read_tool_input_field() {
  local field="$1" stdin="$2"
  echo "$stdin" | jq -r --arg f "$field" '.tool_input[$f] // ""' 2>/dev/null || echo ""
}

check_bash() {
  local cmd
  cmd=$(read_tool_input_field "command" "$STDIN")
  [[ -z "$cmd" ]] && exit 0
  if [[ "$cmd" =~ (pkill|killall)([[:space:]]+-[^[:space:]]+)*[[:space:]]+([\"\']?)([^[:space:]\"\']*touring[^[:space:]\"\']*) ]]; then
    emit_block \
      "Bash command pkill/killall targets touring process(es) — REGRA #19 violation" \
      "${BASH_REMATCH[4]}" \
      "touring daemon-ctl restart   # canonical"
  fi
  if [[ "$cmd" =~ kill([[:space:]]+-[^[:space:]]+)*[[:space:]]+\$\([[:space:]]*pgrep[[:space:]][^\)]*touring ]]; then
    emit_block \
      "Bash command kills PIDs from \$(pgrep ... touring) — REGRA #19 violation" \
      "touring-*" \
      "touring daemon-ctl restart"
  fi
  exit 0
}

STDIN=""
if [[ -p /dev/stdin ]] || [[ ! -t 0 ]]; then
  STDIN=$(cat 2>/dev/null || true)
fi
[[ -z "$STDIN" ]] && exit 0

case "$EVENT" in
  Bash)  check_bash ;;
  *)     exit 0 ;;
esac
exit 0
