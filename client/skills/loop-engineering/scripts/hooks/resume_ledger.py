#!/usr/bin/env python3
"""resume_ledger.py — the resume-uptake ruler (N4, 2026-09-23).

``loop_resume.py`` injected loop state at every SessionStart/PostCompact and
NOTHING measured whether the injection changed anything — "the resume works"
was faith, not a ruler. Every injection lands here as a JSONL record, every
loop action (phase-close, …) lands beside it, and ``loop_resume.py --uptake``
answers followed/injected per ledger. An injection counts as followed only
when a LATER loop-action lands in the SAME session within the window. A
missing or empty ledger is ``{"available": false}``: absence is unknown,
never zero (Lei L2). Every write is fail-open — telemetry never breaks a hook.
"""
from __future__ import annotations

import json
import os
import time
from pathlib import Path

UPTAKE_WINDOW_SECS = 72 * 3600


def ledger_path() -> Path:
    """Canonical ledger path; LOOP_RESUME_LEDGER overrides it (tests, audits)."""
    override = os.environ.get("LOOP_RESUME_LEDGER")
    if override:
        return Path(override)
    return Path.home() / ".claude" / "loop-engineering" / "resume.jsonl"


def append_record(rec: dict, path: Path | None = None) -> bool:
    """Best-effort append; False on any error, never an exception."""
    try:
        p = path or ledger_path()
        p.parent.mkdir(parents=True, exist_ok=True)
        with p.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps({"ts": time.time(), **rec}, ensure_ascii=False) + "\n")
        return True
    except Exception:  # noqa: BLE001 — telemetry must never break a hook
        return False


def iter_records(path: Path | None = None):
    p = path or ledger_path()
    try:
        text = p.read_text(encoding="utf-8")
    except Exception:  # noqa: BLE001
        return
    for line in text.splitlines():
        try:
            yield json.loads(line)
        except Exception:  # noqa: BLE001
            continue


def uptake(path: Path | None = None, window_secs: float = UPTAKE_WINDOW_SECS) -> dict:
    """followed/injections over the ledger — per the same-session, later-action
    rule; `available: false` when there is nothing honest to measure."""
    records = list(iter_records(path))
    if not records:
        return {"available": False, "reason": "no resume ledger yet"}
    injections = [r for r in records if r.get("kind") == "injection"]
    actions = [r for r in records if r.get("kind") == "loop-action"]
    if not injections:
        return {"available": False, "reason": "no injections recorded yet"}
    followed = 0
    for inj in injections:
        sid = inj.get("session_id")
        ts = inj.get("ts") or 0
        if any(
            a.get("session_id") == sid
            and 0 <= (a.get("ts") or 0) - ts <= window_secs
            for a in actions
        ):
            followed += 1
    return {
        "available": True,
        "injections": len(injections),
        "actions": len(actions),
        "followed": followed,
        "uptake": followed / len(injections),
        "window_secs": window_secs,
    }
