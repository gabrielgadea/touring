#!/usr/bin/env python3
"""W5 proof: the 7 pre-bash bypasses of 30.4.105, asked of an INSTALLED toolchain.

Each row is a destructive command that 30.4.105 let through (no `deny`) while the same
command without the prefix was denied; each control must stay free. One JSON line per
check {check, ok, detail}; exit 0 only when every check is ok. Paths under $HOME print as ~.

Git rows are asked of `block_git.sh` (the hook Claude Code runs, `~/.claude/hooks/block_git.sh`):
since 28dbe17e (09/11/2026) `pre-bash` leaves git to it alone, so a git row asked of `pre-bash`
would prove nothing.

Usage: provar_bypasses.py [VERSION]   (default 30.4.108)
"""

import json
import subprocess
import sys
from pathlib import Path

HOME = str(Path.home())
VERSION = sys.argv[1] if len(sys.argv) > 1 else "30.4.108"
HOOK = str(Path(HOME) / f".touring/toolchains/{VERSION}/bin/touring-hook")

MUST_DENY = [
    "nice chmod 000 f",
    "/usr/bin/env chmod 000 f",
    "A+=1 chmod 000 f",
    "timeout .5 chmod 000 f",
    "env -S 'rm -rf /tmp/x'",
    "xargs chmod 000 f",
    # found by the commit security review of 7cf12bad: `S` inside an option cluster
    "env -iS 'chmod 000 f'",
    # onda 2: a quoted `>` does not glue the `&` (review of 7714f7f4, never installed)
    "echo 'a>'&chmod 000 f",
    # found by the review of 6eb0c5a0: the first letter that takes a value owns the cluster
    "env -uS chmod 000 f",
    "sudo -nu root chmod 000 f",
    # the control of the defect: the same command without a prefix
    "chmod 000 f",
]
MUST_PASS = ["nice -n 5 cargo test", "timeout .5 ls", "env -u HOME ls"]

GUARD = str(Path(HOME) / ".claude/hooks/block_git.sh")
GIT_MUST_DENY = [
    "bash -c true <<< x\ngit reset --hard",  # a here-string read as a heredoc swallowed line 2
    "timeout .5 git reset --hard",
    "env -iS 'git reset --hard'",
    "env -uS git reset --hard",  # -u owns S; the program is git
    "git clean --force",
    "git clean --forc",
    "git clean -x -f",
    "git checkout main -- crates/x.py",
    "git checkout -qf main",
    "git restore -S --wor f",
    "git restore -- -S",
    "git clean -n --no-dry-run -f",  # review of 96b7a8f3: a negation undoes -n
    "git restore --staged --no-staged f",
    "bash -c true <<< x; git reset --hard",  # control: no prefix
]
GIT_MUST_PASS = ["cat <<< 'git reset --hard'", "git clean -n", "git clean -fdx -n",
                 "git checkout -b nova", "git restore --staged f", "git status"]
results = []


def decision(command: str) -> str:
    payload = {"hook_event_name": "PreToolUse", "tool_name": "Bash",
               "tool_input": {"command": command}, "session_id": "w5-prova", "cwd": "/tmp"}
    r = subprocess.run([HOOK, "pre-bash"], input=json.dumps(payload), capture_output=True,
                       text=True, timeout=60)
    text = r.stdout + r.stderr
    if r.returncode == 2 or '"permissionDecision":"deny"' in text.replace(" ", ""):
        return "deny"
    return f"rc={r.returncode}"


def guard_decision(command: str) -> str:
    payload = {"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": command}}
    r = subprocess.run(["bash", GUARD], input=json.dumps(payload), capture_output=True, text=True,
                       timeout=60, cwd="/tmp")
    if '"permissionDecision":"deny"' in r.stdout.replace(" ", ""):
        return "deny"
    return f"rc={r.returncode}"


def check(hook: str, ask, command: str, want_deny: bool) -> None:
    got = ask(command)
    ok = (got == "deny") == want_deny
    line = {"check": command, "hook": hook, "ok": ok,
            "detail": {"want": "deny" if want_deny else "free", "got": got}}
    results.append(line)
    print(json.dumps(line, ensure_ascii=False).replace(HOME, "~"))


for command in MUST_DENY + MUST_PASS:
    check("pre-bash", decision, command, command in MUST_DENY)
for command in GIT_MUST_DENY + GIT_MUST_PASS:
    check("block_git.sh", guard_decision, command, command in GIT_MUST_DENY)

ok = all(r["ok"] for r in results)
print(json.dumps({"version": VERSION, "all_ok": ok, "passed": sum(r["ok"] for r in results),
                  "total": len(results)}))
sys.exit(0 if ok else 1)
