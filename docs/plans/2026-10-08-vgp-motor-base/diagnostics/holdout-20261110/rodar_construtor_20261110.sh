#!/usr/bin/env bash
# Build the sealed 20261110 held-out with local dependencies frozen by Git; never run a judge.
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/holdout-20261110
C=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/holdout-20261031
V=/home/gabrielgadea/.claude/touring/vgp
exec "$C/rodar_construtor_holdout.sh" 20261110 "$D/congelados.json" "$V/cache_universo_20261110.json" 20261110 \
  --excluir-corpus "$V/corpus_holdout_disjunto_20261031" \
  --excluir-corpus "$V/corpus_holdout_disjunto_20261031b" \
  --excluir-corpus "$V/corpus_holdout_disjunto_20261101" \
  --excluir-corpus "$V/corpus_holdout_disjunto_20261101c"
