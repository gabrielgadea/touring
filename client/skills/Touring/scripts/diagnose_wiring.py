#!/usr/bin/env python3
"""Wiring forensics for the /Touring skill (REGRA #0 + Cadeia 7).

Composes ``touring wiring orphans``, ``touring wiring chains``, and ``touring
wiring audit`` into a single forensic report — and for every claimed orphan,
runs the VP-Scout Cadeia 7 anti-stale grep verification (the wiring DB can lag
behind recent edits, producing false-positive orphans). Outputs real vs stale
orphan classification so callers do not waste time wiring symbols that already
have consumers.

Usage:
  diagnose_wiring.py                    # full report
  diagnose_wiring.py --json             # machine JSON
  diagnose_wiring.py --verify-stale     # only do the Cadeia 7 grep verify
  diagnose_wiring.py --top 30           # cap orphan output
"""
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    add_common_args, emit_error, emit_json, emit_kv, emit_result, emit_section, emit_table,
    find_workspace_root, mark_degraded, require_touring, touring_run,
)


def gather(timeout: float, *, top: int, verify_stale: bool,
           workspace: Path | None) -> dict[str, Any]:
    report: dict[str, Any] = {"workspace": str(workspace) if workspace else None}

    orphans = touring_run(["wiring", "orphans", "-j"], timeout=timeout)
    report["daemon_degraded"] = orphans.daemon_degraded
    orphans_data = orphans.parsed if isinstance(orphans.parsed, dict) else {}
    items = orphans_data.get("orphans") or []
    if not isinstance(items, list):
        items = []
    report["orphan_count_raw"] = int(orphans_data.get("orphan_count") or len(items))
    report["dead_patterns"] = orphans_data.get("dead_patterns") or []

    if not verify_stale:
        audit = touring_run(["wiring", "audit", "-j"], timeout=timeout * 2)
        report["audit"] = audit.parsed if isinstance(audit.parsed, dict) else {"raw": audit.stdout}

        chains = touring_run(["wiring", "chains"], timeout=timeout, parse_json=False)
        report["chains_text_head"] = chains.stdout.splitlines()[:20] if chains.ok else []

    sample = items[:top]
    verified: list[dict[str, Any]] = []
    for entry in sample:
        if not isinstance(entry, dict):
            continue
        sym = entry.get("symbol_name") or entry.get("symbol")
        if not sym:
            continue
        cadeia7 = grep_consumers(sym, workspace) if workspace else None
        verified.append({
            "symbol": sym,
            "file": entry.get("module_file") or entry.get("file_path"),
            "kind": entry.get("symbol_kind"),
            "visibility": entry.get("visibility"),
            "grep_consumers": cadeia7,
            "verdict": "STALE_WIRING" if cadeia7 and cadeia7["count"] > 0 else "REAL_ORPHAN",
        })
    report["orphan_sample"] = verified
    report["sample_size"] = len(verified)
    report["stale_count"] = sum(1 for v in verified if v["verdict"] == "STALE_WIRING")
    report["real_orphan_count"] = sum(1 for v in verified if v["verdict"] == "REAL_ORPHAN")
    return report


def grep_consumers(symbol: str, workspace: Path) -> dict[str, Any]:
    """Run Cadeia 7: grep -rn for symbol across rust/python/ts."""
    try:
        proc = subprocess.run(
            ["grep", "-rnl", "--include", "*.rs", "--include", "*.py",
             "--include", "*.ts", "--include", "*.tsx", symbol, str(workspace)],
            capture_output=True, text=True, timeout=15.0,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired, OSError):
        return {"count": 0, "files": [], "error": "grep_failed"}
    files = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
    return {"count": len(files), "files": files[:8]}


def emit_human(report: dict[str, Any]) -> None:
    emit_section("WIRING FORENSICS")
    emit_kv("orphan_count_raw", report["orphan_count_raw"])
    emit_kv("sample_size", report["sample_size"])
    emit_kv("stale_count (verified)", report["stale_count"])
    emit_kv("real_orphan_count", report["real_orphan_count"])

    if report["orphan_sample"]:
        emit_section("orphan sample (Cadeia 7 verified)", char="-")
        rows = [[v["symbol"][:32],
                 (v.get("file") or "?")[:38],
                 (v.get("kind") or "?")[:10],
                 v["verdict"],
                 v["grep_consumers"]["count"] if v.get("grep_consumers") else "?"]
                for v in report["orphan_sample"]]
        emit_table(rows, headers=["symbol", "file", "kind", "verdict", "grep_n"])

    if "audit" in report and isinstance(report["audit"], dict):
        emit_section("wiring audit summary", char="-")
        a = report["audit"]
        for k in ("modules_total", "orphan_total", "low_score_count", "avg_score"):
            if k in a:
                emit_kv(k, a[k])

    if report.get("chains_text_head"):
        emit_section("chains (head)", char="-")
        for line in report["chains_text_head"][:8]:
            print(f"  {line}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                      formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--top", type=int, default=20,
                        help="Sample size for Cadeia 7 verification (default: 20)")
    parser.add_argument("--verify-stale", action="store_true",
                        help="Skip audit + chains; only run Cadeia 7 verify")
    parser.add_argument("--workspace", default=None,
                        help="Workspace root for grep (default: auto-detect)")
    add_common_args(parser)
    args = parser.parse_args()

    require_touring()
    ws = Path(args.workspace).expanduser().resolve() if args.workspace else find_workspace_root()
    if not ws:
        emit_error("no Cargo workspace detected — Cadeia 7 grep verification will be skipped")
    report = gather(args.timeout, top=args.top, verify_stale=args.verify_stale, workspace=ws)
    # I-fail-soft: annotate so a consumer never reads a degraded-daemon result as authoritative.
    mark_degraded(report, report.get("daemon_degraded", False))
    # I-density: --brief (compact digest) > --json (full); else human fallback.
    if not emit_result(report, args):
        emit_human(report)
    return 0 if report["real_orphan_count"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
