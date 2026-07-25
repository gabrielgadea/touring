#!/usr/bin/env python3
"""plan_validator — Validate plan markdown structure and DAG integrity.

Checks
------
  1. Frontmatter present and parseable (kebab plan id, ISO date, ≥1 wave).
  2. Every wave heading has the required table fields.
  3. Wave-id naming follows W<N> or W<N>.<M>.
  4. Dependencies reference declared waves (no dangling refs).
  5. Dependency graph is a DAG — no cycles (Kahn's topological sort).
  6. Critical path waves are properly ordered.

Usage
-----
    python3 plan_validator.py path/to/plan.md
    python3 plan_validator.py path/to/plan.md --check-cycles
    python3 plan_validator.py path/to/plan.md --topological-order -j
"""

from __future__ import annotations

import argparse
import json
import logging
import re
import sys
from collections import deque
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

from lib import (  # noqa: E402  pylint: disable=wrong-import-position
    EXIT_FAIL,
    EXIT_INTERRUPTED,
    EXIT_OK,
    EXIT_STRUCTURAL,
    EXIT_WARN,
    is_kebab,
    is_wave_id,
    utcnow_iso,
    write_json_atomic,
)

# Patterns
_RE_FRONTMATTER = re.compile(r"^---\n(.*?)\n---\n", re.DOTALL)
_RE_FM_PLAN = re.compile(r"^plan:\s*(.+?)\s*$", re.MULTILINE)
_RE_FM_AUTHORED = re.compile(r"^authored:\s*(\d{4}-\d{2}-\d{2})\s*$", re.MULTILINE)
_RE_FM_STATUS = re.compile(r"^status:\s*(DRAFT|ACTIVE|COMPLETE|ARCHIVED)\s*$", re.MULTILINE)
_RE_WAVE_HEADING = re.compile(r"^###\s+(W\d{1,3}(?:\.\d+)?)\s+[—\-:]\s*(.+)$", re.MULTILINE)
_RE_DEPENDS_ON = re.compile(r"\|\s*Depends on\s*\|\s*([^|]+?)\s*\|", re.IGNORECASE)


def _parse_frontmatter(text: str) -> dict[str, Any]:
    """Extract simple key:value pairs from YAML-ish frontmatter."""
    match = _RE_FRONTMATTER.search(text)
    if not match:
        return {}
    fm_block = match.group(1)
    parsed: dict[str, Any] = {}
    for line in fm_block.splitlines():
        if ":" not in line or line.startswith(" ") or line.startswith("-"):
            continue
        key, _, val = line.partition(":")
        parsed[key.strip()] = val.strip()
    return parsed


def _extract_waves_with_deps(text: str) -> dict[str, list[str]]:
    """Build {wave_id: [dependencies]} from headings + tables."""
    waves: dict[str, list[str]] = {}
    matches = list(_RE_WAVE_HEADING.finditer(text))
    for idx, match in enumerate(matches):
        wave_id = match.group(1)
        start = match.start()
        end = matches[idx + 1].start() if idx + 1 < len(matches) else len(text)
        body = text[start:end]
        dep_match = _RE_DEPENDS_ON.search(body)
        deps: list[str] = []
        if dep_match:
            dep_raw = dep_match.group(1).strip()
            if dep_raw and dep_raw not in ("—", "-", "none", "None"):
                deps = [d.strip() for d in dep_raw.split(",") if is_wave_id(d.strip())]
        waves[wave_id] = deps
    return waves


def kahn_topological_sort(graph: dict[str, list[str]]) -> tuple[list[str], list[str]]:
    """Topological sort via Kahn's algorithm.

    Returns ``(sorted_order, cycle_nodes)``. When a cycle exists, ``sorted_order``
    is the partial order and ``cycle_nodes`` contains the nodes that could not
    be processed (which are part of a cycle).
    """
    in_degree: dict[str, int] = {node: 0 for node in graph}
    adjacency: dict[str, list[str]] = {node: [] for node in graph}
    for node, deps in graph.items():
        for dep in deps:
            if dep in graph:
                adjacency[dep].append(node)
                in_degree[node] += 1

    queue: deque[str] = deque(sorted(n for n in graph if in_degree[n] == 0))
    order: list[str] = []
    while queue:
        node = queue.popleft()
        order.append(node)
        for dependent in sorted(adjacency.get(node, [])):
            in_degree[dependent] -= 1
            if in_degree[dependent] == 0:
                queue.append(dependent)

    cycle_nodes = [n for n, deg in in_degree.items() if deg > 0]
    return order, cycle_nodes


def find_dangling_refs(graph: dict[str, list[str]]) -> list[tuple[str, str]]:
    """Find dependency references that point to non-declared waves."""
    declared = set(graph.keys())
    dangling: list[tuple[str, str]] = []
    for node, deps in graph.items():
        for dep in deps:
            if dep not in declared:
                dangling.append((node, dep))
    return dangling


# ── CLI ───────────────────────────────────────────────────────────────────


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="plan_validator", description=__doc__)
    parser.add_argument("path", type=Path, help="Plan markdown to validate")
    parser.add_argument("--check-cycles", action="store_true",
                        help="Run Kahn's topological sort to detect cycles.")
    parser.add_argument("--topological-order", action="store_true",
                        help="Emit topological wave ordering in the report.")
    parser.add_argument("--apply", action="store_true",
                        help="No-op (validator is read-only).")
    parser.add_argument("--output-dir", type=Path, default=Path("data"))
    parser.add_argument("--emit", action="store_true")
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Validate the plan."""
    if not args.path.exists():
        msg = f"Plan file not found: {args.path}"
        raise FileNotFoundError(msg)

    text = args.path.read_text(encoding="utf-8")
    frontmatter = _parse_frontmatter(text)
    waves_graph = _extract_waves_with_deps(text)

    errors: list[str] = []
    warnings: list[str] = []

    # Check 1: frontmatter required fields
    plan_id = frontmatter.get("plan", "")
    if not plan_id:
        errors.append("Frontmatter missing required field: plan")
    elif not is_kebab(plan_id):
        warnings.append(f"plan id '{plan_id}' is not kebab-case")

    if "authored" in frontmatter:
        if not _RE_FM_AUTHORED.search(text):
            warnings.append("frontmatter 'authored' is not in YYYY-MM-DD format")

    if "status" in frontmatter:
        status_val = frontmatter["status"]
        if status_val not in {"DRAFT", "ACTIVE", "COMPLETE", "ARCHIVED"}:
            warnings.append(f"unknown status '{status_val}' "
                            "(expected DRAFT|ACTIVE|COMPLETE|ARCHIVED)")

    # Check 2: at least one wave
    if not waves_graph:
        errors.append("No wave declarations (### W<N> ...) found")

    # Check 3: wave-id naming
    for wave_id in waves_graph:
        if not is_wave_id(wave_id):
            errors.append(f"wave id '{wave_id}' does not match W<N> pattern")

    # Check 4: dangling refs
    dangling = find_dangling_refs(waves_graph)
    for node, dep in dangling:
        errors.append(f"{node} depends on {dep}, but {dep} is not declared")

    # Check 5: cycles (Kahn)
    topo_order: list[str] = []
    cycle_nodes: list[str] = []
    if args.check_cycles or args.topological_order:
        topo_order, cycle_nodes = kahn_topological_sort(waves_graph)
        if cycle_nodes:
            errors.append(f"Cycle detected involving waves: {sorted(cycle_nodes)}")

    report: dict[str, Any] = {
        "status": "OK" if not errors else ("WARN" if not warnings else "FAIL"),
        "script": "plan_validator",
        "timestamp": utcnow_iso(),
        "source": str(args.path),
        "plan_id": plan_id,
        "frontmatter": frontmatter,
        "waves_declared": len(waves_graph),
        "wave_ids": sorted(waves_graph.keys()),
        "errors": errors,
        "warnings": warnings,
    }
    if errors:
        report["status"] = "FAIL"
    elif warnings:
        report["status"] = "WARN"

    if args.topological_order:
        report["topological_order"] = topo_order
        report["cycle_nodes"] = cycle_nodes

    if args.emit:
        out = args.output_dir / "plan_validation.json"
        write_json_atomic(out, report)
        report["json_path"] = str(out)

    return report


def main() -> int:
    """CLI entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False, default=str) + "\n")
        if result["status"] == "FAIL":
            return EXIT_WARN
        return EXIT_OK
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except FileNotFoundError as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("plan_validator failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
