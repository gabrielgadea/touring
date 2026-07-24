#!/usr/bin/env python3
"""W5.1 — Generate touring-storage Cargo.toml + lib.rs skeleton.

W5 creates the storage layer (Layer 2 in new arch) by extracting persistence
code currently scattered across touring-hooks, touring-server, touring-cortex:
  - SQLite handles (knowledge_db, memory tier)
  - Tantivy index handles
  - rkyv archives
  - JSON file checkpoints

This script ONLY produces SKELETONS (NO DESTRUCTIVE EXTRACTION). The actual
migration is staged in W5.2-W5.4.

Outputs:
  - staging/w5-touring-storage/Cargo.toml
  - staging/w5-touring-storage/src/lib.rs
  - staging/w5-touring-storage/src/{sqlite,tantivy,rkyv_archive,checkpoint,error}.rs
  - data/w5-storage-extraction-targets.json

Usage
-----
    python3 w5_storage_skeleton.py
    python3 w5_storage_skeleton.py --scan-only
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
_SKELETON_DIR = _STAGING_DIR / "w5-touring-storage"

STORAGE_PATTERNS: dict[str, list[re.Pattern]] = {
    "sqlite": [re.compile(r"\brusqlite::"), re.compile(r"\bConnection::open"),
                re.compile(r"\bsqlx::")],
    "tantivy": [re.compile(r"\btantivy::"), re.compile(r"\bIndexWriter"),
                 re.compile(r"\bSearcher::")],
    "rkyv": [re.compile(r"\brkyv::"),
              re.compile(r"#\[derive\(.*Archive.*\)\]")],
    "checkpoint": [re.compile(r"checkpoint", re.I),
                    re.compile(r"serde_json::to_writer"),
                    re.compile(r"\.write_to_file")],
}


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w5_storage_skeleton", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--scan-only", action="store_true")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def scan_storage_code() -> dict[str, list[dict]]:
    """Scan workspace for files with storage patterns."""
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
            pattern_hits: dict[str, int] = {}
            for category, patterns in STORAGE_PATTERNS.items():
                count = sum(len(p.findall(content)) for p in patterns)
                if count > 0:
                    pattern_hits[category] = count
            if pattern_hits:
                findings[crate_dir.name].append({
                    "file": str(rs.relative_to(_ROOT)),
                    "loc": sum(1 for _ in content.splitlines()),
                    "pattern_hits": pattern_hits,
                    "total_hits": sum(pattern_hits.values()),
                })
    return {k: sorted(v, key=lambda x: -x["total_hits"])
             for k, v in findings.items()}


def emit_cargo_toml() -> Path:
    """Emit touring-storage/Cargo.toml skeleton."""
    _SKELETON_DIR.mkdir(parents=True, exist_ok=True)
    cargo = _SKELETON_DIR / "Cargo.toml"
    content = "\n".join([
        "# AUTO-GENERATED skeleton — touring-storage Cargo.toml",
        f"# Generated: {datetime.now(UTC).isoformat()}",
        "# W5.1 — Storage layer (Layer 2). Refine after W2.",
        "",
        "[package]",
        'name = "touring-storage"',
        'version.workspace = true',
        'edition.workspace = true',
        'rust-version.workspace = true',
        'license.workspace = true',
        "",
        "[dependencies]",
        "touring-foundation = { workspace = true }",
        "rusqlite = { workspace = true, features = [\"bundled\"] }",
        "tantivy = { workspace = true }",
        "rkyv = { workspace = true, features = [\"validation\"] }",
        "moka = { workspace = true }",
        "serde = { workspace = true, features = [\"derive\"] }",
        "serde_json = { workspace = true }",
        "thiserror = { workspace = true }",
        "blake3 = { workspace = true }",
        "tokio = { workspace = true, features = [\"sync\", \"rt\"] }",
        "",
        "[dev-dependencies]",
        "tempfile = { workspace = true }",
        "pretty_assertions = { workspace = true }",
        "tracing-test = { workspace = true }",
        "",
        "[lints]",
        "workspace = true",
        "",
        "[features]",
        'default = ["sqlite", "tantivy", "rkyv"]',
        "sqlite = []",
        "tantivy = []",
        "rkyv = []",
        "",
    ])
    cargo.write_text(content, encoding="utf-8")
    return cargo


def emit_lib_rs() -> Path:
    """Emit touring-storage/src/lib.rs skeleton."""
    src_dir = _SKELETON_DIR / "src"
    src_dir.mkdir(parents=True, exist_ok=True)
    lib = src_dir / "lib.rs"
    content = "\n".join([
        "//! `touring-storage` — Layer 2 of new Touring architecture.",
        "//!",
        "//! Centralizes ALL persistence concerns previously scattered across",
        "//! `touring-hooks`, `touring-server`, `touring-cortex`.",
        "",
        "#![forbid(unsafe_code)]",
        "#![deny(missing_docs)]",
        "",
        "#[cfg(feature = \"sqlite\")]",
        "pub mod sqlite;",
        "",
        "#[cfg(feature = \"tantivy\")]",
        "pub mod tantivy;",
        "",
        "#[cfg(feature = \"rkyv\")]",
        "pub mod rkyv_archive;",
        "",
        "pub mod checkpoint;",
        "pub mod error;",
        "",
        "pub use error::{StorageError, StorageResult};",
        "",
        "/// Common trait for read-write storage backends.",
        "pub trait Store {",
        "    /// Backend identifier (for logging / tracing).",
        "    fn backend(&self) -> &'static str;",
        "    /// Number of records currently stored.",
        "    fn len(&self) -> StorageResult<usize>;",
        "    /// `true` if no records.",
        "    fn is_empty(&self) -> StorageResult<bool> {",
        "        self.len().map(|n| n == 0)",
        "    }",
        "}",
        "",
    ])
    lib.write_text(content, encoding="utf-8")
    return lib


def emit_module_stub(name: str, doc: str) -> Path:
    """Emit src/<name>.rs stub."""
    src_dir = _SKELETON_DIR / "src"
    src_dir.mkdir(parents=True, exist_ok=True)
    p = src_dir / f"{name}.rs"
    struct_name = name.title().replace("_", "") + "Backend"
    content = f"""//! `{name}` — {doc}
//!
//! W5 PLACEHOLDER. Real implementation extracted from existing crates in W5.2-W5.4.

use crate::{{StorageError, StorageResult}};

/// TODO: replace with extracted code from `touring-hooks::{name}_*`.
pub struct {struct_name} {{}}

impl {struct_name} {{
    /// Create a new backend (W5 placeholder).
    pub fn new() -> StorageResult<Self> {{
        Ok(Self {{}})
    }}
}}

impl Default for {struct_name} {{
    fn default() -> Self {{
        Self::new().expect("default backend")
    }}
}}
"""
    p.write_text(content, encoding="utf-8")
    return p


def emit_error_module() -> Path:
    """Emit src/error.rs."""
    src_dir = _SKELETON_DIR / "src"
    src_dir.mkdir(parents=True, exist_ok=True)
    p = src_dir / "error.rs"
    content = '''//! Storage layer error types.

use thiserror::Error;

/// Storage-layer error.
#[derive(Debug, Error)]
pub enum StorageError {
    /// SQLite-specific error.
    #[error("sqlite error: {0}")]
    Sqlite(String),
    /// Tantivy-specific error.
    #[error("tantivy error: {0}")]
    Tantivy(String),
    /// rkyv archive error.
    #[error("rkyv error: {0}")]
    Rkyv(String),
    /// Checkpoint I/O error.
    #[error("checkpoint error: {0}")]
    Checkpoint(String),
    /// Generic I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result alias for storage operations.
pub type StorageResult<T> = Result<T, StorageError>;
'''
    p.write_text(content, encoding="utf-8")
    return p


def run(args: argparse.Namespace) -> dict:
    """Scan + emit skeleton (unless --scan-only)."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    findings = scan_storage_code()
    extraction = {
        "script": "w5_storage_skeleton",
        "wave": "W5", "subtask_refs": ["W5.1"],
        "timestamp": datetime.now(UTC).isoformat(),
        "totals": {
            "crates_with_storage_code": len(findings),
            "total_files": sum(len(v) for v in findings.values()),
            "total_hits": sum(f["total_hits"]
                                for v in findings.values() for f in v),
        },
        "findings_per_crate": findings,
        "top_15_extraction_targets": sorted(
            (f for files in findings.values() for f in files),
            key=lambda x: -x["total_hits"],
        )[:15],
    }
    json_path = args.output_dir / "w5-storage-extraction-targets.json"
    json_path.write_text(json.dumps(extraction, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    emitted: list[str] = []
    if not args.scan_only:
        emitted.append(str(emit_cargo_toml().relative_to(_ROOT)))
        emitted.append(str(emit_lib_rs().relative_to(_ROOT)))
        emitted.append(str(emit_error_module().relative_to(_ROOT)))
        for name, doc in [
            ("sqlite", "SQLite backend (extracted from knowledge_db)"),
            ("tantivy", "Tantivy BM25 index handle"),
            ("rkyv_archive", "rkyv zero-copy archive backend"),
            ("checkpoint", "JSON + TOON checkpoint writer/reader"),
        ]:
            emitted.append(str(emit_module_stub(name, doc).relative_to(_ROOT)))
    return {
        **extraction, "status": "OK",
        "json_path": str(json_path.relative_to(_ROOT)),
        "skeleton_files_emitted": emitted,
        "scan_only": args.scan_only,
    }


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
            "top_5_extraction_targets": [
                {"file": t["file"], "hits": t["total_hits"],
                 "patterns": list(t["pattern_hits"].keys())}
                for t in result["top_15_extraction_targets"][:5]
            ],
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
