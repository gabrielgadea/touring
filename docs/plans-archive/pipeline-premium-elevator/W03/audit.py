#!/usr/bin/env python3
"""W03.audit — Async hooks without asyncio.Runner() (FC-1 / PP6).

FC-1 HIGH severity: asyncio.run() inside NON-async hook invocation points.
Two target sites in pipeline_runner.py:
  - L409: hook_result = asyncio.run(hook.pre_phase(self._taco_state, self.deps))
  - L517: post_result = asyncio.run(hook.post_phase(self._taco_state, result))
Both are inside regular def functions (not async def) — FC-1 applies here.

Evidence: plan.md lines 122-128 (PP6 / FC-1).
Confidence: FACT [1.0] — line references are exact from pipeline_runner.py.
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
_NAME = "audit"

_EXIT_OK = 0
_EXIT_FAIL = 1
_EXIT_INTERRUPTED = 130

# Target lines per spec
_TARGET_LINES = {409, 517}

# FC-1 pattern: asyncio.run()
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

    # Find all asyncio.run() call sites
    for lineno, line in enumerate(lines, start=1):
        if not _RE_ASYNC_RUN.search(line):
            continue

        is_target = lineno in _TARGET_LINES
        is_inside_async = _enclosing_async_def(lineno, lines)

        if is_target and not is_inside_async:
            # FC-1 HIGH severity for target lines in non-async context
            findings.append({
                "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
                "line": lineno,
                "severity": "HIGH",
                "fc": "FC-1",
                "context": (
                    f"FC-1 HIGH: asyncio.run() at line {lineno} in non-async context. "
                    "Fix: replace with asyncio.Runner() to save ~1-3ms per hook call."
                ),
            })
        elif is_target and is_inside_async:
            findings.append({
                "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
                "line": lineno,
                "severity": "HIGH",
                "fc": "FC-1",
                "context": (
                    f"FC-1 HIGH: asyncio.run() at line {lineno} inside async def. "
                    "Creates nested event loop. Fix: inline await with shared Runner."
                ),
            })
        elif is_inside_async:
            findings.append({
                "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
                "line": lineno,
                "severity": "MEDIUM",
                "fc": "FC-1",
                "context": f"FC-1 MEDIUM: asyncio.run() inside async def at line {lineno}",
            })
        else:
            findings.append({
                "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
                "line": lineno,
                "severity": "INFO",
                "fc": "FC-1",
                "context": f"FC-1 INFO: asyncio.run() at line {lineno} — not a target site",
            })

    return findings


def apply_changes(findings: list[dict], root: Path) -> dict:
    """No --apply fix implemented — Runner API requires significant refactor (per spec)."""
    applied = 0
    skipped = len(findings)
    _ = root
    return {"applied": applied, "skipped": skipped}


def run(args: argparse.Namespace) -> dict:
    args.output_dir.mkdir(parents=True, exist_ok=True)

    findings = scan_workspace(_ROOT)
    applied = apply_changes(findings, _ROOT) if args.apply else {}

    status = "P0" if any(f["severity"] == "P0" for f in findings) else "OK"

    report = {
        "script": _NAME,
        "wave": _WAVE,
        "fc": ["FC-1"],
        "pp": ["PP6"],
        "subtask_refs": [f"{_WAVE}.{_NAME}"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": status,
        "apply": args.apply,
        "totals": {"findings": len(findings), **applied},
        "findings": findings,
        "evidence": {
            "plan_ref": "plan.md lines 122-128 (PP6 / FC-1)",
            "target": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
            "target_lines": sorted(_TARGET_LINES),
            "confidence": "FACT [1.0]",
        },
    }

    json_path = args.output_dir / f"{_WAVE}-{_NAME}.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")

    return {"json_path": str(json_path.relative_to(_ROOT)), **report}


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
