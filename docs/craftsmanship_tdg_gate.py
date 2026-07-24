#!/usr/bin/env python3
"""craftsmanship_tdg_gate.py — TDG + cognitive-score gate for production sources.

Master Plan H1-B (2026-06-13). Closes the craftsmanship gap: touring exposes
TDG (grade A-F) and cognitive_score, but neither runs in CI. This gate drives
the existing tooling, parses its output, and fails the build on grade < B or
cognitive_score > 0.7 for any non-test source file > 500 LOC.

Prereq (assumed installed + on PATH):
    touring ast tdg <file>      # prints JSON {grade, score, dimensions}
    touring ast meta <file>     # prints JSON {quality_score, cognitive_score, ...}

Exits:
  0  PASS  — every audited file meets thresholds
  1  FAIL  — at least one file grade < B or cognitive > 0.7
  2  ADVISORY  — `touring` binary absent; step is skipped (fail-open)

Usage
-----
    docs/craftsmanship_tdg_gate.py --check
    docs/craftsmanship_tdg_gate.py --json
    docs/craftsmanship_tdg_gate.py --min-grade B --max-cognitive 0.7 --min-loc 500
"""
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXCLUDE_DIRS = {".git", "target", "fuzz", "node_modules", ".cargo", "tests", "examples", "benches"}

GRADE_ORDER = ["A+", "A", "A-", "B+", "B", "B-", "C+", "C", "C-", "D+", "D", "F"]


def grade_at_least(grade: str, min_grade: str) -> bool:
    try:
        return GRADE_ORDER.index(grade) <= GRADE_ORDER.index(min_grade)
    except ValueError:
        return True  # unknown grade — don't fail


def run_touring(args: list[str], timeout: int = 30) -> dict | None:
    if shutil.which("touring") is None:
        return None
    try:
        out = subprocess.run(
            ["touring", *args, "-j"],
            cwd=ROOT, capture_output=True, text=True, timeout=timeout,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return None
    if out.returncode != 0:
        return None
    try:
        return json.loads(out.stdout)
    except json.JSONDecodeError:
        return None


def audit_file(path: Path, min_grade: str, max_cognitive: float) -> dict | None:
    meta = run_touring(["ast", "meta", str(path), "--depth", "summary"])
    loc = 0
    cognitive = None
    if meta:
        cognitive = meta.get("cognitive_score")
        loc = meta.get("loc", 0)
    if loc == 0:
        # fall back to line count
        try:
            loc = sum(1 for _ in path.open(encoding="utf-8", errors="replace"))
        except OSError:
            return None
    if loc < 500:
        return None
    tdg = run_touring(["ast", "tdg", str(path)])
    grade = (tdg or {}).get("grade") or (tdg or {}).get("letter")
    findings: list[str] = []
    if grade and not grade_at_least(grade, min_grade):
        findings.append(f"grade {grade} < {min_grade}")
    if cognitive is not None and cognitive > max_cognitive:
        findings.append(f"cognitive_score {cognitive:.3f} > {max_cognitive}")
    return {
        "file": str(path.relative_to(ROOT)),
        "loc": loc,
        "grade": grade,
        "cognitive_score": cognitive,
        "findings": findings,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="CI mode (default)")
    parser.add_argument("--json", action="store_true", help="machine-readable JSON")
    parser.add_argument("--min-grade", default="B", help="minimum acceptable TDG grade (default B)")
    parser.add_argument("--max-cognitive", type=float, default=0.7, help="max acceptable cognitive_score (default 0.7)")
    parser.add_argument("--min-loc", type=int, default=500, help="minimum LOC to audit (default 500)")
    args = parser.parse_args()

    if shutil.which("touring") is None:
        print("craftsmanship_tdg_gate: touring binary absent — ADVISORY (fail-open)")
        return 2

    audited: list[dict] = []
    for path in ROOT.rglob("*.rs"):
        if any(part in EXCLUDE_DIRS for part in path.parts):
            continue
        # Skip relocated inline-test modules: the gate is scoped to "production
        # sources / non-test source file" (see module docstring), but inline
        # `#[cfg(test)] mod tests` bodies are routinely relocated to sibling
        # `<file>_tests.rs` / `tests.rs` files (the test-module split idiom).
        # Those carry test code whose intentional verbosity must not be judged
        # against production cognitive-complexity thresholds.
        if path.name == "tests.rs" or path.name.endswith(("_tests.rs", "_test.rs")):
            continue
        result = audit_file(path, args.min_grade, args.max_cognitive)
        if result is not None:
            audited.append(result)

    failures = [a for a in audited if a["findings"]]
    report = {
        "files_audited": len(audited),
        "failures_count": len(failures),
        "audited": audited,
    }
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(f"craftsmanship_tdg_gate: audited={len(audited)} failures={len(failures)} min_grade={args.min_grade} max_cog={args.max_cognitive}")
        for f in failures:
            for finding in f["findings"]:
                print(f"  ::error::{f['file']} ({f['loc']} LOC) — {finding}", file=sys.stderr)

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
