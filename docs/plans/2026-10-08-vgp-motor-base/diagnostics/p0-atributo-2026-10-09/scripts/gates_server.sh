#!/usr/bin/env bash
# touring-server: os E2E procuram touring/touring-daemon em <workspace>/target/{debug,release}. O worktree usa
# CARGO_TARGET_DIR=vgp-test, entao liga-se target -> vgp-test (symlink, desligado ao final), constroem-se os dois
# binarios debug e roda-se a suite INTEIRA do crate.
set -u
WT=/home/gabrielgadea/projects/touring-wt-vgp-attr
OUT=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
cd "$WT" || exit 9
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-test
: > "$OUT/gs.rc"
cargo build -p touring-server --bin touring > "$OUT/gs_build_touring.log" 2>&1
echo "build touring rc=$?" >> "$OUT/gs.rc"
cargo build -p touring-hooks --bin touring-daemon > "$OUT/gs_build_daemon.log" 2>&1
echo "build touring-daemon rc=$?" >> "$OUT/gs.rc"
ls -la "$CARGO_TARGET_DIR/debug/touring" "$CARGO_TARGET_DIR/debug/touring-daemon" >> "$OUT/gs.rc" 2>&1
mkdir -p target/debug
for b in touring touring-daemon; do [ -e "target/debug/$b" ] || ln -s "$CARGO_TARGET_DIR/debug/$b" "target/debug/$b"; done
echo "symlinks: $(ls -la target/debug/touring target/debug/touring-daemon | awk '{print $9$10$11}' | tr '\n' ' ')" >> "$OUT/gs.rc"
cargo test -p touring-server --no-fail-fast > "$OUT/gs_test_server.log" 2>&1
echo "test touring-server rc=$?" >> "$OUT/gs.rc"
grep -a "test result" "$OUT/gs_test_server.log" | sort | uniq -c >> "$OUT/gs.rc"
grep -a "^test .* FAILED" "$OUT/gs_test_server.log" >> "$OUT/gs.rc"
unlink target/debug/touring
unlink target/debug/touring-daemon
echo "symlinks removidos" >> "$OUT/gs.rc"
echo FIM >> "$OUT/gs.rc"
