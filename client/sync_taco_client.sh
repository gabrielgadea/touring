#!/usr/bin/env bash
# sync_taco_client.sh — install/refresh the TACO client layer into ~/.claude
# (Pln2 D6). The CANONICAL copy of the Touring rules/skills/agents lives HERE
# (versioned with the product); ~/.claude holds the installed client copy.
#
#   client/sync_taco_client.sh            # apply (rsync repo → ~/.claude)
#   client/sync_taco_client.sh --dry-run  # show what would change
#
# Scope is EXPLICIT (only the Touring-owned items) — never touches the user's
# constitution (CLAUDE.md), generic skills/rules, settings.json or hooks.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
DEST="${CLAUDE_HOME:-$HOME/.claude}"
DRY=""
[ "${1:-}" = "--dry-run" ] && DRY="--dry-run"

for pair in "rules:rules" "skills:skills" "agents:agents"; do
  src="${pair%%:*}"; dst="${pair##*:}"
  # --delete is scoped per-item below (never across the whole dest dir).
  echo "== $src → $DEST/$dst =="
  rsync -a $DRY --itemize-changes "$HERE/$src/" "$DEST/$dst/" | head -40
done

echo "sync_taco_client: done${DRY:+ (dry-run — nothing written)}"
