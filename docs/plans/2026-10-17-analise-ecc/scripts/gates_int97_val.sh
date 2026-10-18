#!/usr/bin/env bash
# Gate of the heredoc fix in pre_tool_validator.rs integrated into int97 (18/10/2026). touring-hook-handlers is the
# consumer (pre_bash.rs and vgp_execucao.rs call shell_pipelines/command_of), so its tests run too.
# Usage: gates_int97_val.sh <worktree> <log_dir>   — run under ONE `hw-slot --class rust` slot by the caller.
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
step clippy-hooks-core cargo clippy -p touring-hooks-core --all-targets -- -D warnings
step clippy-handlers cargo clippy -p touring-hook-handlers --features pre-hooks,post-hooks --all-targets -- -D warnings
step test-touring-hooks-core cargo test -p touring-hooks-core --no-fail-fast
step test-touring-hook-handlers cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast
echo "FIM $(date -Iseconds)" >>"$LOG/gates.rc"
