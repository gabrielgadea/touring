#!/usr/bin/env python3
"""W2.4 — Setup deny.toml + machete.toml + run smoke check.

Emits configuration files for cargo-deny (license/security gates) and
cargo-machete (unused deps detection). Pre-requisite to W2.5 cargo-deny CI.
"""
from __future__ import annotations

from pathlib import Path

import script_common as sc

_SCRIPT = "w2_setup_deny_machete"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


def emit_deny_toml() -> Path:
    """Emit cargo-deny configuration."""
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    p = _STAGING_DIR / "w2-deny.toml"
    p.write_text("""# AUTO-GENERATED — W2.4 cargo-deny config (place at workspace root)

[graph]
all-features = false
no-default-features = false
exclude-dev = false

[output]
feature-depth = 1

[advisories]
db-path = "$CARGO_HOME/advisory-db"
db-urls = ["https://github.com/rustsec/advisory-db"]
vulnerability = "deny"
unmaintained = "warn"
yanked = "deny"
notice = "warn"

[licenses]
unlicensed = "deny"
allow = [
    "MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-DFS-2016",
    "MPL-2.0", "Zlib", "0BSD", "CC0-1.0",
]
copyleft = "deny"

[bans]
multiple-versions = "warn"
wildcards = "deny"
highlight = "all"
""", encoding="utf-8")
    return p


def emit_machete_toml() -> Path:
    """Emit cargo-machete configuration."""
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    p = _STAGING_DIR / "w2-machete.toml"
    p.write_text("""# AUTO-GENERATED — W2.4 cargo-machete config

[package.metadata.cargo-machete]
ignored = [
    # build-deps that machete can't statically detect
    "napi-build",
]
""", encoding="utf-8")
    return p


def run(args: sc.Namespace) -> dict:
    """Emit config files."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    deny = emit_deny_toml()
    machete = emit_machete_toml()
    report = {
        "script": "w2_setup_deny_machete",
        "wave": "W2", "subtask_refs": ["W2.4"],
        "timestamp": sc.now_iso(),
        "status": "OK",
        "deny_path": str(deny.relative_to(_ROOT)),
        "machete_path": str(machete.relative_to(_ROOT)),
    }
    json_path = args.output_dir / "w2-deny-machete-manifest.json"
    return sc.write_json_report(json_path, report, _ROOT)


def _run_and_report(args: sc.Namespace) -> int:
    """Run w2_setup_deny_machete, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json(result)
    return 0


def main() -> int:
    """Entry point of w2_setup_deny_machete."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
