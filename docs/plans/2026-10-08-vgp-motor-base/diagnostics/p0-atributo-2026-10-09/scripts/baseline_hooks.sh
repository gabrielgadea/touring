#!/usr/bin/env bash
# As 3 falhas de touring-hooks (cli_suggester_e2e x1, wave25_task_enrichment_e2e x2) reproduzem SEM a minha mudanca?
# Troca temporariamente os 4 arquivos tocados pelos de 88c7cc8b (git show, leitura) e poe namespace.rs/namespace/ de lado
# (mv); roda os dois alvos de teste; restaura por cp/mv e confere os sha256. Nada de git checkout/restore/stash.
set -u
WT=/home/gabrielgadea/projects/touring-wt-vgp-attr
P0=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
V=crates/touring-code/src/vgp
SAVE=$P0/fonte_final
cd "$WT" || exit 9
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-test
mkdir -p "$SAVE/checks" "$SAVE/namespace"
: > "$P0/bh.rc"
for f in modules.rs checks.rs mod.rs checks/tests.rs namespace.rs namespace/tests.rs; do cp "$V/$f" "$SAVE/$f"; done
( cd "$SAVE" && sha256sum modules.rs checks.rs mod.rs checks/tests.rs namespace.rs namespace/tests.rs ) > "$P0/bh_sha_antes.txt"
echo "salvo" >> "$P0/bh.rc"
for f in modules.rs checks.rs mod.rs checks/tests.rs; do git show "88c7cc8b:$V/$f" > "$V/$f"; done
mv "$V/namespace.rs" "$SAVE/namespace.rs.movido"
mv "$V/namespace" "$SAVE/namespace.dir.movido"
echo "baseline aplicado" >> "$P0/bh.rc"
mkdir -p target/debug
for b in touring touring-daemon; do [ -e "target/debug/$b" ] || ln -s "$CARGO_TARGET_DIR/debug/$b" "target/debug/$b"; done
cargo test -p touring-hooks --test cli_suggester_e2e --test wave25_task_enrichment_e2e --no-fail-fast > "$P0/bh_test.log" 2>&1
echo "test (baseline 88c7cc8b) rc=$?" >> "$P0/bh.rc"
grep -a "test result\|^test .* FAILED" "$P0/bh_test.log" >> "$P0/bh.rc"
unlink target/debug/touring
unlink target/debug/touring-daemon
# restaura
mv "$SAVE/namespace.rs.movido" "$V/namespace.rs"
mv "$SAVE/namespace.dir.movido" "$V/namespace"
for f in modules.rs checks.rs mod.rs checks/tests.rs; do cp "$SAVE/$f" "$V/$f"; done
( cd "$V" && sha256sum modules.rs checks.rs mod.rs checks/tests.rs namespace.rs namespace/tests.rs ) > "$P0/bh_sha_depois.txt"
cmp "$P0/bh_sha_antes.txt" "$P0/bh_sha_depois.txt" >> "$P0/bh.rc" 2>&1
echo "restauracao cmp rc=$?" >> "$P0/bh.rc"
echo FIM >> "$P0/bh.rc"
