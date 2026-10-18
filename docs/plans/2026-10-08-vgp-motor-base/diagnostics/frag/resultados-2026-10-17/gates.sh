set -u
WT=/home/gabrielgadea/projects/touring-wt-frag
O=$1
cd "$WT" || exit 9
RC=$O/gates.rc
: > "$RC"
F=0
passo() { local r=$1 l=$O/$2; shift 2; local t=$(date +%s); "$@" > "$l" 2>&1; local c=$?; echo "$r rc=$c $(($(date +%s) - t))s" >> "$RC"; grep -a '^test .* FAILED$' "$l" >> "$RC"; [ $c -eq 0 ] || F=$((F + 1)); }
passo "fmt" fmt.log cargo fmt --all -- --check
passo "clippy --workspace --all-targets -D warnings" clippy.log cargo clippy --workspace --all-targets -- -D warnings
passo "test touring-code" t_code.log cargo test -p touring-code --no-fail-fast
passo "test touring-pyright" t_pyright.log cargo test -p touring-pyright --no-fail-fast
passo "test touring-hook-handlers (features)" t_hh.log cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast
passo "test touring-hooks-core" t_core.log cargo test -p touring-hooks-core --no-fail-fast
passo "test touring-cli" t_cli.log cargo test -p touring-cli --no-fail-fast
passo "build touring (debug)" b_touring.log cargo build -p touring-server --bin touring
passo "build touring-daemon (debug)" b_daemon.log cargo build -p touring-hooks --bin touring-daemon
mkdir -p target/debug
for b in touring touring-daemon; do
  if [ -e "target/debug/$b" ] && [ ! -L "target/debug/$b" ]; then mv "target/debug/$b" "target/debug/$b.stale-$(date +%s)"; echo "binário velho posto de lado: $b" >> "$RC"; fi
  [ -L "target/debug/$b" ] || ln -s "$CARGO_TARGET_DIR/debug/$b" "target/debug/$b"
done
passo "test touring-server" t_server.log cargo test -p touring-server --no-fail-fast
for b in touring touring-daemon; do [ -L "target/debug/$b" ] && unlink "target/debug/$b"; done
echo "FALHAS=$F" >> "$RC"
[ $F -eq 0 ] && echo GATES_OK >> "$RC"
exit $F
