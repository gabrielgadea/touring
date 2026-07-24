#!/usr/bin/env python3
"""W7.1 — Generate touring-bindings skeleton (Python + TypeScript bindings crate).

W7 creates the bindings layer (Layer 5 — Product). The skeleton:
  - Empty default features (no auto-loading — caller picks Python OR TS)
  - 6 opt-in features: bind-py (PyO3), bind-ts-napi (napi-rs), bind-wasm,
    bind-async, bind-stream, bind-stable-api
  - Per-language module separation (src/py.rs, src/ts.rs, src/wasm.rs)

This script ONLY produces SKELETON FILES. Actual binding implementations
land in W7.2-W7.7.

Outputs:
  - staging/w7-touring-bindings/Cargo.toml
  - staging/w7-touring-bindings/src/{lib,py,ts,wasm,common}.rs
  - staging/w7-touring-bindings/pyproject.toml  (Python packaging stub)
  - staging/w7-touring-bindings/package.json    (npm packaging stub)
  - data/w7-bindings-skeleton.json              (manifest)

Usage
-----
    python3 w7_bindings_skeleton.py
    python3 w7_bindings_skeleton.py --skip-pyproject --skip-package-json
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
_SKELETON_DIR = _STAGING_DIR / "w7-touring-bindings"


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w7_bindings_skeleton", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--skip-pyproject", action="store_true")
    p.add_argument("--skip-package-json", action="store_true")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def emit_cargo_toml() -> Path:
    """Emit touring-bindings/Cargo.toml with feature flags."""
    _SKELETON_DIR.mkdir(parents=True, exist_ok=True)
    cargo = _SKELETON_DIR / "Cargo.toml"
    content = "\n".join([
        "# AUTO-GENERATED — W7.1 touring-bindings skeleton",
        f"# Generated: {datetime.now(UTC).isoformat()}",
        "# Layer 5 (Product) bindings crate. Default features EMPTY by design.",
        "",
        "[package]",
        'name = "touring-bindings"',
        'version.workspace = true',
        'edition.workspace = true',
        'license.workspace = true',
        "",
        "[lib]",
        "# cdylib for Python wheels + napi node modules",
        'crate-type = ["cdylib", "rlib"]',
        "",
        "[features]",
        "# DEFAULT EMPTY — caller picks language binding",
        'default = []',
        "",
        "# Language-specific feature groups",
        'bind-py = ["dep:pyo3", "dep:pyo3-asyncio", "common"]',
        'bind-ts-napi = ["dep:napi", "dep:napi-derive", "common"]',
        'bind-wasm = ["dep:wasm-bindgen", "dep:js-sys", "common"]',
        "",
        "# Feature modifiers",
        'bind-async = ["tokio/rt-multi-thread"]',
        'bind-stream = ["dep:tokio-stream"]',
        'bind-stable-api = []  # opt-in to lock semver',
        "",
        "# Internal helper feature (re-exported common types)",
        'common = []',
        "",
        "[dependencies]",
        "# Touring core deps (workspace-inherited)",
        "touring-foundation = { workspace = true }",
        "touring-storage = { workspace = true }",
        "touring-intelligence = { workspace = true }",
        "",
        "# Runtime",
        "tokio = { workspace = true }",
        "tokio-stream = { workspace = true, optional = true }",
        "",
        "# Python (PyO3)",
        'pyo3 = { workspace = true, optional = true, features = ["extension-module"] }',
        "pyo3-asyncio = { workspace = true, optional = true }",
        "",
        "# Node (napi-rs)",
        'napi = { workspace = true, optional = true, features = ["napi9", "async"] }',
        "napi-derive = { workspace = true, optional = true }",
        "",
        "# Wasm",
        "wasm-bindgen = { workspace = true, optional = true }",
        "js-sys = { workspace = true, optional = true }",
        "",
        "# Serialization",
        'serde = { workspace = true, features = ["derive"] }',
        "serde_json = { workspace = true }",
        "thiserror = { workspace = true }",
        "",
        "[build-dependencies]",
        "napi-build = { workspace = true, optional = true }",
        "",
        "[dev-dependencies]",
        "pretty_assertions = { workspace = true }",
        "tracing-test = { workspace = true }",
        "",
        "[lints]",
        "workspace = true",
        "",
    ])
    cargo.write_text(content, encoding="utf-8")
    return cargo


def emit_src_files() -> list[Path]:
    """Emit Rust source skeleton files."""
    src = _SKELETON_DIR / "src"
    src.mkdir(parents=True, exist_ok=True)
    paths: list[Path] = []
    # lib.rs — feature-gated module dispatch
    lib_rs = src / "lib.rs"
    lib_rs.write_text("\n".join([
        '//! `touring-bindings` — Language bindings for Touring (W7).',
        '//!',
        '//! Build with EXACTLY ONE language feature: `bind-py`, `bind-ts-napi`,',
        '//! or `bind-wasm`. The default feature set is empty by design — callers',
        '//! must opt in to the target language to avoid pulling unnecessary deps.',
        "",
        "#![forbid(unsafe_code)]",
        "#![deny(missing_docs)]",
        "",
        '#[cfg(feature = "common")]',
        "pub mod common;",
        "",
        '#[cfg(feature = "bind-py")]',
        "pub mod py;",
        "",
        '#[cfg(feature = "bind-ts-napi")]',
        "pub mod ts;",
        "",
        '#[cfg(feature = "bind-wasm")]',
        "pub mod wasm;",
        "",
        "/// Bindings crate version (semver-compatible with workspace).",
        'pub const VERSION: &str = env!("CARGO_PKG_VERSION");',
        "",
    ]), encoding="utf-8")
    paths.append(lib_rs)
    # common.rs
    common = src / "common.rs"
    common.write_text('''//! Shared types across all language bindings.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Language-agnostic binding error.
#[derive(Debug, Error)]
pub enum BindingError {
    /// Underlying Touring error.
    #[error("touring error: {0}")]
    Touring(String),
    /// Serialization/deserialization failure.
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    /// Invalid input from the caller language.
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

/// Result alias for binding operations.
pub type BindingResult<T> = Result<T, BindingError>;

/// Public façade for the simplest workflow ("hello touring").
#[derive(Debug, Serialize, Deserialize)]
pub struct Greeting {
    /// Greeting payload.
    pub message: String,
    /// Touring version.
    pub touring_version: String,
}

impl Greeting {
    /// Construct a new greeting.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            touring_version: crate::VERSION.to_owned(),
        }
    }
}
''', encoding="utf-8")
    paths.append(common)
    # py.rs
    py_rs = src / "py.rs"
    py_rs.write_text('''//! PyO3 bindings (W7.2 placeholder).

#[cfg(feature = "bind-py")]
use pyo3::prelude::*;

#[cfg(feature = "bind-py")]
use crate::common::Greeting;

#[cfg(feature = "bind-py")]
#[pyfunction]
fn hello(message: String) -> PyResult<String> {
    let g = Greeting::new(message);
    Ok(format!("{} (touring {})", g.message, g.touring_version))
}

#[cfg(feature = "bind-py")]
#[pymodule]
fn touring(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(hello, m)?)?;
    Ok(())
}
''', encoding="utf-8")
    paths.append(py_rs)
    # ts.rs
    ts_rs = src / "ts.rs"
    ts_rs.write_text('''//! napi-rs TypeScript bindings (W7.3 placeholder).

#[cfg(feature = "bind-ts-napi")]
use napi_derive::napi;

#[cfg(feature = "bind-ts-napi")]
use crate::common::Greeting;

#[cfg(feature = "bind-ts-napi")]
#[napi]
pub fn hello(message: String) -> String {
    let g = Greeting::new(message);
    format!("{} (touring {})", g.message, g.touring_version)
}
''', encoding="utf-8")
    paths.append(ts_rs)
    # wasm.rs
    wasm_rs = src / "wasm.rs"
    wasm_rs.write_text('''//! wasm-bindgen bindings (W7.4 placeholder).

#[cfg(feature = "bind-wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "bind-wasm")]
use crate::common::Greeting;

#[cfg(feature = "bind-wasm")]
#[wasm_bindgen]
pub fn hello(message: String) -> String {
    let g = Greeting::new(message);
    format!("{} (touring {})", g.message, g.touring_version)
}
''', encoding="utf-8")
    paths.append(wasm_rs)
    return paths


def emit_pyproject() -> Path:
    """Emit pyproject.toml stub (maturin-friendly)."""
    _SKELETON_DIR.mkdir(parents=True, exist_ok=True)
    p = _SKELETON_DIR / "pyproject.toml"
    p.write_text("\n".join([
        '[build-system]',
        'requires = ["maturin>=1.0,<2.0"]',
        'build-backend = "maturin"',
        '',
        '[project]',
        'name = "touring"',
        'description = "Python bindings for Touring code intelligence"',
        'authors = [{name = "Gabriel Gadea"}]',
        'license = "MIT"',
        'requires-python = ">=3.10"',
        'classifiers = [',
        '    "Programming Language :: Rust",',
        '    "Programming Language :: Python :: 3",',
        ']',
        '',
        '[tool.maturin]',
        'features = ["bind-py"]',
        'module-name = "touring"',
        '',
    ]), encoding="utf-8")
    return p


def emit_package_json() -> Path:
    """Emit package.json stub (napi-rs friendly)."""
    _SKELETON_DIR.mkdir(parents=True, exist_ok=True)
    p = _SKELETON_DIR / "package.json"
    p.write_text(json.dumps({
        "name": "@touring/native",
        "version": "0.1.0",
        "description": "TypeScript bindings for Touring code intelligence",
        "main": "index.js",
        "types": "index.d.ts",
        "license": "MIT",
        "engines": {"node": ">=18"},
        "napi": {
            "name": "touring",
            "triples": {"defaults": True, "additional": []},
        },
        "scripts": {
            "build": "napi build --platform --release",
            "test": "node test/index.test.js",
        },
        "devDependencies": {"@napi-rs/cli": "^2.18.0"},
    }, indent=2) + "\n", encoding="utf-8")
    return p


def run(args: argparse.Namespace) -> dict:
    """Emit all skeleton artifacts."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    emitted: list[str] = []
    emitted.append(str(emit_cargo_toml().relative_to(_ROOT)))
    for p in emit_src_files():
        emitted.append(str(p.relative_to(_ROOT)))
    if not args.skip_pyproject:
        emitted.append(str(emit_pyproject().relative_to(_ROOT)))
    if not args.skip_package_json:
        emitted.append(str(emit_package_json().relative_to(_ROOT)))
    manifest = {
        "script": "w7_bindings_skeleton",
        "wave": "W7", "subtask_refs": ["W7.1"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "features_planned": [
            "bind-py", "bind-ts-napi", "bind-wasm",
            "bind-async", "bind-stream", "bind-stable-api",
        ],
        "files_emitted": emitted,
    }
    json_path = args.output_dir / "w7-bindings-skeleton.json"
    json_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    manifest["json_path"] = str(json_path.relative_to(_ROOT))
    return manifest


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
            "status": result["status"],
            "features_planned": result["features_planned"],
            "files_emitted_count": len(result["files_emitted"]),
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
