#!/usr/bin/env bash
# Job A do pedido de determinismo: fmt, suite vgp:: limpa (pre-mutantes), build release e congelamento.
# Roda DENTRO de uma reserva do hw-slot (rust) para nao reentrar na fila entre os passos.
set -u
WT=/home/gabrielgadea/projects/touring-wt-vgp-attr
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
cd "$WT" || exit 9
: > "$P0/det_a.rc"

cargo fmt --all -- --check > "$P0/det_a_fmt.log" 2>&1
echo "fmt --all --check rc=$?" >> "$P0/det_a.rc"

CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-test \
  cargo test -p touring-code --lib vgp:: > "$P0/det_a_test.log" 2>&1
T=$?
echo "test touring-code --lib vgp:: rc=$T" >> "$P0/det_a.rc"
grep -a "test result" "$P0/det_a_test.log" >> "$P0/det_a.rc"
if [ "$T" -ne 0 ]; then echo "FIM (testes falharam; build nao rodou)" >> "$P0/det_a.rc"; exit 1; fi

export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-judge
cargo build --release -p touring-server --bin touring > "$P0/det_a_build.log" 2>&1
echo "build --release -p touring-server --bin touring rc=$?" >> "$P0/det_a.rc"

BIN=$CARGO_TARGET_DIR/release/touring
SHA=$(sha256sum "$BIN" | cut -d' ' -f1)
echo "sha256 $SHA" >> "$P0/det_a.rc"
DEST=/home/gabrielgadea/.claude/touring/vgp/binarios/${SHA:0:8}
mkdir -p "$DEST"
cp "$BIN" "$DEST/touring"
chmod 755 "$DEST/touring"
echo "$SHA  touring" > "$DEST/touring.sha"
cmp "$BIN" "$DEST/touring" && echo "congelado em $DEST (cmp rc=0)" >> "$P0/det_a.rc"
echo FIM >> "$P0/det_a.rc"
