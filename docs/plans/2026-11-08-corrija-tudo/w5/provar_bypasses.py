#!/usr/bin/env python3
"""W5 proof: the 7 pre-bash bypasses of 30.4.105, asked of an INSTALLED toolchain.

Each row is a destructive command that 30.4.105 let through (no `deny`) while the same
command without the prefix was denied; each control must stay free. One JSON line per
check {check, ok, detail}; exit 0 only when every check is ok. Paths under $HOME print as ~.

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
    "bash -c true <<< x\ngit reset --hard",
    "nice chmod 000 f",
    "/usr/bin/env chmod 000 f",
    "A+=1 chmod 000 f",
    "timeout .5 git reset --hard",
    "env -S 'rm -rf /tmp/x'",
    "xargs chmod 000 f",
    # found by the commit security review of 7cf12bad: `S` inside an option cluster
    "env -iS 'chmod 000 f'",
    # onda 2: a quoted `>` does not glue the `&` (review of 7714f7f4, never installed)
    "echo 'a>'&chmod 000 f",
    # the controls of the defect: the same commands without a prefix
    "chmod 000 f",
    "bash -c true <<< x; git reset --hard",
]
MUST_PASS = ["nice -n 5 cargo test", "timeout .5 ls", "cat <<< 'git reset --hard'"]
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


for command in MUST_DENY + MUST_PASS:
    got = decision(command)
    want_deny = command in MUST_DENY
    ok = (got == "deny") == want_deny
    line = {"check": command, "ok": ok, "detail": {"want": "deny" if want_deny else "free", "got": got}}
    results.append(line)
    print(json.dumps(line, ensure_ascii=False).replace(HOME, "~"))

ok = all(r["ok"] for r in results)
print(json.dumps({"version": VERSION, "all_ok": ok, "passed": sum(r["ok"] for r in results),
                  "total": len(results)}))
sys.exit(0 if ok else 1)
