#!/usr/bin/env bash
# P0B-d: gates (REGRA #21 — 0 falhas). Roda DENTRO de uma reserva do hw-slot (rust):
#   hw-slot --class rust --label p0bd --wait 7200 -- bash gates_p0bd.sh <pasta-de-logs>
set -u
WT=/home/gabrielgadea/projects/touring-wt-p0bd
LOGS=${1:?pasta de logs}
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-p0bd
cd "$WT" || exit 9
mkdir -p "$LOGS"
: > "$LOGS/gates_rc.txt"
run() { # nome, comando...
  local nome=$1; shift
  "$@" > "$LOGS/$nome.log" 2>&1
  echo "$nome rc=$?" >> "$LOGS/gates_rc.txt"
}
run fmt cargo fmt --all -- --check
run clippy cargo clippy --workspace --all-targets -- -D warnings
run test_code cargo test -p touring-code --no-fail-fast
run test_pyright cargo test -p touring-pyright --no-fail-fast
run test_hooks cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast
run test_server cargo test -p touring-server --no-fail-fast
echo FIM >> "$LOGS/gates_rc.txt"
