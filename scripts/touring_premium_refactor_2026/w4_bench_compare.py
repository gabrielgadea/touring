#!/usr/bin/env python3
"""W4.10 — Compare cargo bench results vs pre-refactor baseline; FAIL if >5% slower.

Reads Criterion-style JSON output from both pre-refactor and post-refactor
benchmark runs, computes per-benchmark deltas, and reports regressions.

Usage
-----
    python3 w4_bench_compare.py --baseline pre-refactor-2026-05-11 --threshold 0.05
"""
from __future__ import annotations

import argparse
import json
import logging
import sys
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_TARGET_CRITERION = _ROOT / "target" / "criterion"


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w4_bench_compare", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--baseline", type=str, required=False,
                   default="pre-refactor",
                   help="Baseline name (e.g., pre-refactor-2026-05-11)")
    p.add_argument("--current", type=str, default="post-refactor",
                   help="Current run name.")
    p.add_argument("--threshold", type=float, default=0.05,
                   help="Max acceptable regression (0.05 = 5%%).")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def find_bench_dirs(criterion_root: Path) -> list[Path]:
    """Find all Criterion bench directories."""
    if not criterion_root.exists():
        return []
    return sorted(d for d in criterion_root.iterdir() if d.is_dir())


def read_estimates(bench_dir: Path, baseline_name: str) -> dict | None:
    """Read estimates.json for a baseline run."""
    estimates = bench_dir / baseline_name / "estimates.json"
    if not estimates.exists():
        return None
    try:
        return json.loads(estimates.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def compute_delta(baseline: dict, current: dict) -> dict:
    """Compute relative delta between two Criterion estimates."""
    b_mean = baseline.get("mean", {}).get("point_estimate", 0.0)
    c_mean = current.get("mean", {}).get("point_estimate", 0.0)
    if b_mean == 0.0:
        return {"baseline_ns": b_mean, "current_ns": c_mean,
                "delta_pct": float("inf"), "regression": True}
    delta = (c_mean - b_mean) / b_mean
    return {"baseline_ns": b_mean, "current_ns": c_mean,
            "delta_pct": round(delta, 4),
            "regression": delta > 0}


def run(args: argparse.Namespace) -> dict:
    """Compare baseline vs current."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    bench_dirs = find_bench_dirs(_TARGET_CRITERION)
    comparisons: list[dict] = []
    regressions: list[dict] = []
    missing: list[str] = []
    for bd in bench_dirs:
        baseline_data = read_estimates(bd, args.baseline)
        current_data = read_estimates(bd, args.current)
        if not baseline_data or not current_data:
            missing.append(bd.name)
            continue
        delta = compute_delta(baseline_data, current_data)
        entry = {"benchmark": bd.name, **delta}
        comparisons.append(entry)
        if delta["regression"] and delta["delta_pct"] > args.threshold:
            regressions.append(entry)
    report = {
        "script": "w4_bench_compare",
        "wave": "W4", "subtask_refs": ["W4.10"],
        "timestamp": datetime.now(UTC).isoformat(),
        "baseline": args.baseline, "current": args.current,
        "threshold": args.threshold,
        "totals": {
            "benchmarks_compared": len(comparisons),
            "regressions": len(regressions),
            "missing": len(missing),
        },
        "regressions": sorted(regressions, key=lambda x: -x["delta_pct"]),
        "comparisons": comparisons[:30],
        "missing": missing,
    }
    json_path = args.output_dir / "w4-bench-comparison.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    return {**report,
             "status": "FAIL" if regressions else "OK",
             "json_path": str(json_path.relative_to(_ROOT))}


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps({
            "status": result["status"], "totals": result["totals"],
            "top_3_regressions": result["regressions"][:3],
            "json_path": result["json_path"],
        }, indent=2, ensure_ascii=False) + "\n")
        return 0 if result["status"] == "OK" else 1
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
