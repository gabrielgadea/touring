#!/usr/bin/env bash
# NEW-5 — Cross-agent shell hook for Touring sandbox routing.
#
# Universal Bash interceptor: when invoked with a Sandbox<Lang> tool name
# pattern, delegates to `touring sandbox-execute`; otherwise passes through
# to the original argv unchanged. Brings Touring's compression + indexing
# surface to any AI agent that supports a Bash pre-tool hook
# (Claude Code, Gemini CLI, Codex, Cursor, Cline, etc.).
#
# Usage (via init):
#   touring init --agent gemini    # writes hook to ~/.config/gemini/...
#   touring init --agent codex     # writes hook to ~/.codex/...
#
# Manual:
#   touring-rewrite.sh SandboxPython "print(1)"   # → touring sandbox-execute
#   touring-rewrite.sh ls -la                      # → ls -la (passthrough)
#
# Bypass:
#   export TOURING_REWRITE_DISABLED=1

set -euo pipefail

# Bypass shortcut
if [ "${TOURING_REWRITE_DISABLED:-0}" = "1" ]; then
    exec "$@"
fi

# Detect Sandbox<Lang> pattern in first argument
case "${1:-}" in
    Sandbox*)
        # Delegate to touring sandbox-execute (deferred — until CLI lands,
        # passthrough as a safe default rather than failing)
        if command -v touring >/dev/null 2>&1; then
            # Future: exec touring sandbox-execute "$@"
            # Today: passthrough with warn (telemetry only)
            echo "[touring-rewrite] info: Sandbox pattern detected, sandbox-execute CLI deferred" >&2
            exec "$@"
        else
            echo "[touring-rewrite] warn: touring not in PATH, passthrough" >&2
            exec "$@"
        fi
        ;;
    *)
        # Passthrough — exec preserves exit code
        exec "$@"
        ;;
esac
