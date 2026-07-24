#!/usr/bin/env python3
"""relocate_session_docs.py — declutter the docs/ root by relocating dated session
artifacts (docs/2026-{04,05}-*.md) into docs/internal/sessions/, leaving a redirect
stub for any file still referenced elsewhere under ~/.claude (zero link-rot).

Closes Master Plan A14 / D.W1.P2.T2 (G-8, 2026-06-13). Deterministic, stdlib-only,
idempotent. Canonical 2026-06 docs (diagnostic/masterplan/whitepaper/verification)
and undated RFC/CONSTITUTION files are NOT touched — only 2026-04/05 dated artifacts.

Usage:
    docs/relocate_session_docs.py             dry-run (default): print the plan
    docs/relocate_session_docs.py --apply     execute the moves (+ stubs)
    docs/relocate_session_docs.py --json      machine-readable summary
    docs/relocate_session_docs.py --no-stub   move even referenced docs without a stub
"""
from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent  # ~/.claude/rust
DOCS = ROOT / "docs"
DEST = DOCS / "internal" / "sessions"
CLAUDE = Path.home() / ".claude"

STUB = (
    "# Moved\n\n"
    "This document was relocated to declutter the docs/ root "
    "(Master Plan A14 / G-8, 2026-06-13).\n\n"
    "→ **[`docs/internal/sessions/{name}`](internal/sessions/{name})**\n"
)


def targets() -> list[Path]:
    """Dated session/iteration/wave artifacts at the docs/ root (regular files only)."""
    return sorted(
        p for p in DOCS.glob("2026-0[45]-*.md") if p.is_file() and not p.is_symlink()
    )


def referenced_names() -> set[str] | None:
    """Set of dated-doc basenames referenced anywhere under ~/.claude (one grep pass).

    Returns None if the scan could not run, so callers fail safe (treat everything as
    referenced and keep a stub rather than risk link-rot)."""
    try:
        out = subprocess.run(
            [
                "grep", "-rhoE",
                "--include=*.md", "--include=*.py", "--include=*.sh", "--include=*.toml",
                r"2026-0[45]-[A-Za-z0-9._-]+\.md", str(CLAUDE),
            ],
            capture_output=True, text=True, timeout=180,
        )
    except Exception:
        return None
    return set(out.stdout.split())


def main() -> int:
    ap = argparse.ArgumentParser(description="Relocate dated session docs out of docs/ root.")
    ap.add_argument("--apply", action="store_true", help="execute (default is dry-run)")
    ap.add_argument("--json", action="store_true", help="machine-readable summary")
    ap.add_argument("--no-stub", action="store_true", help="do not leave redirect stubs")
    args = ap.parse_args()

    DEST.mkdir(parents=True, exist_ok=True)
    refset = referenced_names()
    scan_ok = refset is not None

    moved: list[str] = []
    stubbed: list[str] = []
    skipped: list[str] = []
    for src in targets():
        name = src.name
        dst = DEST / name
        if dst.exists():
            skipped.append(name)  # already relocated (idempotent)
            continue
        # Conservative: if the scan failed, treat as referenced (stub) to avoid link-rot.
        referenced = (not args.no_stub) and ((not scan_ok) or (name in refset))
        if args.apply:
            dst.write_bytes(src.read_bytes())
            src.unlink()
            if referenced:
                src.write_text(STUB.format(name=name))
        (stubbed if referenced else moved).append(name)

    summary = {
        "dest": str(DEST.relative_to(ROOT)),
        "scan_ok": scan_ok,
        "moved_no_stub": len(moved),
        "moved_with_stub": len(stubbed),
        "already_relocated": len(skipped),
        "applied": args.apply,
    }
    if args.json:
        print(json.dumps(summary, indent=2))
        return 0

    mode = "APPLIED" if args.apply else "DRY-RUN (pass --apply to execute)"
    print(f"relocate_session_docs [{mode}] -> {summary['dest']}")
    print(f"  moved without stub (unreferenced): {len(moved)}")
    print(f"  moved with redirect stub (referenced): {len(stubbed)}")
    print(f"  already relocated (skipped): {len(skipped)}")
    if not scan_ok:
        print("  NOTE: reference scan failed; everything kept a stub (fail-safe).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
