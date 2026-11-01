#!/usr/bin/env python3
"""W12.x — `touring migrate --from-global`: copy filtered DBs + gen touring.toml.

Generates the migration tool spec for moving a user from the global
`~/.claude/rust/` deployment to per-project mode.
"""
from __future__ import annotations

from pathlib import Path

import script_common as sc

_SCRIPT = "w12_migration_tool"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


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


def run(args: sc.Namespace) -> dict:
    """Emit spec."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    spec = emit_spec()
    report = {
        "script": "w12_migration_tool",
        "wave": "W12", "subtask_refs": ["W12.migrate"],
        "timestamp": sc.now_iso(),
        "status": "OK",
        "subcommands": ["from-global", "to-global"],
        "spec_path": str(spec.relative_to(_ROOT)),
    }
    json_path = args.output_dir / "w12-migration-manifest.json"
    return sc.write_json_report(json_path, report, _ROOT)


def _run_and_report(args: sc.Namespace) -> int:
    """Run w12_migration_tool, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json(result)
    return 0


def main() -> int:
    """Entry point of w12_migration_tool."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
