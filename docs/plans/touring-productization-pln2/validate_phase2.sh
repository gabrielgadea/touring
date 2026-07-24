#!/usr/bin/env bash
# validate_phase2.sh — Productization Fase 2 (install lifecycle) gate.
# Plan: ~/.claude/plans/giggly-drifting-kahn.md (§ Fase 2). 7 checks:
#  (a) init-project populates .touring/bin (executable, dev-channel fallback)
#  (b) shim resolves the PROJECT bin (layer 2) under CLAUDE_PROJECT_DIR
#  (c) shim resolves the dev channel (layer 4) outside any project
#  (d) ~/.claude/hooks/touring-hook IS the shim (not a raw symlink)
#  (e) the installed hook answers a real CC-style invocation with exit 0
#  (f) settings.json is valid JSON
#  (g) populate_bin unit tests green (pinned toolchain + fallback + fail-open)
set -uo pipefail
WS="${TOURING_WS:-$HOME/projects/touring}"
cd "$WS" || { echo "FAIL: workspace $WS unreadable"; exit 3; }
fail=0
proj=$(mktemp -d /tmp/vp2-proj-XXXX)

echo "── (a) init-project populates .touring/bin"
(cd "$proj" && touring init-project >/dev/null 2>&1)
if [ -x "$proj/.touring/bin/touring-hook" ]; then
  echo "  PASS ($(readlink "$proj/.touring/bin/touring-hook"))"
else echo "  FAIL (bin not populated)"; fail=1; fi

echo "── (b) shim layer 2: per-project bin wins inside the project"
t=$(echo '{}' | TOURING_HOOK_SHIM_TRACE=1 CLAUDE_PROJECT_DIR="$proj" \
    "$HOME/.claude/hooks/touring-hook" instructions-loaded 2>&1 >/dev/null || true)
if echo "$t" | grep -q "project_bin: $proj/.touring/bin/touring-hook"; then
  echo "  PASS"; else echo "  FAIL: $t"; fail=1; fi

echo "── (c) shim layer 4: dev channel outside any project"
t=$(echo '{}' | TOURING_HOOK_SHIM_TRACE=1 CLAUDE_PROJECT_DIR=/tmp \
    "$HOME/.claude/hooks/touring-hook" instructions-loaded 2>&1 >/dev/null || true)
if echo "$t" | grep -q "global_bin: $HOME/.local/bin/touring-hook"; then
  echo "  PASS"; else echo "  FAIL: $t"; fail=1; fi

echo "── (d) installed hook IS the walk-up shim"
if [ ! -L "$HOME/.claude/hooks/touring-hook" ] \
   && head -2 "$HOME/.claude/hooks/touring-hook" | grep -q "touring-hook-shim"; then
  echo "  PASS"; else echo "  FAIL (raw symlink or wrong content)"; fail=1; fi

echo "── (e) real CC-style invocation exits 0 (fail-open contract)"
echo '{}' | /bin/sh -c "\$HOME/.claude/hooks/touring-hook instructions-loaded" >/dev/null 2>&1
rc=$?
if [ "$rc" -eq 0 ]; then echo "  PASS"; else echo "  FAIL (exit $rc)"; fail=1; fi

echo "── (f) settings.json valid"
if python3 -c "import json; json.load(open('$HOME/.claude/settings.json'))" 2>/dev/null; then
  echo "  PASS"; else echo "  FAIL"; fail=1; fi

echo "── (g) populate_bin unit tests"
out=$(cargo test --release -q -p touring-server --lib init_project 2>/dev/null || true)
if echo "$out" | grep -q "11 passed; 0 failed"; then
  echo "  PASS (11/11)"; else echo "  FAIL"; fail=1; fi

if [ "$fail" -eq 0 ]; then echo "✅ PHASE 2 VALIDATE: ALL PASS"; exit 0
else echo "❌ PHASE 2 VALIDATE: FAILURES ABOVE"; exit 1; fi
