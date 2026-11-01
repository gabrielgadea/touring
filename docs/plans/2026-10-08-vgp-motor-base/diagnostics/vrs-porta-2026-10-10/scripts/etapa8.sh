#!/usr/bin/env bash
# Etapa 8: a correcao D (escopo por modulo inline). UMA aquisicao do hw-slot (rust):
#  (1) testes do adaptador; se verdes:
#  (2) binario debug V22 (= fonte final) e a medida rapida dele (natural + controle de 130);
#  (3) gates (fmt, clippy -D warnings, suite touring-code, doc, release);
#  (4) todos os mutantes.
# NAO EDITAR rust.rs enquanto o marcador .TROCA-EM-CURSO existir.
SC=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-vrs2
cd /home/gabrielgadea/projects/touring-wt-vrs2 || exit 90
: > "$SC/etapa8.rcs"
cargo test -p touring-code --lib vgp::lang::rust --no-fail-fast > "$SC/etapa8_testes.log" 2>&1
rc=$?
echo "testes rc=$rc" >> "$SC/etapa8.rcs"
if [ "$rc" -ne 0 ]; then
  echo "ETAPA8 CONCLUIDA (testes vermelhos; resto nao rodou)" >> "$SC/etapa8.rcs"
  exit 0
fi
python3 -I "$SC/mutantes.py" variantes /home/gabrielgadea/projects/touring/target/wt-vrs2-var "$SC/variantes8.json" V22 > "$SC/etapa8_variantes.log" 2>&1
echo "variantes rc=$?" >> "$SC/etapa8.rcs"
bash "$SC/medida_var.sh" V22 /home/gabrielgadea/projects/touring/target/wt-vrs2-var/V22/touring
echo "medida V22 rc=$?" >> "$SC/etapa8.rcs"
bash "$SC/gates.sh"
echo "gates feitos (ver gates.rcs)" >> "$SC/etapa8.rcs"
# (3b) reprodutibilidade do binario final: copia o release, `touch` nos .rs tocados, rebuild, cmp
REL=/home/gabrielgadea/projects/touring/target/wt-vrs2/release/touring
cp "$REL" "$SC/release_primeiro"
sha256sum "$SC/release_primeiro" > "$SC/release_primeiro.sha256"
touch crates/touring-code/src/vgp/lang/rust.rs crates/touring-code/src/vgp/lang/rust/tests.rs
cargo build --release --locked -p touring-server --bin touring > "$SC/etapa8_repro_build.log" 2>&1
echo "repro build rc=$?" >> "$SC/etapa8.rcs"
sha256sum "$REL" > "$SC/release_segundo.sha256"
cmp "$SC/release_primeiro" "$REL" > "$SC/etapa8_repro_cmp.log" 2>&1
echo "repro cmp rc=$? (0 = identico)" >> "$SC/etapa8.rcs"
python3 -I "$SC/mutantes.py" mutantes "$SC/mutantes_resultado_5.json" > "$SC/mutantes5.log" 2>&1
echo "mutantes rc=$?" >> "$SC/etapa8.rcs"
echo "ETAPA8 CONCLUIDA" >> "$SC/etapa8.rcs"
