#!/usr/bin/env python3
"""Batch VGP verification for the /Touring skill (Symbol Verification Table).

Takes N symbol names (CLI args, file, or stdin) and runs the Verified
Generation Protocol pipeline on each: ``touring index find`` (existence),
``touring ast find`` (signature + module path), ``touring generate verify``
(generator gate). Outputs a per-symbol verdict so callers (architect,
engineer, scriber) can populate the constitutional Symbol Verification Table
without hallucination (Wave TRM 2026-05-02).

Verdicts per symbol:
  VERIFIED            — index find returned definitions; safe to cite
  AMBIGUOUS           — multiple definitions (likely homonimia, see analyze_callers.py)
  NOT_FOUND           — zero defs; classify as ``to_be_created`` OR fix the citation
  STALE_INDEX         — ast find returns hit but index find is empty (index lag)

Usage:
  vgp_batch.py sym1 sym2 sym3                # CLI list
  vgp_batch.py --file symbols.txt            # one per line
  echo "fn foo" | vgp_batch.py --stdin       # stdin
  vgp_batch.py --json sym1                   # machine JSON
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    add_common_args, emit_error, emit_json, emit_result, emit_section,
    emit_table, parse_definitions, require_touring, touring_run,
)


def verify_one(symbol: str, *, with_generate: bool, timeout: float) -> dict[str, Any]:
    """Run the full VGP triad on one symbol."""
    entry: dict[str, Any] = {"symbol": symbol}

    idx = touring_run(["index", "find", symbol, "-j"], timeout=timeout)
    defs = parse_definitions(idx.parsed)
    entry["index_defs"] = len(defs)
    entry["index_locations"] = [
        {"file": d.get("file_path") or d.get("file"),
         "line": d.get("line") or d.get("start_line"),
         "module": d.get("module_path")}
        for d in defs if isinstance(d, dict)
    ][:8]

    ast = touring_run(["ast", "find", symbol, "-j"], timeout=timeout)
    ast_hits = parse_definitions(ast.parsed)
    entry["ast_hits"] = len(ast_hits)
    entry["ast_signatures"] = [
        {"signature": h.get("signature"), "module": h.get("module_path")}
        for h in ast_hits if isinstance(h, dict)
    ][:8]

    if with_generate:
        gen = touring_run(["generate", "verify", "--symbol", symbol], timeout=timeout,
                          parse_json=False)
        entry["generate_verify_ok"] = gen.ok
        entry["generate_verify_output"] = gen.stdout.strip()[:200]

    entry["verdict"] = _classify(entry)
    return entry


def _classify(entry: dict[str, Any]) -> str:
    """Map index/ast results to a VERIFIED / AMBIGUOUS / NOT_FOUND / STALE_INDEX label."""
    idx_n = entry["index_defs"]
    ast_n = entry["ast_hits"]
    if idx_n == 0 and ast_n == 0:
        return "NOT_FOUND"
    if idx_n == 0 and ast_n > 0:
        return "STALE_INDEX"
    if idx_n > 1:
        return "AMBIGUOUS"
    return "VERIFIED"


def collect_symbols(args: argparse.Namespace) -> list[str]:
    """Merge symbols from --file, --stdin, positional, dedup preserving order."""
    out: list[str] = []
    seen: set[str] = set()

    def add(sym: str) -> None:
        sym = sym.strip()
        if sym and sym not in seen:
            seen.add(sym)
            out.append(sym)

    for sym in args.symbols:
        add(sym)
    if args.file:
        try:
            for line in Path(args.file).read_text(encoding="utf-8", errors="replace").splitlines():
                add(line.strip())
        except OSError as exc:
            emit_error(f"--file read failed: {exc}")
    if args.stdin:
        for line in sys.stdin.read().splitlines():
            add(line.strip())
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                      formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("symbols", nargs="*",
                        help="Symbol names to verify (positional)")
    parser.add_argument("--file", help="Read symbols from a file (one per line)")
    parser.add_argument("--stdin", action="store_true",
                        help="Read symbols from stdin (one per line)")
    parser.add_argument("--no-generate", action="store_true",
                        help="Skip ``generate verify`` (faster)")
    add_common_args(parser)
    args = parser.parse_args()

    require_touring()
    symbols = collect_symbols(args)
    if not symbols:
        emit_error("no symbols supplied (use positional, --file, or --stdin)")
        return 2

    results = [verify_one(s, with_generate=not args.no_generate, timeout=args.timeout)
               for s in symbols]
    summary = {
        "total": len(results),
        "verified": sum(1 for r in results if r["verdict"] == "VERIFIED"),
        "ambiguous": sum(1 for r in results if r["verdict"] == "AMBIGUOUS"),
        "not_found": sum(1 for r in results if r["verdict"] == "NOT_FOUND"),
        "stale_index": sum(1 for r in results if r["verdict"] == "STALE_INDEX"),
    }
    report = {"summary": summary, "results": results}

    if not emit_result(report, args):
        emit_section(f"VGP BATCH  ({summary['total']} symbols)")
        rows = [[r["symbol"], r["verdict"], r["index_defs"], r["ast_hits"]]
                for r in results]
        emit_table(rows, headers=["symbol", "verdict", "idx", "ast"])
        emit_section("summary", char="-")
        for k, v in summary.items():
            print(f"  {k:<14} {v}")

    return 0 if summary["not_found"] == 0 and summary["stale_index"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
