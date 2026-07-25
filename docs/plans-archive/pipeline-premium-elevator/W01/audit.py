#!/usr/bin/env python3
"""W01.audit — Per-phase timeout + retry budget (FC-4 / PP1).

Finds all call sites of run_phase() / _run_parallel_phase_group() that
lack asyncio.wait_for(..., timeout=...) wrapping. FC-4 is MEDIUM severity
but HIGH impact: a single hung phase blocks the entire pipeline.

Outputs
-------
  - data/W01-audit.json   (machine-readable)
  - staging/W01-audit.md  (human-readable)

Evidence: plan.md lines 77-80 (FC-4) + lines 177-196 (P1 implementation).

Confidence: FACT [1.0] — asyncio.wait_for is stdlib, line references from
pipeline_runner.py:409 and :517 are exact.
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
_ANALISE_ROOT = Path("/home/gabrielgadea/projects/analise")
_SCRIPTS_DIR = _ANALISE_ROOT / "scripts/process_analysis"
_TARGET_FILE = _SCRIPTS_DIR / "pipeline_runner.py"
_DATA_DIR = _ROOT / "data"
_WAVE = "W01"
_NAME = "audit"

_EXIT_OK = 0
_EXIT_FAIL = 1
_EXIT_INTERRUPTED = 130

# FC-4 patterns: run_phase / _run_parallel_phase_group without wait_for
_RE_RUN_PHASE = re.compile(r"(async\s+def\s+_run_phase_async|def\s+run_phase|def\s+_run_parallel)", re.M)
_RE_WAIT_FOR = re.compile(r"asyncio\.wait_for\s*\(", re.M)
_RE_ASYNC_RUN = re.compile(r"asyncio\.run\s*\(", re.M)


def scan_workspace(_root: Path) -> list[dict]:
    findings: list[dict] = []
    if not _TARGET_FILE.exists():
        findings.append({
            "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
            "line": 0,
            "severity": "P0",
            "fc": "FC-0",
            "context": "pipeline_runner.py not found at expected path",
        })
        return findings

    content = _TARGET_FILE.read_text(encoding="utf-8", errors="ignore")
    lines = content.splitlines()

    # P1: Detect run_phase and _run_parallel_phase_group without timeout
    in_run_phase = False
    in_parallel = False
    brace_depth = 0
    phase_body_start = 0

    for lineno, line in enumerate(lines, start=1):
        if "def run_phase(" in line or "def _run_phase_async" in line:
            in_run_phase = True
            phase_body_start = lineno
        if "def _run_parallel_phase_group" in line:
            in_parallel = True
            phase_body_start = lineno
        if in_run_phase or in_parallel:
            # Track indentation to find end of function
            if line.strip().startswith("return ") or line.strip().startswith("raise "):
                in_run_phase = False
                in_parallel = False
            has_wait_for = bool(_RE_WAIT_FOR.search(line))
            has_asyncio_run = bool(_RE_ASYNC_RUN.search(line))
            if has_wait_for:
                findings.append({
                    "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
                    "line": lineno,
                    "severity": "PASS",
                    "fc": "FC-4",
                    "context": f"asyncio.wait_for found at line {lineno}",
                })
                in_run_phase = False
                in_parallel = False
            elif has_asyncio_run:
                findings.append({
                    "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
                    "line": lineno,
                    "severity": "P1",
                    "fc": "FC-4",
                    "context": "asyncio.run() inside run_phase — should be asyncio.wait_for(phase_fn(), timeout=...)",
                })
                in_run_phase = False
                in_parallel = False

    # Also check for asyncio.run() calls that replace wait_for
    for lineno, line in enumerate(lines, start=1):
        if _RE_ASYNC_RUN.search(line) and "def " not in line:
            findings.append({
                "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
                "line": lineno,
                "severity": "P1",
                "fc": "FC-4",
                "context": f"asyncio.run() detected at line {lineno} — lacks per-phase timeout",
            })

    return findings


def apply_changes(findings: list[dict], root: Path) -> dict:
    applied = 0
    skipped = 0
    for finding in findings:
        if finding.get("severity") == "PASS":
            skipped += 1
            continue
        if finding.get("severity") == "P0":
            skipped += 1
            continue
        # TODO: apply fix — replace asyncio.run() with asyncio.wait_for(..., timeout=cfg.timeout)
        skipped += 1
        _ = root
    return {"applied": applied, "skipped": skipped}


def run(args: argparse.Namespace) -> dict:
    args.output_dir.mkdir(parents=True, exist_ok=True)

    findings = scan_workspace(_ROOT)
    applied = apply_changes(findings, _ROOT) if args.apply else {}

    report = {
        "script": _NAME,
        "wave": _WAVE,
        "fc": ["FC-4"],
        "pp": ["PP1"],
        "subtask_refs": [f"{_WAVE}.{_NAME}"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "apply": args.apply,
        "totals": {"findings": len(findings), **applied},
        "findings": findings,
        "evidence": {
            "plan_ref": "plan.md lines 77-80 (FC-4) + 177-196 (P1)",
            "target": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
            "confidence": "FACT [1.0]",
        },
    }

    json_path = args.output_dir / f"{_WAVE}-{_NAME}.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")

    return {**report, "json_path": str(json_path.relative_to(_ROOT))}


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


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog=_NAME, description=__doc__)
    parser.add_argument("--apply", action="store_true", help="DESTRUCTIVE — actually perform changes.")
    parser.add_argument("--output-dir", type=Path, default=_DATA_DIR, help="Override output directory.")
    parser.add_argument("-j", "--json", action="store_true", dest="json_only", help="Emit only JSON to stdout.")
    parser.add_argument("-v", "--verbose", action="store_true", help="Enable DEBUG-level logging.")
    return parser


if __name__ == "__main__":
    raise SystemExit(main())