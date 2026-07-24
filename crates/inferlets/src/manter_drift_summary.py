#!/usr/bin/env python3
"""
ManTER Drift Summary — summarizes PARCER drift between sessions.

Input:  {"before": "/path/to/before.parcer.yaml", "after": "/path/to/after.parcer.yaml"}
Output: {"drift_detected": true, "changed_dims": ["Audience", "Rules"], "summary": "Audience changed from junior to senior"}
Return: 1 if drift detected, 0 if identical
"""

import json
import sys
import argparse
from pathlib import Path

try:
    import yaml
    HAS_YAML = True
except ImportError:
    HAS_YAML = False


def parse_parcer_file(file_path: str) -> dict:
    """Parse PARCER YAML file, falling back to JSON if needed."""
    path = Path(file_path)
    if not path.exists():
        return {}
    content = path.read_text()
    if HAS_YAML:
        try:
            return yaml.safe_load(content) or {}
        except yaml.YAMLError:
            pass
    try:
        return json.loads(content)
    except json.JSONDecodeError:
        return {}


def extract_dims(doc: dict) -> dict:
    """Extract dimensional keys from PARCER document."""
    dims = {}
    if isinstance(doc, dict):
        for key, value in doc.items():
            if isinstance(value, (str, int, float, bool)):
                dims[key] = value
            elif isinstance(value, dict):
                dims[key] = extract_dims(value)
    return dims


def compare_dims(before: dict, after: dict) -> list[str]:
    """Compare dimensions and return list of changed keys."""
    changed = []
    b_dims = extract_dims(before)
    a_dims = extract_dims(after)
    all_keys = set(b_dims.keys()) | set(a_dims.keys())
    for key in all_keys:
        b_val = b_dims.get(key)
        a_val = a_dims.get(key)
        if b_val != a_val:
            changed.append(str(key))
    return changed


def build_summary(before: dict, after: dict, changed_dims: list[str]) -> str:
    """Build human-readable summary of drift."""
    if not changed_dims:
        return "No drift detected"
    b_dims = extract_dims(before)
    a_dims = extract_dims(after)
    parts = []
    for dim in changed_dims[:3]:
        b_val = b_dims.get(dim, "<missing>")
        a_val = a_dims.get(dim, "<missing>")
        if dim in b_dims and dim in a_dims:
            parts.append(f"{dim} changed from {b_val!r} to {a_val!r}")
        elif dim in a_dims:
            parts.append(f"{dim} set to {a_val!r}")
        else:
            parts.append(f"{dim} removed")
    return "; ".join(parts)


def main() -> int:
    parser = argparse.ArgumentParser(description="ManTER drift summary")
    parser.add_argument("input_json", nargs="?", help="JSON input string")
    args = parser.parse_args()

    if args.input_json:
        input_obj = json.loads(args.input_json)
    else:
        input_obj = json.loads(sys.stdin.read())

    before_path = input_obj.get("before", "")
    after_path = input_obj.get("after", "")

    before_doc = parse_parcer_file(before_path)
    after_doc = parse_parcer_file(after_path)
    changed_dims = compare_dims(before_doc, after_doc)
    drift_detected = len(changed_dims) > 0
    summary = build_summary(before_doc, after_doc, changed_dims)

    result = {
        "drift_detected": drift_detected,
        "changed_dims": changed_dims,
        "summary": summary,
    }
    print(json.dumps(result))
    return 1 if drift_detected else 0


if __name__ == "__main__":
    sys.exit(main())
