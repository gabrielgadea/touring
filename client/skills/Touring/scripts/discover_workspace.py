#!/usr/bin/env python3
"""Workspace structure discovery for the /Touring skill (Reflex #1 + C10).

Builds a complete map of a Rust workspace by composing ``touring ast
workspace-info`` (cargo_metadata) with per-crate LOC + symbol counts +
feature matrix + dependency edges. Replaces ``cargo metadata | jq`` ad-hoc
spelunking with a single structured report.

Sections of the report:
  packages      — every workspace member with name, version, path
  features      — every feature flag declared, with which crates declare it
  dependents    — reverse-dep map (cross-crate consumers)
  per_crate     — LOC, file count, pub-symbol estimate, top deps

Usage:
  discover_workspace.py                # cwd
  discover_workspace.py /path/to/ws    # explicit workspace
  discover_workspace.py --json         # machine JSON
  discover_workspace.py --top 10       # limit per_crate output to top 10
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    add_common_args, emit_error, emit_json, emit_kv, emit_result, emit_section, emit_table,
    find_workspace_root, iter_code_files, mark_degraded, require_touring, touring_run,
)


def gather(root: Path, *, timeout: float) -> dict[str, Any]:
    report: dict[str, Any] = {"workspace_root": str(root)}

    wi = touring_run(["ast", "workspace-info", str(root)], timeout=timeout)
    report["daemon_degraded"] = wi.daemon_degraded
    workspace_data = wi.parsed if isinstance(wi.parsed, dict) else {}
    report["workspace_info"] = workspace_data
    packages = workspace_data.get("packages") or []
    if not isinstance(packages, list):
        packages = []
    report["packages"] = packages
    report["package_count"] = len(packages)

    all_features = workspace_data.get("all_feature_names") or []
    report["features"] = sorted(all_features) if isinstance(all_features, list) else []
    report["feature_count"] = len(report["features"])

    per_crate: list[dict[str, Any]] = []
    crates_dir = root / "crates"
    crate_paths = [Path(p["manifest_path"]).parent for p in packages
                   if isinstance(p, dict) and p.get("manifest_path")]
    if not crate_paths and crates_dir.is_dir():
        crate_paths = sorted(d for d in crates_dir.iterdir() if d.is_dir())

    for crate_path in crate_paths:
        files = iter_code_files(crate_path, exts={".rs"})
        loc = 0
        pub_count = 0
        for f in files:
            try:
                text = f.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            loc += sum(1 for _ in text.splitlines())
            pub_count += sum(1 for line in text.splitlines()
                             if line.lstrip().startswith(("pub fn ", "pub struct ",
                                                          "pub trait ", "pub enum ",
                                                          "pub mod ", "pub type ",
                                                          "pub const ", "pub static ")))
        per_crate.append({
            "name": crate_path.name,
            "path": str(crate_path.relative_to(root) if crate_path.is_relative_to(root) else crate_path),
            "file_count": len(files),
            "loc": loc,
            "pub_symbols_estimate": pub_count,
        })
    report["per_crate"] = sorted(per_crate, key=lambda c: c["loc"], reverse=True)
    report["totals"] = {
        "crates": len(per_crate),
        "files": sum(c["file_count"] for c in per_crate),
        "loc": sum(c["loc"] for c in per_crate),
        "pub_symbols_estimate": sum(c["pub_symbols_estimate"] for c in per_crate),
    }
    return report


def emit_human(report: dict[str, Any], top: int) -> None:
    emit_section(f"WORKSPACE  {report['workspace_root']}")
    t = report["totals"]
    emit_kv("crates", t["crates"])
    emit_kv("files", t["files"])
    emit_kv("LOC", t["loc"])
    emit_kv("pub_symbols (est.)", t["pub_symbols_estimate"])
    emit_kv("features declared", report["feature_count"])

    emit_section(f"per-crate (top {top} by LOC)", char="-")
    rows = [[c["name"], c["loc"], c["file_count"], c["pub_symbols_estimate"]]
            for c in report["per_crate"][:top]]
    emit_table(rows, headers=["crate", "loc", "files", "pub_est"])

    if report["features"]:
        emit_section(f"features ({len(report['features'])})", char="-")
        chunk = report["features"][:40]
        for i in range(0, len(chunk), 4):
            print("  " + "  ".join(f"{f:<24}" for f in chunk[i:i+4]))
        if len(report["features"]) > 40:
            print(f"  ... and {len(report['features']) - 40} more")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                      formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("root", nargs="?", default=".",
                        help="Workspace root path (default: cwd; auto-walks up)")
    parser.add_argument("--top", type=int, default=20,
                        help="Limit per-crate table to top N by LOC (default: 20)")
    add_common_args(parser)
    args = parser.parse_args()

    require_touring()
    start = Path(args.root).expanduser().resolve()
    root = start if (start / "Cargo.toml").exists() else find_workspace_root(start)
    if not root:
        emit_error(f"no Cargo workspace root found from {start}")
        return 2

    report = gather(root, timeout=args.timeout)
    # I-fail-soft: annotate so a consumer never reads a degraded-daemon result as authoritative.
    mark_degraded(report, report.get("daemon_degraded", False))
    # I-density: --brief (compact digest) > --json (full); else human fallback.
    if not emit_result(report, args):
        emit_human(report, top=args.top)
    return 0


if __name__ == "__main__":
    sys.exit(main())
