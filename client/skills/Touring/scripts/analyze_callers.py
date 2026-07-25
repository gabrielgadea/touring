#!/usr/bin/env python3
"""Cross-caller asymmetry analysis for the /Touring skill (Cadeia C08).

Takes 2+ function/symbol names and produces a side-by-side matrix showing
what each function calls. Asymmetric cells (function A calls X, function B
does not) reveal the canonical asymmetry-bug pattern this is designed to
catch — the exact bug class that cost ~40min of reactive iteration in the
2026-05-10 post-mortem that birthed Cadeia C08.

For each function, the script:
  1. ``touring ast find <fn>`` — locate the definition
  2. ``touring index find <fn>`` — VGP existence
  3. Reads the function body (best-effort line range from ast find)
  4. Extracts likely call sites (regex on identifier-followed-by-paren)
  5. Diffs the call set across all input functions

Usage:
  analyze_callers.py fn_a fn_b                     # 2-way compare
  analyze_callers.py fn_a fn_b fn_c                # 3-way
  analyze_callers.py --json fn_a fn_b              # machine JSON
  analyze_callers.py --bodies fn_a fn_b            # also print bodies
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    add_common_args, emit_error, emit_json, emit_kv, emit_result, emit_section, emit_table,
    parse_definitions, require_touring, touring_run,
)

CALL_RE = re.compile(r"\b([a-z_][a-zA-Z0-9_]*)\s*\(")
SKIP_NAMES = {"if", "while", "for", "match", "return", "let", "fn", "pub", "use",
              "mod", "impl", "struct", "enum", "trait", "self", "Self", "as", "in",
              "loop", "ref", "mut", "move", "async", "await", "where"}


def locate(symbol: str, timeout: float) -> dict[str, Any] | None:
    """Use ``touring ast find`` to locate a single canonical definition."""
    ast = touring_run(["ast", "find", symbol, "-j"], timeout=timeout)
    hits = parse_definitions(ast.parsed)
    if not hits:
        return None
    for h in hits:
        if isinstance(h, dict) and h.get("kind") in ("function", "method", "fn", "Fn"):
            return h
    return hits[0] if isinstance(hits[0], dict) else None


def read_body(loc: dict[str, Any]) -> list[str]:
    """Best-effort body extraction from file_path + start_line + end_line."""
    fp = loc.get("file_path") or loc.get("file")
    start = loc.get("start_line") or loc.get("line")
    end = loc.get("end_line")
    if not (fp and start):
        return []
    try:
        lines = Path(fp).read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return []
    s = max(0, int(start) - 1)
    e = int(end) if end else min(len(lines), s + 80)
    return lines[s:e]


def extract_calls(body_lines: list[str]) -> set[str]:
    """Pull likely call sites from a function body — identifier(... ."""
    calls: set[str] = set()
    for line in body_lines:
        stripped = line.split("//", 1)[0]
        for m in CALL_RE.finditer(stripped):
            name = m.group(1)
            if name not in SKIP_NAMES:
                calls.add(name)
    return calls


def gather(symbols: list[str], *, with_bodies: bool, timeout: float) -> dict[str, Any]:
    entries: list[dict[str, Any]] = []
    for sym in symbols:
        loc = locate(sym, timeout)
        body = read_body(loc) if loc else []
        calls = extract_calls(body) if body else set()
        idx = touring_run(["index", "find", sym, "-j"], timeout=timeout)
        defs = idx.parsed if isinstance(idx.parsed, list) else []
        entries.append({
            "symbol": sym,
            "found": loc is not None,
            "file": (loc.get("file_path") or loc.get("file")) if loc else None,
            "start_line": loc.get("start_line") if loc else None,
            "end_line": loc.get("end_line") if loc else None,
            "index_defs": len(defs),
            "body_loc": len(body),
            "calls": sorted(calls),
            "body": body if with_bodies else None,
        })

    union: set[str] = set()
    for e in entries:
        union.update(e["calls"])

    matrix: list[dict[str, Any]] = []
    for call in sorted(union):
        row: dict[str, Any] = {"call": call}
        for e in entries:
            row[e["symbol"]] = call in e["calls"]
        matrix.append(row)

    asymmetric = [row for row in matrix
                  if not all(row[e["symbol"]] for e in entries)
                  and any(row[e["symbol"]] for e in entries)]

    return {
        "entries": entries,
        "call_union_size": len(union),
        "asymmetric_count": len(asymmetric),
        "asymmetric_calls": asymmetric,
        "matrix": matrix,
    }


def emit_human(report: dict[str, Any]) -> None:
    emit_section("CROSS-CALLER COMPARE  (Cadeia C08)")
    emit_kv("call union size", report["call_union_size"])
    emit_kv("asymmetric calls", report["asymmetric_count"])

    emit_section("functions located", char="-")
    rows = [[e["symbol"], "Y" if e["found"] else "N",
             (e["file"] or "?")[-50:],
             f"{e.get('start_line') or '?'}-{e.get('end_line') or '?'}",
             e["body_loc"]]
            for e in report["entries"]]
    emit_table(rows, headers=["symbol", "found", "file", "lines", "body_loc"])

    if report["asymmetric_calls"]:
        emit_section("ASYMMETRY MATRIX (potential bug sites)", char="*")
        headers = ["call"] + [e["symbol"] for e in report["entries"]]
        rows = [[row["call"]] + ["Y" if row[e["symbol"]] else "·" for e in report["entries"]]
                for row in report["asymmetric_calls"]]
        emit_table(rows, headers=headers)
        print("\n  Y = called   · = NOT called   ←  rows with mixed Y/· are the bug candidates")
    else:
        emit_section("SYMMETRY", char="-")
        print("  All functions call the same set — no asymmetry detected.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                      formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("symbols", nargs="+", help="2 or more function names")
    parser.add_argument("--bodies", action="store_true",
                        help="Include full function bodies in output")
    add_common_args(parser)
    args = parser.parse_args()

    if len(args.symbols) < 2:
        emit_error("at least 2 symbols required for cross-caller compare")
        return 2

    require_touring()
    report = gather(args.symbols, with_bodies=args.bodies, timeout=args.timeout)
    if not emit_result(report, args):
        emit_human(report)
    return 0 if report["asymmetric_count"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
