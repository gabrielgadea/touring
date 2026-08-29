#!/usr/bin/env bash
# Repro-loop for the intermittent touring-dispatch lifecycle hang.
# Runs the suite up to MAX_ROUNDS times; on the first round that exceeds
# ROUND_TIMEOUT the test process is photographed (threads, wchan, fds,
# open sockets) BEFORE being killed, and the evidence lands in EVIDENCE.
set -u
ROUNDS="${1:-15}"
ROUND_TIMEOUT="${2:-90}"
EVIDENCE="${3:-/tmp/claude-1000/-home-gabrielgadea-projects-touring/a718151d-1737-4b66-bcb2-af9717c37686/scratchpad/lifecycle_hang_evidence.txt}"
cd /home/gabrielgadea/projects/touring || exit 2

for round in $(seq 1 "$ROUNDS"); do
    echo "[round $round/$ROUNDS] $(date +%H:%M:%S)"
    timeout "$ROUND_TIMEOUT" cargo test -p touring-dispatch --lib lifecycle >/dev/null 2>&1 &
    waiter=$!
    wait "$waiter"
    rc=$?
    if [ "$rc" -ne 124 ]; then
        echo "  round $round finished rc=$rc (no hang)"
        continue
    fi
    # timeout fired but the test binary may survive as an orphan — find it.
    sleep 1
    pid=$(pgrep -f 'target/debug/deps/touring_dispatch-' | head -1)
    if [ -z "$pid" ]; then
        echo "  round $round: timeout but no surviving test binary"
        continue
    fi
    {
        echo "=== HANG CAPTURED round=$round pid=$pid $(date -Is) ==="
        echo "--- thread comms (count) ---"
        cat /proc/"$pid"/task/*/comm 2>/dev/null | sort | uniq -c | sort -rn
        echo "--- per-thread wchan+stat ---"
        for t in /proc/"$pid"/task/*/; do
            tid=$(basename "$t")
            printf '%s comm=%s wchan=%s state=%s\n' \
                "$tid" \
                "$(cat "$t"/comm 2>/dev/null)" \
                "$(cat "$t"/wchan 2>/dev/null)" \
                "$(awk '{print $3}' "$t"/stat 2>/dev/null)"
        done
        echo "--- open fds ---"
        ls -l /proc/"$pid"/fd 2>/dev/null | awk '{print $NF}' | sort | uniq -c | sort -rn | head -30
        echo "--- unix sockets of pid ---"
        ss -xp 2>/dev/null | grep "pid=$pid" | head -10
    } >> "$EVIDENCE" 2>&1
    echo "  HANG captured to $EVIDENCE — killing $pid"
    kill "$pid" 2>/dev/null
    exit 0
done
echo "no hang in $ROUNDS rounds"
exit 1
