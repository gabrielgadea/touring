#!/usr/bin/env python3
"""W13.audit — PP3 ProcessPoolExecutor (FC-2 fix).

Forensic sub-script for W13 — PP3: Replace ThreadPoolExecutor with
ProcessPoolExecutor for CPU-bound BlocoB phases.

FC-2: GIL-bound ThreadPoolExecutor limits CPU-bound phase parallelism.
PP3 plan ref: plan.md lines 252-273.

Evidence
--------
  - data/W13-audit.json   (machine-readable)
  - staging/W13-audit.md  (human-readable)

Usage
-----
    python3 audit.py            # dry-run mode (default)
    python3 audit.py --apply  # actual mutation (not implemented for audit)
    python3 audit.py -j       # JSON-only stdout
"""
from __future__ import annotations

import argparse
import json
import logging
import re
import sys
from datetime import UTC, datetime
from pathlib import Path

_ROOT = Path(__file__).resolve().parents[1]
_DATA_DIR = _ROOT / "data"
_WAVE = "W13"
_NAME = "audit"

_EXIT_OK = 0
_EXIT_FAIL = 1
_EXIT_INTERRUPTED = 130

# Regex patterns
_RE_AS_COMPLETED = re.compile(r"as_completed\s*\(")
_RE_EXECUTOR_SUBMIT = re.compile(r"executor\.submit\s*\(")

_TARGET_FILE = Path("/home/gabrielgadea/projects/analise/scripts/process_analysis/pipeline_runner.py")


def _find_def_lineno(content: str, pattern: str) -> int:
    """Return 1-based line number of first line matching pattern, or 0."""
    lines = content.splitlines()
    for i, line in enumerate(lines, start=1):
        if pattern in line:
            return i
    return 0


def _is_nested_def(content: str, def_pattern: str) -> bool:
    """Check if def name is nested inside another function.

    Walks backwards from the def line to find the previous non-blank,
    non-comment line. If its indentation is less, it's at module level.
    """
    lines = content.splitlines()
    for i, line in enumerate(lines):
        if def_pattern in line:
            # Walk backwards to find previous non-blank/comment line
            for j in range(i - 1, -1, -1):
                prev = lines[j].rstrip()
                if not prev or prev.startswith("#"):
                    continue
                indent = len(lines[j]) - len(lines[j].lstrip())
                def_indent = len(lines[i]) - len(lines[i].lstrip())
                return indent <= def_indent
            # No previous line = module level
            return False
    return False  # not found


def scan_workspace(_root: Path) -> list[dict]:
    """Scan pipeline_runner.py for PP3: ProcessPoolExecutor implementation.
    PP3 requires:
    1. _run_parallel_phase_group uses ProcessPoolExecutor (not ThreadPoolExecutor)
    2. _run_one is a top-level function (not nested — required for pickling)
    3. executor.submit pattern preserved
    4. as_completed pattern preserved
    """
    findings: list[dict] = []

    if not _TARGET_FILE.exists():
        findings.append({
            "file": "scripts/process_analysis/pipeline_runner.py",
            "line": 0,
            "severity": "P0",
            "fc": "FC-2",
            "context": "pipeline_runner.py not found at expected path",
        })
        return findings

    content = _TARGET_FILE.read_text(encoding="utf-8", errors="ignore")
    lines = content.splitlines()

    def line_for(pattern):
        return next(
            ((i + 1) for i, l in enumerate(lines) if pattern in l),
            0,
        )

    # Check 1: ProcessPoolExecutor used (not ThreadPoolExecutor)
    pp_lineno = _find_def_lineno(content, "ProcessPoolExecutor(")
    tp_lineno = _find_def_lineno(content, "ThreadPoolExecutor(")
    has_pp = bool(pp_lineno)
    has_tp = bool(tp_lineno)
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": pp_lineno or tp_lineno,
        "severity": "PASS" if has_pp and not has_tp else "P0",
        "fc": "FC-2",
        "pp": "PP3",
        "context": (
            f"ProcessPoolExecutor found at line {pp_lineno}"
            if has_pp
            else "MISSING: ProcessPoolExecutor — still using ThreadPoolExecutor"
        ),
    })

    # Check 2: _run_one is at module level (not nested)
    # A nested _run_one cannot be pickled for ProcessPoolExecutor
    run_one_nested = _is_nested_def(content, "def _run_one")
    run_one_lineno = _find_def_lineno(content, "def _run_one(")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": run_one_lineno,
        "severity": "PASS" if run_one_lineno and not run_one_nested else "P0",
        "fc": "FC-2",
        "pp": "PP3",
        "context": (
            f"_run_one at module level (line {run_one_lineno}) — pickle-safe for ProcessPoolExecutor"
            if run_one_lineno and not run_one_nested
            else f"_run_one is nested (line {run_one_lineno}) — NOT pickle-safe"
        ),
    })

    # Check 3: executor.submit preserved
    has_submit = bool(_RE_EXECUTOR_SUBMIT.search(content))
    submit_lineno = line_for("executor.submit")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": submit_lineno,
        "severity": "PASS" if has_submit else "P1",
        "fc": "FC-2",
        "pp": "PP3",
        "context": (
            "executor.submit pattern found"
            if has_submit
            else "MISSING: executor.submit pattern"
        ),
    })

    # Check 4: as_completed preserved
    has_as_completed = bool(_RE_AS_COMPLETED.search(content))
    ac_lineno = line_for("as_completed")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": ac_lineno,
        "severity": "PASS" if has_as_completed else "P1",
        "fc": "FC-2",
        "pp": "PP3",
        "context": (
            "concurrent.futures.as_completed pattern found"
            if has_as_completed
            else "MISSING: as_completed pattern"
        ),
    })

    return findings


def apply_changes(findings: list[dict], _root: Path) -> dict:
    """No-op — audit only."""
    applied = 0
    skipped = len(findings)
    return {"applied": applied, "skipped": skipped}


def run(args: argparse.Namespace) -> dict:
    args.output_dir = args.output_dir.resolve()
    args.output_dir.mkdir(parents=True, exist_ok=True)

    findings = scan_workspace(_ROOT)
    applied = apply_changes(findings, _ROOT) if args.apply else {}

    report = {
        "script": _NAME,
        "wave": _WAVE,
        "fc": ["FC-2"],
        "pp": ["PP3"],
        "subtask_refs": [f"{_WAVE}.{_NAME}"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "P0" if any(f["severity"] == "P0" for f in findings) else "OK",
        "apply": args.apply,
        "totals": {"findings": len(findings), **applied},
        "findings": findings,
        "evidence": {
            "plan_ref": "plan.md (PP3 — ProcessPoolExecutor, lines 252-273)",
            "target": "scripts/process_analysis/pipeline_runner.py",
            "confidence": "FACT [1.0]",
        },
    }

    json_path = args.output_dir / f"{_WAVE}-{_NAME}.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    return {**report, "json_path": str(json_path.relative_to(_ROOT))}


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog=_NAME, description=__doc__)
    parser.add_argument("--apply", action="store_true", help="DESTRUCTIVE — not implemented for audit.")
    parser.add_argument("--output-dir", type=Path, default=_DATA_DIR, help="Override output directory.")
    parser.add_argument("-j", "--json", action="store_true", dest="json_only", help="JSON to stdout.")
    parser.add_argument("-v", "--verbose", action="store_true", help="DEBUG-level logging.")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
        return _EXIT_OK if result["status"] == "OK" else _EXIT_FAIL
    except KeyboardInterrupt:
        return _EXIT_INTERRUPTED
    except Exception:
        logging.getLogger(__name__).exception("forensic scan failed")
        return _EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
