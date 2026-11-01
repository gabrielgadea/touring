#!/usr/bin/env bash
# Mutantes do determinismo + semantica de dataclass. Roda DENTRO de uma reserva do hw-slot (rust), com os
# testes limpos ja verdes. Marcador .TROCA-EM-CURSO durante a janela; o fonte volta byte a byte (cmp).
set -u
WT=/home/gabrielgadea/projects/touring-wt-vgp-attr
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
V=$WT/crates/touring-code/src/vgp
cd "$WT" || exit 9
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-test
mkdir -p "$P0/fonte_dc_bom/checks"
for f in checks.rs modules.rs checks/tests.rs; do cp -p "$V/$f" "$P0/fonte_dc_bom/$f"; done
: > "$P0/mut_dc.rc"
sha256sum "$V/checks.rs" "$V/modules.rs" "$V/checks/tests.rs" | cut -c1-16,65- >> "$P0/mut_dc.rc"

printf 'TROCA EM CURSO (subagente p0-atributo): rodando MUTANTES em checks.rs e modules.rs (cada um e aplicado, testado e revertido). Inicio %s. O fonte volta byte a byte (cmp); este arquivo some quando terminar.\n' "$(date -Is)" > "$WT/.TROCA-EM-CURSO"
echo "marcador criado $(date -Is)" >> "$P0/mut_dc.rc"
python3 "$P0/mutantes_dc.py" "$WT" "$P0/mutantes_dc.json" > "$P0/mutantes_dc.log" 2>&1
echo "mutantes rc=$?" >> "$P0/mut_dc.rc"
for f in checks.rs modules.rs checks/tests.rs; do
  cmp "$V/$f" "$P0/fonte_dc_bom/$f" && echo "restaurado $f (cmp rc=0)" >> "$P0/mut_dc.rc"
done
ls "$V"/*.orig_mut "$V"/checks/*.orig_mut >> "$P0/mut_dc.rc" 2>&1
unlink "$WT/.TROCA-EM-CURSO"
echo "marcador removido $(date -Is)" >> "$P0/mut_dc.rc"
echo FIM >> "$P0/mut_dc.rc"
