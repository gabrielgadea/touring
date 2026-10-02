#!/usr/bin/env python3
"""S-11 — r3 shadow evaluator: a SEPARATE process that tails the emitter's events and writes decisions.

    r3_shadow_evaluator.py --once                 # drain the backlog and exit (tests, cron)
    r3_shadow_evaluator.py --follow [--interval 2]  # keep tailing (a user service)

Design (plan §2.1 — all coupling is by FILE, no channel or thread in the daemon):
  daemon emitter --append--> r3_events.jsonl --tail--> THIS PROCESS --append--> r3_shadow.jsonl --read--> `touring kpi`

Guarantees (each one has a named test and a mutation proof):
  * durable offset — a restart resumes where it stopped; only COMPLETE lines are consumed (the emitter may be
    mid-write); a truncated/rotated file restarts from 0;
  * the emitter is never blocked — with this process stopped the queue just grows;
  * the schema is closed — an invalid line is dropped and COUNTED, never fatal; every output row is validated;
  * an `api` arm receives ONLY `redact_for_jev(event)`; a `local` (loopback) arm may see the ≤ 80-char heads;
  * an arm that raises is counted and isolated — the other arms and the loop go on (fail-open);
  * at-least-once output: rows are written BEFORE the offset moves, so a crash can repeat a row (the KPI dedupes
    on (event_ref, arm)); it can never lose one.

Model arms (Laya local / Jev API) live in `r3_arms.py` (S-6) and are registered here if that module exists.
"""
import argparse
import collections
import json
import os
import sys
import time
from pathlib import Path

import r3_event_schema as ev

EVENTS, SHADOW, OFFSET, STATS = "r3_events.jsonl", "r3_shadow.jsonl", "r3_shadow.offset", "r3_shadow.stats.json"
DEDUP_WINDOW = 5
# Until data/exempt_markers.json is frozen (S-4) the online rule uses this conservative default.
EXEMPT_MARKERS = ("MUST", "REGRA #", "CRITICAL", "BLOCK", "[CODE MODE", "deny", "hook error")


def logs_dir() -> Path:
    return Path(os.environ.get("R3_LOGS_DIR") or Path.home() / ".claude/touring/logs")


class DedupArm:
    """A1 online rule: an exact repeat of a block (same first-line sha, same session, last 5 blocks) would be dropped —
    but NEVER an EXEMPT block. The zero-proposition half of B-D needs the full text and is replay-only."""
    name, use_case, model, kind = "B-D", "a1_relevance", "rule:dedup+exempt", "local"

    def __init__(self):
        self._recent = collections.defaultdict(lambda: collections.deque(maxlen=DEDUP_WINDOW))

    def decide(self, event):
        seen = self._recent[event["session_hash"]]
        exempt = any(m in event["first_line_head"] for m in EXEMPT_MARKERS)
        dup = event["first_line_sha"] in seen
        seen.append(event["first_line_sha"])
        return {"would_drop": bool(dup and not exempt)}


def build_arms():
    arms = [DedupArm()]
    try:  # the model arms are optional: absent module ⇒ only the deterministic arm runs
        import r3_arms  # noqa: PLC0415
        arms += list(r3_arms.build_online_arms())
    except ImportError:
        pass
    return arms


def read_new(logs: Path):
    """(complete lines after the saved offset, new offset). A trailing partial line waits for the next pass."""
    events, off_file = logs / EVENTS, logs / OFFSET
    if not events.exists():
        return [], 0
    try:
        off = int(off_file.read_text())
    except (OSError, ValueError):
        off = 0
    if off > events.stat().st_size:  # truncated or rotated
        off = 0
    with events.open("rb") as f:
        f.seek(off)
        data = f.read()
    cut = data.rfind(b"\n")
    if cut < 0:
        return [], off
    chunk = data[: cut + 1]
    return chunk.decode("utf-8", "replace").splitlines(), off + len(chunk)


def _row(event, arm, decision, latency_ms):
    agrees = None
    if event["kind"] == "a2_offer_closed" and "followed" in decision:
        agrees = decision["followed"] == event["baseline_followed"]
    return {
        "v": 1, "kind": "shadow_decision", "use_case": event["use_case"], "ts_ms": int(time.time() * 1000),
        "arm": arm.name, "event_ref": {"kind": event["kind"], "ts_ms": event["ts_ms"], "session_hash": event["session_hash"]},
        "decision": decision, "baseline_agrees": agrees, "latency_ms": latency_ms, "model": arm.model[:40],
    }


def _decide(arm, event, stats):
    t0 = time.perf_counter()
    try:
        payload = ev.redact_for_jev(event) if arm.kind == "api" else event  # the Jev API never sees free text
        decision = arm.decide(payload)
    except Exception:  # noqa: BLE001 — an arm may fail in any way; isolation is the contract
        stats["arm_errors"][arm.name] += 1
        return None
    if decision is None:
        return None
    row = _row(event, arm, decision, int((time.perf_counter() - t0) * 1000))
    if ev.validate_event(row) or ev.scan_privacy(json.dumps(row)):
        stats["invalid_out"] += 1
        return None
    return row


def run_once(logs, arms) -> dict:
    logs = Path(logs)
    lines, new_offset = read_new(logs)
    stats = {"read": 0, "invalid": 0, "invalid_out": 0, "written": 0, "arm_errors": collections.Counter()}
    rows = []
    for line in lines:
        if not line.strip():
            continue
        try:
            event = json.loads(line)
        except ValueError:
            stats["invalid"] += 1
            continue
        if not isinstance(event, dict) or event.get("kind") == "shadow_decision" or ev.validate_event(event):
            stats["invalid"] += 1
            continue
        stats["read"] += 1
        rows += [r for a in arms if a.use_case == event["use_case"] for r in [_decide(a, event, stats)] if r]
    if rows:
        with (logs / SHADOW).open("a") as f:
            f.write("".join(json.dumps(r, ensure_ascii=False) + "\n" for r in rows))
    stats["written"] = len(rows)
    (logs / OFFSET).write_text(str(new_offset))  # AFTER the rows: at-least-once, never lossy
    _persist_stats(logs, stats)
    stats["arm_errors"] = dict(stats["arm_errors"])
    return stats


def _persist_stats(logs: Path, run: dict):
    p = logs / STATS
    try:
        total = json.loads(p.read_text())
    except (OSError, ValueError):
        total = {"read": 0, "invalid": 0, "invalid_out": 0, "written": 0, "arm_errors": {}}
    for k in ("read", "invalid", "invalid_out", "written"):
        total[k] = total.get(k, 0) + run[k]
    errs = total.setdefault("arm_errors", {})
    for name, n in run["arm_errors"].items():
        errs[name] = errs.get(name, 0) + n
    total["updated_ms"] = int(time.time() * 1000)
    p.write_text(json.dumps(total))


def main(argv=None):
    ap = argparse.ArgumentParser(prog="r3_shadow_evaluator")
    ap.add_argument("--once", action="store_true")
    ap.add_argument("--follow", action="store_true")
    ap.add_argument("--interval", type=float, default=2.0)
    a = ap.parse_args(argv)
    logs = logs_dir()
    logs.mkdir(parents=True, exist_ok=True)
    arms = build_arms()
    if not a.follow:
        print(json.dumps(run_once(logs, arms)))
        return 0
    while True:  # --follow: fail-open — one bad pass must never end the service
        try:
            run_once(logs, arms)
        except Exception as e:  # noqa: BLE001
            print(f"r3_shadow_evaluator: pass failed ({e}); continuing", file=sys.stderr, flush=True)
        time.sleep(a.interval)


if __name__ == "__main__":
    raise SystemExit(main())
