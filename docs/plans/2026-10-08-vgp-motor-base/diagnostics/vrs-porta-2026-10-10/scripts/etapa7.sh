#!/usr/bin/env bash
# Etapa 7 (reescrita antes de a vaga abrir): as correcoes (w) `use` autorreferente e (x) ausencia exige
# modulo enumeravel, mais os 3 mutantes que a etapa 6 deixou vivos. Uma UNICA aquisicao do hw-slot:
#  (1) testes do adaptador no fonte atual; se verdes:
#  (2) binarios debug V20 e V21 (V21 = fonte final);
#  (3) gates finais (fmt, clippy -D warnings, suite touring-code, doc, release);
#  (4) todos os mutantes (BASE, RED-stub, 1 por mutante).
# NAO EDITAR rust.rs enquanto o marcador .TROCA-EM-CURSO existir.
SC=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-vrs2
cd /home/gabrielgadea/projects/touring-wt-vrs2 || exit 90
: > "$SC/etapa7.rcs"
cargo test -p touring-code --lib vgp::lang::rust --no-fail-fast > "$SC/etapa7_testes.log" 2>&1
rc=$?
echo "testes rc=$rc" >> "$SC/etapa7.rcs"
if [ "$rc" -ne 0 ]; then
  echo "ETAPA7 CONCLUIDA (testes vermelhos; resto nao rodou)" >> "$SC/etapa7.rcs"
  exit 0
fi
python3 -I "$SC/mutantes.py" variantes /home/gabrielgadea/projects/touring/target/wt-vrs2-var "$SC/variantes7.json" V20 V21 > "$SC/etapa7_variantes.log" 2>&1
echo "variantes rc=$?" >> "$SC/etapa7.rcs"
echo "VARIANTES7 PRONTAS" >> "$SC/etapa7.rcs"
bash "$SC/gates.sh"
echo "gates feitos (ver gates.rcs)" >> "$SC/etapa7.rcs"
python3 -I "$SC/mutantes.py" mutantes "$SC/mutantes_resultado_4.json" > "$SC/mutantes4.log" 2>&1
echo "mutantes rc=$?" >> "$SC/etapa7.rcs"
echo "ETAPA7 CONCLUIDA" >> "$SC/etapa7.rcs"
