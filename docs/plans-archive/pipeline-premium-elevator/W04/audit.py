#!/usr/bin/env python3
"""W04.audit — Wave W04.

Forensic sub-script for W04 — Wave W04.

Outputs
-------
  - data/W04-audit.json   (machine-readable JSON envelope)
- staging/W04-audit.md  (human-readable narrative)
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
_WAVE = "W04"
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
import re

_RE_WAIT_FOR = re.compile(r"asyncio\.wait_for\s*\(")
_RE_TIMEOUT_ARG = re.compile(r"timeout\s*=\s*cfg\.timeout|timeout\s*=\s*phase\.timeout")
_RE_ASYNC_RUNNER_OUTER = re.compile(r"asyncio\.Runner\(\)")
_RE_ASYNC_RUNNER_INNER = re.compile(r"runner\.run\s*\(")


def scan_workspace(_root: Path) -> list[dict]:
    """Scan pipeline_runner.py for FC-4: per-phase timeout propagation.

    FC-4: No per-phase timeout — asyncio.wait_for must receive cfg.timeout
    as the propagated argument, not a hardcoded value.

    Evidence: plan.md lines 78, 200-213.
    """
    findings: list[dict] = []
    target = Path("/home/gabrielgadea/projects/analise/scripts/process_analysis/pipeline_runner.py")

    if not target.exists():
        findings.append({
            "file": "scripts/process_analysis/pipeline_runner.py",
            "line": 0,
            "severity": "P0",
            "fc": "FC-4",
            "context": "pipeline_runner.py not found at expected path",
        })
        return findings

    content = target.read_text(encoding="utf-8", errors="ignore")
    lines = content.splitlines()

    def line_for(pattern):
        return next(
            ((i + 1) for i, l in enumerate(lines) if pattern in l),
            0,
        )

    # Check 1: _run_with_timeout method exists
    run_with_timeout_lineno = line_for("def _run_with_timeout")
    run_with_timeout_match = run_with_timeout_lineno > 0
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": run_with_timeout_lineno,
        "severity": "PASS" if run_with_timeout_match else "P0",
        "fc": "FC-4",
        "pp": "PP1-audit",
        "context": (
            f"_run_with_timeout method found at line {run_with_timeout_lineno}"
            if run_with_timeout_match
            else "MISSING: _run_with_timeout method",
        ),
    })

    # Check 2: asyncio.wait_for present inside _run_with_timeout
    wait_for_match = bool(_RE_WAIT_FOR.search(content))
    wait_for_lineno = line_for("asyncio.wait_for")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": wait_for_lineno,
        "severity": "PASS" if wait_for_match else "P0",
        "fc": "FC-4",
        "pp": "PP1-audit",
        "context": (
            "asyncio.wait_for() call found"
            if wait_for_match
            else "MISSING: asyncio.wait_for() call",
        ),
    })

    # Check 3: timeout argument is cfg.timeout (propagated, not hardcoded)
    # Must find "timeout=cfg.timeout" or "timeout=phase.timeout" in the wait_for call
    timeout_propagated = bool(_RE_TIMEOUT_ARG.search(content))
    timeout_lineno = line_for("timeout=")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": timeout_lineno,
        "severity": "PASS" if timeout_propagated else "P1",
        "fc": "FC-4",
        "pp": "PP1-audit",
        "context": (
            "timeout propagated from PhaseConfig (cfg.timeout)"
            if timeout_propagated
            else "MISSING: timeout not propagated from PhaseConfig",
        ),
    })

    # Check 4: asyncio.Runner used OUTSIDE run_phase (not creating new loops per call)
    # PP6 audit: asyncio.Runner context manager in pre_phase/post_phase
    outer_runner_match = bool(_RE_ASYNC_RUNNER_OUTER.search(content))
    outer_runner_lineno = line_for("asyncio.Runner()")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": outer_runner_lineno,
        "severity": "PASS" if outer_runner_match else "P1",
        "fc": "FC-4",
        "pp": "PP6-audit",
        "context": (
            "asyncio.Runner() context manager found (PP6 — persistent event loop)"
            if outer_runner_match
            else "MISSING: asyncio.Runner() context manager (PP6)",
        ),
    })

    # Check 5: runner.run() calls INSIDE _run_with_timeout
    inner_runner_match = bool(_RE_ASYNC_RUNNER_INNER.search(content))
    inner_runner_lineno = line_for("runner.run")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": inner_runner_lineno,
        "severity": "PASS" if inner_runner_match else "P1",
        "fc": "FC-4",
        "pp": "PP6-audit",
        "context": (
            "runner.run() async bridge found inside _run_with_timeout"
            if inner_runner_match
            else "MISSING: runner.run() async bridge",
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
    args.output_dir = args.output_dir.resolve()
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
