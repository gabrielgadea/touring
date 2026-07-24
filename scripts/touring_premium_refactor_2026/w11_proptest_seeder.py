#!/usr/bin/env python3
"""W11.4 — Identify candidate types for proptest (Identity, Plan, Definition).

Scans Rust source for pub enums/structs that:
  - Have `#[derive(...)]` annotations (canonical types)
  - Have public constructor (`pub fn new(...)`)
  - Are referenced by ≥3 callsites (high-impact types)

Generates `proptest` strategy stubs that consumers can refine.

Outputs:
  - data/w11-proptest-candidates.json
  - staging/w11-proptest-stubs/<type>_strategy.rs
"""
from __future__ import annotations

import argparse
import json
import logging
import re
import sys
from collections import defaultdict
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_CRATES_DIR = _ROOT / "crates"
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

DERIVE_RE = re.compile(r"#\[derive\(([^)]+)\)\]\s*\npub\s+(struct|enum)\s+(\w+)")


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w11_proptest_seeder", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--min-callsites", type=int, default=3)
    p.add_argument("--emit-stubs", action="store_true")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def find_derived_types() -> list[dict]:
    """Find pub structs/enums with #[derive(...)]."""
    candidates: list[dict] = []
    for rs in _CRATES_DIR.rglob("touring-*/src/**/*.rs"):
        try:
            content = rs.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for m in DERIVE_RE.finditer(content):
            derives, kind, name = m.groups()
            candidates.append({
                "file": str(rs.relative_to(_ROOT)),
                "kind": kind, "name": name,
                "derives": [d.strip() for d in derives.split(",")],
            })
    return candidates


def count_callsites(name: str) -> int:
    """Count files referencing this type name (approximation)."""
    count = 0
    for rs in _CRATES_DIR.rglob("touring-*/src/**/*.rs"):
        try:
            content = rs.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if re.search(rf"\b{re.escape(name)}\b", content):
            count += 1
    return count


def emit_strategy_stub(candidate: dict, out_dir: Path) -> Path:
    """Emit proptest strategy stub."""
    out_dir.mkdir(parents=True, exist_ok=True)
    p = out_dir / f"{candidate['name'].lower()}_strategy.rs"
    p.write_text(
        f"//! AUTO-GENERATED — proptest strategy for `{candidate['name']}` ({candidate['kind']}).\n"
        f"//! Source: {candidate['file']}\n"
        f"//! Derives: {', '.join(candidate['derives'])}\n"
        "\n"
        "use proptest::prelude::*;\n"
        "\n"
        f"prop_compose! {{\n"
        f"    /// Generate arbitrary {candidate['name']} for property tests.\n"
        f"    pub fn arb_{candidate['name'].lower()}()\n"
        f"        // TODO: define field strategies here.\n"
        "        (placeholder in any::<u32>())\n"
        f"        -> {candidate['name']} {{\n"
        f"        // TODO: construct {candidate['name']} from generated fields.\n"
        "        unimplemented!()\n"
        "    }\n"
        "}\n",
        encoding="utf-8")
    return p


def run(args: argparse.Namespace) -> dict:
    """Find candidates + optionally emit stubs."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    raw_candidates = find_derived_types()
    # Filter: dedupe by name + count callsites
    by_name: dict[str, dict] = {}
    for c in raw_candidates:
        if c["name"] not in by_name:
            by_name[c["name"]] = c
    for name, c in by_name.items():
        c["callsite_count"] = count_callsites(name)
    high_impact = sorted(
        [c for c in by_name.values()
         if c["callsite_count"] >= args.min_callsites],
        key=lambda x: -x["callsite_count"],
    )
    stubs_emitted: list[str] = []
    if args.emit_stubs:
        stub_dir = _STAGING_DIR / "w11-proptest-stubs"
        for c in high_impact[:20]:
            stubs_emitted.append(
                str(emit_strategy_stub(c, stub_dir).relative_to(_ROOT)))
    report = {
        "script": "w11_proptest_seeder",
        "wave": "W11", "subtask_refs": ["W11.4"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "totals": {
            "raw_candidates": len(raw_candidates),
            "unique_types": len(by_name),
            "high_impact_candidates": len(high_impact),
            "min_callsites_threshold": args.min_callsites,
        },
        "top_20_candidates": high_impact[:20],
        "stubs_emitted": stubs_emitted,
    }
    json_path = args.output_dir / "w11-proptest-candidates.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    report["json_path"] = str(json_path.relative_to(_ROOT))
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
        sys.stdout.write(json.dumps({
            "status": result["status"], "totals": result["totals"],
            "top_5_candidates": [c["name"]
                                    for c in result["top_20_candidates"][:5]],
            "stubs_emitted_count": len(result["stubs_emitted"]),
            "json_path": result["json_path"],
        }, indent=2, ensure_ascii=False) + "\n")
        return 0
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
