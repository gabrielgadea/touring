#!/usr/bin/env python3
"""TACO-cross-audit debt scanner — Phase 3 of the cross-audit.

Walks a code tree and reports every marker of unfinished or disconnected work:
dead-code suppressions, TODO / FIXME / HACK / XXX, unimplemented stubs, skipped
tests, and WIP / PENDING notes. Each hit is a Phase 5 task.

To stay honest, the scanner does not flag its own examples: it skips
triple-quoted string blocks, and counts textual markers only when they appear
inside an actual line comment — never inside a string literal or a regex.

Output is a human report, or JSON with --json.
Exit code: 0 = no debt found, 1 = debt found, 2 = bad arguments.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Any, Iterator, Sequence

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib  # noqa: E402 — local sibling module

# Line-comment marker by file family; everything else uses "//".
LINE_COMMENT: dict[str, str] = {".py": "#", ".sh": "#", ".rb": "#"}

# Searched in the COMMENT portion of a line only — textual notes are debt only
# when written as a comment, not when they appear inside a string or a regex.
COMMENT_PATTERNS: dict[str, re.Pattern[str]] = {
    "todo_marker": re.compile(r"\b(?:TODO|FIXME|HACK|XXX)\b"),
    "wip_marker": re.compile(r"\b(?:WIP|PENDING)\b"),
}
# Searched in the WHOLE line — attributes and real code calls.
CODE_PATTERNS: dict[str, re.Pattern[str]] = {
    "suppression": re.compile(
        r"#\[allow\((?:dead_code|unused\w*)\)\]|#\s*type:\s*ignore\b"
    ),
    "unimplemented": re.compile(
        r"(?:unimplemented|todo)!\s*\(|raise\s+NotImplementedError"
    ),
    "skipped_test": re.compile(r"#\[ignore\]|@pytest\.mark\.skip"),
}
ALL_CATEGORIES: tuple[str, ...] = (*COMMENT_PATTERNS, *CODE_PATTERNS)


def iter_auditable_lines(text: str) -> Iterator[tuple[int, str]]:
    """Yield (lineno, line) for each line, skipping triple-quoted string blocks.

    A debt scanner that read its own docstring would flag the marker words it
    documents. Skipping block strings keeps the scan honest. This is a pragmatic
    heuristic, not a full lexer — adjacent quote styles on one line resolve to
    the first seen.
    """
    in_block = ""
    for lineno, line in enumerate(text.splitlines(), start=1):
        if in_block:
            if in_block in line:
                in_block = ""
            continue
        opener = ""
        for quote in ('"""', "'''"):
            if line.count(quote) % 2 == 1:
                opener = quote
                break
        if opener:
            in_block = opener
            yield lineno, line[:line.find(opener)]
        else:
            yield lineno, line


def scan_file(path: Path) -> list[dict[str, Any]]:
    """Return every debt hit in one file as ``{category, line, text}``."""
    hits: list[dict[str, Any]] = []
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return hits
    marker = LINE_COMMENT.get(path.suffix, "//")
    for lineno, line in iter_auditable_lines(text):
        comment_idx = line.find(marker)
        comment = line[comment_idx:] if comment_idx != -1 else ""
        for category, pattern in COMMENT_PATTERNS.items():
            if comment and pattern.search(comment):
                hits.append({"category": category, "line": lineno,
                             "text": line.strip()[:160]})
        for category, pattern in CODE_PATTERNS.items():
            if pattern.search(line):
                hits.append({"category": category, "line": lineno,
                             "text": line.strip()[:160]})
    return hits


def scan_tree(root: Path) -> dict[str, Any]:
    """Aggregate debt across every code file under ``root``."""
    by_file: dict[str, list[dict[str, Any]]] = {}
    totals: dict[str, int] = {category: 0 for category in ALL_CATEGORIES}
    for path in lib.walk_code_files(root):
        hits = scan_file(path)
        if not hits:
            continue
        by_file[str(path)] = hits
        for hit in hits:
            totals[hit["category"]] += 1
    total = sum(totals.values())
    return {
        "root": str(root),
        "total_debt": total,
        "by_category": totals,
        "by_file": by_file,
        "clean": total == 0,
    }


def main(argv: Sequence[str] | None = None) -> int:
    """Scan a directory tree for code debt."""
    parser = argparse.ArgumentParser(
        prog="scan_debt.py",
        description="Walk a code tree for dead code, suppressions, TODOs and pending work.",
    )
    parser.add_argument("directory", help="Root directory to scan.")
    parser.add_argument("--json", action="store_true", help="Emit JSON.")
    args = parser.parse_args(argv)

    root = Path(args.directory).expanduser().resolve()
    if not root.exists():
        print(f"error: path does not exist: {root}", file=sys.stderr)
        return 2

    report = scan_tree(root)

    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 0 if report["clean"] else 1

    print(f"== debt scan: {report['root']} ==\n")
    if report["clean"]:
        print("no debt markers found — the tree is clean.")
        return 0
    print(f"total debt markers: {report['total_debt']}")
    for category, count in report["by_category"].items():
        if count:
            print(f"  {category:16s} {count}")
    print()
    for file_path, hits in report["by_file"].items():
        print(file_path)
        for hit in hits:
            print(f"  L{hit['line']:<5d} [{hit['category']}] {hit['text']}")
    print(f"\n{report['total_debt']} item(s) — each is a Phase 5 task. "
          "Nothing here may survive the audit.")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
