#!/usr/bin/env python3
"""
Wave Velocity Report — computes delivery velocity from diary entries.

Input:  {"days": 30, "project": "touring"}
Output: {"velocity": 3.2, "units": "deliverables/week", "trend": "increasing", "samples": 14}
Return: 1 if velocity > 0 (data available), 0 if no data
"""

import json
import sys
import argparse
import subprocess
from datetime import datetime, timedelta


def read_diary_entries(project: str, days: int) -> list[dict]:
    """Read diary entries via touring CLI."""
    try:
        result = subprocess.run(
            ["touring", "diary", "read", "--project", project, "--last", "100"],
            capture_output=True,
            text=True,
            timeout=15,
        )
        if result.returncode == 0:
            data = json.loads(result.stdout)
            return data.get("entries", []) if isinstance(data, dict) else data
    except (subprocess.TimeoutExpired, FileNotFoundError, json.JSONDecodeError):
        pass
    return []


def parse_timestamp(entry: dict) -> datetime | None:
    """Parse timestamp from diary entry."""
    ts = entry.get("timestamp") or entry.get("created_at")
    if not ts:
        return None
    try:
        return datetime.fromisoformat(ts.replace("Z", "+00:00"))
    except (ValueError, AttributeError):
        return None


def compute_velocity(entries: list[dict], days: int, project: str) -> tuple[float, str, int]:
    """
    Compute velocity from diary entries.
    Returns (velocity, trend, samples).
    """
    cutoff = datetime.now() - timedelta(days=days)
    recent = [e for e in entries if (ts := parse_timestamp(e)) is not None and ts >= cutoff]
    samples = len(recent)

    if samples < 2:
        return (0.0, "unknown", samples)

    weeks = max(days / 7.0, 1.0)
    velocity = samples / weeks

    trend = "stable"
    if samples >= 5:
        older = [e for e in entries if (ts := parse_timestamp(e)) is not None and ts < cutoff]
        if len(older) > samples:
            trend = "increasing"
        elif len(older) < samples * 0.5:
            trend = "decreasing"

    return (round(velocity, 1), trend, samples)


def main() -> int:
    parser = argparse.ArgumentParser(description="Wave velocity report")
    parser.add_argument("input_json", nargs="?", help="JSON input string")
    args = parser.parse_args()

    if args.input_json:
        input_obj = json.loads(args.input_json)
    else:
        input_obj = json.loads(sys.stdin.read())

    days = int(input_obj.get("days", 30))
    project = input_obj.get("project", "touring")

    entries = read_diary_entries(project, days)
    velocity, trend, samples = compute_velocity(entries, days, project)

    result = {
        "velocity": velocity,
        "units": "deliverables/week",
        "trend": trend,
        "samples": samples,
    }
    print(json.dumps(result))
    return 1 if velocity > 0 else 0


if __name__ == "__main__":
    sys.exit(main())
