#!/usr/bin/env python3
"""perf_p99_gate.py — P99 latency regression guard for Touring benchmarks.

Master Plan H1-B (2026-06-13). Closes the gap that the previous masterplan
identified: criterion benchmarks EXIST (see crates/touring-hooks/benches/) but
NEVER ran in CI, so P99 regressions slipped through.

Behaviour
---------
* Discovers criterion JSON outputs under target/criterion/**/new/benchmark-complete.json
  (the canonical cargo-criterion external-format output).
* Compares the current `typical.upper_bound` (P99 upper bound, in ns) against
  the stored baseline in docs/baselines/benchmarks.json.
* Exits:
    0  PASS — no regressions beyond --max-regress-pct (default 10 %)
    1  REGRESSION — at least one benchmark regressed > threshold
    2  MISSING_BENCHMARKS — no baseline AND no current run (advisory, fail-open)
    3  ERROR — broken baseline JSON / IO error
* Modes:
    --check         CI mode (default; exit non-zero on regression)
    --baseline-init one-time creation/refresh of the baseline from current run
    --json          machine-readable report on stdout
* Zero-LLM, daemon-optional, fail-open when benchmarks absent.

Usage
-----
    docs/perf_p99_gate.py                 # check (CI default)
    docs/perf_p99_gate.py --check
    docs/perf_p99_gate.py --baseline-init
    docs/perf_p99_gate.py --max-regress-pct 15
    docs/perf_p99_gate.py --json
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASELINE = ROOT / "docs" / "baselines" / "benchmarks.json"
CRITERION_ROOT = ROOT / "target" / "criterion"

DEFAULT_MAX_REGRESS_PCT = 10.0


def discover_benchmarks() -> dict[str, dict]:
    """Return {bench_id: {typical_upper_ns, change_pct, ...}} from criterion output."""
    results: dict[str, dict] = {}
    if not CRITERION_ROOT.is_dir():
        return results
    for path in CRITERION_ROOT.glob("**/new/benchmark-complete.json"):
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        bid = data.get("id") or path.parts[-4]
        typical = data.get("typical") or {}
        change = data.get("change") or {}
        results[bid] = {
            "path": str(path.relative_to(ROOT)),
            "typical_upper_ns": typical.get("upper_bound"),
            "mean_estimate_ns": (data.get("mean") or {}).get("estimate"),
            "median_estimate_ns": (data.get("median") or {}).get("estimate"),
            "change_mean_pct": (change.get("mean") or {}).get("estimate"),
            "change_median_pct": (change.get("median") or {}).get("estimate"),
        }
    return results


def load_baseline() -> dict[str, dict] | None:
    if not BASELINE.is_file():
        return None
    try:
        return json.loads(BASELINE.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        print(f"::error::broken baseline {BASELINE}: {exc}", file=sys.stderr)
        sys.exit(3)


def write_baseline(current: dict[str, dict]) -> None:
    BASELINE.parent.mkdir(parents=True, exist_ok=True)
    serialised = {
        bid: {"typical_upper_ns": v.get("typical_upper_ns")} for bid, v in current.items()
    }
    BASELINE.write_text(json.dumps(serialised, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"baseline written: {BASELINE} ({len(serialised)} benchmarks)")


def diff_against_baseline(
    current: dict[str, dict],
    baseline: dict[str, dict] | None,
    max_regress_pct: float,
) -> list[dict]:
    if baseline is None:
        return []
    findings: list[dict] = []
    for bid, cur in current.items():
        cur_ns = cur.get("typical_upper_ns")
        base_ns = (baseline.get(bid) or {}).get("typical_upper_ns")
        if cur_ns is None or base_ns is None or base_ns == 0:
            continue
        delta_pct = (cur_ns - base_ns) / base_ns * 100.0
        if delta_pct > max_regress_pct:
            findings.append(
                {
                    "benchmark": bid,
                    "baseline_ns": base_ns,
                    "current_ns": cur_ns,
                    "delta_pct": round(delta_pct, 2),
                    "threshold_pct": max_regress_pct,
                    "severity": "REGRESSION",
                }
            )
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="CI mode (default)")
    parser.add_argument("--baseline-init", action="store_true", help="create/refresh baseline from current run")
    parser.add_argument("--max-regress-pct", type=float, default=DEFAULT_MAX_REGRESS_PCT, help="P99 regression threshold (default 10.0)")
    parser.add_argument("--json", action="store_true", help="machine-readable JSON on stdout")
    args = parser.parse_args()

    current = discover_benchmarks()
    if args.baseline_init:
        if not current:
            print("::error::no benchmarks found under target/criterion; run `cargo bench` first", file=sys.stderr)
            return 3
        write_baseline(current)
        return 0

    baseline = load_baseline()
    if not current:
        if baseline is None:
            print("perf_p99: no benchmarks, no baseline — fail-open (advisory)")
            return 2
        print("perf_p99: no current benchmarks; baseline exists — fail-open (advisory)")
        return 2

    regressions = diff_against_baseline(current, baseline, args.max_regress_pct)
    report = {
        "benchmarks_seen": len(current),
        "baseline_present": baseline is not None,
        "max_regress_pct": args.max_regress_pct,
        "regressions": regressions,
    }
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(f"perf_p99: {len(current)} benchmarks; baseline={'yes' if baseline else 'no'}; regressions={len(regressions)}")
        for r in regressions:
            print(f"  ::error::REGRESSION {r['benchmark']}: +{r['delta_pct']:.2f}% (> {r['threshold_pct']}%) {r['baseline_ns']}ns -> {r['current_ns']}ns", file=sys.stderr)
    return 1 if regressions else 0


if __name__ == "__main__":
    sys.exit(main())
