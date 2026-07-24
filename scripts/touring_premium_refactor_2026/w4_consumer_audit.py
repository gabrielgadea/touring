#!/usr/bin/env python3
"""W4.7-W4.10 — Map consumers of touring-ast / touring-ast-polyglot / touring-language / touring-semantics.

Scans every crates/touring-*/src/ and reports which files reference these
4 source crates (the ones being merged into touring-code in W4). Output
is a JSON map: {source_crate: {consumer_crate: [files_with_line_count]}}.

This is the CRITICAL pre-merge audit — every found consumer must be
migrated to use `touring_code::<old_module>::...` after the W4 fusion.

Outputs:
  - scripts/touring_premium_refactor_2026/data/w4-consumer-map.json
  - scripts/touring_premium_refactor_2026/data/w4-migration-checklist.md

Usage
-----
    python3 w4_consumer_audit.py                       # full scan, JSON report
    python3 w4_consumer_audit.py --crate touring-ast   # narrow to one source
    python3 w4_consumer_audit.py --markdown            # also emit checklist.md
    python3 w4_consumer_audit.py --strict              # fail if >50 consumers (sanity)
"""
from __future__ import annotations

import argparse
import json
import logging
import re
from collections import defaultdict
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_CRATES_DIR = _ROOT / "crates"
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"

# 4 source crates that fuse into touring-code in W4
SOURCE_CRATES: list[str] = [
    "touring-ast",
    "touring-ast-polyglot",
    "touring-language",
    "touring-semantics",
]


def crate_to_module(crate_name: str) -> str:
    """Convert kebab-case crate name to snake_case Rust module name."""
    return crate_name.replace("-", "_")


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w4_consumer_audit", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--crate", type=str, default="",
                   help="Limit scan to one source crate (default: all 4).")
    p.add_argument("--markdown", action="store_true",
                   help="Also emit human-readable migration-checklist.md.")
    p.add_argument("--strict", action="store_true",
                   help="Fail if >50 distinct consumer files (anomaly check).")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def scan_consumers(source_crate: str) -> dict[str, list[dict]]:
    """For one source crate, find every file in workspace that references it.

    Returns: {consumer_crate_name: [{file, references, lines}]}
    """
    module = crate_to_module(source_crate)
    # Match `use touring_ast::`, `touring_ast::`, `extern crate touring_ast`
    use_re = re.compile(rf"\buse\s+{module}::|\b{module}::|\bextern\s+crate\s+{module}\b")
    cargo_re = re.compile(rf'^\s*{re.escape(source_crate)}\s*=', re.MULTILINE)

    result: dict[str, list[dict]] = defaultdict(list)
    for crate_dir in sorted(_CRATES_DIR.glob("touring-*")):
        if crate_dir.name == source_crate:
            continue  # skip self
        # Check Cargo.toml for direct dep declaration
        cargo = crate_dir / "Cargo.toml"
        if cargo.exists():
            try:
                cargo_text = cargo.read_text(encoding="utf-8", errors="replace")
                if cargo_re.search(cargo_text):
                    LOGGER.debug("%s declares %s as dep", crate_dir.name, source_crate)
            except OSError:
                pass
        # Scan src/ for use statements
        src = crate_dir / "src"
        if not src.exists():
            continue
        for rs_file in src.rglob("*.rs"):
            try:
                content = rs_file.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            matches = use_re.findall(content)
            if not matches:
                continue
            # Find specific lines for evidence
            ref_lines = []
            for n, line in enumerate(content.splitlines(), start=1):
                if use_re.search(line):
                    ref_lines.append({"line": n, "text": line.strip()[:120]})
                if len(ref_lines) >= 3:
                    break
            result[crate_dir.name].append({
                "file": str(rs_file.relative_to(_ROOT)),
                "references": len(matches),
                "loc": sum(1 for _ in content.splitlines()),
                "sample_lines": ref_lines,
            })
    return dict(result)


def build_report(audits: dict[str, dict[str, list[dict]]]) -> dict:
    """Compute summary statistics."""
    total_files = sum(
        len(files)
        for src in audits.values()
        for files in src.values()
    )
    consumers_per_src = {
        src: {"consumer_crates": len(crates), "consumer_files": sum(len(f) for f in crates.values())}
        for src, crates in audits.items()
    }
    all_consumer_crates = {
        crate for src in audits.values() for crate in src
    }
    return {
        "script": "w4_consumer_audit",
        "wave": "W4",
        "subtask_refs": ["W4.7", "W4.8", "W4.9", "W4.10"],
        "timestamp": datetime.now(UTC).isoformat(),
        "total_consumer_files": total_files,
        "distinct_consumer_crates": sorted(all_consumer_crates),
        "consumers_per_source": consumers_per_src,
        "details": audits,
    }


def render_markdown_checklist(report: dict) -> str:
    """Human-readable migration checklist."""
    lines = ["# W4 — Consumer Migration Checklist",
             "",
             f"Generated: {report['timestamp']}",
             f"Total consumer files: {report['total_consumer_files']}",
             f"Distinct consumer crates: {len(report['distinct_consumer_crates'])}",
             "",
             "## Migration plan: rename `<source>::<x>` → `touring_code::<source>::<x>`",
             ""]
    for src, crates in report["details"].items():
        lines.append(f"### Source: `{src}` (module: `{crate_to_module(src)}`)")
        lines.append("")
        for crate_name, files in sorted(crates.items()):
            lines.append(f"- [ ] **`{crate_name}`** ({len(files)} files)")
            for f in files:
                lines.append(f"  - [ ] `{f['file']}` ({f['references']} refs, {f['loc']} LOC)")
        lines.append("")
    return "\n".join(lines)


def run(args: argparse.Namespace) -> dict:
    """Full scan + emit JSON (+ optionally markdown)."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    sources = [args.crate] if args.crate else SOURCE_CRATES
    audits = {src: scan_consumers(src) for src in sources}
    report = build_report(audits)
    json_path = args.output_dir / "w4-consumer-map.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                         encoding="utf-8")
    LOGGER.info("wrote: %s", json_path.relative_to(_ROOT))
    if args.markdown:
        md_path = args.output_dir / "w4-migration-checklist.md"
        md_path.write_text(render_markdown_checklist(report), encoding="utf-8")
        LOGGER.info("wrote: %s", md_path.relative_to(_ROOT))
        report["markdown_path"] = str(md_path.relative_to(_ROOT))
    report["json_path"] = str(json_path.relative_to(_ROOT))
    if args.strict and report["total_consumer_files"] > 50:
        report["status"] = "STRICT_ANOMALY"
        LOGGER.warning("strict mode: %d files exceeds expected ~38",
                       report["total_consumer_files"])
    else:
        report["status"] = "OK"
    return report


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        # Print compact summary to stdout (full JSON in file)
        print(json.dumps({
            "status": result["status"],
            "total_consumer_files": result["total_consumer_files"],
            "distinct_consumer_crates": len(result["distinct_consumer_crates"]),
            "consumers_per_source": result["consumers_per_source"],
            "json_path": result["json_path"],
            "markdown_path": result.get("markdown_path"),
        }, indent=2, ensure_ascii=False))
        return 0 if result["status"] == "OK" else 1
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
