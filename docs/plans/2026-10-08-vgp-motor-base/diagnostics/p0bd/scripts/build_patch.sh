#!/usr/bin/env bash
# P0B-d: builds p0bd.patch (base d1e8c250 -> worktree) with a TEMPORARY index (the worktree's own index
# and HEAD are never touched) and PROVES it rebuilds the worktree two ways:
#   (1) tree hash: base index + `git apply --cached p0bd.patch` == tree of the worktree content;
#   (2) bytes: the touched files, re-created from the base blobs in a scratch dir, patched with
#       `git apply`, are `cmp`-identical to the worktree files (and the deleted ones are gone).
set -u
WT=/home/gabrielgadea/projects/touring-wt-p0bd
OUT=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/p0bd
BASE=d1e8c250
SCR=${SCRATCH:-/tmp/claude-1000/-home-gabrielgadea-projects-touring/efe418c2-95f7-4a62-8bac-57080ed455b8/scratchpad}
IDX=$(mktemp -u "$SCR/idx-p0bd.XXXXXX")
IDX2=$(mktemp -u "$SCR/idx2-p0bd.XXXXXX")
export GIT_OPTIONAL_LOCKS=0

GIT_INDEX_FILE=$IDX git -C "$WT" read-tree "$BASE" || exit 2
GIT_INDEX_FILE=$IDX git -C "$WT" add -A || exit 3
GIT_INDEX_FILE=$IDX git -C "$WT" diff --cached --binary "$BASE" > "$OUT/p0bd.patch" || exit 4
TREE_WT=$(GIT_INDEX_FILE=$IDX git -C "$WT" write-tree) || exit 5
SHA=$(sha256sum "$OUT/p0bd.patch" | cut -d' ' -f1)
echo "$SHA  p0bd.patch" > "$OUT/p0bd.patch.sha256"
echo "patch sha256: $SHA ($(wc -c < "$OUT/p0bd.patch") bytes)"
GIT_INDEX_FILE=$IDX git -C "$WT" diff --cached --name-status "$BASE" > "$OUT/p0bd.arquivos.txt"
echo "arquivos: $(wc -l < "$OUT/p0bd.arquivos.txt")"

# (1) tree hash
GIT_INDEX_FILE=$IDX2 git -C "$WT" read-tree "$BASE" || exit 6
GIT_INDEX_FILE=$IDX2 git -C "$WT" apply --cached "$OUT/p0bd.patch" || exit 7
TREE_AP=$(GIT_INDEX_FILE=$IDX2 git -C "$WT" write-tree) || exit 8
if [ "$TREE_WT" = "$TREE_AP" ]; then echo "RECONSTROI(tree hash): $TREE_AP == $TREE_WT"; R1=ok; else echo "DIVERGE(tree hash): $TREE_AP != $TREE_WT"; R1=falhou; fi

# (2) bytes, touched files only
D=$(mktemp -d "$SCR/rebuild-p0bd.XXXXXX")
R2=ok
while IFS=$'\t' read -r st f rest; do
  case "$st" in
    A) ;;
    D|M) mkdir -p "$D/$(dirname "$f")"; git -C "$WT" show "$BASE:$f" > "$D/$f" || R2=falhou ;;
    *) echo "status inesperado: $st $f"; R2=falhou ;;
  esac
done < "$OUT/p0bd.arquivos.txt"
( cd "$D" && git apply --binary "$OUT/p0bd.patch" ) || R2=falhou
while IFS=$'\t' read -r st f rest; do
  case "$st" in
    A|M) cmp -s "$D/$f" "$WT/$f" || { echo "DIFERE: $f"; R2=falhou; } ;;
    D)   [ ! -e "$D/$f" ] || { echo "DEVIA SUMIR: $f"; R2=falhou; } ;;
  esac
done < "$OUT/p0bd.arquivos.txt"
echo "RECONSTROI(bytes dos tocados): $R2 (dir $D)"
echo "reconstroi: tree=$R1 bytes=$R2" > "$OUT/p0bd.reconstroi.txt"
echo "tree_wt=$TREE_WT" >> "$OUT/p0bd.reconstroi.txt"
unlink "$IDX" 2>/dev/null; unlink "$IDX2" 2>/dev/null
[ "$R1" = ok ] && [ "$R2" = ok ]
