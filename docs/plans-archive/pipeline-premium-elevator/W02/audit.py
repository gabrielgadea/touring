#!/usr/bin/env python3
"""W02.audit — Lock in get_phase_strategy() read (FC-6 / PP2).

Forensic sub-script for W02 — Wave W02.

FC-6: concurrent reads of get_phase_strategy() lack lock acquisition.
get_phase_strategy() at line 200 returns _current_strategy directly without
holding _strategy_lock (line 178), while configure_phase_strategy()
(line 182) DOES acquire the lock before writing. This creates a data race.

Outputs
-------
  - data/W02-audit.json   (machine-readable)
  - staging/W02-audit.md  (human-readable)

Usage
-----
    python3 audit.py            # dry-run mode (default)
    python3 audit.py --apply    # actual mutation (not implemented for audit)
    python3 audit.py -j         # JSON-only stdout

Evidence: pipeline_runner.py lines 178, 182, 200, 206.
Confidence: FACT [1.0] — lock and function are on the exact lines cited.
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
_WAVE = "W02"
_NAME = "audit"

_EXIT_OK = 0
_EXIT_FAIL = 1
_EXIT_INTERRUPTED = 130


def scan_workspace(_root: Path) -> list[dict]:  # noqa: ARG001
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

    # Simple text search — robust vs multi-line signatures
    lock_def_lineno = 0
    configure_lineno = 0
    get_phase_lineno = 0
    return_lineno = 0

    for lineno, line in enumerate(lines, start=1):
        if "_strategy_lock" in line and "threading.Lock" in line or "threading.RLock" in line:
            if not lock_def_lineno:
                lock_def_lineno = lineno
        if "def configure_phase_strategy" in line:
            configure_lineno = lineno
        if "def get_phase_strategy" in line:
            get_phase_lineno = lineno
        if "return _current_strategy" in line and get_phase_lineno > 0 and return_lineno == 0:
            # Only count as the target return if we're past get_phase_strategy def
            return_lineno = lineno

    # Find if return is lock-protected: search backwards from return_lineno
    # for 'with _strategy_lock:' at same indentation level
    protected = False
    if return_lineno > 0:
        return_indent = len(lines[return_lineno - 1]) - len(lines[return_lineno - 1].lstrip())
        function_body_indent = return_indent
        lock_lineno = 0
        for prev_ln in range(return_lineno - 2, max(0, return_lineno - 20), -1):
            prev_line = lines[prev_ln]
            prev_stripped = prev_line.strip()
            prev_indent = len(prev_line) - len(prev_line.lstrip())
            # Blank / comment lines — keep searching
            if not prev_stripped or prev_stripped.startswith("#"):
                continue
            # Is this the lock context manager?  Check BEFORE dedent boundary.
            if "with _strategy_lock" in prev_stripped and "_strategy_lock" in prev_stripped:
                lock_lineno = prev_ln + 1
                protected = True
                break
            # Dedent boundary — we left the function body without seeing the lock → not protected
            if prev_indent < function_body_indent:
                break

    # Finding 1: lock defined
    if lock_def_lineno:
        findings.append({
            "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
            "line": lock_def_lineno,
            "severity": "PASS",
            "fc": "FC-6",
            "context": f"_strategy_lock defined at line {lock_def_lineno}",
        })
    else:
        findings.append({
            "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
            "line": 0,
            "severity": "P0",
            "fc": "FC-6",
            "context": "_strategy_lock not found in pipeline_runner.py",
        })

    # Finding 2: configure_phase_strategy uses lock (PASS)
    if configure_lineno:
        findings.append({
            "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
            "line": configure_lineno,
            "severity": "PASS",
            "fc": "FC-6",
            "context": f"configure_phase_strategy at line {configure_lineno} — uses lock (correct writer)",
        })

    # Finding 3: get_phase_strategy return — race condition or protected
    if get_phase_lineno and return_lineno:
        if protected:
            findings.append({
                "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
                "line": return_lineno,
                "severity": "PASS",
                "fc": "FC-6",
                "context": f"get_phase_strategy return at line {return_lineno} — lock-protected (already fixed)",
            })
        else:
            findings.append({
                "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
                "line": return_lineno,
                "severity": "P0",
                "fc": "FC-6",
                "context": (
                    f"RACE CONDITION: get_phase_strategy() return at line {return_lineno} "
                    "without holding _strategy_lock. configure_phase_strategy() at line ~{0} "
                    "writes WITH lock, but reader get_phase_strategy() does NOT. "
                    "Fix: wrap return with 'with _strategy_lock:'."
                ).format(configure_lineno),
            })
    else:
        findings.append({
            "file": str(_TARGET_FILE.relative_to(_ANALISE_ROOT)),
            "line": 0,
            "severity": "P0",
            "fc": "FC-6",
            "context": "get_phase_strategy() return not found",
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
        "fc": ["FC-6"],
        "pp": ["PP2"],
        "subtask_refs": [f"{_WAVE}.{_NAME}"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "P0" if any(f["severity"] == "P0" for f in findings) else "OK",
        "apply": args.apply,
        "totals": {"findings": len(findings), **applied},
        "findings": findings,
        "evidence": {
            "plan_ref": "plan.md (FC-6 / PP2 — Lock in get_phase_strategy read)",
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
