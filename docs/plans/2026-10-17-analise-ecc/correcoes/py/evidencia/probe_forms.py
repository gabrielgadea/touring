# touring-quality:allow-secrets — the fixtures carry the bypass token on purpose
"""Probe: run command forms through a block_git.sh and print DENY/allow. Usage: probe_forms.py <hooks_dir>"""
import json
import subprocess
import sys
from pathlib import Path

HOOKS = Path(sys.argv[1])

DENY = {
    "simples": "git reset --hard HEAD",
    "bash -c": "bash -c 'git reset --hard HEAD'",
    "sh -c dq": 'sh -c "git clean -fd"',
    "eval": 'eval "git stash"',
    "heredoc->bash": "bash <<'EOF'\ngit reset --hard HEAD\nEOF",
    "python os.system": "python3 -c \"import os; os.system('git reset --hard')\"",
    "xargs": "echo --hard | xargs git reset",
    "hw-slot bash -c": "hw-slot --class rust -- bash -c 'git reset --hard'",
    "timeout git": "timeout 60 git reset --hard",
    "timeout bash -c": 'timeout 60 bash -c "git reset --hard"',
    "env bash -c": "env X=1 bash -c 'git reset --hard'",
    "nice bash -c": "nice -n 5 bash -c 'git reset --hard'",
    "nohup sh -c": "nohup sh -c 'git clean -fd' &",
    "command bash -c": "command bash -c 'git reset --hard'",
    "exec bash -c": "exec bash -c 'git stash'",
    "sudo bash -c": "sudo -u root bash -c 'git reset --hard'",
    "bash -lc": "bash -lc 'git reset --hard'",
    "sh -ec": "sh -ec 'git clean -fd'",
    "git -C": "git -C /tmp/x reset --hard",
    "git -c": "git -c core.x=1 clean -fd",
    "herestring bash": "bash <<< 'git reset --hard'",
    "herestring then newline": 'cat <<< "x"\ngit reset --hard',
    "heredoc then && git": "cat <<EOF && git reset --hard\nbody\nEOF",
    "quoted flag": "git reset '--hard'",
    "quoted git": "'git' reset --hard",
    "pipe to sh": "echo 'git reset --hard' | sh",
    "cat heredoc | bash": "cat <<'EOF' | bash\ngit reset --hard\nEOF",
    "python heredoc": "python3 <<'EOF'\nimport os\nos.system('git reset --hard')\nEOF",
    "subprocess list": "python3 -c \"import subprocess; subprocess.run(['git','reset','--hard'])\"",
    "perl system": "perl -e 'system(\"git clean -fd\")'",
    "node execSync": "node -e \"require('child_process').execSync('git reset --hard')\"",
    "nested 2 levels": "bash -c \"sh -c 'git reset --hard'\"",
    "subst in dq": 'echo "$(bash -c \'git reset --hard\')"',
    "find -exec bash": "find . -name x -exec bash -c 'git reset --hard' \\;",
    "xargs bash -c": "ls | xargs -I{} bash -c 'git reset --hard {}'",
    "unquoted heredoc subst": "cat <<EOF\n$(git reset --hard)\nEOF",
}
ALLOW = {
    "git status": "git status",
    "bash -c git status": "bash -c 'git status'",
    "echo dq": 'echo "git reset --hard"',
    "heredoc cat": "cat <<'EOF' > notas.md\ntexto com git reset --hard\nEOF",
    "grep": 'grep -n "reset --hard" arquivo',
    "commit -m": 'git commit -m "doc: explica git reset --hard"',
    "rg": "rg 'git stash'",
    "bash -c echo data": "bash -c 'echo \"git reset --hard\"'",
    "python print": "python3 -c \"print('git reset --hard')\"",
    "python heredoc data": "python3 <<'EOF'\nopen('x','w').write('git reset --hard')\nEOF",
    "timeout git status": "timeout 60 git status",
    "hw-slot cargo": "hw-slot --class rust -- cargo test",
    "xargs git add": "ls | xargs git add",
    "token outer": "GIT_DESTRUCTIVE_OK=1 bash -c 'git reset --hard'",
    "token inner": "bash -c 'GIT_DESTRUCTIVE_OK=1 git reset --hard'",
    "git -C status": "git -C /tmp/x status",
    "git log grep": "git log --grep='git reset --hard'",
    "tee heredoc": "tee f.txt <<EOF\ngit reset --hard\nEOF",
}


def run(cmd: str) -> bool:
    payload = {"hook_event_name": "PreToolUse", "tool_name": "Bash", "session_id": "probe", "tool_input": {"command": cmd}}
    p = subprocess.run(["bash", str(HOOKS / "block_git.sh")], input=json.dumps(payload).encode(), capture_output=True, timeout=30, check=False)
    try:
        j = json.loads(p.stdout.decode().strip() or "{}")
    except ValueError:
        return False
    return j.get("hookSpecificOutput", {}).get("permissionDecision") == "deny"


bad = 0
for name, cmd in DENY.items():
    d = run(cmd)
    bad += not d
    print(f"{'DENY ok ' if d else 'MISSED  '} {name}")
for name, cmd in ALLOW.items():
    d = run(cmd)
    bad += d
    print(f"{'allow ok' if not d else 'FALSEPOS'} {name}")
print("problems:", bad)
