#!/usr/bin/env python3
"""touring investigate <topic> — R5 topic map for the Touring skill.

Fuses BM25 search + symbol index + wiring neighborhood + institutional memory
into one compact "where does <topic> live and what touches it" map. Designed for
both ad-hoc orientation and SessionStart auto-invocation (use ``--brief``).

Honors the three R2 promotion invariants:
  - I-density   : ``--brief`` default-friendly digest (<2KB; arrays elided).
  - I-fail-soft : degraded daemon/index → VP-Scout Chain 7 grep fallback + degraded:true.
  - I-correctness: uses the touring index/wiring/memory sources of truth, never a proxy.

Exit codes:
  0  topic mapped (hits found OR a clean empty result with a healthy daemon)
  3  degraded — daemon/index unavailable; map backed by grep fallback, degraded:true

Usage:
  investigate.py <topic>            # human report
  investigate.py <topic> --json     # full machine JSON
  investigate.py <topic> --brief    # compact digest (<2KB; SessionStart-friendly)
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    add_common_args, emit_kv, emit_result, emit_section, find_workspace_root,
    grep_fallback, mark_degraded, parse_definitions, require_touring, touring_run,
)

# Cap every list dimension so the map stays a map, not a dump (I-density).
TOP_HITS = 10


def _normalize_hits(parsed: Any) -> list[Any]:
    """Coerce a tantivy/search payload into a flat list of hit records.

    Touring search output has drifted across versions (bare list, ``{"hits": []}``,
    ``{"results": []}``); accept all three so the map never breaks on shape.
    """
    if isinstance(parsed, list):
        return parsed[:TOP_HITS]
    if isinstance(parsed, dict):
        for key in ("hits", "results", "matches"):
            value = parsed.get(key)
            if isinstance(value, list):
                return value[:TOP_HITS]
    return []


def _normalize_symbols(parsed: Any) -> list[dict[str, Any]]:
    """Coerce an ``index find -j`` payload into a list of ``{name, ...}`` records."""
    return parse_definitions(parsed)[:TOP_HITS]


def _anchor_symbol(symbols: list[dict[str, Any]], topic: str) -> str:
    """Pick the symbol whose wiring neighborhood best represents the topic."""
    if symbols:
        name = symbols[0].get("name") or symbols[0].get("symbol")
        if isinstance(name, str) and name:
            return name
    return topic


def gather(topic: str, *, timeout: float) -> dict[str, Any]:
    """Build the topic map from search + index + wiring + memory signals.

    Fail-soft: any degraded signal flips ``degraded`` and triggers a Chain-7 grep
    fallback so an empty map is never reported with false confidence.
    """
    report: dict[str, Any] = {"topic": topic, "signals": {}}
    degraded = False

    search = touring_run(["tantivy", "search", topic], timeout=timeout)
    hits = _normalize_hits(search.parsed)
    report["signals"]["search"] = {"ok": search.ok, "count": len(hits), "hits": hits}
    degraded = degraded or not search.ok

    index = touring_run(["index", "find", topic, "-j"], timeout=timeout)
    symbols = _normalize_symbols(index.parsed) if index.ok else []
    report["signals"]["symbols"] = symbols
    degraded = degraded or not index.ok

    anchor = _anchor_symbol(symbols, topic)
    impact = touring_run(["wiring", "impact", anchor, "--depth", "2"], timeout=timeout)
    report["signals"]["wiring"] = {
        "anchor": anchor,
        "ok": impact.ok,
        "data": impact.parsed if impact.ok else None,
    }

    memory = touring_run(["memory", "recall", topic], timeout=timeout, parse_json=False)
    recall = memory.stdout.strip().splitlines()[:TOP_HITS] if memory.ok else []
    report["signals"]["memory"] = {"ok": memory.ok, "recall": recall}

    if degraded:
        root = find_workspace_root() or Path.cwd()
        report["signals"]["grep_fallback"] = grep_fallback(topic, root)

    report["summary"] = {
        "search_hits": len(hits),
        "symbol_matches": len(symbols),
        "has_wiring": bool(impact.ok),
        "memory_lines": len(recall),
    }
    mark_degraded(
        report,
        degraded,
        "daemon/index degraded — map backed by grep fallback" if degraded else "",
    )
    return report


def emit_human(report: dict[str, Any]) -> None:
    """Render the topic map as a readable report (the no-flag default)."""
    summary = report["summary"]
    emit_section(f"TOURING INVESTIGATE  ·  {report['topic']}")
    emit_kv("search_hits", summary["search_hits"])
    emit_kv("symbol_matches", summary["symbol_matches"])
    emit_kv("has_wiring", summary["has_wiring"])
    emit_kv("memory_lines", summary["memory_lines"])
    emit_kv("degraded", report.get("degraded", False))

    symbols = report["signals"]["symbols"]
    if symbols:
        emit_section("symbols", char="-")
        for sym in symbols:
            name = sym.get("name") or sym.get("symbol") or "?"
            loc = sym.get("file_path") or sym.get("file") or ""
            emit_kv(str(name), loc)

    recall = report["signals"]["memory"]["recall"]
    if recall:
        emit_section("memory recall", char="-")
        for line in recall:
            print(f"  {line}")


def main() -> int:
    """Parse args, build the topic map, and emit it; exit 3 when degraded."""
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("topic", help="The symbol, feature, or concept to map")
    add_common_args(parser)
    args = parser.parse_args()

    require_touring()
    report = gather(args.topic, timeout=args.timeout)

    if not emit_result(report, args):
        emit_human(report)
    return 3 if report.get("degraded") else 0


if __name__ == "__main__":
    sys.exit(main())
