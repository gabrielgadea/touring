#!/usr/bin/env bash
# Second gate of the int97 integration (17/10/2026): C8 (TierManager removed from touring-intelligence) and the C7
# consumer in pre_write.rs (neutral gotcha score while prevented_errors is unmeasured).
# Usage: gates_int97_c8c7.sh <worktree> <log_dir>   — run under ONE `hw-slot --class rust` slot by the caller.
set -u
WT=$1
LOG=$2
mkdir -p "$LOG"
: >"$LOG/gates.rc"
cd "$WT" || exit 2

step() {  # step <name> <cmd...>: full output in <name>.log, one "name rc" line in gates.rc
    local name=$1
    shift
    local t0=$SECONDS
    "$@" >"$LOG/$name.log" 2>&1
    echo "$name rc=$? $((SECONDS - t0))s" >>"$LOG/gates.rc"
}

step fmt cargo fmt --all -- --check
step check-workspace cargo check --workspace
step clippy-intelligence cargo clippy -p touring-intelligence --all-targets -- -D warnings
step clippy-handlers cargo clippy -p touring-hook-handlers --features pre-hooks,post-hooks --all-targets -- -D warnings
step test-touring-intelligence cargo test -p touring-intelligence --no-fail-fast
step test-touring-hook-handlers cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast
echo "FIM $(date -Iseconds)" >>"$LOG/gates.rc"
