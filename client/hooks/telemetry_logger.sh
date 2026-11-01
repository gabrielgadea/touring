#!/usr/bin/env bash
# telemetry_logger.sh — PostToolUse hook: per-event NDJSON telemetry log.
#
# S43 (plan 2026-09-25): replace 3 python3 subprocess calls with a single jq -c invocation.
# Previous cost: 3 python3 processes per every tool call (tool_name + session_id + event).
# Now: 1 jq call extracts all fields + builds the JSON line.
#
# Contract: PostToolUse (*), async. Exit 0 always.

TELEMETRY_FILE="${HOME}/.claude/touring-telemetry.ndjson"
MAX_SIZE_BYTES=10485760  # 10 MB rotation threshold

# Read input from stdin (Claude Code hook JSON).
INPUT=$(cat)

# Single jq call: extract all needed fields and emit an NDJSON line.
# jq -c ensures compact (single-line) output. Strings are properly escaped by jq.
ENTRY=$(printf '%s' "${INPUT}" | jq -c \
    --arg ts "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
    --arg epoch "$(date +%s)" \
    '{
        ts: $ts,
        epoch: ($epoch | tonumber),
        event: (.hook_event_name // "PostToolUse"),
        tool: (.tool_name // "unknown"),
        session: (.session_id // "unknown")
    }' 2>/dev/null)

# If jq failed (malformed input), produce a minimal fallback entry.
if [[ -z "${ENTRY}" ]]; then
    ENTRY="{\"ts\":\"$(date -u +"%Y-%m-%dT%H:%M:%SZ")\",\"epoch\":$(date +%s),\"event\":\"PostToolUse\",\"tool\":\"unknown\",\"session\":\"unknown\"}"
fi

# Rotate if file exceeds max size.
if [[ -f "${TELEMETRY_FILE}" ]]; then
    FILE_SIZE=$(stat -c%s "${TELEMETRY_FILE}" 2>/dev/null || echo 0)
    if [[ "${FILE_SIZE}" -gt "${MAX_SIZE_BYTES}" ]]; then
        mv "${TELEMETRY_FILE}" "${TELEMETRY_FILE}.old"
    fi
fi

printf '%s\n' "${ENTRY}" >> "${TELEMETRY_FILE}"
exit 0
