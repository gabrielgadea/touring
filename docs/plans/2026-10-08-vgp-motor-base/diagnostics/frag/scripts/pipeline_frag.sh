#!/usr/bin/env bash
# FRAG, itens 5–6 (17/10/2026): gates → 31 mutantes → release reproduzível → juízes v2/V3 por id (base 8fd0efb0 × FRAG,
# alternados, vaga RUST exclusiva p/ não contaminar o p95) → compara_frag.py (DENY_NOVO/INESPERADO reprovam) → frag.patch
# (frag-base → frag) com a prova de que reconstrói a árvore do FRAG. Para no primeiro elo vermelho.
# Uso: bash pipeline_frag.sh <pasta-de-saida>
set -u
D=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics
S=$D/frag/scripts
OUT=${1:?pasta de saida}
HW=/home/gabrielgadea/projects/touring/scripts/hw-slot
P=/home/gabrielgadea/projects
WT=$P/touring-wt-frag
export CARGO_TARGET_DIR=$P/touring/target/wt-frag
REF=/home/gabrielgadea/.claude/touring/vgp/binarios/8fd0efb0/touring
VGP=/home/gabrielgadea/.claude/touring/vgp
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
mkdir -p "$OUT"
L=$OUT/pipeline.rc
: > "$L"

# ---------------------------------------------------------------- 1. gates
cat > "$OUT/gates.sh" << 'GATES'
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
GATES
"$HW" --class rust --label frag-gates --wait 7200 -- bash "$OUT/gates.sh" "$OUT"
r=$?
echo "gates rc=$r $(date -Is)" >> "$L"
[ "$r" -eq 0 ] || exit 1

# ---------------------------------------------------------------- 2. mutantes
"$HW" --class rust --label frag-mutantes --wait 7200 -- python3 "$S/mutantes_frag.py" "$OUT/mutantes" > "$OUT/mutantes.log" 2>&1
echo "mutantes rc=$? $(date -Is)" >> "$L"
grep -a -E 'SOBREVIV|COMPILA_MAL|FANTASMA|ABORTA' "$OUT/mutantes.log" | sed 's/^/  ATENCAO /' >> "$L"

# ---------------------------------------------------------------- 3. release reproduzível
cat > "$OUT/release.sh" << 'REL'
set -u
O=$1
cd /home/gabrielgadea/projects/touring-wt-frag || exit 9
BIN=$CARGO_TARGET_DIR/release/touring
RC=$O/release.rc
: > "$RC"
cargo build --release -p touring-server --bin touring > "$O/build1.log" 2>&1 || { echo "build 1 FALHOU" >> "$RC"; exit 1; }
SHA=$(sha256sum "$BIN" | cut -d' ' -f1)
DEST=/home/gabrielgadea/.claude/touring/vgp/binarios/${SHA:0:8}
mkdir -p "$DEST"
if [ -e "$DEST/touring" ]; then cmp -s "$BIN" "$DEST/touring" || { echo "CONFLITO em $DEST" >> "$RC"; exit 1; }; else cp "$BIN" "$DEST/touring"; chmod 755 "$DEST/touring"; echo "$SHA  touring" > "$DEST/touring.sha256"; fi
echo "congelado $DEST sha $SHA" >> "$RC"
touch crates/touring-code/src/vgp/politica.rs crates/touring-code/src/vgp/sombra.rs \
      crates/touring-hook-handlers/src/hooks/pre_bash.rs crates/touring-pyright/src/pyright.rs
cargo build --release -p touring-server --bin touring > "$O/build2.log" 2>&1 || { echo "build 2 FALHOU" >> "$RC"; exit 1; }
cmp "$BIN" "$DEST/touring" > /dev/null && echo REPRODUZIVEL >> "$RC"
echo "BIN_NOVO=$DEST/touring" >> "$RC"
grep -q REPRODUZIVEL "$RC"
REL
"$HW" --class rust --label frag-release --wait 7200 -- bash "$OUT/release.sh" "$OUT"
r=$?
echo "release rc=$r $(date -Is)" >> "$L"
[ "$r" -eq 0 ] || exit 2
NOVO=$(grep -a '^BIN_NOVO=' "$OUT/release.rc" | cut -d= -f2)
echo "novo=$NOVO" >> "$L"

# ---------------------------------------------------------------- 4. juízes por id, base × frag alternados
J=$OUT/juizes
mkdir -p "$J"
cat > "$OUT/juizes.sh" << JZ
set -u
cd /home/gabrielgadea/projects/analise || exit 9
export TOURING_CODE_MODE=native
for c in vivo bak; do
  corpus=$VGP/corpus_v0d; [ "\$c" = bak ] && corpus=$VGP/corpus_v0d.bak-2026-10-09
  for modo in v2 v3; do
    for lado in base:$REF frag:$NOVO; do
      n=\${lado%%:*}; bin=\${lado#*:}
      extra=""; [ "\$modo" = v3 ] && extra="--arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --base $J/\${n}_\${c}_v2.json"
      python3 -I $S/juiz_por_id.py --por-id $J/\${n}_\${c}_\${modo}.porid.json --touring "\$bin" --v5-estrito --corpus "\$corpus" --json $J/\${n}_\${c}_\${modo}.json \$extra > $J/\${n}_\${c}_\${modo}.out 2>&1
      echo "\$n \$c \$modo rc=\$? \$(date +%T)" >> $J/juizes.rc
    done
  done
done
JZ
"$HW" --class rust --label frag-juizes --wait 7200 -- bash "$OUT/juizes.sh"
echo "juizes rc=$? $(date -Is)" >> "$L"
python3 -I "$S/compara_frag.py" "$J" > "$OUT/compara.txt" 2>&1
echo "compara_frag rc=$? (0 = sem DENY_NOVO e sem INESPERADO)" >> "$L"

# ---------------------------------------------------------------- 5. frag.patch + prova de reconstrução
TMP=$(mktemp -d)
G=(git --git-dir=$P/touring/.git)
EX=(':(exclude)crates/*/.claude' ':(exclude)crates/touring-hooks/tests/common/private_daemon.rs'
    ':(exclude)crates/touring-hooks/tests/wave25_task_enrichment_e2e.rs')
arv() { GIT_INDEX_FILE=$TMP/i-$2 "${G[@]}" --work-tree="$1" read-tree 88c7cc8b && GIT_INDEX_FILE=$TMP/i-$2 "${G[@]}" --work-tree="$1" add -A crates Cargo.toml Cargo.lock && GIT_INDEX_FILE=$TMP/i-$2 "${G[@]}" --work-tree="$1" write-tree; }
TB=$(arv "$P/touring-wt-frag-base" base)
TF=$(arv "$WT" frag)
"${G[@]}" diff --binary "$TB" "$TF" -- crates Cargo.toml Cargo.lock "${EX[@]}" > "$D/frag/frag.patch"
GIT_INDEX_FILE=$TMP/rec "${G[@]}" read-tree "$TB"
GIT_INDEX_FILE=$TMP/rec "${G[@]}" apply --cached "$D/frag/frag.patch"
TR=$(GIT_INDEX_FILE=$TMP/rec "${G[@]}" write-tree)
if "${G[@]}" diff --quiet "$TR" "$TF" -- crates Cargo.toml Cargo.lock "${EX[@]}"; then echo "frag.patch sha $(sha256sum "$D/frag/frag.patch" | cut -c1-16) RECONSTRÓI o FRAG" >> "$L"; else echo "frag.patch NÃO reconstrói" >> "$L"; fi
for w in int97 adapt; do git -C "$P/touring-wt-$w" apply --check "$D/frag/frag.patch" 2> /dev/null && echo "frag.patch aplica limpo no $w (sozinho)" >> "$L" || echo "frag.patch NÃO aplica sozinho no $w" >> "$L"; done
echo FIM >> "$L"
