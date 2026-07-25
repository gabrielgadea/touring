#!/usr/bin/env python3
"""loop_stop_guard.py — Stop hook: converge-or-continue for an active loop.

Hardened 2026-07-02 against three design defects (see ``loop_marker.py``):

  1. **Per-project scope.** The active marker is resolved via
     ``loop_marker.active_marker()`` keyed by the session cwd, so a Stop event in
     project B never gates on project A's loop (no cross-project bleed, no
     last-writer-wins clobber between concurrent runs).
  2. **Fail-OPEN over a dead/orphaned DAG.** The old guard ran
     ``loop_converged.py`` unconditionally; its ``dag_done`` clause is fail-CLOSED
     over a missing DAG, so once the task vanished from the daemon the guard
     blocked *forever*. Now the DAG is inspected FIRST: we only run the
     convergence gate — and only ever BLOCK — when the daemon POSITIVELY confirms
     the task exists with pending subtasks (a live run). Task-gone → archive +
     release. Undeterminable (daemon down) → release.
  3. **Convergence cleans up.** On convergence (or an orphaned DAG) the marker is
     archived to a ``.converged.json`` / ``.archived.json`` sidecar so the hook
     goes inert instead of holding future Stop events hostage.

Absolutely fail-open: no active marker, any error, or the continuation cap →
exit 0 (allow the session to stop). Supports ``--help`` (smoke) and
``--marker <path>`` (test override, bypasses cwd scoping).
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from loop_marker import active_marker, archive_marker, save_marker, _read  # noqa: E402

CONVERGED = Path(__file__).resolve().parent.parent / "loop_converged.py"
OUTER_GATE = Path(__file__).resolve().parent / "loop_outer_gate.py"
MAX_CONTINUATIONS = 30
OUTER_MAX_CONTINUATIONS = 5  # manifest may lower it; the OUTER phase is short


def dag_pending(task: str):
    """Return ``(exists, pending_count)`` for a task's DAG, or ``None`` if the
    state cannot be determined (daemon down / non-JSON / error).

    ``None`` and "does not exist" both drive the guard to fail-OPEN; only a
    POSITIVE ``(True, n>0)`` lets it block. This is the core of defect-#2's fix:
    the guard never blocks on ambiguity, only on a confirmed live run."""
    try:
        proc = subprocess.run(["touring", "decompose", "get", task],
                              capture_output=True, text=True, timeout=30)
    except Exception:  # noqa: BLE001 — fail-open
        return None
    out = (proc.stdout or "").strip()
    if not out.startswith("{"):
        return None  # no structured response (daemon down / error text) → undeterminable
    try:
        data = json.loads(out)
    except Exception:  # noqa: BLE001
        return None
    # A missing task answers `{"subtask_count":0,"subtasks":[],"task":null}` (no
    # "error" key) — so a null/absent "task" is the orphan discriminator, not just
    # "error". Without this, a gone DAG reads as (True, 0) and its marker is never
    # archived (defect #3 cleanup missed).
    if data.get("error") or not data.get("task"):
        return (False, 0)  # daemon answered but no live task (task:null / error) → orphan
    subs = data.get("subtasks")
    if subs is None:
        return (False, 0)  # structured envelope without a subtask list → not a live DAG
    pending = [s for s in subs
               if str(s.get("status")) not in ("done", "finalized")]
    return (True, len(pending))


def run_converged(task: str, marker: dict):
    """Run ``loop_converged.py``; return ``(returncode, report)`` or ``(None, {})``
    if it could not be run (→ fail-open)."""
    cmd = [sys.executable, str(CONVERGED), "--task", task,
           "--scope", marker.get("scope", "."), "--json"]
    if marker.get("bundle"):
        cmd += ["--bundle", marker["bundle"]]
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=90)
    except Exception:  # noqa: BLE001 — fail-open
        return None, {}
    report = _read_json_str(proc.stdout) or {}
    return proc.returncode, report


def _read_json_str(text):
    try:
        return json.loads(text)
    except Exception:  # noqa: BLE001
        return None


def outer_phase_gate(path, marker):
    """Converge-or-continue for the OUTER phase: verdict = artifacts on disk
    (loop_outer_gate.py + flow_manifests.json), never narrative (ADW Law L3).

    Complete manifest → allow stop (the human gate is a legitimate stop).
    Incomplete → block with the missing artifacts + exact next_action, capped at
    the manifest's max_continuations. Any failure to evaluate → fail-open."""
    cmd = [sys.executable, str(OUTER_GATE), "--marker", str(path), "--json"]
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
    except Exception:  # noqa: BLE001 — fail-open
        return 0
    report = _read_json_str(proc.stdout) or {}
    if not report.get("applicable"):
        return 0
    if report.get("complete"):
        if not marker.get("outer_complete"):
            marker["outer_complete"] = True
            save_marker(path, marker)
        return 0  # evidence complete → the turn may end (human gate)
    cap = int(report.get("max_continuations") or OUTER_MAX_CONTINUATIONS)
    count = int(marker.get("continuations", 0)) + 1
    if count > cap:
        print(f"loop-stop-guard: OUTER continuation cap ({cap}) reached — allowing stop",
              file=sys.stderr)
        return 0
    marker["continuations"] = count
    save_marker(path, marker)
    missing = [m.get("id") for m in report.get("missing", [])]
    reason = (f"Flow guard [{report.get('flow')}]: OUTER evidence incomplete "
              f"({count}/{cap}). Missing artifacts: {missing}. "
              f"Next action: {report.get('next_action')}")
    print(json.dumps({"decision": "block", "reason": reason}))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(
        description="Loop Stop hook: block stop until the loop converges (per-project scoped).")
    ap.add_argument("--marker", default=None,
                    help="explicit marker path (test override; bypasses cwd scoping)")
    args, _ = ap.parse_known_args(argv)

    # Resolve the marker: explicit path for tests, else the cwd-scoped active one.
    if args.marker:
        path = Path(args.marker)
        marker = _read(path)
        if not marker or not marker.get("task") or marker.get("status") in (
                "CONVERGED", "ARCHIVED", "ABANDONED"):
            return 0
    else:
        path, marker = active_marker()
        if not marker:
            return 0  # no active loop for THIS project → allow stop

    # OUTER phase (marker armed at flow invocation by loop_outer_arm.py): gate on
    # the flow's artifact manifest, never on a DAG — no DAG exists yet at this
    # stage, and the generic path below would archive the marker as an orphan.
    if marker.get("status") == "outer":
        return outer_phase_gate(path, marker)

    task = marker["task"]

    # Defect #2: inspect the DAG FIRST. Only a confirmed live run can block.
    dag = dag_pending(task)
    if dag is None:
        return 0  # undeterminable (daemon down) → fail-open, keep marker
    exists, pending = dag
    if not exists:
        # Orphaned DAG (task gone from the daemon) → release + archive (defect #3).
        archive_marker(path, marker, status="ARCHIVED")
        return 0

    rc, report = run_converged(task, marker)
    if rc is None:
        return 0  # gate could not run → fail-open
    if rc == 0:
        archive_marker(path, marker, status="CONVERGED")  # converged → clean up + allow stop
        return 0
    if pending == 0:
        # No ready subtask to continue on: adding a phase is a deliberate
        # orchestrator act, not something the Stop hook should force.
        return 0

    # Live run, pending work, not converged → converge-or-continue (the one block).
    count = int(marker.get("continuations", 0)) + 1
    if count > MAX_CONTINUATIONS:
        print(f"loop-stop-guard: continuation cap ({MAX_CONTINUATIONS}) reached — allowing stop",
              file=sys.stderr)
        return 0
    marker["continuations"] = count
    save_marker(path, marker)

    nxt = report.get("next_action") or "continue the loop"
    unmet = report.get("unmet", [])
    reason = (f"Loop Engineering: not converged ({count}/{MAX_CONTINUATIONS}). "
              f"Unmet clauses: {unmet}. Next action: {nxt}")
    print(json.dumps({"decision": "block", "reason": reason}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
