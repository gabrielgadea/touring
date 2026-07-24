#!/usr/bin/env python3
"""
patch_plan.py — Atomic patch tool for touring-v19-improvements plan.

When the plan needs changes, edit generate_plan.py and run:
    python patch_plan.py --dry-run   # preview changes
    python patch_plan.py             # apply atomically

All files are written together or not at all (no partial updates).
Usage:
    python patch_plan.py [--dry-run] [--phase N]
"""
import argparse
import hashlib
import sys
import tempfile
from pathlib import Path

# Re-use generate_plan logic
sys.path.insert(0, str(Path(__file__).parent))
from generate_plan import (
    PHASES,
    generate_phase_md,
    generate_validate_py,
    generate_readme,
    generate_audit_script,
    _phase_filename,
    OUTPUT_DIR,
)


def _hash(path: Path) -> str:
    return hashlib.md5(path.read_bytes()).hexdigest() if path.exists() else ""


def patch(dry_run: bool = False, phase_filter: int | None = None) -> int:
    """Compute diff and apply atomically."""
    phases = PHASES if phase_filter is None else [p for p in PHASES if p["phase"] == phase_filter]
    if not phases and phase_filter is not None:
        print(f"Phase {phase_filter} not found.")
        return 1

    # Compute all new content first (no writes yet)
    pending: list[tuple[Path, str]] = []
    for phase in phases:
        md_path = OUTPUT_DIR / _phase_filename(phase)
        val_path = OUTPUT_DIR / f"validate_phase{phase['phase']:02d}.py"
        pending.append((md_path, generate_phase_md(phase)))
        pending.append((val_path, generate_validate_py(phase)))

    if phase_filter is None:
        readme_path = OUTPUT_DIR / "README.md"
        audit_path = OUTPUT_DIR / "audit_v19.py"
        pending.append((readme_path, generate_readme()))
        pending.append((audit_path, generate_audit_script()))

    # Identify changed files
    changed = [(p, content) for p, content in pending if _hash(p) != hashlib.md5(content.encode()).hexdigest()]

    if not changed:
        print("No changes detected.")
        return 0

    print(f"{'DRY RUN: ' if dry_run else ''}Changes to apply ({len(changed)} files):")
    for path, _ in changed:
        print(f"  → {path.name}")

    if dry_run:
        print("\nDry run complete. Run without --dry-run to apply.")
        return 0

    # Write to temp files first, then rename atomically
    tmp_files: list[tuple[Path, Path]] = []
    try:
        for path, content in changed:
            _, tmp = tempfile.mkstemp(dir=path.parent, suffix=".tmp")
            tmp_path = Path(tmp)
            tmp_path.write_text(content, encoding="utf-8")
            tmp_files.append((tmp_path, path))

        # All temps written successfully → rename atomically
        for tmp_path, final_path in tmp_files:
            tmp_path.rename(final_path)

        print(f"\nApplied {len(changed)} changes atomically.")
        return 0
    except Exception as exc:
        print(f"\nERROR: {exc}")
        # Cleanup temps
        for tmp_path, _ in tmp_files:
            try:
                tmp_path.unlink()
            except FileNotFoundError:
                pass
        return 1


def main() -> None:
    parser = argparse.ArgumentParser(description="Atomic patch tool for touring-v19 plan")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--phase", type=int)
    args = parser.parse_args()
    sys.exit(patch(dry_run=args.dry_run, phase_filter=args.phase))


if __name__ == "__main__":
    main()
