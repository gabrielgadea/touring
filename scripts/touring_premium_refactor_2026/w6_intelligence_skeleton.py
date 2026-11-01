#!/usr/bin/env python3
"""W6.1 — Generate touring-intelligence crate skeleton (cortex+cognitive+learning+antt fusion).

Emits the target structure for the mega-fusion touring-intelligence crate
that absorbs touring-cortex + touring-cognitive + touring-learning + touring-antt.
"""
from __future__ import annotations

from pathlib import Path

import script_common as sc

_SCRIPT = "w6_intelligence_skeleton"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR
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


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


def emit_cargo() -> Path:
    """Emit Cargo.toml skeleton."""
    _SKELETON_DIR.mkdir(parents=True, exist_ok=True)
    cargo = _SKELETON_DIR / "Cargo.toml"
    deps_lines = "\n".join(f'{f} = []' for f in FEATURES)
    cargo.write_text("\n".join([
        "# AUTO-GENERATED — touring-intelligence mega-fusion skeleton",
        f"# Generated: {sc.now_iso()}",
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


def run(args: sc.Namespace) -> dict:
    """Emit skeleton."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    cargo = emit_cargo()
    lib = emit_lib_rs()
    report = {
        "script": "w6_intelligence_skeleton",
        "wave": "W6", "subtask_refs": ["W6.1"],
        "timestamp": sc.now_iso(),
        "status": "OK",
        "features": FEATURES,
        "internal_modules": INTERNAL_MODULES,
        "cargo_path": str(cargo.relative_to(_ROOT)),
        "lib_path": str(lib.relative_to(_ROOT)),
    }
    json_path = args.output_dir / "w6-intelligence-skeleton.json"
    return sc.write_json_report(json_path, report, _ROOT)


def _run_and_report(args: sc.Namespace) -> int:
    """Run w6_intelligence_skeleton, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json(result)
    return 0


def main() -> int:
    """Entry point of w6_intelligence_skeleton."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
