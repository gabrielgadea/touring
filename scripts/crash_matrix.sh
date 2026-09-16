#!/usr/bin/env bash
# crash_matrix.sh — reproduction harness: a TABLE OF RATES, never one anecdote.
#
# Camada 2 of docs/plans/2026-09-16-crash-do-motor/estrategia-crash-classe-memoria.md.
# The touring-quality engine died twice with SIGSEGV/SIGILL and the first census
# (segv_census.sh, 15/09) was a one-off script that measured one configuration.
# A single crash tells you nothing about a race: the question is whether the RATE
# changes with the environment, and that needs cells, repetitions and a baseline.
#
# The first matrix tests H1 (data race in the engine's nested par_iter over
# dimensions × files, scope_report.rs): if RAYON_NUM_THREADS=1 comes back clean
# while 2 and 8 do not, the cause is concurrency and the search space collapses
# from "the engine" to "what the dimensions share".
#
# READ THE RESULT HONESTLY: a clean row does not prove absence. With a rate as low
# as the observed one (0 of 24 in the first census), N=20 per cell can miss the
# fault entirely — the table reports the confidence interval so a zero is read as
# "not seen in N", never as "does not happen".
#
# Usage:
#   scripts/crash_matrix.sh [--target <path>] [--runs N] [--threads "1 2 8"]
#                           [--engine <bin>] [--out <dir>] [--timeout SECS]
#
# Exit: 0 the matrix ran (crashes or not) · 2 usage · 3 engine/target unusable.

set -euo pipefail

SELF="$(readlink -f "${BASH_SOURCE[0]}")"
WORKSPACE="$(cd "$(dirname "$SELF")/.." && pwd)"

TARGET="$WORKSPACE/crates/touring-quality/src"
RUNS=20
THREADS="1 2 8"
ENGINE="${TOURING_QUALITY_BIN:-$WORKSPACE/target/release/touring-quality}"
OUT="${TOURING_CRASH_MATRIX_OUT:-$HOME/.claude/touring/logs/crash-matrix}"
PER_RUN_TIMEOUT=600

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)  TARGET="$2"; shift 2 ;;
    --runs)    RUNS="$2"; shift 2 ;;
    --threads) THREADS="$2"; shift 2 ;;
    --engine)  ENGINE="$2"; shift 2 ;;
    --out)     OUT="$2"; shift 2 ;;
    --timeout) PER_RUN_TIMEOUT="$2"; shift 2 ;;
    -h|--help) sed -n '2,25p' "$SELF"; exit 0 ;;
    *) echo "crash_matrix: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

[[ -x "$ENGINE" ]] || { echo "crash_matrix: engine not executable: $ENGINE" >&2; exit 3; }
[[ -e "$TARGET" ]] || { echo "crash_matrix: target does not exist: $TARGET" >&2; exit 3; }
[[ "$RUNS" =~ ^[0-9]+$ ]] && (( RUNS > 0 )) || { echo "crash_matrix: --runs must be a positive integer" >&2; exit 2; }

STAMP="$(date +%Y%m%dT%H%M%S)"
RUN_DIR="$OUT/$STAMP"
mkdir -p "$RUN_DIR"
JSONL="$RUN_DIR/runs.jsonl"

echo "crash_matrix: engine=$ENGINE"
echo "              target=$TARGET"
echo "              cells=[$THREADS] × N=$RUNS  →  $RUN_DIR"
echo

# The engine is invoked DIRECTLY, never through the caching wrapper: a cache hit
# would return in 0s without running anything, and a matrix of cache hits reports
# a clean table that measured nothing (`verde-de-cache-nao-prova-medicao`).
run_once() {
  local threads="$1" idx="$2" rc=0 start end
  local err="$RUN_DIR/t${threads}-r${idx}.stderr"
  start="$(date +%s.%N)"
  RAYON_NUM_THREADS="$threads" TOURING_QUALITY_CACHE_DISABLE=1 \
    nice -n 19 timeout "$PER_RUN_TIMEOUT" \
    "$ENGINE" score "$TARGET" --format json >/dev/null 2>"$err" || rc=$?
  end="$(date +%s.%N)"
  # Keep stderr only when it says something: N clean runs must not litter the dir.
  if (( rc == 0 )) && [[ ! -s "$err" ]]; then rm -f "$err"; fi
  printf '{"threads":%s,"run":%d,"exit":%d,"signal":%d,"secs":%s}\n' \
    "$threads" "$idx" "$rc" "$(( rc >= 128 ? rc - 128 : 0 ))" \
    "$(awk -v a="$start" -v b="$end" 'BEGIN{printf "%.2f", b-a}')" >>"$JSONL"
  return 0
}

for threads in $THREADS; do
  printf 'RAYON_NUM_THREADS=%-3s ' "$threads"
  for (( i = 1; i <= RUNS; i++ )); do
    run_once "$threads" "$i"
    # One character per run, so a long matrix shows progress and shape at once.
    last_rc="$(tail -n1 "$JSONL" | sed 's/.*"exit":\([0-9]*\).*/\1/')"
    case "$last_rc" in
      0)   printf '.' ;;
      124) printf 'T' ;;   # timeout — a hang, not a crash
      13[0-9]|1[4-9][0-9]) printf 'X' ;;
      *)   printf '?' ;;
    esac
  done
  printf '\n'
done
echo

python3 - "$JSONL" <<'PY'
import json, math, sys
from collections import defaultdict

rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
cells = defaultdict(list)
for r in rows:
    cells[r["threads"]].append(r)

def wilson_upper(k, n, z=1.96):
    """Upper bound of the 95% interval — the honest reading of a zero.

    A cell with 0 crashes in 20 runs is consistent with a true rate up to ~16%:
    reporting '0%' would claim an absence the sample cannot support."""
    if n == 0:
        return 1.0
    p = k / n
    d = 1 + z * z / n
    centre = p + z * z / (2 * n)
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n))
    return min(1.0, (centre + half) / d)

print(f"{'threads':>8} {'N':>4} {'crashes':>8} {'timeouts':>9} {'rate':>7} {'95% up to':>10} {'median s':>9}")
print("-" * 60)
for threads in sorted(cells, key=int):
    rs = cells[threads]
    n = len(rs)
    crashes = sum(1 for r in rs if r["signal"] > 0)
    timeouts = sum(1 for r in rs if r["exit"] == 124)
    secs = sorted(r["secs"] for r in rs)
    median = secs[len(secs) // 2] if secs else 0.0
    print(f"{threads:>8} {n:>4} {crashes:>8} {timeouts:>9} {crashes / n:>7.1%} "
          f"{wilson_upper(crashes, n):>10.1%} {median:>9.1f}")

sigs = {r["signal"] for r in rows if r["signal"] > 0}
print()
if sigs:
    print(f"signals seen: {sorted(sigs)} — artifacts beside runs.jsonl")
else:
    print("no crash in this matrix. NOT 'the bug is gone': with these N the true rate")
    print("could still be as high as the '95% up to' column. A clean single-thread row")
    print("only discriminates H1 when the other rows DID crash.")
PY

echo
echo "artifacts: $RUN_DIR"
