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
import subprocess
import sys
from pathlib import Path

S1_RATIO, S1_ABS_MS, MIN_N = 1.10, 20.0, 200
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


def run_hook(argv, payload, shadow) -> bytes:
    env = dict(os.environ, TOURING_NO_DAEMON="1", TOURING_R3_SHADOW=shadow)
    return subprocess.run(argv, input=json.dumps(payload).encode(), env=env, capture_output=True, timeout=60).stdout


def s4_byte_identical(argv, inputs) -> dict:
    diffs = [i for i, p in enumerate(inputs) if run_hook(argv, p, "1") != run_hook(argv, p, "0")]
    return {"identical": not diffs and bool(inputs), "n": len(inputs), "n_diff": len(diffs),
            "first_diff_input": diffs[0] if diffs else None}


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
    for name in ("s1", "s2"):
        p = sub.add_parser(name)
        p.add_argument("--trace", required=True)
        p.add_argument("--hook", default="cli-suggest")
        p.add_argument("--on-ts", type=int, required=True)
        p.add_argument("--lookback-days", type=float, default=7)
    p4 = sub.add_parser("s4")
    p4.add_argument("--argv-json", required=True)
    p4.add_argument("--inputs", required=True)
    a = ap.parse_args(argv)
    if a.cmd == "s4":
        inputs = [json.loads(x) for x in Path(a.inputs).read_text().splitlines() if x.strip()]
        out = s4_byte_identical(json.loads(a.argv_json), inputs)
        ok = out["identical"]
    else:
        lb = int(a.lookback_days * 86_400_000)
        if a.cmd == "s1":
            out = s1_verdict(*split_windows(a.trace, a.hook, a.on_ts, lb))
        else:
            out = s2_verdict(exit_reasons(a.trace, a.hook, a.on_ts - lb, a.on_ts - 1),
                             exit_reasons(a.trace, a.hook, a.on_ts, 10**15))
        ok = out["passes"]
    print(json.dumps(out, indent=1))
    return 2 if ok is None else (0 if ok else 1)


if __name__ == "__main__":
    sys.exit(main())
