#!/usr/bin/env bash
# Gates FINAIS do pedido de determinismo (aceite 5), numa unica reserva do hw-slot (rust):
# fmt · clippy -D warnings (4 pacotes) · testes de touring-code, touring-server, touring-hook-handlers,
# touring-hooks · rebuild release + cmp com o binario congelado 4501e1e5.
# touring-server/touring-hooks: os E2E procuram touring e touring-daemon em <workspace>/target/debug;
# o worktree usa CARGO_TARGET_DIR=vgp-test, entao liga-se por symlink (desligado ao final).
set -u
WT=/home/gabrielgadea/projects/touring-wt-vgp-attr
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
cd "$WT" || exit 9
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-test
RC=$P0/dg2.rc
: > "$RC"
sha256sum crates/touring-code/src/vgp/{checks.rs,modules.rs,checks/tests.rs,namespace.rs,namespace/tests.rs,mod.rs} | cut -c1-16,65- >> "$RC"

cargo fmt --all -- --check > "$P0/dg2_fmt.log" 2>&1
echo "fmt --all --check rc=$?" >> "$RC"

cargo clippy -p touring-code -p touring-server -p touring-hooks --all-targets -- -D warnings > "$P0/dg2_clippy.log" 2>&1
echo "clippy -D warnings (touring-code touring-server touring-hooks, all-targets) rc=$?" >> "$RC"

cargo clippy -p touring-hook-handlers --all-targets --features pre-hooks,post-hooks -- -D warnings > "$P0/dg2_clippy_hh.log" 2>&1
echo "clippy -D warnings (touring-hook-handlers, features pre/post) rc=$?" >> "$RC"

cargo test -p touring-code --no-fail-fast > "$P0/dg2_test_code.log" 2>&1
echo "test touring-code rc=$?" >> "$RC"
grep -a "test result" "$P0/dg2_test_code.log" | sort | uniq -c >> "$RC"
grep -a "^test .* FAILED" "$P0/dg2_test_code.log" >> "$RC"

cargo build -p touring-server --bin touring > "$P0/dg2_build_touring.log" 2>&1
echo "build debug touring rc=$?" >> "$RC"
cargo build -p touring-hooks --bin touring-daemon > "$P0/dg2_build_daemon.log" 2>&1
echo "build debug touring-daemon rc=$?" >> "$RC"
mkdir -p target/debug
for b in touring touring-daemon; do [ -e "target/debug/$b" ] || ln -s "$CARGO_TARGET_DIR/debug/$b" "target/debug/$b"; done

cargo test -p touring-server --no-fail-fast > "$P0/dg2_test_server.log" 2>&1
echo "test touring-server rc=$?" >> "$RC"
grep -a "test result" "$P0/dg2_test_server.log" | sort | uniq -c >> "$RC"
grep -a "^test .* FAILED" "$P0/dg2_test_server.log" >> "$RC"

cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast > "$P0/dg2_test_hh.log" 2>&1
echo "test touring-hook-handlers (features) rc=$?" >> "$RC"
grep -a "test result" "$P0/dg2_test_hh.log" | sort | uniq -c >> "$RC"
grep -a "^test .* FAILED" "$P0/dg2_test_hh.log" >> "$RC"

cargo test -p touring-hooks --no-fail-fast > "$P0/dg2_test_hooks.log" 2>&1
echo "test touring-hooks rc=$?" >> "$RC"
grep -a "test result" "$P0/dg2_test_hooks.log" | sort | uniq -c >> "$RC"
grep -a "^test .* FAILED" "$P0/dg2_test_hooks.log" >> "$RC"

unlink target/debug/touring
unlink target/debug/touring-daemon
echo "symlinks removidos" >> "$RC"

# reprodutibilidade: rebuild release do mesmo worktree/target deve reproduzir o binario congelado
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-judge
cargo build --release -p touring-server --bin touring > "$P0/dg2_rebuild.log" 2>&1
echo "rebuild release rc=$?" >> "$RC"
sha256sum "$CARGO_TARGET_DIR/release/touring" /home/gabrielgadea/.claude/touring/vgp/binarios/4501e1e5/touring | cut -c1-16,65- >> "$RC"
cmp "$CARGO_TARGET_DIR/release/touring" /home/gabrielgadea/.claude/touring/vgp/binarios/4501e1e5/touring >> "$RC" 2>&1
echo "cmp rc=$?" >> "$RC"
echo FIM >> "$RC"
