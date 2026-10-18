#!/usr/bin/env bash
# HORUST: as tres medicoes (cada uma sob hw-slot, nativa porque o sandbox nao le ~/.claude/projects). Ordem: positivos, negativos, outras fontes.
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/horust-2026-10-10
cd /tmp
export TOURING_CODE_MODE=native
for passo in "medir_horust.py $D/dados/sessoes_vgp.json $D/dados" "negativos_janela.py $D/dados/sessoes_vgp.json $D/dados/negativos_janela.json" "outros_projetos.py $D/dados/sessoes_vgp.json $D/dados/outros_projetos.json"; do
  nome=${passo%%.py*}
  /home/gabrielgadea/projects/touring/scripts/hw-slot --wait 3300 -- nice -n 19 python3 -I -B $D/$passo > $D/dados/$nome.log 2>&1
  echo "$nome rc=$?" >> $D/dados/medidas.rcs
done
echo fim >> $D/dados/medidas.rcs
