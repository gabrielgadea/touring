set -u
O=$1
cd /home/gabrielgadea/projects/touring-wt-frag || exit 9
BIN=$CARGO_TARGET_DIR/release/touring
RC=$O/release.rc
: > "$RC"
cargo build --release -p touring-server --bin touring > "$O/build1.log" 2>&1 || { echo "build 1 FALHOU" >> "$RC"; exit 1; }
SHA=$(sha256sum "$BIN" | cut -d' ' -f1)
DEST=/home/gabrielgadea/.claude/touring/vgp/binarios/${SHA:0:8}
mkdir -p "$DEST"
if [ -e "$DEST/touring" ]; then cmp -s "$BIN" "$DEST/touring" || { echo "CONFLITO em $DEST" >> "$RC"; exit 1; }; else cp "$BIN" "$DEST/touring"; chmod 755 "$DEST/touring"; echo "$SHA  touring" > "$DEST/touring.sha256"; fi
echo "congelado $DEST sha $SHA" >> "$RC"
touch crates/touring-code/src/vgp/politica.rs crates/touring-code/src/vgp/sombra.rs \
      crates/touring-hook-handlers/src/hooks/pre_bash.rs crates/touring-pyright/src/pyright.rs
cargo build --release -p touring-server --bin touring > "$O/build2.log" 2>&1 || { echo "build 2 FALHOU" >> "$RC"; exit 1; }
cmp "$BIN" "$DEST/touring" > /dev/null && echo REPRODUZIVEL >> "$RC"
echo "BIN_NOVO=$DEST/touring" >> "$RC"
grep -q REPRODUZIVEL "$RC"
