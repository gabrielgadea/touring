#!/usr/bin/env bash
# INT-ADAPT (brief 2026-10-11-int-adapt.md): patches dos adaptadores contra o worktree da VL0, o do F21K contra o
# F21 que o int97 já tem, e a SIMULAÇÃO da aplicação em sequência sobre o int97 — tudo em índices temporários e
# objetos do git (aditivo): nenhum worktree, índice de worktree ou branch é tocado.
# Uso: bash patches_adapt.sh   (saída em ../patches/ e ../simulacao.txt)
set -u
P=/home/gabrielgadea/projects
GIT_DIR=$P/touring/.git
D=$P/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics
OUT=$D/int-adapt/patches
SIM=$D/int-adapt/simulacao.txt
mkdir -p "$OUT"
TMP=$(mktemp -d)
CAMINHOS=(crates Cargo.toml Cargo.lock ':(exclude)crates/touring-server/.claude' ':(exclude)crates/*/.claude')
F21=(crates/touring-analysis/tests/security_analyzer_test.rs crates/touring-offensive/src/vuln/cwe_patterns.rs
     crates/touring-quality/src/verifications/f2_1_owasp.rs)

g() { git --git-dir="$GIT_DIR" "$@"; }

arvore() { # <diretório> -> árvore de crates/ + Cargo.toml + Cargo.lock como `add -A` a vê
  local idx
  idx=$TMP/idx-$(basename "$1")
  GIT_INDEX_FILE=$idx g --work-tree="$1" read-tree 88c7cc8b || return 1
  GIT_INDEX_FILE=$idx g --work-tree="$1" add -A crates Cargo.toml Cargo.lock || return 1
  GIT_INDEX_FILE=$idx g --work-tree="$1" write-tree
}

: > "$SIM"
T_VL0=$(arvore "$P/touring-wt-vl0") || exit 2
T_INT=$(arvore "$P/touring-wt-int97") || exit 2
echo "arvore vl0=$T_VL0 int97=$T_INT" >> "$SIM"
for c in vtransv vts vhtml vrs2; do
  T=$(arvore "$P/touring-wt-$c") || exit 2
  g diff --binary -M "$T_VL0" "$T" -- "${CAMINHOS[@]}" > "$OUT/$c.patch"
  echo "$c: arvore=$T $(g diff --shortstat -M "$T_VL0" "$T" -- "${CAMINHOS[@]}") sha=$(sha256sum "$OUT/$c.patch" | cut -c1-12)" >> "$SIM"
done
T_F21=$(arvore "$P/touring-wt-f21") || exit 2
g diff --binary "$T_INT" "$T_F21" -- "${F21[@]}" > "$OUT/f21k.patch"
echo "f21k: $(g diff --shortstat "$T_INT" "$T_F21" -- "${F21[@]}") sha=$(sha256sum "$OUT/f21k.patch" | cut -c1-12)" >> "$SIM"

# simulação: int97 + vtransv + vts + vhtml + vrs2 + f21k, em sequência, num índice descartável
IDX=$TMP/idx-sim
GIT_INDEX_FILE=$IDX g read-tree "$T_INT"
for c in vtransv vts vhtml vrs2 f21k; do
  if GIT_INDEX_FILE=$IDX g apply --cached --3way "$OUT/$c.patch" > "$TMP/$c.apply" 2>&1; then
    echo "SIM $c: APLICA" >> "$SIM"
  else
    echo "SIM $c: FALHA" >> "$SIM"
    grep -a -E 'conflict|error|Falha|failed' "$TMP/$c.apply" | head -12 | sed 's/^/    /' >> "$SIM"
  fi
done
echo "arvore simulada=$(GIT_INDEX_FILE=$IDX g write-tree 2>&1)" >> "$SIM"
echo FIM >> "$SIM"
cat "$SIM"
