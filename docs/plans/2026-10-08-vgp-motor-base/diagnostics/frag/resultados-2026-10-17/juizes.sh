set -u
cd /home/gabrielgadea/projects/analise || exit 9
export TOURING_CODE_MODE=native
for c in vivo bak; do
  corpus=/home/gabrielgadea/.claude/touring/vgp/corpus_v0d; [ "$c" = bak ] && corpus=/home/gabrielgadea/.claude/touring/vgp/corpus_v0d.bak-2026-10-09
  for modo in v2 v3; do
    for lado in base:/home/gabrielgadea/.claude/touring/vgp/binarios/8fd0efb0/touring frag:/home/gabrielgadea/.claude/touring/vgp/binarios/e59f61e2/touring; do
      n=${lado%%:*}; bin=${lado#*:}
      extra=""; [ "$modo" = v3 ] && extra="--arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --base /home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/frag/resultados-2026-10-17/juizes/${n}_${c}_v2.json"
      python3 -I /home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/frag/scripts/juiz_por_id.py --por-id /home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/frag/resultados-2026-10-17/juizes/${n}_${c}_${modo}.porid.json --touring "$bin" --v5-estrito --corpus "$corpus" --json /home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/frag/resultados-2026-10-17/juizes/${n}_${c}_${modo}.json $extra > /home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/frag/resultados-2026-10-17/juizes/${n}_${c}_${modo}.out 2>&1
      echo "$n $c $modo rc=$? $(date +%T)" >> /home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/frag/resultados-2026-10-17/juizes/juizes.rc
    done
  done
done
