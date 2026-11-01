#!/usr/bin/env python3
"""W12.x — Generate `touring toolchain` subcommand spec (~/.touring/toolchains/).

Emits skeleton for managing multiple Touring versions installed in parallel
(stable, nightly, pinned). Pattern mirrors rustup.
"""
from __future__ import annotations

from pathlib import Path

import script_common as sc

_SCRIPT = "w12_toolchain_manager"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


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


def run(args: sc.Namespace) -> dict:
    """Emit spec."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    spec = emit_subcommand_spec()
    report = {
        "script": "w12_toolchain_manager",
        "wave": "W12", "subtask_refs": ["W12.toolchain"],
        "timestamp": sc.now_iso(),
        "status": "OK",
        "subcommands": ["list", "install", "remove", "default", "link", "update"],
        "spec_path": str(spec.relative_to(_ROOT)),
    }
    json_path = args.output_dir / "w12-toolchain-manifest.json"
    return sc.write_json_report(json_path, report, _ROOT)


def _run_and_report(args: sc.Namespace) -> int:
    """Run w12_toolchain_manager, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json(result)
    return 0


def main() -> int:
    """Entry point of w12_toolchain_manager."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
