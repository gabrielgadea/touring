# touring-quality:allow-secrets — the fixtures carry the GIT_DESTRUCTIVE_OK=1 bypass token on purpose
"""test_s25_block_git_nested.py — the REGRA #11 guard sees destructive git through its wrappers.

Measured 17/10/2026 (docs/plans/2026-10-17-analise-ecc/scripts/sonda_git_aninhado.py): of the six
PreToolUse Bash hooks only block_git.sh denies, and only the direct `git reset --hard HEAD`.  Every
nested spelling passed — `bash -c '…'`, `eval`, a heredoc into a shell, `xargs`, an interpreter's
`os.system(…)`, `hw-slot … -- bash -c '…'` — because _shell_mask.py blanked every quoted literal and
heredoc body whatever its destination.  Three more holes surfaced in the same probe: `git -C dir reset
--hard` (the patterns want `git reset` adjacent), `git reset '--hard'` (the quoted flag was blanked),
and a heredoc marker (or a here-string) swallowing the rest of its line / the following lines.

Contract under test: text handed to something that EXECUTES it is code and is re-scanned (bounded:
depth and a work budget proportional to the input; exhaustion is a DENY that names the way back); text
handed to something passive (`echo`, `cat`, `tee`, `grep`, `git commit -m`) stays DATA.  The bypass
token counts at any level.
"""
from __future__ import annotations

import shlex
import sys
import time

import pytest
from helpers import CLIENT_HOOKS, run_hook_sh

SCRIPT = CLIENT_HOOKS / "block_git.sh"
sys.path.insert(0, str(CLIENT_HOOKS))
import _shell_mask  # the masker under test, imported from the hooks dir (the bare sys.path insert above exempts E402)


def _payload(command: str) -> dict:
    return {"hook_event_name": "PreToolUse", "tool_name": "Bash", "session_id": "hooks-contract-s25-nested",
            "tool_input": {"command": command}}


def _verdict(command: str) -> tuple[bool, str]:
    result = run_hook_sh(SCRIPT, _payload(command))
    assert result.rc == 0, result.stderr_raw
    hso = (result.json or {}).get("hookSpecificOutput", {})
    return hso.get("permissionDecision") == "deny", hso.get("permissionDecisionReason", "")


# id -> command.  Each is a spelling of a destructive git the old guard let through.
DENIED = {
    # shells with -c, in every flag spelling
    "bash -c": "bash -c 'git reset --hard HEAD'",
    "sh -c double quotes": 'sh -c "git clean -fd"',
    "zsh -c": "zsh -c 'git stash'",
    "dash -c": "dash -c 'git reset --hard'",
    "bash -lc combined": "bash -lc 'git reset --hard'",
    "sh -ec combined": "sh -ec 'git clean -fd'",
    "bash -o pipefail -c": "bash -o pipefail -c 'git reset --hard'",
    "bash --login -c": "bash --login -c 'git reset --hard'",
    "bash absolute path": "/bin/bash -c 'git reset --hard'",
    "bash -c with $0 arg": "bash -c 'git reset --hard' arg0",
    "nested two levels": "bash -c \"sh -c 'git reset --hard'\"",
    "nested three levels": "bash -c \"sh -c \\\"bash -c 'git reset --hard'\\\"\"",
    "eval": 'eval "git stash"',
    "eval words": "eval 'git reset' '--hard'",
    # a program fed through stdin to a shell
    "heredoc into bash": "bash <<'EOF'\ngit reset --hard HEAD\nEOF",
    "heredoc into bash unquoted": "bash <<EOF\ngit clean -fd\nEOF",
    "heredoc into sh -s": "sh -s <<'EOF'\ngit stash\nEOF",
    "heredoc into bash -s --": "bash -s -- arg <<'EOF'\ngit reset --hard\nEOF",
    "heredoc tab-stripped into bash": "bash <<-'EOF'\n\tgit reset --hard\n\tEOF",
    "cat heredoc piped to bash": "cat <<'EOF' | bash\ngit reset --hard\nEOF",
    "here-string into bash": "bash <<< 'git reset --hard'",
    "echo piped to sh": "echo 'git reset --hard' | sh",
    "printf piped to bash": "printf '%s\\n' 'git clean -fd' | bash",
    # command / process substitution
    "substitution in double quotes": "echo \"$(bash -c 'git reset --hard')\"",
    "backtick substitution": "echo `git stash`",
    "unquoted heredoc substitution": "cat <<EOF\n$(git reset --hard)\nEOF",
    "process substitution": "diff <(git stash) /dev/null",
    # launchers: what they run is code
    "env": "env X=1 bash -c 'git reset --hard'",
    "env -i": "env -i bash -c 'git reset --hard'",
    "env -u name": "env -u HOME bash -c 'git reset --hard'",
    "env -S": "env -S 'git reset --hard'",
    "timeout": 'timeout 60 bash -c "git reset --hard"',
    "timeout -k": "timeout -k 5 60 sh -c 'git clean -fd'",
    "nice": "nice -n 5 bash -c 'git reset --hard'",
    "nohup": "nohup sh -c 'git clean -fd' &",
    "command": "command bash -c 'git reset --hard'",
    "exec": "exec bash -c 'git stash'",
    "sudo": "sudo -u root bash -c 'git reset --hard'",
    "sudo -s": "sudo -s 'git reset --hard'",
    "time": "time bash -c 'git reset --hard'",
    "setsid": "setsid bash -c 'git reset --hard'",
    "ionice": "ionice -c 3 bash -c 'git reset --hard'",
    "stdbuf": "stdbuf -o0 bash -c 'git reset --hard'",
    "hw-slot --": "hw-slot --class rust -- bash -c 'git reset --hard'",
    "hw-slot label --": "hw-slot --label ecc-py -- bash -c 'git clean -fd'",
    "wrappers stacked": "env X=1 timeout 60 nice -n 5 hw-slot -- bash -c 'git reset --hard'",
    "busybox sh": "busybox sh -c 'git reset --hard'",
    "su -c": "su -c 'git reset --hard' root",
    "flock -c": "flock /tmp/lock -c 'git reset --hard'",
    "find -exec": "find . -name x -exec bash -c 'git reset --hard' \\;",
    "find -exec plus": "find . -name x -exec sh -c 'git clean -fd' +",
    "touring exec": "touring exec 'git reset --hard'",
    "touring run bash": "touring run --lang bash --code 'git reset --hard'",
    "touring run python": "touring run --lang python --code \"import os; os.system('git reset --hard')\"",
    # xargs: stdin supplies the arguments
    "xargs bare subcommand": "echo --hard | xargs git reset",
    "xargs clean": "ls | xargs git clean",
    "xargs checkout --": "ls | xargs -n1 git checkout --",
    "xargs -I bash -c": "ls | xargs -I{} bash -c 'git reset --hard {}'",
    "xargs stash": "echo x | xargs git stash",
    "xargs no subcommand": "echo reset --hard | xargs git",
    "xargs -0 option": "find . -print0 | xargs -0 git restore",
    # interpreters that call a shell
    "python os.system": "python3 -c \"import os; os.system('git reset --hard')\"",
    "python subprocess list": "python3 -c \"import subprocess; subprocess.run(['git','reset','--hard'])\"",
    "python subprocess shell": "python3 -c \"import subprocess; subprocess.run('git clean -fd', shell=True)\"",
    "python Popen": "python3 -c \"import subprocess; subprocess.Popen(['git', 'stash'])\"",
    "python subprocess inner quoting": (
        "python3 -c \"import subprocess; subprocess.run(['bash', '-c', \\\"echo hi; git 'reset' '--hard'\\\"])\""),
    "python subprocess nested shell literal": (
        "python3 -c \"import subprocess; subprocess.run(['sh', '-c', \\\"bash -c 'git reset --hard'\\\"])\""),
    "python -Ic cluster": "python3 -Ic \"import os; os.system('git reset --hard')\"",
    "python heredoc": "python3 <<'EOF'\nimport os\nos.system('git reset --hard')\nEOF",
    "python - heredoc": "python3 - <<'EOF'\nimport subprocess\nsubprocess.check_call(['git', 'reset', '--hard'])\nEOF",
    "echo piped to python": "echo \"import os; os.system('git reset --hard')\" | python3",
    "perl system": "perl -e 'system(\"git clean -fd\")'",
    "perl system list": "perl -e 'system(\"git\", \"reset\", \"--hard\")'",
    "perl backticks": "perl -e 'print `git stash`'",
    "ruby system": "ruby -e 'system(\"git reset --hard\")'",
    "node execSync": "node -e \"require('child_process').execSync('git reset --hard')\"",
    "node spawnSync": "node -e \"require('child_process').spawnSync('git', ['reset', '--hard'])\"",
    "awk system": "awk 'BEGIN { system(\"git reset --hard\") }'",
    "php shell_exec": "php -r 'shell_exec(\"git clean -fd\");'",
    # holes in the data masking itself
    "git -C": "git -C /tmp/x reset --hard",
    "git -c": "git -c core.x=1 clean -fd",
    "git --no-pager": "git --no-pager reset --hard",
    "git --git-dir=": "git --git-dir=/tmp/x/.git reset --hard",
    "git -C stash": "git -C /tmp/x stash",
    "git -C push delete": "git -C /tmp/x push origin :main",
    "quoted subcommand": 'git "reset" --hard',
    "quoted flag single": "git reset '--hard'",
    "quoted flag double": 'git clean "-fd"',
    "ansi-c flag": "git reset $'--hard'",
    "quoted git": "'git' reset --hard",
    "escaped git": "\\git reset --hard",
    "heredoc marker then command": "cat <<EOF && git reset --hard\nbody\nEOF",
    "heredoc marker then pipe": "cat <<EOF | git clean -fd\nbody\nEOF",
    "here-string then newline": 'cat <<< "x"\ngit reset --hard',
    "heredoc marker inside quotes": 'echo "<<EOF"\ngit reset --hard',
    "apostrophe in double quotes": "git commit -m \"don't\" && git reset --hard HEAD~1 && echo 'ok'",
    "arithmetic shift then command": "echo $((1<<2))\ngit reset --hard",
    "arithmetic command then command": "(( x = 1 << 2 ))\ngit reset --hard",
    "assignment before shell": "FOO=1 bash -c 'git reset --hard'",
    "quoted assignment before shell": "PATH=\"$PATH:/x\" bash -c 'git reset --hard'",
    "after comment line": "# fine\ngit reset --hard",
    "hash inside word is not a comment": "echo a#b; git reset --hard",
}

# Controls: the same machinery must NOT turn data into code.
ALLOWED = {
    "git status": "git status",
    "bash -c git status": "bash -c 'git status'",
    "nested git status": "bash -c \"sh -c 'git status'\"",
    "quoted text echo": 'echo "git reset --hard"',
    "single quoted echo": "echo 'git clean -fd'",
    "heredoc cat redirect": "cat <<'EOF' > notas.md\ntexto citando git reset --hard\nEOF",
    "heredoc cat unquoted": "cat > notas.md <<EOF\ntexto citando git reset --hard\nEOF",
    "heredoc tee": "tee f.txt <<EOF\ngit reset --hard\nEOF",
    "heredoc cat then pipe to grep": "cat <<'EOF' | grep reset\ngit reset --hard\nEOF",
    "unquoted-delimiter heredoc without substitution": "cat <<EOF\nuse git reset --hard with care\nEOF",
    "quoted-delimiter heredoc substitution is data": "cat <<'EOF'\n$(git reset --hard)\nEOF",
    "grep pattern": 'grep -n "reset --hard" arquivo',
    "rg pattern": "rg 'git stash'",
    "commit message": 'git commit -m "doc: explica git reset --hard"',
    "commit message heredoc with apostrophes": (
        "git commit -m \"$(cat <<'EOF'\nFix don't break; explica git reset --hard\nEOF\n)\""),
    "commit -C quoted": "git -C /tmp/x commit -m 'nota sobre git clean -fd'",
    "git log grep": "git log --grep='git reset --hard'",
    "git log grep double": 'git log --grep="git stash"',
    "bash -c echo data": "bash -c 'echo \"git reset --hard\"'",
    "bash -c grep data": "bash -c \"grep -n 'git clean -fd' notes.md\"",
    "bash script file": "bash script.sh",
    "bash script file fed data on stdin": "bash script.sh <<'EOF'\ngit reset --hard\nEOF",
    "python script file fed data on stdin": "python3 script.py <<'EOF'\nos.system('git reset --hard')\nEOF",
    "bash -c with $0 naming git": "bash -c 'echo hi' 'git reset --hard'",
    "eval echo": 'eval "echo hi"',
    "python print": "python3 -c \"print('git reset --hard')\"",
    "python heredoc writing text": "python3 <<'EOF'\nopen('x', 'w').write('git reset --hard')\nEOF",
    "python subprocess git status": "python3 -c \"import subprocess; subprocess.run(['git', 'status'])\"",
    "python script file": "python3 script.py -c \"import os; os.system('git reset --hard')\"",
    "python -m module": "python3 -m pytest -k 'git reset --hard'",
    "node console.log": "node -e \"console.log('git reset --hard')\"",
    "perl print": "perl -e 'print \"git clean -fd\"'",
    "awk print": "awk 'BEGIN { print \"git reset --hard\" }'",
    "timeout git status": "timeout 60 git status",
    "env bash echo": "env X=1 bash -c 'echo hi'",
    "hw-slot cargo": "hw-slot --class rust -- cargo test -p touring-hooks",
    "hw-slot status": "hw-slot --status",
    "nice cargo": "nice -n 5 cargo build",
    "xargs git add": "ls | xargs git add",
    "xargs git status": "ls | xargs git status -s",
    "xargs restore staged": "ls | xargs git restore --staged",
    "xargs stash list": "echo x | xargs git stash list",
    "xargs echo": "echo x | xargs echo",
    "git -C status": "git -C /tmp/x status",
    "git -C stash list": "git -C /tmp/x stash list",
    "git -C restore staged": "git -C /tmp/x restore --staged f",
    "git --no-pager log": "git --no-pager log -3",
    "git -c config status": "git -c core.pager=cat status",
    "git branch -d": "git branch -d merged-topic",
    "comment mentions it": "git status # nunca rode git reset --hard",
    "comment line": "# git reset --hard\nls",
    "apostrophe in double quotes harmless": "git commit -m \"don't\" && git status && echo 'ok'",
    "arithmetic shift": "echo $((1<<2))\nls",
    "arithmetic command": "(( x = 1 << 2 ))\ngit status",
    "command -v": "command -v git",
    "find -exec ls": "find . -name x -exec ls {} \\;",
    "touring run python print": "touring run --lang python --code \"print('git reset --hard')\"",
    "su without command": "su - root",
}

# The token is a per-command human decision, valid at any level — and only when it is CODE.
TOKEN_ALLOWED = {
    "token outer then bash -c": "GIT_DESTRUCTIVE_OK=1 bash -c 'git reset --hard'",
    "token inner": "bash -c 'GIT_DESTRUCTIVE_OK=1 git reset --hard'",
    "token two levels in": "bash -c \"sh -c 'GIT_DESTRUCTIVE_OK=1 git reset --hard'\"",
    "token via env": "env GIT_DESTRUCTIVE_OK=1 bash -c 'git clean -fd'",
    "token on xargs": "echo --hard | GIT_DESTRUCTIVE_OK=1 xargs git reset",
    "token inside python exec call": "python3 -c \"import os; os.system('GIT_DESTRUCTIVE_OK=1 git reset --hard')\"",
    "token on git -C": "GIT_DESTRUCTIVE_OK=1 git -C /tmp/x reset --hard",
}
TOKEN_NOT_A_BYPASS = {
    "token quoted at the outer level": "echo 'GIT_DESTRUCTIVE_OK=1'; bash -c 'git reset --hard'",
    "token in a heredoc body": "cat > f <<'EOF'\nGIT_DESTRUCTIVE_OK=1\nEOF\nbash -c 'git reset --hard'",
    "token as data in an inner echo": "bash -c \"echo 'GIT_DESTRUCTIVE_OK=1'; git reset --hard\"",
}


@pytest.mark.parametrize("command", DENIED.values(), ids=DENIED.keys())
def test_nested_destructive_git_is_denied(command: str) -> None:
    denied, reason = _verdict(command)
    assert denied, f"destructive git slipped through: {command!r}"
    assert "REGRA #11" in reason, reason


@pytest.mark.parametrize("command", ALLOWED.values(), ids=ALLOWED.keys())
def test_data_and_harmless_forms_stay_allowed(command: str) -> None:
    denied, reason = _verdict(command)
    assert not denied, f"false positive on {command!r}: {reason}"


@pytest.mark.parametrize("command", TOKEN_ALLOWED.values(), ids=TOKEN_ALLOWED.keys())
def test_token_is_valid_at_any_level(command: str) -> None:
    denied, reason = _verdict(command)
    assert not denied, f"GIT_DESTRUCTIVE_OK=1 was ignored in {command!r}: {reason}"


@pytest.mark.parametrize("command", TOKEN_NOT_A_BYPASS.values(), ids=TOKEN_NOT_A_BYPASS.keys())
def test_token_as_data_is_not_a_bypass(command: str) -> None:
    denied, _ = _verdict(command)
    assert denied, f"the token written as DATA bypassed the guard: {command!r}"


def test_xargs_deny_names_the_way_back() -> None:
    denied, reason = _verdict("echo --hard | xargs git reset")
    assert denied
    assert "xargs" in reason and "GIT_DESTRUCTIVE_OK=1" in reason, reason


# ── bounded recursion: exhaustion is a DENY that names the route, never a pass ──────────────────────


def _nest(inner: str, levels: int) -> str:
    cmd = inner
    for _ in range(levels):
        cmd = "bash -c " + shlex.quote(cmd)
    return cmd


def test_depth_within_the_limit_is_judged_normally() -> None:
    assert _verdict(_nest("git reset --hard", 4))[0]
    assert not _verdict(_nest("git status", 4))[0]


def test_depth_beyond_the_limit_is_denied_with_the_route() -> None:
    too_deep = _nest("git status", _shell_mask.MAX_DEPTH + 3)
    denied, reason = _verdict(too_deep)
    assert denied, "an unverifiable command must be denied, not waved through"
    assert "profundidade" in reason and "GIT_DESTRUCTIVE_OK=1" in reason, reason


def test_token_overrides_the_budget_deny() -> None:
    too_deep = "GIT_DESTRUCTIVE_OK=1 " + _nest("git status", _shell_mask.MAX_DEPTH + 3)
    assert not _verdict(too_deep)[0]


def test_work_budget_is_proportional_and_bounded() -> None:
    """Thousands of nested exec calls: the scan must stop at the budget (a deny), in bounded time."""
    nested = "system(" * 4000 + "'git status'" + ")" * 4000
    command = f"perl -e {shlex.quote(nested)}"
    started = time.monotonic()
    result = _shell_mask.scan(command)
    elapsed = time.monotonic() - started
    assert any(note.startswith("budget:") for note in result.notes), result.notes
    assert elapsed < 5, f"scan took {elapsed:.1f}s: the budget must bound the work"
    denied, reason = _verdict(command)
    assert denied and "orçamento" in reason and "GIT_DESTRUCTIVE_OK=1" in reason, reason


def test_large_harmless_command_is_not_a_budget_deny() -> None:
    body = "\n".join(f"line {i} with some text" for i in range(5000))
    assert not _verdict(f"cat > big.txt <<'EOF'\n{body}\nEOF")[0]
    many = " && ".join("git status" for _ in range(500))
    assert not _verdict(many)[0]


def test_destructive_git_is_denied_whatever_the_size_of_the_command() -> None:
    """Under pipefail, `printf | grep -q` read an early match as "no match" once the input passed the 64 KB
    pipe buffer (grep exits, printf dies of SIGPIPE, status 141): a destructive command followed by padding
    was waved through.  The patterns run on here-strings now."""
    # CODE lines, not comments: the masker blanks comments, and a masked command that small never reaches
    # the pipe buffer (the mutant that restored `printf | grep -q` survived the comment version of this test).
    padding = ("\necho " + "y" * 200) * 1000
    assert len(padding) > 200_000
    assert _verdict("git reset --hard HEAD" + padding)[0]
    assert _verdict("bash -c 'git reset --hard'" + padding)[0]
    assert not _verdict("GIT_DESTRUCTIVE_OK=1 git reset --hard" + padding)[0]
    assert not _verdict("git status" + padding)[0]


# ── the masker itself ────────────────────────────────────────────────────────────────────────────────


def test_views_put_the_masked_outer_command_first() -> None:
    views = _shell_mask.scan("echo 'a'; bash -c 'git status'").views
    assert views[0] == "echo ''; bash -c ''"
    assert "git status" in views


def test_public_mask_function_keeps_its_one_level_contract() -> None:
    assert _shell_mask.mask_shell_data('echo "git reset --hard"') == 'echo ""'
    assert _shell_mask.mask_shell_data("cat <<'EOF'\ngit clean -fd\nEOF\nls") == "cat \nls"
    assert _shell_mask.mask_shell_data("git clean -fd") == "git clean -fd"


def test_masker_never_raises_on_hostile_input() -> None:
    """Deterministic fuzz: shell-ish garbage may be denied or allowed but must never crash the analyser."""
    import random

    rng = random.Random(20261017)
    alphabet = ["'", '"', "`", "$(", ")", "(", "<<", "<<<", "<<-", "EOF", "\n", " ", ";", "|", "&", "\\", "git ", "reset",
                "--hard", "bash -c ", "eval ", "xargs ", "$'", "${", "}", "#", "<(", ">", "2>&1 ", "os.system(", "python3 -c "]
    for _ in range(1500):
        text = "".join(rng.choice(alphabet) for _ in range(rng.randint(1, 40)))
        started = time.monotonic()
        result = _shell_mask.scan(text)
        assert result.views, text
        assert time.monotonic() - started < 2, text


def test_masker_cli_prints_views_and_notes() -> None:
    import subprocess

    proc = subprocess.run([sys.executable, str(CLIENT_HOOKS / "_shell_mask.py")], input=b"echo --hard | xargs git reset",
                          capture_output=True, check=False)
    assert proc.returncode == 0, proc.stderr
    lines = proc.stdout.decode().split("\n")  # not splitlines(): it treats the RS separator as a line break
    assert lines[0] == "echo --hard | xargs git reset"
    assert any(line.startswith(_shell_mask.NOTE_PREFIX + "xargs-git:reset") for line in lines), lines
