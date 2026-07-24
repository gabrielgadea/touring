#!/usr/bin/env python3
"""wiring_integrity_gate.py - no-back-edge guard: the workspace DAG must stay acyclic.

Master Plan H (C.W3.P3.T15). Materializes the dogfooding lesson "a cura esta dentro":
Touring detects dependency cycles for every workspace it indexes, yet never enforced
acyclicity on itself. A05 + a wiring rebuild this session cleared a phantom depth-683
cycle; this gate locks that win in so the back edge can never silently return.

The gate is a no-back-edge guard, NOT an orphan gate. Orphan count is reported for
information only (the workspace carries thousands of intentional pub symbols behind
feature flags / generated inferlets - failing on orphans is a known false signal).

Daemon-optional by design (fail-open): if the touring CLI is missing or the daemon
is down, the gate prints a WARNING and exits 0 so CI never breaks on infrastructure.
This mirrors docs/sync_metrics.py and docs/file_size_gate.py.

Usage:
    docs/wiring_integrity_gate.py            human-readable cycle + orphan report
    docs/wiring_integrity_gate.py --json     machine-readable JSON
    docs/wiring_integrity_gate.py --check    exit 1 if cycle_count > 0 (no-back-edge guard)
"""
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MIN_DEPTH = 2  # depth-1 self-edges are noise; real back edges span >= 2 modules


def _run(args: list[str], timeout: int = 30) -> subprocess.CompletedProcess | None:
    """Run a touring CLI subcommand, returning None on any failure (fail-open)."""
    if shutil.which("touring") is None:
        return None
    try:
        return subprocess.run(
            args,
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except Exception:
        return None


def cycle_count() -> int | None:
    """Authoritative cycle count via `touring wiring cycles --format json`.

    Returns the cycle_count int, or None if the daemon/CLI is unavailable
    (None => fail-open, the caller must not treat it as a violation).

    Note: `wiring cycles` emits JSON only under `--format json`; the generic
    `-j` flag is silently ignored by this subcommand (verified against the CLI).
    """
    proc = _run(["touring", "wiring", "cycles", "--min-depth", str(MIN_DEPTH), "--format", "json"])
    if proc is None or proc.returncode != 0:
        return None
    try:
        return int(json.loads(proc.stdout).get("cycle_count"))
    except Exception:
        return None


def cycles_detail() -> list:
    """Full cycle list (paths + depths) for the human / JSON report."""
    proc = _run(["touring", "wiring", "cycles", "--min-depth", str(MIN_DEPTH), "--format", "json"])
    if proc is None or proc.returncode != 0:
        return []
    try:
        return json.loads(proc.stdout).get("cycles", []) or []
    except Exception:
        return []


def orphan_count() -> int | None:
    """Orphan count via `touring wiring orphans -j` (informational only)."""
    proc = _run(["touring", "wiring", "orphans", "-j"])
    if proc is None or proc.returncode != 0:
        return None
    try:
        return int(json.loads(proc.stdout).get("orphan_count"))
    except Exception:
        return None


def collect() -> dict:
    """Gather the wiring-integrity snapshot. daemon_available is False when the
    cycle probe could not run (CLI absent or daemon down)."""
    cc = cycle_count()
    return {
        "min_depth": MIN_DEPTH,
        "daemon_available": cc is not None,
        "cycle_count": cc,
        "cycles": cycles_detail(),
        "orphan_count": orphan_count(),
    }


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Touring wiring-integrity gate (no-back-edge / acyclic DAG guard)."
    )
    ap.add_argument("--json", action="store_true", help="machine-readable JSON")
    ap.add_argument(
        "--check",
        action="store_true",
        help="exit 1 if cycle_count > 0 (the no-back-edge guard); fail-open when daemon down",
    )
    args = ap.parse_args()

    snap = collect()

    if args.json:
        print(json.dumps(snap, indent=2))
        if args.check and snap["daemon_available"] and (snap["cycle_count"] or 0) > 0:
            return 1
        return 0

    if not snap["daemon_available"]:
        print(
            "WARNING: touring CLI/daemon unavailable - wiring-integrity gate skipped (fail-open)",
            file=sys.stderr,
        )
        # Fail-open: CI must not break because the daemon is not up.
        return 0

    cc = snap["cycle_count"] or 0
    orph = snap["orphan_count"]
    orph_str = "n/a" if orph is None else str(orph)

    print("Touring wiring-integrity gate (no-back-edge / acyclic DAG):")
    print(f"  min_depth     : {snap['min_depth']}")
    print(f"  cycle_count   : {cc}")
    print(f"  orphan_count  : {orph_str}  (informational - not a failure condition)")

    if cc > 0:
        print(f"\n  {cc} BACK EDGE(S) DETECTED - workspace DAG is no longer acyclic:", file=sys.stderr)
        for cyc in snap["cycles"]:
            path = cyc.get("path", cyc)
            depth = cyc.get("depth", "?")
            print(f"    depth {depth}: {path}", file=sys.stderr)

    if args.check:
        if cc > 0:
            print("\nFAIL: dependency cycle(s) present (no-back-edge guard tripped)", file=sys.stderr)
            return 1
        print("\nOK: 0 cycles - workspace DAG is acyclic")
        return 0

    if cc == 0:
        print("\n  OK: 0 cycles - workspace DAG is acyclic")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
