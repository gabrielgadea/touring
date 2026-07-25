#!/usr/bin/env python3
"""loop_outer_arm.py — UserPromptSubmit hook: arm the OUTER-phase marker when a
gated flow is invoked.

Origin (2026-07-23, Gabriel): protocol steps in the loop-engineering OUTER phase
were skipped silently because nothing was armed until step 11 (post-human-gate).
Persuasion (skill instructions, cli-suggest nudges) demonstrably does not change
behavior — affordance and structural gates do (touring-4-pillars cont.¹⁰, thesis
①). This hook is the structural half: the moment the user invokes a gated flow,
a marker with status="outer" is written DETERMINISTICALLY — no orchestrator
discipline involved — and from then on loop_stop_guard.py verifies the flow's
artifact manifest (loop_outer_gate.py) before any turn is allowed to end.

Detection is textual on the submitted prompt (slash command or the
<command-name> tag Claude Code injects for skill invocations). Mapping:

  /loop-engineering , /goal        → flow "strategy-outer"
  /TACO-cross-audit                → flow "cross-audit"

Rules:
  * an existing REAL loop (marker with a DAG task, status "active") is never
    overwritten — arming only applies before a loop exists;
  * an existing outer marker for the same flow is refreshed (updated_at), its
    created_at is preserved so artifact mtime floors stay honest;
  * absolutely fail-open: any error → exit 0, no output, session unaffected.

Emits hookSpecificOutput.additionalContext so the orchestrator SEES the armed
contract (dense + specific, per the injection-density invariant).
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from loop_marker import active_marker, write_marker  # noqa: E402

# Pattern → flow key. ONLY genuine invocation forms match: a slash command
# anchored at the start of the prompt, or the <command-name> tag Claude Code
# injects for skill invocations. Prose mentions ("o skill loop-engineering
# ficou ótimo", "qual é o /goal disso?") must NEVER arm — the cross-audit of
# 2026-07-23 proved the earlier loose pattern (optional slash, any position)
# armed on both.
def _invocation(name: str) -> re.Pattern[str]:
    return re.compile(
        rf"(?:^\s*/{name}\b|<command-name>\s*/?{name}\s*</command-name>)",
        re.IGNORECASE,
    )


FLOW_PATTERNS = (
    (_invocation("loop-engineering"), "strategy-outer"),
    (_invocation("goal"), "strategy-outer"),
    (_invocation("TACO-cross-audit"), "cross-audit"),
)


def detect_flow(prompt: str):
    """Return the flow key the prompt invokes, or None."""
    for pattern, flow in FLOW_PATTERNS:
        if pattern.search(prompt or ""):
            return flow
    return None


def arm(cwd: str, flow: str):
    """Write/refresh the outer marker unless a real loop is already active."""
    _, existing = active_marker(cwd)
    if existing and existing.get("status") == "active" and existing.get("task") not in (None, "", "OUTER"):
        return None  # a live loop owns this project — never clobber it
    return write_marker(task="OUTER", scope=cwd, bundle=(existing or {}).get("bundle"),
                        cwd=cwd, status="outer", flow=flow)


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except Exception:  # noqa: BLE001 — fail-open
        return 0
    prompt = str(payload.get("prompt") or "")
    cwd = str(payload.get("cwd") or "") or None
    # A payload without cwd is malformed — NEVER fall back to the process
    # environment (CLAUDE_PROJECT_DIR/getcwd): during the 2026-07-23 cross-audit
    # that fallback silently re-armed the REAL project's marker from a test
    # payload (finding F5). No cwd → no arming, fail-safe.
    if not cwd:
        return 0
    flow = detect_flow(prompt)
    if not flow:
        return 0
    try:
        marker = arm(cwd, flow)
    except Exception:  # noqa: BLE001 — fail-open
        return 0
    if marker is None:
        return 0
    context = (
        f"[FLOW GUARD] flow '{flow}' armed (marker: {marker}). The Stop hook now "
        f"verifies this flow's artifact manifest (flow_manifests.json) before any "
        f"turn may end — artifacts on disk, never narrative. For 'strategy-outer' "
        f"run the deterministic OUTER in one command: touring adw from-template "
        f"strategy-loop 2>/dev/null; touring adw run strategy-loop "
        f"--var topic='<topic>' --var scope='{cwd}' --var bundle='<plan bundle dir>'. "
        f"Skipped steps will surface as Stop blocks with the exact next_action."
    )
    print(json.dumps({"hookSpecificOutput": {
        "hookEventName": "UserPromptSubmit", "additionalContext": context}}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
