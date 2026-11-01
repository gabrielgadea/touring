#!/usr/bin/env bash
# Uso: medida2.sh <rotulo> <caminho do binario release>
# Congela o binario em ~/.claude/touring/vgp/binarios/<sha8>/touring (+ .sha256) e roda, SO LEITURA nos corpora:
#  - juiz (--exigir-limite-superior) no natural e na sanidade (copia normalizada) e na sanidade_v2 (quando carregavel);
#  - por-id nos tres; o controle de 130 .rs; a varredura real. Rodar sob `hw-slot --class heavy`.
SC=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
VGP=/home/gabrielgadea/.claude/touring/vgp
rot=$1
src=$2
[ -x "$src" ] || { echo "binario ausente: $src"; exit 90; }
sha=$(sha256sum "$src" | cut -d' ' -f1)
s8=${sha:0:8}
dest=$VGP/binarios/$s8
mkdir -p "$dest"
[ -f "$dest/touring" ] || cp "$src" "$dest/touring"
echo "$sha  touring" > "$dest/touring.sha256"
( cd "$dest" && sha256sum -c touring.sha256 ) || exit 91
echo "$rot $s8 $sha $dest/touring" > "$SC/binario_$rot.txt"
echo "juiz sha16: $(sha256sum $B/scripts/juiz_motor_vgp.py | cut -c1-16)"
: > "$SC/juiz_$rot.rcs"
for c in natural sanidade sanidade_v2; do
  case $c in
    natural) DIRC=$VGP/corpus_rust_natural ;;
    sanidade) DIRC=$SC/corpus_rust_norm ;;
    sanidade_v2) DIRC=$VGP/corpus_rust_sanidade_v2 ;;
  esac
  python3 -I "$B/scripts/juiz_motor_vgp.py" --touring "$dest/touring" --corpus "$DIRC" \
    --exigir-limite-superior --json "$SC/juiz_${rot}_$c.json" > "$SC/juiz_${rot}_$c.log" 2>&1
  echo "$c: juiz rc=$?" >> "$SC/juiz_$rot.rcs"
  python3 -I "$SC/medir_ids.py" --touring "$dest/touring" --corpus "$DIRC" \
    --out "$SC/ids_${rot}_$c.json" >> "$SC/ids_$rot.log" 2>&1
done
python3 -I "$SC/controle130.py" --manifesto "$SC/manifesto_130.json" --touring "$dest/touring" \
  --out "$SC/ctl_${rot}.json" > "$SC/ctl_${rot}.txt" 2>&1
echo "controle130 rc=$?" >> "$SC/juiz_$rot.rcs"
python3 -I "$SC/varredura_real.py" --touring "$dest/touring" --out "$SC/varredura_${rot}.json" > "$SC/varredura_${rot}.txt" 2>&1
echo "MEDIDA2 $rot CONCLUIDA" >> "$SC/juiz_$rot.rcs"
