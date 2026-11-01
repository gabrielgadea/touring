#!/usr/bin/env bash
# P0B (aceite 2–3, 17/10/2026): mutantes (VERDE → VERMELHO → por regra) → release reproduzível → juízes v2/V3 do binário
# do P0B nos corpora vivo e bak → réguas R1/R2 contra o INT97 final 8fd0efb0 (saídas já gravadas em
# int97/resultados-2026-10-17/juizes_{ev,bak}). Os juízes reservam a classe RUST do hw-slot: nenhum cargo roda em paralelo
# e o p95 do V3 não é contaminado. Uso: bash pipeline_p0b.sh <pasta-de-saida>
set -u
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics
S=$D/p0b/scripts
OUT=${1:?pasta de saida}
HW=/home/gabrielgadea/projects/touring/scripts/hw-slot
INT=$D/int97/resultados-2026-10-17
REF=/home/gabrielgadea/.claude/touring/vgp/binarios/8fd0efb0/touring
VGP=/home/gabrielgadea/.claude/touring/vgp
mkdir -p "$OUT"
L=$OUT/pipeline.rc
: > "$L"

"$HW" --class rust --label p0b-mutantes --wait 7200 -- bash "$S/mut_p0b.sh" "$OUT/mutantes"
echo "mutantes rc=$? $(date -Is)" >> "$L"
grep -a -c ': MORTO ' "$OUT/mutantes/mutantes.log" | sed 's/^/  mortos=/' >> "$L"
grep -a -E ': (SOBREVIVEU|ERRO-DE-COMPILACAO) ' "$OUT/mutantes/mutantes.log" | sed 's/^/  ATENCAO /' >> "$L"
grep -a -q '^ABORTA' "$OUT/mutantes/rc.txt" && { echo "VERDE falhou — para aqui" >> "$L"; exit 1; }

"$HW" --class rust --label p0b-release --wait 7200 -- bash "$S/release_p0b.sh" "$OUT/release"
echo "release rc=$? $(date -Is)" >> "$L"
NOVO=$(grep -a -o 'congelado em [^ ]*' "$OUT/release/release.rc" | head -1 | cut -d' ' -f3)/touring
grep -a -q '^REPRODUZIVEL' "$OUT/release/release.rc" || { echo "release NÃO reproduzível — para aqui" >> "$L"; exit 2; }
echo "novo=$NOVO" >> "$L"

for c in ev bak; do
  corpus=$VGP/corpus_v0d
  [ "$c" = bak ] && corpus=$VGP/corpus_v0d.bak-2026-10-09
  ROTULOS="novo_v2 novo_v3" "$HW" --class rust --label "p0b-juiz-$c" --wait 7200 -- \
    bash "$S/juizes_p0b.sh" "$REF" "$NOVO" "$OUT/juizes_$c" "$corpus"
  echo "juizes $c rc=$? $(date -Is)" >> "$L"
  for v in v2 v3; do
    python3 -I "$S/comparar_porid2.py" "$INT/juizes_$c/novo_$v.porid.json" "$OUT/juizes_$c/novo_$v.porid.json" \
      "$OUT/juizes_$c/regua_$v.json" > "$OUT/juizes_$c/regua_$v.txt" 2>&1
    echo "regua $c $v rc=$?" >> "$L"
  done
done
echo FIM >> "$L"
