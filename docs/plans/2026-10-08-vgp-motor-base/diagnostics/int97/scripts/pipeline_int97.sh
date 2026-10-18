#!/usr/bin/env bash
# INT97, 2º passe: gates -> release reproduzível -> juízes v2/V3 do binário novo nos 2 corpora -> réguas R1/R2
# por id contra o P0 final 4501e1e5 (saídas já gravadas em p0-atributo-2026-10-09/resultados/juizes_dc_{ev,bak}).
# Para no primeiro elo vermelho. Uso: bash pipeline_int97.sh <pasta-de-saida>
set -u
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics
S=$D/int97/scripts
OUT=${1:?pasta de saida}
HW=/home/gabrielgadea/projects/touring/scripts/hw-slot
P0=$D/p0-atributo-2026-10-09/resultados
REF=/home/gabrielgadea/.claude/touring/vgp/binarios/4501e1e5/touring
VGP=/home/gabrielgadea/.claude/touring/vgp
mkdir -p "$OUT"
L=$OUT/pipeline.rc
: > "$L"

"$HW" --class rust --label int97-gates --wait 7200 -- bash "$S/gates_int97.sh" "$OUT/gates"
r=$?
echo "gates rc=$r $(date -Is)" >> "$L"
[ "$r" -eq 0 ] || exit 1

"$HW" --class rust --label int97-release --wait 7200 -- bash "$S/release_int97.sh" "$OUT/release"
r=$?
echo "release rc=$r $(date -Is)" >> "$L"
[ "$r" -eq 0 ] || exit 2
NOVO=$(grep -a '^BIN_NOVO=' "$OUT/release/release.rc" | cut -d= -f2)
echo "novo=$NOVO" >> "$L"

for c in ev bak; do
  corpus=$VGP/corpus_v0d
  [ "$c" = bak ] && corpus=$VGP/corpus_v0d.bak-2026-10-09
  ROTULOS="novo_v2 novo_v3" "$HW" --label "int97-juiz-$c" --wait 7200 -- \
    bash "$D/p0b/scripts/juizes_p0b.sh" "$REF" "$NOVO" "$OUT/juizes_$c" "$corpus"
  echo "juizes $c rc=$? $(date -Is)" >> "$L"
  for v in v2 v3; do
    python3 -I "$D/p0b/scripts/comparar_porid2.py" "$P0/juizes_dc_$c/novo_$v.porid.json" \
      "$OUT/juizes_$c/novo_$v.porid.json" "$OUT/juizes_$c/regua_$v.json" > "$OUT/juizes_$c/regua_$v.txt" 2>&1
    echo "regua $c $v rc=$?" >> "$L"
  done
done
echo FIM >> "$L"
