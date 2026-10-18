#!/usr/bin/env bash
# Phase D of the VGP latency gate on the INT97 integrated release (PREREGISTRO-FASE-D-2026-10-18.md, sha
# def79d90bf510b17): for each corpus (ev, then bak), A/A (ref x ref) chooses the ruler, then A/B (ref x novo) is read on
# that ruler; v2 (layer 1) is the GATE, V3 (layers 1,2) is only reported. Reference FIXED at 8fd0efb0. Waits for the
# integrated pipeline to finish (FIM in its pipeline.rc) so the judges' p95 and this run never share the machine; each
# corpus runs in ONE rust reservation of the hw-slot, so its A/A and A/B share the same machine window.
# Usage: bash fase_d_int97i.sh <pipeline-results-dir> <out-dir>
set -u
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics
S=$D/int97-integrado/scripts
RES=${1:?pipeline results dir}
OUT=${2:?out dir}
HW=/home/gabrielgadea/projects/touring/scripts/hw-slot
REF=/home/gabrielgadea/.claude/touring/vgp/binarios/8fd0efb0/touring
VGP=/home/gabrielgadea/.claude/touring/vgp
mkdir -p "$OUT"
L=$OUT/fase_d.rc
: > "$L"
until grep -a -q '^FIM' "$RES/pipeline.rc" 2> /dev/null; do sleep 60; done
NOVO=$(grep -a '^novo=' "$RES/pipeline.rc" | cut -d= -f2)
[ -x "$NOVO" ] || { echo "binário novo ausente ou não executável: '$NOVO' — o pipeline não chegou ao release" >> "$L"; exit 3; }
echo "inicio $(date -Is) ref=$REF novo=$NOVO" >> "$L"
for c in ev bak; do
  corpus=$VGP/corpus_v0d
  [ "$c" = bak ] && corpus=$VGP/corpus_v0d.bak-2026-10-09
  "$HW" --class rust --label "fase-d-v2-$c" --wait 7200 -- bash "$S/pareado_corpus.sh" "$REF" "$NOVO" "$corpus" "$OUT/v2_$c" d
  echo "v2 $c rc=$? $(date -Is)" >> "$L"
done
# V3 (layers 1,2): reported only — its own 1 s ceiling, the JIT/GC of the Node inside pyright makes its instructions
# less deterministic (the A/A measures that; no numeric prediction was registered).
for c in ev bak; do
  corpus=$VGP/corpus_v0d
  [ "$c" = bak ] && corpus=$VGP/corpus_v0d.bak-2026-10-09
  JUIZ_EXTRA='--arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000' \
    "$HW" --class rust --label "fase-d-v3-$c" --wait 7200 -- bash "$S/pareado_corpus.sh" "$REF" "$NOVO" "$corpus" "$OUT/v3_$c" d
  echo "v3 $c rc=$? (relatado, não é portão) $(date -Is)" >> "$L"
done
echo FIM >> "$L"
