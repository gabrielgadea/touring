#!/usr/bin/env bash
# Uso: heavy.sh <nome-do-log> <cmd...>   (hw-slot classe heavy; repete em rc 75; nunca contorna a espera)
SC=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
SLOT=/home/gabrielgadea/projects/touring/scripts/hw-slot
nome=$1
shift
rc=1
for i in 1 2 3 4 5 6 7 8; do
  "$SLOT" --wait 3600 --label "$nome" -- "$@" > "$SC/$nome.log" 2>&1
  rc=$?
  if [ "$rc" -ne 75 ]; then break; fi
  sleep 60
done
echo "$rc" > "$SC/$nome.rc"
echo "SLOT_RC=$rc" >> "$SC/$nome.log"
exit "$rc"
