#!/usr/bin/env python3
"""scout_perpetuo.py — F1.7: exploration as a permanent process, not a phase.

Every convergence gate admits a round N+1 that finds something (targets are
endogenous — CCE v2.6). The regress only ends by changing the answer's TYPE:
exploration becomes a background process with adaptive cadence, findings become
tickets, and the human gate stops asking "is it complete?" (undecidable) and
starts deciding "act or wait?" under a declared yield curve.

Subcommands:
  cycle  --topic T [--root R]   # one scout cycle: explore → yield → ticket → backoff
  status --topic T [--root R]   # yield curve + open questions + due + act-vs-wait

Mechanics:
  - yield  = delta in the explore ledger's finding count (the CCE ledger is the
    source of truth; `touring explore` maintains it).
  - ticket = a `touring decompose create` container per yielding cycle (the
    factory queue F4 will route) — findings feed work, nobody is interrupted.
  - cadence: yield > 0 → interval halves (min 6h); dry → doubles (max 720h ≈
    monthly), NEVER to zero. The system, not Gabriel, carries the pressure.
  - `cycle` prints NEW_FINDINGS=<yield> so an ADW loop node (adw.py, Law L2)
    can count dryness when scout-perpetuo runs as a workflow.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

LEDGER_DIRNAME = ".touring-explore"
STATE_DIRNAME = ".touring/scout-perpetuo"
MIN_INTERVAL_H = 6.0
MAX_INTERVAL_H = 720.0  # ~monthly; the scout never stops entirely
DEFAULT_INTERVAL_H = 24.0
EXPLORE_TIMEOUT_S = 900


def slugify(topic: str) -> str:
    """Same slug rule as explore_until_dry.py — the ledgers must line up."""
    return "".join(c if c.isalnum() else "-" for c in topic.lower()).strip("-")[:48]


def ledger_path(root: Path, topic: str) -> Path:
    return root / LEDGER_DIRNAME / f"{slugify(topic)}.ledger.json"


def state_path(root: Path, topic: str) -> Path:
    return root / STATE_DIRNAME / f"{slugify(topic)}.json"


def load_state(root: Path, topic: str) -> dict:
    path = state_path(root, topic)
    if path.is_file():
        return json.loads(path.read_text(encoding="utf-8"))
    return {"topic": topic, "interval_hours": DEFAULT_INTERVAL_H,
            "last_cycle_ts": 0.0, "history": []}


def save_state(root: Path, topic: str, state: dict) -> None:
    path = state_path(root, topic)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(state, ensure_ascii=False, indent=1), encoding="utf-8")


def ledger_snapshot(root: Path, topic: str) -> tuple[int, list, dict]:
    """(finding_count, open_questions, rounds_meta) from the CCE ledger."""
    path = ledger_path(root, topic)
    if not path.is_file():
        return 0, [], {}
    data = json.loads(path.read_text(encoding="utf-8"))
    findings = data.get("findings") or {}
    count = len(findings) if isinstance(findings, dict) else len(findings or [])
    questions = data.get("questions") or []
    open_qs = [q for q in questions if not (isinstance(q, dict) and q.get("resolved"))]
    return count, open_qs, {"rounds": data.get("rounds", []), "verdict": data.get("verdict", {})}


def run_explore(topic: str, root: Path) -> int:
    """Drive one `touring explore` pass; any of its honest exits is a valid cycle."""
    try:
        proc = subprocess.run(["touring", "explore", topic], cwd=root,
                              capture_output=True, timeout=EXPLORE_TIMEOUT_S)
        return proc.returncode
    except subprocess.TimeoutExpired:
        return 124
    except FileNotFoundError:
        return 127


def file_ticket(topic: str, new_findings: int, sample_keys: list[str]) -> str:
    """A yielding cycle becomes a factory-queue ticket (decompose container)."""
    desc = (f"scout-ticket: '{topic}' yielded {new_findings} new finding(s); "
            f"sample: {', '.join(sample_keys[:5]) or 'n/a'} — review for plan-delta")
    try:
        proc = subprocess.run(["touring", "decompose", "create", "plan", desc],
                              capture_output=True, text=True, timeout=30)
        parsed = json.loads(proc.stdout.strip().splitlines()[-1])
        return str(parsed.get("task_id", ""))
    except Exception:
        return ""


def new_finding_keys(root: Path, topic: str, before_count: int) -> list[str]:
    path = ledger_path(root, topic)
    if not path.is_file():
        return []
    data = json.loads(path.read_text(encoding="utf-8"))
    findings = data.get("findings") or {}
    items = list(findings.values()) if isinstance(findings, dict) else findings
    return [str(f.get("key", f.get("id", "?"))) for f in items[before_count:]]


def cmd_cycle(root: Path, topic: str) -> int:
    state = load_state(root, topic)
    before, _, _ = ledger_snapshot(root, topic)
    explore_exit = run_explore(topic, root)
    after, open_qs, _ = ledger_snapshot(root, topic)
    yield_n = max(0, after - before)

    ticket = ""
    if yield_n > 0:
        ticket = file_ticket(topic, yield_n, new_finding_keys(root, topic, before))
        state["interval_hours"] = max(MIN_INTERVAL_H, state["interval_hours"] / 2)
    else:
        state["interval_hours"] = min(MAX_INTERVAL_H, state["interval_hours"] * 2)

    state["last_cycle_ts"] = time.time()
    state["history"].append({"ts": state["last_cycle_ts"], "yield": yield_n,
                             "findings_total": after, "ticket": ticket,
                             "explore_exit": explore_exit})
    save_state(root, topic, state)

    print(json.dumps({
        "cycle": len(state["history"]), "yield": yield_n, "findings_total": after,
        "ticket": ticket or None, "open_questions": len(open_qs),
        "next_interval_hours": state["interval_hours"], "explore_exit": explore_exit,
    }, ensure_ascii=False))
    print(f"NEW_FINDINGS={yield_n}")  # ADW loop-node protocol (adw.py, Law L2)
    return 0


def recommendation(history: list[dict], open_qs: list) -> dict:
    """Act-vs-wait: the runner presents evidence; the HUMAN decides (video 08:01)."""
    recent = [h["yield"] for h in history[-2:]]
    dry_equilibrium = len(recent) == 2 and sum(recent) == 0
    if dry_equilibrium and not open_qs:
        stance, why = "act", "2 consecutive dry cycles and no open questions — marginal scout yield; backoff engaged"
    elif dry_equilibrium:
        stance, why = "act-with-caveats", f"2 dry cycles but {len(open_qs)} open question(s) — act, and route the questions as tickets"
    else:
        stance, why = "wait", "the scout is still yielding — acting now forfeits cheap findings (irreversible phases should re-run a directed explore first)"
    return {"stance": stance, "why": why}


def cmd_status(root: Path, topic: str) -> int:
    state = load_state(root, topic)
    total, open_qs, meta = ledger_snapshot(root, topic)
    now = time.time()
    next_due_ts = state["last_cycle_ts"] + state["interval_hours"] * 3600
    print(json.dumps({
        "topic": topic,
        "cycles_run": len(state["history"]),
        "yield_curve": [h["yield"] for h in state["history"]],
        "tickets": [h["ticket"] for h in state["history"] if h.get("ticket")],
        "findings_total": total,
        "open_questions": open_qs[:10],
        "interval_hours": state["interval_hours"],
        "next_due_in_hours": round(max(0.0, next_due_ts - now) / 3600, 2),
        "overdue": now >= next_due_ts and bool(state["history"]),
        "ledger_verdict": (meta.get("verdict") or {}).get("statement", "no ledger yet"),
        "recommendation": recommendation(state["history"], open_qs),
    }, ensure_ascii=False, indent=1))
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="scout_perpetuo", description=__doc__)
    sub = parser.add_subparsers(dest="sub", required=True)
    for name in ("cycle", "status"):
        p = sub.add_parser(name)
        p.add_argument("--topic", required=True)
        p.add_argument("--root", default=".")
    args = parser.parse_args(argv)
    root = Path(args.root).resolve()
    if args.sub == "cycle":
        return cmd_cycle(root, args.topic)
    return cmd_status(root, args.topic)


if __name__ == "__main__":
    sys.exit(main())
