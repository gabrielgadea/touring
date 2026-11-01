#!/usr/bin/env bash
# 15 execucoes do suite vgp:: do touring-code (a corrida do diretorio /tmp compartilhado falhava ~1 em N)
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-p0bd
cd /home/gabrielgadea/projects/touring-wt-p0bd || exit 9
OUT=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0bd/logs/estresse.txt
: > "$OUT"
for i in $(seq 1 15); do
  cargo test -p touring-code --lib vgp:: 2>&1 | grep -E "^test result|FAILED" | tr '\n' ' ' >> "$OUT"
  echo " rc=${PIPESTATUS[0]}" >> "$OUT"
done
echo FIM >> "$OUT"
