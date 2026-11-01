#!/usr/bin/env bash
# P0M tentativa 2 (10/10/2026, touring-09). A tentativa 1 (tentativa1-corpus-puro/) deu 0 chamadas: o juiz recusa corpus
# sem positivo ("corpus degenerado"). Procedimento registrado em diagnostics/holdout/RELATORIO.md §3.1: corpus HIBRIDO no
# scratchpad = copia do held-out + positivos do vivo SO como controle de liveness; os falsos contam so nos negativos do
# held-out, que fica puro. v2 como no 20261010 (cd /tmp, TOURING_CODE_MODE=native); V3 como no P2 (cwd analise).
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base; O=$B/diagnostics/p0m
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad
C=$HOME/.claude/touring/vgp/corpus_holdout_disjunto_20261011; SELO=$B/diagnostics/holdout-20261011/selo.json
CTL=$S/p0m_hibrido_20261011; I=$B/diagnostics/instrumentos-2026-10-09
J=$B/scripts/juiz_motor_vgp.py; HW=/home/gabrielgadea/projects/touring/scripts/hw-slot
P0=$HOME/.claude/touring/vgp/binarios/f55ec4b5/touring; BASE=$HOME/.claude/touring/vgp/binarios/e935aaf1/touring
R=$O/rcs2.txt; : > $R
date -u +%FT%TZ > $O/inicio2_utc.txt
sha256sum $P0 $BASE $J $I/montar_ctl_generico.py > $O/shas2.txt
python3 $B/scripts/confere_selo.py --corpus $C --selo $SELO > $O/selo_antes2.out 2>&1; echo "selo_antes rc=$?" >> $R
grep -q "selo_antes rc=0" $R || { echo "ABORTADO: selo violado" >> $R; exit 3; }
[ -e $CTL ] && { echo "ABORTADO: $CTL ja existe" >> $R; exit 4; }
(cd /tmp && python3 -I $I/montar_ctl_generico.py $C $CTL) > $O/hibrido.out 2>&1; echo "hibrido rc=$?" >> $R
(cd $CTL && find . -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -c1-16) > $O/hibrido_sha_tree.txt
for bin in f55ec4b5:$P0 e935aaf1:$BASE; do
  n=${bin%%:*}; t=${bin#*:}
  (cd /tmp && TOURING_CODE_MODE=native $HW --label p0m2-$n-v2 --wait 7200 -- python3 -I $J --touring $t --v5-estrito --corpus $CTL --exigir-limite-superior --json $O/${n}_v2.json > $O/${n}_v2.out 2>&1); echo "$n v2 rc=$?" >> $R
done
for bin in f55ec4b5:$P0 e935aaf1:$BASE; do
  n=${bin%%:*}; t=${bin#*:}
  (cd /home/gabrielgadea/projects/analise && $HW --label p0m2-$n-v3 --wait 7200 -- python3 $J --touring $t --corpus $CTL --v5-estrito --arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --exigir-limite-superior --json $O/${n}_v3.json > $O/${n}_v3.out 2>&1); echo "$n V3 rc=$?" >> $R
done
python3 $B/scripts/confere_selo.py --corpus $C --selo $SELO > $O/selo_depois2.out 2>&1; echo "selo_depois rc=$?" >> $R
date -u +%FT%TZ > $O/fim2_utc.txt
echo FIM >> $R
