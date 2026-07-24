#!/usr/bin/env python3
"""W9.1-W9.7 — Plan touring-server split into 6 sub-crates (mirror of W8).

touring-server (~161 files) parallels touring-hooks structurally — same split
playbook applies. v1 implementation reuses the W8 algorithm with server-
specific bucket rules:

  - touring-server-core         (rpc dispatch, RPC types, daemon socket)
  - touring-server-tools        (mcp tool handlers)
  - touring-server-reasoning    (reasoning loop, persistence)
  - touring-server-protocol     (capnp/rkyv wire types)
  - touring-server-bench        (benchmarks, telemetry)
  - touring-server-facade       (lib.rs re-exports)

Outputs:
  - data/w9-server-split-plan.json
  - staging/w9-server-bucket-map.md
  - staging/w9-classify-evidence.json

Usage
-----
    python3 w9_server_split_planner.py --emit-cargo --emit-evidence
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
_SERVER_SRC = _ROOT / "crates" / "touring-server" / "src"
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

RULES: list[tuple[str, re.Pattern | None, re.Pattern]] = [
    ("touring-server-core",
     re.compile(r"^(server|rpc|runtime|shared)$"),
     re.compile(r"\b(lib\.rs|main\.rs|server|rpc|dispatch|actor|error|"
                r"daemon_main|tools_infra)", re.I)),
    ("touring-server-tools",
     re.compile(r"^(tools|mcp)$"),
     re.compile(r"\b(tools_|mcp_|tool_handlers|file_tools|project_tools)",
                re.I)),
    ("touring-server-reasoning",
     re.compile(r"^(reasoning|cognitive|cortex_bridge)$"),
     re.compile(r"\b(reasoning|persistence|cognitive|cortex|"
                r"plan_speculate|shadow_validate)", re.I)),
    ("touring-server-protocol",
     re.compile(r"^(capnp|rkyv|protocol|schemas)$"),
     re.compile(r"\b(capnp|rkyv|protocol|schema|wire_format)", re.I)),
    ("touring-server-bench",
     re.compile(r"^(bench|metrics|telemetry)$"),
     re.compile(r"\b(bench|metric|telemetry|profile|pprof)", re.I)),
    ("touring-server-cli",
     None,
     re.compile(r"\bcli_", re.I)),
]

FACADE_BUCKET = "touring-server-facade"
USE_CRATE_RE = re.compile(r"^\s*use\s+crate::(\w+)", re.MULTILINE)


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w9_server_split_planner", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--emit-cargo", action="store_true")
    p.add_argument("--emit-evidence", action="store_true")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def classify_file(rs_file: Path, src_root: Path) -> tuple[str, str]:
    """Classify a single .rs file."""
    rel = rs_file.relative_to(src_root)
    name = rs_file.name
    parts = rel.parts
    if len(parts) == 1 and name == "lib.rs":
        return FACADE_BUCKET, "façade-exempt"
    if len(parts) > 1:
        parent = parts[0]
        for bucket, parent_re, _ in RULES:
            if parent_re and parent_re.match(parent):
                return bucket, f"parent-dir:{parent}"
    for bucket, _, basename_re in RULES:
        if basename_re.search(name):
            return bucket, f"basename:{basename_re.pattern[:30]}"
    return "touring-server-misc", "fallback"


def run(args: argparse.Namespace) -> dict:
    """Walk + classify + report."""
    if not _SERVER_SRC.exists():
        return {"status": "MISSING_CRATE", "path": str(_SERVER_SRC)}
    rs_files = sorted(f for f in _SERVER_SRC.rglob("*.rs") if f.is_file())
    classifications: list[dict] = []
    file_to_bucket: dict[Path, str] = {}
    for f in rs_files:
        bucket, ev = classify_file(f, _SERVER_SRC)
        try:
            loc = sum(1 for _ in f.read_text(encoding="utf-8",
                                              errors="replace").splitlines())
        except OSError:
            loc = 0
        classifications.append({
            "file": str(f.relative_to(_ROOT)),
            "bucket": bucket, "evidence": ev, "loc": loc,
        })
        file_to_bucket[f] = bucket
    buckets: dict[str, list[dict]] = defaultdict(list)
    for c in classifications:
        buckets[c["bucket"]].append(c)
    bucket_summary = {
        b: {"file_count": len(items),
            "total_loc": sum(i["loc"] for i in items),
            "sample_files": [i["file"] for i in items[:3]]}
        for b, items in buckets.items()
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    plan = {
        "script": "w9_server_split_planner",
        "wave": "W9", "subtask_refs": ["W9.1", "W9.2", "W9.3", "W9.4",
                                          "W9.5", "W9.6", "W9.7"],
        "timestamp": datetime.now(UTC).isoformat(),
        "totals": {"rs_files": len(rs_files),
                    "total_loc": sum(c["loc"] for c in classifications),
                    "bucket_count": len(bucket_summary)},
        "buckets": bucket_summary,
    }
    plan_path = args.output_dir / "w9-server-split-plan.json"
    plan_path.write_text(json.dumps(plan, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    md_lines = ["# W9 — touring-server Split Plan", "",
                f"Files: {plan['totals']['rs_files']} | "
                f"LOC: {plan['totals']['total_loc']:,}",
                "", "| Bucket | Files | LOC |", "|--------|-------|-----|"]
    for b, info in sorted(bucket_summary.items(),
                           key=lambda x: -x[1]["total_loc"]):
        md_lines.append(f"| `{b}` | {info['file_count']} | "
                        f"{info['total_loc']:,} |")
    md_path = _STAGING_DIR / "w9-server-bucket-map.md"
    md_path.write_text("\n".join(md_lines) + "\n", encoding="utf-8")
    evidence_path: str | None = None
    if args.emit_evidence:
        ev_path = _STAGING_DIR / "w9-classify-evidence.json"
        ev_path.write_text(
            json.dumps({"classifications": classifications}, indent=2),
            encoding="utf-8")
        evidence_path = str(ev_path.relative_to(_ROOT))
    plan["status"] = "OK"
    plan["plan_path"] = str(plan_path.relative_to(_ROOT))
    plan["md_path"] = str(md_path.relative_to(_ROOT))
    plan["evidence_path"] = evidence_path
    return plan


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        if result.get("status") == "MISSING_CRATE":
            sys.stdout.write(json.dumps(result, indent=2) + "\n")
            return 1
        sys.stdout.write(json.dumps({
            "status": result["status"], "totals": result["totals"],
            "buckets": {b: info["total_loc"]
                         for b, info in result["buckets"].items()},
            "plan_path": result["plan_path"], "md_path": result["md_path"],
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
