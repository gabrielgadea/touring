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
from loop_marker import _read, active_marker, archive_marker, pending_subtask_ids, save_marker  # noqa: E402

CONVERGED = Path(__file__).resolve().parent.parent / "loop_converged.py"
OUTER_GATE = Path(__file__).resolve().parent / "loop_outer_gate.py"
MAX_CONTINUATIONS = 30
OUTER_MAX_CONTINUATIONS = 5  # manifest may lower it; the OUTER phase is short

# The guard's own worst-case wall-clock, declared so a test can assert that the
# timeout it is REGISTERED with in settings.json dominates it. Measured
# 20/08/2026: the guard was registered with timeout=20 while the convergence
# gate alone took 24.0s, so Claude Code killed it before it could ever print a
# verdict. A gate slower than its own timeout is not a gate — it is a gate-shaped
# no-op, and it looks perfectly healthy when run by hand.
SELF_BUDGET_SECONDS = 30 + 90  # dag RPCs + run_converged()'s own timeout


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
    return (True, len(pending_subtask_ids(subs)))


def dag_ready(task: str):
    """Return the list of READY subtask ids, or ``None`` if undeterminable.

    This is the whole blocking decision, and it costs one RPC (measured 0.00s).
    A non-empty ``ready`` set means ``dag_done`` — the first convergence clause —
    is necessarily unmet, so the expensive gate cannot change the verdict; it can
    only make the guard miss its timeout. The full gate still runs when the DAG
    has drained, which is where it decides CONVERGED vs. keep-going."""
    try:
        proc = subprocess.run(["touring", "decompose", "ready", task],
                              capture_output=True, text=True, timeout=30)
    except Exception:  # noqa: BLE001 — fail-open (caller falls back to the gate)
        return None
    data = _read_json_str((proc.stdout or "").strip())
    if not isinstance(data, dict):
        return None
    ready = data.get("ready_subtasks")
    if not isinstance(ready, list):
        return None
    return [str(r.get("subtask_id") or "") for r in ready if isinstance(r, dict)]


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


def _still_awaiting_dag(marker: dict) -> bool:
    """True when the OUTER finished but no real DAG ever took over.

    `loop_outer_arm.py` writes the placeholder task ``"OUTER"`` before any DAG
    exists; step 11 is supposed to replace it with a real ``task_…`` id. A marker
    that still holds the placeholder has, by construction, never gated a phase.
    """
    task = str(marker.get("task") or "")
    return marker.get("status") == "outer" and (not task or task == "OUTER")


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
        first_completion = not marker.get("outer_complete")
        if first_completion:
            marker["outer_complete"] = True
            save_marker(path, marker)
            return 0  # the human gate (step 9) — stopping here is legitimate
        # Every LATER stop with the OUTER still satisfied is a different thing.
        # The human already answered and work resumed, yet the marker is parked
        # at status "outer" with the placeholder task. Step 11 (register the DAG,
        # upgrade to "active") was narrative — nothing made it happen — so the
        # guard silenced permanently the moment the OUTER artifacts existed, and
        # the substantive work was gated by nothing at all. Observed 20/08/2026:
        # the OUTER of a skills-consolidation goal completed, the DAG stayed at
        # subtask_count 0, and the Stop hook allowed every subsequent turn.
        if _still_awaiting_dag(marker):
            cap = int(report.get("max_continuations") or OUTER_MAX_CONTINUATIONS)
            count = int(marker.get("continuations", 0)) + 1
            if count > cap:
                print("loop-stop-guard: OUTER→INNER handoff cap reached — allowing stop",
                      file=sys.stderr)
                return 0
            marker["continuations"] = count
            save_marker(path, marker)
            reason = (
                f"Flow guard [{report.get('flow')}]: OUTER evidence is complete and the "
                f"human gate has been passed, but the loop never entered the INNER "
                f"({count}/{cap}). The marker still carries the placeholder task "
                f"\"OUTER\" — step 11 was never executed, so no phase is being gated. "
                f"Next action: register the real DAG "
                f"(touring decompose create/add …), then "
                f"loop_marker.py write --task <task_id> --scope <path> --bundle "
                f"{marker.get('bundle') or '<bundle>'} — which flips this marker to "
                f"status \"active\" and hands the verdict to loop_converged.py.")
            print(json.dumps({"decision": "block", "reason": reason}))
            return 0
        return 0  # complete and already handed off — nothing owed here
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


def _block(path, marker: dict, unmet, next_action: str) -> int:
    """The one blocking decision, shared by the fast path and the full gate:
    bump the continuation counter, respect the cap, emit converge-or-continue."""
    count = int(marker.get("continuations", 0)) + 1
    if count > MAX_CONTINUATIONS:
        print(f"loop-stop-guard: continuation cap ({MAX_CONTINUATIONS}) reached — allowing stop",
              file=sys.stderr)
        return 0
    marker["continuations"] = count
    save_marker(path, marker)
    reason = (f"Loop Engineering: not converged ({count}/{MAX_CONTINUATIONS}). "
              f"Unmet clauses: {list(unmet)}. Next action: {next_action}")
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

    # FAST PATH (20/08/2026). A non-empty `ready` set settles the verdict on its
    # own: `dag_done` is the first convergence clause, so the gate CANNOT return
    # 0 while a subtask is ready. Running it here bought no information and cost
    # 24s against a 20s registered timeout — the guard was killed mid-flight on
    # every single Stop, which is why the loop never once held a turn. The cheap
    # query is authoritative for the blocking half; the gate stays authoritative
    # for the converged half, where it is affordable because the DAG has drained.
    ready = dag_ready(task)
    if ready:
        names = ", ".join(rid.split("::")[-1] for rid in ready) or "the ready subtask(s)"
        return _block(path, marker, unmet=["dag_done"],
                      next_action=(f"execute pending subtask(s): {names} "
                                   f"(remaining clauses are scored once the DAG drains)"))

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
    return _block(path, marker,
                  unmet=report.get("unmet", []),
                  next_action=report.get("next_action") or "continue the loop")


if __name__ == "__main__":
    sys.exit(main())
