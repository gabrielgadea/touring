#!/usr/bin/env python3
"""dag_builder — Extract DAG from plan markdown and emit Mermaid + textual.

Parses the plan markdown to discover Phase declarations and their dependencies
(extracted from inline text or a `depends_on` row), then emits:
  * Mermaid `graph LR` diagram with one node per phase + one per subtask
  * Textual sequence (`P1 -> P2 -> ...`)
  * Cycle detection via Kahn's algorithm

Optionally rewrites the plan's Section 4 (DAG) with the emitted content.

Usage
-----
    python3 dag_builder.py plan.md -j
    python3 dag_builder.py plan.md --inject --apply   # mutates Section 4
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
    utcnow_iso,
    write_json_atomic,
)

_RE_PHASE_HEADING = re.compile(
    r"^###\s+Phase\s+(\d+)\s+[—\-:]\s*(.+?)(?:\s+\(([^)]+)\))?\s*$",
    re.MULTILINE | re.IGNORECASE,
)
_RE_DEPENDS_LINE = re.compile(r"\*\*Depends on\*\*:\s*([^\n]+)", re.IGNORECASE)
_RE_SECTION_DAG = re.compile(r"(##\s+\d+\.\s*DAG[^\n]*\n)", re.IGNORECASE)
_RE_NEXT_SECTION = re.compile(r"^##\s+\d+\.", re.MULTILINE)


def parse_phases(plan_md: str) -> list[dict[str, Any]]:
    """Extract phases with dependencies from plan markdown."""
    matches = list(_RE_PHASE_HEADING.finditer(plan_md))
    phases: list[dict[str, Any]] = []
    for idx, match in enumerate(matches):
        number = int(match.group(1))
        name = match.group(2).strip()
        meta = match.group(3) or ""
        mode = "parallel" if "parallel" in meta.lower() else "sequential"
        start = match.start()
        end = matches[idx + 1].start() if idx + 1 < len(matches) else len(plan_md)
        body = plan_md[start:end]
        deps_match = _RE_DEPENDS_LINE.search(body)
        depends_on: list[int] = []
        if deps_match:
            tokens = re.findall(r"P(?:hase\s+)?(\d+)", deps_match.group(1), re.IGNORECASE)
            depends_on = [int(t) for t in tokens]
        else:
            # Sequential default: implicit dependency on previous phase
            if number > 1 and mode == "sequential":
                depends_on = [number - 1]
        phases.append({
            "number": number,
            "name": name,
            "mode": mode,
            "depends_on": depends_on,
        })
    return phases


def kahn_sort(phases: list[dict[str, Any]]) -> tuple[list[int], list[int]]:
    """Kahn's topological sort. Returns (sorted, cycle_nodes)."""
    nodes = {p["number"] for p in phases}
    deps = {p["number"]: list(p.get("depends_on", [])) for p in phases}
    in_degree = {n: 0 for n in nodes}
    adjacency: dict[int, list[int]] = {n: [] for n in nodes}
    for n, deplist in deps.items():
        for d in deplist:
            if d in nodes:
                adjacency[d].append(n)
                in_degree[n] += 1
    queue: deque[int] = deque(sorted(n for n in nodes if in_degree[n] == 0))
    order: list[int] = []
    while queue:
        cur = queue.popleft()
        order.append(cur)
        for nxt in sorted(adjacency.get(cur, [])):
            in_degree[nxt] -= 1
            if in_degree[nxt] == 0:
                queue.append(nxt)
    cycle_nodes = [n for n, deg in in_degree.items() if deg > 0]
    return order, cycle_nodes


def build_mermaid(phases: list[dict[str, Any]]) -> str:
    """Generate a `graph LR` Mermaid diagram from phases."""
    if not phases:
        return "graph LR\n  empty([no phases])"
    lines = ["graph LR"]
    for p in phases:
        label = f"P{p['number']}[{p['name']} ({p['mode']})]"
        lines.append(f"  {label}")
    edges_emitted: set[tuple[int, int]] = set()
    for p in phases:
        for dep in p.get("depends_on", []):
            edge = (dep, p["number"])
            if edge in edges_emitted:
                continue
            edges_emitted.add(edge)
            lines.append(f"  P{dep} --> P{p['number']}")
    if not edges_emitted and len(phases) > 1:
        # Implicit linear: P1 -> P2 -> ...
        for i in range(len(phases) - 1):
            lines.append(f"  P{phases[i]['number']} --> P{phases[i + 1]['number']}")
    return "\n".join(lines)


def build_textual(phases: list[dict[str, Any]], order: list[int]) -> str:
    """Generate a textual sequence like 'P1 -> P2 -> ...'."""
    if not order:
        return "(no phases)"
    by_num = {p["number"]: p for p in phases}
    parts: list[str] = []
    for num in order:
        phase = by_num.get(num)
        if phase is None:
            continue
        parts.append(f"P{num} ({phase['mode']}: {phase['name']})")
    return " -> ".join(parts)


def inject_dag_section(plan_md: str, mermaid: str, textual: str) -> str:
    """Replace the body of section '## N. DAG' with the new content."""
    match = _RE_SECTION_DAG.search(plan_md)
    if not match:
        # Append at end
        return (
            plan_md.rstrip() + "\n\n"
            "## 4. DAG (injected)\n\n"
            "```mermaid\n" + mermaid + "\n```\n\n"
            "Textual sequence:\n" + textual + "\n"
        )
    section_start = match.end()
    next_match = _RE_NEXT_SECTION.search(plan_md, pos=section_start)
    section_end = next_match.start() if next_match else len(plan_md)
    new_body = (
        "\n```mermaid\n" + mermaid + "\n```\n\n"
        "Textual sequence:\n" + textual + "\n\n"
    )
    return plan_md[:section_start] + new_body + plan_md[section_end:]


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="dag_builder", description=__doc__)
    parser.add_argument("path", type=Path, help="Plan markdown.")
    parser.add_argument("--inject", action="store_true",
                        help="Insert/replace the DAG section in the plan.")
    parser.add_argument("--apply", action="store_true",
                        help="MUTATING — rewrite the file when combined with --inject.")
    parser.add_argument("--output-dir", type=Path, default=Path("data"))
    parser.add_argument("--emit", action="store_true",
                        help="Write data/dag.json.")
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Build."""
    if not args.path.exists():
        msg = f"Plan file not found: {args.path}"
        raise FileNotFoundError(msg)
    plan_md = args.path.read_text(encoding="utf-8")
    phases = parse_phases(plan_md)
    order, cycles = kahn_sort(phases)
    mermaid = build_mermaid(phases)
    textual = build_textual(phases, order)

    mutated = False
    if args.inject and args.apply:
        rewritten = inject_dag_section(plan_md, mermaid, textual)
        args.path.write_text(rewritten, encoding="utf-8")
        mutated = True

    report = {
        "status": "OK" if not cycles else "WARN",
        "script": "dag_builder",
        "timestamp": utcnow_iso(),
        "source": str(args.path),
        "phase_count": len(phases),
        "phases": phases,
        "topological_order": order,
        "cycle_nodes": cycles,
        "mermaid": mermaid,
        "textual": textual,
        "mutated": mutated,
    }
    if args.emit:
        out = args.output_dir / "dag.json"
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
        if result.get("cycle_nodes"):
            return EXIT_WARN
        return EXIT_OK
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except FileNotFoundError as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("dag_builder failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
