#!/usr/bin/env python3
"""Deep symbol forensics for the /Touring skill (C03 + Cadeia 4 + Cadeia 4b).

Resolves a symbol name across the workspace by composing ``touring index find``
(canonical lookup), ``touring ast find`` (signature + module path), ``touring
wiring impact`` (transitive consumers BFS), and a polyglot homonimia check via
grep across .rs/.py/.ts/.tsx/.go for cross-language homonyms (VP-Scout Cadeia
4b — mandatory in polyglot workspaces).

Verdict:
  UNIQUE          — single canonical definition; safe to act on
  HOMONIMIA_INTRA — multiple defs in same language (Cadeia 4)
  HOMONIMIA_CROSS — defs in multiple languages (Cadeia 4b)
  NOT_FOUND       — zero definitions anywhere

Usage:
  discover_symbol.py CognitiveMCTS
  discover_symbol.py CognitiveMCTS --depth 3        # wiring impact depth
  discover_symbol.py UrnLex --json                  # machine JSON
  discover_symbol.py CognitiveMCTS --workspace ~/projects/touring
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
    find_workspace_root, mark_degraded, parse_definitions, require_touring, touring_run,
)

POLYGLOT_EXTS = ("*.rs", "*.py", "*.ts", "*.tsx", "*.js", "*.jsx", "*.go", "*.java",
                  "*.cpp", "*.c", "*.h", "*.swift", "*.kt")


def gather(symbol: str, workspace: Path | None, depth: int, timeout: float) -> dict[str, Any]:
    report: dict[str, Any] = {"symbol": symbol, "workspace": str(workspace) if workspace else None}

    idx = touring_run(["index", "find", symbol, "-j"], timeout=timeout)
    report["daemon_degraded"] = idx.daemon_degraded
    defs = parse_definitions(idx.parsed)
    report["index_definitions"] = defs
    report["index_count"] = len(defs)

    ast = touring_run(["ast", "find", symbol, "-j"], timeout=timeout)
    ast_hits = parse_definitions(ast.parsed)
    report["ast_hits"] = ast_hits
    report["ast_count"] = len(ast_hits)

    impact = touring_run(["wiring", "impact", symbol, "--depth", str(depth)], timeout=timeout)
    report["wiring_impact"] = impact.parsed if impact.parsed else impact.stdout

    report["polyglot_grep"] = polyglot_grep(symbol, workspace)
    report["verdict"] = classify(report)
    return report


def polyglot_grep(symbol: str, workspace: Path | None) -> dict[str, list[str]]:
    """Cadeia 4b: grep --include for every language extension simultaneously."""
    if not workspace or not workspace.is_dir():
        return {}
    by_ext: dict[str, list[str]] = {}
    for pattern in POLYGLOT_EXTS:
        try:
            proc = subprocess.run(
                ["grep", "-rln", "--include", pattern, symbol, str(workspace)],
                capture_output=True, text=True, timeout=20.0,
            )
        except (FileNotFoundError, subprocess.TimeoutExpired, OSError):
            continue
        if proc.returncode == 0:
            ext = pattern.lstrip("*")
            files = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
            if files:
                by_ext[ext] = files[:20]
    return by_ext


def classify(report: dict[str, Any]) -> dict[str, Any]:
    """Apply Cadeia 4 + Cadeia 4b homonimia rules."""
    defs = report["index_definitions"]
    polyglot = report["polyglot_grep"]
    languages_with_defs = set(polyglot.keys())

    if not defs and not polyglot:
        return {"label": "NOT_FOUND", "reasons": ["index empty + grep empty across all languages"]}

    reasons: list[str] = []
    label = "UNIQUE"

    if len(languages_with_defs) > 1:
        label = "HOMONIMIA_CROSS"
        reasons.append(f"symbol appears in {len(languages_with_defs)} languages: "
                       f"{', '.join(sorted(languages_with_defs))}")

    if len(defs) > 1:
        module_paths = {d.get("module_path") for d in defs
                        if isinstance(d, dict) and d.get("module_path")}
        if len(module_paths) > 1:
            if label == "UNIQUE":
                label = "HOMONIMIA_INTRA"
            reasons.append(f"{len(defs)} defs across {len(module_paths)} module paths")

    return {"label": label, "reasons": reasons}


def emit_human(report: dict[str, Any]) -> None:
    v = report["verdict"]
    emit_section(f"SYMBOL  {report['symbol']}  →  {v['label']}")
    emit_kv("index definitions", report["index_count"])
    emit_kv("ast hits", report["ast_count"])
    emit_kv("verdict", v["label"])

    if report["index_definitions"]:
        emit_section("index definitions", char="-")
        rows = [[(d.get("file_path") or d.get("file") or "?")[:60],
                 str(d.get("line") or d.get("start_line") or ""),
                 (d.get("module_path") or "")[:40],
                 (d.get("kind") or "")[:12]]
                for d in report["index_definitions"][:12] if isinstance(d, dict)]
        emit_table(rows, headers=["file", "line", "module", "kind"])

    if report["polyglot_grep"]:
        emit_section("polyglot grep (Cadeia 4b)", char="-")
        for ext, files in sorted(report["polyglot_grep"].items()):
            print(f"  {ext:<8} {len(files)} file(s)")
            for f in files[:3]:
                print(f"            {f}")

    if v["reasons"]:
        emit_section("homonimia signals", char="-")
        for r in v["reasons"]:
            print(f"  - {r}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                      formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("symbol", help="Symbol name to investigate")
    parser.add_argument("--depth", type=int, default=2,
                        help="wiring impact BFS depth (default: 2)")
    parser.add_argument("--workspace", default=None,
                        help="Workspace root for Cadeia 4b grep (default: auto)")
    add_common_args(parser)
    args = parser.parse_args()

    require_touring()
    ws = Path(args.workspace).expanduser().resolve() if args.workspace else find_workspace_root()
    if not ws:
        emit_error("no Cargo workspace detected — polyglot grep (Cadeia 4b) will be empty")
    report = gather(args.symbol, ws, args.depth, args.timeout)
    label = report["verdict"]["label"]
    if label == "NOT_FOUND":
        emit_error(f"symbol '{args.symbol}' NOT_FOUND across index, ast, and polyglot grep")
    elif label.startswith("HOMONIMIA"):
        emit_error(f"{label}: '{args.symbol}' resolves ambiguously — see verdict.reasons")
    # I-fail-soft: annotate so a consumer never reads a degraded-daemon result as authoritative.
    mark_degraded(report, report.get("daemon_degraded", False))
    # I-density: --brief (compact digest) > --json (full); else human fallback.
    if not emit_result(report, args):
        emit_human(report)
    return 0 if label == "UNIQUE" else (2 if label == "NOT_FOUND" else 1)


if __name__ == "__main__":
    sys.exit(main())
