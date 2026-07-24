#!/usr/bin/env python3
"""
patch_plan.py — Atomic Modification of Plan Files.

Applies modifications to phase markdown files atomically with backup.
If any operation fails, all changes are rolled back.

Usage:
    python patch_plan.py --phase phase-01 --set status=in_progress
    python patch_plan.py --phase phase-01 --set estimated_effort="8h"
    python patch_plan.py --list-fields phase-01
    python patch_plan.py --dry-run --phase phase-01 --set status=completed
"""
from __future__ import annotations

import argparse
import shutil
import sys
from datetime import datetime
from pathlib import Path

try:
    import yaml
except ImportError:
    print("PyYAML required: pip install pyyaml", file=sys.stderr)
    sys.exit(1)

PLAN_DIR = Path(__file__).parent
BACKUP_DIR = PLAN_DIR / ".patch_backups"


def find_phase_file(phase_key: str) -> Path | None:
    """Find the markdown file for a given phase key."""
    # Try exact match first
    for f in PLAN_DIR.glob("*.md"):
        if f.name.startswith(phase_key):
            return f
    # Try partial match
    for f in PLAN_DIR.glob("phase-*.md"):
        if phase_key in f.name:
            return f
    return None


def parse_frontmatter(content: str) -> tuple[dict, str]:
    """Parse YAML frontmatter from markdown content.

    Returns (frontmatter_dict, body_after_frontmatter).
    """
    if not content.startswith("---"):
        return {}, content

    parts = content.split("---", 2)
    if len(parts) < 3:
        return {}, content

    fm_text = parts[1]
    body = "---".join(parts[2:])  # rejoin if body has --- separators

    try:
        fm = yaml.safe_load(fm_text) or {}
    except yaml.YAMLError:
        return {}, content

    return fm, body


def render_frontmatter(fm: dict) -> str:
    """Render frontmatter dict back to YAML string with --- delimiters."""
    return "---\n" + yaml.dump(fm, default_flow_style=False, sort_keys=False) + "---"


def backup_file(path: Path) -> Path:
    """Create a timestamped backup of a file."""
    BACKUP_DIR.mkdir(exist_ok=True)
    ts = datetime.now().strftime("%Y%m%d_%H%M%S")
    backup = BACKUP_DIR / f"{path.stem}_{ts}{path.suffix}"
    shutil.copy2(path, backup)
    return backup


def patch_frontmatter(phase_file: Path, field: str, value: str, *, dry_run: bool = False) -> bool:
    """Update a single field in the frontmatter of a phase file.

    Returns True on success.
    """
    content = phase_file.read_text(encoding="utf-8")
    fm, body = parse_frontmatter(content)

    if not fm:
        print(f"  ERROR: No valid frontmatter in {phase_file.name}", file=sys.stderr)
        return False

    old_value = fm.get(field, "<not set>")

    # Auto-convert value types
    parsed_value: str | int | float | bool | list[str] = value
    if value.isdigit():
        parsed_value = int(value)
    elif value.replace(".", "", 1).isdigit():
        parsed_value = float(value)
    elif value.lower() in ("true", "false"):
        parsed_value = value.lower() == "true"
    elif value.startswith("[") and value.endswith("]"):
        # Simple list parsing: [a, b, c]
        parsed_value = [v.strip().strip("'\"") for v in value[1:-1].split(",") if v.strip()]

    if dry_run:
        print(f"  [DRY RUN] {phase_file.name}: {field}: {old_value} -> {parsed_value}")
        return True

    # Backup before modifying
    backup = backup_file(phase_file)
    print(f"  Backup: {backup.name}")

    fm[field] = parsed_value
    new_content = render_frontmatter(fm) + body
    phase_file.write_text(new_content, encoding="utf-8")
    print(f"  Patched: {phase_file.name}: {field}: {old_value} -> {parsed_value}")
    return True


def list_fields(phase_file: Path) -> None:
    """List all frontmatter fields and their current values."""
    content = phase_file.read_text(encoding="utf-8")
    fm, _ = parse_frontmatter(content)
    if not fm:
        print(f"  No frontmatter found in {phase_file.name}")
        return

    print(f"  Frontmatter fields in {phase_file.name}:")
    for key, val in fm.items():
        val_str = str(val)
        if len(val_str) > 80:
            val_str = val_str[:77] + "..."
        print(f"    {key}: {val_str}")


def atomic_patch(
    patches: list[tuple[Path, str, str]],
    *,
    dry_run: bool = False,
) -> bool:
    """Apply multiple patches atomically.

    If any patch fails, all are rolled back.
    """
    backups: list[tuple[Path, Path]] = []

    try:
        for file_path, field, value in patches:
            if not dry_run:
                backup = backup_file(file_path)
                backups.append((file_path, backup))

            content = file_path.read_text(encoding="utf-8")
            fm, body = parse_frontmatter(content)
            if not fm:
                msg = f"No frontmatter in {file_path.name}"
                raise ValueError(msg)

            old_val = fm.get(field, "<not set>")
            fm[field] = value
            new_content = render_frontmatter(fm) + body

            if dry_run:
                print(f"  [DRY RUN] {file_path.name}: {field}: {old_val} -> {value}")
            else:
                file_path.write_text(new_content, encoding="utf-8")
                print(f"  Patched: {file_path.name}: {field}: {old_val} -> {value}")

        return True

    except Exception as e:
        print(f"  ERROR: {e}. Rolling back...", file=sys.stderr)
        for file_path, backup in reversed(backups):
            shutil.copy2(backup, file_path)
            print(f"  Restored: {file_path.name} from {backup.name}")
        return False


def main() -> None:
    parser = argparse.ArgumentParser(
        description="patch_plan.py — Atomic Plan Modifier"
    )
    parser.add_argument("--phase", type=str, help="Phase key (e.g. phase-01)")
    parser.add_argument("--set", type=str, help="Field=Value to set in frontmatter")
    parser.add_argument("--list-fields", type=str, metavar="PHASE", help="List frontmatter fields for a phase")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    if args.list_fields:
        phase_file = find_phase_file(args.list_fields)
        if not phase_file:
            print(f"ERROR: Phase file not found for '{args.list_fields}'", file=sys.stderr)
            sys.exit(1)
        list_fields(phase_file)
        return

    if not args.phase or not args.set:
        parser.print_help()
        return

    phase_file = find_phase_file(args.phase)
    if not phase_file:
        print(f"ERROR: Phase file not found for '{args.phase}'", file=sys.stderr)
        sys.exit(1)

    # Parse field=value
    if "=" not in args.set:
        print("ERROR: --set must be FIELD=VALUE format", file=sys.stderr)
        sys.exit(1)

    field, value = args.set.split("=", 1)
    success = patch_frontmatter(phase_file, field.strip(), value.strip(), dry_run=args.dry_run)
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
