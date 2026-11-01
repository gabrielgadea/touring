#!/usr/bin/env bash
# Binario do P0B: build release a partir do fonte final, congelamento em binarios/<sha8>/ e REPRODUTIBILIDADE
# (touch nas fontes tocadas -> build --release de novo -> cmp byte a byte). Roda DENTRO de uma reserva do hw-slot:
#   hw-slot --class rust --wait 7200 -- bash release_p0b.sh <pasta-de-saida>
set -u
WT=/home/gabrielgadea/projects/touring-wt-p0b
OUT=${1:?pasta de saida}
mkdir -p "$OUT"
cd "$WT" || exit 9
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-p0b
BIN=$CARGO_TARGET_DIR/release/touring
: > "$OUT/release.rc"

cargo build --release -p touring-server --bin touring > "$OUT/build1.log" 2>&1
echo "build 1 rc=$?" >> "$OUT/release.rc"
SHA=$(sha256sum "$BIN" | cut -d' ' -f1)
echo "sha256 $SHA" >> "$OUT/release.rc"
DEST=/home/gabrielgadea/.claude/touring/vgp/binarios/${SHA:0:8}
mkdir -p "$DEST"
cp "$BIN" "$DEST/touring"
chmod 755 "$DEST/touring"
echo "$SHA  touring" > "$DEST/touring.sha256"
cmp "$BIN" "$DEST/touring" && echo "congelado em $DEST (cmp rc=0)" >> "$OUT/release.rc"
cp "$BIN" "$OUT/touring.build1"

# reprodutibilidade: toca as fontes tocadas (mtime = agora) e reconstroi
for f in crates/touring-code/src/vgp/lang/python.rs crates/touring-code/src/vgp/lang/python/modules.rs \
         crates/touring-code/src/vgp/sombra.rs crates/touring-pyright/src/pyright.rs crates/touring-pyright/src/achado.rs; do
  touch "$f"
done
echo "touch das 5 fontes $(date -Is)" >> "$OUT/release.rc"
cargo build --release -p touring-server --bin touring > "$OUT/build2.log" 2>&1
echo "build 2 rc=$?" >> "$OUT/release.rc"
sha256sum "$BIN" >> "$OUT/release.rc"
cmp "$BIN" "$DEST/touring" > "$OUT/cmp2.txt" 2>&1
RC=$?
echo "cmp build 2 x congelado rc=$RC" >> "$OUT/release.rc"
[ "$RC" -eq 0 ] && echo "REPRODUZIVEL: build 2 == congelado" >> "$OUT/release.rc"
unlink "$OUT/touring.build1"
echo FIM >> "$OUT/release.rc"
