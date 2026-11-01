#!/usr/bin/env bash
# Gates of the ECC fix patches (hk, qa, mem) integrated into the int97 worktree (17/10/2026).
# Usage: gates_int97.sh <worktree> <log_dir>   — run under ONE `hw-slot --class rust` slot by the caller.
# Every step runs even after a failure (REGRA #21 wants the whole list); rc per step goes to <log_dir>/gates.rc.
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
    local rc=$?
    echo "$name rc=$rc $((SECONDS - t0))s" >>"$LOG/gates.rc"
}

HOOK_FEATURES=(--features pre-hooks,post-hooks)
step fmt cargo fmt --all -- --check
step check-workspace cargo check --workspace
step clippy cargo clippy -p touring-foundation -p touring-intelligence -p touring-ceg -p touring-hook-runtime \
    -p touring-storage -p touring-quality -p touring-analysis -p touring-cli -p touring-server --all-targets -- -D warnings
step clippy-hooks cargo clippy -p touring-dispatch -p touring-hooks "${HOOK_FEATURES[@]}" --all-targets -- -D warnings
# The touring-server e2e spawn target/debug/{touring,touring-hook,touring-daemon} of THIS worktree.
step build-bins cargo build -p touring-hooks -p touring-server
for pkg in touring-foundation touring-intelligence touring-ceg touring-hook-runtime touring-quality touring-analysis \
    touring-cli touring-server; do
    step "test-$pkg" cargo test -p "$pkg" --no-fail-fast
done
step test-touring-storage cargo test -p touring-storage --features knowledge,session-hooks --no-fail-fast
step test-touring-dispatch cargo test -p touring-dispatch "${HOOK_FEATURES[@]}" --no-fail-fast
step test-touring-hooks cargo test -p touring-hooks "${HOOK_FEATURES[@]}" --no-fail-fast
step pytest-r3 python3 -m pytest scripts/test_r3_a1_kpi.py -q -p no:cacheprovider
echo "FIM $(date -Iseconds)" >>"$LOG/gates.rc"
