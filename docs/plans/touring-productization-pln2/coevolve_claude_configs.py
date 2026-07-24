#!/usr/bin/env python3
"""coevolve_claude_configs.py — F4-5: re-reference ~/.claude configs to the new
canonical source root (~/projects/touring).

Deterministic rewrite of OPERATIONAL references only (paths that get resolved or
executed). Historical data is never touched: memory/, taco-forge sessions
(deactivated 2026-07-02), cah-diagnostic runs, session reports.

Special-cased OUT of the generic rewrite (they must keep BOTH roots during the
transition, edited separately): tools/disk-watch.sh, tools/safe-clean.sh.

Usage:
  coevolve_claude_configs.py            # dry-run report (default)
  coevolve_claude_configs.py --apply    # rewrite + central tar.gz backup
"""
from __future__ import annotations

import argparse
import json
import sys
import tarfile
import time
from pathlib import Path

HOME = Path.home()
CLAUDE = HOME / ".claude"
OLD = "/home/gabrielgadea/.claude/rust"
NEW = "/home/gabrielgadea/projects/touring"
REWRITES = [
    (f"{HOME}/.claude/rust", f"{HOME}/projects/touring"),
    ("~/.claude/rust", "~/projects/touring"),
    ("$HOME/.claude/rust", "$HOME/projects/touring"),
    ("${HOME}/.claude/rust", "${HOME}/projects/touring"),
]
BACKUP_DIR = Path(__file__).resolve().parent / "backups"

# Explicit operational core (curated from the 2026-07-24 grep map).
CORE = [
    "CLAUDE.md",
    "settings.json",
    "rules/disk-hygiene.md",
    "rules/touring-4-pillars.md",
    "agents/_shared-touring-base.md",
    "agents/touring-architect.md",
    "agents/touring-engineer.md",
    "agents/touring-scriber.md",
    "commands/checkpoint.md",
    "commands/plan.md",
    "commands/.gemini/touring.gemini.md",
    "commands/.jules/touring.jules.md",
    "commands/.serena/touring.serena.md",
    "commands/.specify/touring.specify.md",
    "hooks/ensure_daemon.sh",
    "hooks/touring-quality-block-all.sh",
    "hooks/touring-quality-f2-5-block.sh",
    "tools/check-doc-coverage.sh",
    "tools/holon/benchmarks/bench_d34.sh",
    "tools/holon/clients/py/README.md",
    "tools/holon/clients/py/scripts/smoke_e2e.sh",
    "tools/holon/tests/run_full_audit.sh",
]
# Never rewrite (historical data / deactivated / dual-root special cases).
EXCLUDE_PARTS = ("memory/", "taco-forge/", "cah-diagnostic/",
                 "tools/disk-watch.sh", "tools/safe-clean.sh")


def targets():
    seen = []
    for rel in CORE:
        p = CLAUDE / rel
        if p.is_file():
            seen.append(p)
    for p in sorted((CLAUDE / "skills").rglob("*")):
        if p.is_file() and not any(x in str(p) for x in EXCLUDE_PARTS):
            try:
                if ".claude/rust" in p.read_text(errors="ignore"):
                    seen.append(p)
            except Exception:  # noqa: BLE001 — unreadable = skip
                pass
    return seen


def plan(files):
    report = []
    for p in files:
        text = p.read_text(errors="ignore")
        n = sum(text.count(old) for old, _ in REWRITES)
        if n:
            report.append({"file": str(p.relative_to(CLAUDE)), "hits": n})
    return report


def apply(files):
    BACKUP_DIR.mkdir(parents=True, exist_ok=True)
    tag = time.strftime("%Y%m%d-%H%M%S")
    tar_path = BACKUP_DIR / f"coevolve-{tag}.tar.gz"
    changed = []
    with tarfile.open(tar_path, "w:gz") as tar:
        for p in files:
            text = p.read_text(errors="ignore")
            new_text = text
            for old, new in REWRITES:
                new_text = new_text.replace(old, new)
            if new_text != text:
                tar.add(p, arcname=str(p.relative_to(HOME)))
                p.write_text(new_text)
                changed.append(str(p.relative_to(CLAUDE)))
    return {"backup": str(tar_path), "changed": changed}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--apply", action="store_true",
                    help="rewrite in place (default: dry-run report)")
    args = ap.parse_args()
    files = targets()
    report = plan(files)
    if not args.apply:
        print(json.dumps({"mode": "dry-run", "files_with_hits": len(report),
                          "total_hits": sum(r["hits"] for r in report),
                          "detail": report}, indent=2))
        return 0
    result = apply(files)
    ok = json.loads(Path(CLAUDE / "settings.json").read_text()) is not None
    print(json.dumps({"mode": "apply", "changed": len(result["changed"]),
                      "backup": result["backup"],
                      "settings_json_valid": bool(ok),
                      "detail": result["changed"]}, indent=2))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
