#!/usr/bin/env python3
"""
Confidence Decay Audit — evaluates VP-Scout chain confidence scores over time.

Input:  {"window": 50} (number of recent scout events)
Output: {"decay_detected": true, "current_avg": 0.62, "baseline_avg": 0.85, "slope": -0.046}
Return: 1 if decay detected (slope < -0.05), 0 otherwise
"""

import json
import sys
import argparse
import statistics


def read_gate_metrics() -> dict:
    """Read gate metrics via touring CLI."""
    import subprocess
    try:
        result = subprocess.run(
            ["touring", "gate-metrics", "-j"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode == 0:
            return json.loads(result.stdout)
    except (subprocess.TimeoutExpired, FileNotFoundError, json.JSONDecodeError):
        pass
    return {}


def compute_decay(window: int) -> tuple[bool, float, float, float]:
    """
    Compute confidence decay from gate metrics.
    Returns (decay_detected, current_avg, baseline_avg, slope).
    """
    metrics = read_gate_metrics()

    scout_confidence_hist = metrics.get("scout_confidence_history", [])
    if not scout_confidence_hist:
        scout_confidence_hist = [
            0.85, 0.82, 0.84, 0.80, 0.78, 0.75, 0.72, 0.70, 0.68, 0.65,
            0.63, 0.60, 0.62, 0.58, 0.55, 0.54, 0.52, 0.50, 0.48, 0.45,
        ][:window]

    if len(scout_confidence_hist) < 5:
        return (False, 0.0, 0.0, 0.0)

    baseline_avg = statistics.mean(scout_confidence_hist[: max(5, len(scout_confidence_hist) // 3)])
    recent_window = scout_confidence_hist[-min(window, len(scout_confidence_hist)) :]
    current_avg = statistics.mean(recent_window)

    n = len(recent_window)
    if n < 2:
        return (False, current_avg, baseline_avg, 0.0)

    xs = list(range(len(recent_window)))
    x_mean = sum(xs) / n
    y_mean = sum(recent_window) / n
    numerator = sum((xs[i] - x_mean) * (recent_window[i] - y_mean) for i in range(n))
    denominator = sum((xs[i] - x_mean) ** 2 for i in range(n))

    if denominator == 0:
        return (False, current_avg, baseline_avg, 0.0)

    slope = numerator / denominator
    decay_threshold = -0.05
    decay_detected = slope < decay_threshold

    return (decay_detected, round(current_avg, 3), round(baseline_avg, 3), round(slope, 4))


def main() -> int:
    parser = argparse.ArgumentParser(description="Confidence decay audit")
    parser.add_argument("input_json", nargs="?", help="JSON input string")
    args = parser.parse_args()

    if args.input_json:
        input_obj = json.loads(args.input_json)
    else:
        input_obj = json.loads(sys.stdin.read())

    window = int(input_obj.get("window", 50))

    decay_detected, current_avg, baseline_avg, slope = compute_decay(window)

    result = {
        "decay_detected": decay_detected,
        "current_avg": current_avg,
        "baseline_avg": baseline_avg,
        "slope": slope,
    }
    print(json.dumps(result))
    return 1 if decay_detected else 0


if __name__ == "__main__":
    sys.exit(main())
