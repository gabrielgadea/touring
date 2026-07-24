#!/usr/bin/env python3
"""W6.1 — Generate touring-intelligence crate skeleton (cortex+cognitive+learning+antt fusion).

Emits the target structure for the mega-fusion touring-intelligence crate
that absorbs touring-cortex + touring-cognitive + touring-learning + touring-antt.
"""
from __future__ import annotations

import argparse
import json
import logging
import sys
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"
_SKELETON_DIR = _STAGING_DIR / "w6-touring-intelligence"

INTERNAL_MODULES = [
    "cortex", "cognitive", "learning", "antt",
    "reasoning", "rl_bridge", "pipeline", "ann",
    "scoring", "fusion", "error",
]

FEATURES = [
    "intel-cortex", "intel-cognitive", "intel-learning", "intel-antt",
    "intel-reasoning", "intel-mcts", "intel-bandit", "intel-ann",
    "intel-pipeline", "intel-scoring", "intel-async",
]


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w6_intelligence_skeleton", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def emit_cargo() -> Path:
    """Emit Cargo.toml skeleton."""
    _SKELETON_DIR.mkdir(parents=True, exist_ok=True)
    cargo = _SKELETON_DIR / "Cargo.toml"
    deps_lines = "\n".join(f'{f} = []' for f in FEATURES)
    cargo.write_text("\n".join([
        "# AUTO-GENERATED — touring-intelligence mega-fusion skeleton",
        f"# Generated: {datetime.now(UTC).isoformat()}",
        "",
        "[package]",
        'name = "touring-intelligence"',
        'version.workspace = true',
        'edition.workspace = true',
        'license.workspace = true',
        "",
        "[features]",
        'default = []',
        deps_lines,
        "",
        "[dependencies]",
        "touring-foundation = { workspace = true }",
        "touring-storage = { workspace = true }",
        "touring-code = { workspace = true }",
        'serde = { workspace = true, features = ["derive"] }',
        "thiserror = { workspace = true }",
        'tokio = { workspace = true, features = ["sync"] }',
        "",
        "[lints]",
        "workspace = true",
        "",
    ]), encoding="utf-8")
    return cargo


def emit_lib_rs() -> Path:
    """Emit src/lib.rs with feature-gated module declarations."""
    src = _SKELETON_DIR / "src"
    src.mkdir(parents=True, exist_ok=True)
    lib = src / "lib.rs"
    mod_lines = []
    for m in INTERNAL_MODULES:
        if m == "error":
            mod_lines.append(f"pub mod {m};")
        else:
            mod_lines.append(f'#[cfg(feature = "intel-{m}")]')
            mod_lines.append(f"pub(crate) mod {m};")
    lib.write_text("\n".join([
        '//! `touring-intelligence` — Layer 4 mega-fusion crate.',
        '//!',
        '//! Combines cortex + cognitive + learning + antt under one boundary.',
        '//! Internal modules are pub(crate) to enforce façade re-export.',
        "",
        "#![forbid(unsafe_code)]",
        "#![deny(missing_docs)]",
        "",
        *mod_lines,
        "",
        "pub use error::{IntelError, IntelResult};",
        "",
    ]), encoding="utf-8")
    return lib


def run(args: argparse.Namespace) -> dict:
    """Emit skeleton."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    cargo = emit_cargo()
    lib = emit_lib_rs()
    report = {
        "script": "w6_intelligence_skeleton",
        "wave": "W6", "subtask_refs": ["W6.1"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "features": FEATURES,
        "internal_modules": INTERNAL_MODULES,
        "cargo_path": str(cargo.relative_to(_ROOT)),
        "lib_path": str(lib.relative_to(_ROOT)),
    }
    json_path = args.output_dir / "w6-intelligence-skeleton.json"
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
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
        return 0
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
