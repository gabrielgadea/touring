#!/bin/bash
# taco-cache-refresh.sh — single-flight, content-validated cache refresh (L7).
# Usage: taco-cache-refresh.sh <name> <cacheRoot> <cmd...>
#   <cmd...> output (stdout JSON) → <cacheRoot>/<name>.json atomically;
#   freshness stamp (UTC or "FAIL") → <cacheRoot>/<name>.stamp.
#
# Single-flight via flock on <cacheRoot>/.<name>.lock: overlapping refreshes
# skip silently (a skip is NOT a failure — the in-flight refresh owns the stamp).
# Success is validated by CONTENT (non-empty + jq parses), not exit code —
# a degraded composite may exit non-zero with valid partial JSON.
set -u

name="$1"; root="$2"; shift 2
mkdir -p "$root"
json="$root/$name.json"
tmp="$json.tmp"
stamp="$root/$name.stamp"
lock="$root/.$name.lock"

exec 9>"$lock"
flock -n 9 || exit 0

"$@" > "$tmp" 2>/dev/null
if [ -s "$tmp" ] && jq -e . "$tmp" >/dev/null 2>&1; then
  mv "$tmp" "$json" && date -u +%FT%TZ > "$stamp"
else
  rm -f "$tmp"
  echo FAIL > "$stamp"
fi
