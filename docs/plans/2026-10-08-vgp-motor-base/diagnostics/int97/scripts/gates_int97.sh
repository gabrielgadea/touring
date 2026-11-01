#!/usr/bin/env bash
# Gates da INT97, 2º passe (17/10/2026): CLI não-UTF-8 + E2EHERM sobre a árvore integrada.
# Brief §Método 4 + os consumidores do helper do E2EHERM (touring-generator, touring-integration-tests).
# Roda DENTRO de uma reserva do hw-slot:
#   hw-slot --class rust --wait 7200 -- bash gates_int97.sh <pasta-de-saida>
# Os E2E procuram touring/touring-daemon em <worktree>/target/debug: lá ficam symlinks para $CARGO_TARGET_DIR/debug.
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

echo "inicio $(date -Is) HEAD=$(git rev-parse --short HEAD)" >> "$RC"
passo "fmt --all -- --check" fmt.log cargo fmt --all -- --check
passo "clippy --workspace --all-targets -D warnings" clippy.log cargo clippy --workspace --all-targets -- -D warnings
passo "test touring-code" test_code.log cargo test -p touring-code --no-fail-fast
passo "test touring-pyright" test_pyright.log cargo test -p touring-pyright --no-fail-fast
passo "test touring-hook-handlers (pre-hooks,post-hooks)" test_hh.log \
  cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast
passo "test touring-cli" test_cli.log cargo test -p touring-cli --no-fail-fast
passo "test touring-lsp" test_lsp.log cargo test -p touring-lsp --no-fail-fast
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
passo "test touring-server" test_server.log cargo test -p touring-server --no-fail-fast
passo "test touring-hooks" test_hooks.log cargo test -p touring-hooks --no-fail-fast
passo "test touring-generator" test_generator.log cargo test -p touring-generator --no-fail-fast
passo "test touring-integration-tests" test_integ.log cargo test -p touring-integration-tests --no-fail-fast
passo "doc touring-code -D warnings" doc.log env RUSTDOCFLAGS="-D warnings" cargo doc -p touring-code --no-deps
echo "FALHAS=$FALHAS fim $(date -Is)" >> "$RC"
[ "$FALHAS" -eq 0 ] && echo GATES_OK >> "$RC"
echo FIM >> "$RC"
exit "$FALHAS"
