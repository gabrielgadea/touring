#!/usr/bin/env bash
cd /home/gabrielgadea/projects/analise || exit 9
export TOURING_CODE_MODE=native
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
BASE=/home/gabrielgadea/.claude/touring/vgp/binarios/e935aaf1/touring
NOVO=/home/gabrielgadea/.claude/touring/vgp/binarios/f55ec4b5/touring
CORP=/home/gabrielgadea/.claude/touring/vgp/corpus_v0d.bak-2026-10-09
for i in 1 2 3; do
  for lado in base novo; do
    if [ $lado = base ]; then BIN=$BASE; else BIN=$NOVO; fi
    python3 -I $P0/juiz_tempos.py $B/scripts/juiz_motor_vgp.py $P0/tempos/${lado}_$i.json --touring $BIN --v5-estrito --corpus $CORP --json $P0/tempos/${lado}_$i.juiz.json > $P0/tempos/${lado}_$i.out 2>&1
    echo "$lado $i rc=$?" >> $P0/tempos/rc
  done
done
echo FIM >> $P0/tempos/rc
