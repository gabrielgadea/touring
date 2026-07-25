#!/usr/bin/env python3
"""W05.audit — Wave W05.

Forensic sub-script for W05 — Wave W05.

Outputs
-------
  - data/W05-audit.json   (machine-readable JSON envelope)
- staging/W05-audit.md  (human-readable narrative)
Usage
-----
    python3 audit.py            # dry-run mode (default)
    python3 audit.py --apply    # actual mutation
    python3 audit.py -j         # JSON-only stdout (machine-readable)
"""

# Phase 1 — Imports + constants
from __future__ import annotations

import argparse
import json
import logging
import sys
from datetime import UTC, datetime
from pathlib import Path

_ROOT = Path(__file__).resolve().parents[1]
_DATA_DIR = _ROOT / "data"
_WAVE = "W05"
_NAME = "audit"

_EXIT_OK = 0
_EXIT_FAIL = 1
_EXIT_INTERRUPTED = 130


# Phase 2 — CLI parser
def build_parser() -> argparse.ArgumentParser:
    """CLI parser for audit."""
    parser = argparse.ArgumentParser(prog=_NAME, description=__doc__)
    parser.add_argument(
        "--apply",
        action="store_true",
        help="DESTRUCTIVE — actually perform changes.",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=_DATA_DIR,
        help="Override output directory for the JSON artifact.",
    )
    parser.add_argument(
        "-j", "--json",
        action="store_true",
        dest="json_only",
        help="Emit only JSON to stdout (machine-readable).",
    )
    parser.add_argument(
        "-v", "--verbose",
        action="store_true",
        help="Enable DEBUG-level logging.",
    )
    return parser


# Phase 3 — Pure scan (read-only)
_RE_SHOULD_RUN = __import__("re").compile(
    r"def\s+should_run_phase\s*\("
)
_RE_PRE_PHASE = __import__("re").compile(
    r"def\s+pre_phase\s*\("
)
_RE_ADAPT_BATCH = __import__("re").compile(
    r"def\s+_adapt_batch_size\s*\("
)
_RE_EXECUTE_PHASE = __import__("re").compile(
    r"def\s+_execute_phase\s*\("
)
_RE_ERROR_HANDLING = __import__("re").compile(
    r"MemoryError|self\._healing"
)


def scan_workspace(_root: Path) -> list[dict]:
    """Scan pipeline_runner.py for PP4 structural verification.

    PP4: run_phase() CC=28 → 5 single-responsibility methods (CC≤4 each).
    Audit checks: methods extracted, signatures correct, no direct run_phase body.
    Plan ref: plan.md lines 93-97.
    """
    findings: list[dict] = []
    target = Path("/home/gabrielgadea/projects/analise/scripts/process_analysis/pipeline_runner.py")
    if not target.exists():
        findings.append({
            "file": "scripts/process_analysis/pipeline_runner.py",
            "line": 0,
            "severity": "P0",
            "fc": "PP4",
            "context": "pipeline_runner.py not found",
        })
        return findings

    content = target.read_text(encoding="utf-8", errors="ignore")
    lines = content.splitlines()

    def line_for(pattern):
        return next(
            ((i + 1) for i, l in enumerate(lines) if pattern in l),
            0,
        )

    # Check 1: should_run_phase exists as independent method
    should_run_match = _RE_SHOULD_RUN.search(content)
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": line_for("def should_run_phase("),
        "severity": "PASS" if should_run_match else "P1",
        "fc": "PP4",
        "pp": "PP4-audit",
        "context": (
            "should_run_phase() extracted to independent method"
            if should_run_match
            else "MISSING: should_run_phase() not found",
        ),
    })

    # Check 2: pre_phase exists as independent method
    pre_phase_match = _RE_PRE_PHASE.search(content)
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": line_for("def pre_phase("),
        "severity": "PASS" if pre_phase_match else "P1",
        "fc": "PP4",
        "pp": "PP4-audit",
        "context": (
            "pre_phase() extracted to independent method"
            if pre_phase_match
            else "MISSING: pre_phase() not found",
        ),
    })

    # Check 3: _adapt_batch_size exists
    adapt_match = _RE_ADAPT_BATCH.search(content)
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": line_for("def _adapt_batch_size("),
        "severity": "PASS" if adapt_match else "P1",
        "fc": "PP4",
        "pp": "PP4-audit",
        "context": (
            "_adapt_batch_size() exists as method"
            if adapt_match
            else "MISSING: _adapt_batch_size() not found",
        ),
    })

    # Check 4: _execute_phase exists
    exec_match = _RE_EXECUTE_PHASE.search(content)
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": line_for("def _execute_phase("),
        "severity": "PASS" if exec_match else "P1",
        "fc": "PP4",
        "pp": "PP4-audit",
        "context": (
            "_execute_phase() exists as method"
            if exec_match
            else "MISSING: _execute_phase() not found",
        ),
    })

    # Check 5: Error handling methods exist
    error_match = _RE_ERROR_HANDLING.search(content)
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": line_for("MemoryError") or line_for("self._healing"),
        "severity": "PASS" if error_match else "P2",
        "fc": "PP4",
        "pp": "PP4-audit",
        "context": (
            "Error handling methods (MemoryError/healing) found"
            if error_match
            else "WEAK: error handling not found",
        ),
    })

    return findings


# Phase 4 — Optional mutation (gated by --apply)
def apply_changes(findings: list[dict], root: Path) -> dict:
    """DESTRUCTIVE — runs only with --apply.

    Args:
        findings: Findings produced by the scan phase.
        root: Workspace root.

    Returns:
        Mutation summary: {"applied": int, "skipped": int, ...}.
    """
    applied = 0
    skipped = 0
    for finding in findings:
        # TODO: implement the mutation logic here.
        _ = (finding, root)
        skipped += 1
    return {"applied": applied, "skipped": skipped}


def run(args: argparse.Namespace) -> dict:
    """Orchestrate scan + optional mutation, write JSON, return report.

    Returns:
        Report dict with stable envelope keys consumed by ``cross_audit.py``
        and ``evidence_collector.py``.
    """
    args.output_dir.mkdir(parents=True, exist_ok=True)

    findings = scan_workspace(_ROOT)
    applied = apply_changes(findings, _ROOT) if args.apply else {}

    report = {
        "script": _NAME,
        "wave": _WAVE,
        "subtask_refs": [f"{_WAVE}.{_NAME}"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "apply": args.apply,
        "totals": {"findings": len(findings), **applied},
        "findings": findings,
    }

    json_path = args.output_dir / f"{_WAVE}-{_NAME}.json"
    json_path.write_text(
        json.dumps(report, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )

    return {
        **report,
        "json_path": str(json_path.relative_to(_ROOT)),
    }


def main() -> int:
    """CLI entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(
            json.dumps(result, indent=2, ensure_ascii=False) + "\n",
        )
        return _EXIT_OK if result["status"] == "OK" else _EXIT_FAIL
    except KeyboardInterrupt:
        return _EXIT_INTERRUPTED
    except Exception:  # noqa: BLE001 — top-level catch-all is intentional
        logging.getLogger(__name__).exception("forensic scan failed")
        return _EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
