#!/usr/bin/env bash
# P0B-d: RED run (tests written, fix NOT applied). Runs INSIDE an hw-slot (rust) reservation:
#   hw-slot --class rust --label p0bd --wait 7200 -- bash red_p0bd.sh
set -u
WT=/home/gabrielgadea/projects/touring-wt-p0bd
OUT=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0bd
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-p0bd
if [ ! -d "$CARGO_TARGET_DIR" ]; then
  cp -a --reflink=always /home/gabrielgadea/projects/touring/target/wt-frag "$CARGO_TARGET_DIR" || exit 8
fi
cd "$WT" || exit 9
: > "$OUT/logs/red_rc.txt"
cargo test -p touring-code --lib --no-fail-fast vgp:: > "$OUT/logs/red_code.log" 2>&1
echo "touring-code rc=$?" >> "$OUT/logs/red_rc.txt"
cargo test -p touring-pyright --lib --no-fail-fast p0bd > "$OUT/logs/red_pyright.log" 2>&1
echo "touring-pyright rc=$?" >> "$OUT/logs/red_rc.txt"
cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --lib --no-fail-fast p0bd > "$OUT/logs/red_hooks.log" 2>&1
echo "touring-hook-handlers rc=$?" >> "$OUT/logs/red_rc.txt"
echo FIM >> "$OUT/logs/red_rc.txt"
