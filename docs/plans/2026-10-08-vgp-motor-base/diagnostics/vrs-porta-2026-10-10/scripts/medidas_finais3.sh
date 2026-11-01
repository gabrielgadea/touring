#!/usr/bin/env bash
# Medidas FINAIS no release congelado final3: juiz (natural, v2 sem normalizacao, copia normalizada), ids, controle de 130,
# varredura real e o p95 A/B intercalado contra a a6.
SC=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
VGP=/home/gabrielgadea/.claude/touring/vgp
REL=/home/gabrielgadea/projects/touring/target/wt-vrs2/release/touring
S8=$(sha256sum "$REL" | cut -c1-8)
: > "$SC/medidas_finais3.rcs"
bash "$SC/medida2.sh" final3 "$REL" > "$SC/medida2_final3.out" 2>&1
echo "medida2 rc=$?" >> "$SC/medidas_finais3.rcs"
python3 -I "$SC/p95_ab.py" "$VGP/corpus_rust_natural" "$VGP/binarios/d5b434d7/touring" "$VGP/binarios/$S8/touring" > "$SC/p95_ab_final3.txt" 2>&1
echo "p95 A/B rc=$?" >> "$SC/medidas_finais3.rcs"
echo "MEDIDAS FINAIS3 CONCLUIDAS ($S8)" >> "$SC/medidas_finais3.rcs"
