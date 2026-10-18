#!/usr/bin/env bash
# P0B-d, rodada final (na arvore final): liga os binarios do touring-server no target do worktree (os 6 e2e
# os procuram em <wt>/target/debug — ver a memoria worktree-e2e-exige-binarios-no-target-do-worktree), roda
# TODOS os gates e o mutante M9 (o unico que sobreviveu na rodada 1; fechado pelo teste novo de sombra.rs).
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0bd
WT=/home/gabrielgadea/projects/touring-wt-p0bd
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-p0bd
cd "$WT" || exit 9
mkdir -p "$D/logs"
cargo build -p touring-server -p touring-hooks > "$D/logs/build_binarios.log" 2>&1
echo "build binarios rc=$?" > "$D/logs/build_rc.txt"
mkdir -p "$WT/target/debug"
for b in touring touring-daemon; do ln -sfn "$CARGO_TARGET_DIR/debug/$b" "$WT/target/debug/$b"; done
ls -l "$WT/target/debug/" >> "$D/logs/build_rc.txt"
bash "$D/scripts/gates_p0bd.sh" "$D/logs"
bash "$D/scripts/mut_p0bd.sh" "$D/mutantes_final" M9-PROMOVE-QUALQUER-ALVO M13-DIRETORIO-TEMPORARIO-COMPARTILHADO
