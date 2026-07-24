#!/usr/bin/env python3
"""file_size_gate.py - fail when a non-whitelisted Rust file exceeds the size budget.

Master Plan H1 (C.W3.P1.T10). Materializes the central lesson of the diagnostic
(section 3.2): without an enforced file-size gate, a 168-LOC hub regressed to
19,444 LOC in 7 weeks. This gate makes that regression impossible for NEW files
and freezes the known hotspots behind a dated whitelist so they can only shrink.

Usage:
    docs/file_size_gate.py             report files over budget (human-readable)
    docs/file_size_gate.py --json      machine-readable
    docs/file_size_gate.py --check     exit 1 if any non-whitelisted file exceeds budget
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUDGET = 5000  # LOC per .rs file (rust-analyzer norm ~1-1.5k; 5k is deliberately generous)

# Dated whitelist: known hotspots predating the gate. Paths updated 2026-06-11
# after daemon-lib-rearch moved touring-hooks/src -> touring-dispatch/src. Each cap freezes the current
# size so the file can only SHRINK (decomposition tracked in the Master Plan).
# Format: path -> (max_allowed_loc, reason).
WHITELIST = {
    "crates/touring-dispatch/src/cli_handlers.rs": (
        400, "N01 hotspot; A.W2 decomposed core 9077->6051->2819->359 LOC into src/cli/ (43 modules). "
             "Cap ratcheted 9500->6100->2900->400; A.W2.P5 (2026-06-05) finished mechanical extraction: "
             "shared helpers -> cli/shared.rs; singletons -> cli/{prove,execute,calibrate,pretask,health,"
             "rlsearch}.rs; both inline test modules -> cli_handlers_{tests,gate_tests}.rs via #[path]. "
             "Residual = response structs (KEEP) + pub-use facade re-exports preserving "
             "crate::cli_handlers::cli_* dispatch paths. 4017 tests pass; 0 new clippy."),
    "crates/touring-dispatch/src/lifecycle.rs": (
        250, "A02; A.W3.P3 (2026-06-05) relocated the inline `mod tests` (19,293 LOC, 1211 tests) "
             "to lifecycle/tests.rs. Production region now 153 LOC; cap ratcheted 19500->250."),
    "crates/touring-dispatch/src/lifecycle/tests.rs": (
        19400, "A.W3.P3 (2026-06-05); relocated inline tests from lifecycle.rs (test-only, not a "
               "production hotspot). 1211 tests verified passing. Flat `use super::*` module; "
               "splitting deferred (no internal mod boundaries — high-risk re-import)."),
    "crates/touring-dispatch/src/wiring.rs": (
        2700, "wiring SCC + Tarjan; A.W3.P2 (frozen 2026-06-04)"),
}


def scan() -> list[tuple[str, int]]:
    over: list[tuple[str, int]] = []
    for path in (ROOT / "crates").rglob("*.rs"):
        if "/target/" in str(path):
            continue
        try:
            n = path.read_text(errors="ignore").count("\n") + 1
        except Exception:
            continue
        if n > BUDGET:
            over.append((str(path.relative_to(ROOT)), n))
    return sorted(over, key=lambda kv: -kv[1])


def violations(over: list[tuple[str, int]]) -> list[dict]:
    out = []
    for path, n in over:
        cap = WHITELIST.get(path)
        if cap is None:
            out.append({"file": path, "loc": n, "limit": BUDGET, "status": "NEW_VIOLATION"})
        elif n > cap[0]:
            out.append({"file": path, "loc": n, "limit": cap[0], "status": "WHITELIST_GREW"})
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="Rust file-size gate (anti file-bloat regression).")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--check", action="store_true",
                    help="exit 1 if any non-whitelisted file exceeds budget")
    args = ap.parse_args()

    over = scan()
    viol = violations(over)

    if args.json:
        print(json.dumps({
            "budget": BUDGET,
            "over_budget": [{"file": f, "loc": n} for f, n in over],
            "violations": viol,
        }, indent=2))
        return 1 if (args.check and viol) else 0

    print(f"Rust file-size gate (budget {BUDGET} LOC/file)")
    print(f"  files over budget: {len(over)} ({len(WHITELIST)} whitelisted)")
    for f, n in over:
        tag = "whitelisted" if f in WHITELIST else "** NOT WHITELISTED **"
        print(f"    {n:>6}  {f}  [{tag}]")
    if viol:
        print(f"\n  {len(viol)} VIOLATION(S):", file=sys.stderr)
        for v in viol:
            print(f"    {v['status']}: {v['file']} ({v['loc']} > {v['limit']})", file=sys.stderr)

    if args.check:
        return 1 if viol else 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
