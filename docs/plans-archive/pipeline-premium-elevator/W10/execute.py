#!/usr/bin/env python3
"""W10.execute — Wave W10.

Forensic sub-script for W10 — Wave W10.

Outputs
-------
  - data/W10-execute.json   (machine-readable JSON envelope)
- staging/W10-execute.md  (human-readable narrative)
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
_WAVE = "W10"
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
def scan_workspace(root: Path) -> list[dict]:
    """Read-only forensic scan over the workspace.

    Args:
        root: Workspace root to scan.

    Returns:
        List of finding dicts. Each finding MUST include at minimum:
        ``file``, ``line`` (1-based), ``severity`` (P0/P1/P2/P3).
    """
    findings: list[dict] = []
    for path in root.rglob("*.py"):
        # TODO: implement the scan logic here.
        # Example pattern:
        #     if _RE_PATTERN.search(path.read_text(encoding="utf-8", errors="ignore")):
        #         findings.append({
        #             "file": str(path.relative_to(root)),
        #             "line": 1,
        #             "severity": "P2",
        #             "context": "",
        #         })
        _ = path  # silence linters until implementation lands
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
