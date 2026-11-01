#!/usr/bin/env bash
# Medidas do binario FINAL (P0 + determinismo + semantica de dataclass): varredura de determinismo nos dois corpora,
# nos 7 ids da evidencia (K=10) e o juiz v2 + V3 no corpus_v0d (ev) e no bak. A base e935aaf1 ja foi reexecutada
# por id nos dois (juizes_det_ev/, juizes_det_bak/). Uma reserva heavy do hw-slot; sequencial (o p95 e medido).
set -u
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
R=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0-atributo-2026-10-09/resultados
VIVO=/home/gabrielgadea/.claude/touring/vgp
SHA=$(grep '^sha256' "$P0/det_a.rc" | tail -1 | cut -d' ' -f2)
S8=${SHA:0:8}
NOVO=$VIVO/binarios/$S8/touring
[ -x "$NOVO" ] || { echo "binario $NOVO ausente" > "$P0/dc_medidas.rc"; exit 9; }
echo "$S8" > "$P0/dc_sha8.txt"
cd /home/gabrielgadea/projects/analise || exit 9
export TOURING_CODE_MODE=native
: > "$P0/dc_medidas.rc"

python3 -I "$P0/det_varre.py" "$NOVO" "$VIVO/corpus_v0d" "$P0/det_green_${S8}_v0d.json" 5 > "$P0/det_green_${S8}_v0d.out" 2>&1
echo "sweep v0d rc=$?" >> "$P0/dc_medidas.rc"
python3 -I "$P0/det_varre.py" "$NOVO" "$VIVO/corpus_v0d.bak-2026-10-09" "$P0/det_green_${S8}_bak.json" 5 > "$P0/det_green_${S8}_bak.out" 2>&1
echo "sweep bak rc=$?" >> "$P0/dc_medidas.rc"
python3 -I "$P0/det_varre.py" "$NOVO" "$VIVO/corpus_v0d.bak-2026-10-09" "$P0/det_green_${S8}_bak_7ids.json" 10 \
  --ids 0dcf0bab45c5,15d469304a03,2463291faf76,34511b9ac100,8b7a3476e024,c876d78f82ee,cda03737f157 > "$P0/det_green_${S8}_bak_7ids.out" 2>&1
echo "sweep 7 ids rc=$?" >> "$P0/dc_medidas.rc"

for par in "ev:corpus_v0d:juizes_det_ev" "bak:corpus_v0d.bak-2026-10-09:juizes_det_bak"; do
  IFS=: read -r rot corp refdir <<< "$par"
  OUT=$R/juizes_dc_$rot
  mkdir -p "$OUT"
  for f in base_v2.json base_v2.porid.json base_v3.json base_v3.porid.json; do cp -p "$R/$refdir/$f" "$OUT/$f"; done
  ROTULOS="novo_v2 novo_v3" bash "$P0/juizes.sh" "$NOVO" "$OUT" "$VIVO/$corp" > "$OUT/novo.out" 2>&1
  echo "juizes $rot rc=$?" >> "$P0/dc_medidas.rc"
done
echo FIM >> "$P0/dc_medidas.rc"
