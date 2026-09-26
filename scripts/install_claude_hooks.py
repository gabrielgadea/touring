#!/usr/bin/env python3
"""install_claude_hooks.py — S49: the global Claude Code hooks section, from a versioned source.

Every mode takes --settings PATH and --source PATH (defaults: ~/.claude/settings.json and
client/claude/hooks.settings.json), so tests and dry runs never touch the real files.

  --check        exit 1 when the settings' hooks section differs from the source
  --apply        write the source into the settings. Refused when the source ADDS a lint
                 finding over the live file (pre-existing debt has its own phase — S31);
                 atomic (.next + rename), the file mode preserved, nothing else touched
  --propose OUT  write the merged settings to OUT; the real file is never touched
  --lint-only    lint the source; exit 1 on findings (--json for machine output)
  --generate     bootstrap the source from the live settings' hooks section

Lint — on each HOOK, where the `if` field lives (the first version read it on the matcher
GROUP and never fired, 26/09/2026):
  L1  `if` holds more than one permission rule (`|`, `&&`, a list): the hook never runs
  L2  `if` on an event outside the five tool events: the hook never runs (Claude Code docs,
      hooks guide — Context7 26/09/2026)
  L3  a project's script registered globally by path: every other project pays a process
  L4  no `timeout`: Claude Code's default is 600 s
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections.abc import Iterator
from pathlib import Path
from typing import Any

HOME = Path.home()
SETTINGS_PATH = HOME / ".claude" / "settings.json"
REPO_ROOT = Path(__file__).resolve().parent.parent
HOOKS_SETTINGS_PATH = REPO_ROOT / "client" / "claude" / "hooks.settings.json"

IF_SUPPORTED_EVENTS = frozenset(
    {"PreToolUse", "PostToolUse", "PostToolUseFailure", "PermissionRequest", "PermissionDenied"}
)
IF_MULTIPATTERN = re.compile(r"\||&&")
#: A command that runs a script living inside ONE project, by path.
PROJECT_SCRIPT = re.compile(r"(\$HOME|~|/home/[^/\s]+)/projects/[^/\s]+/")


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def dump(data: Any) -> str:
    """The settings file's own format (it reserializes byte for byte this way)."""
    return json.dumps(data, indent=2, ensure_ascii=False) + "\n"


def write_atomic(path: Path, text: str) -> None:
    """`.next` + rename: a crash mid-write never leaves a half-written settings file."""
    staged = path.with_name(path.name + ".next")
    mode = path.stat().st_mode & 0o777 if path.exists() else 0o644
    try:
        staged.write_text(text, encoding="utf-8")
        os.chmod(staged, mode)
        os.replace(staged, path)
    except OSError:
        staged.unlink(missing_ok=True)
        raise


def iter_hooks(hooks: dict[str, Any]) -> Iterator[tuple[str, dict[str, Any], dict[str, Any]]]:
    for event, groups in hooks.items():
        for group in groups if isinstance(groups, list) else []:
            for hook in group.get("hooks", []):
                yield event, group, hook


def _finding(rule: str, event: str, group: dict[str, Any], hook: dict[str, Any], detail: str) -> dict[str, str]:
    return {
        "rule": rule,
        "event": event,
        "matcher": str(group.get("matcher", "")),
        "command": str(hook.get("command") or hook.get("prompt", ""))[:200],
        "if": json.dumps(hook.get("if", group.get("if"))),
        "detail": detail,
    }


def _if_findings(event: str, group: dict[str, Any], hook: dict[str, Any]) -> list[dict[str, str]]:
    found = []
    for if_val in (group.get("if"), hook.get("if")):
        if if_val is None:
            continue
        if isinstance(if_val, list) or IF_MULTIPATTERN.search(str(if_val)):
            found.append(_finding("L1", event, group, hook, f"`if` holds more than one rule: {if_val!r}"))
        if event not in IF_SUPPORTED_EVENTS:
            found.append(_finding("L2", event, group, hook, f"`if` on {event}, which ignores it: {if_val!r}"))
    return found


def lint_hooks(hooks: dict[str, Any]) -> list[dict[str, str]]:
    """Every finding of L1-L4. Empty list = clean."""
    findings: list[dict[str, str]] = []
    for event, group, hook in iter_hooks(hooks):
        findings += _if_findings(event, group, hook)
        command = str(hook.get("command", ""))
        if PROJECT_SCRIPT.search(command):
            findings.append(_finding("L3", event, group, hook, f"project script registered globally: {command!r}"))
        if hook.get("timeout") is None:
            findings.append(_finding("L4", event, group, hook, "no `timeout` (Claude Code's default is 600 s)"))
    return findings


def _key(finding: dict[str, str]) -> tuple[str, ...]:
    """Identity of a finding across files: position-free, so reordering is not a new finding."""
    return (finding["rule"], finding["event"], finding["matcher"], finding["command"], finding["if"])


def new_findings(source_hooks: dict[str, Any], live_hooks: dict[str, Any]) -> list[dict[str, str]]:
    old = {_key(f) for f in lint_hooks(live_hooks)}
    return [f for f in lint_hooks(source_hooks) if _key(f) not in old]


def cmd_lint_only(args: argparse.Namespace) -> int:
    findings = lint_hooks(load_json(args.source))
    if args.json:
        print(json.dumps({"source": str(args.source), "findings": findings}, ensure_ascii=False))
    for f in findings:
        print(f"  [{f['rule']}] {f['event']} {f['matcher']!r}: {f['detail']}", file=sys.stderr)
    if not args.json:
        print(f"LINT: {len(findings)} finding(s) in {args.source}")
    return 1 if findings else 0


def cmd_check(args: argparse.Namespace) -> int:
    source_hooks = load_json(args.source)
    live_hooks = load_json(args.settings).get("hooks", {})
    if json.dumps(source_hooks, sort_keys=True) != json.dumps(live_hooks, sort_keys=True):
        print(f"DRIFT: {args.settings} hooks differ from {args.source}", file=sys.stderr)
        return 1
    print("OK: no drift")
    return 0


def cmd_apply(args: argparse.Namespace) -> int:
    source_hooks = load_json(args.source)
    if not source_hooks:
        print("REFUSED: empty source — applying it would remove every hook", file=sys.stderr)
        return 1
    live = load_json(args.settings)
    added = new_findings(source_hooks, live.get("hooks", {}))
    if added:
        print(f"REFUSED: the source adds {len(added)} lint finding(s):", file=sys.stderr)
        for f in added:
            print(f"  [{f['rule']}] {f['event']} {f['matcher']!r}: {f['detail']}", file=sys.stderr)
        return 1
    live["hooks"] = source_hooks
    write_atomic(args.settings, dump(live))
    print(f"APPLIED: hooks section written to {args.settings}")
    return 0


def cmd_propose(args: argparse.Namespace) -> int:
    live = load_json(args.settings)
    live["hooks"] = load_json(args.source)
    Path(args.propose).write_text(dump(live), encoding="utf-8")
    print(f"PROPOSED: merged settings written to {args.propose} (real file untouched)")
    return 0


def cmd_generate(args: argparse.Namespace) -> int:
    hooks = load_json(args.settings).get("hooks", {})
    args.source.parent.mkdir(parents=True, exist_ok=True)
    write_atomic(args.source, dump(hooks))
    print(f"GENERATED: {args.source} ({len(hooks)} events)")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Claude Code global hooks section manager (S49)")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", action="store_true", help="exit 1 if the live hooks drifted from the source")
    group.add_argument("--apply", action="store_true", help="write the source into the settings")
    group.add_argument("--propose", metavar="OUT", help="write the merged settings to OUT (dry run)")
    group.add_argument("--lint-only", action="store_true", help="lint the source; exit 1 on findings")
    group.add_argument("--generate", action="store_true", help="bootstrap the source from the settings")
    parser.add_argument("--settings", type=Path, default=SETTINGS_PATH)
    parser.add_argument("--source", type=Path, default=HOOKS_SETTINGS_PATH)
    parser.add_argument("--json", action="store_true", help="--lint-only: machine-readable findings")
    args = parser.parse_args(argv)
    for flag, handler in (
        ("check", cmd_check),
        ("apply", cmd_apply),
        ("propose", cmd_propose),
        ("lint_only", cmd_lint_only),
        ("generate", cmd_generate),
    ):
        if getattr(args, flag):
            return handler(args)
    return 1


if __name__ == "__main__":
    sys.exit(main())
