#!/usr/bin/env bash
# Uso: slot.sh <nome-do-log> <diretorio-de-trabalho> <cmd...>
# Roda <cmd> sob hw-slot (class rust), repetindo em rc 75 (sem vaga). Grava o log e o rc em
# $SC/<nome>.log / $SC/<nome>.rc. Nunca contorna a espera.
SC=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
SLOT=/home/gabrielgadea/projects/touring/scripts/hw-slot
nome=$1
dir=$2
shift 2
cd "$dir" || exit 90
rc=1
for i in 1 2 3 4 5 6 7 8; do
  "$SLOT" --class rust --wait 3600 -- "$@" > "$SC/$nome.log" 2>&1
  rc=$?
  if [ "$rc" -ne 75 ]; then break; fi
  sleep 60
done
echo "$rc" > "$SC/$nome.rc"
echo "SLOT_RC=$rc" >> "$SC/$nome.log"
exit "$rc"
