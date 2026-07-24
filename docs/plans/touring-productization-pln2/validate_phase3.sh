#!/usr/bin/env bash
# validate_phase3.sh — Pln2 F3 cross-audit gate: `touring update` + `touring component`.
#
# Proves the propagation core IN PRACTICE (plan §Fase 3): fake toolchains vA/vB,
# a pinned project, channel switch + lockfile + deterministic rollback, component
# lifecycle, install --from-source, command_table registration, and the LIVE
# global daemon untouched.
#
# Pattern note (gotcha 2026-07-24, twice): NEVER `producer | grep -q` under
# pipefail — a multi-block producer dies with SIGPIPE 141 when grep exits
# early. Always capture-then-grep.
set -uo pipefail

PASS=0; FAIL=0
TOURING_BIN="${TOURING_BIN:-$HOME/.local/bin/touring}"

check() { # $1 label, $2 = 0 for ok
  if [ "$2" -eq 0 ]; then PASS=$((PASS+1)); echo "  ✅ $1";
  else FAIL=$((FAIL+1)); echo "  ❌ $1"; fi
}

echo "=== validate_phase3 — F3 update + component ($(date -Iseconds)) ==="

# ── Sandbox: isolated HOME-side dirs, real deployed binary ──────────────────
SANDBOX=$(mktemp -d /tmp/validate-phase3-XXXXXX)
TH="$SANDBOX/touring-home"
PROJ="$SANDBOX/proj"
mkdir -p "$TH/toolchains" "$PROJ/.touring/bin" "$PROJ/.touring/data"

make_toolchain() { # $1 version, $2... extra bins
  local v="$1"; shift
  local d="$TH/toolchains/$v/bin"; mkdir -p "$d"
  for b in touring touring-hook touring-daemon "$@"; do
    printf '#!/bin/sh\necho fake-%s-%s\n' "$b" "$v" > "$d/$b"; chmod 755 "$d/$b"
  done
}
make_toolchain vA
make_toolchain vB touring-quality
printf '[toolchain]\nchannel = "vA"\n' > "$PROJ/.touring/touring.toml"

run_t() { # run the deployed touring with sandboxed TOURING_HOME (HOME kept: binary needs nothing global here)
  env TOURING_HOME="$TH" "$TOURING_BIN" "$@" 2>&1
}

# ── 1. update resolves the pin (vA) ─────────────────────────────────────────
OUT=$(run_t update --project "$PROJ" --no-restart); RC=$?
LINK=$(readlink "$PROJ/.touring/bin/touring" 2>/dev/null || echo MISSING)
[ $RC -eq 0 ] && echo "$LINK" | grep -q "/vA/"; check "1. update resolves pin vA (link=$LINK)" $?

# ── 2. update --channel vB switches + writes lock ───────────────────────────
OUT=$(run_t update --channel vB --project "$PROJ" --no-restart); RC=$?
LINK=$(readlink "$PROJ/.touring/bin/touring-daemon" 2>/dev/null || echo MISSING)
LOCK=$(cat "$PROJ/.touring/toolchain.lock" 2>/dev/null || echo MISSING)
OK=1
[ $RC -eq 0 ] && echo "$LINK" | grep -q "/vB/" && echo "$LOCK" | grep -q 'active = "vB"' \
  && echo "$LOCK" | grep -q 'previous = "vA"' && OK=0
check "2. update --channel vB: links vB + lock{active=vB,previous=vA}" $OK

# ── 3. rollback restores vA deterministically ───────────────────────────────
OUT=$(run_t update --rollback --project "$PROJ" --no-restart); RC=$?
LINK=$(readlink "$PROJ/.touring/bin/touring" 2>/dev/null || echo MISSING)
LOCK=$(cat "$PROJ/.touring/toolchain.lock" 2>/dev/null || echo MISSING)
OK=1
[ $RC -eq 0 ] && echo "$LINK" | grep -q "/vA/" && echo "$LOCK" | grep -q 'active = "vA"' && OK=0
check "3. update --rollback restores vA (lock swapped)" $OK

# ── 4. update to a missing toolchain fails LOUD ─────────────────────────────
OUT=$(run_t update --channel v-missing --project "$PROJ" --no-restart); RC=$?
OK=1; [ $RC -ne 0 ] && echo "$OUT" | grep -q "not installed" && OK=0
check "4. update to missing toolchain refused loud" $OK

# ── 5. component lifecycle (list / add / remove / core-refusal) ─────────────
OUT=$(run_t update --channel vB --project "$PROJ" --no-restart)   # back to vB (has touring-quality)
L=$(run_t component list --project "$PROJ"); RC_L=$?
A=$(run_t component add touring-quality --project "$PROJ"); RC_A=$?
QLINK=$(readlink "$PROJ/.touring/bin/touring-quality" 2>/dev/null || echo MISSING)
R=$(run_t component remove touring-quality --project "$PROJ"); RC_R=$?
CORE=$(run_t component remove touring-hook --project "$PROJ"); RC_CORE=$?
OK=1
[ $RC_L -eq 0 ] && echo "$L" | grep -q "touring-quality" \
  && [ $RC_A -eq 0 ] && echo "$QLINK" | grep -q "/vB/" \
  && [ $RC_R -eq 0 ] && [ ! -e "$PROJ/.touring/bin/touring-quality" ] \
  && [ $RC_CORE -ne 0 ] && OK=0
check "5. component list/add/remove + core-removal refused" $OK

# ── 6. toolchain install --from-source (the dev→toolchain bridge) ───────────
SRC="$SANDBOX/src-ws"; mkdir -p "$SRC/target/release"
for b in touring touring-hook touring-daemon; do
  printf '#!/bin/sh\necho src-%s\n' "$b" > "$SRC/target/release/$b"; chmod 755 "$SRC/target/release/$b"
done
OUT=$(run_t toolchain install --from-source "$SRC" vsrc); RC=$?
OK=1
[ $RC -eq 0 ] && [ -f "$TH/toolchains/vsrc/bin/touring-daemon" ] \
  && grep -q "local-source" "$TH/toolchains/vsrc/meta.toml" 2>/dev/null && OK=0
check "6. toolchain install --from-source populates bin/ + meta" $OK

# ── 7. command_table registration: --help exit 0 for both new commands ──────
H1=$(run_t update --help); RC1=$?
H2=$(run_t component --help); RC2=$?
OK=1; [ $RC1 -eq 0 ] && [ $RC2 -eq 0 ] && echo "$H1$H2" | grep -q "USAGE" && OK=0
check "7. update/component registered (--help exit 0)" $OK

# ── 8. LIVE global daemon untouched (plan §7 recurring safeguard) ───────────
DS=$(env -u TOURING_DAEMON_SOCKET -u TOURING_DAEMON_SOCK "$TOURING_BIN" daemon-ctl status 2>&1); RC=$?
OK=1; [ $RC -eq 0 ] && echo "$DS" | grep -q "touring-daemon-1000.sock" && OK=0
check "8. live global daemon unchanged (socket global, status ok)" $OK

echo ""
echo "=== validate_phase3: $PASS PASS / $FAIL FAIL ==="
[ $FAIL -eq 0 ] && echo "ALL PASS" || echo "GATE FAILED"
exit $FAIL
