#!/usr/bin/env python3
"""Pre-edit GO/CAUTION/NO_GO gate for the /Touring skill (Reflex #1 + C04).

Bundles every Reflex #1/#2/#4 + C04 check into a single decision:
file metadata, blast radius, pre-edit score, TDG grade, gotcha pitfall match,
and memory recall for prior lessons on the file. Returns a structured Verdict
so callers (orchestrator, agent, touring-native tooling) can route accordingly.

Decision matrix:
  GO       — all gates pass; safe to Edit
  CAUTION  — pass with warnings; proceed with plan/mitigation
  NO_GO    — at least one blocking gate failed; do not Edit

Usage:
  pre_edit_gate.py <path>                     # human report
  pre_edit_gate.py <path> --json              # machine JSON
  pre_edit_gate.py <path> --symbols x,y,z     # also VGP-verify these symbols
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    Verdict, add_common_args, emit_error, emit_json, emit_kv, emit_result,
    emit_section, parse_definitions, quality_score, require_touring, resolve_file, touring_run,
)

BLAST_HARD = 20
BLAST_SOFT = 10
QUALITY_HARD = 0.3
QUALITY_SOFT = 0.5
PRE_EDIT_FLOOR = 0.8


def gate(path: Path, *, symbols: list[str], timeout: float) -> dict[str, Any]:
    signals: dict[str, Any] = {}
    reasons: list[str] = []
    warnings: list[str] = []
    decision = "GO"

    meta = touring_run(["ast", "meta", str(path), "--depth", "summary", "-j"], timeout=timeout)
    meta_data = meta.parsed if isinstance(meta.parsed, dict) else {}
    signals["meta"] = meta_data
    blast_radius = int(meta_data.get("blast_radius") or 0)
    _qs = quality_score(path)
    quality = float(_qs.get("composite") or meta_data.get("quality_score") or 0.0)
    cognitive = float(meta_data.get("cognitive_score") or 0.0)

    if blast_radius >= BLAST_HARD:
        reasons.append(f"blast_radius={blast_radius} >= {BLAST_HARD} (hard cap)")
        decision = "NO_GO"
    elif blast_radius >= BLAST_SOFT:
        warnings.append(f"blast_radius={blast_radius} >= {BLAST_SOFT} (soft cap)")
        if decision == "GO":
            decision = "CAUTION"

    if quality and quality < QUALITY_HARD:
        reasons.append(f"quality_score={quality:.2f} < {QUALITY_HARD} (hard floor)")
        decision = "NO_GO"
    elif quality and quality < QUALITY_SOFT:
        warnings.append(f"quality_score={quality:.2f} < {QUALITY_SOFT}")
        if decision == "GO":
            decision = "CAUTION"

    pre = touring_run(["pre-edit"], timeout=timeout)
    pre_data = pre.parsed if isinstance(pre.parsed, dict) else {}
    signals["pre_edit"] = pre_data
    pre_score = float(pre_data.get("score") or 0.0) if pre_data else 0.0
    if pre_score and pre_score < PRE_EDIT_FLOOR:
        reasons.append(f"pre-edit score={pre_score:.2f} < {PRE_EDIT_FLOOR}")
        decision = "NO_GO"

    if path.suffix == ".rs":
        tdg = touring_run(["ast", "tdg", str(path)], timeout=timeout, parse_json=False)
        signals["tdg_text"] = tdg.stdout
        grade = _extract_grade(tdg.stdout)
        signals["tdg_grade"] = grade
        if grade in ("D", "F"):
            reasons.append(f"TDG grade={grade} (D or F blocks edit)")
            decision = "NO_GO"

    gotcha = touring_run(["gotcha", "match", str(path)], timeout=timeout)
    gotcha_data = gotcha.parsed if isinstance(gotcha.parsed, list) else []
    signals["gotcha_hits"] = gotcha_data
    if gotcha_data:
        warnings.append(f"{len(gotcha_data)} gotcha pitfall(s) match this file")
        if decision == "GO":
            decision = "CAUTION"

    memory = touring_run(["memory", "recall", f"edit:{path.name}"], timeout=timeout)
    if memory.ok and memory.parsed:
        entries = memory.parsed.get("entries") if isinstance(memory.parsed, dict) else None
        signals["memory_hits"] = len(entries) if isinstance(entries, list) else 0

    symbols_report: list[dict[str, Any]] = []
    for sym in symbols:
        found = touring_run(["index", "find", sym, "-j"], timeout=timeout)
        defs = parse_definitions(found.parsed)
        symbols_report.append({"symbol": sym, "definitions": len(defs)})
        if not defs:
            warnings.append(f"VGP: symbol '{sym}' not found in index (will need to_be_created)")
    signals["vgp"] = symbols_report

    verdict = Verdict(decision=decision, score=_compose_score(blast_radius, quality, pre_score, cognitive),
                      reasons=reasons, signals={**signals,
                                                 "blast_radius": blast_radius,
                                                 "quality_score": quality,
                                                 "cognitive_score": cognitive,
                                                 "pre_edit_score": pre_score})
    report = {
        "file": str(path),
        "verdict": verdict.to_dict(),
        "warnings": warnings,
    }
    return report


def _extract_grade(tdg_text: str) -> str | None:
    """Pull a single-letter A+..F grade from ``ast tdg`` output."""
    for line in tdg_text.splitlines():
        line = line.strip()
        if "grade" in line.lower():
            for token in line.replace(":", " ").replace("=", " ").split():
                if token in ("A+", "A", "B+", "B", "C+", "C", "D+", "D", "F"):
                    return token
    return None


def _compose_score(blast: int, quality: float, pre: float, cognitive: float) -> float:
    """Compose a 0-1 score (higher = safer)."""
    blast_penalty = min(1.0, blast / 20.0)
    cog_penalty = max(0.0, cognitive - 0.5)
    return max(0.0, min(1.0,
        (quality or 0.5) * 0.4 +
        (pre or 0.5) * 0.4 +
        (1.0 - blast_penalty) * 0.15 +
        (1.0 - cog_penalty) * 0.05
    ))


def emit_human(report: dict[str, Any]) -> None:
    v = report["verdict"]
    s = v["signals"]
    emit_section(f"PRE-EDIT GATE  {report['file']}  →  {v['decision']}")
    emit_kv("decision", v["decision"])
    emit_kv("composite_score", f"{v['score']:.3f}")
    emit_kv("blast_radius", s.get("blast_radius", "?"))
    emit_kv("quality_score", f"{s.get('quality_score') or 0:.3f}")
    emit_kv("cognitive_score", f"{s.get('cognitive_score') or 0:.3f}")
    emit_kv("pre_edit_score", f"{s.get('pre_edit_score') or 0:.3f}")
    if "tdg_grade" in s:
        emit_kv("tdg_grade", s.get("tdg_grade") or "?")
    if "vgp" in s and s["vgp"]:
        emit_section("VGP", char="-")
        for item in s["vgp"]:
            print(f"  {item['symbol']:<32} defs={item['definitions']}")
    if v["reasons"]:
        emit_section("blockers", char="-")
        for r in v["reasons"]:
            print(f"  RED  {r}")
    if report["warnings"]:
        emit_section("warnings", char="-")
        for w in report["warnings"]:
            print(f"  YEL  {w}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                      formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("path", help="File about to be edited")
    parser.add_argument("--symbols", default="",
                        help="Comma-separated symbols to VGP-verify (optional)")
    add_common_args(parser)
    args = parser.parse_args()

    require_touring()
    path = resolve_file(args.path)
    if not path.is_file():
        emit_error(f"not a file: {path}")
        return 2

    symbols = [s.strip() for s in args.symbols.split(",") if s.strip()]
    report = gate(path, symbols=symbols, timeout=args.timeout)
    if not emit_result(report, args):
        emit_human(report)

    d = report["verdict"]["decision"]
    return 0 if d == "GO" else (1 if d == "CAUTION" else 2)


if __name__ == "__main__":
    sys.exit(main())
