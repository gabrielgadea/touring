#!/usr/bin/env bash
# Uso: medida.sh <rotulo: antes|depois> <caminho do binario release>
# Congela o binario em ~/.claude/touring/vgp/binarios/<sha8>/touring (+ .sha256), roda o juiz
# (so leitura, --exigir-limite-superior) nos dois corpora Rust e grava o detalhe POR ID.
# Nao usa cargo; rodar FORA do hw-slot nao e permitido para jobs pesados -> quem chama a envolve
# num `hw-slot -- ` da classe heavy (o juiz spawna ~600 processos).
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
if [ ! -f "$dest/touring" ]; then
  cp "$src" "$dest/touring"
fi
echo "$sha  touring" > "$dest/touring.sha256"
( cd "$dest" && sha256sum -c touring.sha256 ) || exit 91
echo "$rot $s8 $sha $dest/touring" > "$SC/binario_$rot.txt"
echo "juiz sha16: $(sha256sum $B/scripts/juiz_motor_vgp.py | cut -c1-16)"
rc_total=0
for c in sanidade natural; do
  # `corpus_rust/` tem `contexto` como string e o juiz (6c8a31b0) quebra nele: o sanidade e medido
  # numa COPIA normalizada (contexto=[]; snap/base/estado_no_instante por symlink, nada alterado)
  if [ "$c" = sanidade ]; then DIRC=$SC/corpus_rust_norm; else DIRC=$VGP/corpus_rust_natural; fi
  python3 -I "$B/scripts/juiz_motor_vgp.py" --touring "$dest/touring" --corpus "$DIRC" \
    --exigir-limite-superior --json "$SC/juiz_${rot}_$c.json" > "$SC/juiz_${rot}_$c.log" 2>&1
  rc=$?
  echo "$c: juiz rc=$rc" | tee -a "$SC/juiz_${rot}.rcs"
  python3 -I "$SC/medir_ids.py" --touring "$dest/touring" --corpus "$DIRC" \
    --out "$SC/ids_${rot}_$c.json" >> "$SC/ids_${rot}.log" 2>&1
done
python3 -I "$SC/varredura_real.py" --touring "$dest/touring" --out "$SC/varredura_${rot}.json" > "$SC/varredura_${rot}.txt" 2>&1
echo "medida $rot concluida"
