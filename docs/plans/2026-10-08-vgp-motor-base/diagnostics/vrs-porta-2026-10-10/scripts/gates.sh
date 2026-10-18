#!/usr/bin/env bash
# Gates do vrs2 numa UNICA aquisicao do hw-slot (quem chama envolve em `slot.sh gates ... bash gates.sh`).
# Cada gate grava log e rc proprios; nenhum `set -e` (o rc e lido explicitamente).
WT=/home/gabrielgadea/projects/touring-wt-vrs2
SC=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-vrs2
cd "$WT" || exit 90
: > "$SC/gates.rcs"
gate() {
  local nome=$1; shift
  "$@" > "$SC/gate_$nome.log" 2>&1
  local rc=$?
  echo "$nome rc=$rc" | tee -a "$SC/gates.rcs"
  return 0
}
gate fmt cargo fmt --all -- --check
gate clippy cargo clippy -p touring-code --all-targets -- -D warnings
gate teste_adaptador cargo test -p touring-code --lib vgp::lang::rust --no-fail-fast
gate teste_touring_code cargo test -p touring-code --no-fail-fast
RUSTDOCFLAGS="-D warnings" gate doc cargo doc -p touring-code --no-deps
gate release cargo build --release --locked -p touring-server --bin touring
echo "GATES CONCLUIDOS" | tee -a "$SC/gates.rcs"
