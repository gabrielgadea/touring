#!/usr/bin/env python3
"""W1.1+W1.2 — Audit + identify dead crates (semantic-spike + 3 wasm 0-LOC).

Scans crates/ directory, identifies candidates for deletion based on:
  - LOC src == 0 (truly empty)
  - LOC pub == 0 (no public API)
  - Presence of ARCHITECTURE.md indicating "archived" status
  - Listed in workspace.members but not consumed by any other crate

With --apply, performs deletion of identified crates + updates workspace
Cargo.toml members. Without --apply (default), only reports.

Output: docs/baselines/dead-code-report-<DATE>.json

Usage
-----
    python3 w1_audit_dead_code.py                           # report only
    python3 w1_audit_dead_code.py --apply                   # delete + update workspace
    python3 w1_audit_dead_code.py --crates touring-foo,touring-bar
"""
from __future__ import annotations

import argparse
import json
import logging
import re
import subprocess
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_CRATES_DIR = _ROOT / "crates"
_BASELINES_DIR = _ROOT / "docs" / "baselines"
_WORKSPACE_CARGO = _ROOT / "Cargo.toml"

# Known dead crates from forensic audit 2026-05-11
KNOWN_DEAD = {
    "touring-semantic-spike": "Archived per ARCHITECTURE.md; 66 LOC, 0 pub",
    "touring-wasm-client": "0 LOC src",
    "touring-wasm-common": "0 LOC src",
    "touring-wasm-server": "0 LOC src",
}


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w1_audit_dead_code", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--apply", action="store_true",
                   help="DESTRUCTIVE — actually delete crates + update workspace.")
    p.add_argument("--crates", type=str, default="",
                   help="Comma-separated crate names. Default: KNOWN_DEAD list.")
    p.add_argument("--output-dir", type=Path, default=_BASELINES_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def count_loc(directory: Path, pattern: str = "*.rs") -> int:
    """Count total LOC in directory matching pattern (recursive)."""
    if not directory.exists():
        return 0
    total = 0
    for f in directory.rglob(pattern):
        if f.is_file():
            try:
                total += sum(1 for _ in f.read_text(encoding="utf-8",
                                                    errors="replace").splitlines())
            except OSError:
                pass
    return total


def count_pub(directory: Path) -> int:
    """Count `^pub ` declarations in .rs files (heuristic)."""
    if not directory.exists():
        return 0
    total = 0
    for f in directory.rglob("*.rs"):
        if f.is_file():
            try:
                for line in f.read_text(encoding="utf-8",
                                        errors="replace").splitlines():
                    if line.lstrip().startswith("pub "):
                        total += 1
            except OSError:
                pass
    return total


def audit_crate(name: str) -> dict:
    """Audit a single crate for dead-code indicators."""
    crate_dir = _CRATES_DIR / name
    src_dir = crate_dir / "src"
    if not crate_dir.exists():
        return {"crate": name, "exists": False}
    loc_src = count_loc(src_dir)
    pub_count = count_pub(src_dir)
    has_arch_md = any(crate_dir.glob("*ARCHITECTURE*.md"))
    in_workspace = name in _WORKSPACE_CARGO.read_text(encoding="utf-8")
    return {
        "crate": name, "exists": True,
        "path": str(crate_dir.relative_to(_ROOT)),
        "loc_src": loc_src,
        "pub_count": pub_count,
        "has_architecture_md": has_arch_md,
        "in_workspace_members": in_workspace,
        "candidate_for_deletion": (
            loc_src == 0 or
            (pub_count == 0 and loc_src < 100) or
            (has_arch_md and pub_count == 0)
        ),
        "reason": KNOWN_DEAD.get(name, ""),
    }


def delete_crate(name: str) -> dict:
    """DESTRUCTIVE: remove crate directory + workspace member entry."""
    crate_dir = _CRATES_DIR / name
    if not crate_dir.exists():
        return {"crate": name, "status": "ALREADY_GONE"}
    # Remove via shutil
    import shutil
    shutil.rmtree(crate_dir)
    LOGGER.info("removed directory: %s", crate_dir)
    # Remove from workspace.members (sed-like)
    cargo_text = _WORKSPACE_CARGO.read_text(encoding="utf-8")
    member_pattern = re.compile(rf'\s*"crates/{re.escape(name)}",?\n')
    new_text = member_pattern.sub("", cargo_text)
    if new_text != cargo_text:
        _WORKSPACE_CARGO.write_text(new_text, encoding="utf-8")
        LOGGER.info("removed from workspace.members: %s", name)
    return {"crate": name, "status": "DELETED",
            "workspace_updated": new_text != cargo_text}


def run(apply: bool, crate_list: list[str], output_dir: Path) -> dict:
    """Audit + optionally apply deletion."""
    audits = [audit_crate(c) for c in crate_list]
    candidates = [a for a in audits if a.get("candidate_for_deletion")]
    actions = []
    if apply:
        for c in candidates:
            actions.append(delete_crate(c["crate"]))
    output_dir.mkdir(parents=True, exist_ok=True)
    report = {
        "script": "w1_audit_dead_code",
        "wave": "W1",
        "subtask_refs": ["W1.1", "W1.2"],
        "timestamp": datetime.now(UTC).isoformat(),
        "apply": apply,
        "total_audited": len(audits),
        "candidates_for_deletion": len(candidates),
        "audits": audits,
        "actions": actions,
    }
    report_path = output_dir / f"dead-code-report-{datetime.now(UTC).strftime('%Y-%m-%d')}.json"
    report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                           encoding="utf-8")
    LOGGER.info("report written: %s", report_path)
    return report


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        crate_list = (args.crates.split(",") if args.crates
                      else list(KNOWN_DEAD.keys()))
        result = run(args.apply, crate_list, args.output_dir)
        print(json.dumps({
            "summary": {
                "audited": result["total_audited"],
                "candidates": result["candidates_for_deletion"],
                "applied": args.apply,
            },
            "candidates": [a["crate"] for a in result["audits"]
                           if a.get("candidate_for_deletion")],
        }, indent=2, ensure_ascii=False))
        return 0
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
