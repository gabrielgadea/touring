#!/usr/bin/env bash
# Uma unica reserva do hw-slot: RED do dataclass (com marcador), mutantes (com marcador), suite limpa + build
# release + congelamento. Sequencial; cada etapa grava o proprio .rc.
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
bash "$P0/dc_red.sh"
bash "$P0/det_a.sh"
bash "$P0/mut_dc.sh"
echo FIM > "$P0/dc_chain.rc"
