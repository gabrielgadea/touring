#!/usr/bin/env bash
# Build the new sealed 20261101 held-out, excluding every earlier corpus.
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/holdout-20261101
C=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/holdout-20261031
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/064bba23-c3ac-461e-b4f6-e2a4d998460e/scratchpad/ho31
V=/home/gabrielgadea/.claude/touring/vgp
exec "$C/rodar_construtor_holdout.sh" 20261101 "$D/congelados.json" "$S/cache_universo_20261101.json" 20261101 \
  --excluir-corpus "$V/corpus_holdout_disjunto_20261031" \
  --excluir-corpus "$V/corpus_holdout_disjunto_20261031b"
