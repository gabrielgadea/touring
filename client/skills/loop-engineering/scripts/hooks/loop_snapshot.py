#!/usr/bin/env python3
"""loop_snapshot.py — PreCompact hook: snapshot active loop state.

So a loop survives context compaction (the LangGraph checkpointer analog): before
compaction, persist the pending subtasks + bundle to Touring memory and append a
resume note to the bundle log. Absolutely fail-open — never blocks compaction.

Invoked by Claude Code as a PreCompact hook (no args). Supports ``--help`` and
``--marker <path>`` (testing override).
"""
from __future__ import annotations

import argparse
import datetime
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from loop_marker import _read, active_marker, pending_subtask_ids, state_key  # noqa: E402


def pending_subtasks(task):
    try:
        proc = subprocess.run(["touring", "decompose", "get", task],
                              capture_output=True, text=True, timeout=30)
    except Exception:  # noqa: BLE001
        return []
    out = (proc.stdout or "").strip()
    if not out.startswith("{"):
        return []
    try:
        subs = json.loads(out).get("subtasks", [])
    except Exception:  # noqa: BLE001
        return []
    return pending_subtask_ids(subs)


def snapshot_outer(marker: dict, ts: str) -> int:
    """Persist the OUTER-phase evidence state under a per-project key so the
    next context knows which manifest artifacts are still missing."""
    gate = Path(__file__).resolve().parent / "loop_outer_gate.py"
    missing, nxt = [], ""
    try:
        proc = subprocess.run(
            [sys.executable, str(gate), "--json", "--no-emit"],
            capture_output=True, text=True, timeout=60,
        )
        report = json.loads(proc.stdout or "{}")
        missing = [m.get("id") for m in report.get("missing", [])]
        nxt = report.get("next_action") or ""
    except Exception:  # noqa: BLE001 — snapshot must never block compaction
        pass
    cwd = marker.get("cwd") or ""
    # Deterministic key (REGRA #17) — see loop_marker.state_key. The old
    # `abs(hash(cwd))` was randomized per process, so this record could never be
    # looked up again; loop_resume.py recomputes THIS string to read it back.
    key = state_key(marker)
    snap = (f"OUTER flow={marker.get('flow')} cwd={cwd} missing={missing} "
            f"next={nxt} bundle={marker.get('bundle')} snapshot_at={ts}")
    try:
        subprocess.run(["touring", "memory", "store", key, snap,
                        "--tier", "semantic", "--type", "reference"],
                       capture_output=True, text=True, timeout=30)
    except Exception:  # noqa: BLE001
        pass
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="Loop PreCompact hook: snapshot loop state.")
    ap.add_argument("--marker", default=None,
                    help="explicit marker path (test override; bypasses cwd scoping)")
    args, _ = ap.parse_known_args(argv)

    # Per-project scoped resolution (same convention as loop_stop_guard.py).
    if args.marker:
        marker = _read(Path(args.marker))
    else:
        _, marker = active_marker()
    if not marker or not marker.get("task"):
        return 0  # no active loop for THIS project → no-op

    task = marker["task"]
    ts = datetime.datetime.now().astimezone().isoformat()

    # OUTER phase (no DAG yet): snapshot the ARTIFACT state, not subtasks — a
    # 'loop-state:OUTER' record would collide across projects and carry no
    # resume value (cross-audit finding F2, 2026-07-23). The gate report says
    # exactly what evidence is still missing after the compaction.
    if marker.get("status") == "outer":
        return snapshot_outer(marker, ts)

    pending = pending_subtasks(task)
    ready = ",".join(pending[:8])
    snap = (f"loop-state pending=[{ready}] bundle={marker.get('bundle')} "
            f"scope={marker.get('scope')} snapshot_at={ts}. "
            f"Resume: touring decompose ready {task}")
    try:
        subprocess.run(["touring", "memory", "store", state_key(marker), snap,
                        "--tier", "semantic", "--type", "reference"],
                       capture_output=True, text=True, timeout=30)
    except Exception:  # noqa: BLE001
        pass

    bundle = marker.get("bundle")
    if bundle:
        log = Path(bundle) / "log.md"
        try:
            # `loop_diagnose` creates log.md when the bundle is born, so this
            # append now actually lands; before 2026-08-02 nothing ever created
            # the file and the whole human-readable leg silently never engaged.
            if log.exists():
                log.write_text(log.read_text()
                               + f"\n## {ts} — PreCompact snapshot\n\n"
                               + f"Loop active. Pending: [{ready}]. "
                               + f"Resume: `touring decompose ready {task}`.\n")
        except Exception:  # noqa: BLE001
            pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
