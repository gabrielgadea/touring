"""Replay real Bash commands from Claude Code transcripts through two copies of block_git.sh and diff the decisions.

Read-only on the transcripts. Usage:
  replay_block_git.py <old_hooks_dir> <new_hooks_dir> <out.json> [--max N] [--workers W]
Each hooks dir must contain block_git.sh (and _shell_mask.py next to it). Each payload carries the cwd recorded in
the transcript, as the real hook receives it. A decision is "deny" when the hook prints a
permissionDecision deny; anything else (allow, advisory, silence) is "allow". The guard never runs the command.
"""
import argparse
import glob
import hashlib
import json
import os
import random
import subprocess
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path


def bash_commands_in(line: str) -> list[tuple[str, str]]:
    """(command, cwd) pairs of the Bash tool calls in one transcript line; cwd is the record's own field."""
    if '"Bash"' not in line:
        return []
    try:
        rec = json.loads(line)
        content = (rec.get("message") or {}).get("content")
    except (json.JSONDecodeError, AttributeError):
        return []
    if not isinstance(content, list):
        return []
    cwd = rec.get("cwd") if isinstance(rec.get("cwd"), str) else ""
    found = [(p.get("input") or {}).get("command") for p in content
             if isinstance(p, dict) and p.get("type") == "tool_use" and p.get("name") == "Bash"]
    return [(c, cwd) for c in found if isinstance(c, str) and c]


def commands(max_n: int) -> list[tuple[str, str]]:
    seen, out = set(), []
    files = sorted(glob.glob(os.path.expanduser("~/.claude/projects/*/*.jsonl")), key=os.path.getmtime, reverse=True)
    for f in files:
        try:
            lines = Path(f).read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            continue
        for cmd, cwd in (pair for line in lines for pair in bash_commands_in(line)):
            if (cmd, cwd) not in seen:
                seen.add((cmd, cwd))
                out.append((cmd, cwd))
    # Keep every command that mentions git (the guard's domain) and a fixed-seed sample of the rest.
    gitty = [c for c in out if "git" in c[0]]
    rest = [c for c in out if "git" not in c[0]]
    random.Random(17).shuffle(rest)
    return (gitty + rest)[:max_n]


def decide(hooks_dir: str, cmd: str, cwd: str) -> str:
    # The cwd travels like in a real payload: the hook-bypass check (D2) judges the repository the command runs in.
    payload = json.dumps({"tool_name": "Bash", "tool_input": {"command": cmd}, "cwd": cwd,
                          "hook_event_name": "PreToolUse"})
    try:
        r = subprocess.run(["bash", str(Path(hooks_dir) / "block_git.sh")], input=payload, capture_output=True,
                           text=True, timeout=30, check=False)
    except subprocess.TimeoutExpired:
        return "timeout"
    if '"permissionDecision":"deny"' in r.stdout.replace(" ", "") or r.returncode == 2:
        return "deny"
    return "allow" if r.returncode == 0 else f"rc{r.returncode}"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("old")
    ap.add_argument("new")
    ap.add_argument("out")
    ap.add_argument("--max", type=int, default=4000)
    ap.add_argument("--workers", type=int, default=4)
    a = ap.parse_args()
    cmds = commands(a.max)
    with ThreadPoolExecutor(a.workers) as ex:
        old = list(ex.map(lambda c: decide(a.old, *c), cmds))
        new = list(ex.map(lambda c: decide(a.new, *c), cmds))
    diff = [{"id": hashlib.sha256(c.encode()).hexdigest()[:12], "old": o, "new": n, "cwd": d, "cmd": c[:600]}
            for (c, d), o, n in zip(cmds, old, new) if o != n]
    resumo = {"comandos": len(cmds), "com_git": sum("git" in c for c, _ in cmds),
              "deny_old": old.count("deny"), "deny_new": new.count("deny"),
              "outros_old": sorted({x for x in old if x not in ("allow", "deny")}),
              "outros_new": sorted({x for x in new if x not in ("allow", "deny")}),
              "divergencias": len(diff),
              "novos_deny": sum(d["new"] == "deny" for d in diff), "deny_removidos": sum(d["old"] == "deny" for d in diff)}
    Path(a.out).write_text(json.dumps({"resumo": resumo, "divergencias": diff}, ensure_ascii=False, indent=1),
                           encoding="utf-8")
    print(json.dumps(resumo, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
