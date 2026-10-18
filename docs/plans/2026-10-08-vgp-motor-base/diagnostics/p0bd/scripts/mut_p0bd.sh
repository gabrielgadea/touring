#!/usr/bin/env bash
# P0B-d: VERDE dos testes novos, depois VERMELHO (conserto revertido) e os mutantes M1..M12.
# Roda DENTRO de uma reserva do hw-slot (rust):
#   hw-slot --class rust --label p0bd --wait 7200 -- bash mut_p0bd.sh <pasta-de-saida> [IDs-de-mutante...]
# Marcador .TROCA-EM-CURSO durante a janela; o fonte volta byte a byte (cmp) e com mtime = AGORA.
set -u
WT=/home/gabrielgadea/projects/touring-wt-p0bd
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0bd
OUT=${1:?pasta de saida}
shift
mkdir -p "$OUT/fonte_bom"
CR=$WT/crates
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-p0bd
cd "$WT" || exit 9
ARQS="touring-code/src/vgp/sombra.rs touring-code/src/vgp/politica.rs touring-code/src/vgp/lang/python.rs touring-code/src/vgp/lang/python/stdlib_model.rs touring-code/src/vgp/lang/python/tests.rs touring-pyright/src/pyright.rs touring-hook-handlers/src/hooks/vgp_l2.rs touring-server/src/cli/vgp.rs"
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
cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --lib vgp_l2 > "$OUT/verde_hooks.log" 2>&1
RC3=$?
echo "verde touring-hook-handlers rc=$RC3" >> "$OUT/rc.txt"
cargo test -p touring-server --lib vgp > "$OUT/verde_server.log" 2>&1
RC4=$?
echo "verde touring-server rc=$RC4" >> "$OUT/rc.txt"
if [ "$RC1" -ne 0 ] || [ "$RC2" -ne 0 ] || [ "$RC3" -ne 0 ] || [ "$RC4" -ne 0 ]; then
  # sem verde nao ha vermelho nem mutante que valha
  echo "ABORTA: o VERDE falhou (rc $RC1 / $RC2 / $RC3 / $RC4); nenhum mutante rodou" >> "$OUT/rc.txt"
  echo FIM >> "$OUT/rc.txt"
  exit 3
fi

printf 'TROCA EM CURSO (subagente p0bd, P0B-d): rodando VERMELHO e MUTANTES em sombra.rs, politica.rs, python.rs, stdlib_model.rs, python/tests.rs, pyright.rs, vgp_l2.rs e cli/vgp.rs (cada um e aplicado, testado e revertido). Inicio %s. O fonte volta byte a byte (cmp); este arquivo some quando terminar.\n' "$(date -Is)" > "$WT/.TROCA-EM-CURSO"
echo "marcador criado $(date -Is)" >> "$OUT/rc.txt"
python3 "$B/scripts/mut_p0bd.py" "$WT" "$OUT/mutantes.json" "$@" > "$OUT/mutantes.log" 2>&1
echo "mutantes rc=$?" >> "$OUT/rc.txt"
RESTAURADO=1
for f in $ARQS; do
  if cmp "$CR/$f" "$OUT/fonte_bom/$f"; then
    echo "restaurado $f (cmp rc=0)" >> "$OUT/rc.txt"
  else
    echo "NAO RESTAURADO $f" >> "$OUT/rc.txt"; RESTAURADO=0
  fi
done
if [ "$RESTAURADO" -eq 1 ]; then unlink "$WT/.TROCA-EM-CURSO"; echo "marcador removido $(date -Is)" >> "$OUT/rc.txt"; fi
echo FIM >> "$OUT/rc.txt"
