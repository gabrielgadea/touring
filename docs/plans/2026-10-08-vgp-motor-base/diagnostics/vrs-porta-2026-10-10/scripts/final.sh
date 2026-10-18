#!/usr/bin/env bash
# Uma unica aquisicao do hw-slot: gates finais (fmt, clippy, testes, doc, release) e depois os 2 mutantes
# que a 1a rodada deixou pendentes (R11a: teste confundido; R12: gatilho desatualizado).
SC=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
bash $SC/gates.sh
python3 $SC/mutantes.py $SC/mutantes_resultado_2.json R11a-glob-externo-sem-dinamismo R12-variante-invertida
echo "FINAL CONCLUIDO rc_mutantes=$?" >> $SC/gates.rcs
