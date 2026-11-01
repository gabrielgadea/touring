#!/usr/bin/env bash
# Etapa 9 (reescrita antes de a vaga abrir): o teste do motivo certo do `#[path]` + a correcao (z) (bloco `extern` declara
# itens). UMA aquisicao do hw-slot (rust):
#  (1) testes do adaptador; se verdes:
#  (2) binario debug V23 (= fonte final) e a medida rapida dele;
#  (3) gates completos (fmt, clippy -D warnings, suite touring-code, doc, release) + reprodutibilidade do release;
#  (4) todos os mutantes;
#  (5) medidas finais no release (juiz, v2, controle 130, p95 A/B);
#  (6) por ultimo, o `corpus_rust_real` (so se NAO existir).
SC=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
VGP=/home/gabrielgadea/.claude/touring/vgp
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-vrs2
cd /home/gabrielgadea/projects/touring-wt-vrs2 || exit 90
: > "$SC/etapa9.rcs"
cargo test -p touring-code --lib vgp::lang::rust --no-fail-fast > "$SC/etapa9_testes.log" 2>&1
rc=$?
echo "testes rc=$rc" >> "$SC/etapa9.rcs"
if [ "$rc" -ne 0 ]; then
  echo "ETAPA9 CONCLUIDA (testes vermelhos; resto nao rodou)" >> "$SC/etapa9.rcs"
  exit 0
fi
python3 -I "$SC/mutantes.py" variantes /home/gabrielgadea/projects/touring/target/wt-vrs2-var "$SC/variantes9.json" V23 > "$SC/etapa9_variantes.log" 2>&1
echo "variantes rc=$?" >> "$SC/etapa9.rcs"
bash "$SC/medida_var.sh" V23 /home/gabrielgadea/projects/touring/target/wt-vrs2-var/V23/touring
echo "medida V23 rc=$?" >> "$SC/etapa9.rcs"
bash "$SC/gates.sh"
echo "gates feitos (ver gates.rcs)" >> "$SC/etapa9.rcs"
REL=/home/gabrielgadea/projects/touring/target/wt-vrs2/release/touring
cp "$REL" "$SC/release_primeiro3"
sha256sum "$SC/release_primeiro3" > "$SC/release_primeiro3.sha256"
touch crates/touring-code/src/vgp/lang/rust.rs crates/touring-code/src/vgp/lang/rust/tests.rs
cargo build --release --locked -p touring-server --bin touring > "$SC/etapa9_repro_build.log" 2>&1
echo "repro build rc=$?" >> "$SC/etapa9.rcs"
sha256sum "$REL" > "$SC/release_segundo3.sha256"
cmp "$SC/release_primeiro3" "$REL" > "$SC/etapa9_repro_cmp.log" 2>&1
echo "repro cmp rc=$? (0 = identico)" >> "$SC/etapa9.rcs"
python3 -I "$SC/mutantes.py" mutantes "$SC/mutantes_resultado_6.json" > "$SC/mutantes6.log" 2>&1
echo "mutantes rc=$?" >> "$SC/etapa9.rcs"
bash "$SC/medidas_finais3.sh"
echo "medidas finais3 feitas (ver medidas_finais3.rcs)" >> "$SC/etapa9.rcs"
if [ -e "$VGP/corpus_rust_real" ]; then
  echo "corpus_rust_real JA EXISTE: nao gerado (PARE e avise)" >> "$SC/etapa9.rcs"
else
  python3 -I "$B/scripts/corpus_rust_real.py" --selftest > "$SC/real_selftest.log" 2>&1
  echo "real selftest rc=$?" >> "$SC/etapa9.rcs"
  python3 -I "$B/scripts/corpus_rust_real.py" --n 20 > "$SC/real_n20.log" 2>&1
  echo "real n20 rc=$?" >> "$SC/etapa9.rcs"
fi
echo "ETAPA9 CONCLUIDA" >> "$SC/etapa9.rcs"
