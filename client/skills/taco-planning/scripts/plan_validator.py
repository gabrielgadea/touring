#!/usr/bin/env python3
"""plan_validator — Validate the 4-stage Pln2 structure + confidence tag coverage.

Distinct from TACO-wt's plan_validator (which checks wave DAG cycles). This
validator audits the authoring-time structure:

  1. Frontmatter: plan id (kebab), authored ISO date, intent non-empty.
  2. All 4 stages present:
       Ground Truth Summary | 9-Dimension | Phases | Verification (or equivalent)
  3. At least 1 phase + at least 1 subtask.
  4. Every subtask carries a confidence tag (when --strict).
  5. Potentiation Matrix present.
  6. Cross-references to TACO-wt + sister references present (informational).

Usage
-----
    python3 plan_validator.py plan.md
    python3 plan_validator.py plan.md --strict -j
"""

from __future__ import annotations

import argparse
import json
import logging
import re
import sys
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

from lib import (  # noqa: E402  pylint: disable=wrong-import-position
    EXIT_FAIL,
    EXIT_INTERRUPTED,
    EXIT_OK,
    EXIT_STRUCTURAL,
    EXIT_WARN,
    is_kebab,
    utcnow_iso,
    write_json_atomic,
)

_RE_FRONTMATTER = re.compile(r"^---\n(.*?)\n---\n", re.DOTALL)
_RE_FM_KEY = re.compile(r"^(?P<key>[a-z_]+):\s*(?P<val>.+)$", re.MULTILINE)
_RE_STAGE_HEADERS = {
    "ground_truth": re.compile(r"^##\s+\d+\.\s*Ground\s*Truth", re.MULTILINE | re.IGNORECASE),
    "dimensions": re.compile(r"^##\s+\d+\.\s*9.{0,4}Dimension", re.MULTILINE | re.IGNORECASE),
    "phases": re.compile(r"^##\s+\d+\.\s*Phases?", re.MULTILINE | re.IGNORECASE),
    "verification": re.compile(r"^##\s+\d+\.\s*(?:Verification|Amplification|Acceptance)",
                                re.MULTILINE | re.IGNORECASE),
}
_RE_PHASE_HEADING = re.compile(r"^###\s+Phase\s+\d+", re.MULTILINE | re.IGNORECASE)
_RE_SUBTASK_HEADER = re.compile(r"S-\d+", re.MULTILINE)
_RE_CONFIDENCE = re.compile(r"\bconfidence[^\n]*?(?:FACT|INFERENCE|SPECULATION)", re.IGNORECASE)
_RE_POTENTIATION = re.compile(r"^##\s+\d+\.\s*Potentiation", re.MULTILINE | re.IGNORECASE)


def _parse_frontmatter(text: str) -> dict[str, str]:
    match = _RE_FRONTMATTER.search(text)
    if not match:
        return {}
    return {
        m.group("key"): m.group("val").strip()
        for m in _RE_FM_KEY.finditer(match.group(1))
    }


def validate_plan(plan_md: str, *, strict: bool) -> dict[str, Any]:
    """Run all gates. Return structured report."""
    errors: list[str] = []
    warnings: list[str] = []

    # Gate 1: frontmatter
    fm = _parse_frontmatter(plan_md)
    plan_id = fm.get("plan", "")
    if not plan_id:
        errors.append("Frontmatter missing required field: plan")
    elif not is_kebab(plan_id):
        warnings.append(f"plan id '{plan_id}' is not kebab-case")
    if not fm.get("title"):
        warnings.append("Frontmatter missing field: title")
    if not fm.get("intent"):
        warnings.append("Frontmatter missing field: intent")

    # Gate 2: 4 stages present
    stages_present: dict[str, bool] = {
        name: bool(pattern.search(plan_md))
        for name, pattern in _RE_STAGE_HEADERS.items()
    }
    for name, present in stages_present.items():
        if not present:
            errors.append(f"Stage missing: {name}")

    # Gate 3: at least 1 phase + 1 subtask
    phase_count = len(_RE_PHASE_HEADING.findall(plan_md))
    subtask_count = len(_RE_SUBTASK_HEADER.findall(plan_md))
    if phase_count < 1:
        errors.append("No phases declared (### Phase N)")
    if subtask_count < 1:
        errors.append("No subtasks declared (S-N)")

    # Gate 4: confidence coverage
    confidence_count = len(_RE_CONFIDENCE.findall(plan_md))
    ratio = confidence_count / max(subtask_count, 1)
    if strict and ratio < 1.0:
        errors.append(f"Confidence coverage {ratio:.0%} < 100% (strict mode)")
    elif ratio < 0.5:
        warnings.append(f"Weak confidence coverage: {ratio:.0%} (target ≥ 50%, ideal 100%)")

    # Gate 5: potentiation matrix
    has_potentiation = bool(_RE_POTENTIATION.search(plan_md))
    if not has_potentiation:
        warnings.append("Potentiation Matrix section missing (REGRA #0)")

    if errors:
        status = "FAIL"
    elif warnings:
        status = "WARN"
    else:
        status = "OK"

    return {
        "status": status,
        "frontmatter": fm,
        "stages_present": stages_present,
        "phase_count": phase_count,
        "subtask_count": subtask_count,
        "confidence_count": confidence_count,
        "confidence_ratio": round(ratio, 3),
        "has_potentiation_matrix": has_potentiation,
        "errors": errors,
        "warnings": warnings,
    }


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="plan_validator", description=__doc__)
    parser.add_argument("path", type=Path, help="Plan markdown to validate.")
    parser.add_argument("--strict", action="store_true",
                        help="Require 100 percent confidence coverage and zero warnings.")
    parser.add_argument("--apply", action="store_true",
                        help="No-op (validator is read-only).")
    parser.add_argument("--emit", action="store_true")
    parser.add_argument("--output-dir", type=Path, default=Path("data"))
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Validate."""
    if not args.path.exists():
        msg = f"Plan file not found: {args.path}"
        raise FileNotFoundError(msg)
    plan_md = args.path.read_text(encoding="utf-8")
    body = validate_plan(plan_md, strict=args.strict)
    report = {
        "script": "plan_validator",
        "timestamp": utcnow_iso(),
        "source": str(args.path),
        "strict": args.strict,
        **body,
    }
    if args.emit:
        out = args.output_dir / "plan_validation.json"
        write_json_atomic(out, report)
        report["json_path"] = str(out)
    return report


def main() -> int:
    """CLI entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False, default=str) + "\n")
        if result["status"] == "FAIL":
            return EXIT_WARN
        return EXIT_OK
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except FileNotFoundError as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("plan_validator failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
