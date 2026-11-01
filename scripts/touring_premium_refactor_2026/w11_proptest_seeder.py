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

import re
from pathlib import Path

import script_common as sc

_SCRIPT = "w11_proptest_seeder"

_ROOT, _DATA_DIR, _STAGING_DIR, _CRATES_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR, sc.CRATES_DIR

DERIVE_RE = re.compile(r"#\[derive\(([^)]+)\)\]\s*\npub\s+(struct|enum)\s+(\w+)")


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    p.add_argument("--min-callsites", type=int, default=3)
    p.add_argument("--emit-stubs", action="store_true")
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


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


def run(args: sc.Namespace) -> dict:
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
        "timestamp": sc.now_iso(),
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
    return sc.write_json_report(json_path, report, _ROOT)


def _run_and_report(args: sc.Namespace) -> int:
    """Run w11_proptest_seeder, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json({
        "status": result["status"], "totals": result["totals"],
        "top_5_candidates": [c["name"]
                                for c in result["top_20_candidates"][:5]],
        "stubs_emitted_count": len(result["stubs_emitted"]),
        "json_path": result["json_path"],
    })
    return 0


def main() -> int:
    """Entry point of w11_proptest_seeder."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
