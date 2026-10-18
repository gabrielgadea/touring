#!/usr/bin/env bash
SC=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
W=/home/gabrielgadea/projects/touring/target/wt-vrs2-var
: > "$SC/variantes7_medidas.rcs"
for v in V21 V20 V19 VB V17 V18; do
  bash "$SC/medida_var.sh" "$v" "$W/$v/touring"
  echo "$v rc=$?" >> "$SC/variantes7_medidas.rcs"
done
echo "MEDIDAS7 CONCLUIDAS" >> "$SC/variantes7_medidas.rcs"
