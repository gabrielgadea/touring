#!/usr/bin/env bash
# juizes_p0b.sh <binario-referencia> <binario-novo> <saida-dir> [corpus]
# Juiz v2 (camada 1, --v5-estrito) e V3 (camadas 1,2, --p95-ms 1000) com o binario de REFERENCIA e com o NOVO,
# gravando o veredito do motor POR ID de cada um (rodar_juiz_instrumentado.py). Forma de rodar_p2.sh / juizes.sh do P0.
# Sequencial de proposito: o p95 do juiz e medido, rodar dois em paralelo o contamina.
# ROTULOS controla o que roda (default: base_v2 novo_v2 base_v3 novo_v3). O corpus default e o vivo.
set -u
BIN_BASE=$1
BIN_NOVO=$2
OUT=$3
CORPUS=${4:-/home/gabrielgadea/.claude/touring/vgp/corpus_v0d}
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
JUIZ=$B/scripts/juiz_motor_vgp.py
INST=$B/diagnostics/instrumentos-2026-10-09/rodar_juiz_instrumentado.py
mkdir -p "$OUT"
cd /home/gabrielgadea/projects/analise || exit 9
export TOURING_CODE_MODE=native

rodar() { # <rotulo> <binario> [args extras do juiz]
  local rot=$1 bin=$2
  shift 2
  python3 -I "$INST" "$JUIZ" "$OUT/$rot.porid.json" --touring "$bin" --v5-estrito --corpus "$CORPUS" --json "$OUT/$rot.json" "$@" > "$OUT/$rot.out" 2>&1
  echo "$rot rc=$? $(date -Is)" >> "$OUT/juizes.rc"
}

: > "$OUT/juizes.rc"
sha256sum "$BIN_BASE" "$BIN_NOVO" | cut -c1-16,65- >> "$OUT/juizes.rc"
sha256sum "$CORPUS/entradas.jsonl" "$CORPUS/entradas_inline_neg.jsonl" "$JUIZ" | cut -c1-16,65- >> "$OUT/juizes.rc"

ROTULOS=${ROTULOS:-"base_v2 novo_v2 base_v3 novo_v3"}
for r in $ROTULOS; do
  case $r in
    base_v2) rodar base_v2 "$BIN_BASE" ;;
    novo_v2) rodar novo_v2 "$BIN_NOVO" ;;
    base_v3) rodar base_v3 "$BIN_BASE" --arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --base "$OUT/base_v2.json" ;;
    novo_v3) rodar novo_v3 "$BIN_NOVO" --arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --base "$OUT/novo_v2.json" ;;
  esac
done
echo FIM >> "$OUT/juizes.rc"
