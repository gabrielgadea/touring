#!/usr/bin/env bash
# V-11 / 3.9 (31/10/2026) — the Write/Edit P0 gate does NOT fail open while a directory score
# holds the quality lock. Holds the REAL lock (flock(2), the one scripts/touring-quality-score
# takes), proves it is held, then feeds the LIVE hook a Write that must be denied (F2.4 secret,
# new file → ABSOLUTE mode) and a clean Write that must pass. Exit 0 only if both verdicts hold
# and neither run logged a fail-open.
#
# Why it can pass: the lanes of the wrapper (17/10/2026, Gabriel's decision) — a FILE target
# never takes the scan lock; only a directory score and `--output` do. Before them, every
# Write/Edit queued behind a 165-184 s directory scan, spent its 20 s budget and skipped the gate
# (~40% of one afternoon, RETOMAR-AQUI-2026-10-09.md:1124-1127).
set -uo pipefail
HOOK="${HOOK:-$HOME/.claude/hooks/touring-quality-block-all.sh}"
LOCK="${TOURING_QUALITY_LOCK:-${XDG_RUNTIME_DIR:-$HOME/.cache}/touring-quality.lock}"
WORK="$(mktemp -d)"
LOG=/tmp/hook_invocations.log
trap 'rm -rf "$WORK"; [ -n "${HOLDER:-}" ] && wait "$HOLDER" 2>/dev/null' EXIT

python3 - "$LOCK" <<'EOF' &
import fcntl, sys, time
fd = open(sys.argv[1], "a")
fcntl.flock(fd, fcntl.LOCK_EX)
time.sleep(20)
EOF
HOLDER=$!
sleep 1
if flock -n "$LOCK" true; then
    echo "INVALIDA: o lock nao ficou ocupado"; exit 2
fi
echo "lock ocupado: $LOCK"

N=$(date +%s%N)
before=$(grep -c "fail-open" "$LOG" 2>/dev/null || true); before=${before:-0}
run() {  # <path> <content> -> prints "<decision> <secs>"
    local pay t0 t1 out
    pay=$(jq -cn --arg p "$1" --arg c "$2" '{tool_name:"Write",tool_input:{file_path:$p,content:$c}}')
    t0=$(date +%s.%N); out=$(printf '%s' "$pay" | "$HOOK"); t1=$(date +%s.%N)
    # An allowed call prints NOTHING; jq over empty input prints nothing too, so the
    # empty answer is spelled as the object it means before it is read.
    out=${out:-'{}'}
    printf '%s %s\n' "$(printf '%s' "$out" | jq -r '.hookSpecificOutput.permissionDecision // "allow"' 2>/dev/null || echo allow)" \
        "$(awk -v a="$t0" -v b="$t1" 'BEGIN{printf "%.2f", b-a}')"
}
read -r d1 s1 < <(run "$WORK/cfg_$N.py" "password = \"hunter2value$N\"
")
read -r d2 s2 < <(run "$WORK/ok_$N.py" "x = 1 + $N
")
after=$(grep -c "fail-open" "$LOG" 2>/dev/null || true); after=${after:-0}
echo "segredo: $d1 em ${s1}s (esperado deny) · limpo: $d2 em ${s2}s (esperado allow) · fail-open novos: $((after - before))"
[ "$d1" = deny ] && [ "$d2" = allow ] && [ "$((after - before))" -eq 0 ]
