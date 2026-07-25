#!/usr/bin/env python3
"""TACO-cross-audit harmony map — Phase 4 of the cross-audit.

Aggregates ``touring wiring`` orphans, audit, and cycles into a single
connection-harmony report: orphan public symbols, dependency cycles, and
low-score modules. When the touring daemon is down, falls back to a grep-based
public-definition count and marks the report ``daemon_degraded``.

Output is a human report, or JSON with ``--json``.
Exit code: 0 = harmonious, 1 = disharmony found or degraded, 2 = bad arguments.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any, Sequence

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib  # noqa: E402 — local sibling module


def _count(value: Any) -> int:
    """Best-effort count from a touring JSON payload (list, or {count}/{items})."""
    if isinstance(value, list):
        return len(value)
    if isinstance(value, dict):
        for key in ("count", "orphan_count", "cycle_count"):
            if isinstance(value.get(key), int):
                return value[key]
        for key in ("orphans", "items", "cycles", "modules"):
            if isinstance(value.get(key), list):
                return len(value[key])
    return 0


def collect(root: Path) -> dict[str, Any]:
    """Gather wiring-harmony signals from touring, with a degraded fallback."""
    if lib.touring_available():
        orphans = lib.touring("wiring", "orphans", "-j")
        audit = lib.touring("wiring", "audit", "-j")
        cycles = lib.touring("wiring", "cycles", "--format", "json")
        return {
            "daemon_degraded": False,
            "orphan_symbols": _count(orphans),
            "low_score_modules": _count(audit),
            "dependency_cycles": _count(cycles),
        }
    # Degraded fallback: a grep-based count cannot see consumers, so it is an
    # upper bound on public surface, explicitly marked — not an orphan count.
    pub_defs = 0
    for path in lib.walk_code_files(root, frozenset({".rs"})):
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        pub_defs += text.count("pub fn ") + text.count("pub struct ")
    return {
        "daemon_degraded": True,
        "orphan_symbols": None,
        "pub_definitions_seen": pub_defs,
        "low_score_modules": None,
        "dependency_cycles": None,
    }


def main(argv: Sequence[str] | None = None) -> int:
    """Build the connection-harmony report for a directory."""
    parser = argparse.ArgumentParser(
        prog="harmony_map.py",
        description="Aggregate touring wiring orphans/audit/cycles into a harmony report.",
    )
    parser.add_argument("directory", nargs="?", default=".",
                        help="Root directory (default: current).")
    parser.add_argument("--json", action="store_true", help="Emit JSON.")
    args = parser.parse_args(argv)

    root = Path(args.directory).expanduser().resolve()
    if not root.is_dir():
        print(f"error: not a directory: {root}", file=sys.stderr)
        return 2

    report = collect(root)
    harmonious = (
        not report["daemon_degraded"]
        and report["orphan_symbols"] == 0
        and report["dependency_cycles"] == 0
    )
    report["harmonious"] = harmonious

    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 0 if harmonious else 1

    print(f"== harmony map: {root} ==\n")
    if report["daemon_degraded"]:
        print("touring daemon down — degraded mode (daemon_degraded).")
        print(f"  public definitions seen (grep): {report['pub_definitions_seen']}")
        print("  orphans / cycles / scores need the daemon — re-run when it is up.")
        return 1
    print(f"  orphan public symbols: {report['orphan_symbols']}")
    print(f"  low-score modules:     {report['low_score_modules']}")
    print(f"  dependency cycles:     {report['dependency_cycles']}")
    print()
    if harmonious:
        print("every connection is sound — the tree is in harmony.")
        return 0
    print("disharmony found — each orphan and cycle is a Phase 5 task.")
    print("REGRA #0: wire orphans to consumers, do not delete them.")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
