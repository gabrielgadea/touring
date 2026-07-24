#!/usr/bin/env python3
"""W10.1 — Generate touring-orchestration crate skeleton (decompose + session + RL bridge).

W10 creates the orchestration layer (Layer 5 in arch). Combines what's currently
spread across touring-hooks (decompose/session), touring-cortex (decompose
gateway), and touring-server (session lifecycle) into a single coherent crate.

Features (default empty by design):
  - orch-decompose: DAG creation + subtask deps + ready/finalize
  - orch-session: session lifecycle (start, assess, end) + persistence
  - orch-rl-bridge: reward injection + bandit consultation
  - orch-async: tokio-based reactor for long-running workflows

This script ONLY produces SKELETON. Real extraction happens in W10.2-W10.5.

Outputs:
  - staging/w10-touring-orchestration/Cargo.toml
  - staging/w10-touring-orchestration/src/{lib,decompose,session,rl_bridge,error}.rs
  - data/w10-orchestration-extraction-targets.json
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
_SKELETON_DIR = _STAGING_DIR / "w10-touring-orchestration"

PATTERNS: dict[str, list[re.Pattern]] = {
    "decompose": [re.compile(r"\bdecompose"), re.compile(r"\bsubtask\b"),
                   re.compile(r"\bTaskGraph")],
    "session": [re.compile(r"\bsession_(start|assess|end)"),
                 re.compile(r"\bSessionState")],
    "rl_bridge": [re.compile(r"\blearning::reward"),
                   re.compile(r"\bLinUCB"), re.compile(r"\bBandit")],
}


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w10_orchestration_skeleton", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--scan-only", action="store_true")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def scan_orchestration_code() -> dict[str, list[dict]]:
    """Scan workspace for orchestration patterns."""
    findings: dict[str, list[dict]] = defaultdict(list)
    for crate_dir in sorted(_CRATES_DIR.glob("touring-*")):
        src = crate_dir / "src"
        if not src.exists():
            continue
        for rs in src.rglob("*.rs"):
            try:
                content = rs.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            hits: dict[str, int] = {}
            for category, patterns in PATTERNS.items():
                count = sum(len(p.findall(content)) for p in patterns)
                if count > 0:
                    hits[category] = count
            if hits:
                findings[crate_dir.name].append({
                    "file": str(rs.relative_to(_ROOT)),
                    "loc": sum(1 for _ in content.splitlines()),
                    "pattern_hits": hits,
                    "total_hits": sum(hits.values()),
                })
    return {k: sorted(v, key=lambda x: -x["total_hits"])
             for k, v in findings.items()}


def emit_cargo() -> Path:
    """Emit touring-orchestration/Cargo.toml."""
    _SKELETON_DIR.mkdir(parents=True, exist_ok=True)
    cargo = _SKELETON_DIR / "Cargo.toml"
    cargo.write_text("\n".join([
        "# AUTO-GENERATED — W10.1 touring-orchestration skeleton",
        f"# Generated: {datetime.now(UTC).isoformat()}",
        "",
        "[package]",
        'name = "touring-orchestration"',
        'version.workspace = true',
        'edition.workspace = true',
        'license.workspace = true',
        "",
        "[features]",
        'default = []',
        'orch-decompose = []',
        'orch-session = []',
        'orch-rl-bridge = []',
        'orch-async = ["tokio/rt-multi-thread"]',
        "",
        "[dependencies]",
        "touring-foundation = { workspace = true }",
        "touring-storage = { workspace = true }",
        "touring-intelligence = { workspace = true, optional = true }",
        'serde = { workspace = true, features = ["derive"] }',
        "serde_json = { workspace = true }",
        "thiserror = { workspace = true }",
        'tokio = { workspace = true, features = ["sync"] }',
        "",
        "[lints]",
        "workspace = true",
        "",
    ]), encoding="utf-8")
    return cargo


def emit_lib() -> Path:
    """Emit src/lib.rs."""
    src = _SKELETON_DIR / "src"
    src.mkdir(parents=True, exist_ok=True)
    lib = src / "lib.rs"
    lib.write_text("\n".join([
        '//! `touring-orchestration` — Layer 5 (Product) orchestration.',
        '//!',
        '//! Combines decompose (DAG), session (lifecycle), rl_bridge (reward).',
        "",
        "#![forbid(unsafe_code)]",
        "#![deny(missing_docs)]",
        "",
        '#[cfg(feature = "orch-decompose")]',
        "pub mod decompose;",
        '#[cfg(feature = "orch-session")]',
        "pub mod session;",
        '#[cfg(feature = "orch-rl-bridge")]',
        "pub mod rl_bridge;",
        "pub mod error;",
        "",
        "pub use error::{OrchError, OrchResult};",
        "",
    ]), encoding="utf-8")
    return lib


def emit_module(name: str, doc: str) -> Path:
    """Emit src/<name>.rs placeholder."""
    src = _SKELETON_DIR / "src"
    src.mkdir(parents=True, exist_ok=True)
    p = src / f"{name}.rs"
    cls = name.title().replace("_", "")
    p.write_text(
        f"//! `{name}` — {doc}\n"
        "//!\n"
        "//! W10 PLACEHOLDER. Extracted from existing crates in W10.2-W10.5.\n"
        "\n"
        f"use crate::{{OrchError, OrchResult}};\n"
        "\n"
        f"/// TODO: real impl extracted from existing crates.\n"
        f"pub struct {cls}Service {{}}\n"
        "\n"
        f"impl {cls}Service {{\n"
        "    /// Create new service.\n"
        f"    pub fn new() -> OrchResult<Self> {{ Ok(Self {{}}) }}\n"
        "}\n"
        "\n"
        f"impl Default for {cls}Service {{\n"
        "    fn default() -> Self { Self::new().expect(\"default\") }\n"
        "}\n",
        encoding="utf-8")
    return p


def emit_error() -> Path:
    """Emit src/error.rs."""
    p = _SKELETON_DIR / "src" / "error.rs"
    p.write_text('''//! Orchestration error types.

use thiserror::Error;

/// Orchestration-layer error.
#[derive(Debug, Error)]
pub enum OrchError {
    /// Decompose DAG error.
    #[error("decompose error: {0}")]
    Decompose(String),
    /// Session lifecycle error.
    #[error("session error: {0}")]
    Session(String),
    /// RL bridge error.
    #[error("rl_bridge error: {0}")]
    RlBridge(String),
    /// I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result alias for orchestration operations.
pub type OrchResult<T> = Result<T, OrchError>;
''', encoding="utf-8")
    return p


def run(args: argparse.Namespace) -> dict:
    """Scan + emit."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    findings = scan_orchestration_code()
    emitted: list[str] = []
    if not args.scan_only:
        emitted.append(str(emit_cargo().relative_to(_ROOT)))
        emitted.append(str(emit_lib().relative_to(_ROOT)))
        emitted.append(str(emit_error().relative_to(_ROOT)))
        for name, doc in [
            ("decompose", "DAG creation, subtask deps, ready/finalize"),
            ("session", "session lifecycle (start, assess, end)"),
            ("rl_bridge", "reward injection + bandit consultation"),
        ]:
            emitted.append(str(emit_module(name, doc).relative_to(_ROOT)))
    report = {
        "script": "w10_orchestration_skeleton",
        "wave": "W10", "subtask_refs": ["W10.1"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "totals": {
            "crates_with_orch_code": len(findings),
            "total_files": sum(len(v) for v in findings.values()),
            "total_hits": sum(f["total_hits"]
                                for v in findings.values() for f in v),
        },
        "findings_per_crate": findings,
        "skeleton_files_emitted": emitted,
    }
    json_path = args.output_dir / "w10-orchestration-extraction-targets.json"
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
            "skeleton_files_emitted_count": len(result["skeleton_files_emitted"]),
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
