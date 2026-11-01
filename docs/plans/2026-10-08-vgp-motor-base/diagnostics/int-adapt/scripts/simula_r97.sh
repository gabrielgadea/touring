#!/usr/bin/env bash
# Simula, em índices temporários (aditivo: só objetos do git), P0B e FRAG entrando em sequência no int97 (R97) e no
# adapt (INT-ADAPT). P0B: diff int97 -> p0b só nos arquivos do P0B (fora E2EHERM, CLI e bancos de runtime).
# FRAG: diff frag-base -> frag. Uso: bash simula_r97.sh  (saída em ../simulacao_r97.txt e ../patches/)
set -u
P=/home/gabrielgadea/projects
GIT_DIR=$P/touring/.git
D=$P/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/int-adapt
OUT=$D/patches
SIM=$D/simulacao_r97.txt
TMP=$(mktemp -d)
EXCL=(':(exclude)crates/touring-server/.claude' ':(exclude)crates/*/.claude' ':(exclude).claude'
      ':(exclude)crates/touring-hooks/tests/common/private_daemon.rs'
      ':(exclude)crates/touring-hooks/tests/wave25_task_enrichment_e2e.rs'
      ':(exclude)crates/touring-server/src/cli/vgp.rs')

g() { git --git-dir="$GIT_DIR" "$@"; }

arvore() {
  local idx
  idx=$TMP/idx-$(basename "$1")
  GIT_INDEX_FILE=$idx g --work-tree="$1" read-tree 88c7cc8b || return 1
  GIT_INDEX_FILE=$idx g --work-tree="$1" add -A crates Cargo.toml Cargo.lock || return 1
  GIT_INDEX_FILE=$idx g --work-tree="$1" write-tree
}

: > "$SIM"
T_INT=$(arvore "$P/touring-wt-int97") || exit 2
T_ADA=$(arvore "$P/touring-wt-adapt") || exit 2
T_P0B=$(arvore "$P/touring-wt-p0b") || exit 2
T_FB=$(arvore "$P/touring-wt-frag-base") || exit 2
T_FRAG=$(arvore "$P/touring-wt-frag") || exit 2
g diff --binary -M "$T_INT" "$T_P0B" -- crates Cargo.toml Cargo.lock "${EXCL[@]}" > "$OUT/p0b-simulado.patch"
g diff --binary -M "$T_FB" "$T_FRAG" -- crates Cargo.toml Cargo.lock "${EXCL[@]}" > "$OUT/frag-simulado.patch"
echo "p0b: $(g diff --shortstat "$T_INT" "$T_P0B" -- crates "${EXCL[@]}")" >> "$SIM"
g diff --name-only "$T_INT" "$T_P0B" -- crates "${EXCL[@]}" | sed 's/^/    /' >> "$SIM"
echo "frag: $(g diff --shortstat "$T_FB" "$T_FRAG" -- crates "${EXCL[@]}")" >> "$SIM"
g diff --name-only "$T_FB" "$T_FRAG" -- crates "${EXCL[@]}" | sed 's/^/    /' >> "$SIM"

for alvo in int97:$T_INT adapt:$T_ADA; do
  nome=${alvo%%:*}
  IDX=$TMP/sim-$nome
  GIT_INDEX_FILE=$IDX g read-tree "${alvo#*:}"
  for c in p0b frag; do
    if GIT_INDEX_FILE=$IDX g apply --cached --3way "$OUT/$c-simulado.patch" > "$TMP/$nome-$c" 2>&1; then
      echo "SIM $nome + $c: APLICA" >> "$SIM"
    else
      echo "SIM $nome + $c: FALHA" >> "$SIM"
      grep -a -v '^Applied\|^Checking\|^Applying' "$TMP/$nome-$c" | head -8 | sed 's/^/    /' >> "$SIM"
    fi
  done
  echo "arvore $nome simulada=$(GIT_INDEX_FILE=$IDX g write-tree 2>&1 | head -1)" >> "$SIM"
done
echo FIM >> "$SIM"
cat "$SIM"
