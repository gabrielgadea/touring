#!/usr/bin/env python3
"""scout_perpetuo_reminder.py — SessionStart hook: arm the Scout Perpétuo reflex.

Origin (2026-07-25, Gabriel): the perpetual scout (F1.7) had run only during
acceptance — "perpétuo" in name only, because nothing surfaced it at the moment
work begins. Persuasion does not change behavior; affordance does (thesis ①,
touring-4-pillars). This hook is the affordance: at every session start it
injects (a) WHY the scout matters — it is the demand generator of the software
factory: cycles → tickets → factory route → ADW → RL outcomes — and (b) the
EXACT command already loaded for this project's topics, plus the armed queue of
factory-routed tickets, so firing a cycle before new work costs one paste.

Absolutely fail-open: any error → exit 0, no output, session unaffected.
"""
from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

STATE_DIRNAME = ".touring/scout-perpetuo"
SCRIPT = Path(__file__).resolve().parent.parent / "scout_perpetuo.py"
MAX_WALK_UP = 8
QUEUE_TAIL = 3
CURVE_TAIL = 6


def project_root(start: Path) -> Path | None:
    """First ancestor (incl. start) carrying a `.touring/` — the L4 marker."""
    node = start
    for _ in range(MAX_WALK_UP):
        if (node / ".touring").is_dir():
            return node
        if node.parent == node:
            break
        node = node.parent
    return None


def load_states(root: Path) -> list[dict]:
    """Every scout state file under the project root, parsed leniently."""
    states: list[dict] = []
    state_dir = root / STATE_DIRNAME
    if not state_dir.is_dir():
        return states
    for path in sorted(state_dir.glob("*.json")):
        try:
            states.append(json.loads(path.read_text(encoding="utf-8")))
        except (OSError, json.JSONDecodeError):
            continue
    return states


def topic_lines(state: dict, root: Path, now: float) -> list[str]:
    """Status + armed command for one topic; MUST when the cycle is overdue."""
    topic = str(state.get("topic", "?"))
    interval_h = float(state.get("interval_hours", 24.0))
    last_ts = float(state.get("last_cycle_ts", 0.0))
    history = state.get("history") or []
    curve = [h.get("yield", 0) for h in history][-CURVE_TAIL:]
    age_h = (now - last_ts) / 3600 if last_ts else float("inf")
    overdue = bool(history) and age_h >= interval_h
    when = f"há {age_h:.1f}h" if last_ts else "nunca"
    status = f"VENCIDO ({when}, intervalo {interval_h:g}h)" if overdue \
        else f"em dia ({when}; próximo em {max(0.0, interval_h - age_h):.1f}h)"
    verb = "MUST (antes de trabalho novo)" if overdue else "SHOULD (ao abrir frente nova)"
    lines = [
        f"  · '{topic}': {status} — yield {curve} · {sum(1 for h in history if h.get('ticket'))} ticket(s)",
        f"    {verb} → python3 {SCRIPT} cycle --topic \"{topic}\" --root {root}",
    ]
    queued = [h for h in history if h.get("ticket")][-QUEUE_TAIL:]
    for entry in queued:
        if entry.get("start_hint"):
            lines.append(f"    fila: {entry['ticket']} → adw={entry.get('adw') or '?'}"
                         f" → {entry['start_hint']}")
        else:  # pre-intake ticket: no recorded route — never fabricate a command
            lines.append(f"    fila: {entry['ticket']} (pré-intake, sem rota registrada"
                         " — o próximo ciclo roteia via factory)")
    return lines


def build_context(cwd: Path) -> str:
    """The dense injection: importance + live per-topic status + armed queue."""
    now = time.time()
    header = (
        "[SCOUT PERPÉTUO · gerador de demanda da software factory]\n"
        "Por quê: exploração é processo permanente, não fase (F1.7/CCE) — cada ciclo vira "
        "tickets que o factory roteia para ADWs e cujos outcomes ensinam o RL do router; "
        "sem ciclos o funil ADW/factory fica sem demanda. Disparar 1 ciclo antes de "
        "trabalho novo custa 1 comando e rende findings baratos."
    )
    root = project_root(cwd)
    if root is None:
        return (header + "\nNenhum projeto Touring (.touring/) neste cwd — ao entrar num "
                "projeto, arme o scout: touring adw run scout-perpetuo "
                '--var topic="<tema central do projeto>"')
    states = load_states(root)
    if not states:
        return (header + f"\nNenhum scout armado em {root} — arme antes do primeiro "
                "trabalho: touring adw run scout-perpetuo "
                '--var topic="<tema central do projeto>"')
    lines = [header, f"Estado em {root}:"]
    for state in states:
        lines.extend(topic_lines(state, root, now))
    return "\n".join(lines)


def main() -> int:
    """Read the SessionStart payload, emit additionalContext; never fail."""
    try:
        try:
            payload = json.load(sys.stdin)
        except (json.JSONDecodeError, OSError):
            payload = {}
        cwd = Path(payload.get("cwd")
                   or os.environ.get("CLAUDE_PROJECT_DIR")
                   or os.getcwd())
        print(json.dumps({"hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": build_context(cwd),
        }}, ensure_ascii=False))
    except Exception:  # noqa: BLE001 — fail-open is the hook contract
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
