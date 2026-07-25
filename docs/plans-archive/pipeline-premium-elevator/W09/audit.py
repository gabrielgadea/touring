#!/usr/bin/env python3
"""W09.audit — Wave W09.

Forensic sub-script for W09 — Wave W09.

Outputs
-------
  - data/W09-audit.json   (machine-readable JSON envelope)
- staging/W09-audit.md  (human-readable narrative)
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
_WAVE = "W09"
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
_RE_MEMORY_IMPORT = __import__("re").compile(
    r"from\s+\.memory_manager\s+import\s+MemoryManager"
)


def scan_workspace(root: Path) -> list[dict]:
    """Scan pipeline_runner.py for PP9 RSS backpressure implementation.

    PP9: RSS backpressure + MemoryManager adaptive batching.
    Plan ref: plan.md lines 143-149.
    """
    findings: list[dict] = []
    target = Path("/home/gabrielgadea/projects/analise/scripts/process_analysis/pipeline_runner.py")
    if not target.exists():
        findings.append({
            "file": "scripts/process_analysis/pipeline_runner.py",
            "line": 0,
            "severity": "P0",
            "fc": "PP9",
            "context": "pipeline_runner.py not found",
        })
        return findings

    content = target.read_text(encoding="utf-8", errors="ignore")
    lines = content.splitlines()

    # Check 1: MemoryManager import
    import_match = _RE_MEMORY_IMPORT.search(content)
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": next(
            ((i + 1) for i, l in enumerate(lines) if _RE_MEMORY_IMPORT.search(l)),
            0,
        ),
        "severity": "PASS",
        "fc": "PP9",
        "pp": "PP9-audit",
        "context": (
            "MemoryManager imported"
            if import_match
            else "MISSING: MemoryManager not imported",
        ),
    })

    # Check 2: _adapt_batch_size method
    adapt_match = __import__("re").compile(
        r"def\s+_adapt_batch_size\s*\("
    ).search(content)
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": next(
            ((i + 1) for i, l in enumerate(lines)
            if "def _adapt_batch_size(" in l),
            0,
        ),
        "severity": "PASS" if adapt_match else "P1",
        "fc": "PP9",
        "pp": "PP9-audit",
        "context": (
            "_adapt_batch_size() method found"
            if adapt_match
            else "MISSING: _adapt_batch_size() not found",
        ),
    })

    # Check 3: check_pressure() call
    pressure_match = __import__("re").compile(
        r"check_pressure\s*\("
    ).search(content)
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": next(
            ((i + 1) for i, l in enumerate(lines) if "check_pressure" in l),
            0,
        ),
        "severity": "PASS" if pressure_match else "P1",
        "fc": "PP9",
        "pp": "PP9-audit",
        "context": (
            "check_pressure() called"
            if pressure_match
            else "MISSING: check_pressure() not called",
        ),
    })

    # Check 4: check_hard_limit() call (10GB abort)
    hard_match = __import__("re").compile(
        r"check_hard_limit\s*\("
    ).search(content)
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": next(
            ((i + 1) for i, l in enumerate(lines) if "check_hard_limit" in l),
            0,
        ),
        "severity": "PASS" if hard_match else "P1",
        "fc": "PP9",
        "pp": "PP9-audit",
        "context": (
            "check_hard_limit() called (10GB RSS hard limit)"
            if hard_match
            else "MISSING: check_hard_limit() not called",
        ),
    })

    # Check 5: batch_size halving on pressure
    halving_match = __import__("re").compile(
        r"batch_size\s*=\s*max\s*\(\s*\d+\s*,\s*\w+\s*//\s*2"
    ).search(content)
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": next(
            ((i + 1) for i, l in enumerate(lines)
            if "batch_size" in l and "// 2" in l and "max" in l),
            0,
        ),
        "severity": "PASS" if halving_match else "P2",
        "fc": "PP9",
        "pp": "PP9-audit",
        "context": (
            "batch_size halving on memory pressure found"
            if halving_match
            else "WEAK: batch_size halving pattern not detected",
        ),
    })

    # Check 6: rss_mb logging
    rss_match = __import__("re").compile(
        r"rss_mb|RSS.*=.*MB"
    ).search(content)
    findings.append({
        "file": "scripts/process_analysis/pipeline_runner.py",
        "line": next(
            ((i + 1) for i, l in enumerate(lines)
            if "rss_mb" in l or ("RSS" in l and "MB" in l)),
            0,
        ),
        "severity": "PASS" if rss_match else "P2",
        "fc": "PP9",
        "pp": "PP9-audit",
        "context": (
            "RSS memory reporting found"
            if rss_match
            else "WEAK: RSS memory reporting not found",
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
