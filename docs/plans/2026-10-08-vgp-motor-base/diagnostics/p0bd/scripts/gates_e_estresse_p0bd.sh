#!/usr/bin/env bash
# gates completos + teste novo do hook isolado + estresse, numa reserva so
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0bd
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-p0bd
cd /home/gabrielgadea/projects/touring-wt-p0bd || exit 9
bash "$D/scripts/gates_p0bd.sh" "$D/logs"
cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --lib p0bd -- --nocapture > "$D/logs/hooks_p0bd.log" 2>&1
bash "$D/scripts/estresse_p0bd.sh"
