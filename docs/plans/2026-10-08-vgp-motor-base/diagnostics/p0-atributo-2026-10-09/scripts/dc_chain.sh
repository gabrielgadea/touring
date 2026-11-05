#!/usr/bin/env bash
_HOME_SLUG=$(printf '%s' "$HOME" | tr -c 'A-Za-z0-9' '-')  # Claude Code project-dir slug of $HOME
# Uma unica reserva do hw-slot: RED do dataclass (com marcador), mutantes (com marcador), suite limpa + build
# release + congelamento. Sequencial; cada etapa grava o proprio .rc.
P0=/tmp/claude-1000/${_HOME_SLUG}-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
bash "$P0/dc_red.sh"
bash "$P0/det_a.sh"
bash "$P0/mut_dc.sh"
echo FIM > "$P0/dc_chain.rc"
