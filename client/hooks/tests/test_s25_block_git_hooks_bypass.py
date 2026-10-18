# touring-quality:allow-secrets  the fixtures carry the GIT_*_OK=1 bypass tokens and SKIP=/HUSKY= assignments on purpose
"""test_s25_block_git_hooks_bypass.py  skipping a repository's git hooks is denied where the hooks are ACTIVE (D2).

Evidence (29/09/2026): an agent ran `git commit --no-verify` in ~/projects/analise, whose pre-commit checks PII,
owners and LFS.  The REGRA #11 guard looked only for destructive git, so the bypass sailed through.  Contrato:

- the forms that skip hooks are denied only when the TARGET repository (the payload's cwd, a `cd`, or `git -C`)
  holds an active hook: an executable regular file named like a githooks(5) hook in `git rev-parse --git-path hooks`
  or in `core.hooksPath` (a `.sample`, a `pre-commit.bak` backup or any other name never runs);
- the deny NAMES the hooks that would be skipped and the way out, GIT_HOOKS_BYPASS_OK=1 per command  a token of
  its own that asks for no snapshot (nothing is lost) and that GIT_DESTRUCTIVE_OK=1 does not stand in for;
- `push -n` (dry run), `merge -n` (no-stat), `commit -m -n` (a message) are not bypasses.

Residual, documented in the report: a persistent `git config core.hooksPath …`, `--git-dir=`, and the path with
neither jq nor python3 (the raw-JSON fallback has no cwd and no notes) are not judged.
"""
from __future__ import annotations

import io
import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

import pytest
from helpers import CLIENT_HOOKS, run_hook_sh

SCRIPT = CLIENT_HOOKS / "block_git.sh"
sys.path.insert(0, str(CLIENT_HOOKS))
import _shell_mask  # the masker under test, imported from the hooks dir (the bare sys.path insert above exempts E402)

#: hermetic git: no global/system config, so a developer's own core.hooksPath cannot change a verdict
GIT_ENV = {"GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1"}
HOOK_BODY = "#!/bin/sh\nexit 0\n"
#: 14 githooks(5) names besides pre-commit, for the fixture whose listing must be capped
MANY_HOOKS = ("applypatch-msg", "commit-msg", "post-applypatch", "post-checkout", "post-commit", "post-merge",
              "post-rewrite", "pre-applypatch", "pre-auto-gc", "pre-merge-commit", "pre-push", "pre-rebase",
              "prepare-commit-msg", "reference-transaction")


def _git(cwd: Path, *args: str, home: Path | None = None) -> None:
    env = {**os.environ, **GIT_ENV, **({"HOME": str(home)} if home else {})}
    subprocess.run(["git", "-C", str(cwd), *args], check=True, capture_output=True, env=env)


def _hook(path: Path, body: str = HOOK_BODY, mode: int = 0o755) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(body)
    path.chmod(mode)


def _repo(path: Path) -> Path:
    path.mkdir(parents=True)
    _git(path, "init", "-q")
    for sample in (path / ".git" / "hooks").glob("*.sample"):  # start from a repo with no hook at all
        sample.unlink()
    return path


@dataclass
class World:
    root: Path
    home: Path
    hooked: Path  # pre-commit + commit-msg active; a .sample, a non-executable file and executable backups that must not count
    plain: Path  # .sample (executable), a non-executable commit-msg and an executable backup only
    pathed: Path  # core.hooksPath=.githooks (relative) holding an active hook; .git/hooks is empty
    pathed_missing: Path  # core.hooksPath=.absent: the directory does not exist
    pathed_empty: Path  # core.hooksPath=.nohooks (empty) while .git/hooks/pre-commit is executable: git runs nothing
    framework: Path  # .pre-commit-config.yaml + the pre-commit shim in .git/hooks
    fake_framework: Path  # .pre-commit-config.yaml but the hook is not the framework's shim
    many: Path  # 15 hooks and 14 framework ids: the listings are capped
    outside: Path  # core.hooksPath=<absolute dir outside the repo>
    tilde: Path  # core.hooksPath=~/hk
    notrepo: Path
    parent: Path  # not a repo; holds sub/ (hooked) and mid/repo (hooked) and mid/plain
    wt_main: Path  # a repo with a linked worktree
    wt: Path


@pytest.fixture(scope="module")
def world(tmp_path_factory: pytest.TempPathFactory) -> World:
    root = tmp_path_factory.mktemp("d2")
    home = root / "home"
    (home / "hk").mkdir(parents=True)
    _hook(home / "hk" / "pre-commit")

    hooked = _repo(root / "hooked")
    _hook(hooked / ".git/hooks/pre-commit")
    _hook(hooked / ".git/hooks/commit-msg")
    _hook(hooked / ".git/hooks/pre-push.sample")  # executable but a sample
    _hook(hooked / ".git/hooks/prepare-commit-msg", mode=0o644)  # not executable: git does not run it
    _hook(hooked / ".git/hooks/pre-commit.pre-pii-20260925")  # executable backups, as ~/projects/analise keeps them:
    _hook(hooked / ".git/hooks/pre-push.bak")  # git runs only the githooks(5) names, so these never run

    plain = _repo(root / "plain")
    _hook(plain / ".git/hooks/pre-commit.sample")
    _hook(plain / ".git/hooks/commit-msg", mode=0o644)
    (plain / ".git/hooks/old.d").mkdir()  # a directory is executable to the OS but git runs no directory
    (plain / ".git/hooks/post-commit").mkdir()  # not even one named like a hook
    _hook(plain / ".git/hooks/pre-commit.bak")  # executable, but not a hook name: a repo holding only this has no hook

    pathed = _repo(root / "pathed")
    _git(pathed, "config", "core.hooksPath", ".githooks")
    _hook(pathed / ".githooks/pre-commit")

    pathed_missing = _repo(root / "pathed_missing")
    _git(pathed_missing, "config", "core.hooksPath", ".absent")

    pathed_empty = _repo(root / "pathed_empty")
    _git(pathed_empty, "config", "core.hooksPath", ".nohooks")
    (pathed_empty / ".nohooks").mkdir()
    _hook(pathed_empty / ".git/hooks/pre-commit")

    framework = _repo(root / "framework")
    _hook(framework / ".git/hooks/pre-commit", "#!/usr/bin/env python3\n# File generated by pre-commit: https://pre-commit.com\n")
    (framework / ".pre-commit-config.yaml").write_text(
        "repos:\n- repo: local\n  hooks:\n  - id: black\n    name: black\n  - id: ruff-check\n  - id: end-of-file-fixer\n")

    fake = _repo(root / "fake_framework")
    _hook(fake / ".git/hooks/pre-commit")
    (fake / ".pre-commit-config.yaml").write_text("repos:\n- repo: local\n  hooks:\n  - id: black\n")

    many = _repo(root / "many")
    for name in MANY_HOOKS:  # real hook names: an invented name is not a hook and must not fill the listing
        _hook(many / f".git/hooks/{name}")
    _hook(many / ".git/hooks/pre-commit", "#!/usr/bin/env python3\n# File generated by pre-commit\n")
    (many / ".pre-commit-config.yaml").write_text(
        "repos:\n- repo: local\n  hooks:\n" + "".join(f"  - id: check{i:02d}\n" for i in range(1, 15)))

    outside = _repo(root / "outside")
    shared = root / "shared-hooks"
    _hook(shared / "pre-commit")
    _git(outside, "config", "core.hooksPath", str(shared))

    tilde = _repo(root / "tilde")
    _git(tilde, "config", "core.hooksPath", "~/hk", home=home)

    notrepo = root / "notrepo"
    notrepo.mkdir()

    parent = root / "parent"
    parent.mkdir()
    sub = _repo(parent / "sub")
    _hook(sub / ".git/hooks/pre-commit")
    mid = parent / "mid"
    mid.mkdir()
    inner = _repo(mid / "repo")
    _hook(inner / ".git/hooks/pre-commit")
    _repo(mid / "plain")

    wt_main = _repo(root / "wt_main")
    _hook(wt_main / ".git/hooks/pre-commit")
    _git(wt_main, "-c", "user.name=t", "-c", "user.email=t@example.com", "commit", "--allow-empty", "-q", "-m", "i")
    wt = root / "wt"
    _git(wt_main, "worktree", "add", "-q", str(wt))

    return World(root, home, hooked, plain, pathed, pathed_missing, pathed_empty, framework, fake, many, outside, tilde, notrepo, parent, wt_main, wt)


def _payload(command: str, cwd: Path | None) -> dict:
    payload = {"hook_event_name": "PreToolUse", "tool_name": "Bash", "session_id": "hooks-contract-s25-d2",
               "tool_input": {"command": command}}
    if cwd is not None:
        payload["cwd"] = str(cwd)
    return payload


def _verdict(w: World, command: str, cwd: Path, *, home: Path | None = None) -> tuple[bool, str]:
    result = run_hook_sh(SCRIPT, _payload(command, cwd), home=home or w.home, extra_env=GIT_ENV)
    assert result.rc == 0, result.stderr_raw
    hso = (result.json or {}).get("hookSpecificOutput", {})
    return hso.get("permissionDecision") == "deny", hso.get("permissionDecisionReason", "")


def _denied(w: World, command: str, cwd: Path) -> bool:
    return _verdict(w, command, cwd)[0]


# Every spelling that skips the hooks of the repository the command runs in (cwd = a repo with active hooks).
DENIED = {
    "commit --no-verify": "git commit --no-verify -m x",
    "commit -n": "git commit -n -m x",
    "commit -nm cluster": "git commit -nm x",
    "commit -an cluster": "git commit -an -m x",
    "commit -n after the message": "git commit -m x -n",
    "commit -n after an option with a value": "git commit -C HEAD -n",
    "commit --no-verif abbreviation": "git commit --no-verif -m x",
    "commit --no-veri shortest abbreviation": "git commit --no-veri -m x",
    "commit attached message then -n": "git commit -mx -n",
    "merge --squash is a flag, not a value": "git merge --squash --no-verify topic",
    "commit quoted flag": "git commit '--no-verify' -m x",
    "commit quoted -n": 'git commit "-n" -m x',
    "commit quoted subcommand": "git 'commit' -n -m x",
    "commit long option with value then -n": "git commit --author=a -n -m x",
    "commit message option then --no-verify": "git commit --message=-n --no-verify",
    "push --no-verify": "git push --no-verify origin main",
    "merge --no-verify": "git merge --no-verify topic",
    "am --no-verify": "git am --no-verify p.mbox",
    "am -n": "git am -n p.mbox",
    "rebase --no-verify": "GIT_DESTRUCTIVE_OK=1 git rebase --no-verify main",  # the destructive token never waives this check
    "cherry-pick --no-verify": "git cherry-pick --no-verify abc123",
    "-c core.hooksPath": "git -c core.hooksPath=/dev/null commit -m x",
    "-c core.hooksPath other value": "git -c core.hooksPath=/tmp/elsewhere push",
    "-c hookspath any case": "git -c CORE.HOOKSPATH=/dev/null commit -m x",
    "-c core.hooksPath on any subcommand": "git -c core.hooksPath=/dev/null merge topic",
    "-c after another -c": "git -c user.name=t -c core.hooksPath=/dev/null commit -m x",
    "--config-env core.hooksPath": "git --config-env=core.hooksPath=HOOKS commit -m x",
    "GIT_CONFIG_KEY_0": "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git commit -m x",
    "GIT_CONFIG_KEY_n": "GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=user.name GIT_CONFIG_VALUE_0=t GIT_CONFIG_KEY_1=core.hookspath "
                        "GIT_CONFIG_VALUE_1=/dev/null git commit -m x",
    "GIT_CONFIG_PARAMETERS": "GIT_CONFIG_PARAMETERS=\"'core.hooksPath=/dev/null'\" git commit -m x",
    "SKIP": "SKIP=flake8 git commit -m x",
    "SKIP list": "SKIP=black,ruff-check git commit -m x",
    "PRE_COMMIT_ALLOW_NO_CONFIG": "PRE_COMMIT_ALLOW_NO_CONFIG=1 git commit -m x",
    "HUSKY=0": "HUSKY=0 git commit -m x",
    "HUSKY=0 pull": "HUSKY=0 git pull",
    "SKIP revert": "SKIP=black git revert HEAD",
    "SKIP push": "SKIP=black git push origin main",
    "export SKIP": "export SKIP=black; git commit -m x",
    "export then &&": "export HUSKY=0 && git commit -m x",
    "bare assignment on its own line": "SKIP=black\ngit commit -m x",
    "bare assignment then ;": "SKIP=black; git commit -m x",
    "env wrapper": "env SKIP=black git commit -m x",
    "assignment after a reserved word": "if true; then SKIP=black git commit -m x; fi",
    "env wrapper after an option": "env -i SKIP=black git commit -m x",
    "assignment before a wrapper": "SKIP=black timeout 5 git commit -m x",
    "timeout wrapper": "timeout 5 git commit -n -m x",
    "nohup wrapper": "nohup git commit --no-verify -m x",
    "absolute git": "/usr/bin/git commit --no-verify -m x",
    "bash -c": "bash -c 'git commit --no-verify -m x'",
    "sh -c double quotes": 'sh -c "git commit -n -m x"',
    "bash -c -c core.hooksPath": "bash -c 'git -c core.hooksPath=/dev/null commit -m x'",
    "SKIP inherited by bash -c": "SKIP=black bash -c 'git commit -m x'",
    "export inherited by bash -c": "export SKIP=black; bash -c 'git commit -m x'",
    "env inherited by the nested of a nested": "HUSKY=0 bash -c \"sh -c 'git commit -m x'\"",
    "eval": 'eval "git commit --no-verify -m x"',
    "heredoc into a shell": "bash <<'EOF'\ngit commit --no-verify -m x\nEOF",
    "command substitution": "echo $(git commit -n -m x)",
    "after &&": "echo hi && git commit --no-verify -m x",
    "after a pipe": "echo hi | git commit --no-verify -F -",
    "in a subshell": "(git commit -n -m x)",
    "xargs": "echo m | xargs git commit -n -m",
    "interpreter os.system": "python3 -c \"import os; os.system('git commit -n -m x')\"",
    "with the destructive token only": "GIT_DESTRUCTIVE_OK=1 git commit --no-verify -m x",
    "token only as data": "echo 'GIT_HOOKS_BYPASS_OK=1'; git commit --no-verify -m x",
    "token inside the message": "git commit --no-verify -m 'GIT_HOOKS_BYPASS_OK=1'",
}

# Not a bypass: another option that shares the letter, a value that looks like one, or no hook at stake.
ALLOWED = {
    "plain commit": "git commit -m x",
    "amend": "git commit --amend -m x",
    "commit -a -m": "git commit -am x",
    "push -n is a dry run": "git push -n origin main",
    "push --dry-run": "git push --dry-run origin main",
    "merge -n is no-stat": "git merge -n topic",
    "rebase -n is no-stat": "GIT_DESTRUCTIVE_OK=1 git rebase -n main",
    "cherry-pick -n is no-commit": "git cherry-pick -n abc123",
    "revert -n is no-commit": "git revert -n HEAD",
    "log -n": "git log -n 5",
    "commit -m -n (the message)": "git commit -m -n",
    "commit -mn (message n)": "git commit -mn",
    "commit -m x -- -n (a pathspec)": "git commit -m x -- -n",
    "commit --message=-n": "git commit --message=-n",
    "commit -F -n (a file name)": "git commit -F -n",
    "commit -c -n (a commit named -n)": "git commit -c -n",
    "commit -C -n (a commit named -n)": "git commit -C -n",
    "commit -t -n (a template named -n)": "git commit -t -n",
    "commit -am -n (the message)": "git commit -am -n",
    "GIT_CONFIG_PARAMETERS for another key": "GIT_CONFIG_PARAMETERS=\"'user.name=t'\" git commit -m x",
    "commit --no-verify in the message": "git commit -m 'use --no-verify only if told'",
    "commit -n in the message": "git commit -m 'git commit -n'",
    "echoing the flag": "echo git commit --no-verify",
    "grep for the flag": "grep -n no-verify README.md",
    "comment": "git commit -m x # --no-verify",
    "status": "git status",
    "-c with another key": "git -c user.name=t -c core.pager=cat commit -m x",
    "-c key that merely contains hooksPath": "git -c my.core.hooksPath=1 commit -m x",
    "config read": "git config --get core.hooksPath",
    "--no-ver is ambiguous (no-verbose)": "git commit --no-ver -m x",
    "SKIP only for a command that runs no hook": "SKIP=black git status",
    "HUSKY=0 on log": "HUSKY=0 git log",
    "PRE_COMMIT_ALLOW_NO_CONFIG on diff": "PRE_COMMIT_ALLOW_NO_CONFIG=1 git diff",
    "SKIP empty": "SKIP= git commit -m x",
    "HUSKY=1": "HUSKY=1 git commit -m x",
    "PRE_COMMIT_ALLOW_NO_CONFIG=0": "PRE_COMMIT_ALLOW_NO_CONFIG=0 git commit -m x",
    "GIT_CONFIG_KEY other key": "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=user.name GIT_CONFIG_VALUE_0=t git commit -m x",
    "assignment scoped to another command": "SKIP=black true; git commit -m x",
    "export inside a nested shell does not leak": "bash -c 'export SKIP=black'; git commit -m x",
    "assignment as data": "echo SKIP=black; git commit -m x",
    "export inside a command substitution does not leak": "x=$(export SKIP=black); git commit -m x",
    "an argument that looks like an assignment": "git commit -m SKIP=black",
    "token for the bypass": "GIT_HOOKS_BYPASS_OK=1 git commit --no-verify -m x",
    "token for the bypass, nested": "bash -c 'GIT_HOOKS_BYPASS_OK=1 git commit -n -m x'",
    "token for the bypass, on the outer shell": "GIT_HOOKS_BYPASS_OK=1 bash -c 'git commit -n -m x'",
    "both tokens": "GIT_DESTRUCTIVE_OK=1 GIT_HOOKS_BYPASS_OK=1 git commit --no-verify -m x",
    "token for the bypass with -c core.hooksPath": "GIT_HOOKS_BYPASS_OK=1 git -c core.hooksPath=/dev/null commit -m x",
}


@pytest.mark.parametrize("command", DENIED.values(), ids=DENIED.keys())
def test_the_bypass_is_denied_where_the_repo_has_active_hooks(world: World, command: str) -> None:
    denied, reason = _verdict(world, command, world.hooked)
    assert denied, f"{command!r} skips hooks and was allowed"
    assert "GIT_HOOKS_BYPASS_OK=1" in reason, reason


@pytest.mark.parametrize("command", ALLOWED.values(), ids=ALLOWED.keys())
def test_what_does_not_skip_hooks_is_allowed(world: World, command: str) -> None:
    denied, reason = _verdict(world, command, world.hooked)
    assert not denied, f"{command!r} was denied: {reason}"


@pytest.mark.parametrize("command", DENIED.values(), ids=DENIED.keys())
@pytest.mark.parametrize("where", ["plain", "pathed_empty", "pathed_missing", "notrepo"])
def test_a_repo_without_active_hooks_is_left_alone(world: World, command: str, where: str) -> None:
    assert not _denied(world, command, getattr(world, where)), f"{command!r} denied in {where}, which has no active hook"


def test_the_destructive_token_does_not_waive_the_bypass_and_the_bypass_token_does_not_waive_a_destructive(world: World) -> None:
    assert _denied(world, "GIT_DESTRUCTIVE_OK=1 git commit --no-verify -m x", world.hooked)
    assert _denied(world, "GIT_HOOKS_BYPASS_OK=1 git reset --hard HEAD", world.hooked)
    assert not _denied(world, "GIT_HOOKS_BYPASS_OK=1 GIT_DESTRUCTIVE_OK=1 git reset --hard HEAD", world.hooked)
    assert not _denied(world, "GIT_DESTRUCTIVE_OK=1 git reset --hard HEAD", world.hooked)


def test_a_bypass_and_a_destructive_in_one_command_are_both_judged(world: World) -> None:
    assert _denied(world, "git commit --no-verify -m x && git reset --hard", world.plain)  # destructive, no hook at stake
    assert _denied(world, "git commit -n -m x && git status", world.hooked)


# ── which repository is the target ────────────────────────────────────────────────────────────────────────────────


def test_dash_C_points_the_check_at_the_other_repo(world: World) -> None:
    assert _denied(world, f"git -C {world.hooked} commit -n -m x", world.plain)
    assert _denied(world, f"git -C {world.hooked} commit -n -m x", world.notrepo)
    assert not _denied(world, f"git -C {world.plain} commit -n -m x", world.hooked)  # cwd has hooks, the target has none


def test_dash_C_relative_and_chained(world: World) -> None:
    assert _denied(world, "git -C sub commit -n -m x", world.parent)
    assert _denied(world, "git -C mid -C repo commit -n -m x", world.parent)
    assert not _denied(world, "git -C mid -C plain commit -n -m x", world.parent)
    assert not _denied(world, f"git -C mid -C {world.plain} commit -n -m x", world.parent)  # an absolute -C resets the chain
    assert _denied(world, f"git -C {world.plain} -C {world.hooked} commit -n -m x", world.notrepo)


def test_dash_C_that_does_not_resolve_falls_back_to_the_cwd(world: World) -> None:
    assert _denied(world, "git -C /nonexistent/place commit -n -m x", world.hooked)
    assert not _denied(world, "git -C /nonexistent/place commit -n -m x", world.plain)


def test_dash_C_tilde_and_variables_are_expanded(world: World, monkeypatch: pytest.MonkeyPatch) -> None:
    assert _denied(world, "git -C ~/../hooked commit -n -m x", world.notrepo)
    # a variable only the shell knows cannot be resolved: the check falls back to the cwd, not to "no repo"
    assert _denied(world, "HOOKS=/x; git -C $HOOKS commit -n -m x", world.hooked)
    assert not _denied(world, "HOOKS=/x; git -C $HOOKS commit -n -m x", world.plain)
    monkeypatch.setenv("D2_REPO", str(world.hooked))  # one the hook inherits is expanded
    assert _denied(world, "git -C $D2_REPO commit -n -m x", world.notrepo)


def test_cd_moves_the_target(world: World) -> None:
    assert _denied(world, "cd sub && git commit -n -m x", world.parent)
    assert _denied(world, "cd mid && cd repo && git commit -n -m x", world.parent)
    assert _denied(world, "cd mid; git -C repo commit -n -m x", world.parent)
    assert _denied(world, f"cd {world.hooked} && git commit --no-verify -m x", world.plain)
    assert _denied(world, "cd -P sub && git commit -n -m x", world.parent)
    assert not _denied(world, "cd mid/plain && git commit -n -m x", world.parent)
    assert not _denied(world, "cd sub && cd .. && git commit -n -m x", world.parent)  # back out of the repo


def test_cd_without_an_argument_goes_home(world: World) -> None:
    assert _verdict(world, "cd && git commit -n -m x", world.notrepo, home=world.hooked)[0]
    assert not _verdict(world, "cd && git commit -n -m x", world.notrepo, home=world.plain)[0]


def test_cd_inside_a_nested_shell_or_substitution_does_not_leak(world: World) -> None:
    assert not _denied(world, f"bash -c 'cd {world.hooked}'; git commit -n -m x", world.plain)
    assert not _denied(world, f"x=$(cd {world.hooked} && pwd); git commit -n -m x", world.plain)
    assert _denied(world, f"cd {world.hooked}; bash -c 'git commit -n -m x'", world.plain)  # a nested shell inherits it


def test_a_linked_worktree_uses_the_hooks_of_the_main_repo(world: World) -> None:
    denied, reason = _verdict(world, "git commit --no-verify -m x", world.wt)
    assert denied and "pre-commit" in reason, reason


def test_the_payload_without_cwd_falls_back_to_the_process_cwd(world: World) -> None:
    def run(cwd: Path) -> bool:
        proc = subprocess.run(["bash", str(SCRIPT)], input=json.dumps(_payload("git commit -n -m x", None)).encode(),
                              capture_output=True, env={**os.environ, **GIT_ENV, "HOME": str(world.home)}, cwd=cwd, timeout=30, check=False)
        assert proc.returncode == 0, proc.stderr
        return b'"deny"' in proc.stdout

    assert run(world.hooked)
    assert not run(world.plain)


# ── what counts as an ACTIVE hook ─────────────────────────────────────────────────────────────────────────────────


def test_samples_and_non_executable_files_are_not_active_hooks(world: World) -> None:
    assert _shell_mask.active_hooks(str(world.plain)) is None
    found = _shell_mask.active_hooks(str(world.hooked))
    assert found is not None and found.names == ["commit-msg", "pre-commit"], found


def test_core_hookspath_replaces_the_default_directory(world: World) -> None:
    pathed = _shell_mask.active_hooks(str(world.pathed))
    assert pathed is not None and pathed.names == ["pre-commit"] and pathed.hooks_dir.endswith(".githooks"), pathed
    assert _denied(world, "git commit -n -m x", world.pathed)
    assert _shell_mask.active_hooks(str(world.pathed_empty)) is None, "git runs nothing: hooksPath points at an empty dir"
    assert not _denied(world, "git commit -n -m x", world.pathed_empty)


def test_core_hookspath_outside_the_repo_and_with_a_tilde(world: World) -> None:
    out = _verdict(world, "git commit -n -m x", world.outside)
    assert out[0] and str(world.root / "shared-hooks") in out[1], out
    tilde = _verdict(world, "git commit -n -m x", world.tilde)
    assert tilde[0] and "pre-commit" in tilde[1], tilde


def test_the_pre_commit_framework_ids_are_named_only_when_its_shim_is_installed(world: World) -> None:
    reason = _verdict(world, "git commit -n -m x", world.framework)[1]
    assert "framework pre-commit: black, ruff-check, end-of-file-fixer)" in reason and "(+" not in reason, reason
    fake = _verdict(world, "git commit -n -m x", world.fake_framework)[1]
    assert "framework pre-commit" not in fake and "black" not in fake, fake


def test_a_repo_with_a_config_but_no_hook_installed_is_not_active(world: World, tmp_path: Path) -> None:
    repo = _repo(tmp_path / "configonly")
    (repo / ".pre-commit-config.yaml").write_text("repos:\n- repo: local\n  hooks:\n  - id: black\n")
    assert _shell_mask.active_hooks(str(repo)) is None


# ── the deny text ─────────────────────────────────────────────────────────────────────────────────────────────────


def test_the_deny_names_the_form_the_repo_the_hooks_and_the_way_out(world: World) -> None:
    denied, reason = _verdict(world, "git commit --no-verify -m x", world.hooked)
    assert denied
    assert "`git commit --no-verify`" in reason
    assert str(world.hooked) in reason
    assert "hooks ativos em .git/hooks/: commit-msg, pre-commit" in reason
    assert "pre-push" not in reason and "prepare-commit-msg" not in reason  # a sample and a non-executable file
    assert "pre-pii" not in reason and ".bak" not in reason  # executable backups git never runs
    assert "GIT_HOOKS_BYPASS_OK=1" in reason
    assert "diferente de GIT_DESTRUCTIVE_OK=1" in reason and "não exige snapshot" in reason
    assert "\n" not in reason, "the JSON reason is one line"
    assert "(+" not in reason, "nothing was cut from the listing"


def test_the_deny_lists_every_form_of_the_command_and_every_repo(world: World) -> None:
    reason = _verdict(world, "SKIP=black git commit --no-verify -m x", world.hooked)[1]
    assert "`git commit --no-verify` e `SKIP=black` contornam" in reason, reason
    both = _verdict(world, f"git -C {world.hooked} commit -n -m x && git -C {world.pathed} commit -n -m x", world.notrepo)[1]
    assert str(world.hooked) in both and str(world.pathed) in both, both
    one = _verdict(world, f"git -C {world.hooked} commit -n -m x && git -C {world.plain} commit -n -m x", world.notrepo)[1]
    assert str(world.hooked) in one and str(world.plain) not in one, one


def test_the_same_form_twice_is_named_once(world: World) -> None:
    reason = _verdict(world, "git commit -n -m a; git commit -n -m b", world.hooked)[1]
    assert reason.count("`git commit -n`") == 1, reason
    across = _verdict(world, "git -C . commit -n -m a; git commit -n -m b", world.hooked)[1]  # two notes, one repo
    assert across.count("`git commit -n`") == 1, across


def test_the_listings_are_capped(world: World) -> None:
    reason = _verdict(world, "git commit -n -m x", world.many)[1]
    shown = ", ".join(sorted((*MANY_HOOKS, "pre-commit"))[:12])  # 15 hooks: the first 12 by name, then (+3)
    assert f"hooks ativos em .git/hooks/: {shown} (+3)" in reason, reason
    assert "check01" in reason and "check12" in reason and "check13" not in reason and "(+2)" in reason, reason


def test_resolve_main_reads_only_the_bypass_notes(world: World, monkeypatch: pytest.MonkeyPatch,
                                                  capsys: pytest.CaptureFixture[str]) -> None:
    note = _note("git commit -n")
    monkeypatch.setattr(sys, "stdin", io.StringIO(f"unrelated line\n\x1eBLOCK_GIT_NOTE:{note}\nxargs-git:reset\n"))
    assert _shell_mask.resolve_main(str(world.hooked)) == 0
    out = capsys.readouterr().out
    assert out.count("`git commit -n`") == 1 and "``" not in out, out


def test_the_resolver_failing_is_a_loud_deny_not_a_silent_allow(world: World, tmp_path: Path) -> None:
    sandbox = tmp_path / "hooks"
    sandbox.mkdir()
    shutil.copy(SCRIPT, sandbox / "block_git.sh")
    stub = sandbox / "_shell_mask.py"
    stub.write_text(
        "import importlib.util, sys\n"
        f"spec = importlib.util.spec_from_file_location('_real_mask', {str(CLIENT_HOOKS / '_shell_mask.py')!r})\n"
        "real = importlib.util.module_from_spec(spec); sys.modules['_real_mask'] = real; spec.loader.exec_module(real)\n"
        "main = real.main\n"
        "def resolve_main(cwd):\n    raise RuntimeError('boom')\n")
    stub.chmod(0o755)
    result = run_hook_sh(sandbox / "block_git.sh", _payload("git commit --no-verify -m x", world.hooked), home=world.home, extra_env=GIT_ENV)
    hso = (result.json or {}).get("hookSpecificOutput", {})
    assert hso.get("permissionDecision") == "deny" and "não conseguiu verificar" in hso.get("permissionDecisionReason", ""), result.stdout_raw
    ok = run_hook_sh(sandbox / "block_git.sh", _payload("git commit -m x", world.hooked), home=world.home, extra_env=GIT_ENV)
    assert "deny" not in ok.stdout_raw, "a command without a bypass never reaches the resolver"


# ── the masker's notes (no filesystem) ────────────────────────────────────────────────────────────────────────────


def _bypass_notes(command: str) -> list[str]:
    return [n for n in _shell_mask.scan(command).notes if n.startswith(_shell_mask.BYPASS_NOTE)]


def _note(form: str, target: str = "") -> str:
    return f"{_shell_mask.BYPASS_NOTE}{form}{_shell_mask.SEP}{target}"


@pytest.mark.parametrize(
    ("command", "expected"),
    [
        ("git commit --no-verify", [_note("git commit --no-verify")]),
        ("git commit -n", [_note("git commit -n")]),
        ("git commit -m x", []),
        ("git commit -n; git commit -n", [_note("git commit -n")]),
        ("git -C a -C b commit -n", [_note("git commit -n", "a/b")]),
        ("git -C /abs commit -n", [_note("git commit -n", "/abs")]),
        ("git -C /abs -C rel commit -n", [_note("git commit -n", "/abs/rel")]),
        ("git -C /one -C /two commit -n", [_note("git commit -n", "/two")]),
        ("git -C ~ commit -n", [_note("git commit -n", os.path.expanduser("~"))]),
        ("cd x && git commit -n", [_note("git commit -n", "x")]),
        ("cd x && git -C y commit -n", [_note("git commit -n", "x/y")]),
        ("git -c core.hooksPath=/dev/null status", [_note("git -c core.hooksPath=…")]),
        ("git -c core.hooksPath=/dev/null", [_note("git -c core.hooksPath=…")]),
        ("SKIP=a git commit", [_note("SKIP=a")]),
        ("SKIP=a git status", []),
        ("HUSKY=0 git pull", [_note("HUSKY=0")]),
        ("PRE_COMMIT_ALLOW_NO_CONFIG=1 git commit", [_note("PRE_COMMIT_ALLOW_NO_CONFIG=1")]),
        ("GIT_CONFIG_KEY_2=core.hooksPath git commit", [_note("GIT_CONFIG_KEY_2=core.hooksPath")]),
        ("git commit -n --no-verify", [_note("git commit -n")]),
        ("git commit -n -- --no-verify", [_note("git commit -n")]),
        ("git commit -m x -- --no-verify", []),
        ("git commit --message --no-verify", []),
        ("git commit --message=a --no-verify", [_note("git commit --no-verify")]),
        ("git push -n", []),
        ("git am -n", [_note("git am -n")]),
        ("git am -p1 -n", [_note("git am -n")]),
        ("git am -C3 -n", [_note("git am -n")]),
        ("git am -Sn", []),
        ("git commit -un", []),
        ("git commit -S1 -n", [_note("git commit -n")]),
        ("git commit -c HEAD -n", [_note("git commit -n")]),
        ("git commit -tn", []),
        ("git -c", []),  # an option cut short must not crash the scan
        ("git -C", []),
        ("git commit -Sn", []),
        ("git am -Cn", []),
        ("git am -pn", []),
        ("GIT_CONFIG_KEY_12=core.hooksPath git commit", [_note("GIT_CONFIG_KEY_12=core.hooksPath")]),
        ("GIT_CONFIG_KEY_0x=core.hooksPath git commit", []),
        ("GIT_CONFIG_KEY_=core.hooksPath git commit", []),  # git reads GIT_CONFIG_KEY_<n> only
        ("cd x && cd - && git commit -n", [_note("git commit -n", "x")]),
        ("cd a b && git commit -n", [_note("git commit -n")]),  # too many operands: the shell stays where it was
        ("git commit -t x -n", [_note("git commit -n")]),
    ],
)
def test_the_masker_notes_exactly_the_bypass_forms(command: str, expected: list[str]) -> None:
    assert _bypass_notes(command) == expected


def test_a_command_without_a_bypass_adds_no_note_and_no_view_changes() -> None:
    plain = _shell_mask.scan("git status && git log -n 3")
    assert plain.notes == [] and plain.views[0] == "git status && git log -n 3"


# ── no jq ─────────────────────────────────────────────────────────────────────────────────────────────────────────


def test_without_jq_python_still_reads_the_command_and_the_cwd(world: World, tmp_path: Path) -> None:
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    for tool in ("cat", "grep", "sed", "readlink", "dirname", "awk", "python3", "git"):
        found = shutil.which(tool)
        assert found, tool
        (bin_dir / tool).symlink_to(found)
    env = {"PATH": str(bin_dir), "HOME": str(world.home), **GIT_ENV}

    def run(cwd: Path, command: str) -> bool:
        proc = subprocess.run([shutil.which("bash") or "/bin/bash", str(SCRIPT)], input=json.dumps(_payload(command, cwd)).encode(),
                              capture_output=True, env=env, timeout=30, check=False)
        assert proc.returncode == 0, proc.stderr
        return b'"deny"' in proc.stdout

    assert run(world.hooked, "git commit --no-verify -m x")
    assert not run(world.plain, "git commit --no-verify -m x")
    assert not run(world.hooked, "GIT_HOOKS_BYPASS_OK=1 git commit --no-verify -m x")
