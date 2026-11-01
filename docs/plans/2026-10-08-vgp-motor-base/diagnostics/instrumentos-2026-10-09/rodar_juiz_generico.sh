#!/usr/bin/env bash
# rodar_juiz_generico.sh <binario> <corpus> <json> <log> [args extras do juiz...]
# Espera a V3 acabar (pgrep "[j]uiz_motor_vgp.py.*camadas" vazio em 2 checagens), roda o juiz stock e registra se
# alguma V3 apareceu durante a medicao.
BIN=$1
CORPUS=$2
OUTJSON=$3
LOG=$4
shift 4
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts
: > "$LOG"
log() { echo "$(date -Is) $*" >> "$LOG"; }
log "inicio | binario sha $(sha256sum "$BIN" | cut -c1-16) | corpus $CORPUS"
vazio=0
while [ "$vazio" -lt 2 ]; do
  if pgrep -f "[j]uiz_motor_vgp.py.*camadas" > /dev/null; then vazio=0; else vazio=$((vazio + 1)); fi
  [ "$vazio" -lt 2 ] && sleep 5
done
log "V3 ausente | load $(cut -d' ' -f1-3 /proc/loadavg)"
cd /tmp || exit 9
TOURING_CODE_MODE=native python3 -I "$D/juiz_motor_vgp.py" --touring "$BIN" --v5-estrito --corpus "$CORPUS" \
  --json "$OUTJSON" "$@" > "${OUTJSON%.json}.out" 2>&1 &
PID=$!
amostras=0
com_v3=0
while kill -0 "$PID" 2> /dev/null; do
  amostras=$((amostras + 1))
  pgrep -f "[j]uiz_motor_vgp.py.*camadas" > /dev/null && com_v3=$((com_v3 + 1))
  sleep 2
done
wait "$PID"
rc=$?
log "juiz terminou rc=$rc | amostras de V3 durante a medicao: $com_v3 de $amostras | load $(cut -d' ' -f1-3 /proc/loadavg)"
echo "rc=$rc" >> "${OUTJSON%.json}.out"
