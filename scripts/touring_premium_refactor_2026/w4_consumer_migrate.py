#!/usr/bin/env python3
"""W4.8 — Migrate consumers from touring-{ast,polyglot,language,semantics} → touring-code.

Uses output from w4_consumer_audit.py to render ast-grep rewrite script.
With --apply, performs the actual rewrites via touring CLI.

Usage
-----
    python3 w4_consumer_migrate.py                   # render script only
    python3 w4_consumer_migrate.py --apply           # actually rewrite
"""
from __future__ import annotations

import argparse
import json
import logging
import re
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

MODULE_MAP: dict[str, str] = {
    "touring_ast": "touring_code::ast",
    "touring_ast_polyglot": "touring_code::polyglot",
    "touring_language": "touring_code::language",
    "touring_semantics": "touring_code::semantics",
}
CRATE_MAP: dict[str, str] = {
    "touring-ast": "touring-code",
    "touring-ast-polyglot": "touring-code",
    "touring-language": "touring-code",
    "touring-semantics": "touring-code",
}


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w4_consumer_migrate", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--apply", action="store_true",
                   help="DESTRUCTIVE — perform actual rewrites.")
    p.add_argument("--crate", type=str, default="")
    p.add_argument("--map-file", type=Path,
                   default=_DATA_DIR / "w4-consumer-map.json")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def load_consumer_map(path: Path) -> dict:
    """Load w4-consumer-map.json."""
    if not path.exists():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def render_migration_script(consumer_map: dict) -> str:
    """Render idempotent bash script."""
    lines = ["#!/usr/bin/env bash",
             "# AUTO-GENERATED — W4.8 migration",
             f"# Generated: {datetime.now(UTC).isoformat()}",
             "set -euo pipefail", "",
             "echo 'Phase 1: Rewrite use statements...'", ""]
    details = consumer_map.get("details", {})
    for source_crate, crates in details.items():
        old_mod = source_crate.replace("-", "_")
        new_mod = MODULE_MAP[old_mod]
        for crate_name, files in crates.items():
            for f in files:
                lines.append(
                    f"touring ast grep '{f['file']}' '{old_mod}' "
                    f"--rewrite '{new_mod}' --lang rust || true"
                )
    lines.extend(["", "echo 'Phase 2: Update Cargo.toml dependencies...'", ""])
    for source_crate, crates in details.items():
        for crate_name in crates:
            lines.append(
                f"sed -i 's/{source_crate} *=/touring-code =/g' "
                f"crates/{crate_name}/Cargo.toml 2>/dev/null || true"
            )
    lines.extend(["", "echo 'Phase 3: cargo check --workspace'", ""])
    return "\n".join(lines) + "\n"


def apply_rewrites(consumer_map: dict, narrow_crate: str = "") -> dict:
    """Run migration via touring CLI."""
    applied: list[dict] = []
    errors: list[dict] = []
    details = consumer_map.get("details", {})
    for source_crate, crates in details.items():
        old_mod = source_crate.replace("-", "_")
        new_mod = MODULE_MAP[old_mod]
        for crate_name, files in crates.items():
            if narrow_crate and crate_name != narrow_crate:
                continue
            for f in files:
                abs_file = _ROOT / f["file"]
                if not abs_file.exists():
                    continue
                try:
                    proc = subprocess.run(
                        ["touring", "ast", "grep", str(abs_file), old_mod,
                         "--rewrite", new_mod, "--lang", "rust"],
                        cwd=_ROOT, capture_output=True, timeout=60,
                    )
                    (applied if proc.returncode == 0 else errors).append({
                        "file": f["file"], "old_mod": old_mod, "new_mod": new_mod,
                        "exit_code": proc.returncode,
                    })
                except subprocess.TimeoutExpired:
                    errors.append({"file": f["file"], "status": "TIMEOUT"})
                except FileNotFoundError:
                    errors.append({"file": f["file"], "status": "MISSING_TOOL"})
                    return {"applied": applied, "errors": errors}
    return {"applied": applied, "errors": errors}


def update_cargo_files(consumer_map: dict, narrow_crate: str = "") -> dict:
    """Update Cargo.toml declarations via regex."""
    updates: list[dict] = []
    details = consumer_map.get("details", {})
    consumer_crates: set[str] = set()
    for _src, crates in details.items():
        for crate_name in crates:
            if not narrow_crate or crate_name == narrow_crate:
                consumer_crates.add(crate_name)
    for crate_name in consumer_crates:
        cargo = _ROOT / "crates" / crate_name / "Cargo.toml"
        if not cargo.exists():
            continue
        original = cargo.read_text(encoding="utf-8", errors="replace")
        modified = original
        for old_crate, new_crate in CRATE_MAP.items():
            modified = re.sub(
                rf"^(\s*){re.escape(old_crate)}(\s*=)",
                rf"\g<1>{new_crate}\g<2>",
                modified, flags=re.MULTILINE,
            )
        if modified != original:
            cargo.write_text(modified, encoding="utf-8")
            updates.append({"crate": crate_name,
                             "cargo_toml": str(cargo.relative_to(_ROOT))})
    return {"cargo_updates": updates}


def run(args: argparse.Namespace) -> dict:
    """Render script + optionally apply."""
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    consumer_map = load_consumer_map(args.map_file)
    if not consumer_map:
        return {"status": "MAP_NOT_FOUND"}
    script_path = _STAGING_DIR / "w4-migration-script.sh"
    script_path.write_text(render_migration_script(consumer_map), encoding="utf-8")
    script_path.chmod(0o755)
    applied_summary: dict = {}
    if args.apply:
        applied_summary = apply_rewrites(consumer_map, args.crate)
        applied_summary.update(update_cargo_files(consumer_map, args.crate))
    report = {
        "script": "w4_consumer_migrate",
        "wave": "W4", "subtask_refs": ["W4.8"],
        "timestamp": datetime.now(UTC).isoformat(),
        "apply": args.apply, "narrow_crate": args.crate,
        "script_path": str(script_path.relative_to(_ROOT)),
        "totals": {
            "source_crates": len(consumer_map.get("details", {})),
            "applied": len(applied_summary.get("applied", [])) if args.apply else 0,
            "errors": len(applied_summary.get("errors", [])) if args.apply else 0,
        },
        "status": "OK",
    }
    json_path = args.output_dir / "w4-migration-applied.json"
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
        sys.stdout.write(json.dumps({
            "status": result["status"], "apply": result.get("apply", False),
            "totals": result.get("totals", {}),
            "script_path": result.get("script_path"),
            "json_path": result.get("json_path"),
        }, indent=2, ensure_ascii=False) + "\n")
        return 0 if result["status"] == "OK" else 1
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
