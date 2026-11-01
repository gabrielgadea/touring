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

# D3 (analise-92, 06/10/2026): the socket follows the SAME three layers as the Rust resolver
# `TouringConfig::resolve_daemon_socket_path` (touring-foundation/src/config/paths.rs). Until then this
# script wrote the global socket unconditionally, so a per-project session (analise: `[daemon]
# per_project = true` + the socket in settings.local.json) sent every CLI call from Bash to the global
# daemon while its hooks went to the project's — two daemons over one symbols.db.
per_project_opt_in() {
    [ -f "$1/.touring/touring.toml" ] || return 1
    python3 -c 'import sys, tomllib
d = tomllib.load(open(sys.argv[1], "rb")).get("daemon") or {}
sys.exit(0 if d.get("per_project") is True else 1)' "$1/.touring/touring.toml" 2>/dev/null
}

resolve_daemon_socket() {
    # Layer 1: an explicit override wins (canonical name first, legacy second).
    if [ -n "$TOURING_DAEMON_SOCKET" ]; then echo "$TOURING_DAEMON_SOCKET"; return; fi
    if [ -n "$TOURING_DAEMON_SOCK" ]; then echo "$TOURING_DAEMON_SOCK"; return; fi
    # Layer 2: walk up from the project dir (or cwd) — a bound socket, or the per-project opt-in.
    local d="${CLAUDE_PROJECT_DIR:-$(pwd)}"
    while :; do
        if [ -e "$d/.touring/daemon.sock" ] || per_project_opt_in "$d"; then
            echo "$d/.touring/daemon.sock"
            return
        fi
        [ "$d" = "/" ] && break
        d=$(dirname "$d")
    done
    # Layer 3: the global daemon.
    echo "/tmp/touring-daemon-$(id -u).sock"
}

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
        echo "export TOURING_DAEMON_SOCKET=\"$(resolve_daemon_socket)\""
        echo "export TOURING_PROJECT_ROOT=\"${TOURING_PROJECT_ROOT:-$(pwd)}\""
    } >> "$TMPFILE"

    mv "$TMPFILE" "$CLAUDE_ENV_FILE"
fi

exit 0
