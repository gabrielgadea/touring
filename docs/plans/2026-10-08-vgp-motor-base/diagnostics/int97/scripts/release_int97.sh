#!/usr/bin/env bash
# Binário da INT97, 2º passe: build release, congelamento em binarios/<sha8>/ e REPRODUTIBILIDADE
# (touch nas fontes do VGP -> build --release de novo -> cmp byte a byte). Forma de release_p0b.sh.
# Roda DENTRO de uma reserva do hw-slot:
#   hw-slot --class rust --wait 7200 -- bash release_int97.sh <pasta-de-saida>
set -u
WT=/home/gabrielgadea/projects/touring-wt-int97
OUT=${1:?pasta de saida}
mkdir -p "$OUT"
cd "$WT" || exit 9
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-int97
BIN=$CARGO_TARGET_DIR/release/touring
RC=$OUT/release.rc
: > "$RC"

cargo build --release -p touring-server --bin touring > "$OUT/build1.log" 2>&1
r=$?
echo "build 1 rc=$r $(date -Is)" >> "$RC"
[ "$r" -eq 0 ] || exit 1
SHA=$(sha256sum "$BIN" | cut -d' ' -f1)
echo "sha256 $SHA" >> "$RC"
DEST=/home/gabrielgadea/.claude/touring/vgp/binarios/${SHA:0:8}
if [ -e "$DEST/touring" ]; then
  if cmp "$BIN" "$DEST/touring"; then
    echo "já congelado e idêntico em $DEST" >> "$RC"
  else
    echo "CONFLITO: $DEST/touring existe e difere" >> "$RC"
    exit 1
  fi
else
  mkdir -p "$DEST"
  cp "$BIN" "$DEST/touring"
  chmod 755 "$DEST/touring"
  echo "$SHA  touring" > "$DEST/touring.sha256"
  cmp "$BIN" "$DEST/touring" && echo "congelado em $DEST (cmp rc=0)" >> "$RC"
fi

for f in crates/touring-server/src/cli/vgp.rs crates/touring-code/src/vgp/lang/python.rs \
         crates/touring-code/src/vgp/sombra.rs crates/touring-pyright/src/pyright.rs \
         crates/touring-hook-handlers/src/hooks/vgp_l2.rs; do
  touch "$f"
done
echo "touch das 5 fontes $(date -Is)" >> "$RC"
cargo build --release -p touring-server --bin touring > "$OUT/build2.log" 2>&1
echo "build 2 rc=$? $(date -Is)" >> "$RC"
sha256sum "$BIN" | cut -c1-16 >> "$RC"
cmp "$BIN" "$DEST/touring" > "$OUT/cmp2.txt" 2>&1
r=$?
echo "cmp build 2 x congelado rc=$r" >> "$RC"
[ "$r" -eq 0 ] && echo REPRODUZIVEL >> "$RC"
echo "BIN_NOVO=$DEST/touring" >> "$RC"
echo FIM >> "$RC"
[ "$r" -eq 0 ]
