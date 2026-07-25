#!/usr/bin/env python3
"""Premium one-shot file reading for the /Touring skill (Reflex #4 + C02).

Replaces the manual sequence ``ast meta + ast blast + ast tdg + ast rust-semantic
+ ast overview`` with a single call that aggregates all signals into one
structured report. Implements File Metadata First (the Golden Rule) plus
Reading-Comprehend (C02) in the Touring Decision Matrix.

Usage:
  read_file.py <path>                  # human report
  read_file.py <path> --json           # machine JSON
  read_file.py <path> --depth full     # ast meta --depth full instead of summary
  read_file.py <path> --no-semantic    # skip Rust syn semantic call
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    add_common_args, emit_error, emit_json, emit_kv, emit_result, emit_section,
    emit_table, is_rust_file, mark_degraded, quality_score, require_touring,
    resolve_file, touring_run, traffic_light,
)


def gather(path: Path, *, depth: str, with_semantic: bool, timeout: float) -> dict[str, Any]:
    """Collect every premium signal Touring exposes for ``path``."""
    report: dict[str, Any] = {"file": str(path), "depth": depth, "signals": {}}

    meta = touring_run(["ast", "meta", str(path), "--depth", depth, "-j"], timeout=timeout)
    report["signals"]["meta"] = {
        "ok": meta.ok,
        "data": meta.parsed,
        "daemon_degraded": meta.daemon_degraded,
    }

    overview = touring_run(["ast", "overview", str(path), "-j"], timeout=timeout)
    report["signals"]["overview"] = {
        "ok": overview.ok,
        "data": overview.parsed,
    }

    blast = touring_run(["ast", "blast", str(path)], timeout=timeout, parse_json=False)
    report["signals"]["blast"] = {
        "ok": blast.ok,
        "text": blast.stdout if blast.ok else blast.stderr,
    }

    if is_rust_file(path):
        tdg = touring_run(["ast", "tdg", str(path)], timeout=timeout, parse_json=False)
        report["signals"]["tdg"] = {"ok": tdg.ok, "text": tdg.stdout}

        if with_semantic:
            sem = touring_run(["ast", "rust-semantic", str(path)], timeout=timeout)
            report["signals"]["rust_semantic"] = {"ok": sem.ok, "data": sem.parsed}

    fk = touring_run(["file-knowledge", "extended", str(path)], timeout=timeout)
    report["signals"]["file_knowledge"] = {"ok": fk.ok, "data": fk.parsed}

    # I-correctness: 50-dim authority replaces ast meta quality_score proxy.
    qs = quality_score(path)
    report["signals"]["quality_score_50d"] = qs

    report["verdict"] = score_verdict(report["signals"])
    return report


def score_verdict(signals: dict[str, Any]) -> dict[str, Any]:
    """Build a triage verdict from blast_radius + quality_score thresholds."""
    meta = (signals.get("meta") or {}).get("data") or {}
    _qs50 = signals.get("quality_score_50d") or {}
    quality = float(_qs50.get("composite") or meta.get("quality_score") or 0.0)
    blast_radius = int(meta.get("blast_radius") or 0)
    cognitive = float(meta.get("cognitive_score") or 0.0)
    fan_in = int(meta.get("fan_in") or 0)
    fan_out = int(meta.get("fan_out") or 0)

    reasons: list[str] = []
    if blast_radius > 10:
        reasons.append(f"blast_radius={blast_radius} > 10 — pause; consider scope reduction")
    if quality < 0.5:
        reasons.append(f"quality_score={quality:.2f} < 0.5 — robustness work first")
    if cognitive > 0.7:
        reasons.append(f"cognitive_score={cognitive:.2f} > 0.7 — high complexity")
    if blast_radius > 10 and quality < 0.5:
        decision = "NO_GO"
    elif reasons:
        decision = "CAUTION"
    else:
        decision = "GO"
    return {
        "decision": decision,
        "blast_radius": blast_radius,
        "quality_score": quality,
        "cognitive_score": cognitive,
        "fan_in": fan_in,
        "fan_out": fan_out,
        "light": traffic_light(quality),
        "reasons": reasons,
    }


def emit_human(report: dict[str, Any]) -> None:
    """Render a human-readable report to stdout."""
    emit_section(f"READ-COMPREHEND  {report['file']}")
    meta = (report["signals"].get("meta") or {}).get("data") or {}
    verdict = report["verdict"]
    emit_kv("decision", f"{verdict['decision']}  ({verdict['light']})")
    emit_kv("blast_radius", verdict["blast_radius"])
    emit_kv("quality_score", f"{verdict['quality_score']:.3f}")
    emit_kv("cognitive_score", f"{verdict['cognitive_score']:.3f}")
    emit_kv("fan_in / fan_out", f"{verdict['fan_in']} / {verdict['fan_out']}")
    if meta.get("language"):
        emit_kv("language", meta["language"])
    if meta.get("loc"):
        emit_kv("LOC", meta["loc"])
    if verdict["reasons"]:
        emit_section("triage signals", char="-")
        for r in verdict["reasons"]:
            print(f"  - {r}")

    overview = (report["signals"].get("overview") or {}).get("data") or {}
    syms = overview.get("symbols") if isinstance(overview, dict) else None
    if isinstance(syms, list) and syms:
        emit_section("top symbols (first 12)", char="-")
        rows = []
        for s in syms[:12]:
            if isinstance(s, dict):
                rows.append([s.get("kind", "?"), s.get("name", "?"),
                             str(s.get("visibility", "")), str(s.get("line", ""))])
        emit_table(rows, headers=["kind", "name", "vis", "line"])

    sem = (report["signals"].get("rust_semantic") or {}).get("data") if "rust_semantic" in report["signals"] else None
    if isinstance(sem, dict):
        emit_section("rust semantic (syn)", char="-")
        for key in ("semantic_complexity", "unsafe_count", "async_count",
                    "generic_param_count", "trait_bound_count", "lifetime_count"):
            if key in sem:
                emit_kv(key, sem[key])

    tdg_text = (report["signals"].get("tdg") or {}).get("text") if "tdg" in report["signals"] else ""
    if tdg_text:
        emit_section("TDG grade", char="-")
        for line in tdg_text.strip().splitlines()[:8]:
            print(f"  {line}")

    blast_text = (report["signals"].get("blast") or {}).get("text") or ""
    if blast_text:
        emit_section("blast radius (truncated 12L)", char="-")
        for line in blast_text.strip().splitlines()[:12]:
            print(f"  {line}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                      formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("path", help="File to inspect")
    parser.add_argument("--depth", choices=["summary", "full"], default="summary",
                        help="ast meta depth (default: summary)")
    parser.add_argument("--no-semantic", action="store_true",
                        help="Skip ast rust-semantic call for Rust files")
    add_common_args(parser)
    args = parser.parse_args()

    require_touring()
    path = resolve_file(args.path)
    if not path.is_file():
        emit_error(f"not a file: {path}")
        return 2

    report = gather(path, depth=args.depth, with_semantic=not args.no_semantic,
                    timeout=args.timeout)
    # I-fail-soft: meta.daemon_degraded is surfaced in signals["meta"]["daemon_degraded"].
    mark_degraded(report, bool((report["signals"].get("meta") or {}).get("daemon_degraded")))
    # I-density: --brief (compact digest) > --json (full); else human fallback.
    if not emit_result(report, args):
        emit_human(report)

    verdict = report["verdict"]["decision"]
    return 0 if verdict == "GO" else (1 if verdict == "CAUTION" else 2)


if __name__ == "__main__":
    sys.exit(main())
