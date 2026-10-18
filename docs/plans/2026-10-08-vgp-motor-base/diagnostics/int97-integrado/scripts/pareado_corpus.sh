#!/usr/bin/env bash
# Protocol of the latency gate for ONE corpus: A/A calibration (ref × ref) and then A/B (ref × novo), both v2 (layer 1,
# the 150 ms contract), in the same environment the judges use (cwd analise, TOURING_CODE_MODE=native). Runs INSIDE one
# rust reservation of the hw-slot, so A/A and A/B share the same machine window:
#   hw-slot --class rust --wait 7200 -- bash pareado_corpus.sh <bin_ref> <bin_novo> <corpus> <pasta> [d|c]
# Phase D (default, PREREGISTRO-FASE-D-2026-10-18.md): every motor call goes through conta_custo.py (retired
# instructions + tree CPU); the A/A chooses the ruler (instructions, else CPU) and the A/B is read on THAT ruler.
# Phase c = the C' protocol of 17/10/2026 (wall time only), unchanged.
# JUIZ_EXTRA (env, optional, word-split): extra judge arguments, e.g. the V3 `--arg-motor=--camadas --arg-motor=1,2`.
# Exit: 0 SEM_REGRESSAO · 1 REGRESSAO · 2 PROTOCOLO_INADEQUADO (A/A failed → the A/B is not read) · 3 INCONCLUSIVO
#       · 4 misuse
set -u
REF=${1:?bin ref}
NOVO=${2:?bin novo}
CORPUS=${3:?corpus}
OUT=${4:?pasta}
FASE=${5:-d}
case "$FASE" in d | c) ;; *) echo "fase '$FASE' invalida (d|c)" >&2; exit 4 ;; esac
S=$(cd "$(dirname "$0")" && pwd)
JUIZ=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/juiz_motor_vgp.py
mkdir -p "$OUT"
cd /home/gabrielgadea/projects/analise || exit 9
export TOURING_CODE_MODE=native
SEM_CUSTO=()
[ "$FASE" = c ] && SEM_CUSTO=(--sem-custo)
for modo in aa ab; do
  b=$REF
  [ "$modo" = ab ] && b=$NOVO
  # shellcheck disable=SC2086 # JUIZ_EXTRA is meant to be word-split
  python3 -I "$S/tempos_pareados.py" ${SEM_CUSTO[@]+"${SEM_CUSTO[@]}"} "$JUIZ" "$REF" "$b" "$OUT/pareado_$modo.json" \
    --v5-estrito --corpus "$CORPUS" --json "$OUT/juiz_$modo.json" ${JUIZ_EXTRA:-} > "$OUT/pareado_$modo.out" 2>&1
  echo "pareado $modo rc=$? $(date -Is) temp=$(sensors 2> /dev/null | grep -m1 Package | awk '{print $4}')" >> "$OUT/pareado.rc"
  if [ "$FASE" = c ]; then
    python3 -I "$S/analisa_pareado.py" "$OUT/pareado_$modo.json" --modo "$modo" --fase c --json "$OUT/veredito_$modo.json" > /dev/null
  elif [ "$modo" = ab ]; then
    python3 -I "$S/analisa_pareado.py" "$OUT/pareado_$modo.json" --modo ab --aa "$OUT/veredito_aa.json" --json "$OUT/veredito_$modo.json" > /dev/null
  else
    python3 -I "$S/analisa_pareado.py" "$OUT/pareado_$modo.json" --modo aa --json "$OUT/veredito_$modo.json" > /dev/null
  fi
  rc=$?
  echo "veredito $modo rc=$rc $(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print(d["veredito"], "regua=" + str(d.get("regua")))' "$OUT/veredito_$modo.json")" >> "$OUT/pareado.rc"
  if [ "$modo" = aa ] && [ "$rc" -ne 0 ]; then
    echo "A/A não validou o protocolo — A/B não é lido (pré-registro)" >> "$OUT/pareado.rc"
    exit "$rc"
  fi
done
exit "$rc"
