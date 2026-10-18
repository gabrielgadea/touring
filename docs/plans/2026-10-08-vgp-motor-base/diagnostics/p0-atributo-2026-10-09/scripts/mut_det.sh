#!/usr/bin/env bash
# Mutantes do determinismo. Roda DENTRO de uma reserva do hw-slot (rust). Antes: o job A (testes limpos
# verdes). Guarda copia do fonte, roda os 10 mutantes e prova que o fonte voltou byte a byte.
set -u
WT=/home/gabrielgadea/projects/touring-wt-vgp-attr
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
V=$WT/crates/touring-code/src/vgp
cd "$WT" || exit 9
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-test
mkdir -p "$P0/fonte_det_bom"
for f in checks.rs modules.rs checks/tests.rs; do
  mkdir -p "$P0/fonte_det_bom/$(dirname "$f")"
  cp -p "$V/$f" "$P0/fonte_det_bom/$f"
done
: > "$P0/mut_det.rc"
sha256sum "$V/checks.rs" "$V/modules.rs" "$V/checks/tests.rs" | cut -c1-16,65- >> "$P0/mut_det.rc"
python3 "$P0/mutantes_det.py" "$WT" "$P0/mutantes_det.json" > "$P0/mutantes_det.log" 2>&1
echo "mutantes rc=$?" >> "$P0/mut_det.rc"
for f in checks.rs modules.rs checks/tests.rs; do
  cmp "$V/$f" "$P0/fonte_det_bom/$f" && echo "restaurado $f (cmp rc=0)" >> "$P0/mut_det.rc"
done
ls "$V"/*.orig_mut "$V"/checks/*.orig_mut >> "$P0/mut_det.rc" 2>&1
echo FIM >> "$P0/mut_det.rc"
