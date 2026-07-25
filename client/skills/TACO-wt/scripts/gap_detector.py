#!/usr/bin/env python3
"""gap_detector — Detect P0-P3 gaps between declared waves and actual evidence.

Adapted from ``analise/scripts/pln2_generator/gap_detector.py``. Scope here:
detect gaps in a multi-wave plan — declared waves missing evidence, missing
sub-scripts, missing wave validators, asymmetric cross-references.

Severity policy
---------------
  P0  critical path waves missing evidence            → block plan acceptance
  P1  non-critical waves missing evidence             → resolve mid-plan
  P2  asymmetric cross-references (declared not impl) → resolve before close
  P3  cosmetic / documentation gaps                    → defer if time-bound

Usage
-----
    python3 gap_detector.py path/to/plan.md
    python3 gap_detector.py path/to/plan.md --plan-dir scripts/<plan>
    python3 gap_detector.py path/to/plan.md -j
"""

from __future__ import annotations

import argparse
import json
import logging
import re
import sys
from collections import defaultdict
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
    find_plan_dir,
    utcnow_iso,
    write_json_atomic,
)

# Patterns
_RE_WAVE_HEADING = re.compile(r"^###\s+(W\d{1,3}(?:\.\d+)?)\s+[—\-:]\s*(.+)$", re.MULTILINE)
_RE_WAVE_FIELD_TABLE = re.compile(
    r"\|\s*Critical path\?\s*\|\s*(yes|no)\s*\|", re.IGNORECASE,
)
_RE_DEPENDS_ON = re.compile(r"\|\s*Depends on\s*\|\s*([^|]+?)\s*\|", re.IGNORECASE)
_RE_SUB_SCRIPTS = re.compile(r"\|\s*Sub-scripts\s*\|\s*([^|]+?)\s*\|", re.IGNORECASE)


# ── Plan parsing ──────────────────────────────────────────────────────────


def parse_waves(plan_md: str) -> list[dict[str, Any]]:
    """Extract wave declarations from a plan markdown.

    Each wave is expected as ``### W<N> — <title>`` followed by a table of
    fields. This parser is tolerant: missing fields default to sensible values.
    """
    waves: list[dict[str, Any]] = []
    matches = list(_RE_WAVE_HEADING.finditer(plan_md))
    for idx, match in enumerate(matches):
        wave_id = match.group(1)
        title = match.group(2).strip()
        start = match.start()
        end = matches[idx + 1].start() if idx + 1 < len(matches) else len(plan_md)
        body = plan_md[start:end]

        critical = False
        deps: list[str] = []
        subs: list[str] = []

        cp_match = _RE_WAVE_FIELD_TABLE.search(body)
        if cp_match:
            critical = cp_match.group(1).lower() == "yes"

        dep_match = _RE_DEPENDS_ON.search(body)
        if dep_match:
            dep_raw = dep_match.group(1).strip()
            if dep_raw and dep_raw != "—":
                deps = [d.strip() for d in dep_raw.split(",") if d.strip()]

        sub_match = _RE_SUB_SCRIPTS.search(body)
        if sub_match:
            sub_raw = sub_match.group(1).strip()
            # Sub-scripts can be inline-code, backtick-wrapped; strip the formatting
            sub_clean = re.sub(r"[`*]", "", sub_raw)
            subs = [s.strip() for s in sub_clean.split(",") if s.strip()]

        waves.append({
            "wave": wave_id,
            "title": title,
            "critical_path": critical,
            "depends_on": deps,
            "sub_scripts": subs,
        })
    return waves


# ── Gap detection ─────────────────────────────────────────────────────────


def _severity_for_wave(wave: dict[str, Any]) -> str:
    """Map (critical_path, ...) to a severity letter."""
    if wave.get("critical_path", False):
        return "P0"
    return "P1"


def detect_evidence_gaps(
    waves: list[dict[str, Any]],
    plan_dir: Path | None,
) -> list[dict[str, Any]]:
    """For each declared wave, check whether ``data/<wave>-*.json`` exists."""
    if plan_dir is None:
        return [
            {
                "id": f"G-EV-{wave['wave']}",
                "wave": wave["wave"],
                "current_state": f"{wave['wave']} declared but plan_dir not found on disk",
                "target_state": "Plan directory exists with data/<wave>-*.json artifacts",
                "severity": "P2",
                "remediation": "Run `scaffold_wave.py` to create plan structure.",
            }
            for wave in waves
        ]

    gaps: list[dict[str, Any]] = []
    data_dir = plan_dir / "data"
    for wave in waves:
        if not data_dir.exists():
            gaps.append({
                "id": f"G-EV-{wave['wave']}",
                "wave": wave["wave"],
                "current_state": f"data/ directory missing for {wave['wave']}",
                "target_state": f"data/{wave['wave']}-*.json artifacts present",
                "severity": _severity_for_wave(wave),
                "remediation": "Run the wave's forensic sub-scripts.",
            })
            continue
        evidence = list(data_dir.glob(f"{wave['wave']}-*.json"))
        if not evidence:
            gaps.append({
                "id": f"G-EV-{wave['wave']}",
                "wave": wave["wave"],
                "current_state": f"No data/{wave['wave']}-*.json artifacts found",
                "target_state": f"At least one {wave['wave']}-*.json artifact in data/",
                "severity": _severity_for_wave(wave),
                "remediation": f"Run sub-scripts for {wave['wave']}.",
            })
    return gaps


def detect_subscript_gaps(
    waves: list[dict[str, Any]],
    plan_dir: Path | None,
) -> list[dict[str, Any]]:
    """For each declared sub-script in the plan, check whether the .py file exists."""
    if plan_dir is None:
        return []
    gaps: list[dict[str, Any]] = []
    for wave in waves:
        wave_dir = plan_dir / wave["wave"]
        for sub in wave.get("sub_scripts", []):
            # Strip .py if present, then test both forms
            sub_clean = sub.replace(".py", "").strip()
            if not sub_clean:
                continue
            candidate_a = wave_dir / f"{sub_clean}.py"
            candidate_b = plan_dir / f"{sub_clean}.py"
            if not candidate_a.exists() and not candidate_b.exists():
                gaps.append({
                    "id": f"G-SUB-{wave['wave']}-{sub_clean}",
                    "wave": wave["wave"],
                    "current_state": f"Declared sub-script {sub_clean}.py not found",
                    "target_state": f"{sub_clean}.py present in {wave_dir.name}/ or plan root",
                    "severity": _severity_for_wave(wave),
                    "remediation": f"Run scaffold_wave.py --wave {wave['wave']} or write the script manually.",
                })
    return gaps


def detect_validator_gaps(
    waves: list[dict[str, Any]],
    plan_dir: Path | None,
) -> list[dict[str, Any]]:
    """Every wave should have a validate_W<N>.py."""
    if plan_dir is None:
        return []
    gaps: list[dict[str, Any]] = []
    for wave in waves:
        candidates = [
            plan_dir / wave["wave"] / f"validate_{wave['wave']}.py",
            plan_dir / f"validate_{wave['wave']}.py",
        ]
        if not any(c.exists() for c in candidates):
            gaps.append({
                "id": f"G-VAL-{wave['wave']}",
                "wave": wave["wave"],
                "current_state": f"No validate_{wave['wave']}.py validator script",
                "target_state": f"validate_{wave['wave']}.py present",
                "severity": "P1",
                "remediation": f"Run scaffold_wave.py --wave {wave['wave']} to generate validator.",
            })
    return gaps


def detect_dependency_gaps(waves: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Check dependencies reference declared waves; flag dangling references."""
    declared = {w["wave"] for w in waves}
    gaps: list[dict[str, Any]] = []
    for wave in waves:
        for dep in wave.get("depends_on", []):
            if dep and dep not in declared:
                gaps.append({
                    "id": f"G-DEP-{wave['wave']}-{dep}",
                    "wave": wave["wave"],
                    "current_state": f"{wave['wave']} depends on {dep}, but {dep} is not declared",
                    "target_state": f"Either declare {dep} or remove the dependency",
                    "severity": "P2",
                    "remediation": f"Add a `### {dep}` section or correct the typo.",
                })
    return gaps


def prioritize(gaps: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Sort gaps by severity (P0 first)."""
    order = {"P0": 0, "P1": 1, "P2": 2, "P3": 3}
    return sorted(gaps, key=lambda g: order.get(g.get("severity", "P3"), 99))


def group_remediations(gaps: list[dict[str, Any]]) -> dict[str, list[str]]:
    """Group remediation strings by severity."""
    grouped: dict[str, list[str]] = defaultdict(list)
    for gap in gaps:
        grouped[gap.get("severity", "P3")].append(gap.get("remediation", ""))
    return dict(grouped)


# ── CLI ───────────────────────────────────────────────────────────────────


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="gap_detector", description=__doc__)
    parser.add_argument("path", type=Path, help="Plan markdown to analyse")
    parser.add_argument("--plan-dir", type=Path, default=None,
                        help="Plan directory (data/, W<N>/). Auto-detected if absent.")
    parser.add_argument("--apply", action="store_true",
                        help="No-op (gap_detector is read-only).")
    parser.add_argument("--output-dir", type=Path, default=Path("data"),
                        help="Where to emit the report (when --emit).")
    parser.add_argument("--emit", action="store_true")
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    parser.add_argument("--fail-on", choices=["P0", "P1", "P2", "any", "none"],
                        default="P0",
                        help="Exit with code 2 if a gap of this severity-or-worse is found.")
    return parser


def _should_fail(gaps: list[dict[str, Any]], threshold: str) -> bool:
    """Decide exit code based on highest gap severity."""
    if threshold == "none":
        return False
    if threshold == "any":
        return bool(gaps)
    order = {"P0": 0, "P1": 1, "P2": 2, "P3": 3}
    limit = order.get(threshold, 0)
    for gap in gaps:
        if order.get(gap.get("severity", "P3"), 99) <= limit:
            return True
    return False


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Detect gaps."""
    if not args.path.exists():
        msg = f"Plan file not found: {args.path}"
        raise FileNotFoundError(msg)
    plan_md = args.path.read_text(encoding="utf-8")
    waves = parse_waves(plan_md)

    plan_dir = args.plan_dir
    if plan_dir is None:
        # Try auto-detect: assume plan name == parent dir name
        plan_name = args.path.stem.replace("plan", "").strip("-_")
        if not plan_name:
            plan_name = args.path.parent.name
        plan_dir = find_plan_dir(plan_name)

    evidence_gaps = detect_evidence_gaps(waves, plan_dir)
    subscript_gaps = detect_subscript_gaps(waves, plan_dir)
    validator_gaps = detect_validator_gaps(waves, plan_dir)
    dep_gaps = detect_dependency_gaps(waves)

    all_gaps = prioritize(evidence_gaps + subscript_gaps + validator_gaps + dep_gaps)
    remediations = group_remediations(all_gaps)

    severity_counts: dict[str, int] = defaultdict(int)
    for gap in all_gaps:
        severity_counts[gap.get("severity", "P3")] += 1

    report = {
        "status": "OK",
        "script": "gap_detector",
        "timestamp": utcnow_iso(),
        "source": str(args.path),
        "plan_dir": str(plan_dir) if plan_dir else None,
        "waves_declared": len(waves),
        "gaps_total": len(all_gaps),
        "severity_counts": dict(severity_counts),
        "gaps": all_gaps,
        "remediations_by_severity": remediations,
    }

    if args.emit:
        out = args.output_dir / "gap_detection.json"
        write_json_atomic(out, report)
        report["json_path"] = str(out)

    report["_fail_threshold"] = args.fail_on
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
        threshold = result.pop("_fail_threshold", "P0")
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False, default=str) + "\n")
        if _should_fail(result.get("gaps", []), threshold):
            return EXIT_WARN
        return EXIT_OK
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except FileNotFoundError as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("gap_detector failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
