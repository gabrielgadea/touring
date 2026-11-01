#!/usr/bin/env bash
cd "$(dirname "$0")" || exit 9
BIN=/home/gabrielgadea/.claude/touring/vgp/binarios/f55ec4b5/touring
ROTULOS="base_v2 novo_v2 novo_v3" ./juizes.sh $BIN $PWD/juizes_f55
ROTULOS="base_v2 novo_v2 novo_v3" ./juizes.sh $BIN $PWD/juizes_f55_bak /home/gabrielgadea/.claude/touring/vgp/corpus_v0d.bak-2026-10-09
