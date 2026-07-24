#!/usr/bin/env python3
"""
Entity Homonymy Detector — queries Entity Registry for homonymic entities across crates.

Input:  {"entity_name": "CognitiveMCTS"}
Output: {"homonyms": [{"crate": "touring-cognitive", "module": "cognitive_mcts", "definition": "type alias"}, ...], "count": 3}
Return: 1 if homonyms found (potential conflict), 0 if unique
"""

import json
import sys
import argparse
import subprocess
from pathlib import Path


def query_touring_index(entity_name: str) -> list[dict]:
    """Query touring index for symbol definitions."""
    results = []
    try:
        result = subprocess.run(
            ["touring", "index", "find", entity_name, "-j"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode == 0:
            data = json.loads(result.stdout)
            for item in data.get("definitions", []):
                results.append(item)
    except (subprocess.TimeoutExpired, FileNotFoundError, json.JSONDecodeError):
        pass
    return results


def classify_definition(file_path: str, line: int, symbol_name: str) -> str:
    """Classify the kind of definition based on context."""
    try:
        path = Path(file_path)
        if not path.exists():
            return "unknown"
        lines = path.read_text().splitlines()
        if line <= len(lines):
            context = "\n".join(lines[max(0, line - 5) : line + 5])
            if " type alias " in context or "type " in context and "=" in context:
                return "type alias"
            if " struct " in context:
                return "struct"
            if " fn " in context or "pub fn " in context:
                return "function"
            if " enum " in context:
                return "enum"
            if " mod " in context:
                return "module"
        return "unknown"
    except Exception:
        return "unknown"


def detect_homonyms(entity_name: str) -> tuple[list[dict], int]:
    """
    Detect homonymic entities across crates.
    Returns (homonyms_list, count).
    """
    from pathlib import Path

    index_results = query_touring_index(entity_name)
    homonyms = []
    seen = set()

    for item in index_results:
        file_path = item.get("file_path", "")
        module_parts = file_path.split("/src/")[-1].rsplit(".", 1)[0].split("/")
        module_name = "/".join(module_parts)
        crate = module_parts[0] if module_parts else "unknown"

        key = f"{crate}:{module_name}"
        if key in seen:
            continue
        seen.add(key)

        definition = classify_definition(file_path, item.get("line", 0), entity_name)
        homonyms.append({
            "crate": crate,
            "module": module_name,
            "definition": definition,
            "file_path": file_path,
            "line": item.get("line", 0),
        })

    return (homonyms, len(homonyms))


def main() -> int:
    parser = argparse.ArgumentParser(description="Entity homonymy detector")
    parser.add_argument("input_json", nargs="?", help="JSON input string")
    args = parser.parse_args()

    if args.input_json:
        input_obj = json.loads(args.input_json)
    else:
        input_obj = json.loads(sys.stdin.read())

    entity_name = input_obj.get("entity_name", "")
    homonyms, count = detect_homonyms(entity_name)

    result = {
        "homonyms": homonyms,
        "count": count,
    }
    print(json.dumps(result))
    return 1 if count > 0 else 0


if __name__ == "__main__":
    sys.exit(main())
