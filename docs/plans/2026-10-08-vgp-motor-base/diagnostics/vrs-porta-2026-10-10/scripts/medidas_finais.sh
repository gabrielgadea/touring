#!/usr/bin/env bash
# Medidas FINAIS no release congelado: juiz (natural, v2 sem normalizacao, copia normalizada), ids, controle de 130,
# varredura real e o p95 A/B intercalado contra a a6. Classe heavy.
SC=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
VGP=/home/gabrielgadea/.claude/touring/vgp
: > "$SC/medidas_finais.rcs"
bash "$SC/medida2.sh" final2 /home/gabrielgadea/projects/touring/target/wt-vrs2/release/touring > "$SC/medida2_final2.out" 2>&1
echo "medida2 rc=$?" >> "$SC/medidas_finais.rcs"
python3 -I "$SC/p95_ab.py" "$VGP/corpus_rust_natural" "$VGP/binarios/d5b434d7/touring" "$VGP/binarios/73ffc5fa/touring" > "$SC/p95_ab_final2.txt" 2>&1
echo "p95 A/B rc=$?" >> "$SC/medidas_finais.rcs"
echo "MEDIDAS FINAIS CONCLUIDAS" >> "$SC/medidas_finais.rcs"
