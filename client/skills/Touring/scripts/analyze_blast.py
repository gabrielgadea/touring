#!/usr/bin/env python3
"""Multi-file blast radius analysis for the /Touring skill (C06 + C11).

Takes one or more files and composes ``touring ast blast`` (local tree),
``touring wiring impact`` (transitive BFS for symbols touched), ``touring ast
blast-cross-feature`` (cross-feature gates), and ``touring wiring cycles``
(Tarjan SCC) into a single risk report. Used before any L3+ refactor.

Risk levels per file:
  LOW       — blast_radius < 5, no cycles, no cross-feature edges
  MEDIUM    — blast_radius < 15, or cross-feature edges present
  HIGH      — blast_radius >= 15, or appears in any cycle, or both
  CRITICAL  — HIGH + low quality_score (< 0.4)

Usage:
  analyze_blast.py file.rs
  analyze_blast.py file1.rs file2.rs file3.rs
  analyze_blast.py file.rs --depth 3 --cycles
  analyze_blast.py file.rs --json
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    add_common_args, emit_error, emit_json, emit_kv, emit_result, emit_section, emit_table,
    mark_degraded, require_touring, resolve_file, touring_run,
)


def analyze_one(path: Path, *, depth: int, timeout: float) -> dict[str, Any]:
    entry: dict[str, Any] = {"file": str(path)}

    meta = touring_run(["ast", "meta", str(path), "--depth", "summary", "-j"], timeout=timeout)
    meta_data = meta.parsed if isinstance(meta.parsed, dict) else {}
    entry["meta"] = {
        "blast_radius": int(meta_data.get("blast_radius") or 0),
        "quality_score": float(meta_data.get("quality_score") or 0.0),
        "cognitive_score": float(meta_data.get("cognitive_score") or 0.0),
        "fan_in": int(meta_data.get("fan_in") or 0),
        "fan_out": int(meta_data.get("fan_out") or 0),
    }

    blast = touring_run(["ast", "blast", str(path)], timeout=timeout, parse_json=False)
    entry["blast_lines"] = len(blast.stdout.splitlines()) if blast.ok else 0
    entry["blast_text_head"] = blast.stdout.splitlines()[:6] if blast.ok else []

    cf = touring_run(["ast", "blast-cross-feature", str(path)], timeout=timeout)
    cf_data = cf.parsed if isinstance(cf.parsed, dict) else {}
    entry["cross_feature"] = {
        "feature_count": len(cf_data.get("features") or []) if isinstance(cf_data, dict) else 0,
        "symbol_count": len(cf_data.get("symbols") or []) if isinstance(cf_data, dict) else 0,
    }

    overview = touring_run(["ast", "overview", str(path), "-j"], timeout=timeout)
    syms = overview.parsed.get("symbols") if isinstance(overview.parsed, dict) else None
    top_syms = []
    if isinstance(syms, list):
        for s in syms[:5]:
            if isinstance(s, dict) and s.get("visibility") == "pub":
                top_syms.append(s.get("name"))
    impact_summary: list[dict[str, Any]] = []
    for sym in top_syms[:3]:
        if not sym:
            continue
        imp = touring_run(["wiring", "impact", sym, "--depth", str(depth)], timeout=timeout)
        imp_data = imp.parsed if isinstance(imp.parsed, dict) else {}
        impact_summary.append({
            "symbol": sym,
            "direct_consumers": imp_data.get("direct_consumers"),
            "max_depth": imp_data.get("max_depth"),
        })
    entry["transitive_impact"] = impact_summary

    entry["risk"] = classify_risk(entry)
    return entry


def classify_risk(entry: dict[str, Any]) -> dict[str, Any]:
    m = entry["meta"]
    blast = m["blast_radius"]
    quality = m["quality_score"]
    cf = entry["cross_feature"]["feature_count"]
    reasons: list[str] = []

    if blast >= 15:
        level = "HIGH"
        reasons.append(f"blast_radius={blast} >= 15")
    elif blast >= 5 or cf > 0:
        level = "MEDIUM"
        if blast >= 5:
            reasons.append(f"blast_radius={blast} >= 5")
        if cf > 0:
            reasons.append(f"cross-feature edges={cf}")
    else:
        level = "LOW"

    if level == "HIGH" and quality < 0.4:
        level = "CRITICAL"
        reasons.append(f"quality_score={quality:.2f} < 0.4 amplifies risk")

    return {"level": level, "reasons": reasons}


def cycle_scan(timeout: float) -> dict[str, Any]:
    cy = touring_run(["wiring", "cycles", "--min-depth", "2", "--format", "json"], timeout=timeout)
    cy_data = cy.parsed if isinstance(cy.parsed, dict) else {}
    return {
        "cycle_count": cy_data.get("cycle_count", 0) if isinstance(cy_data, dict) else 0,
        "cycles": (cy_data.get("cycles") or [])[:6] if isinstance(cy_data, dict) else [],
    }


def emit_human(report: dict[str, Any]) -> None:
    emit_section("BLAST ANALYSIS")
    emit_kv("files analyzed", len(report["files"]))
    if report.get("cycles"):
        emit_kv("global cycle_count", report["cycles"]["cycle_count"])

    emit_section("per-file risk", char="-")
    rows = [[Path(f["file"]).name,
             f["meta"]["blast_radius"],
             f"{f['meta']['quality_score']:.2f}",
             f["cross_feature"]["feature_count"],
             f["risk"]["level"]]
            for f in report["files"]]
    emit_table(rows, headers=["file", "blast", "quality", "x-feat", "risk"])

    for f in report["files"]:
        if f["transitive_impact"]:
            emit_section(f"impact — {Path(f['file']).name}", char=".")
            for imp in f["transitive_impact"]:
                print(f"  {imp['symbol']:<32} direct={imp.get('direct_consumers')}  "
                      f"depth={imp.get('max_depth')}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                      formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("files", nargs="+", help="Files to analyze")
    parser.add_argument("--depth", type=int, default=2,
                        help="wiring impact BFS depth (default: 2)")
    parser.add_argument("--cycles", action="store_true",
                        help="Include global Tarjan SCC cycle scan")
    add_common_args(parser)
    args = parser.parse_args()

    require_touring()
    paths = [resolve_file(f) for f in args.files]
    report: dict[str, Any] = {
        "files": [analyze_one(p, depth=args.depth, timeout=args.timeout)
                  for p in paths]
    }
    for entry in report["files"]:
        m = entry.get("meta") or {}
        if m.get("blast_radius") == 0 and m.get("quality_score") == 0.0:
            emit_error(f"empty signals for {entry['file']} — daemon_degraded or unindexed?")
    if args.cycles:
        report["cycles"] = cycle_scan(args.timeout)

    any_degraded = any(
        (e.get("meta") or {}).get("blast_radius") == 0
        and (e.get("meta") or {}).get("quality_score") == 0.0
        for e in report["files"]
    )
    mark_degraded(report, any_degraded)
    if not emit_result(report, args):
        emit_human(report)

    max_risk = max((f["risk"]["level"] for f in report["files"]),
                    key=lambda lv: ("LOW", "MEDIUM", "HIGH", "CRITICAL").index(lv),
                    default="LOW")
    return {"LOW": 0, "MEDIUM": 0, "HIGH": 1, "CRITICAL": 2}[max_risk]


if __name__ == "__main__":
    sys.exit(main())
