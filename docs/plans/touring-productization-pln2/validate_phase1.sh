#!/usr/bin/env bash
# validate_phase1.sh — Productization Fase 1 (W12.5 daemon multi-instância) gate.
# Plan: ~/.claude/plans/giggly-drifting-kahn.md (§ Fase 1). 8 checks:
#  (a) E2E: two per-project daemons coexist + same-socket race idempotent (RED→GREEN)
#  (b) foundation unit tests: lock derivation, opt-in, legacy env
#  (c) daemon-ctl list-all runs and emits parseable JSON
#  (d) global daemon unaffected (socket alive from the CLI's view)
#  (e) clippy -D warnings on the 5 touched crates
#  (f) opt-in documented in the init-project template (default OFF)
#  (g) canonical PID file written by a post-upgrade global daemon (REGRA #19 gap)
#  (h) update-touring deployed the new binaries (exe = new root, not "(deleted)")
set -uo pipefail
WS="${TOURING_WS:-$HOME/projects/touring}"
cd "$WS" || { echo "FAIL: workspace $WS unreadable"; exit 3; }
fail=0

echo "── (a) W12.5 E2E (multi-daemon coexistence + idempotent race)"
# Capture-then-grep: `producer | grep -q` under pipefail dies with SIGPIPE 141
# when grep exits early (the exact validate_phase4 check-(a) bug, reintroduced
# and re-caught here on 2026-07-24 — multi-block cargo test output).
out=$(cargo test -q -p touring-server --test w12_5_per_project_daemon_e2e 2>/dev/null || true)
if echo "$out" | grep -q "3 passed; 0 failed"; then
  echo "  PASS (3/3 — incl. cross-cwd routing)"; else echo "  FAIL"; fail=1; fi

echo "── (b) foundation W12.5 unit tests"
out=$(cargo test --release -q -p touring-foundation w12_5 2>/dev/null || true)
if echo "$out" | grep -q "4 passed; 0 failed"; then
  echo "  PASS (4/4)"; else echo "  FAIL"; fail=1; fi

echo "── (c) daemon-ctl list-all"
if touring daemon-ctl list-all -j 2>/dev/null | python3 -c '
import json,sys
d=json.load(sys.stdin); sys.exit(0 if "daemons" in d else 1)'; then
  echo "  PASS"; else echo "  FAIL"; fail=1; fi

echo "── (d) global daemon alive"
if touring daemon-ctl status 2>&1 | grep -q "daemon PID: [0-9]"; then
  echo "  PASS"; else echo "  FAIL"; fail=1; fi

echo "── (e) clippy -D warnings (5 touched crates)"
errs=$(cargo clippy --release -q -p touring-foundation -p touring-hooks-core \
  -p touring-dispatch -p touring-server -p touring-server-reasoning \
  -- -D warnings 2>&1 | grep -cE "^error")
if [ "$errs" -eq 0 ]; then echo "  PASS (0 errors)"; else echo "  FAIL ($errs)"; fail=1; fi

echo "── (f) opt-in documented, default OFF"
if grep -q "# per_project = true" crates/touring-server/src/cli/init_project.rs; then
  echo "  PASS"; else echo "  FAIL"; fail=1; fi

echo "── (g) canonical PID file (post-deploy daemon)"
pidf="/run/user/$(id -u)/touring-daemon.pid"
pid=$(cat "$pidf" 2>/dev/null)
if [ -n "$pid" ] && [ "$(cat /proc/"$pid"/comm 2>/dev/null)" = "touring-daemon" ]; then
  echo "  PASS ($pidf → $pid)"; else echo "  FAIL (empty/stale $pidf)"; fail=1; fi

echo "── (h) deployed binaries from the canonical root"
pid=$(touring daemon-ctl status 2>&1 | grep -oE 'daemon PID: [0-9]+' | grep -oE '[0-9]+')
exe=$(readlink /proc/"$pid"/exe 2>/dev/null)
if [ "$exe" = "$WS/target/release/touring-daemon" ]; then
  echo "  PASS ($exe)"; else echo "  FAIL (exe=$exe)"; fail=1; fi

if [ "$fail" -eq 0 ]; then echo "✅ PHASE 1 VALIDATE: ALL PASS"; exit 0
else echo "❌ PHASE 1 VALIDATE: FAILURES ABOVE"; exit 1; fi
