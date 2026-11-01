#!/usr/bin/env python3
"""W11.5 — Identify candidate fuzz targets (parsers, serializers).

Scans for `pub fn parse`, `from_bytes`, `serialize` — high-value fuzz targets.
"""
from __future__ import annotations

import re
from pathlib import Path

import script_common as sc

_SCRIPT = "w11_fuzz_target_seeder"

_ROOT, _DATA_DIR, _STAGING_DIR, _CRATES_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR, sc.CRATES_DIR

FUZZ_TARGET_PATTERNS = [
    re.compile(r"pub\s+fn\s+(parse\w*)\s*\("),
    re.compile(r"pub\s+fn\s+(from_bytes\w*)\s*\("),
    re.compile(r"pub\s+fn\s+(deserialize\w*)\s*\("),
    re.compile(r"pub\s+fn\s+(decode\w*)\s*\("),
]


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    p.add_argument("--emit-stubs", action="store_true")
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


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


def run(args: sc.Namespace) -> dict:
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
        "timestamp": sc.now_iso(),
        "status": "OK",
        "totals": {"candidates": len(candidates),
                    "stubs_emitted": len(emitted)},
        "top_20_candidates": candidates[:20],
        "stubs_emitted": emitted,
    }
    json_path = args.output_dir / "w11-fuzz-candidates.json"
    return sc.write_json_report(json_path, report, _ROOT)


def _run_and_report(args: sc.Namespace) -> int:
    """Run w11_fuzz_target_seeder, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json({
        "status": result["status"], "totals": result["totals"],
        "top_5": [c["function"]
                   for c in result["top_20_candidates"][:5]],
        "json_path": result["json_path"],
    })
    return 0


def main() -> int:
    """Entry point of w11_fuzz_target_seeder."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
