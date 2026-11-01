import json, statistics, subprocess, sys, time
from pathlib import Path
CASES = {
    "git status": "git status",
    "cargo test pipeline": "cd /tmp && cargo test -p touring-hooks 2>&1 | tail -20 && echo done",
    "commit heredoc": "git commit -m \"$(cat <<'EOF'\nfix: algo\n\nCo-Authored-By: x\nEOF\n)\"",
    "bash -c nested": "bash -c 'cd /tmp && ls -la | head -5'",
}
for label, hooks in (("ORIGINAL", Path(sys.argv[1])), ("FIXED", Path(sys.argv[2]))):
    for name, cmd in CASES.items():
        payload = json.dumps({"tool_name": "Bash", "tool_input": {"command": cmd}}).encode()
        times = []
        for _ in range(25):
            t = time.perf_counter()
            subprocess.run(["bash", str(hooks / "block_git.sh")], input=payload, capture_output=True, check=False)
            times.append((time.perf_counter() - t) * 1000)
        print(f"{label:9s} {name:22s} median {statistics.median(times):6.1f} ms  p95 {sorted(times)[int(.95 * len(times)) - 1]:6.1f} ms")
