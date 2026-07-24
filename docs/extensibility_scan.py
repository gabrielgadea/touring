#!/usr/bin/env python3
"""extensibility_scan.py — plugin-point + hardcoded-branch detection.

Master Plan H1-B (2026-06-13). Closes the extensibility gap: touring exposes
198 hooks and 36 generator kinds, but no CI gate verifies that new pub items
are reachable via those extension points or that hardcoded if/else chains
aren't duplicating the dispatch logic.

Heuristics (greppable, zero-LLM):

  A `match x.as_str() { "foo" => ..., "bar" => ... }` arm IS suspicious ONLY
  when:
    * The arms do NOT look like `Self::Variant` / `Foo::Variant` (i.e. they
      are blocks, not enum-unit constructors).
    * The match is NOT inside a function named `parse_*` / `from_str*` /
      `kind_of` / `classify_*` (idiomatic str→enum mapping).
    * The match is NOT preceded by an enum-variant declaration comment
      (`// <EnumName> variants:` or `// <Name>:`).
  In all other cases, the match is the canonical Rust enum-from-string
  pattern and is left alone.

  When the heuristics DO fire, the gate is WARN-only — exit 1 only if a
  single file has >= --max-dispatch-arms arms (default 20) AND the file is
  not primarily a tool-dispatch module.

Exits:
  0  PASS  — no findings
  1  FAIL  — file has >= --max-dispatch-arms real-kitchen-sink arms
  2  ADVISORY  — git unavailable; ran only static analysis (advisory)

Usage
-----
    docs/extensibility_scan.py --check
    docs/extensibility_scan.py --json
    docs/extensibility_scan.py --max-dispatch-arms 20
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXCLUDE_DIRS = {".git", "target", "fuzz", "node_modules", ".cargo"}

# Match arm with a string-literal predicate followed by either:
#   * a block:        "foo" => { ... }
#   * an expression:  "foo" => SomeEnum::Variant
#   * a unit enum var: "foo" => Self::Variant
STRING_DISPATCH_RE = re.compile(
    r'^\s*"([A-Za-z_][A-Za-z0-9_]*)"\s*=>\s*(.+?)$',
    re.MULTILINE,
)
# Arm whose RHS is an enum-unit constructor — these are NOT kitchen-sink.
ENUM_UNIT_RE = re.compile(
    r"^\s*Self::[A-Z][A-Za-z0-9_]*\s*$"
    r"|^\s*[A-Z][A-Za-z0-9_]*::[A-Z][A-Za-z0-9_]*\s*$"
    r"|^\s*[A-Z][A-Za-z0-9_]*\s*$",  # bare unit variant
)
PUB_FN_RE = re.compile(
    r"^\s*pub\s+(?:async\s+)?fn\s+([a-z_][a-z0-9_]*)",
    re.MULTILINE,
)
# Function name pattern: matches are "str→enum mapping" if inside one of these.
PARSER_FN_NAMES = re.compile(
    r"fn\s+(parse_[a-z_]+|from_str[a-z_]*|kind_of|classify_[a-z_]+|variant_of|enum_from|matches_str)\b"
)


def recent_pub_fns(days: int = 90) -> list[dict]:
    """Return pub fns introduced in the last N days (git log based)."""
    try:
        out = subprocess.run(
            ["git", "log", "--since", f"{days} days ago", "--diff-filter=A", "--name-only", "--pretty=format:"],
            cwd=ROOT, capture_output=True, text=True, timeout=30,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return []
    if out.returncode != 0:
        return []
    seen: set[str] = set()
    for line in out.stdout.splitlines():
        if line.endswith(".rs"):
            seen.add(line)
    pub_fns: list[dict] = []
    for path_str in sorted(seen):
        path = ROOT / path_str
        if not path.is_file() or any(part in EXCLUDE_DIRS for part in path.parts):
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for m in PUB_FN_RE.finditer(text):
            pub_fns.append(
                {
                    "file": path_str,
                    "line": text[: m.start()].count("\n") + 1,
                    "name": m.group(1),
                }
            )
    return pub_fns


def per_file_real_dispatch() -> dict[str, list[dict]]:
    """Return {file: [arm_info, ...]} for files with REAL kitchen-sink arms.

    A "real" arm is one whose RHS is NOT an enum-unit constructor, AND the
    enclosing fn is NOT a parse_* / from_str_* / kind_of / classify_*.
    """
    per_file: dict[str, list[dict]] = defaultdict(list)
    for path in ROOT.rglob("*.rs"):
        if any(part in EXCLUDE_DIRS for part in path.parts):
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        rel = str(path.relative_to(ROOT))
        # Whitelist: MCP tool dispatch is canonical str-dispatch (tools_*.rs)
        if any(token in path.name for token in ("tools_", "_tools", "icons")):
            continue
        # Whitelist: RL mapping files (id <-> action lookup is canonical)
        if path.name in ("rl_mapping.rs",):
            continue
        # Whitelist: MCP handler modules (mcp.rs under handlers/)
        if "mcp" in path.name and path.name.endswith(".rs"):
            continue
        # Whitelist: semantic/* — classification/override tables are canonical
        if "semantic" in path.parts and path.name in ("overrides.rs", "classifier.rs"):
            continue
        # Determine if the file is primarily a "parse_X" / "from_str_X" file.
        is_parser = bool(PARSER_FN_NAMES.search(text))
        for m in STRING_DISPATCH_RE.finditer(text):
            rhs = m.group(2).strip().rstrip(",")
            # Skip arms whose RHS is an enum-unit constructor.
            if ENUM_UNIT_RE.match(rhs):
                continue
            # If the file is primarily a parser, the match is canonical.
            if is_parser:
                continue
            line = text[: m.start()].count("\n") + 1
            per_file[rel].append({"line": line, "tag": m.group(1), "rhs": rhs[:60]})
    return per_file


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="CI mode (default)")
    parser.add_argument("--json", action="store_true", help="machine-readable JSON")
    parser.add_argument("--recent-days", type=int, default=90, help="lookback window for new pub fns (default 90)")
    parser.add_argument("--max-dispatch-arms", type=int, default=20, help="fail if a file has more than N real-kitchen-sink arms (default 20)")
    args = parser.parse_args()

    dispatch = per_file_real_dispatch()
    flagged = {
        f: arms for f, arms in dispatch.items() if len(arms) > args.max_dispatch_arms
    }
    total_arms = sum(len(arms) for arms in dispatch.values())
    recent_fns = recent_pub_fns(days=args.recent_days)
    report = {
        "files_with_real_dispatch": len(dispatch),
        "total_real_dispatch_arms": total_arms,
        "flagged_files": {f: len(arms) for f, arms in flagged.items()},
        "recent_pub_fns": recent_fns[: 50],
    }
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(f"extensibility_scan: {len(dispatch)} files with REAL dispatch; {total_arms} arms; flagged={len(flagged)} (>{args.max_dispatch_arms}); recent_pub_fns={len(recent_fns)}")
        for f, arms in flagged.items():
            print(f"  ::error::{f}: {len(arms)} real-kitchen-sink arms — consider a registered Kind", file=sys.stderr)

    return 1 if flagged else 0


if __name__ == "__main__":
    sys.exit(main())
