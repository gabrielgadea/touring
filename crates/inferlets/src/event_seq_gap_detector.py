#!/usr/bin/env python3
"""
Event Sequence Gap Detector — validates monotonicity of event_seq in activity logs.

Input:  {"activity_log": "/path/to/activity.jsonl", "task_id": "t-123"}
Output: {"valid": true, "gaps": [], "last_seq": 42} or {"valid": false, "gaps": [{"expected": 5, "found": 7}], "last_seq": 42}
Return: 1 if valid (no gaps), 0 if gaps found
"""

import json
import sys
import argparse
from pathlib import Path


def parse_activity_log(log_path: str) -> list[dict]:
    """Parse JSONL file, returning list of event dicts with event_seq."""
    events = []
    path = Path(log_path)
    if not path.exists():
        return events
    with path.open() as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                events.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    return events


def detect_gaps(events: list[dict]) -> tuple[bool, list[dict], int]:
    """
    Detect gaps in event_seq monotonic sequence.
    Returns (valid, gaps_list, last_seq).
    """
    gaps = []
    last_seq = 0
    for event in events:
        seq = event.get("event_seq")
        if seq is None:
            continue
        seq = int(seq)
        if last_seq != 0 and seq != last_seq + 1:
            gaps.append({"expected": last_seq + 1, "found": seq})
        last_seq = seq
    return (len(gaps) == 0, gaps, last_seq)


def main() -> int:
    parser = argparse.ArgumentParser(description="Event sequence gap detector")
    parser.add_argument("input_json", nargs="?", help="JSON input string")
    args = parser.parse_args()

    if args.input_json:
        input_obj = json.loads(args.input_json)
    else:
        input_obj = json.loads(sys.stdin.read())

    log_path = input_obj.get("activity_log", "")
    task_id = input_obj.get("task_id", "")

    events = parse_activity_log(log_path)
    valid, gaps, last_seq = detect_gaps(events)

    result = {
        "valid": valid,
        "gaps": gaps,
        "last_seq": last_seq,
    }
    print(json.dumps(result))
    return 1 if valid else 0


if __name__ == "__main__":
    sys.exit(main())
