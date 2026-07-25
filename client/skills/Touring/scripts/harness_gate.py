#!/usr/bin/env python3
"""touring harness gate — R6 quality gate for the master command surface.

Scores every implementation file behind the master CLI commands
(``scout/read/health/guard/map/blast/investigate``) on the 50-dimension
``touring-quality`` harness and fails when any drops below the Gold floor
(0.80). This is the "harness-metric as quality gate" of the coupling roadmap
(§6 R6): the master commands must themselves meet the elite bar they help the
agent reach.

I-correctness: scores come from ``touring-quality`` (the 50-dim authority via
``lib_touring.quality_score``), never the ``ast meta`` quality_score proxy.

Exit codes:
  0  all scored targets ≥ floor (gate PASS)
  1  at least one target below floor (gate FAIL)
  3  could not score any target (touring-quality unavailable — UNVERIFIED)

Usage:
  harness_gate.py                 # human report, floor 0.80
  harness_gate.py --json          # machine JSON
  harness_gate.py --brief         # compact digest
  harness_gate.py --floor 0.90    # raise the bar (e.g. Platinum)
"""
from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    add_common_args, emit_kv, emit_result, emit_section, emit_table,
    mark_degraded, quality_score,
)

# The Python scripts backing the master commands (siblings of this gate), plus the
# shared Layer-3 core every command depends on.
SCRIPT_TARGETS = (
    "discover_symbol.py",   # scout
    "read_file.py",         # read
    "diagnose_health.py",   # health
    "pre_edit_gate.py",     # guard
    "discover_workspace.py",  # map
    "analyze_blast.py",     # blast
    "investigate.py",       # investigate (R5)
    "lib_touring.py",       # shared core
    "explore_until_dry.py",  # explore (F1 — CCE loop-until-dry)
    "plan_refine.py",       # plan refinement (F2 — refine-to-plateau)
    "adw.py",               # adw (F0 — durable workflow runner)
    "factory.py",           # factory (F4 — ticket router)
    "scout_perpetuo.py",    # scout-perpetuo engine (F1.7)
)

# The Rust modules that wire the master commands into the CLI.
RUST_TARGETS_REL = (
    "crates/touring-server/src/cli/master.rs",   # scout/read/health/guard/map/blast/investigate
    "crates/touring-server/src/cli/audit.rs",    # audit (adapter over the run_audit engine)
)


def workspace_root() -> Path:
    """Touring workspace root: ``$TOURING_WORKSPACE_ROOT`` or the default install."""
    return Path(os.environ.get("TOURING_WORKSPACE_ROOT") or (Path.home() / ".claude" / "rust"))


def collect_targets() -> list[Path]:
    """All existing implementation files of the master command surface."""
    here = Path(__file__).resolve().parent
    targets = [here / name for name in SCRIPT_TARGETS]
    targets.extend(workspace_root() / rel for rel in RUST_TARGETS_REL)
    return [p for p in targets if p.is_file()]


def score_targets(targets: list[Path], *, timeout: float) -> list[dict[str, Any]]:
    """Score each target on the 50-dim harness, preserving degraded markers."""
    rows: list[dict[str, Any]] = []
    for path in targets:
        result = quality_score(path, timeout=timeout)
        rows.append({
            "path": str(path),
            "composite": result.get("composite"),
            "tier": result.get("tier"),
            "degraded": bool(result.get("degraded")),
            "blockers": result.get("blockers", []),
        })
    return rows


def evaluate(rows: list[dict[str, Any]], *, floor: float) -> dict[str, Any]:
    """Apply the floor: PASS only when every *scored* target is at or above it.

    Degraded targets (touring-quality unavailable for them) are reported but do
    not count as failures — a missing tool must not masquerade as a quality
    regression. When nothing could be scored, the gate is UNVERIFIED (exit 3).
    """
    scored = [r for r in rows if not r["degraded"] and isinstance(r["composite"], (int, float))]
    below = [r for r in scored if r["composite"] < floor]
    report: dict[str, Any] = {
        "floor": floor,
        "targets": rows,
        "scored_count": len(scored),
        "below_floor": [{"path": r["path"], "composite": r["composite"]} for r in below],
    }
    if not scored:
        report["verdict"] = "UNVERIFIED"
        report["exit_code"] = 3
        mark_degraded(report, True, "touring-quality could not score any target")
    elif below:
        report["verdict"] = "FAIL"
        report["exit_code"] = 1
        mark_degraded(report, False)
    else:
        report["verdict"] = "PASS"
        report["exit_code"] = 0
        mark_degraded(report, False)
    return report


def emit_human(report: dict[str, Any]) -> None:
    """Render the gate result as a readable table + verdict."""
    emit_section(f"HARNESS GATE  ·  master command surface  ({report['verdict']})")
    emit_kv("floor", report["floor"])
    emit_kv("verdict", report["verdict"])
    emit_kv("scored", report["scored_count"])
    rows = [
        [Path(r["path"]).name, r["composite"], r["tier"], "degraded" if r["degraded"] else "ok"]
        for r in report["targets"]
    ]
    emit_section("scores", char="-")
    emit_table(rows, headers=["file", "composite", "tier", "state"])
    if report["below_floor"]:
        emit_section("below floor", char="-")
        for item in report["below_floor"]:
            print(f"  FAIL  {Path(item['path']).name}  {item['composite']}")


def main() -> int:
    """Score the master surface and gate at the floor; return the verdict code."""
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--floor", type=float, default=0.80,
                        help="Minimum composite per target (default 0.80 = Gold)")
    add_common_args(parser)
    args = parser.parse_args()

    targets = collect_targets()
    rows = score_targets(targets, timeout=args.timeout)
    report = evaluate(rows, floor=args.floor)

    if not emit_result(report, args):
        emit_human(report)
    return int(report["exit_code"])


if __name__ == "__main__":
    sys.exit(main())
