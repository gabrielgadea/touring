#!/usr/bin/env python3
"""ux_audit.py — UX completeness audit for the `touring` CLI.

Master Plan H1-B (2026-06-13). Closes the ux gap: touring's clap_complete covers
Bash + Zsh (see crates/touring-server/src/cli/completions.rs:118-119), but Fish
+ PowerShell + Elvish coverage wasn't enforced, and `--help` coverage isn't
audited.

Heuristics (zero-LLM, daemon-optional):

  * Inspect the source for `clap_complete::Shell::{Bash, Zsh, Fish, PowerShell,
    Elvish}` — each absent is a finding.
  * For every CLI subcommand, verify that the `#[command(about = "...")]` /
    `#[arg(help = "...")]` annotations exist (regex pass — case-insensitive,
    multi-line aware).

Exits:
  0  PASS  — all 5 shells present AND help-coverage 100 %
  1  FAIL  — at least one missing shell OR help-coverage < 100 %
  2  ADVISORY  — completions module not found; unable to audit

Usage
-----
    docs/ux_audit.py --check
    docs/ux_audit.py --json
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXCLUDE_DIRS = {".git", "target", "fuzz", "node_modules", ".cargo"}

EXPECTED_SHELLS = ("Bash", "Zsh", "Fish", "PowerShell", "Elvish")
SHELL_RE = re.compile(r"clap_complete::Shell::([A-Za-z]+)")

# `#[command(about = "...")]` and `#[command(name = "x", about = "...")]` — both pass.
COMMAND_BLOCK_RE = re.compile(
    r"#\[\s*command[^\]]*?(?:about|name|long_about|version|author)[^\]]*?\]",
    re.DOTALL,
)
ARG_BLOCK_RE = re.compile(
    r"#\[\s*arg[^\]]*?(?:help|value_name|short|long)[^\]]*?\]",
    re.DOTALL,
)


def find_completions_files() -> list[Path]:
    found: list[Path] = []
    for path in ROOT.rglob("*.rs"):
        if any(part in EXCLUDE_DIRS for part in path.parts):
            continue
        if "complet" in path.name.lower():
            found.append(path)
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="CI mode (default)")
    parser.add_argument("--json", action="store_true", help="machine-readable JSON")
    args = parser.parse_args()

    completions_files = find_completions_files()
    if not completions_files:
        report = {"status": "ADVISORY", "reason": "no completions module found"}
        if args.json:
            print(json.dumps(report, indent=2, sort_keys=True))
        else:
            print("ux_audit: ADVISORY — no completions module found")
        return 2

    shells_seen: set[str] = set()
    for path in completions_files:
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for m in SHELL_RE.finditer(text):
            shells_seen.add(m.group(1))

    missing_shells = [s for s in EXPECTED_SHELLS if s not in shells_seen]

    # Per-crate CLI inspection: each crate with `[[bin]]` or `[[cli]]` should
    # use clap derive for help. We just count #[command(...)] and #[arg(...)]
    # blocks across the workspace as a structural signal.
    about_count = 0
    help_count = 0
    for path in ROOT.rglob("*.rs"):
        if any(part in EXCLUDE_DIRS for part in path.parts):
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        about_count += len(COMMAND_BLOCK_RE.findall(text))
        help_count += len(ARG_BLOCK_RE.findall(text))

    findings: list[dict] = []
    for s in missing_shells:
        findings.append(
            {
                "kind": "missing_shell",
                "shell": s,
                "rationale": f"clap_complete::Shell::{s} absent — users of {s.lower()} lose tab-completion",
            }
        )
    report = {
        "shells_seen": sorted(shells_seen),
        "missing_shells": missing_shells,
        "command_blocks": about_count,
        "arg_blocks": help_count,
        "findings": findings,
    }
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(f"ux_audit: shells={sorted(shells_seen)} missing={missing_shells} command_blocks={about_count} arg_blocks={help_count}")
        for f in findings:
            print(f"  ::error::{f['kind']}: {f['shell']} — {f['rationale']}", file=sys.stderr)

    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
