#!/usr/bin/env python3
"""loop_resume.py — SessionStart/PostCompact hook: re-inject this loop's state.

The checkpointer was **write-only**. ``loop_snapshot`` persisted state on
PreCompact and NOTHING ever read it back: a grep for readers across
``~/.claude/hooks`` and the Rust crates returned only the compliance-KPI reader
and comments. Recovery existed solely as prose in SKILL.md ("on resume, run
`touring decompose ready`…"), i.e. as *persuasion* — the exact mechanism the
2026-07-23 protocol-adherence diagnosis proved does not survive a compaction
(root cause C4, "prior-zero pós-compactação"). So the marker kept *enforcing* a
loop the next context had no memory of.

Design choice — **live evidence beats a stored snapshot**:

* an OUTER marker is resolved against ``loop_outer_gate.py`` (artifacts on disk,
  recomputed now), so the injection can never claim a stale "missing" set;
* an active loop is resolved against ``touring decompose ready`` (the
  authoritative DAG);
* the memory snapshot is *enrichment*, looked up by the deterministic
  ``loop_marker.state_key`` — which only became possible once that key stopped
  using ``hash()`` (randomized per process, so every record was unrecallable).

Absolutely fail-open: any error, no marker, or a slow subprocess → exit 0 with no
output. It must never delay or break a session start.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from loop_marker import active_marker, pending_subtask_ids, state_key  # noqa: E402

OUTER_GATE = Path(__file__).resolve().parent / "loop_outer_gate.py"

# Events whose `hookSpecificOutput` carries an `additionalContext` channel.
#
# The harness validates that object as a discriminated union on
# `hookEventName`, and **PostCompact is not a member** — so echoing the payload's
# event name back (as this hook did until 08/08/2026) made every
# post-compaction run fail with `(root): Invalid input`: the state was computed
# correctly and then thrown away by the validator. SessionStart is a member
# empirically (its injections are accepted on every session start, including
# `source: "compact"`), which is also why the same script succeeded on one of
# its two registrations and failed on the other.
CONTEXT_EVENTS = frozenset({
    "SessionStart", "UserPromptSubmit", "PostToolUse", "PostToolBatch",
    "Stop", "SubagentStop",
})


def _run(cmd, timeout=45):
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except Exception:  # noqa: BLE001 — fail-open
        return None
    return proc.stdout or ""


def outer_state(marker: dict, marker_path=None):
    """Missing artifacts + next action, recomputed from disk (never narrative).

    The marker PATH is threaded through so the gate evaluates the marker we
    already resolved. Letting it re-resolve from its own cwd made the two
    disagree whenever the hook payload's cwd differed from the process cwd —
    which is exactly what an explicit ``--marker`` invocation looks like.
    """
    cmd = [sys.executable, str(OUTER_GATE), "--json", "--no-emit"]
    if marker_path:
        cmd += ["--marker", str(marker_path)]
    out = _run(cmd, timeout=60)
    try:
        report = json.loads(out or "{}")
    except Exception:  # noqa: BLE001
        return None
    if not report.get("applicable"):
        return None
    return {
        "flow": report.get("flow") or marker.get("flow"),
        "complete": bool(report.get("complete")),
        "missing": [m.get("id") for m in report.get("missing", [])],
        "next_action": report.get("next_action") or "",
    }


def dag_state(task: str):
    """Pending subtasks straight from the DAG — the authoritative progress."""
    out = _run(["touring", "decompose", "get", task], timeout=45)
    if not (out or "").strip().startswith("{"):
        return None
    try:
        data = json.loads(out)
    except Exception:  # noqa: BLE001
        return None
    if data.get("error") or not data.get("task"):
        return None
    subs = data.get("subtasks") or []
    return {"pending": pending_subtask_ids(subs), "total": len(subs)}


def snapshot_note(marker: dict):
    """The PreCompact snapshot, fetched by its DETERMINISTIC key (else None)."""
    key = state_key(marker)
    out = _run(["touring", "memory", "recall", key], timeout=45)
    try:
        entries = json.loads(out or "{}").get("entries", [])
    except Exception:  # noqa: BLE001
        return None
    for e in entries:
        if str(e.get("key")) == key:
            return str(e.get("value", ""))[:400]
    return None


def build_context(marker: dict, marker_path=None) -> str | None:
    """Dense, specific, MUST/SHOULD-structured — never a generic banner."""
    bundle = marker.get("bundle") or "<no bundle registered>"
    scope = marker.get("scope") or marker.get("cwd") or "."
    lines = []
    if marker.get("status") == "outer":
        st = outer_state(marker, marker_path)
        if not st or st["complete"]:
            return None  # nothing owed → stay silent
        lines.append(
            f"[LOOP RESUME] flow '{st['flow']}' is still armed for this project and "
            f"session, and its artifact manifest is UNMET: missing "
            f"{', '.join(st['missing'])}. The Stop hook will refuse to end a turn "
            f"until these exist on disk (ADW Law L3)."
        )
        if st["next_action"]:
            lines.append(f"MUST → {st['next_action']}")
        lines.append(
            f"MUST → one command satisfies the whole OUTER: touring adw run "
            f"strategy-loop --var topic='<the goal of this turn>' "
            f"--var scope='{scope}' --var bundle='{bundle}'"
        )
    else:
        task = marker.get("task")
        st = dag_state(task)
        if not st:
            return None  # orphaned/vanished DAG → fail-open, say nothing
        if not st["pending"]:
            return None
        shown = ", ".join(st["pending"][:8])
        lines.append(
            f"[LOOP RESUME] loop '{task}' is active with "
            f"{len(st['pending'])}/{st['total']} subtasks pending: {shown}."
        )
        lines.append(f"MUST → touring decompose ready {task}   # next topological subtask")
        lines.append(
            f"MUST → convergence is measured, not asserted: python3 "
            f"~/.claude/skills/loop-engineering/scripts/loop_converged.py --task {task} "
            f"--scope '{scope}' --bundle '{bundle}'  (exit 0 is the only \"done\")"
        )
    note = snapshot_note(marker)
    if note:
        lines.append(f"SHOULD → last PreCompact snapshot: {note}")
    lines.append(f"SHOULD → touring memory recall \"{state_key(marker)}\"   # full state history")
    return "\n".join(lines)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Loop SessionStart/PostCompact hook: re-inject loop state.")
    ap.add_argument("--marker", default=None, help="explicit marker path (test override)")
    args, _ = ap.parse_known_args(argv)

    payload = {}
    try:
        raw = sys.stdin.read()
        payload = json.loads(raw) if raw.strip() else {}
    except Exception:  # noqa: BLE001 — a stdin-less invocation is still valid
        payload = {}

    try:
        if args.marker:
            path = Path(args.marker)
            marker = json.loads(path.read_text())
        else:
            path, marker = active_marker(payload.get("cwd") or None,
                                         payload.get("session_id") or None)
        if not marker or not marker.get("task"):
            return 0
        context = build_context(marker, path)
    except Exception:  # noqa: BLE001 — fail-open
        return 0
    if not context:
        return 0
    event = str(payload.get("hook_event_name") or "SessionStart")
    if event in CONTEXT_EVENTS:
        print(json.dumps({"hookSpecificOutput": {
            "hookEventName": event, "additionalContext": context}}))
    else:
        # No context channel for this event (PostCompact). `systemMessage` is a
        # ROOT-level field valid for every event, so the state still surfaces
        # instead of being discarded by a validation error. The model-facing
        # injection on the same transition comes from SessionStart(source=
        # "compact"), which this same script serves.
        print(json.dumps({"systemMessage": context}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
