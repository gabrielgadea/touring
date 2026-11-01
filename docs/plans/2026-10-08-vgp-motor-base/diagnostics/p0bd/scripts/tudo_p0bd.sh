#!/usr/bin/env bash
# gates + mutantes numa unica reserva (a fila do slot so e paga uma vez)
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0bd
bash "$D/scripts/gates_p0bd.sh" "$D/logs"
bash "$D/scripts/mut_p0bd.sh" "$D/mutantes"
