#!/usr/bin/env bash
_HOME_SLUG=$(printf '%s' "$HOME" | tr -c 'A-Za-z0-9' '-')  # Claude Code project-dir slug of $HOME
# Prova de que o binario congelado (f55ec4b5) vem do fonte final: rebuild release do mesmo worktree/target
# e comparacao de sha256. Se divergir, mede o que diverge (strings com data/hora) antes de concluir.
set -u
WT=${HOME}/projects/touring-wt-vgp-attr
P0=/tmp/claude-1000/${_HOME_SLUG}-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/p0
cd "$WT" || exit 9
export CARGO_TARGET_DIR=${HOME}/projects/touring/target/vgp-judge
cargo build --release -p touring-server --bin touring > "$P0/rv_build.log" 2>&1
echo "build rc=$?" > "$P0/rv.rc"
sha256sum "$CARGO_TARGET_DIR/release/touring" ${HOME}/.claude/touring/vgp/binarios/f55ec4b5/touring >> "$P0/rv.rc"
cmp "$CARGO_TARGET_DIR/release/touring" ${HOME}/.claude/touring/vgp/binarios/f55ec4b5/touring >> "$P0/rv.rc" 2>&1
echo "cmp rc=$?" >> "$P0/rv.rc"
echo FIM >> "$P0/rv.rc"
