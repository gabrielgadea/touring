#!/usr/bin/env python3
# #tags: kind:script lang:python purpose:kv-cache-hygiene domain:hooks process:w6 status:active
"""W6 D9 — measure which hook injections break the provider prefix cache (P21).

Postmortem evidence (dsh `compaction-summary-prefix-cache-reuse`): ONE varying
`system` segment invalidated the provider's entire KV cache; the fix moved the
variable part to the END. Our hooks inject content on every SessionStart and
UserPromptSubmit — any byte that varies between two identical invocations is
paid on EVERY turn of EVERY session.

Method: run each registered SessionStart/UserPromptSubmit hook command twice
with an IDENTICAL frozen stdin payload, diff the outputs line-by-line, and
report the unstable lines per hook. A hook whose output differs with no real
state change is a cache breaker.

Security posture: commands are read EXCLUSIVELY from the user's own
`~/.claude/settings.json` — the same strings the Claude Code harness itself
executes on every hook event. Nothing from argv/stdin/network ever reaches
the command line; this audit merely re-runs the user's registered hooks.

Usage:
    python3 scripts/kv_cache_audit.py [--json] [--assert-stable]
`--assert-stable` exits 1 when any hook is unstable (the CI gate).
"""
from __future__ import annotations

import argparse
import difflib
import json
import pathlib
import subprocess
import sys

SETTINGS = pathlib.Path.home() / ".claude/settings.json"
EVENTS = ("SessionStart", "UserPromptSubmit")
FROZEN_PAYLOAD = json.dumps(
    {
        "session_id": "kvaudit-frozen",
        "cwd": str(pathlib.Path.home()),
        "hook_event_name": "audit",
        "prompt": "kv-cache audit probe",
        "source": "startup",
    }
)


def hook_commands() -> list[tuple[str, str]]:
    """Hook command strings from the user's OWN settings.json (trusted source:
    these exact strings are what the harness runs on every hook event)."""
    try:
        cfg = json.loads(SETTINGS.read_text())
    except (OSError, json.JSONDecodeError) as e:
        print(f"cannot read {SETTINGS}: {e}", file=sys.stderr)
        return []
    out: list[tuple[str, str]] = []
    for event in EVENTS:
        for matcher in cfg.get("hooks", {}).get(event, []):
            for h in matcher.get("hooks", []):
                if h.get("type") == "command" and h.get("command"):
                    out.append((event, h["command"]))
    return out


def run_once(command: str) -> str:
    """Execute one registered hook exactly as the harness would (bash -c).
    Argv-list form (no shell=True); `command` comes only from settings.json."""
    try:
        r = subprocess.run(
            ["/bin/bash", "-c", command],
            input=FROZEN_PAYLOAD,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        return r.stdout
    except subprocess.TimeoutExpired:
        return "<timeout>"
    except OSError as e:
        return f"<oserror: {e}>"


def audit() -> dict:
    report = []
    for event, command in hook_commands():
        a, b = run_once(command), run_once(command)
        if a == b:
            report.append({"event": event, "command": command[:90], "stable": True})
            continue
        unstable = [
            line
            for line in difflib.unified_diff(a.splitlines(), b.splitlines(), lineterm="", n=0)
            if line.startswith(("+", "-")) and not line.startswith(("+++", "---"))
        ]
        report.append(
            {
                "event": event,
                "command": command[:90],
                "stable": False,
                "unstable_lines": unstable[:12],
            }
        )
    unstable_count = sum(1 for r in report if not r["stable"])
    return {"hooks": len(report), "unstable": unstable_count, "report": report}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--assert-stable", action="store_true")
    args = ap.parse_args()
    result = audit()
    if args.json:
        print(json.dumps(result, indent=1))
    else:
        print(f"hooks audited: {result['hooks']} | unstable: {result['unstable']}")
        for r in result["report"]:
            if not r["stable"]:
                print(f"  UNSTABLE [{r['event']}] {r['command']}")
                for line in r.get("unstable_lines", [])[:6]:
                    print(f"    {line[:140]}")
    if args.assert_stable and result["unstable"]:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
