#!/usr/bin/env python3
"""W10.2 — Extract decompose/ module from touring-server to touring-orchestration.

Generates the move plan: identifies files in touring-server containing decompose
logic + proposes destination paths in touring-orchestration/src/decompose/.
"""
from __future__ import annotations

import re

import script_common as sc

_SCRIPT = "w10_decompose_extract"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR
_SERVER_SRC = _ROOT / "crates" / "touring-server" / "src"

DECOMPOSE_PATTERN = re.compile(r"\b(decompose|TaskGraph|subtask)", re.IGNORECASE)


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    p.add_argument("--min-hits", type=int, default=3,
                   help="Min decompose-pattern hits to consider a file.")
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


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


def run(args: sc.Namespace) -> dict:
    """Scan + emit extraction plan."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    candidates = scan_decompose_files(args.min_hits)
    report = {
        "script": "w10_decompose_extract",
        "wave": "W10", "subtask_refs": ["W10.2"],
        "timestamp": sc.now_iso(),
        "status": "OK",
        "totals": {
            "candidates": len(candidates),
            "total_loc": sum(c["loc"] for c in candidates),
        },
        "candidates": candidates,
    }
    json_path = args.output_dir / "w10-decompose-extraction.json"
    return sc.write_json_report(json_path, report, _ROOT)


def _run_and_report(args: sc.Namespace) -> int:
    """Run w10_decompose_extract, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json({
        "status": result["status"], "totals": result["totals"],
        "top_5": [c["file"] for c in result["candidates"][:5]],
        "json_path": result["json_path"],
    })
    return 0


def main() -> int:
    """Entry point of w10_decompose_extract."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
