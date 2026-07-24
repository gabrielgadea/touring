#!/usr/bin/env python3
"""W10.2 — Extract decompose/ module from touring-server to touring-orchestration.

Generates the move plan: identifies files in touring-server containing decompose
logic + proposes destination paths in touring-orchestration/src/decompose/.
"""
from __future__ import annotations

import argparse
import json
import logging
import re
import sys
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_SERVER_SRC = _ROOT / "crates" / "touring-server" / "src"
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

DECOMPOSE_PATTERN = re.compile(r"\b(decompose|TaskGraph|subtask)", re.I)


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w10_decompose_extract", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--min-hits", type=int, default=3,
                   help="Min decompose-pattern hits to consider a file.")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def scan_decompose_files(min_hits: int) -> list[dict]:
    """Find candidate files in touring-server for extraction."""
    candidates: list[dict] = []
    if not _SERVER_SRC.exists():
        return []
    for rs in _SERVER_SRC.rglob("*.rs"):
        try:
            content = rs.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        hits = len(DECOMPOSE_PATTERN.findall(content))
        if hits >= min_hits:
            candidates.append({
                "file": str(rs.relative_to(_ROOT)),
                "loc": sum(1 for _ in content.splitlines()),
                "decompose_hits": hits,
                "proposed_dest": f"crates/touring-orchestration/src/decompose/"
                                  f"{rs.name}",
            })
    return sorted(candidates, key=lambda x: -x["decompose_hits"])


def run(args: argparse.Namespace) -> dict:
    """Scan + emit extraction plan."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    candidates = scan_decompose_files(args.min_hits)
    report = {
        "script": "w10_decompose_extract",
        "wave": "W10", "subtask_refs": ["W10.2"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "totals": {
            "candidates": len(candidates),
            "total_loc": sum(c["loc"] for c in candidates),
        },
        "candidates": candidates,
    }
    json_path = args.output_dir / "w10-decompose-extraction.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    report["json_path"] = str(json_path.relative_to(_ROOT))
    return report


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps({
            "status": result["status"], "totals": result["totals"],
            "top_5": [c["file"] for c in result["candidates"][:5]],
            "json_path": result["json_path"],
        }, indent=2, ensure_ascii=False) + "\n")
        return 0
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
