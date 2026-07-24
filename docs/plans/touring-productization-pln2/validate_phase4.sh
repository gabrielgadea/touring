#!/usr/bin/env bash
# validate_phase4.sh — Productization Fase 4 (F4' move-first) cross-audit gate.
# Plan: ~/.claude/plans/giggly-drifting-kahn.md (§ Fase 4) + strategy-2026-07-23-retomada.md
# Runs AFTER the cutover (F4-4/F4-5). 8 checks:
#  (a) update-touring resolves the workspace via TOURING_WORKSPACE_ROOT (env-parametrized)
#  (b) ~/.local/bin + ~/.claude/hooks symlinks resolve into the NEW release dir
#  (c) live daemon exe is the NEW binary, not "(deleted)"
#  (d) touring doctor: daemon_socket + daemon_health ok
#  (e) zero unexpected runtime hardcodes of the historical root in crates/ (phase0 re-audit from the new root)
#  (f) co-evolution: settings.json + CLAUDE.md + disk-watch TARGETS reference the new root
#  (g) binary --version == [workspace.package] version (single source, new build)
#  (h) frozen copy intact: ~/.claude/rust still present (D4 — discard is Gabriel's future call)
set -uo pipefail
NEW_WS="${TOURING_WS:-$HOME/projects/touring}"
OLD_WS="$HOME/.claude/rust"
BIN="$NEW_WS/target/release/touring"
cd "$NEW_WS" || { echo "FAIL: workspace $NEW_WS unreadable"; exit 3; }
fail=0

echo "── (a) update-touring is env-parametrized (no hardcoded workspace)"
# Capture the trace in a variable: `bash -x | grep -q` under pipefail dies
# with SIGPIPE 141 when grep exits early (false FAIL observed 2026-07-24).
trace=$(TOURING_WORKSPACE_ROOT=/tmp/vp4-fake bash -x "$HOME/.local/bin/update-touring" --verify-only 2>&1 || true)
if grep -q 'TOURING_WORKSPACE_ROOT' "$HOME/.local/bin/update-touring" \
   && echo "$trace" | grep -q 'RUST_WORKSPACE=/tmp/vp4-fake'; then
  echo "  PASS"; else echo "  FAIL (script not env-resolved)"; fail=1; fi

echo "── (b) symlinks resolve into the new release dir"
ok=0; total=0
for d in "$HOME/.local/bin" "$HOME/.claude/hooks"; do
  for b in touring touring-hook touring-daemon; do
    total=$((total+1))
    [ "$(readlink -f "$d/$b" 2>/dev/null)" = "$NEW_WS/target/release/$b" ] && ok=$((ok+1))
  done
done
if [ "$ok" -eq "$total" ]; then echo "  PASS ($ok/$total)"; else echo "  FAIL ($ok/$total point to new root)"; fail=1; fi

echo "── (c) live daemon runs the new binary (no stale inode)"
# Primary source: daemon-ctl status (the REGRA #19 canonical helper) — the
# pid file was observed EMPTY on 2026-07-24 (gap queued for Fase 1 / W12.5).
# NB: daemon-ctl status prints to STDERR (measured 2026-07-24) — merge streams.
pid=$(touring daemon-ctl status 2>&1 | grep -oE 'daemon PID: [0-9]+' | grep -oE '[0-9]+')
[ -z "$pid" ] && pid=$(cat /run/user/$(id -u)/touring-daemon.pid 2>/dev/null || cat /tmp/touring-daemon-$(id -u).pid 2>/dev/null)
exe=$(readlink /proc/"$pid"/exe 2>/dev/null)
if [ -n "$pid" ] && [ "$exe" = "$NEW_WS/target/release/touring-daemon" ]; then
  echo "  PASS (PID $pid → $exe)"; else echo "  FAIL (PID=$pid exe=$exe)"; fail=1; fi

echo "── (d) doctor: daemon_socket + daemon_health ok"
if touring doctor -j 2>/dev/null | python3 -c '
import json,sys
c={x["name"]:x["status"] for x in json.load(sys.stdin)}
sys.exit(0 if c.get("daemon_socket")=="ok" and c.get("daemon_health")=="ok" else 1)'; then
  echo "  PASS"; else echo "  FAIL"; fail=1; fi

echo "── (e) zero unexpected runtime hardcodes of the historical root"
hits=$(grep -rn '/home/gabrielgadea/\.claude/rust' crates/ --include='*.rs' \
  | grep -v 'unwrap_or_else' \
  | grep -vE '^[^:]+:[0-9]+:\s*//' \
  | grep -v '/tests/' \
  | grep -cvE 'quality_rules\.rs|txn\.rs|gotcha_loader\.rs')
if [ "$hits" -eq 0 ]; then echo "  PASS (0 unexpected)"; else echo "  FAIL: $hits unexpected"; fail=1; fi

echo "── (f) co-evolution: configs reference the new root"
ok=0
grep -q 'projects/touring' "$HOME/.claude/settings.json" && ok=$((ok+1))
grep -q 'projects/touring' "$HOME/.claude/CLAUDE.md" && ok=$((ok+1))
grep -q 'projects/touring/target' "$HOME/.claude/tools/disk-watch.sh" && ok=$((ok+1))
if [ "$ok" -eq 3 ]; then echo "  PASS (3/3)"; else echo "  FAIL ($ok/3: settings/CLAUDE.md/disk-watch)"; fail=1; fi

echo "── (g) binary version == workspace version"
ws_ver=$(grep -A8 '^\[workspace.package\]' Cargo.toml | grep '^version' | head -1 | cut -d'"' -f2)
bin_ver=$("$BIN" --version 2>&1 | head -1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
if [ -n "$ws_ver" ] && [ "$ws_ver" = "$bin_ver" ]; then
  echo "  PASS ($ws_ver)"; else echo "  FAIL (workspace=$ws_ver binary=$bin_ver)"; fail=1; fi

echo "── (h) frozen copy intact (D4)"
if [ -f "$OLD_WS/Cargo.toml" ]; then echo "  PASS (frozen copy present)"; else
  echo "  FAIL (frozen copy missing — discard was NOT authorized yet)"; fail=1; fi

if [ "$fail" -eq 0 ]; then echo "✅ PHASE 4 VALIDATE: ALL PASS"; exit 0
else echo "❌ PHASE 4 VALIDATE: FAILURES ABOVE"; exit 1; fi
