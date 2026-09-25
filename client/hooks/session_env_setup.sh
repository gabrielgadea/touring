#!/bin/bash
# session_env_setup.sh — S4.1: CLAUDE_ENV_FILE setup for Touring sessions.
#
# PURPOSE: Persist Touring environment variables into CLAUDE_ENV_FILE so all
# Bash commands in the session have access to TOURING_* vars.
#
# NOTE: This hook does NOT call touring-hook session-start.
# The dedicated "touring-hook session-start" hook in settings.json handles that.
# Previously this script duplicated that call, causing double DB writes and
# empty-stdin on the second invocation. Fixed 2026-04-03.

INPUT=$(cat)

if [ -n "$CLAUDE_ENV_FILE" ]; then
    SESSION_ID=$(echo "$INPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('session_id','unknown'))" 2>/dev/null || echo "unknown")

    # Write (overwrite) TOURING_* vars — do not append to avoid accumulation
    # Use a temp file + mv for atomicity
    TMPFILE=$(mktemp "${CLAUDE_ENV_FILE}.tmp.XXXXXX")

    # Preserve any non-TOURING lines already in CLAUDE_ENV_FILE
    if [ -f "$CLAUDE_ENV_FILE" ]; then
        grep -v "^export TOURING_" "$CLAUDE_ENV_FILE" > "$TMPFILE" 2>/dev/null || true
    fi

    {
        echo "export TOURING_SESSION_ID=\"$SESSION_ID\""
        echo "export TOURING_DAEMON_SOCKET=\"/tmp/touring-daemon-$(id -u).sock\""
        echo "export TOURING_PROJECT_ROOT=\"$(pwd)\""
    } >> "$TMPFILE"

    mv "$TMPFILE" "$CLAUDE_ENV_FILE"
fi

exit 0
