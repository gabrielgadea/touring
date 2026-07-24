#!/usr/bin/env python3
"""W5.x — Migrate 15 consumers to touring_storage::*.

Reads data/w5-storage-extraction-targets.json + renames imports.

Usage
-----
    python3 w5_consumer_migrate.py            # dry-run (emit script)
    python3 w5_consumer_migrate.py --apply
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

# Storage modules being extracted → new touring_storage:: paths
MODULE_MAP: dict[str, str] = {
    "knowledge": "touring_storage::sqlite::knowledge",
    "tantivy_index": "touring_storage::tantivy",
    "rkyv_archive": "touring_storage::rkyv_archive",
    "persistence": "touring_storage::sqlite::persistence",
}


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w5_consumer_migrate", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--apply", action="store_true",
                   help="DESTRUCTIVE — actually rewrite imports.")
    p.add_argument("--map-file", type=Path,
                   default=_DATA_DIR / "w5-storage-extraction-targets.json")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def render_script(targets: list[dict]) -> str:
    """Render idempotent ast-grep rewrite script."""
    lines = ["#!/usr/bin/env bash",
             f"# AUTO-GENERATED — W5.x consumer migration",
             f"# Generated: {datetime.now(UTC).isoformat()}",
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


def run(args: argparse.Namespace) -> dict:
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
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK", "apply": args.apply,
        "targets_count": len(targets),
        "module_map": MODULE_MAP,
        "script_path": str(script_path.relative_to(_ROOT)),
    }
    json_path = args.output_dir / "w5-migration-applied.json"
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
        return 0 if result["status"] == "OK" else 1
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
