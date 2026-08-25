#!/bin/bash
# validate_all.sh — validate every plugin manifest in the suite (P0 scaffold; P1 adds
# qmllint sweep + vendored drift check + boundary check).
set -euo pipefail
SUITE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fail=0
for manifest in "$SUITE_ROOT"/plugins/*/manifest.json; do
  plugin_dir="$(dirname "$manifest")"
  if omarchy plugin validate "$plugin_dir" >/dev/null 2>&1; then
    echo "OK   $(basename "$plugin_dir")"
  else
    echo "FAIL $(basename "$plugin_dir")"
    omarchy plugin validate "$plugin_dir" 2>&1 | head -5 || true
    fail=1
  fi
done
# No symlinks in any payload (self-contained law)
if find "$SUITE_ROOT/plugins" -type l | grep -q .; then
  echo "FAIL symlinks detected in plugin payloads:"
  find "$SUITE_ROOT/plugins" -type l
  fail=1
fi
exit $fail
