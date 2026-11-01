#!/usr/bin/env bash
# Rebuild the held-out as 20261031b after the timestamp-provenance fix.
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/holdout-20261031
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/064bba23-c3ac-461e-b4f6-e2a4d998460e/scratchpad/ho31
exec "$D/rodar_construtor_holdout.sh" 20261031b "$D/congelados_20261031b.json" "$S/cache_universo_20261031b.json" 20261031
