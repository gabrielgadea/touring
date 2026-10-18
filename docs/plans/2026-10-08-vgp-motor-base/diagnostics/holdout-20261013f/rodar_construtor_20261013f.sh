#!/usr/bin/env bash
# Reconstrucao do held-out Python como 20261013f (fecho transitivo). Mesma semente/parametros do 20261013.
V=/home/gabrielgadea/.claude/touring/vgp
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/holdout-20261013f
cd /tmp
TOURING_CODE_MODE=native /home/gabrielgadea/projects/touring/scripts/hw-slot --wait 3300 -- nice -n 19 python3 -I -B /home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/instrumentos-2026-10-09/construir_disjunto.py \
  --seed 20261013 --n-arquivo 600 --n-inline-por-estrato 200 --n-tmp 100 --teto 0.03 \
  --saida $V/corpus_holdout_disjunto_20261013f \
  --excluir-corpus $V/corpus_holdout_disjunto --excluir-corpus $V/corpus_holdout_disjunto_20261011 \
  --excluir-corpus $V/corpus_v0d.bak-20261009-pre --excluir-corpus $V/corpus_v0d_staging_evidencia \
  --excluir-corpus $V/corpus_v0d.bak-2026-10-09 \
  --exigir-estado-completo --congelados $D/congelados.json --cache /tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/ho13/cache_universo_20261013.json
