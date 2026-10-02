#!/usr/bin/env python3
"""S-12 — delivery proofs for the r3 shadow: S1 (hot path), S2 (fail-open), S4 (byte-identical).

    r3_prove_delivery.py s1 --trace hook_trace.jsonl --hook cli-suggest --on-ts <ms> [--lookback-days 7]
    r3_prove_delivery.py s2 --trace hook_trace.jsonl --hook cli-suggest --on-ts <ms> [--lookback-days 7]
    r3_prove_delivery.py s4 --argv-json '["~/.claude/hooks/touring-hook","pre-bash"]' --inputs real_inputs.jsonl

Thresholds are the pre-registered ones (CRITERIOS §4): S1 p95 with the shadow ON ≤ 1.10 × the p95 of the prior
window OR ≤ +20 ms (whichever is larger); S2 non-ok exit_reason share not above the baseline by more than 0.5 pp;
S4 `additionalContext` byte-identical for every real input with the flag ON and OFF.

"No signal is not a pass": too few samples ⇒ `passes: null` and exit 2, never a green.

S4 runs the hook STANDALONE (TOURING_NO_DAEMON=1): the emitter lives inside the daemon, so a per-command env var
only reaches it when the hook runs in-process. With the daemon, ON and OFF would both hit the same daemon and the
diff would be 0 by construction — a proof of nothing.
"""
import argparse
import json
import math
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import r3_event_schema as ev_schema

S1_RATIO, S1_ABS_MS, MIN_N = 1.10, 20.0, 200
# Errata 5 (E5.1): the two hooks where the A2 observer runs — `cli-suggest` (record_route_offer) and `post-tool-rl`
# (claim_route_reward, every Bash). S1/S1b/S2 cover BOTH; the worst verdict decides.
HOOKS = ("cli-suggest", "post-tool-rl")
DAY_MS = 86_400_000
S2_MAX_DELTA, S2_MIN_N = 0.005, 50
BAD_WORDS = ("error", "fail", "timeout", "panic", "unavailable", "refused", "denied-by-error")


def _rows(trace, hook, lo, hi):
    out = []
    for line in Path(trace).read_text(errors="replace").splitlines():
        try:
            r = json.loads(line)
            ts = int(r.get("ts_ms") or 0)
        except (ValueError, TypeError):
            continue
        if r.get("hook") == hook and lo <= ts <= hi:
            out.append(r)
    return out


def _pctl(sorted_vals, p):
    return sorted_vals[max(1, math.ceil(p / 100 * len(sorted_vals))) - 1]  # nearest rank


def hook_window(trace, hook, lo, hi) -> dict:
    vals = sorted(float(r["elapsed_ms"]) for r in _rows(trace, hook, lo, hi) if r.get("elapsed_ms") is not None)
    if not vals:
        return {"n": 0, "p50": None, "p95": None}
    return {"n": len(vals), "p50": _pctl(vals, 50), "p95": _pctl(vals, 95)}


def split_windows(trace, hook, on_ts, lookback_ms):
    return (hook_window(trace, hook, on_ts - lookback_ms, on_ts - 1), hook_window(trace, hook, on_ts, 10**15))


def s1_verdict(before, after, min_n=MIN_N) -> dict:
    if before["n"] < min_n or after["n"] < min_n:
        return {"passes": None, "reason": f"too few samples (before n={before['n']}, after n={after['n']}, need {min_n})"}
    limit = max(before["p95"] * S1_RATIO, before["p95"] + S1_ABS_MS)
    return {"passes": round(after["p95"], 6) <= round(limit, 6), "p95_before": before["p95"],
            "p95_after": after["p95"], "limit": limit, "n_before": before["n"], "n_after": after["n"]}


def daily_p95s(trace, hook, lo, hi, min_n=MIN_N) -> list:
    """The p95 of each UTC day in [lo, hi] with at least `min_n` rows — the NATURAL envelope (Errata 5, E5.2): measured on
    the real baseline the daily p95 swings 122-596 ms (cli-suggest) and 315-3244 ms (post-tool-rl) with no shadow at all."""
    days = {}
    for r in _rows(trace, hook, lo, hi):
        if r.get("elapsed_ms") is not None:
            days.setdefault(int(r["ts_ms"]) // DAY_MS, []).append(float(r["elapsed_ms"]))
    return sorted(_pctl(sorted(v), 95) for v in days.values() if len(v) >= min_n)


def s1b_verdict(prior_daily_p95s, after, min_n=MIN_N) -> dict:
    """S1b: with the shadow ON the p95 stays inside the natural envelope (<= the WORST prior daily p95). It catches a
    pathological interaction, not +10% — S1a pins the fine cost. No baseline day or too few ON rows ⇒ no verdict."""
    if not prior_daily_p95s:
        return {"passes": None, "reason": "no baseline day with enough samples"}
    if after["n"] < min_n:
        return {"passes": None, "reason": f"too few samples (after n={after['n']}, need {min_n})"}
    ceiling = max(prior_daily_p95s)
    return {"passes": round(after["p95"], 6) <= round(ceiling, 6), "p95_after": after["p95"], "envelope_max": ceiling,
            "envelope_min": min(prior_daily_p95s), "n_days": len(prior_daily_p95s), "n_after": after["n"]}


LIVE_STATE_VARS = ("TOURING_HOOK_TRACE_FILE", "TOURING_DAEMON_SOCKET", "TOURING_DAEMON_SOCK")


def s6_privacy(events_file) -> dict:
    """S6: every line is valid in the CLOSED event schema (an unknown key is a violation) and carries no free text over
    the 80-char cap. Reuses r3_event_schema (the evaluator's own gate) — no second regex. No lines ⇒ no signal, never a
    pass; the report counts violations and NEVER echoes the offending text."""
    p = Path(events_file)
    if not p.exists():
        return {"passes": None, "n": 0, "reason": "no events file"}
    n = bad_schema = bad_privacy = 0
    for line in p.read_text(errors="replace").splitlines():
        if not line.strip():
            continue
        n += 1
        try:
            event = json.loads(line)
        except ValueError:
            event = None
        bad_schema += event is None or not isinstance(event, dict) or bool(ev_schema.validate_event(event))
        bad_privacy += bool(ev_schema.scan_privacy(line))
    if not n:
        return {"passes": None, "n": 0, "reason": "no events in the file"}
    return {"passes": not (bad_schema or bad_privacy), "n": n, "n_invalid": bad_schema, "n_privacy": bad_privacy}


def run_hook(argv, payload, shadow, state_dir=None) -> bytes:
    """One STANDALONE hook run. With `state_dir` the run is isolated: the live trace/daemon are unreachable (a standalone
    run is slow and would inflate the very p95 S1b reads) and the project root + events file are the scratch dir, so
    the real journals/counters are never touched."""
    env = {k: v for k, v in os.environ.items() if k not in LIVE_STATE_VARS}
    env.update(TOURING_NO_DAEMON="1", TOURING_R3_SHADOW=shadow)
    if state_dir is not None:
        Path(state_dir).mkdir(parents=True, exist_ok=True)
        env.update(TOURING_PROJECT_ROOT=str(state_dir), TOURING_R3_EVENTS_FILE=str(Path(state_dir) / "r3_events.jsonl"))
    return subprocess.run(argv, input=json.dumps(payload).encode(), env=env, capture_output=True, timeout=60).stdout


def s4_byte_identical(argv, inputs, toml=None) -> dict:
    """ON vs OFF per input, plus a CONTROL (OFF vs OFF). Every run starts from a FRESH scratch project: the suggester's
    300 s dedup cache would otherwise make a repeated input differ with no flag involved. A control that differs means
    the hook is not deterministic in this harness, so the proof is VOID (passes=None) — a diff is never blamed on the flag."""
    n_diff = n_ctl = 0
    first = None
    with tempfile.TemporaryDirectory() as tmp:
        for i, p in enumerate(inputs):
            outs = []
            for tag, flag in (("on", "1"), ("off", "0"), ("ctl", "0")):
                d = Path(tmp) / f"{i}-{tag}"
                if toml:
                    (d / ".touring").mkdir(parents=True)
                    shutil.copy(toml, d / ".touring" / "touring.toml")   # realistic code-mode gating, identical on every run
                outs.append(run_hook(argv, {**p, "cwd": str(d)}, flag, state_dir=d))
            if outs[0] != outs[1]:
                n_diff += 1
                first = i if first is None else first
            n_ctl += outs[1] != outs[2]
    base = {"n": len(inputs), "n_diff": n_diff, "n_control_diff": n_ctl, "control_identical": n_ctl == 0,
            "identical": not n_diff and bool(inputs), "first_diff_input": first}
    if n_ctl:
        return {**base, "passes": None, "reason": "control (OFF x OFF) differs: the hook is not deterministic here, so a diff cannot be attributed to the flag"}
    return {**base, "passes": base["identical"]}


def exit_reasons(trace, hook, lo, hi) -> dict:
    rows = _rows(trace, hook, lo, hi)
    by = {}
    for r in rows:
        by[r.get("exit_reason", "?")] = by.get(r.get("exit_reason", "?"), 0) + 1
    bad = sum(n for k, n in by.items() if any(w in str(k).lower() for w in BAD_WORDS))
    return {"n": len(rows), "non_ok_share": (bad / len(rows)) if rows else None, "by": by}


def s2_verdict(base, run) -> dict:
    if base["n"] < S2_MIN_N or run["n"] < S2_MIN_N:
        return {"passes": None, "reason": f"too few samples (base n={base['n']}, run n={run['n']}, need {S2_MIN_N})"}
    delta = run["non_ok_share"] - base["non_ok_share"]
    return {"passes": round(delta, 9) <= S2_MAX_DELTA, "delta_non_ok_share": delta,
            "base_non_ok": base["non_ok_share"], "run_non_ok": run["non_ok_share"]}


def main(argv=None):
    ap = argparse.ArgumentParser(prog="r3_prove_delivery")
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("s1", "s1b", "s2"):
        p = sub.add_parser(name)
        p.add_argument("--trace", required=True)
        p.add_argument("--hook", action="append", default=None, help=f"repeatable; default: {', '.join(HOOKS)}")
        p.add_argument("--on-ts", type=int, required=True)
        p.add_argument("--lookback-days", type=float, default=7)
    p4 = sub.add_parser("s4")
    p4.add_argument("--argv-json", required=True)
    p4.add_argument("--inputs", required=True)
    p4.add_argument("--toml", help="project .touring/touring.toml copied into every scratch project (realistic gating)")
    p6 = sub.add_parser("s6")
    p6.add_argument("--events", action="append", required=True, help="events/shadow journal; repeatable")
    a = ap.parse_args(argv)
    if a.cmd == "s6":
        res = {f: s6_privacy(f) for f in a.events}
        print(json.dumps(res, indent=1))
        deciding = [r["passes"] for r in res.values()]
        return 1 if any(v is False for v in deciding) else (2 if any(v is None for v in deciding) else 0)
    if a.cmd == "s4":
        inputs = [json.loads(x) for x in Path(a.inputs).read_text().splitlines() if x.strip()]
        out = s4_byte_identical(json.loads(a.argv_json), inputs, toml=a.toml)
        print(json.dumps(out, indent=1))
        return 2 if out["passes"] is None else (0 if out["passes"] else 1)
    return _per_hook(a)


def _per_hook(a) -> int:
    """S1 (informational), S1b and S2 per hook; the WORST verdict decides, and 'no signal' is never a pass."""
    lb = int(a.lookback_days * DAY_MS)
    results = {}
    for h in a.hook or HOOKS:
        if a.cmd == "s1":
            results[h] = {**s1_verdict(*split_windows(a.trace, h, a.on_ts, lb)), "informational": True}
        elif a.cmd == "s1b":
            results[h] = s1b_verdict(daily_p95s(a.trace, h, a.on_ts - lb, a.on_ts - 1), hook_window(a.trace, h, a.on_ts, 10**15))
        else:
            results[h] = s2_verdict(exit_reasons(a.trace, h, a.on_ts - lb, a.on_ts - 1), exit_reasons(a.trace, h, a.on_ts, 10**15))
    print(json.dumps(results, indent=1))
    deciding = [r["passes"] for r in results.values() if not r.get("informational")]
    if any(v is False for v in deciding):
        return 1
    return 2 if any(v is None for v in deciding) else 0


if __name__ == "__main__":
    sys.exit(main())
