#!/usr/bin/env python3
"""Find Type-1 block clones (6+ consecutive identical production lines).

Mirrors the touring-quality F1_3 signal so we can SEE what is duplicated in each
failing file and classify it: extractable-logic clone (REAL target) vs inherent
structural repetition / data table / N-way scaffold (FALSE POSITIVE where dedup
would be gaming). #[cfg(test)] regions are excluded (as F1_3 does).
"""
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from report_contract import print_contract  # noqa: E402 — sibling module, path set above

MIN = 6  # F1_3 Type-1 block threshold


def strip_tests(lines: list[str]) -> list[tuple[int, str]]:
    """Return (1-based lineno, text) for production lines, dropping cfg(test) mods."""
    out: list[tuple[int, str]] = []
    depth = 0
    in_test = False
    test_depth = 0
    for i, raw in enumerate(lines, 1):
        s = raw.strip()
        if not in_test and "#[cfg(test)]" in s:
            in_test = True
            test_depth = depth
            continue
        if in_test:
            depth += raw.count("{") - raw.count("}")
            if depth <= test_depth and "}" in raw:
                in_test = False
            continue
        depth += raw.count("{") - raw.count("}")
        out.append((i, s))
    return out


def find_clones(path: str) -> list[tuple[str, list[int]]]:
    """Return [(block_text, [start_linenos]), ...] for 6+ line dup blocks."""
    lines = Path(path).read_text().splitlines()
    prod = strip_tests(lines)
    # normalized non-trivial lines only (skip blank / single-brace noise)
    norm = [(ln, t) for ln, t in prod if len(t) > 2 and t not in ("{", "}", "})", "],")]
    seen: dict[str, list[int]] = defaultdict(list)
    for i in range(len(norm) - MIN + 1):
        block = "\n".join(t for _, t in norm[i:i + MIN])
        seen[block].append(norm[i][0])
    return sorted(((b, ls) for b, ls in seen.items() if len(ls) >= 2),
                  key=lambda x: -len(x[1]))


USAGE = """usage: clone_blocks.py [--all | --limit N] [--help] <file.rs> [file.rs ...]

Find Type-1 block clones (6+ consecutive identical production lines), mirroring
the touring-quality F1_3 signal so each block can be classified as an
extractable-logic clone (REAL) or inherent structural repetition (FALSE
POSITIVE, where dedup would be gaming — REGRA #0).

options:
  --limit N   show at most N distinct blocks per file (default: 6)
  --all       show every distinct block — what the reporting contract requires
              for a full audit; --limit 6 is only a preview
  --help      show this message and exit
"""


def parse_args(argv: list[str]) -> tuple[int | None, list[str]]:
    """Split `argv` into `(limit, paths)`; `limit is None` means "show all"."""
    limit: int | None = 6  # a preview — the contract's full audit needs --all
    paths: list[str] = []
    rest = list(argv)
    while rest:
        arg = rest.pop(0)
        if arg == "--all":
            limit = None
        elif arg == "--limit" or arg.startswith("--limit="):
            value = arg.split("=", 1)[1] if "=" in arg else (rest.pop(0) if rest else "")
            if not value.isdigit():
                print("error: --limit needs a non-negative integer", file=sys.stderr)
                raise SystemExit(2)
            limit = int(value)
        elif arg.startswith("-"):
            print(f"error: unknown option {arg!r}\n\n{USAGE}", file=sys.stderr)
            raise SystemExit(2)
        else:
            paths.append(arg)
    return limit, paths


def main(argv: list[str]) -> None:
    """Print the dup-block digest for each path given on the command line."""
    if "--help" in argv or "-h" in argv:
        print(USAGE)
        return
    limit, paths = parse_args(argv)
    for path in paths:
        if not Path(path).is_file():
            print(f"\n{'='*78}\n{path}  — SKIPPED (not a readable file)", file=sys.stderr)
            continue
        clones = find_clones(path)
        total_dup = sum(len(ls) - 1 for _, ls in clones)
        shown = clones if limit is None else clones[:limit]
        print(f"\n{'='*78}\n{path}  — {len(clones)} distinct 6-line dup blocks, ~{total_dup} redundant instances")
        for block, locs in shown:
            first = block.split("\n")[0][:70]
            print(f"  ×{len(locs)} @ lines {locs[:8]}  first: {first}")
        if limit is not None and len(clones) > limit:
            print(f"  … {len(clones) - limit} more block(s) hidden — re-run with --all for the full audit")
    if paths:  # no input → no diagnostic → no contract (a zero-path run stays a no-op)
        print_contract(
            "(stdout only — re-run per file for the full block list)",
            "clone_blocks — F1_3 Type-1 clone detector",
            [
                "every distinct 6-line duplicate block found, with its line locations (not just the top 6)",
                "the real-dedup vs scaffold-FP classification — do NOT game structural FPs "
                "(data tables, N-handler trait scaffolds) into a fake score gain (REGRA #0)",
            ],
        )


if __name__ == "__main__":
    main(sys.argv[1:])
