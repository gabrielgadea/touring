#!/usr/bin/env bash
# P0B: VERDE dos testes novos, depois VERMELHO (decisoes revertidas) e os mutantes por regra.
# Roda DENTRO de uma reserva do hw-slot (rust):
#   hw-slot --class rust --wait 7200 -- bash mut_p0b.sh <pasta-de-saida> [IDs-de-mutante...]
# Marcador .TROCA-EM-CURSO durante a janela; o fonte volta byte a byte (cmp) e com mtime = AGORA.
set -u
WT=/home/gabrielgadea/projects/touring-wt-p0b
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0b
OUT=${1:?pasta de saida}
shift
mkdir -p "$OUT/fonte_bom"
CR=$WT/crates
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-p0b
cd "$WT" || exit 9
ARQS="touring-code/src/vgp/lang/python.rs touring-code/src/vgp/lang/python/modules.rs touring-code/src/vgp/sombra.rs touring-pyright/src/pyright.rs touring-pyright/src/achado.rs"
for f in $ARQS; do mkdir -p "$OUT/fonte_bom/$(dirname "$f")"; cp "$CR/$f" "$OUT/fonte_bom/$f"; done
: > "$OUT/rc.txt"
(cd "$CR" && sha256sum $ARQS | cut -c1-16,65-) >> "$OUT/rc.txt"

echo "== VERDE $(date -Is)" >> "$OUT/rc.txt"
cargo test -p touring-code --lib vgp:: > "$OUT/verde_code.log" 2>&1
RC1=$?
echo "verde touring-code rc=$RC1" >> "$OUT/rc.txt"
cargo test -p touring-pyright --lib > "$OUT/verde_pyright.log" 2>&1
RC2=$?
echo "verde touring-pyright rc=$RC2" >> "$OUT/rc.txt"
if [ "$RC1" -ne 0 ] || [ "$RC2" -ne 0 ]; then
  # sem verde nao ha vermelho nem mutante que valha: nao gasta a vaga com 33 compilacoes de um fonte quebrado
  echo "ABORTA: o VERDE falhou (rc $RC1 / $RC2); nenhum mutante rodou" >> "$OUT/rc.txt"
  echo FIM >> "$OUT/rc.txt"
  exit 3
fi

printf 'TROCA EM CURSO (subagente p0-atributo, P0B): rodando VERMELHO e MUTANTES em python.rs, modules.rs, sombra.rs, pyright.rs e achado.rs (cada um e aplicado, testado e revertido). Inicio %s. O fonte volta byte a byte (cmp); este arquivo some quando terminar.\n' "$(date -Is)" > "$WT/.TROCA-EM-CURSO"
echo "marcador criado $(date -Is)" >> "$OUT/rc.txt"
python3 "$B/scripts/mut_p0b.py" "$WT" "$OUT/mutantes.json" "$@" > "$OUT/mutantes.log" 2>&1
echo "mutantes rc=$?" >> "$OUT/rc.txt"
for f in $ARQS; do
  cmp "$CR/$f" "$OUT/fonte_bom/$f" && echo "restaurado $f (cmp rc=0)" >> "$OUT/rc.txt"
done
unlink "$WT/.TROCA-EM-CURSO"
echo "marcador removido $(date -Is)" >> "$OUT/rc.txt"
echo FIM >> "$OUT/rc.txt"
