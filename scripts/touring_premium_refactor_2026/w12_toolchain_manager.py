#!/usr/bin/env python3
"""W12.x — Generate `touring toolchain` subcommand spec (~/.touring/toolchains/).

Emits skeleton for managing multiple Touring versions installed in parallel
(stable, nightly, pinned). Pattern mirrors rustup.
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
        prog="w12_toolchain_manager", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def emit_subcommand_spec() -> Path:
    """Emit subcommand spec Markdown."""
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    md = _STAGING_DIR / "w12-toolchain-subcommands.md"
    md.write_text("""# `touring toolchain` — Subcommand Spec (W12.x)

## Subcommands

| Cmd | Description |
|---|---|
| `touring toolchain list` | List installed toolchains |
| `touring toolchain install <channel>` | Install channel (stable\\|nightly\\|<version>) |
| `touring toolchain remove <channel>` | Remove a toolchain |
| `touring toolchain default <channel>` | Set default toolchain |
| `touring toolchain link <name> <path>` | Link a local build as toolchain |
| `touring update` | Update default toolchain to latest stable |

## Filesystem layout

```
~/.touring/
├── toolchains/
│   ├── stable-1.0.0/        (active)
│   ├── nightly-2026-05-11/
│   └── pinned-0.9.5/
├── cache/                    (shared downloads)
├── settings.toml             ({active_channel = "stable-1.0.0"})
└── bin/touring               (symlink to active toolchain)
```

## Per-project override

```toml
# <project>/.touring/touring.toml
[toolchain]
channel = "nightly-2026-05-11"   # overrides ~/.touring/settings.toml
```
""", encoding="utf-8")
    return md


def run(args: argparse.Namespace) -> dict:
    """Emit spec."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    spec = emit_subcommand_spec()
    report = {
        "script": "w12_toolchain_manager",
        "wave": "W12", "subtask_refs": ["W12.toolchain"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "subcommands": ["list", "install", "remove", "default", "link", "update"],
        "spec_path": str(spec.relative_to(_ROOT)),
    }
    json_path = args.output_dir / "w12-toolchain-manifest.json"
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
