#!/usr/bin/env bash
_HOME_SLUG=$(printf '%s' "$HOME" | tr -c 'A-Za-z0-9' '-')  # Claude Code project-dir slug of $HOME
SC=/tmp/claude-1000/${_HOME_SLUG}-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
W=${HOME}/projects/touring/target/wt-vrs2-var
: > "$SC/variantes7_medidas.rcs"
for v in V21 V20 V19 VB V17 V18; do
  bash "$SC/medida_var.sh" "$v" "$W/$v/touring"
  echo "$v rc=$?" >> "$SC/variantes7_medidas.rcs"
done
echo "MEDIDAS7 CONCLUIDAS" >> "$SC/variantes7_medidas.rcs"
