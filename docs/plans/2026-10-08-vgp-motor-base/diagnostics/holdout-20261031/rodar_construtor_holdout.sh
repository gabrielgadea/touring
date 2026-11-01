#!/usr/bin/env bash
# Shared disjoint held-out constructor: label, frozen manifest, and cache path.
set -u
ROTULO=${1:?rótulo do corpus}
CONGELADOS=${2:?manifesto de congelados}
CACHE=${3:?cache do universo}
SEED=${4:?semente}
shift 4
V=/home/gabrielgadea/.claude/touring/vgp
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
cd /tmp
TOURING_CODE_MODE=native /home/gabrielgadea/projects/touring/scripts/hw-slot --wait 3300 -- nice -n 19 python3 -I -B \
  "$B/diagnostics/instrumentos-2026-10-09/construir_disjunto.py" \
  --seed "$SEED" --n-arquivo 600 --n-inline-por-estrato 200 --n-tmp 100 --teto 0.03 \
  --saida "$V/corpus_holdout_disjunto_$ROTULO" \
  --excluir-corpus "$V/corpus_holdout_disjunto" \
  --excluir-corpus "$V/corpus_holdout_disjunto_20261011" \
  --excluir-corpus "$V/corpus_v0d.bak-20261009-pre" \
  --excluir-corpus "$V/corpus_v0d_staging_evidencia" \
  --excluir-corpus "$V/corpus_v0d.bak-2026-10-09" \
  --excluir-corpus "$V/corpus_holdout_disjunto_20261013" \
  --excluir-corpus "$V/corpus_holdout_disjunto_20261013f" \
  --exigir-estado-completo --congelados "$CONGELADOS" \
  --cache "$CACHE" "$@"
