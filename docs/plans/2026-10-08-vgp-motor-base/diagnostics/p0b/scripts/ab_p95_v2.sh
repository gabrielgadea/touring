#!/usr/bin/env bash
# A/B de latência do juiz v2 (camada 1) no corpus bak: INT97 final (8fd0efb0) × P0B (4047373f), alternados,
# 2 rodadas cada, dentro de UMA reserva rust do hw-slot (nenhum cargo em paralelo). Decide se o p95 175,6 ms do
# P0B no bak (teto 150; o INT97 deu 137,7) é regressão do motor ou ruído de carga.
#   hw-slot --class rust --label p0b-ab-p95 --wait 7200 -- bash ab_p95_v2.sh <pasta-de-saida>
set -u
OUT=${1:?pasta de saida}
mkdir -p "$OUT"
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
JUIZ=$B/scripts/juiz_motor_vgp.py
CORPUS=/home/gabrielgadea/.claude/touring/vgp/corpus_v0d.bak-2026-10-09
BIN=/home/gabrielgadea/.claude/touring/vgp/binarios
cd /home/gabrielgadea/projects/analise || exit 9
export TOURING_CODE_MODE=native
: > "$OUT/ab.rc"
for r in 1 2; do
  for b in int97:8fd0efb0 p0b:4047373f; do
    nome=${b%%:*}
    python3 -I "$JUIZ" --touring "$BIN/${b#*:}/touring" --v5-estrito --corpus "$CORPUS" --json "$OUT/${nome}_$r.json" \
      > "$OUT/${nome}_$r.out" 2>&1
    p95=$(python3 -c "import json,sys; print(round(json.load(open(sys.argv[1])).get('p95_ms') or -1, 1))" "$OUT/${nome}_$r.json")
    echo "$nome rodada $r rc=$? p95=$p95 temp=$(sensors 2> /dev/null | grep -m1 Package | awk '{print $4}') $(date +%T)" >> "$OUT/ab.rc"
  done
done
echo FIM >> "$OUT/ab.rc"
cat "$OUT/ab.rc"
