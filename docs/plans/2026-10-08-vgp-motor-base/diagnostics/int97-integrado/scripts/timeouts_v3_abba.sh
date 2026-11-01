#!/usr/bin/env bash
# Do the layer-2 (pyright) timeouts follow the BINARY or the MACHINE? The integrated release's V3 judges showed
# 521 timeouts on ev against 46 for INT97 and 56 for P0B (reconciliation of 18/10/2026). This runs the V3 judge on one
# corpus four times in ABBA order (ref, novo, novo, ref) inside ONE rust reservation, so both binaries share the same
# machine window, and counts the "prazo ... estourado" layer-2 states per run. The `--base` v2 summary only feeds the
# report's deltas, never the per-id records, so one v2 summary serves every run.
# Usage: hw-slot --class rust --wait 7200 -- bash timeouts_v3_abba.sh <ref> <novo> <corpus> <base_v2.json> <out>
set -u
REF=${1:?ref}
NOVO=${2:?novo}
CORPUS=${3:?corpus}
BASE=${4:?base v2 json}
OUT=${5:?out}
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
JUIZ=$B/scripts/juiz_motor_vgp.py
INST=$B/diagnostics/instrumentos-2026-10-09/rodar_juiz_instrumentado.py
mkdir -p "$OUT"
: > "$OUT/abba.rc"
cd /home/gabrielgadea/projects/analise || exit 9
export TOURING_CODE_MODE=native
n=0
for lado in A B B A; do
  n=$((n + 1))
  bin=$REF
  [ "$lado" = B ] && bin=$NOVO
  rot="${n}_${lado}"
  python3 -I "$INST" "$JUIZ" "$OUT/$rot.porid.json" --touring "$bin" --v5-estrito --corpus "$CORPUS" --json "$OUT/$rot.json" \
    --arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --base "$BASE" > "$OUT/$rot.out" 2>&1
  rc=$?
  t=$(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); d=d.get("por_id", d); print(sum(str(r.get("camada2") or "").startswith("unknown: prazo de") for r in d.values()), len(d))' "$OUT/$rot.porid.json" 2> /dev/null)
  echo "$rot rc=$rc timeouts/ids=${t:-ilegivel} temp=$(sensors 2> /dev/null | grep -m1 Package | awk '{print $4}') $(date -Is)" >> "$OUT/abba.rc"
done
echo FIM >> "$OUT/abba.rc"
