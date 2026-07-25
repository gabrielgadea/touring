#!/usr/bin/env python3
"""loop_outer_gate.py — deterministic artifact gate for a gated flow's OUTER phase.

Origin (2026-07-23, Gabriel): the loop-engineering Stop guard only engaged once a
marker with a real DAG task existed (written at step 11, AFTER the human gate) —
so the whole OUTER phase (steps 1-9: recall, deep diagnostic, CCE exploration,
strategy persistence) ran with no enforcement at all, and steps were skipped
silently. This gate closes that hole: it verifies the flow's manifest of
REQUIRED ARTIFACTS on disk (files + mtimes — never the orchestrator's narrative,
ADW Law L3) and reports what is missing with a concrete next_action.

Contract:
  * marker.status == "outer" (armed by loop_outer_arm.py at flow invocation)
  * marker.flow selects the manifest in flow_manifests.json (default strategy-outer)
  * an artifact counts only when its glob matches >= min files with
    mtime >= marker.created_at - SLACK (produced DURING this flow, not stale)
  * every evaluation appends one JSONL record to compliance.jsonl (the E3
    measurement feed for the touring.flow KPI)

Exit codes: 0 = complete OR not applicable (no outer marker / unknown flow —
fail-open); 1 = incomplete (missing artifacts listed in JSON). The caller
(loop_stop_guard.py) turns exit 1 into a Stop block; this script never blocks
by itself.

Usage:
  loop_outer_gate.py [--marker <path>] [--manifests <path>] [--json] [--no-emit]
"""
from __future__ import annotations

import argparse
import glob as globmod
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from loop_marker import MARKER_DIR, active_marker, _read  # noqa: E402

MANIFESTS = Path(__file__).resolve().parent / "flow_manifests.json"
COMPLIANCE_LOG = MARKER_DIR / "compliance.jsonl"
MTIME_SLACK_SECONDS = 120  # artifacts written moments before arming still count


def _resolve_glob(pattern: str, scope: str, bundle) -> str | None:
    """Fill {scope}/{bundle} placeholders; None when the pattern needs a bundle
    that the marker does not carry yet (the artifact then reports as missing)."""
    if "{bundle}" in pattern and not bundle:
        return None
    return pattern.replace("{scope}", scope or ".").replace("{bundle}", bundle or "")


def _fill(text: str, scope: str, bundle) -> str:
    return (text or "").replace("{scope}", scope or ".").replace(
        "{bundle}", bundle or "<bundle — run strategy-loop to register it>")


def evaluate(marker: dict, manifests: dict) -> dict:
    """Check every manifest artifact against disk; deterministic, narrative-free."""
    flow = marker.get("flow") or "strategy-outer"
    manifest = manifests.get(flow)
    if not isinstance(manifest, dict):
        return {"applicable": False, "flow": flow, "reason": "unknown flow — fail-open"}
    scope = marker.get("scope") or marker.get("cwd") or "."
    bundle = marker.get("bundle")
    floor = float(marker.get("created_at") or 0) - MTIME_SLACK_SECONDS
    missing, present = [], []
    for art in manifest.get("artifacts", []):
        pattern = _resolve_glob(str(art.get("glob", "")), scope, bundle)
        hits = []
        if pattern:
            try:
                hits = [p for p in globmod.glob(pattern)
                        if Path(p).stat().st_mtime >= floor]
            except Exception:  # noqa: BLE001 — unreadable path = no hit
                hits = []
        if len(hits) >= int(art.get("min", 1)):
            present.append({"id": art.get("id"), "files": sorted(hits)[-3:]})
        else:
            missing.append({"id": art.get("id"),
                            "next_action": _fill(art.get("next_action", ""), scope, bundle)})
    complete = not missing
    next_action = (missing[0]["next_action"] if missing
                   else _fill(manifest.get("preferred_next", ""), scope, bundle))
    return {
        "applicable": True, "flow": flow, "complete": complete,
        "expected": len(manifest.get("artifacts", [])),
        "present": present, "missing": missing, "next_action": next_action,
        "max_continuations": int(manifest.get("max_continuations", 5)),
    }


def emit_compliance(marker: dict, report: dict) -> None:
    """Append the E3 measurement record; one line per evaluation, fail-open."""
    try:
        COMPLIANCE_LOG.parent.mkdir(parents=True, exist_ok=True)
        record = {
            "ts": time.time(),
            "cwd": marker.get("cwd"),
            "flow": report.get("flow"),
            "complete": bool(report.get("complete")),
            "expected": report.get("expected", 0),
            "missing": [m.get("id") for m in report.get("missing", [])],
        }
        with COMPLIANCE_LOG.open("a") as fh:
            fh.write(json.dumps(record) + "\n")
        _trim_log()
    except Exception:  # noqa: BLE001 — measurement must never break the gate
        pass


def _trim_log(max_lines: int = 2000, keep: int = 1000) -> None:
    """Bound the compliance log: past `max_lines`, keep only the newest `keep`.
    The KPI is a recent-behavior meter, not an archive — unbounded growth was
    cross-audit finding F4 (2026-07-23)."""
    try:
        lines = COMPLIANCE_LOG.read_text().splitlines(keepends=True)
        if len(lines) > max_lines:
            COMPLIANCE_LOG.write_text("".join(lines[-keep:]))
    except Exception:  # noqa: BLE001
        pass


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Deterministic OUTER-phase artifact gate.")
    ap.add_argument("--marker", default=None,
                    help="explicit marker path (test override; bypasses cwd scoping)")
    ap.add_argument("--manifests", default=str(MANIFESTS))
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--no-emit", action="store_true",
                    help="skip the compliance.jsonl record (dry evaluation)")
    args = ap.parse_args(argv)

    if args.marker:
        marker = _read(Path(args.marker))
    else:
        _, marker = active_marker()
    if not marker or marker.get("status") != "outer":
        print(json.dumps({"applicable": False, "reason": "no outer marker"}))
        return 0

    manifests = _read(Path(args.manifests)) or {}
    report = evaluate(marker, manifests)
    if report.get("applicable") and not args.no_emit:
        emit_compliance(marker, report)
    print(json.dumps(report, indent=None if args.json else 2))
    if report.get("applicable") and not report.get("complete"):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
