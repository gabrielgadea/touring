#!/usr/bin/env bash
# guard_settings.sh — FileChanged hook: settings.json integrity guard.
#
# Copied from ~/.claude/hooks/guard_settings.sh into client/hooks/ (S48).
# Source of truth is now this file; ~/.claude/hooks/guard_settings.sh is a symlink.

SETTINGS="$HOME/.claude/settings.json"
BACKUP="$HOME/.claude/settings.json.touring-backup"
REQUIRED_EVENTS='["PreToolUse","PostToolUse","SessionStart","SessionEnd","PostToolUseFailure","PostCompact","InstructionsLoaded"]'

FILE_PATH=$(python3 -c "import sys,json; print(json.load(sys.stdin).get('file_path',''))" 2>/dev/null <<< "$(cat)" || echo "")
case "$FILE_PATH" in
  *settings.json) ;;
  *) exit 0 ;;
esac

is_valid_settings() {
  local file="$1"
  [ -f "$file" ] || return 1
  python3 - "$file" "$REQUIRED_EVENTS" 2>/dev/null <<'PYEOF'
import sys, json
settings_file = sys.argv[1]
required_events = json.loads(sys.argv[2])
try:
    with open(settings_file) as f:
        data = json.load(f)
except (json.JSONDecodeError, OSError):
    sys.exit(1)
hooks = data.get("hooks", {})
if not isinstance(hooks, dict):
    sys.exit(1)
missing = [e for e in required_events if e not in hooks]
if missing:
    sys.exit(1)
sys.exit(0)
PYEOF
}

if is_valid_settings "$SETTINGS"; then
  cp "$SETTINGS" "$BACKUP" 2>/dev/null
  exit 0
fi

if [ -f "$BACKUP" ] && is_valid_settings "$BACKUP"; then
  cp "$BACKUP" "$SETTINGS" 2>/dev/null
  echo '{"additionalContext": "⚠️ settings.json was structurally invalid — auto-restored from backup."}'
else
  echo '{"additionalContext": "⚠️ settings.json is structurally invalid and no valid backup available."}'
fi
exit 0
