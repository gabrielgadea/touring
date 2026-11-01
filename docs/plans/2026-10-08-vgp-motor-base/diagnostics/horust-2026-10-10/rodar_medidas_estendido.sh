#!/usr/bin/env bash
# HORUST: as medicoes com o reconhecedor de cargo ESTENDIDO (HORUST_ESTENDIDO=1). Saida em dados_estendido/.
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/horust-2026-10-10
cd /tmp
export TOURING_CODE_MODE=native HORUST_ESTENDIDO=1
for passo in "medir_horust.py $D/dados_estendido/sessoes_vgp.json $D/dados_estendido" "negativos_janela.py $D/dados_estendido/sessoes_vgp.json $D/dados_estendido/negativos_janela.json"; do
  nome=${passo%%.py*}
  /home/gabrielgadea/projects/touring/scripts/hw-slot --wait 3300 -- nice -n 19 python3 -I -B $D/$passo > $D/dados_estendido/$nome.log 2>&1
  echo "$nome rc=$?" >> $D/dados_estendido/medidas.rcs
done
echo fim >> $D/dados_estendido/medidas.rcs
