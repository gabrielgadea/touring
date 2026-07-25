#!/usr/bin/env python3
"""Code quality sweep for the /Touring skill (C09 + regression detection).

Sweeps a path (file, dir, or workspace) collecting per-file ``ast meta``
(quality_score, cognitive_score, blast_radius) + ``ast tdg`` grade letters,
then ranks hot-spots and detects regressions vs ``touring health-delta status``
streaks. Outputs a triage report: top-N worst, grade distribution, regression
list.

Usage:
  analyze_quality.py                    # cwd recursive
  analyze_quality.py crates/touring     # explicit dir
  analyze_quality.py file.rs            # single file
  analyze_quality.py --top 10 --json    # top 10 worst, machine JSON
  analyze_quality.py --grades-only      # skip ast meta, just TDG grades (faster)
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    add_common_args, emit_error, emit_json, emit_kv, emit_result, emit_section, emit_table,
    iter_code_files, quality_score, require_touring, resolve_file, touring_run, traffic_light,
)

GRADES_ORDER = ("A+", "A", "B+", "B", "C+", "C", "D+", "D", "F")


def gather(target: Path, *, top: int, grades_only: bool, timeout: float) -> dict[str, Any]:
    files = iter_code_files(target, exts={".rs"})
    if not files:
        return {"files": [], "error": "no .rs files under target"}

    per_file: list[dict[str, Any]] = []
    for f in files:
        entry: dict[str, Any] = {"file": str(f.relative_to(target) if target.is_dir() and f.is_relative_to(target) else f)}
        if not grades_only:
            meta = touring_run(["ast", "meta", str(f), "--depth", "summary", "-j"], timeout=timeout)
            md = meta.parsed if isinstance(meta.parsed, dict) else {}
            qs = quality_score(f, timeout=timeout)
            entry["quality_score"] = float(qs.get("composite") or 0.0)
            entry["cognitive_score"] = float(md.get("cognitive_score") or 0.0)
            entry["blast_radius"] = int(md.get("blast_radius") or 0)
            entry["loc"] = int(md.get("loc") or 0)
        tdg = touring_run(["ast", "tdg", str(f), "-j"], timeout=timeout)
        tdg_data = (tdg.parsed or {}).get("tdg") or {} if isinstance(tdg.parsed, dict) else {}
        entry["grade"] = tdg_data.get("grade") or extract_grade(tdg.stdout)
        entry["tdg_composite"] = float(tdg_data.get("composite") or 0.0)
        entry["tdg_action"] = tdg_data.get("action") or ""
        per_file.append(entry)

    hot_spots = rank_hot_spots(per_file, top=top, grades_only=grades_only)
    distribution = grade_distribution(per_file)
    regressions = detect_regressions([e["file"] for e in per_file[:top * 2]], timeout=timeout)

    return {
        "target": str(target),
        "file_count": len(files),
        "hot_spots": hot_spots,
        "grade_distribution": distribution,
        "regressions": regressions,
        "all_files": per_file if not grades_only else None,
    }


def extract_grade(tdg_text: str) -> str | None:
    for line in tdg_text.splitlines():
        ll = line.strip().lower()
        if "grade" in ll:
            for token in line.replace(":", " ").replace("=", " ").split():
                if token in GRADES_ORDER:
                    return token
    return None


def rank_hot_spots(per_file: list[dict[str, Any]], *, top: int, grades_only: bool) -> list[dict[str, Any]]:
    """Composite hot-spot score: low quality × high blast × bad grade."""
    grade_weight = {g: i / (len(GRADES_ORDER) - 1) for i, g in enumerate(GRADES_ORDER)}
    if grades_only:
        return sorted(
            [e for e in per_file if e.get("grade")],
            key=lambda e: grade_weight.get(e.get("grade") or "A+", 0.0),
            reverse=True,
        )[:top]
    scored = []
    for e in per_file:
        q = e.get("quality_score") or 0.0
        c = e.get("cognitive_score") or 0.0
        b = e.get("blast_radius") or 0
        g = grade_weight.get(e.get("grade") or "A+", 0.0)
        score = (1.0 - q) * 0.4 + min(1.0, c) * 0.2 + min(1.0, b / 20.0) * 0.2 + g * 0.2
        scored.append({**e, "hot_score": round(score, 3)})
    return sorted(scored, key=lambda e: e["hot_score"], reverse=True)[:top]


def grade_distribution(per_file: list[dict[str, Any]]) -> dict[str, int]:
    dist = {g: 0 for g in GRADES_ORDER}
    dist["?"] = 0
    for e in per_file:
        g = e.get("grade") or "?"
        dist[g] = dist.get(g, 0) + 1
    return dist


def detect_regressions(files: list[str], *, timeout: float) -> list[dict[str, Any]]:
    """Use ``touring health-delta status`` per file to surface regressions."""
    out: list[dict[str, Any]] = []
    for f in files:
        hd = touring_run(["health-delta", "status", f], timeout=timeout)
        data = hd.parsed if isinstance(hd.parsed, dict) else {}
        streak = data.get("regression_streak") if isinstance(data, dict) else 0
        if isinstance(streak, int) and streak >= 2:
            out.append({"file": f, "regression_streak": streak,
                        "warning_hint": data.get("warning_hint")})
    return out


def emit_human(report: dict[str, Any], top: int) -> None:
    emit_section("QUALITY SWEEP")
    emit_kv("target", report["target"])
    emit_kv("files scanned", report["file_count"])

    emit_section(f"top {top} hot-spots", char="-")
    rows = [[(e["file"])[-50:],
             e.get("grade") or "?",
             f"{e.get('tdg_composite') or 0:.2f}",
             f"{e.get('quality_score') or 0:.2f}",
             e.get("blast_radius", "?"),
             f"{e.get('hot_score', 0):.2f}"]
            for e in report["hot_spots"]]
    emit_table(rows, headers=["file", "grade", "tdg", "qual", "blast", "score"])

    emit_section("grade distribution", char="-")
    for g in GRADES_ORDER + ("?",):
        n = report["grade_distribution"].get(g, 0)
        if n:
            print(f"  {g:<4} {n}")

    if report["regressions"]:
        emit_section("regressions (streak >= 2)", char="-")
        for r in report["regressions"]:
            print(f"  {r['file']}   streak={r['regression_streak']}   "
                  f"hint={r.get('warning_hint') or ''}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                      formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("target", nargs="?", default=".",
                        help="Path to sweep (file, dir, or workspace; default: cwd)")
    parser.add_argument("--top", type=int, default=12,
                        help="Top-N hot-spots to surface (default: 12)")
    parser.add_argument("--grades-only", action="store_true",
                        help="Skip ast meta; just collect TDG grades (faster)")
    add_common_args(parser)
    args = parser.parse_args()

    require_touring()
    target = resolve_file(args.target)
    report = gather(target, top=args.top, grades_only=args.grades_only, timeout=args.timeout)
    if report.get("error"):
        emit_error(report["error"])
        return 2
    spots = report.get("hot_spots") or []
    avg_q = sum(e.get("quality_score", 0.0) or 0.0 for e in spots) / max(1, len(spots))
    report["overall_light"] = traffic_light(avg_q, good=0.7, warn=0.4)
    if not emit_result(report, args):
        emit_human(report, top=args.top)
        print(f"\n  overall traffic-light: {report['overall_light']}  "
              f"(avg hot-spot quality {avg_q:.2f})")
    return 0 if not report.get("regressions") else 1


if __name__ == "__main__":
    sys.exit(main())
