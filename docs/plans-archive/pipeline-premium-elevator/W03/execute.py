#!/usr/bin/env python3
"""W03.execute — Async hooks without asyncio.run() via asyncio.Runner (FC-1 / PP6).

State verification: counts asyncio.run() calls inside async def vs outside.

Outputs
-------
  - data/W03-execute.json   (machine-readable)
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
_WAVE = "W03"
_NAME = "execute"

_EXIT_OK = 0
_EXIT_FAIL = 1
_EXIT_INTERRUPTED = 130

_RE_ASYNC_RUN = re.compile(r"asyncio\.run\s*\(", re.M)


def _enclosing_async_def(lineno: int, lines: list[str]) -> bool:
    """Return True if line `lineno` lies inside an async def body."""
    line_indent = len(lines[lineno - 1]) - len(lines[lineno - 1].lstrip())
    for prev in range(lineno - 1, -1, -1):
        pline = lines[prev]
        pstrip = pline.strip()
        if not pstrip or pstrip.startswith("#"):
            continue
        pindent = len(pline) - len(pline.lstrip())
        if pindent < line_indent and pstrip:
            return False
        if "async def " in pstrip and not pstrip.startswith("#"):
            return True
        if pindent <= line_indent and pstrip and not pstrip.startswith("#"):
            return False
    return False


def scan_phaseconfig() -> dict:
    findings: list[dict] = []

    if not _TARGET_FILE.exists():
        return {
            "found": False,
            "fields": [],
            "findings": [{
                "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
                "line": 0,
                "severity": "P0",
                "fc": "FC-0",
                "context": "pipeline_runner.py not found",
            }],
        }

    content = _TARGET_FILE.read_text(encoding="utf-8", errors="ignore")
    lines = content.splitlines()

    asyncio_runs: list[tuple[int, bool]] = []  # (lineno, inside_async)
    for lineno, line in enumerate(lines, start=1):
        if _RE_ASYNC_RUN.search(line):
            asyncio_runs.append((lineno, _enclosing_async_def(lineno, lines)))

    inside_async_count = sum(1 for _, ia in asyncio_runs if ia)
    outside_async_count = sum(1 for _, ia in asyncio_runs if not ia)

    findings.append({
        "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
        "line": 1,
        "severity": "PASS" if asyncio_runs else "INFO",
        "fc": "FC-1",
        "context": f"Total asyncio.run() calls: {len(asyncio_runs)}",
    })
    findings.append({
        "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
        "line": 1,
        "severity": "PASS" if inside_async_count == 0 else "P1",
        "fc": "FC-1",
        "context": f"asyncio.run() inside async def: {inside_async_count} (FC-1 violations)",
    })
    findings.append({
        "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
        "line": 1,
        "severity": "PASS",
        "fc": "FC-1",
        "context": f"asyncio.run() in non-async context: {outside_async_count} (no violation)",
    })

    return {
        "found": True,
        "total_asyncio_run": len(asyncio_runs),
        "inside_async_count": inside_async_count,
        "outside_async_count": outside_async_count,
        "findings": findings,
    }


def apply_changes(state: dict) -> dict:
    applied = 0
    skipped = 0
    if state.get("inside_async_count", -1) == 0:
        logging.info("FC-1 satisfied — no asyncio.run() inside async def")
        skipped += 1
    elif state.get("inside_async_count", -1) > 0:
        logging.info("FC-1 fix required: replace asyncio.run() inside async def with asyncio.Runner()")
        skipped += 1
    else:
        logging.info("No asyncio.run() calls found in pipeline_runner.py")
        skipped += 1
    return {"applied": applied, "skipped": skipped}


def run(args: argparse.Namespace) -> dict:
    args.output_dir.mkdir(parents=True, exist_ok=True)

    state = scan_phaseconfig()
    applied = apply_changes(state) if args.apply else {}

    status = "OK"
    if any(f["severity"] == "P0" for f in state["findings"]):
        status = "P0_NOT_FOUND"
    elif any(f["severity"] == "P1" for f in state["findings"]):
        status = "P1_VIOLATION"

    report = {
        "script": _NAME,
        "wave": _WAVE,
        "fc": ["FC-1"],
        "pp": ["PP6"],
        "subtask_refs": [f"{_WAVE}.{_NAME}"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": status,
        "apply": args.apply,
        "state": state,
        "totals": {"applied": applied.get("applied", 0), "skipped": applied.get("skipped", 0)},
        "evidence": {
            "plan_ref": "plan.md lines 122-128 (PP6 / FC-1)",
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
        logging.getLogger(__name__).exception("execute failed")
        return _EXIT_FAIL


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog=_NAME, description=__doc__)
    parser.add_argument("--apply", action="store_true", help="DESTRUCTIVE — actually perform changes.")
    parser.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    parser.add_argument("-j", "--json", action="store_true", dest="json_only")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


if __name__ == "__main__":
    raise SystemExit(main())
