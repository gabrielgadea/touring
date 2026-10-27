#!/usr/bin/env python3
"""SessionStart hook (R5, opt-in): inject a `touring investigate` topic map.

Default OFF — set ``TOURING_INVESTIGATE_ON_START=1`` to enable. When enabled, runs
``touring investigate <topic> --brief`` (topic = ``$TOURING_INVESTIGATE_TOPIC`` or
the current project/cwd basename) and injects the compact map as SessionStart
``additionalContext``, orienting the agent to where the session subject lives.

Keeping it opt-in honors REGRA #16 (anti-pollution): a normal session pays only a
no-op process spawn; the topic map appears only when explicitly requested.

Fail-open: any error prints nothing and exits 0 — a SessionStart hook must never
block or break session startup.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

HOOK_NAME = "SessionStart"
INVESTIGATE_TIMEOUT = 8.0


def enabled() -> bool:
    """True only when the opt-in env flag is explicitly set to ``1``."""
    return os.environ.get("TOURING_INVESTIGATE_ON_START", "0") == "1"


def resolve_topic() -> str:
    """The topic to map: ``$TOURING_INVESTIGATE_TOPIC`` or the cwd basename."""
    explicit = os.environ.get("TOURING_INVESTIGATE_TOPIC")
    if explicit:
        return explicit
    return Path.cwd().name or "workspace"


def run_investigate(topic: str) -> str | None:
    """Run ``touring investigate <topic> --brief``; return its digest or None.

    Fail-open: a missing binary, timeout, or non-zero exit yields None so the
    caller injects nothing rather than breaking session startup.
    """
    try:
        proc = subprocess.run(
            ["touring", "investigate", topic, "--brief"],
            capture_output=True,
            text=True,
            timeout=INVESTIGATE_TIMEOUT,
            check=False,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired, OSError):
        return None
    digest = proc.stdout.strip()
    return digest or None


def main() -> int:
    """Inject the topic map when opted in; otherwise stay silent (exit 0)."""
    if not enabled():
        return 0  # opt-in: inert by default, zero session pollution
    topic = resolve_topic()
    digest = run_investigate(topic)
    if not digest:
        return 0  # fail-open: nothing to inject
    context = f"[touring investigate · {topic}] {digest}"
    envelope = {
        "hookSpecificOutput": {
            "hookEventName": HOOK_NAME,
            "additionalContext": context,
        }
    }
    print(json.dumps(envelope))
    return 0


# Interruptor de injecao de contexto (26/10/2026) — client/hooks/injection_policy.py
import sys as _ipsys, pathlib as _ippath
_ip_spec = _ipsys.path
_ip_mod_path = _ippath.Path.home() / ".claude/hooks/injection_policy.py"
if _ip_mod_path.exists():
    import importlib.util as _ipu
    _ip_sp = _ipu.spec_from_file_location("injection_policy", _ip_mod_path)
    _ip_m = _ipu.module_from_spec(_ip_sp)
    _ip_sp.loader.exec_module(_ip_m)
    if not _ip_m.allows("plugin-hook"):
        _ipsys.exit(0)

if __name__ == "__main__":
    sys.exit(main())
