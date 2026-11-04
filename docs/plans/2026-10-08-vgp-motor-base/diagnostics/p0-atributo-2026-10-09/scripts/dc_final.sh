#!/usr/bin/env bash
_HOME_SLUG=$(printf '%s' "$HOME" | tr -c 'A-Za-z0-9' '-')  # Claude Code project-dir slug of $HOME
# Depois que os mutantes terminarem (dc_chain.rc): suite limpa + build release + congelamento (det_a.sh) e, se
# congelou, as medidas (dc_medidas.sh). Cada etapa pede a propria reserva do hw-slot.
P0=/tmp/claude-1000/${_HOME_SLUG}-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
until [ -s "$P0/dc_chain.rc" ]; do sleep 30; done
sleep 45
cd ${HOME}/projects/touring-wt-vgp-attr || exit 9
${HOME}/projects/touring/scripts/hw-slot --class rust --wait 21600 -- bash "$P0/det_a.sh"
if grep -q '^congelado em' "$P0/det_a.rc"; then
  ${HOME}/projects/touring/scripts/hw-slot --wait 21600 -- bash "$P0/dc_medidas.sh"
fi
echo FIM > "$P0/dc_final.rc"
