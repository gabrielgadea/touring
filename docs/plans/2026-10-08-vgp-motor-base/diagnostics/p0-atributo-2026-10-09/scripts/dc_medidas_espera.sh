#!/usr/bin/env bash
# espera o det_a.sh DESTA rodada terminar (arquivo mais novo que dc_t0, com FIM e sha diferente do 70e0051a) e mede
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
until [ "$P0/det_a.rc" -nt "$P0/dc_t0" ] && grep -q '^FIM' "$P0/det_a.rc" && grep -q '^sha256' "$P0/det_a.rc" && ! grep -q '^sha256 70e0051a' "$P0/det_a.rc"; do
  if [ -s "$P0/dc_chain.rc" ]; then echo "cadeia terminou sem novo binario" > "$P0/dc_medidas.rc"; exit 9; fi
  sleep 30
done
exec /home/gabrielgadea/projects/touring/scripts/hw-slot --wait 21600 -- bash "$P0/dc_medidas.sh"
