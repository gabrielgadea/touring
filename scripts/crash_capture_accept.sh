#!/usr/bin/env bash
# crash_capture_accept.sh — the accept criterion for camada 1, run against the
# REAL engine: kill a live score with a fatal signal and demand that an artifact
# with a NAMED stack appears without anybody intervening.
#
# The unit tests around the wrapper use a fake engine, which proves the plumbing
# and nothing about the build. This proves the build: `strip = true` in
# [profile.release] is what made the two real cores carry one frame, and only a
# real binary, really killed, really read back by gdb can show that it is fixed.
# It is the "exercise the legitimate path too" lesson of 16/09 applied to a
# collector instead of a barrier.
#
# REGRA #19: the signal goes to ONE pid, looked up by `/proc/<pid>/comm` and
# confirmed to be a descendant of THIS script's own score. Never pkill, never by
# name, never a process this script did not start.
#
# Usage: scripts/crash_capture_accept.sh [--target <path>] [--signal ABRT] [--keep]
# Exit:  0 the criterion is met · 1 it is not (says which half failed) · 3 setup

set -euo pipefail

SELF="$(readlink -f "${BASH_SOURCE[0]}")"
WORKSPACE="$(cd "$(dirname "$SELF")/.." && pwd)"
WRAPPER="$WORKSPACE/scripts/touring-quality-score"
ENGINE="${TOURING_QUALITY_BIN:-$WORKSPACE/target/release/touring-quality}"
# The default target is the whole crates tree: the score has to still be running
# when the signal arrives, and a single crate finishes in under a second.
TARGET="$WORKSPACE/crates"
KEEP=0
# SIGABRT, and NOT SIGSEGV — measured 16/09/2026, and worth knowing before anyone
# tries to reproduce the real crash by hand. `/proc/<pid>/status` of a running
# engine reports `SigCgt: 0000000100000440`, i.e. signals 7 and 11 are CAUGHT: the
# Rust runtime installs a SIGSEGV/SIGBUS handler to turn a stack overflow into a
# readable message. A SIGSEGV delivered by kill(2) carries no guard-page address,
# so that handler swallows it — measured: the engine took the signal, finished the
# score and exited 0 with 18 KB of JSON. Anyone testing the collector with
# `kill -SEGV` would conclude the engine is immune to segfaults, which is false;
# a HARDWARE fault is a different path. SIGABRT is uncaught here (it is also what
# `panic = "abort"` produces), so it kills and dumps core like the real thing.
SIGNAL="ABRT"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --signal) SIGNAL="$2"; shift 2 ;;
    --keep)   KEEP=1; shift ;;
    -h|--help) sed -n '2,18p' "$SELF"; exit 0 ;;
    *) echo "crash_capture_accept: unknown argument '$1'" >&2; exit 3 ;;
  esac
done

[[ -x "$ENGINE" ]]  || { echo "engine not executable: $ENGINE" >&2; exit 3; }
[[ -x "$WRAPPER" ]] || { echo "wrapper not executable: $WRAPPER" >&2; exit 3; }

# A private HOME so the artifact lands in a directory this script owns, and the
# operator's real journal is never touched by a deliberate crash.
SANDBOX="$(mktemp -d "${TMPDIR:-/tmp}/crash-accept.XXXXXX")"
cleanup() { (( KEEP )) || rm -rf "$SANDBOX"; }
trap cleanup EXIT
LOGS="$SANDBOX/.claude/touring/logs"

echo "accept: engine=$ENGINE"
echo "        target=$TARGET"
echo "        sandbox=$SANDBOX"

# Start a real score through the wrapper, in the background.
HOME="$SANDBOX" \
TOURING_QUALITY_BIN="$ENGINE" \
TOURING_QUALITY_CACHE_DIR="$SANDBOX/cache" \
TOURING_QUALITY_LOCK="$SANDBOX/lock" \
TOURING_QUALITY_CACHE_DISABLE=1 \
  "$WRAPPER" score "$TARGET" --format json >"$SANDBOX/stdout.txt" 2>"$SANDBOX/stderr.txt" &
WRAPPER_PID=$!

# Find the engine process BELOW our own wrapper — never by name alone.
engine_pid() {
  local pid comm
  for pid in $(pgrep -P "$(pgrep -P "$WRAPPER_PID" -f 'timeout' | head -1)" 2>/dev/null || true) \
             $(pgrep -P "$WRAPPER_PID" 2>/dev/null || true); do
    [[ -r "/proc/$pid/comm" ]] || continue
    comm="$(cat "/proc/$pid/comm" 2>/dev/null || true)"
    if [[ "$comm" == "touring-quality" ]]; then printf '%s' "$pid"; return 0; fi
    # nice/timeout wrap the engine: descend one more level.
    local sub
    for sub in $(pgrep -P "$pid" 2>/dev/null || true); do
      comm="$(cat "/proc/$sub/comm" 2>/dev/null || true)"
      if [[ "$comm" == "touring-quality" ]]; then printf '%s' "$sub"; return 0; fi
    done
  done
  return 1
}

PID=""
for _ in $(seq 1 100); do
  if PID="$(engine_pid)" && [[ -n "$PID" ]]; then break; fi
  sleep 0.1
done
if [[ -z "$PID" ]]; then
  echo "FAIL(setup): the engine process never appeared under the wrapper" >&2
  wait "$WRAPPER_PID" 2>/dev/null || true
  exit 3
fi

echo "        engine pid=$PID comm=$(cat "/proc/$PID/comm") — sending SIG$SIGNAL"
# No wait at all. The engine is FAST — 82 files scored in 0.74s, measured — so a
# 2s pause (and then a 0.4s one) let the score finish first: the signal went to a
# dead pid, `kill` said nothing (it is `|| true`), and the criterion reported "no
# artifact" for a run that never crashed. The detection loop above already costs
# ~100ms, which is start-up enough. The remaining defence is the liveness check
# below plus a target big enough to hold still: an accept test must never confuse
# "the capture failed" with "there was nothing to capture".
if ! kill -0 "$PID" 2>/dev/null; then
  echo "FAIL(setup): the engine finished before it could be killed — this target is too" >&2
  echo "             small to hold a signal. Use a larger --target." >&2
  wait "$WRAPPER_PID" 2>/dev/null || true
  exit 3
fi
kill -s "$SIGNAL" "$PID" 2>/dev/null || true

RC=0
wait "$WRAPPER_PID" || RC=$?
echo "        wrapper exited $RC"

fail=0
check() {  # check <description> <condition-already-evaluated>
  if (( $2 )); then printf '  ✅ %s\n' "$1"; else printf '  ❌ %s\n' "$1"; fail=1; fi
}

echo
echo "criterion:"
EXPECTED_RC=$(( 128 + $(kill -l "$SIGNAL") ))
check "the wrapper reports the signal ($EXPECTED_RC)" "$([[ "$RC" == "$EXPECTED_RC" ]] && echo 1 || echo 0)"

JOURNAL="$LOGS/quality-runs.jsonl"
check "the run left a journal line" "$([[ -s "$JOURNAL" ]] && echo 1 || echo 0)"

ARTIFACT=""
if [[ -d "$LOGS/quality-crashes" ]]; then
  ARTIFACT="$(find "$LOGS/quality-crashes" -mindepth 1 -maxdepth 1 -type d | head -1)"
fi
check "a crash artifact was created" "$([[ -n "$ARTIFACT" ]] && echo 1 || echo 0)"

if [[ -n "$ARTIFACT" ]]; then
  check "it carries the engine's stderr" "$([[ -f "$ARTIFACT/stderr.txt" ]] && echo 1 || echo 0)"
  check "it names the signal" "$(grep -q "signal: $(kill -l "$SIGNAL")" "$ARTIFACT/run.txt" 2>/dev/null && echo 1 || echo 0)"
  # The half that the build setting decides: a stack with NAMES, not addresses.
  named=0
  if [[ -f "$ARTIFACT/backtrace.txt" ]]; then
    frames="$(grep -cE '^#[0-9]+ ' "$ARTIFACT/backtrace.txt" || true)"
    if grep -qE '^#[0-9]+ .*(touring|rayon|core::|std::)' "$ARTIFACT/backtrace.txt"; then named=1; fi
    echo "     (backtrace frames: ${frames:-0})"
  fi
  check "the backtrace has NAMED frames (this is what strip=true destroyed)" "$named"
  if (( KEEP )); then echo "     artifact kept at $ARTIFACT"; fi
fi

echo
if (( fail )); then
  echo "ACCEPT: NOT met — see the ❌ above."
  echo "        a missing core is usually systemd-coredump storage or permissions:"
  echo "        coredumpctl list | tail   /   /proc/sys/kernel/core_pattern"
  exit 1
fi
echo "ACCEPT: met — a deliberate SIG$SIGNAL produced a named stack with no intervention."
