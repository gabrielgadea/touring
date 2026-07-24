#!/usr/bin/env python3
"""W12.x — `touring migrate --from-global`: copy filtered DBs + gen touring.toml.

Generates the migration tool spec for moving a user from the global
`~/.claude/rust/` deployment to per-project mode.
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
        prog="w12_migration_tool", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def emit_spec() -> Path:
    """Emit migration tool spec."""
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    md = _STAGING_DIR / "w12-migration-tool-spec.md"
    md.write_text("""# `touring migrate` — Migration Tool Spec (W12.x)

## Use case
User wants to migrate from global deployment (`~/.claude/rust/` shared) to
per-project mode (`<project>/.touring/`).

## Subcommands

```bash
touring migrate from-global --to <project>     # copy + filter global DBs
touring migrate from-global --dry-run          # preview without copying
touring migrate to-global --from <project>     # reverse (merge to global)
```

## Migration steps (from-global)

1. **Inspect** existing global state:
   - `~/.claude/projects/-home-gabrielgadea/memory/` (memory tier=*)
   - `~/.claude/rust/.touring-cache/` (knowledge_db, tantivy)
2. **Filter** by project path: keep only entries with path-prefix matching `<project>`
3. **Copy** filtered subset to `<project>/.touring/data/`:
   - `memory.db` (SQLite, filtered subset)
   - `tantivy/` (FTS5 index, rebuilt for project scope)
   - `knowledge.db` (semantic memories)
4. **Generate** `<project>/.touring/touring.toml`:
   - `channel = "stable-X.Y.Z"`
   - `[storage] backend = "sqlite"`
   - `[features] tier = "free"` (default)
5. **Verify** post-migration:
   - `touring doctor` from project dir
   - Compare memory count: global vs project-filtered

## Risks

| Risk | Mitigation |
|---|---|
| Lose cross-project memory | `--keep-global` flag (don't delete source) |
| Index rebuild slow (~5min) | Background job + progress bar |
| Partial migration corrupts state | Atomic: write to .touring.tmp then rename |
""", encoding="utf-8")
    return md


def run(args: argparse.Namespace) -> dict:
    """Emit spec."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    spec = emit_spec()
    report = {
        "script": "w12_migration_tool",
        "wave": "W12", "subtask_refs": ["W12.migrate"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "subcommands": ["from-global", "to-global"],
        "spec_path": str(spec.relative_to(_ROOT)),
    }
    json_path = args.output_dir / "w12-migration-manifest.json"
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
