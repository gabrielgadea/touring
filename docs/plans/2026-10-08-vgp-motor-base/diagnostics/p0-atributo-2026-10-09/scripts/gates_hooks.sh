#!/usr/bin/env bash
# suites dos crates que consomem vgp::check (hooks em sombra): touring-hook-handlers (features) e touring-hooks
set -u
WT=/home/gabrielgadea/projects/touring-wt-vgp-attr
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
cd "$WT" || exit 9
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-test
: > "$P0/gh.rc"
cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast > "$P0/gh_hh.log" 2>&1
echo "test touring-hook-handlers rc=$?" >> "$P0/gh.rc"
grep -a "test result" "$P0/gh_hh.log" >> "$P0/gh.rc"
grep -a "^test .* FAILED" "$P0/gh_hh.log" >> "$P0/gh.rc"
mkdir -p target/debug
for b in touring touring-daemon; do [ -e "target/debug/$b" ] || ln -s "$CARGO_TARGET_DIR/debug/$b" "target/debug/$b"; done
cargo test -p touring-hooks --no-fail-fast > "$P0/gh_hooks.log" 2>&1
echo "test touring-hooks rc=$?" >> "$P0/gh.rc"
grep -a "test result" "$P0/gh_hooks.log" >> "$P0/gh.rc"
grep -a "^test .* FAILED" "$P0/gh_hooks.log" >> "$P0/gh.rc"
unlink target/debug/touring
unlink target/debug/touring-daemon
echo FIM >> "$P0/gh.rc"
