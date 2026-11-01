#!/usr/bin/env bash
# p95 PAREADO por id (mesma maquina, no mesmo periodo): 3 repeticoes ALTERNADAS entre os binarios, minimo por id.
#  L1 (v2): e935aaf1 · f55ec4b5 · 4501e1e5        V3 (camadas 1,2): e935aaf1 · 4501e1e5
# Corpus: o vivo atual (corpus_v0d, staging com evidencia). O p95 absoluto depende da maquina (92-96 C, load 6-7);
# o que vale e a comparacao pareada. Uma reserva do hw-slot; sequencial de proposito.
set -u
cd /home/gabrielgadea/projects/analise || exit 9
export TOURING_CODE_MODE=native
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
T=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0-atributo-2026-10-09/resultados/tempos_dc
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
BIN=/home/gabrielgadea/.claude/touring/vgp/binarios
CORP=/home/gabrielgadea/.claude/touring/vgp/corpus_v0d
mkdir -p "$T"
: > "$T/rc"
for i in 1 2 3; do
  for par in "l1 e935aaf1" "l1 f55ec4b5" "l1 4501e1e5" "v3 e935aaf1" "v3 4501e1e5"; do
    set -- $par
    modo=$1; bin=$2
    extra=()
    [ "$modo" = v3 ] && extra=(--arg-motor=--camadas --arg-motor=1,2)
    python3 -I "$P0/juiz_tempos.py" "$B/scripts/juiz_motor_vgp.py" "$T/${modo}_${bin}_$i.json" \
      --touring "$BIN/$bin/touring" --v5-estrito --corpus "$CORP" "${extra[@]}" \
      --json "$T/${modo}_${bin}_$i.juiz.json" > "$T/${modo}_${bin}_$i.out" 2>&1
    echo "$modo $bin rep=$i rc=$? $(date -Is)" >> "$T/rc"
  done
done
echo FIM >> "$T/rc"
