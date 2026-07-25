#!/usr/bin/env python3
"""TACO-skilling quality gate — CREATE Phase 4 and REFINE Phase 5.

Validates a skill against the REGRA #13 hygiene limits and the structural checks
from the quality rubric, then emits a score and a pass/fail verdict.

A failed gate is not a warning. Over a length limit, the fix is extraction to a
reference file — never deletion of substance.

Exit code: 0 = passed, 1 = a gate failed, 2 = bad arguments.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Any, Sequence

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib  # noqa: E402 — local sibling module

NAME_MAX = 64
DESCRIPTION_MAX = 1024
BODY_MAX = 500


def check_skill(skill_dir: Path) -> dict[str, Any]:
    """Run every hygiene and structural gate against a skill directory."""
    checks: list[dict[str, Any]] = []

    def gate(name: str, passed: bool, detail: str) -> None:
        checks.append({"gate": name, "passed": bool(passed), "detail": detail})

    skill_md = skill_dir / "SKILL.md"
    if not skill_md.is_file():
        gate("skill_md_exists", False, "SKILL.md not found")
        return {"skill": skill_dir.name, "dir": str(skill_dir),
                "checks": checks, "score": 0.0, "passed": False}
    gate("skill_md_exists", True, "SKILL.md present")

    frontmatter = lib.read_frontmatter(skill_md)
    name = frontmatter.get("name", "")
    description = frontmatter.get("description", "")

    gate("frontmatter_name", bool(name), f"name = '{name or '(missing)'}'")
    gate("frontmatter_description", bool(description),
         f"description present ({len(description)} chars)"
         if description else "description missing")
    gate("name_length", len(name) <= NAME_MAX, f"{len(name)}/{NAME_MAX} chars")
    gate("description_length", len(description) <= DESCRIPTION_MAX,
         f"{len(description)}/{DESCRIPTION_MAX} chars")

    body = lib.body_line_count(skill_md)
    gate("body_under_limit", body < BODY_MAX, f"{body}/{BODY_MAX} lines")

    # Every reference link in SKILL.md must resolve to a real file.
    text = skill_md.read_text(encoding="utf-8", errors="replace")
    refs = sorted(set(re.findall(r"\]\((references/[^)]+)\)", text)))
    missing = [ref for ref in refs if not (skill_dir / ref).is_file()]
    gate("references_resolve", not missing,
         "all reference links resolve" if not missing
         else f"missing: {', '.join(missing)}")

    # Informational note (always passes): does the skill bundle a layer-3 of scripts?
    scripts_dir = skill_dir / "scripts"
    has_scripts = scripts_dir.is_dir() and (
        any(scripts_dir.glob("*.py")) or any(scripts_dir.glob("*.sh"))
    )
    gate("layer3_note", True,
         "scripts/ present" if has_scripts
         else "no scripts/ — fine only if the skill has no repeated deterministic step")

    passed = all(check["passed"] for check in checks)
    score = sum(1 for check in checks if check["passed"]) / len(checks)
    return {
        "skill": name or skill_dir.name,
        "dir": str(skill_dir),
        "checks": checks,
        "score": round(score, 2),
        "passed": passed,
    }


def main(argv: Sequence[str] | None = None) -> int:
    """Validate a skill directory against the quality gates."""
    parser = argparse.ArgumentParser(
        prog="quality_gate.py",
        description="Validate a skill against REGRA #13 hygiene + structural gates.",
    )
    parser.add_argument("skill_dir", help="Path to the skill directory.")
    parser.add_argument("--json", action="store_true", help="Emit JSON.")
    args = parser.parse_args(argv)

    skill_dir = Path(args.skill_dir).expanduser().resolve()
    if not skill_dir.is_dir():
        print(f"error: not a directory: {skill_dir}", file=sys.stderr)
        return 2

    report = check_skill(skill_dir)

    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 0 if report["passed"] else 1

    print(f"== quality gate: {report['skill']} ==\n")
    for check in report["checks"]:
        mark = "PASS" if check["passed"] else "FAIL"
        print(f"  [{mark}] {check['gate']:22s} {check['detail']}")
    verdict = "PASS" if report["passed"] else "FAIL"
    print(f"\nscore: {report['score']:.0%}   verdict: {verdict}")
    if not report["passed"]:
        print("\na failed gate blocks the skill from shipping.")
        print("over a length limit -> extract content to a reference, never delete substance.")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
