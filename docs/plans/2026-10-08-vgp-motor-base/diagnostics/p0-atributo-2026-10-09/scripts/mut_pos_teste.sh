#!/usr/bin/env bash
# testes limpos primeiro (um teste novo quebrado "mataria" mutantes de graca); so entao os mutantes
cd /home/gabrielgadea/projects/touring-wt-vgp-attr || exit 9
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-test
cargo test -p touring-code --lib vgp:: > "$P0/g6_test.log" 2>&1
T=$?
echo "test rc=$T" > "$P0/g6.rc"
grep "test result" "$P0/g6_test.log" >> "$P0/g6.rc"
if [ "$T" -ne 0 ]; then echo "FIM (testes falharam; mutantes nao rodaram)" >> "$P0/g6.rc"; exit 1; fi
cp crates/touring-code/src/vgp/namespace.rs "$P0/namespace.rs.bom"
python3 "$P0/mutantes.py" "$PWD" "$P0/mutantes_final.json" > "$P0/mutantes_final.log" 2>&1
echo "mutantes rc=$?" >> "$P0/g6.rc"
echo FIM >> "$P0/g6.rc"
