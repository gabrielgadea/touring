#!/usr/bin/env bash
# gates1: testes vgp:: + censo + clippy do touring-code e, se tudo verde, o release do touring (juiz) congelado.
# Roda DENTRO de uma reserva do hw-slot (rust). Saidas em $OUT.
set -u
WT=/home/gabrielgadea/projects/touring-wt-vgp-attr
OUT=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
cd "$WT" || exit 9

export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-test
cargo test -p touring-code --lib vgp:: > "$OUT/g1_test.log" 2>&1
T=$?
echo "test rc=$T" > "$OUT/g1.rc"

VGP_CENSUS_FILES="$OUT/censo_lista.txt" cargo test -p touring-code --lib namespace::tests::censo -- --ignored --nocapture > "$OUT/censo.raw" 2>&1
echo "censo rc=$?" >> "$OUT/g1.rc"
grep -a '^CENSO' "$OUT/censo.raw" > "$OUT/censo.tsv"

cargo clippy -p touring-code --all-targets -- -D warnings > "$OUT/g1_clippy.log" 2>&1
C=$?
echo "clippy rc=$C" >> "$OUT/g1.rc"

if [ "$T" -eq 0 ] && [ "$C" -eq 0 ]; then
  export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-judge
  cargo build --release -p touring-server --bin touring > "$OUT/g1_build.log" 2>&1
  B=$?
  echo "build rc=$B" >> "$OUT/g1.rc"
  if [ "$B" -eq 0 ]; then
    cp "$CARGO_TARGET_DIR/release/touring" "$OUT/touring_p0_candidato"
    SHA=$(sha256sum "$OUT/touring_p0_candidato" | cut -d' ' -f1)
    echo "sha256=$SHA" >> "$OUT/g1.rc"
  fi
else
  echo "build PULADO (test=$T clippy=$C)" >> "$OUT/g1.rc"
fi
echo "FIM" >> "$OUT/g1.rc"
