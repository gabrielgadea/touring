#!/usr/bin/env python3
"""W11.5 — Identify candidate fuzz targets (parsers, serializers).

Scans for `pub fn parse`, `from_bytes`, `serialize` — high-value fuzz targets.
"""
from __future__ import annotations

import argparse
import json
import logging
import re
import sys
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_CRATES_DIR = _ROOT / "crates"
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

FUZZ_TARGET_PATTERNS = [
    re.compile(r"pub\s+fn\s+(parse\w*)\s*\("),
    re.compile(r"pub\s+fn\s+(from_bytes\w*)\s*\("),
    re.compile(r"pub\s+fn\s+(deserialize\w*)\s*\("),
    re.compile(r"pub\s+fn\s+(decode\w*)\s*\("),
]


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w11_fuzz_target_seeder", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--emit-stubs", action="store_true")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def scan_fuzz_candidates() -> list[dict]:
    """Find pub fns matching fuzz target patterns."""
    candidates: list[dict] = []
    for rs in _CRATES_DIR.rglob("touring-*/src/**/*.rs"):
        try:
            content = rs.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for pat in FUZZ_TARGET_PATTERNS:
            for m in pat.finditer(content):
                candidates.append({
                    "file": str(rs.relative_to(_ROOT)),
                    "function": m.group(1),
                })
    return candidates


def emit_stub(candidate: dict, out_dir: Path) -> Path:
    """Emit cargo-fuzz target stub."""
    out_dir.mkdir(parents=True, exist_ok=True)
    name = candidate["function"]
    p = out_dir / f"fuzz_{name}.rs"
    p.write_text(
        f"//! AUTO-GENERATED fuzz target for `{name}`\n"
        f"//! Source: {candidate['file']}\n"
        "\n"
        "#![no_main]\n"
        "use libfuzzer_sys::fuzz_target;\n"
        "\n"
        "fuzz_target!(|data: &[u8]| {\n"
        f"    // TODO: invoke {name}(data) and ensure no panic.\n"
        f"    let _ = std::panic::catch_unwind(|| {{\n"
        f"        // touring_crate::{name}(data)\n"
        "    });\n"
        "});\n",
        encoding="utf-8")
    return p


def run(args: argparse.Namespace) -> dict:
    """Scan + emit stubs."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    candidates = scan_fuzz_candidates()
    emitted: list[str] = []
    if args.emit_stubs:
        stub_dir = _STAGING_DIR / "w11-fuzz-targets"
        for c in candidates[:30]:
            emitted.append(str(emit_stub(c, stub_dir).relative_to(_ROOT)))
    report = {
        "script": "w11_fuzz_target_seeder",
        "wave": "W11", "subtask_refs": ["W11.5"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "totals": {"candidates": len(candidates),
                    "stubs_emitted": len(emitted)},
        "top_20_candidates": candidates[:20],
        "stubs_emitted": emitted,
    }
    json_path = args.output_dir / "w11-fuzz-candidates.json"
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
            "top_5": [c["function"]
                       for c in result["top_20_candidates"][:5]],
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
