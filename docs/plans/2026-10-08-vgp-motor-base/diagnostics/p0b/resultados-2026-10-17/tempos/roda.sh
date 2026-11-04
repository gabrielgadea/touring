#!/usr/bin/env bash
cd ${HOME}/projects/analise || exit 9
export TOURING_CODE_MODE=native
for r in 1 2 3; do for b in int97:8fd0efb0 p0b:4047373f; do
  python3 -I ${HOME}/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0b/scripts/rodar_juiz_tempos.py ${HOME}/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/juiz_motor_vgp.py ${HOME}/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0b/resultados-2026-10-17/tempos/${b%%:*}_$r.json --touring ${HOME}/.claude/touring/vgp/binarios/${b#*:}/touring --v5-estrito --corpus ${HOME}/.claude/touring/vgp/corpus_v0d.bak-2026-10-09 --json ${HOME}/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0b/resultados-2026-10-17/tempos/${b%%:*}_$r.juiz.json > ${HOME}/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0b/resultados-2026-10-17/tempos/${b%%:*}_$r.out 2>&1
  echo "${b%%:*} $r rc=$? $(date +%T)"
done; done
