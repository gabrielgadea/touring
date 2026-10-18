#!/usr/bin/env bash
# Juiz v2 e V3 com o binario novo (70e0051a) nos dois corpora: o vivo anterior (bak) e o staging com
# evidencia (corpus_v0d). A base (e935aaf1) ja foi reexecutada por id: no bak em juizes_bak/, no ev em
# juizes_det_ev/. Roda em UMA reserva do hw-slot; sequencial de proposito (o p95 e medido).
set -u
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
R=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0-atributo-2026-10-09/resultados
NOVO=/home/gabrielgadea/.claude/touring/vgp/binarios/70e0051a/touring
VIVO=/home/gabrielgadea/.claude/touring/vgp
cp "$R/juizes_det_ev/juizes.rc" "$R/juizes_det_ev/juizes_base.rc"

ROTULOS="novo_v2 novo_v3" bash "$P0/juizes.sh" "$NOVO" "$R/juizes_det_ev" "$VIVO/corpus_v0d" > "$R/juizes_det_ev/novo.out" 2>&1
echo "ev rc=$?" > "$P0/det_juizes.rc"

mkdir -p "$R/juizes_det_bak"
for f in base_v2.json base_v2.porid.json base_v3.json base_v3.porid.json; do
  cp -p "$R/juizes_bak/$f" "$R/juizes_det_bak/$f"
done
ROTULOS="novo_v2 novo_v3" bash "$P0/juizes.sh" "$NOVO" "$R/juizes_det_bak" "$VIVO/corpus_v0d.bak-2026-10-09" > "$R/juizes_det_bak/novo.out" 2>&1
echo "bak rc=$?" >> "$P0/det_juizes.rc"
echo FIM >> "$P0/det_juizes.rc"
