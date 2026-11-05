#!/usr/bin/env bash
# HORUST: prova comportamental no DEV. 1) gera o corpus com o crate (e com os crates irmaos) no instante; 2) roda o motor da vrs-porta (binario
# congelado 42830beb) por id; 3) compara com o que o mesmo binario devolveu SEM overlay (ids_depois_natural.json).
D=${HOME}/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/horust-2026-10-10
V=${HOME}/.claude/touring/vgp
BIN=$V/binarios/42830beb/touring
cd /tmp
export TOURING_CODE_MODE=native
for modo in crate ws; do
  OUT=$V/horust_medicao/$modo
  ${HOME}/projects/touring/scripts/hw-slot --wait 3300 -- nice -n 19 python3 -I -B $D/dev_com_overlay.py $OUT $modo > $D/dados_dev_$modo.build.log 2>&1
  echo "build $modo rc=$?" >> $D/dados_dev.rcs
  ${HOME}/projects/touring/scripts/hw-slot --wait 3300 -- python3 -I -B ${HOME}/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/vrs-porta-2026-10-10/scripts/medir_ids.py --touring $BIN --corpus $OUT/corpus --out $D/dados_dev_$modo.ids.json > $D/dados_dev_$modo.motor.log 2>&1
  echo "motor $modo rc=$?" >> $D/dados_dev.rcs
done
echo fim >> $D/dados_dev.rcs
