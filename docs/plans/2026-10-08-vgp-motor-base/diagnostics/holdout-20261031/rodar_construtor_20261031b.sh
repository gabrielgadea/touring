#!/usr/bin/env bash
# Rebuild the held-out as 20261031b after the timestamp-provenance fix.
V=/home/gabrielgadea/.claude/touring/vgp
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
D=$B/diagnostics/holdout-20261031
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/064bba23-c3ac-461e-b4f6-e2a4d998460e/scratchpad/ho31
cd /tmp
TOURING_CODE_MODE=native /home/gabrielgadea/projects/touring/scripts/hw-slot --wait 3300 -- nice -n 19 python3 -I -B \
  "$B/diagnostics/instrumentos-2026-10-09/construir_disjunto.py" \
  --seed 20261031 --n-arquivo 600 --n-inline-por-estrato 200 --n-tmp 100 --teto 0.03 \
  --saida "$V/corpus_holdout_disjunto_20261031b" \
  --excluir-corpus "$V/corpus_holdout_disjunto" \
  --excluir-corpus "$V/corpus_holdout_disjunto_20261011" \
  --excluir-corpus "$V/corpus_v0d.bak-20261009-pre" \
  --excluir-corpus "$V/corpus_v0d_staging_evidencia" \
  --excluir-corpus "$V/corpus_v0d.bak-2026-10-09" \
  --excluir-corpus "$V/corpus_holdout_disjunto_20261013" \
  --excluir-corpus "$V/corpus_holdout_disjunto_20261013f" \
  --exigir-estado-completo --congelados "$D/congelados_20261031b.json" \
  --cache "$S/cache_universo_20261031b.json"
