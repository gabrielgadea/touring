#!/usr/bin/env python3
"""Where `block_git.sh` and the Rust command policy disagree, over the replay corpus.

`client/hooks/block_git.sh` (git only, REGRA #11 v2) and the full Rust
`touring_hooks_shared::command_policy::judge` both read destructive Git forms:
the shell guard is the only Claude Code hook that denies them, while the Rust
mirror remains available to the CEG. `judge_pre_bash` deliberately excludes Git,
so the two hooks stay disjoint. This script measures where the full Rust verdict
and the shell guard disagree, without anyone re-deriving the table.

It READS the shell guard; it never edits it. Each corpus command is fed to the script
the way Claude Code feeds it (the PreToolUse JSON on stdin) from a scratch git
repository, once with an ACTIVE repository hook and once without, because the
hooks-bypass forms (`--no-verify`, `core.hooksPath`, `SKIP=`, `HUSKY=0`) are judged
against what the target repository holds.

Usage:
    scripts/block_git_divergence.py [--baseline PATH] [--justified PATH] [--guard PATH] [--json]

The Rust verdict is TODAY's, composed exactly as the replay test
(`command_policy/tests.rs`, `the_corpus_gets_the_verdicts_it_got_before_the_module_existed`)
pins it: the `pipeline` verdict of the baseline
(`crates/touring-hooks-shared/tests/fixtures/command_policy/baseline.jsonl`, the policy frozen
BEFORE C2), replaced by the `after` of a `pipeline` row of `justified_differences.jsonl` when
there is one. The replay test fails whenever today's `judge` differs from that composition, so the
composition is today's verdict without building Rust. Reading the baseline alone (until
10/11/2026) reported `git restore --staged a && git restore b` as "Rust allows" — a verdict the
policy had not given since C2. `judge`, not the Git-free `judge_pre_bash` used by the hook.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DEFAULT_BASELINE = REPO / "crates/touring-hooks-shared/tests/fixtures/command_policy/baseline.jsonl"
DEFAULT_JUSTIFIED = DEFAULT_BASELINE.with_name("justified_differences.jsonl")
DEFAULT_GUARD = REPO / "client/hooks/block_git.sh"
GIT_WORDS = ("git ", "git\t", "/git ", "git\n")


def todays_rust_blocks(baseline_rows: list[dict], justified: Path) -> dict[str, bool]:
    """Command -> does today's Rust `judge` block it. Rows are `[cmd, field, before, after, why]`;
    only the `pipeline` field is the verdict this script compares."""
    blocks = {row["cmd"]: row["pipeline"]["v"] == "block" for row in baseline_rows}
    for line in justified.read_text().splitlines():
        if not line.strip():
            continue
        cmd, field, _before, after, _why = json.loads(line)
        if field == "pipeline" and cmd in blocks:
            blocks[cmd] = after.startswith("block")
    return blocks


def make_repo(root: Path, *, active_hook: bool) -> Path:
    """A scratch repository, with or without an executable pre-commit hook."""
    repo = root / ("hooked" if active_hook else "plain")
    repo.mkdir()
    env = {**os.environ, "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null"}
    subprocess.run(["git", "init", "-q", str(repo)], check=True, capture_output=True, env=env)
    if active_hook:
        hook = repo / ".git/hooks/pre-commit"
        hook.write_text("#!/bin/sh\nexit 0\n")
        hook.chmod(0o755)
    return repo


def guard_denies(guard: Path, command: str, cwd: Path) -> bool:
    """Whether the shell guard denies `command` when run from `cwd`."""
    payload = {
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "session_id": "divergence-probe",
        "cwd": str(cwd),
        "tool_input": {"command": command},
    }
    proc = subprocess.run(
        ["bash", str(guard)],
        input=json.dumps(payload).encode(),
        capture_output=True,
        timeout=60,
        cwd=cwd,
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"guard exited {proc.returncode} on {command!r}: {proc.stderr[:200]!r}")
    try:
        out = json.loads(proc.stdout.decode() or "{}")
    except json.JSONDecodeError:
        return False
    return (out.get("hookSpecificOutput") or {}).get("permissionDecision") in ("deny", "block")


def classify(is_git: bool, rust_block: bool, guard_block: bool) -> str:
    if rust_block == guard_block:
        return "agree"
    if guard_block:
        return "shell-only (git destructive; Rust allows)" if is_git else "shell-only (non-git form)"
    return "rust-only (guard allows)"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    ap.add_argument("--justified", type=Path, default=DEFAULT_JUSTIFIED)
    ap.add_argument("--guard", type=Path, default=DEFAULT_GUARD)
    ap.add_argument("--json", action="store_true", help="emit the full table as JSON")
    ap.add_argument("--workers", type=int, default=4)
    args = ap.parse_args()

    rows = [json.loads(line) for line in args.baseline.read_text().splitlines() if line.strip()]
    rust_blocks = todays_rust_blocks(rows, args.justified)
    with tempfile.TemporaryDirectory(prefix="block-git-divergence-") as tmp:
        root = Path(tmp)
        hooked, plain = make_repo(root, active_hook=True), make_repo(root, active_hook=False)

        def judge(row: dict) -> dict:
            cmd = row["cmd"]
            rust_block = rust_blocks[cmd]
            g_hooked = guard_denies(args.guard, cmd, hooked)
            g_plain = guard_denies(args.guard, cmd, plain)
            return {
                "cmd": cmd,
                "rust": "block" if rust_block else "allow",
                "guard_hooked_repo": "deny" if g_hooked else "allow",
                "guard_plain_repo": "deny" if g_plain else "allow",
                "class": classify(any(w in cmd for w in GIT_WORDS), rust_block, g_hooked),
                "hooks_dependent": g_hooked != g_plain,
            }

        with ThreadPoolExecutor(max_workers=args.workers) as pool:
            table = list(pool.map(judge, rows))

    counts = Counter(r["class"] for r in table)
    if args.json:
        json.dump({"counts": counts, "rows": table}, sys.stdout, ensure_ascii=False, indent=1)
        return 0
    print(f"{len(table)} commands; shell guard judged from a repo with an active hook")
    for klass, n in counts.most_common():
        print(f"  {n:5d}  {klass}")
    print("\ncommands the two owners judge differently:")
    for r in table:
        if r["class"] != "agree":
            cmd = r["cmd"].replace("\n", "\\n")
            print(f"  [{r['class']}] rust={r['rust']} guard={r['guard_hooked_repo']}  {cmd[:110]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
