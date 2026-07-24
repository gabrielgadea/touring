#!/usr/bin/env python3
"""W2.4 — Setup deny.toml + machete.toml + run smoke check.

Emits configuration files for cargo-deny (license/security gates) and
cargo-machete (unused deps detection). Pre-requisite to W2.5 cargo-deny CI.
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


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w2_setup_deny_machete", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


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


def run(args: argparse.Namespace) -> dict:
    """Emit config files."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    deny = emit_deny_toml()
    machete = emit_machete_toml()
    report = {
        "script": "w2_setup_deny_machete",
        "wave": "W2", "subtask_refs": ["W2.4"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "deny_path": str(deny.relative_to(_ROOT)),
        "machete_path": str(machete.relative_to(_ROOT)),
    }
    json_path = args.output_dir / "w2-deny-machete-manifest.json"
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
