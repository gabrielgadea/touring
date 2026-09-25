#!/usr/bin/env python3
"""install_claude_hooks.py — S49: manage the global Claude Code hooks section.

Modes:
  --check        Exit 1 if settings.json hooks section drifted from hooks.settings.json.
  --apply        Merge hooks.settings.json into settings.json (preserves all other sections).
  --propose OUT  Write the proposed merged settings.json to OUT (never touches the real file).
  --lint-only    Lint hooks.settings.json against known anti-patterns; exit 1 on findings.
  --generate     Extract current settings.json hooks section → hooks.settings.json (bootstrap).

Lint rules (exit 1 = RED, intentional — the current state has violations):
  L1: `if` field contains `|`, `&&`, `||`, or list separators → single-permission per entry.
  L2: `if` field used in event outside {PreToolUse, PostToolUse, PostToolUseFailure,
       PermissionRequest, PermissionDenied} — those are the only events that support `if`.
  L3: script from a project directory (e.g. analise/) registered in the global settings.
  L4: hook entry with no `timeout` declared.

The current state is EXPECTED to fail L1 (the `if` with `|` bug, S31 in the plan) and L3
(hook_medicao.py, S44). These are the RED tests that the subsequent phases will fix.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Any

HOME = Path.home()
SETTINGS_PATH = HOME / ".claude" / "settings.json"
REPO_ROOT = Path(__file__).resolve().parent.parent
HOOKS_SETTINGS_PATH = REPO_ROOT / "client" / "claude" / "hooks.settings.json"

# Events that support the `if` field (Claude Code docs, Context7 query 25/09/2026).
IF_SUPPORTED_EVENTS = {
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "PermissionDenied",
}

# Patterns in `if` values that indicate multiple-permission anti-pattern.
IF_MULTIPATTERN = re.compile(r'[|]|&&|\|\|')

# Project directories that should NOT be in the global settings.
PROJECT_DIRS = [
    "/home/gabrielgadea/projects/analise/",
    "analise/",
    "projects/analise/",
]


def load_json(path: Path) -> dict[str, Any]:
    with open(path) as f:
        return json.load(f)


def save_json(path: Path, data: dict[str, Any]) -> None:
    with open(path, "w") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
        f.write("\n")


def lint_hooks(hooks: dict[str, list[dict]]) -> list[dict[str, str]]:
    """Return list of lint findings. Empty list = clean."""
    findings: list[dict[str, str]] = []

    for event, entries in hooks.items():
        if not isinstance(entries, list):
            continue
        for entry_idx, entry in enumerate(entries):
            if_val = entry.get("if", "")
            hook_list = entry.get("hooks", [])

            # L1: `if` with alternation operators (only valid in `matcher` field).
            if if_val and IF_MULTIPATTERN.search(if_val):
                findings.append({
                    "rule": "L1",
                    "event": event,
                    "entry_idx": str(entry_idx),
                    "detail": f"`if` contains alternation/logic operator: {if_val!r}. "
                              f"Use separate hook entries instead of `|`/`&&`/`||`.",
                })

            # L2: `if` used in event that does not support it.
            if if_val and event not in IF_SUPPORTED_EVENTS:
                findings.append({
                    "rule": "L2",
                    "event": event,
                    "entry_idx": str(entry_idx),
                    "detail": f"`if` field in event {event!r} which does not support it "
                              f"(supported: {sorted(IF_SUPPORTED_EVENTS)}). Hook will never fire.",
                })

            # L3 + L4: per-hook checks.
            for hook_idx, hook in enumerate(hook_list):
                cmd = hook.get("command", "")
                timeout = hook.get("timeout")

                # L3: project-specific script registered globally.
                for proj_dir in PROJECT_DIRS:
                    if proj_dir in cmd:
                        findings.append({
                            "rule": "L3",
                            "event": event,
                            "entry_idx": str(entry_idx),
                            "hook_idx": str(hook_idx),
                            "detail": f"Project-specific script registered globally: {cmd!r}. "
                                      f"Register in the project's .claude/settings.local.json instead.",
                            "command": cmd,
                        })

                # L4: no timeout declared.
                if timeout is None:
                    findings.append({
                        "rule": "L4",
                        "event": event,
                        "entry_idx": str(entry_idx),
                        "hook_idx": str(hook_idx),
                        "detail": f"Hook has no timeout: {cmd!r}. "
                                  f"Default is 10 min which can stall sessions.",
                        "command": cmd,
                    })

    return findings


def cmd_lint_only(args: argparse.Namespace) -> int:
    """Lint hooks.settings.json; exit 1 on findings."""
    source = HOOKS_SETTINGS_PATH if HOOKS_SETTINGS_PATH.exists() else SETTINGS_PATH
    data = load_json(source)
    hooks = data.get("hooks", data) if "hooks" in data else data
    findings = lint_hooks(hooks)

    if findings:
        print(f"LINT: {len(findings)} finding(s) in {source}:", file=sys.stderr)
        for f in findings:
            print(f"  [{f['rule']}] event={f['event']} entry={f.get('entry_idx','?')}: {f['detail']}", file=sys.stderr)
        return 1

    print(f"LINT: clean ({source})")
    return 0


def cmd_check(args: argparse.Namespace) -> int:
    """Check for drift between hooks.settings.json and settings.json."""
    if not HOOKS_SETTINGS_PATH.exists():
        print("ERROR: hooks.settings.json not found — run --generate first", file=sys.stderr)
        return 1

    source_hooks = load_json(HOOKS_SETTINGS_PATH)
    live_settings = load_json(SETTINGS_PATH)
    live_hooks = live_settings.get("hooks", {})

    source_str = json.dumps(source_hooks, sort_keys=True)
    live_str = json.dumps(live_hooks, sort_keys=True)

    if source_str != live_str:
        print("DRIFT DETECTED: settings.json hooks section differs from hooks.settings.json", file=sys.stderr)
        return 1

    print("OK: no drift")
    return 0


def cmd_apply(args: argparse.Namespace) -> int:
    """Merge hooks.settings.json into settings.json."""
    if not HOOKS_SETTINGS_PATH.exists():
        print("ERROR: hooks.settings.json not found — run --generate first", file=sys.stderr)
        return 1

    source_hooks = load_json(HOOKS_SETTINGS_PATH)
    live_settings = load_json(SETTINGS_PATH)
    live_settings["hooks"] = source_hooks
    save_json(SETTINGS_PATH, live_settings)
    print(f"APPLIED: hooks section written to {SETTINGS_PATH}")
    return 0


def cmd_propose(args: argparse.Namespace) -> int:
    """Write proposed merged settings.json to output path (dry run)."""
    if not HOOKS_SETTINGS_PATH.exists():
        print("ERROR: hooks.settings.json not found — run --generate first", file=sys.stderr)
        return 1

    source_hooks = load_json(HOOKS_SETTINGS_PATH)
    live_settings = load_json(SETTINGS_PATH)
    live_settings["hooks"] = source_hooks
    out_path = Path(args.propose)
    save_json(out_path, live_settings)
    print(f"PROPOSED: merged settings written to {out_path} (real file untouched)")
    return 0


def cmd_generate(args: argparse.Namespace) -> int:
    """Extract hooks section from settings.json → hooks.settings.json (bootstrap)."""
    live_settings = load_json(SETTINGS_PATH)
    hooks = live_settings.get("hooks", {})
    HOOKS_SETTINGS_PATH.parent.mkdir(parents=True, exist_ok=True)
    save_json(HOOKS_SETTINGS_PATH, hooks)
    print(f"GENERATED: {HOOKS_SETTINGS_PATH} ({len(hooks)} events)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Claude hooks section manager (S49)")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", action="store_true", help="Exit 1 if drift detected")
    group.add_argument("--apply", action="store_true", help="Merge into settings.json")
    group.add_argument("--propose", metavar="OUT", help="Write proposed settings to OUT (dry run)")
    group.add_argument("--lint-only", action="store_true", help="Lint rules only; exit 1 on findings")
    group.add_argument("--generate", action="store_true", help="Bootstrap hooks.settings.json from settings.json")
    args = parser.parse_args()

    if args.check:
        return cmd_check(args)
    if args.apply:
        return cmd_apply(args)
    if args.propose:
        return cmd_propose(args)
    if args.lint_only:
        return cmd_lint_only(args)
    if args.generate:
        return cmd_generate(args)
    return 1


if __name__ == "__main__":
    sys.exit(main())
