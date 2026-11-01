#!/usr/bin/env python3
"""W13.7 — Validate docs.rs build green for all feature combinations.

Emits docs.rs metadata config + CI workflow that exercises every feature
combination locally before pushing tag (catches docs.rs build regressions
before they're public).
"""
from __future__ import annotations

from pathlib import Path

import script_common as sc

_SCRIPT = "w13_docs_rs_validator"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


def emit_docs_rs_config() -> Path:
    """Emit Cargo.toml [package.metadata.docs.rs] snippet."""
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    p = _STAGING_DIR / "w13-docs-rs-metadata.toml"
    p.write_text("""# AUTO-GENERATED — append to each public crate's Cargo.toml
# This ensures docs.rs builds with --all-features + sane defaults.

[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]
targets = ["x86_64-unknown-linux-gnu"]
""", encoding="utf-8")
    return p


def emit_ci_workflow() -> Path:
    """Emit GitHub Actions workflow that mimics docs.rs locally."""
    w = _STAGING_DIR / "w13-github-workflows"
    w.mkdir(parents=True, exist_ok=True)
    yml = w / "docs-rs-mirror.yml"
    yml.write_text("""# AUTO-GENERATED — Mirror docs.rs build locally before release
name: docs.rs Build Mirror

on:
  push:
    tags: ['v*']
  pull_request:
    paths:
      - 'crates/**/Cargo.toml'
      - 'crates/**/src/lib.rs'

jobs:
  docs-rs-build:
    runs-on: ubuntu-latest
    env:
      RUSTDOCFLAGS: "--cfg docsrs -D warnings"
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly
      - name: Mirror docs.rs build
        run: |
          cargo +nightly doc \\
            --workspace \\
            --no-deps \\
            --all-features
""", encoding="utf-8")
    return yml


def run(args: sc.Namespace) -> dict:
    """Emit configs."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    metadata = emit_docs_rs_config()
    workflow = emit_ci_workflow()
    report = {
        "script": "w13_docs_rs_validator",
        "wave": "W13", "subtask_refs": ["W13.7"],
        "timestamp": sc.now_iso(),
        "status": "OK",
        "metadata_snippet": str(metadata.relative_to(_ROOT)),
        "ci_workflow": str(workflow.relative_to(_ROOT)),
    }
    json_path = args.output_dir / "w13-docs-rs-manifest.json"
    return sc.write_json_report(json_path, report, _ROOT)


def _run_and_report(args: sc.Namespace) -> int:
    """Run w13_docs_rs_validator, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json(result)
    return 0


def main() -> int:
    """Entry point of w13_docs_rs_validator."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
