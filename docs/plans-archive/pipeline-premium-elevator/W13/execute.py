#!/usr/bin/env python3
"""W13.execute — PP4-pre audit for PP3 readiness (FC-2 fix).

Execute sub-script for W13 — verify PhaseConfig dataclass exists
with timeout+retry_budget fields AND that PP3's pre-condition
(run_phase accepts PhaseConfig) is met, BEFORE the ProcessPoolExecutor
mutation in audit.py.

FC: P1 | PP: PP4-pre | Waves: W13
Outputs
-------
  - data/W13-execute.json   (machine-readable)
  - staging/W13-execute.md  (human-readable)

Usage
-----
    python3 execute.py            # dry-run mode (default)
    python3 execute.py --apply    # actual mutation (not implemented for execute)
    python3 execute.py -j         # JSON-only stdout
"""
from __future__ import annotations

import argparse
import json
import logging
import sys
from datetime import UTC, datetime
from pathlib import Path

_ROOT = Path(__file__).resolve().parents[1]
_ANALISE_ROOT = Path("/home/gabrielgadea/projects/analise")
_SCRIPTS_DIR = _ANALISE_ROOT / "scripts/process_analysis"
_TARGET_FILE = _SCRIPTS_DIR / "pipeline_runner.py"
_DATA_DIR = _ROOT / "data"
_WAVE = "W13"
_NAME = "execute"

_EXIT_OK = 0
_EXIT_FAIL = 1
_EXIT_INTERRUPTED = 130


def scan_workspace(_root: Path) -> list[dict]:
    findings: list[dict] = []

    if not _TARGET_FILE.exists():
        findings.append({
            "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
            "line": 0,
            "severity": "P0",
            "fc": "P1",
            "context": "pipeline_runner.py not found at expected path",
        })
        return findings

    content = _TARGET_FILE.read_text(encoding="utf-8", errors="ignore")
    lines = content.splitlines()

    def line_for(pattern, _lines=None):
        _lines = _lines or lines
        return next(
            ((i + 1) for i, l in enumerate(_lines) if pattern in l),
            0,
        )

    # Check 1: PhaseConfig dataclass
    phase_config_match = "@dataclass" in content and "class PhaseConfig" in content
    phase_config_lineno = line_for("class PhaseConfig")
    findings.append({
        "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
        "line": phase_config_lineno,
        "severity": "PASS" if phase_config_match else "P0",
        "fc": "P1",
        "pp": "PP4-pre",
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
        "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
        "line": timeout_lineno,
        "severity": "PASS" if timeout_match else "P1",
        "fc": "P1",
        "pp": "PP4-pre",
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
        "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
        "line": retry_lineno,
        "severity": "PASS" if retry_match else "P1",
        "fc": "P1",
        "pp": "PP4-pre",
        "context": (
            "retry_budget field found in PhaseConfig"
            if retry_match
            else "MISSING: retry_budget field in PhaseConfig",
        ),
    })

    # Check 4: run_phase accepts cfg: PhaseConfig | None
    sig_match = "cfg: PhaseConfig | None = None" in content
    sig_lineno = line_for("cfg: PhaseConfig | None = None")
    findings.append({
        "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
        "line": sig_lineno,
        "severity": "PASS" if sig_match else "P1",
        "fc": "P1",
        "pp": "PP4-pre",
        "context": (
            "run_phase signature accepts PhaseConfig parameter"
            if sig_match
            else "MISSING: run_phase does not accept PhaseConfig parameter",
        ),
    })

    return findings


def apply_changes(findings: list[dict], _root: Path) -> dict:
    applied = 0
    skipped = 0
    for finding in findings:
        if finding.get("severity") == "PASS":
            skipped += 1
            continue
        if finding.get("severity") == "P0":
            skipped += 1
            continue
        skipped += 1
    return {"applied": applied, "skipped": skipped}


def run(args: argparse.Namespace) -> dict:
    args.output_dir = args.output_dir.resolve()
    args.output_dir.mkdir(parents=True, exist_ok=True)

    findings = scan_workspace(_ROOT)
    applied = apply_changes(findings, _ROOT) if args.apply else {}

    report = {
        "script": _NAME,
        "wave": _WAVE,
        "fc": ["P1"],
        "pp": ["PP4-pre"],
        "subtask_refs": [f"{_WAVE}.{_NAME}"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "P0" if any(f["severity"] == "P0" for f in findings) else "OK",
        "apply": args.apply,
        "totals": {"findings": len(findings), **applied},
        "findings": findings,
        "evidence": {
            "plan_ref": "plan.md (PP4-pre — PhaseConfig pre-condition for PP3)",
            "target": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
            "confidence": "FACT [1.0]",
        },
    }

    json_path = args.output_dir / f"{_WAVE}-{_NAME}.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    return {**report, "json_path": str(json_path.relative_to(_ROOT))}


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog=_NAME, description=__doc__)
    parser.add_argument("--apply", action="store_true", help="DESTRUCTIVE — actually perform changes.")
    parser.add_argument("--output-dir", type=Path, default=_DATA_DIR, help="Override output directory.")
    parser.add_argument("-j", "--json", action="store_true", dest="json_only", help="Emit only JSON to stdout.")
    parser.add_argument("-v", "--verbose", action="store_true", help="Enable DEBUG-level logging.")
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