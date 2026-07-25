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


def main(paths: list[str]) -> None:
    """Print the dup-block digest for each path given on the command line."""
    for path in paths:
        clones = find_clones(path)
        total_dup = sum(len(ls) - 1 for _, ls in clones)
        print(f"\n{'='*78}\n{path}  — {len(clones)} distinct 6-line dup blocks, ~{total_dup} redundant instances")
        for block, locs in clones[:6]:
            first = block.split("\n")[0][:70]
            print(f"  ×{len(locs)} @ lines {locs[:8]}  first: {first}")
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
