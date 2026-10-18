#!/usr/bin/env bash
# gates finais do P0 (aceite 5): fmt, clippy -D warnings e as suites de touring-code e touring-server.
# Roda DENTRO de uma reserva do hw-slot (rust). Saidas e rcs em $OUT/gf.*
set -u
WT=/home/gabrielgadea/projects/touring-wt-vgp-attr
OUT=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
cd "$WT" || exit 9
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-test
: > "$OUT/gf.rc"

cargo fmt --all -- --check > "$OUT/gf_fmt.log" 2>&1
echo "fmt --all --check rc=$?" >> "$OUT/gf.rc"

cargo clippy -p touring-code -p touring-server -p touring-hooks --all-targets -- -D warnings > "$OUT/gf_clippy.log" 2>&1
echo "clippy -D warnings (touring-code touring-server touring-hooks, all-targets) rc=$?" >> "$OUT/gf.rc"

cargo clippy -p touring-hook-handlers --all-targets --features pre-hooks,post-hooks -- -D warnings > "$OUT/gf_clippy_hh.log" 2>&1
echo "clippy -D warnings (touring-hook-handlers, features pre/post) rc=$?" >> "$OUT/gf.rc"

cargo test -p touring-code --no-fail-fast > "$OUT/gf_test_code.log" 2>&1
echo "test touring-code rc=$?" >> "$OUT/gf.rc"
grep -a "test result" "$OUT/gf_test_code.log" >> "$OUT/gf.rc"

cargo test -p touring-server --no-fail-fast > "$OUT/gf_test_server.log" 2>&1
echo "test touring-server rc=$?" >> "$OUT/gf.rc"
grep -a "test result" "$OUT/gf_test_server.log" >> "$OUT/gf.rc"

echo FIM >> "$OUT/gf.rc"
