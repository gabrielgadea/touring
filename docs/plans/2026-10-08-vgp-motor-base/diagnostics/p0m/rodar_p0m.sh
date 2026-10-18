#!/usr/bin/env bash
# P0M (10/10/2026, touring-09): binario congelado do P0 (f55ec4b5) no held-out disjunto 20261011 SELADO,
# com --exigir-limite-superior (CP95 <= 1% por balde), v2 e V3; e935 como referencia por id. Uso UNICO do held-out
# para o P0: se reprovar, o conserto exige held-out novo (memoria juiz-taxa-pontual-aprova-o-que-o-limite-reprova).
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base; O=$B/diagnostics/p0m
C=$HOME/.claude/touring/vgp/corpus_holdout_disjunto_20261011; SELO=$B/diagnostics/holdout-20261011/selo.json
J=$B/scripts/juiz_motor_vgp.py; HW=/home/gabrielgadea/projects/touring/scripts/hw-slot
P0=$HOME/.claude/touring/vgp/binarios/f55ec4b5/touring; BASE=$HOME/.claude/touring/vgp/binarios/e935aaf1/touring
cd /home/gabrielgadea/projects/analise || exit 9
date -u +%FT%TZ > $O/inicio_utc.txt
sha256sum $P0 $BASE $J > $O/shas.txt
python3 $B/scripts/confere_selo.py --corpus $C --selo $SELO > $O/selo_antes.out 2>&1; echo "selo_antes rc=$?" >> $O/rcs.txt
grep -q "rc=0" $O/rcs.txt || { echo "ABORTADO: selo violado antes do juiz" >> $O/rcs.txt; exit 3; }
V3="--arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000"
for bin in f55ec4b5:$P0 e935aaf1:$BASE; do
  n=${bin%%:*}; t=${bin#*:}
  $HW --label p0m-$n-v2 --wait 7200 -- python3 $J --touring $t --corpus $C --v5-estrito --exigir-limite-superior --json $O/${n}_v2.json > $O/${n}_v2.out 2>&1; echo "$n v2 rc=$?" >> $O/rcs.txt
  $HW --label p0m-$n-v3 --wait 7200 -- python3 $J --touring $t --corpus $C --v5-estrito $V3 --exigir-limite-superior --json $O/${n}_v3.json > $O/${n}_v3.out 2>&1; echo "$n V3 rc=$?" >> $O/rcs.txt
done
python3 $B/scripts/confere_selo.py --corpus $C --selo $SELO > $O/selo_depois.out 2>&1; echo "selo_depois rc=$?" >> $O/rcs.txt
date -u +%FT%TZ > $O/fim_utc.txt
echo FIM >> $O/rcs.txt
