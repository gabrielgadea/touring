#!/usr/bin/env bash
# INT97 INTEGRADO (INT97 + P0B + FRAG + P0B-d + roteamento, 17/10/2026): gates derivados dos pacotes tocados →
# release reproduzível → juízes v2/V3 por id nos corpora vivo e bak contra o INT97 final 8fd0efb0 → réguas R1/R2 contra
# as saídas do INT97 (a referência) e do P0B (o que o P0B já mudou). Toda divergência por id contra o INT97 tem de ser
# explicada por P0B, FRAG ou P0B-d — a reconciliação roda depois, sobre estes porid. Os juízes reservam a classe RUST do
# hw-slot (p95 sem contaminação). Para no 1º gate vermelho (REGRA #21: consertar antes de medir).
# Uso: bash pipeline_int97i.sh <pasta-de-saida>
set -u
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics
S=$D/int97-integrado/scripts
OUT=${1:?pasta de saida}
HW=/home/gabrielgadea/projects/touring/scripts/hw-slot
INT=$D/int97/resultados-2026-10-17
P0B=$D/p0b/resultados-2026-10-17/aceite
for f in "$INT"/juizes_{ev,bak}/novo_v{2,3}.porid.json "$P0B"/juizes_{ev,bak}/novo_v{2,3}.porid.json; do
  [ -s "$f" ] || { echo "referência ausente: $f" >&2; exit 3; }
done
REF=/home/gabrielgadea/.claude/touring/vgp/binarios/8fd0efb0/touring
VGP=/home/gabrielgadea/.claude/touring/vgp
mkdir -p "$OUT"
L=$OUT/pipeline.rc
: > "$L"

"$HW" --class rust --label int97i-gates --wait 7200 -- bash "$S/gates_int97i.sh" "$OUT/gates"
echo "gates rc=$? $(date -Is)" >> "$L"
grep -a -q '^GATES_OK' "$OUT/gates/gates.rc" || { echo "GATES VERMELHOS — para aqui" >> "$L"; exit 1; }

"$HW" --class rust --label int97i-release --wait 7200 -- bash "$D/int97/scripts/release_int97.sh" "$OUT/release"
echo "release rc=$? $(date -Is)" >> "$L"
grep -a -q '^REPRODUZIVEL' "$OUT/release/release.rc" || { echo "release NÃO reproduzível — para aqui" >> "$L"; exit 2; }
NOVO=$(grep -a '^BIN_NOVO=' "$OUT/release/release.rc" | cut -d= -f2)
echo "novo=$NOVO" >> "$L"

for c in ev bak; do
  corpus=$VGP/corpus_v0d
  [ "$c" = bak ] && corpus=$VGP/corpus_v0d.bak-2026-10-09
  ROTULOS="novo_v2 novo_v3" "$HW" --class rust --label "int97i-juiz-$c" --wait 7200 -- \
    bash "$D/p0b/scripts/juizes_p0b.sh" "$REF" "$NOVO" "$OUT/juizes_$c" "$corpus"
  echo "juizes $c rc=$? $(date -Is)" >> "$L"
  for v in v2 v3; do
    for ref in INT P0B; do
      base=$INT
      [ "$ref" = P0B ] && base=$P0B
      python3 -I "$D/p0b/scripts/comparar_porid2.py" "$base/juizes_$c/novo_$v.porid.json" "$OUT/juizes_$c/novo_$v.porid.json" \
        "$OUT/juizes_$c/regua_${v}_vs_$ref.json" > "$OUT/juizes_$c/regua_${v}_vs_$ref.txt" 2>&1
      echo "regua $c $v vs $ref rc=$?" >> "$L"
    done
  done
done
# p95 by the PRE-REGISTERED protocol C' (PREREGISTRO-P95-PAREADO-2026-10-17.md): the single-run p95 of the judges
# above is informative only; this is the latency gate (A/A first, then A/B, against the FIXED reference 8fd0efb0).
# PAREADO=0 skips it: on 18/10 the C' A/A failed and the gate moved to phase D (PREREGISTRO-FASE-D-2026-10-18.md), whose
# measuring harness is run on its own once it exists.
[ "${PAREADO:-1}" = 1 ] || { echo "pareado PULADO (PAREADO=0) $(date -Is)" >> "$L"; echo FIM >> "$L"; exit 0; }
for c in ev bak; do
  corpus=$VGP/corpus_v0d
  [ "$c" = bak ] && corpus=$VGP/corpus_v0d.bak-2026-10-09
  "$HW" --class rust --label "int97i-pareado-$c" --wait 7200 -- bash "$S/pareado_corpus.sh" "$REF" "$NOVO" "$corpus" "$OUT/pareado_$c"
  echo "pareado $c rc=$? $(date -Is)" >> "$L"
done
echo FIM >> "$L"
