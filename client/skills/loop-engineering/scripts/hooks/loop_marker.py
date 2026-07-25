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

Usable as a library (imported by ``loop_stop_guard.py`` / ``loop_snapshot.py``)
and as a CLI so the orchestrator writes markers with cwd + timestamps guaranteed:

    loop_marker.py write --task <id> --scope <path> [--bundle <dir>] [--status active]
    loop_marker.py show                 # the active marker for this cwd (JSON), if any
    loop_marker.py archive [--status CONVERGED]
    loop_marker.py path                 # print the per-project marker path

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


def _resolve_cwd(cwd=None) -> str:
    raw = cwd or os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd()
    try:
        return str(Path(raw).resolve())
    except Exception:  # noqa: BLE001 — fail-open
        return str(raw)


def _key(cwd: str) -> str:
    return hashlib.sha1(cwd.encode("utf-8")).hexdigest()[:12]


def marker_path(cwd=None) -> Path:
    """Per-project marker path, keyed by resolved cwd (never a global singleton)."""
    return MARKER_DIR / f"active-{_key(_resolve_cwd(cwd))}.json"


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


def active_marker(cwd=None):
    """Return ``(path, data)`` of the active marker scoped to this cwd, else ``(None, None)``.

    Precedence: the per-project marker; then the legacy singleton BUT ONLY when
    its recorded cwd matches this project (defect #2 — never act on another
    project's loop). A finished (CONVERGED/ARCHIVED/ABANDONED) or stale marker is
    treated as inert (defects #1/#3)."""
    resolved = _resolve_cwd(cwd)
    p = marker_path(resolved)
    data = _read(p)
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
    if data.get("status") in FINISHED_STATUSES:
        return None, None
    if is_stale(data):
        return None, None
    return p, data


def write_marker(task, scope, bundle=None, cwd=None, status="active", **extra):
    """Create/refresh the per-project marker with cwd + timestamps guaranteed."""
    MARKER_DIR.mkdir(parents=True, exist_ok=True)
    resolved = _resolve_cwd(cwd)
    p = marker_path(resolved)
    now = time.time()
    prev = _read(p) or {}
    data = {
        "task": task,
        "scope": scope,
        "bundle": bundle,
        "cwd": resolved,
        "status": status,
        "continuations": int(prev.get("continuations", 0)),
        "created_at": prev.get("created_at", now),
        "updated_at": now,
    }
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
    w.add_argument("--flow", default=None,
                   help="gated-flow key (flow_manifests.json) for status=outer markers")

    sub.add_parser("show", help="print the active marker for this cwd (JSON) if any")
    sub.add_parser("path", help="print the per-project marker path")

    a = sub.add_parser("archive", help="retire the active marker for this cwd")
    a.add_argument("--status", default="ARCHIVED")

    args = ap.parse_args(argv)

    if args.cmd == "write":
        extra = {"flow": args.flow} if args.flow else {}
        p = write_marker(args.task, args.scope, args.bundle, cwd=args.cwd,
                         status=args.status, **extra)
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
