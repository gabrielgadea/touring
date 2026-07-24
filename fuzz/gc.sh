#!/usr/bin/env bash
# fuzz/gc.sh — garbage-collect cargo-fuzz corpora + crash artifacts (REGRA #12 disk hygiene).
#
# Master Plan C.W3. cargo-fuzz corpora grow unbounded (every new-coverage input is
# kept) and `artifacts/` accumulates crash/timeout reproducers. This script reports
# sizes and, with --apply, prunes each corpus to the N most-recent inputs and removes
# crash artifacts older than a retention window. Dry-run by default — never deletes
# without --apply. Touches only fuzz/corpus and fuzz/artifacts; never source.
#
# Usage:
#   fuzz/gc.sh                      # report sizes only (dry-run)
#   fuzz/gc.sh --apply              # prune corpora to KEEP newest + drop old artifacts
#   fuzz/gc.sh --keep 500 --age 30  # keep 500 newest inputs/target; drop artifacts >30d
set -euo pipefail

FUZZ_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KEEP=1000          # inputs to retain per corpus target
AGE_DAYS=14        # crash-artifact retention window
APPLY=0

while [ $# -gt 0 ]; do
  case "$1" in
    --apply) APPLY=1 ;;
    --keep) KEEP="$2"; shift ;;
    --age) AGE_DAYS="$2"; shift ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
  shift
done

du_h() { du -sh "$1" 2>/dev/null | cut -f1; }

echo "fuzz GC — dir: $FUZZ_DIR  (KEEP=$KEEP/target, AGE=${AGE_DAYS}d, apply=$APPLY)"

corpus_root="$FUZZ_DIR/corpus"
art_root="$FUZZ_DIR/artifacts"

if [ -d "$corpus_root" ]; then
  echo "== corpus (total: $(du_h "$corpus_root")) =="
  for target in "$corpus_root"/*/; do
    [ -d "$target" ] || continue
    name="$(basename "$target")"
    count="$(find "$target" -maxdepth 1 -type f | wc -l | tr -d ' ')"
    printf "  %-32s %6s inputs  %8s\n" "$name" "$count" "$(du_h "$target")"
    if [ "$APPLY" -eq 1 ] && [ "$count" -gt "$KEEP" ]; then
      # delete oldest-first beyond KEEP (mtime asc), staying inside this target dir
      find "$target" -maxdepth 1 -type f -printf '%T@ %p\n' \
        | sort -n | head -n "$((count - KEEP))" | cut -d' ' -f2- \
        | while IFS= read -r f; do rm -f -- "$f"; done
      echo "    pruned $((count - KEEP)) oldest inputs -> now $(du_h "$target")"
    fi
  done
else
  echo "  (no corpus/ dir)"
fi

if [ -d "$art_root" ]; then
  old_count="$(find "$art_root" -type f -mtime +"$AGE_DAYS" 2>/dev/null | wc -l | tr -d ' ')"
  echo "== artifacts (total: $(du_h "$art_root"), >${AGE_DAYS}d: $old_count) =="
  if [ "$APPLY" -eq 1 ] && [ "$old_count" -gt 0 ]; then
    find "$art_root" -type f -mtime +"$AGE_DAYS" -delete
    echo "    removed $old_count stale artifact(s) -> now $(du_h "$art_root")"
  fi
else
  echo "  (no artifacts/ dir)"
fi

[ "$APPLY" -eq 0 ] && echo "(dry-run — re-run with --apply to act)"
exit 0
