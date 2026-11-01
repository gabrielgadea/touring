"""Hook latency, D2: a command with no bypass pays nothing; a bypass pays the resolver (python + ~3 git calls).

Usage: latency_d2.py <hooks_dir> <repo_with_hooks> <repo_without_hooks> [rounds]
Medians over interleaved rounds (ABBA-like round-robin) so machine drift hits every case alike.
"""
import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path

HOOKS, HOOKED, PLAIN = (Path(a) for a in sys.argv[1:4])
ROUNDS = int(sys.argv[4]) if len(sys.argv) > 4 else 25
ENV = {**os.environ, "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1"}
CASES = {
    "git status (no bypass)": (HOOKED, "git status"),
    "git commit -m x (no bypass)": (HOOKED, "git commit -m x"),
    "commit -n, repo WITH hooks (deny)": (HOOKED, "git commit -n -m x"),
    "commit -n, repo WITHOUT hooks (allow)": (PLAIN, "git commit -n -m x"),
    "commit -n + bypass token (allow)": (HOOKED, "GIT_HOOKS_BYPASS_OK=1 git commit -n -m x"),
}


def once(cwd: Path, command: str) -> float:
    payload = json.dumps({"hook_event_name": "PreToolUse", "tool_name": "Bash", "cwd": str(cwd), "tool_input": {"command": command}})
    t0 = time.perf_counter()
    subprocess.run(["bash", str(HOOKS / "block_git.sh")], input=payload.encode(), capture_output=True, env=ENV, check=True)
    return (time.perf_counter() - t0) * 1000


times: dict[str, list[float]] = {name: [] for name in CASES}
for _ in range(3):  # warm the __pycache__ and the page cache
    for cwd, command in CASES.values():
        once(cwd, command)
for _ in range(ROUNDS):
    for name, (cwd, command) in CASES.items():
        times[name].append(once(cwd, command))
for name, series in times.items():
    print(f"{name:42s} median {statistics.median(series):6.1f} ms  p90 {sorted(series)[int(0.9 * len(series)) - 1]:6.1f} ms")
