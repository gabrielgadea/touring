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
# L4: zero color literals in suite QML (hue comes from qs.Commons/ColorRoles)
if grep -rnE '"#[0-9A-Fa-f]{3,8}"' "$SUITE_ROOT/plugins" "$SUITE_ROOT/shared" --include="*.qml" 2>/dev/null; then
  echo "FAIL color literal found in QML (L4 — derive from ColorRoles/Theme)"
  fail=1
fi
# L1: vendored copies must match shared/ sources (drift check)
if ! bash "$SUITE_ROOT/scripts/sync-taco-vendored" --check; then
  fail=1
fi
exit $fail
