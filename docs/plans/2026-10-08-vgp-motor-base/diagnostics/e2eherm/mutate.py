#!/usr/bin/env python3
"""Content-addressed mutation: replace EXACTLY one occurrence of OLD with NEW in FILE.

usage: mutate.py FILE OLD NEW      (exit 3 if OLD does not occur exactly once)
Reversal is the same call with OLD/NEW swapped, so a restore is by content, never by git.
"""
import sys

path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, encoding="utf-8") as fh:
    text = fh.read()
n = text.count(old)
if n != 1:
    print(f"mutate.py: expected exactly 1 occurrence of {old!r} in {path}, found {n}", file=sys.stderr)
    sys.exit(3)
with open(path, "w", encoding="utf-8") as fh:
    fh.write(text.replace(old, new))
print(f"mutate.py: replaced 1 occurrence in {path}")
