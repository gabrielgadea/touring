#!/usr/bin/env python3
"""W04.execute — Wave W04.

Forensic sub-script for W04 — Wave W04.

Outputs
-------
  - data/W04-execute.json   (machine-readable JSON envelope)
- staging/W04-execute.md  (human-readable narrative)
Usage
-----
    python3 execute.py            # dry-run mode (default)
    python3 execute.py --apply    # actual mutation
    python3 execute.py -j         # JSON-only stdout (machine-readable)
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
_NAME = "execute"

_EXIT_OK = 0
_EXIT_FAIL = 1
_EXIT_INTERRUPTED = 130


# Phase 2 — CLI parser
def build_parser() -> argparse.ArgumentParser:
    """CLI parser for execute."""
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

_RE_WAIT_FOR_TIMEOUT = re.compile(r"timeout\s*=\s*cfg\.timeout")


def scan_workspace(_root: Path) -> list[dict]:
    """Scan pipeline_runner.py for PP1/FC-4: per-phase timeout + retry budget implementation.

    PP1: Per-phase timeout + retry budget — FC-4 fix.
    Plan ref: plan.md lines 178-227.
    Verifies: PhaseConfig has timeout+retry_budget, run_phase passes cfg.timeout
    to _run_with_timeout, _is_transient helper exists.
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

    # Check 1: PhaseConfig dataclass
    phase_config_match = "@dataclass" in content and "class PhaseConfig" in content
    phase_config_lineno = line_for("class PhaseConfig")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": phase_config_lineno,
        "severity": "PASS" if phase_config_match else "P0",
        "fc": "FC-4",
        "pp": "PP1",
        "context": (
            f"PhaseConfig dataclass found at line {phase_config_lineno}"
            if phase_config_match
            else "MISSING: PhaseConfig dataclass not found",
        ),
    })

    # Check 2: timeout field in PhaseConfig
    timeout_match = "timeout: float" in content
    timeout_lineno = line_for("timeout: float")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": timeout_lineno,
        "severity": "PASS" if timeout_match else "P1",
        "fc": "FC-4",
        "pp": "PP1",
        "context": (
            "timeout field found in PhaseConfig"
            if timeout_match
            else "MISSING: timeout field in PhaseConfig",
        ),
    })

    # Check 3: retry_budget field
    retry_match = "retry_budget: int" in content
    retry_lineno = line_for("retry_budget: int")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": retry_lineno,
        "severity": "PASS" if retry_match else "P1",
        "fc": "FC-4",
        "pp": "PP1",
        "context": (
            "retry_budget field found in PhaseConfig"
            if retry_match
            else "MISSING: retry_budget field in PhaseConfig"
        ),
    })

    # Check 4: run_phase accepts cfg: PhaseConfig | None
    sig_match = "cfg: PhaseConfig | None = None" in content or "cfg: Optional[PhaseConfig]" in content
    sig_lineno = line_for("cfg: PhaseConfig | None")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": sig_lineno,
        "severity": "PASS" if sig_match else "P1",
        "fc": "FC-4",
        "pp": "PP1",
        "context": (
            "run_phase signature accepts PhaseConfig parameter"
            if sig_match
            else "MISSING: run_phase does not accept PhaseConfig parameter",
        ),
    })

    # Check 5: _is_transient helper method
    transient_match = "def _is_transient" in content
    transient_lineno = line_for("def _is_transient")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": transient_lineno,
        "severity": "PASS" if transient_match else "P1",
        "fc": "FC-4",
        "pp": "PP1",
        "context": (
            "_is_transient helper method found"
            if transient_match
            else "MISSING: _is_transient helper method",
        ),
    })

    # Check 6: retry logic uses cfg.retry_budget > 0
    retry_logic = "cfg.retry_budget > 0" in content
    retry_logic_lineno = line_for("cfg.retry_budget > 0")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": retry_logic_lineno,
        "severity": "PASS" if retry_logic else "P1",
        "fc": "FC-4",
        "pp": "PP1",
        "context": (
            "Retry logic with PhaseConfig.retry_budget found"
            if retry_logic
            else "MISSING: retry logic in run_phase",
        ),
    })

    # Check 7: asyncio.wait_for receives cfg.timeout (not hardcoded)
    wait_for_timeout = bool(_RE_WAIT_FOR_TIMEOUT.search(content))
    wait_for_timeout_lineno = line_for("timeout=")
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": wait_for_timeout_lineno,
        "severity": "PASS" if wait_for_timeout else "P1",
        "fc": "FC-4",
        "pp": "PP1",
        "context": (
            "asyncio.wait_for receives cfg.timeout (timeout propagated)"
            if wait_for_timeout
            else "MISSING: asyncio.wait_for timeout not propagated from PhaseConfig"
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
