#!/usr/bin/env bash
# Gates do INT97 INTEGRADO (INT97 + P0B + FRAG + roteamento-crates, 17/10/2026). Diferença para gates_int97.sh: a
# lista de pacotes testados é DERIVADA dos arquivos tocados (git status contra o HEAD, cada caminho no Cargo.toml
# mais próximo) — o FRAG mexe em touring-hooks-core e o roteamento em touring-hooks-shared, que o gate do INT97 não
# testava. Sinal ausente nunca estreita: sem lista → todos os pacotes do workspace (regra do gate 1/6 do
# propagate-release). Roda DENTRO de uma reserva do hw-slot:
#   hw-slot --class rust --wait 7200 -- bash gates_int97i.sh <pasta-de-saida>
set -u
WT=/home/gabrielgadea/projects/touring-wt-int97
OUT=${1:?pasta de saida}
mkdir -p "$OUT"
cd "$WT" || exit 9
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-int97
RC=$OUT/gates.rc
: > "$RC"
FALHAS=0

passo() { # <rotulo> <log> <comando...>
  local rot=$1 log=$OUT/$2
  shift 2
  local t0 rc
  t0=$(date +%s)
  "$@" > "$log" 2>&1
  rc=$?
  echo "$rot rc=$rc $(($(date +%s) - t0))s" >> "$RC"
  grep -a "^test result" "$log" >> "$RC"
  grep -a "^test .* FAILED$" "$log" >> "$RC"
  [ "$rc" -eq 0 ] || FALHAS=$((FALHAS + 1))
}

PACOTES=$(git status --porcelain --untracked-files=all | cut -c4- | python3 -I -c '
import re, sys
from pathlib import Path
pk = set()
for line in sys.stdin:
    p = Path(line.strip().split(" -> ")[-1])
    for d in [p.parent, *p.parent.parents]:
        m = d / "Cargo.toml"
        if str(d) not in ("", ".") and m.exists():
            s = re.search(r"(?ms)^\[package\].*?^name\s*=\s*\"([^\"]+)\"", m.read_text(encoding="utf-8"))
            if s:
                pk.add(s.group(1))
            break
print(" ".join(sorted(pk)))')
if [ -z "$PACOTES" ]; then
  PACOTES=$(cargo metadata --no-deps --format-version 1 | python3 -I -c 'import json,sys; print(" ".join(sorted(p["name"] for p in json.load(sys.stdin)["packages"])))')
  echo "AVISO: lista derivada vazia — testando TODOS os pacotes" >> "$RC"
fi
# The INT97 gate's own list stays: generator and integration-tests are CONSUMERS of the E2EHERM helper, not touched
# files, so the derivation alone would narrow the gate below what INT97 already proved.
INT97_SEMPRE="touring-code touring-pyright touring-hook-handlers touring-cli touring-lsp touring-server touring-hooks touring-generator touring-integration-tests"
PACOTES=$(printf '%s\n' $PACOTES $INT97_SEMPRE | sort -u | tr '\n' ' ')
echo "inicio $(date -Is) HEAD=$(git rev-parse --short HEAD) pacotes: $PACOTES" >> "$RC"

passo "fmt --all -- --check" fmt.log cargo fmt --all -- --check
passo "clippy --workspace --all-targets -D warnings" clippy.log cargo clippy --workspace --all-targets -- -D warnings
passo "build touring (debug)" build_touring.log cargo build -p touring-server --bin touring
passo "build touring-daemon (debug)" build_daemon.log cargo build -p touring-hooks --bin touring-daemon
for b in touring touring-daemon; do
  if [ -e "target/debug/$b" ]; then
    echo "symlink target/debug/$b -> $(readlink target/debug/$b)" >> "$RC"
  else
    echo "AVISO: target/debug/$b não resolve" >> "$RC"
    FALHAS=$((FALHAS + 1))
  fi
done
for p in $PACOTES; do
  feats=()
  [ "$p" = touring-hook-handlers ] && feats=(--features pre-hooks,post-hooks)
  passo "test $p ${feats[*]}" "test_$p.log" cargo test -p "$p" "${feats[@]}" --no-fail-fast
done
passo "doc touring-code -D warnings" doc.log env RUSTDOCFLAGS="-D warnings" cargo doc -p touring-code --no-deps
echo "FALHAS=$FALHAS fim $(date -Is)" >> "$RC"
[ "$FALHAS" -eq 0 ] && echo GATES_OK >> "$RC"
echo FIM >> "$RC"
exit "$FALHAS"
