"""Probe the D2 hook-bypass verdicts of block_git.sh against throwaway repos (hermetic: no global git config)."""
import json
import os
import subprocess
import sys
from pathlib import Path

W = Path(sys.argv[1])
SCR = Path(sys.argv[2])
ENV = {**os.environ, "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1", "HOME": str(SCR)}
R, H = SCR / "d2repo", SCR / "d2plain"
for d in (R, H):
    d.mkdir(exist_ok=True)
    if not (d / ".git").exists():
        subprocess.run(["git", "-C", str(d), "init", "-q"], check=True, env=ENV)
hook = R / ".git/hooks/pre-commit"
if not hook.exists():
    hook.write_text("#!/bin/sh\nexit 0\n")
    hook.chmod(0o755)


def probe(cwd: Path, cmd: str) -> None:
    payload = json.dumps({"cwd": str(cwd), "tool_input": {"command": cmd}})
    p = subprocess.run(["bash", str(W / "client/hooks/block_git.sh")], input=payload, capture_output=True, text=True, env=ENV)
    out = p.stdout.strip()
    verdict = "deny " if '"deny"' in out else "allow"
    why = json.loads(out)["hookSpecificOutput"]["permissionDecisionReason"][:230] if out else p.stderr.strip()[:100]
    print(f"{verdict} | {cmd} | {why}")


for cwd, cmd in [
    (R, "git commit --no-verify -m x"),
    (R, "git commit -n -m x"),
    (R, "git commit -nm x"),
    (R, "git commit -m -n"),
    (R, "git push -n origin main"),
    (R, "git merge -n topic"),
    (R, "git -c core.hooksPath=/dev/null commit -m x"),
    (R, "SKIP=foo git commit -m x"),
    (R, 'bash -c "git commit --no-verify -m x"'),
    (R, "GIT_HOOKS_BYPASS_OK=1 git commit --no-verify -m x"),
    (H, "git commit --no-verify -m x"),
    (H, f"cd {R} && git commit -n -m x"),
    (H, f"git -C {R} commit -n -m x"),
    (R, "GIT_DESTRUCTIVE_OK=1 git commit --no-verify -m x"),
]:
    probe(cwd, cmd)
