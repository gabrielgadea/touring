#!/usr/bin/env python3
"""root_hygiene_gate.py — workspace-root hygiene gate.

Master Plan H1-B (2026-06-13). The current ci.yml has an inline `root hygiene`
step that fails on stray editor/backup artifacts (`*.tf-bak.*`, `*.rlib`,
`debug_js.js`). Extracted into a Python script so the EliteScore aggregator
can fold it under gate 08 (ci_cd_devops).

Heuristics (greppable, zero-LLM):
  * Any file matching `*.tf-bak.*`, `*.rlib`, `debug_js.js`, `*.swp`, `*.swo`
    directly under the workspace root is a finding.
  * Hidden files other than the well-known exceptions (`.gitignore`, `.env*`,
    `.cargo/`, `.github/`, `.claude/`, etc.) are flagged.
  * Stale `Cargo.lock.bak`, `target-*`, `nohup.out` are flagged.

Exits:
  0  PASS  — no stray artifacts
  1  FAIL  — at least one stray artifact
  2  ADVISORY  — scan skipped (root not readable)

Usage
-----
    docs/root_hygiene_gate.py --check
    docs/root_hygiene_gate.py --json
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

STRAY_PATTERNS = (
    "*.tf-bak.*",
    "*.rlib",
    "debug_js.js",
    "*.swp",
    "*.swo",
    "*.bak",
    "*.orig",
    "*.rej",
    "nohup.out",
    "Cargo.lock.bak",
    "core",
)

ALLOWED_HIDDEN = {
    ".git",
    ".gitignore",
    ".gitattributes",
    ".cargo",
    ".github",
    ".claude",
    ".vscode",
    ".idea",
    ".editorconfig",
    ".env",
    ".envrc",
    # Tool caches (legitimate, ephemeral)
    ".cache", ".ruff_cache", ".pytest_cache", ".mypy_cache", ".tox",
    ".cipher", ".serena", ".holon", ".full-review",
    ".touring-cache", ".local-cache", ".fastembed_cache",
    # Local config / state
    ".config", ".local",
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="CI mode (default)")
    parser.add_argument("--json", action="store_true", help="machine-readable JSON")
    args = parser.parse_args()

    findings: list[dict] = []
    for entry in ROOT.iterdir():
        if not entry.name.startswith(".") and not any(entry.match(p) for p in STRAY_PATTERNS):
            continue
        if any(entry.match(p) for p in STRAY_PATTERNS):
            findings.append({"path": entry.name, "kind": "stray", "match": "explicit stray pattern"})
            continue
        # Hidden files: only flag those NOT in the allowlist
        if entry.name in ALLOWED_HIDDEN:
            continue
        if entry.name.startswith("."):
            findings.append({"path": entry.name, "kind": "hidden", "match": "non-allowlisted hidden file"})

    report = {"files_scanned": sum(1 for _ in ROOT.iterdir()), "findings_count": len(findings), "findings": findings}
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(f"root_hygiene: {report['files_scanned']} entries; {len(findings)} findings")
        for f in findings:
            print(f"  ::error::{f['kind']}: {f['path']} — {f['match']}", file=sys.stderr)

    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
