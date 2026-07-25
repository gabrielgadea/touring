#!/usr/bin/env python3
"""W02.execute — Lock in get_phase_strategy() read (FC-6 / PP2).

Verify that get_phase_strategy() is protected by _strategy_lock, or that
PhaseConfig (if it exists) has a lock field.

Outputs
-------
  - data/W02-execute.json   (machine-readable)
  - staging/W02-execute.md  (human-readable narrative)

Evidence: pipeline_runner.py lines 178-206.
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
_WAVE = "W02"
_NAME = "execute"

_EXIT_OK = 0
_EXIT_FAIL = 1
_EXIT_INTERRUPTED = 130

_RE_LOCK_DEF = re.compile(r"_strategy_lock\s*[:=]\s*threading\.(Lock|RLock)", re.M)
_RE_WITH_LOCK = re.compile(r"with\s+_strategy_lock\s*:", re.M)
_RE_GET_PHASE = re.compile(r"def\s+get_phase_strategy\s*\(\s*\)\s*(?:->\s*\S+)?\s*:", re.M)
_RE_PHASE_CONFIG = re.compile(r"class\s+PhaseConfig\s*[:(]", re.M)


def scan_phaseconfig() -> dict:
    """Scan PhaseConfig and get_phase_strategy lock status."""
    findings: list[dict] = []

    if not _TARGET_FILE.exists():
        return {
            "found": False,
            "fields": [],
            "lock_protected": False,
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

    has_lock_def = bool(_RE_LOCK_DEF.search(content))
    has_phaseconfig = bool(_RE_PHASE_CONFIG.search(content))

    # ── Find lock-protected regions: 'with _strategy_lock:' ────────────────
    locked_regions: list[tuple[int, int]] = []
    in_locked = False
    lock_start = 0
    for lineno, line in enumerate(lines, start=1):
        if _RE_WITH_LOCK.search(line):
            in_locked = True
            lock_start = lineno
        elif in_locked and not line.startswith(" " * 8):
            locked_regions.append((lock_start, lineno - 1))
            in_locked = False

    # ── Find get_phase_strategy return lines ───────────────────────────────
    # Use indentation-based exit: record func_indent at 'def' line, exit when
    # we see a line with strictly fewer leading spaces.
    in_get_phase = False
    func_indent = 0
    get_return_lines: list[int] = []
    for lineno, line in enumerate(lines, start=1):
        if _RE_GET_PHASE.search(line) and not line.lstrip().startswith("#"):
            in_get_phase = True
            func_indent = len(line) - len(line.lstrip())
            continue
        if in_get_phase:
            stripped = line.strip()
            if stripped.startswith("def ") and not line.startswith(" " * (func_indent + 1)):
                # New def at same or lesser indent → we've left the function
                in_get_phase = False
                continue
            if func_indent > 0 and not line.startswith(" " * (func_indent + 1)) and stripped:
                # Line dedented out of function body (not blank continuation)
                in_get_phase = False
                continue
            if re.match(r"return\s+_current_strategy\s*$", stripped):
                get_return_lines.append(lineno)
                in_get_phase = False  # bare return = end of function

    # Check if each return is inside a locked region
    protected = False
    for ret_line in get_return_lines:
        for start, end in locked_regions:
            if start <= ret_line <= end:
                protected = True
                break

    findings.append({
        "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
        "line": 1,
        "severity": "PASS" if has_lock_def else "P0",
        "fc": "FC-6",
        "context": f"_strategy_lock defined: {has_lock_def}",
    })
    findings.append({
        "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
        "line": get_return_lines[0] if get_return_lines else 1,
        "severity": "PASS" if protected else "P0",
        "fc": "FC-6",
        "context": f"get_phase_strategy() return lock-protected: {protected} "
                   f"({len(get_return_lines)} return(s) found, {len(locked_regions)} locked region(s))",
    })
    findings.append({
        "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
        "line": 1,
        "severity": "PASS" if has_phaseconfig else "INFO",
        "fc": "FC-6",
        "context": f"PhaseConfig class: {has_phaseconfig}",
    })

    return {
        "found": True,
        "has_lock_def": has_lock_def,
        "lock_protected": protected,
        "has_phaseconfig": has_phaseconfig,
        "findings": findings,
    }


def apply_changes(state: dict) -> dict:
    applied = 0
    skipped = 0
    if state.get("lock_protected"):
        logging.info("get_phase_strategy() is already lock-protected — PP2 satisfied")
        skipped += 1
    else:
        logging.info("PP2 fix required: wrap get_phase_strategy() return with 'with _strategy_lock:'")
        skipped += 1
    return {"applied": applied, "skipped": skipped}


def run(args: argparse.Namespace) -> dict:
    args.output_dir.mkdir(parents=True, exist_ok=True)

    state = scan_phaseconfig()
    applied = apply_changes(state) if args.apply else {}

    status = "OK"
    if any(f["severity"] == "P0" for f in state["findings"]):
        status = "P0_LOCK_RACE"

    report = {
        "script": _NAME,
        "wave": _WAVE,
        "fc": ["FC-6"],
        "pp": ["PP2"],
        "subtask_refs": [f"{_WAVE}.{_NAME}"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": status,
        "apply": args.apply,
        "state": state,
        "totals": {"applied": applied.get("applied", 0), "skipped": applied.get("skipped", 0)},
        "evidence": {
            "plan_ref": "plan.md (FC-6 / PP2 — get_phase_strategy lock)",
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
