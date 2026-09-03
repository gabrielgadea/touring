#!/usr/bin/env python3
"""loop_marker.py — per-project active-loop marker for the Loop Engineering hooks.

Fixes the three design defects of the old global singleton
``~/.claude/loop-engineering/active.json`` (diagnosed 2026-07-02):

  1. **Global singleton → last-writer-wins clobber.** Two loops in different
     projects overwrote each other's marker. Now the marker path is keyed by the
     run's cwd (``active-<sha1(cwd)[:12]>.json``), so concurrent runs never
     collide.
  2. **Global Stop hook acted on ANY active loop.** Now a marker is only "mine"
     if its recorded ``cwd`` matches the current session's cwd — a Stop event in
     project B never gates on project A's loop.
  3. **No convergence/TTL cleanup → eternal block.** A converged-but-uncleaned
     marker (or an orphaned DAG) blocked Stop forever. Now convergence *archives*
     the marker, a ``status`` field short-circuits already-finished runs, and a
     TTL renders a stale marker inert.
  4. **Per-project ≠ per-session → cross-session bleed** (Gabriel, 2026-08-02).
     Keying by cwd alone meant N Claude Code sessions open on the SAME project
     shared ONE marker, so: session B's Stop was held by A's unmet manifest; B
     free-rode on A's artifacts once A completed; the ``continuations`` cap was
     consumed jointly; and A's convergence archived the marker out from under B.
     The key is now ``active-<sha1(cwd)[:12]>-<sha1(session)[:8]>.json``, and a
     marker stamped for another session is never "mine" even if its path is
     reached. Session identity comes from the hook payload's ``session_id``, else
     ``CLAUDE_CODE_SESSION_ID``/``TOURING_SESSION_ID`` (Claude Code exports both
     into every hook's environment, so even stdin-less hooks can scope). With no
     resolvable session the pre-2026-08-02 project-wide behavior is kept exactly,
     so nothing regresses where the identity is unavailable.

Usable as a library (imported by ``loop_stop_guard.py`` / ``loop_snapshot.py``)
and as a CLI so the orchestrator writes markers with cwd + timestamps guaranteed:

    loop_marker.py write --task <id> --scope <path> [--bundle <dir>]
                         [--status active] [--session-id <id>]
    loop_marker.py show                 # this (project, session)'s marker (JSON), if any
    loop_marker.py archive [--status CONVERGED]
    loop_marker.py path                 # print this (project, session)'s marker path

Absolutely fail-open: every helper swallows its own errors so a hook that
imports this module can never crash the session.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import time
from pathlib import Path

# Overridable so tests (and any sandboxed run) never write markers into the
# production directory — pytest subprocess runs were leaving orphan tmp-cwd
# markers in the real dir (finding 2026-07-24).
MARKER_DIR = Path(os.environ.get("LOOP_ENGINEERING_HOME")
                  or Path.home() / ".claude" / "loop-engineering")
LEGACY_MARKER = MARKER_DIR / "active.json"
TTL_SECONDS = 24 * 3600  # a marker not updated in 24h is stale → inert
FINISHED_STATUSES = ("CONVERGED", "ARCHIVED", "ABANDONED")

# The DAG mixes vocabularies: `touring decompose` closes a subtask as
# "completed", while `loop_phase_close` writes "done" and legacy closes wrote
# "finalized". All three are terminal.
#
# This lived as a literal at FOUR call sites and only ONE of them
# (`loop_converged.py`) listed "completed" — so on 08/08/2026 a DAG with 4 of 6
# subtasks closed was reported by the resume hook as "6/6 pending", i.e. the
# next context was told to redo finished work. Single definition so the sites
# cannot drift apart again (decision matrix C08 — cross-caller compare).
TERMINAL_SUBTASK_STATUSES = ("done", "completed", "finalized")


def pending_subtask_ids(subtasks) -> list:
    """Short ids of the subtasks that are NOT in a terminal state.

    `subtasks` is the `subtasks` array of `touring decompose get`. The short id
    is the part after `::` (the full id embeds the parent task).
    """
    return [
        str(s.get("subtask_id", "")).split("::")[-1]
        for s in (subtasks or [])
        if str(s.get("status")) not in TERMINAL_SUBTASK_STATUSES
    ]


def _resolve_cwd(cwd=None) -> str:
    raw = cwd or os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd()
    try:
        return str(Path(raw).resolve())
    except Exception:  # noqa: BLE001 — fail-open
        return str(raw)


def _key(cwd: str) -> str:
    return hashlib.sha1(cwd.encode("utf-8")).hexdigest()[:12]


def resolve_session(session_id=None):
    """This hook invocation's session identity, or ``None`` if unavailable.

    Precedence: an explicit id (the hook payload's ``session_id`` — the most
    authoritative source, used by ``loop_outer_arm``), then the environment.
    Claude Code exports ``CLAUDE_CODE_SESSION_ID`` into every hook process and
    ``session_env_setup.sh`` mirrors the payload into ``TOURING_SESSION_ID``, so
    hooks that never parse stdin (``loop_stop_guard``, ``loop_snapshot``) still
    resolve the SAME id the arming hook used — which is what makes the marker
    findable by its writer and invisible to everyone else.

    ``None`` deliberately means "fall back to project-wide scoping": absent an
    identity we must not invent one, and the pre-2026-08-02 behavior is the
    safe, unchanged default.
    """
    raw = (session_id
           or os.environ.get("CLAUDE_CODE_SESSION_ID")
           or os.environ.get("TOURING_SESSION_ID")
           or "")
    raw = str(raw).strip()
    return raw if raw and raw != "unknown" else None


def _skey(session: str) -> str:
    return hashlib.sha1(session.encode("utf-8")).hexdigest()[:8]


def project_marker_path(cwd=None) -> Path:
    """The pre-2026-08-02 project-wide path — kept for migration/compat only."""
    return MARKER_DIR / f"active-{_key(_resolve_cwd(cwd))}.json"


def marker_path(cwd=None, session_id=None) -> Path:
    """Marker path keyed by (project, session) — never a global singleton, and
    never shared between concurrent Claude Code sessions on the same project."""
    session = resolve_session(session_id)
    if not session:
        return project_marker_path(cwd)
    return MARKER_DIR / f"active-{_key(_resolve_cwd(cwd))}-{_skey(session)}.json"


def state_key(marker: dict) -> str:
    """Canonical memory key for a marker's snapshot — the SAME string for the
    writer (``loop_snapshot``) and the reader (``loop_resume``).

    DETERMINISTIC by construction (REGRA #17: an id is derived from the canonical
    name, never emergent). The previous OUTER key used ``abs(hash(cwd)) % 10**8``;
    Python randomizes ``str`` hashing per process (PYTHONHASHSEED), so three
    consecutive processes produced 86221029 / 375623 / 47015023 for one cwd —
    every compaction stored under a fresh key that nothing could ever look up.

    An active loop keys on its ``task`` (already unique, and the convention of the
    records already in memory); an OUTER phase has no DAG yet, so it keys on the
    (project, session) pair that owns it.
    """
    if marker.get("status") == "outer":
        scope = f"{_key(_resolve_cwd(marker.get('cwd')))}"
        session = marker.get("session_id")
        if session:
            scope = f"{scope}-{_skey(str(session))}"
        return f"flow-state:{marker.get('flow') or 'outer'}:{scope}"
    return f"loop-state:{marker.get('task')}"


def _read(path: Path):
    try:
        return json.loads(path.read_text())
    except Exception:  # noqa: BLE001 — fail-open
        return None


def is_stale(marker: dict) -> bool:
    ts = marker.get("updated_at") or marker.get("created_at") or 0
    try:
        return (time.time() - float(ts)) > TTL_SECONDS
    except Exception:  # noqa: BLE001
        return False


def _claim_unattributed(resolved: str, session: str):
    """One-time migration: adopt a pre-session project-wide marker, or ``(None, None)``.

    Markers written before 2026-08-02 carry no ``session_id``, so no session can
    prove ownership. Dropping them would silently un-gate a loop that is live
    right now, so the first session to evaluate one CLAIMS it: the marker is
    rewritten at the session-scoped path, stamped, and the old file removed.
    "First evaluator wins" is arbitrary but bounded and strictly better than the
    status quo where EVERY session shared it; the losers simply start clean on
    their next prompt. Only unattributed markers are claimable — one already
    stamped for a session is never taken.
    """
    legacy = project_marker_path(resolved)
    data = _read(legacy)
    if not (data and data.get("task")) or data.get("session_id"):
        return None, None
    data["session_id"] = session
    dest = marker_path(resolved, session)
    try:
        MARKER_DIR.mkdir(parents=True, exist_ok=True)
        dest.write_text(json.dumps(data, indent=2))
        legacy.unlink(missing_ok=True)
    except Exception:  # noqa: BLE001 — fail-open
        return None, None
    return dest, data


def active_marker(cwd=None, session_id=None):
    """Return ``(path, data)`` of the marker owned by THIS (project, session), else ``(None, None)``.

    Precedence: the session-scoped marker; then a one-time claim of an
    unattributed project-wide marker (defect #4 migration); then the legacy
    singleton BUT ONLY when its recorded cwd matches this project (defect #2 —
    never act on another project's loop). A marker stamped for a DIFFERENT
    session is never mine (defect #4). A finished
    (CONVERGED/ARCHIVED/ABANDONED) or stale marker is inert (defects #1/#3)."""
    resolved = _resolve_cwd(cwd)
    session = resolve_session(session_id)
    p = marker_path(resolved, session)
    data = _read(p)
    if not (data and data.get("task")) and session:
        p, data = _claim_unattributed(resolved, session)
    if not (data and data.get("task")):
        # Backward-compat: honor the legacy singleton only if it CARRIES its own
        # cwd AND that cwd is THIS project's. A legacy marker without a cwd field
        # (the pre-fix format) is unattributable — ignore it, never guess, else the
        # per-project isolation collapses back to the global-singleton bleed.
        ld = _read(LEGACY_MARKER)
        if ld and ld.get("task") and ld.get("cwd") and _resolve_cwd(ld.get("cwd")) == resolved:
            p, data = LEGACY_MARKER, ld
        else:
            return None, None
    # Defense in depth: the path already separates sessions, but an explicitly
    # foreign stamp is rejected even when the path is reached some other way
    # (a `--marker` override, a hand-copied file, a hash collision).
    owner = data.get("session_id")
    if session and owner and owner != session:
        return None, None
    if data.get("status") in FINISHED_STATUSES:
        return None, None
    if is_stale(data):
        return None, None
    return p, data


def write_marker(task, scope, bundle=None, cwd=None, status="active",
                 session_id=None, **extra):
    """Create/refresh the (project, session) marker with cwd/session/timestamps guaranteed."""
    MARKER_DIR.mkdir(parents=True, exist_ok=True)
    resolved = _resolve_cwd(cwd)
    session = resolve_session(session_id)
    p = marker_path(resolved, session)
    now = time.time()
    prev = _read(p) or {}
    data = {
        "task": task,
        "scope": scope,
        "bundle": bundle,
        "cwd": resolved,
        # Stamped even when None, so a marker is always self-describing: a reader
        # can tell "unattributed (claimable)" from "owned by another session".
        "session_id": session,
        "status": status,
        "continuations": int(prev.get("continuations", 0)),
        "created_at": prev.get("created_at", now),
        # When the CURRENT flow started — distinct from `created_at`, which is
        # when the marker first appeared and which survives every re-arm. The
        # artifact gate uses this as its mtime floor ("produced DURING this
        # flow"), so a marker that persists across days must not drag the floor
        # with it: measured 20/08/2026, a floor 39.4h old accepted five ledgers
        # from unrelated topics, and the Stop hook evaluated 54 times in one
        # session without ever blocking. Re-stamped when the flow CHANGES or
        # when the previous flow's manifest was already satisfied (a new cycle
        # begins) — never on every prompt, which would push the floor to "now"
        # and make the gate unsatisfiable in the other direction.
        "updated_at": now,
    }
    prev_flow = prev.get("flow")
    new_flow = extra.get("flow", prev_flow)
    starts_new_cycle = (prev_flow != new_flow) or bool(prev.get("outer_complete"))
    data["flow_armed_at"] = (
        now if (starts_new_cycle or not prev.get("flow_armed_at"))
        else prev["flow_armed_at"]
    )
    # Carry the flow forward EXPLICITLY. `new_flow` above was computed only to decide
    # `starts_new_cycle`, and `data` receives `flow` solely through `data.update(extra)`
    # below — so a refresh without an explicit flow silently ERASED the armed contract.
    # A flowless marker is not neutral: loop_outer_gate.py reads `marker.get("flow") or
    # "strategy-outer"`, so erasing it PROMOTES the turn from the 2-artifact work-outer
    # to the 5-artifact strategy-outer. Found 03/09/2026 by the positive assertion
    # "armed work-outer + remedy → still work-outer", which returned None.
    if new_flow:
        data["flow"] = new_flow
    data.update(extra)
    try:
        p.write_text(json.dumps(data, indent=2))
    except Exception:  # noqa: BLE001
        pass
    return p


def save_marker(path: Path, data: dict):
    """Persist an in-place mutation (e.g. bumped continuation count), restamping updated_at."""
    data["updated_at"] = time.time()
    try:
        Path(path).write_text(json.dumps(data, indent=2))
    except Exception:  # noqa: BLE001
        pass


def archive_marker(path: Path, data: dict, status="ARCHIVED"):
    """Retire a marker: stamp a terminal status + rename to a ``.<status>.json``
    sidecar so the hooks go inert (defect #3) while an audit trail survives."""
    data = dict(data)
    data["status"] = status
    data["archived_at"] = time.time()
    dest = Path(path).with_suffix(f".{status.lower()}.json")
    try:
        dest.write_text(json.dumps(data, indent=2))
        Path(path).unlink(missing_ok=True)
    except Exception:  # noqa: BLE001
        pass
    return dest


def _main(argv=None):
    ap = argparse.ArgumentParser(description="Per-project Loop Engineering marker.")
    sub = ap.add_subparsers(dest="cmd")

    w = sub.add_parser("write", help="create/refresh the marker for this cwd")
    w.add_argument("--task", required=True)
    w.add_argument("--scope", required=True)
    w.add_argument("--bundle", default=None)
    w.add_argument("--status", default="active")
    w.add_argument("--cwd", default=None)
    w.add_argument("--session-id", default=None,
                   help="owning session (default: CLAUDE_CODE_SESSION_ID / TOURING_SESSION_ID)")
    w.add_argument("--flow", default=None,
                   help="gated-flow key (flow_manifests.json) for status=outer markers")
    w.add_argument("--flow-if-absent", default=None,
                   help="set this flow ONLY when the marker has none — never promote an "
                        "armed contract (see the write branch below)")

    sub.add_parser("show", help="print this (project, session)'s active marker (JSON) if any")
    sub.add_parser("path", help="print this (project, session)'s marker path")

    a = sub.add_parser("archive", help="retire the active marker for this cwd")
    a.add_argument("--status", default="ARCHIVED")

    args = ap.parse_args(argv)

    if args.cmd == "write":
        extra = {"flow": args.flow} if args.flow else {}
        # `--flow-if-absent` exists because the SAME `strategy-loop` ADW is both the
        # body of `strategy-outer` AND the prescribed remedy for `work-outer`. When it
        # ran with a hard `--flow strategy-outer`, executing the remedy REWROTE the
        # armed contract from 2 artifacts to 5 — measured live on 03/09/2026: the Stop
        # hook blocked as `[work-outer] 1/2`, the agent ran the prescribed
        # `touring adw run strategy-loop`, and the next block came back as
        # `[strategy-outer] 3/5`, demanding a strategy-doc that `work-outer` explicitly
        # waives. Complying with the prescription raised the bar — which is what turns a
        # gate into an unwinnable toll booth (compliance.jsonl: runs of up to 63
        # consecutive blocks, and `max_continuations` never bites because the counter
        # is per-marker while the contract moves).
        #
        # `loop_outer_arm` already refuses the symmetric case (the default `work-outer`
        # never DEMOTES an armed `strategy-outer`/`cross-audit`). This closes the
        # promotion half. A human invoking `/loop-engineering` still arms
        # `strategy-outer` through `--flow`, which is unaffected.
        if not extra and args.flow_if_absent:
            _, existing = active_marker(cwd=args.cwd, session_id=args.session_id)
            if not (existing and existing.get("flow")):
                extra = {"flow": args.flow_if_absent}
        p = write_marker(args.task, args.scope, args.bundle, cwd=args.cwd,
                         status=args.status, session_id=args.session_id, **extra)
        print(str(p))
        return 0
    if args.cmd == "path":
        print(str(marker_path()))
        return 0
    if args.cmd == "show":
        _, data = active_marker()
        print(json.dumps(data, indent=2) if data else "{}")
        return 0
    if args.cmd == "archive":
        p, data = active_marker()
        if not data:
            print("{}")
            return 0
        dest = archive_marker(p, data, status=args.status)
        print(str(dest))
        return 0
    ap.print_help()
    return 0


if __name__ == "__main__":
    sys.exit(_main())
