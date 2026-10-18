#!/usr/bin/env bash
# RED dos testes dc_* (semantica do dataclass): troca checks.rs e modules.rs pelos do binario 70e0051a
# (guardados em fonte_det_bom/), roda os testes novos, RESTAURA e prova por cmp. Troca temporaria num
# worktree que outros leem => marcador visivel .TROCA-EM-CURSO durante a janela (pedido da coordenadora).
# Roda DENTRO de uma reserva do hw-slot (rust).
set -u
WT=/home/gabrielgadea/projects/touring-wt-vgp-attr
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
V=$WT/crates/touring-code/src/vgp
cd "$WT" || exit 9
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-test
: > "$P0/dc_red.rc"

mkdir -p "$P0/fonte_dc_novo"
cp -p "$V/checks.rs" "$V/modules.rs" "$P0/fonte_dc_novo/"
sha256sum "$V/checks.rs" "$V/modules.rs" | cut -c1-16,65- >> "$P0/dc_red.rc"

printf 'TROCA EM CURSO (subagente p0-atributo): checks.rs e modules.rs estao TEMPORARIAMENTE iguais aos do binario 70e0051a, para provar o RED dos testes dc_*. Inicio %s. Restauracao por cmp ao fim; este arquivo some quando terminar.\n' "$(date -Is)" > "$WT/.TROCA-EM-CURSO"
echo "marcador criado $(date -Is)" >> "$P0/dc_red.rc"

cp "$P0/fonte_det_bom/checks.rs" "$V/checks.rs"
cp "$P0/fonte_det_bom/modules.rs" "$V/modules.rs"
cargo test -p touring-code --lib -- dc_ > "$P0/dc_red.log" 2>&1
echo "test dc_ no codigo 70e0051a rc=$?" >> "$P0/dc_red.rc"

cp -p "$P0/fonte_dc_novo/checks.rs" "$V/checks.rs"
cp -p "$P0/fonte_dc_novo/modules.rs" "$V/modules.rs"
cmp "$V/checks.rs" "$P0/fonte_dc_novo/checks.rs" && echo "restaurado checks.rs (cmp rc=0)" >> "$P0/dc_red.rc"
cmp "$V/modules.rs" "$P0/fonte_dc_novo/modules.rs" && echo "restaurado modules.rs (cmp rc=0)" >> "$P0/dc_red.rc"
unlink "$WT/.TROCA-EM-CURSO"
echo "marcador removido $(date -Is)" >> "$P0/dc_red.rc"
echo FIM >> "$P0/dc_red.rc"
