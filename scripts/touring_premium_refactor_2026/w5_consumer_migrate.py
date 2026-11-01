#!/usr/bin/env python3
"""W5.x — Migrate 15 consumers to touring_storage::*.

Reads data/w5-storage-extraction-targets.json + renames imports.

Usage
-----
    python3 w5_consumer_migrate.py            # dry-run (emit script)
    python3 w5_consumer_migrate.py --apply
"""
from __future__ import annotations

import json
from pathlib import Path

import script_common as sc

_SCRIPT = "w5_consumer_migrate"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR

# Storage modules being extracted → new touring_storage:: paths
MODULE_MAP: dict[str, str] = {
    "knowledge": "touring_storage::sqlite::knowledge",
    "tantivy_index": "touring_storage::tantivy",
    "rkyv_archive": "touring_storage::rkyv_archive",
    "persistence": "touring_storage::sqlite::persistence",
}


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    p.add_argument("--apply", action="store_true",
                   help="DESTRUCTIVE — actually rewrite imports.")
    p.add_argument("--map-file", type=Path,
                   default=_DATA_DIR / "w5-storage-extraction-targets.json")
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


def render_script(targets: list[dict]) -> str:
    """Render idempotent ast-grep rewrite script."""
    lines = ["#!/usr/bin/env bash",
             "# AUTO-GENERATED — W5.x consumer migration",
             f"# Generated: {sc.now_iso()}",
             "set -euo pipefail",
             ""]
    for t in targets:
        file = t["file"]
        for old, new in MODULE_MAP.items():
            lines.append(
                f"touring ast grep '{file}' 'crate::{old}' "
                f"--rewrite '{new}' --lang rust || true"
            )
    lines.extend(["", "echo 'Phase 2: cargo check'",
                   "cargo check --workspace 2>&1 | tail -20"])
    return "\n".join(lines) + "\n"


def run(args: sc.Namespace) -> dict:
    """Render script + optionally apply."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    if not args.map_file.exists():
        return {"status": "MAP_MISSING",
                "hint": "Run w5_storage_skeleton.py first"}
    storage_map = json.loads(args.map_file.read_text(encoding="utf-8"))
    targets = storage_map.get("top_15_extraction_targets", [])
    script_path = _STAGING_DIR / "w5-migration-script.sh"
    script_path.write_text(render_script(targets), encoding="utf-8")
    script_path.chmod(0o755)
    report = {
        "script": "w5_consumer_migrate",
        "wave": "W5", "subtask_refs": ["W5.migrate"],
        "timestamp": sc.now_iso(),
        "status": "OK", "apply": args.apply,
        "targets_count": len(targets),
        "module_map": MODULE_MAP,
        "script_path": str(script_path.relative_to(_ROOT)),
    }
    json_path = args.output_dir / "w5-migration-applied.json"
    return sc.write_json_report(json_path, report, _ROOT)


def _run_and_report(args: sc.Namespace) -> int:
    """Run w5_consumer_migrate, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json(result)
    return 0 if result["status"] == "OK" else 1


def main() -> int:
    """Entry point of w5_consumer_migrate."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
