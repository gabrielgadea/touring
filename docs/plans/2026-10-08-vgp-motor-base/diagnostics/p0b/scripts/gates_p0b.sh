#!/usr/bin/env bash
# Gates do P0B (aceite 1): fmt, clippy do workspace com -D warnings e as suites de touring-code, touring-pyright,
# touring-server e touring-hook-handlers (features pre-hooks,post-hooks). Roda DENTRO de uma reserva do hw-slot:
#   hw-slot --class rust --wait 7200 -- bash gates_p0b.sh <pasta-de-saida>
# Os E2E do touring-server procuram touring/touring-daemon em <workspace>/target/debug: o worktree usa
# CARGO_TARGET_DIR=wt-p0b, entao liga-se target/debug/{touring,touring-daemon} (symlinks, desligados ao final).
set -u
WT=/home/gabrielgadea/projects/touring-wt-p0b
OUT=${1:?pasta de saida}
mkdir -p "$OUT"
cd "$WT" || exit 9
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-p0b
: > "$OUT/gates.rc"

cargo fmt --all -- --check > "$OUT/fmt.log" 2>&1
echo "fmt --all -- --check rc=$?" >> "$OUT/gates.rc"

cargo clippy --workspace --all-targets -- -D warnings > "$OUT/clippy.log" 2>&1
echo "clippy --workspace --all-targets -- -D warnings rc=$?" >> "$OUT/gates.rc"

cargo test -p touring-code --no-fail-fast > "$OUT/test_code.log" 2>&1
echo "test touring-code rc=$?" >> "$OUT/gates.rc"
grep -a "test result" "$OUT/test_code.log" >> "$OUT/gates.rc"
grep -a "^test .* FAILED" "$OUT/test_code.log" >> "$OUT/gates.rc"

cargo test -p touring-pyright --no-fail-fast > "$OUT/test_pyright.log" 2>&1
echo "test touring-pyright rc=$?" >> "$OUT/gates.rc"
grep -a "test result" "$OUT/test_pyright.log" >> "$OUT/gates.rc"
grep -a "^test .* FAILED" "$OUT/test_pyright.log" >> "$OUT/gates.rc"

cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast > "$OUT/test_hh.log" 2>&1
echo "test touring-hook-handlers --features pre-hooks,post-hooks rc=$?" >> "$OUT/gates.rc"
grep -a "test result" "$OUT/test_hh.log" >> "$OUT/gates.rc"
grep -a "^test .* FAILED" "$OUT/test_hh.log" >> "$OUT/gates.rc"

cargo build -p touring-server --bin touring > "$OUT/build_touring.log" 2>&1
echo "build touring (debug) rc=$?" >> "$OUT/gates.rc"
cargo build -p touring-hooks --bin touring-daemon > "$OUT/build_daemon.log" 2>&1
echo "build touring-daemon (debug) rc=$?" >> "$OUT/gates.rc"
mkdir -p target/debug
for b in touring touring-daemon; do [ -e "target/debug/$b" ] || ln -s "$CARGO_TARGET_DIR/debug/$b" "target/debug/$b"; done
echo "symlinks: $(ls -la target/debug/touring target/debug/touring-daemon | awk '{print $9$10$11}' | tr '\n' ' ')" >> "$OUT/gates.rc"
cargo test -p touring-server --no-fail-fast > "$OUT/test_server.log" 2>&1
echo "test touring-server rc=$?" >> "$OUT/gates.rc"
grep -a "test result" "$OUT/test_server.log" >> "$OUT/gates.rc"
grep -a "^test .* FAILED" "$OUT/test_server.log" >> "$OUT/gates.rc"
unlink target/debug/touring
unlink target/debug/touring-daemon
echo "symlinks removidos" >> "$OUT/gates.rc"
echo FIM >> "$OUT/gates.rc"
