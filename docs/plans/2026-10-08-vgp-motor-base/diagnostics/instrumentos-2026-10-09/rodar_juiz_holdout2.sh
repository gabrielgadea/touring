#!/usr/bin/env bash
# Igual a rodar_juiz_holdout.sh, mas sobre o corpus HIBRIDO (negativos do holdout + positivos de controle do vivo).
# Espera a V3 acabar (pgrep "[j]uiz_motor_vgp.py.*camadas" vazio em 2 checagens seguidas), roda o juiz com o binario
# congelado da Kimi (e935) e registra se alguma V3 apareceu DURANTE a medicao.
SP=/tmp/claude-1000/-home-gabrielgadea-projects-analise/e7c33fba-4d8d-4026-84db-fe7083ece971/scratchpad
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts
KIMI=$SP/touring_kimi_e935
CTL=$SP/corpus_holdout_ctl
LOG=$SP/rodar_juiz_holdout2.log
: > "$LOG"
log() { echo "$(date -Is) $*" >> "$LOG"; }

log "inicio da espera | binario sha $(sha256sum "$KIMI" | cut -c1-16)"
vazio=0
espera=0
while [ "$vazio" -lt 2 ]; do
  if pgrep -f "[j]uiz_motor_vgp.py.*camadas" > /dev/null; then vazio=0; else vazio=$((vazio + 1)); fi
  [ "$vazio" -lt 2 ] && { sleep 5; espera=$((espera + 5)); }
done
log "V3 ausente; esperei ${espera}s | load $(cut -d' ' -f1-3 /proc/loadavg)"

cd /tmp || exit 9
TOURING_CODE_MODE=native python3 -I "$D/juiz_motor_vgp.py" --touring "$KIMI" --v5-estrito --corpus "$CTL" \
  --json "$SP/k_v2_holdout.json" > "$SP/k_v2_holdout.out" 2>&1 &
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
echo "rc=$rc" >> "$SP/k_v2_holdout.out"
