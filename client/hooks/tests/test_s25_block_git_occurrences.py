"""test_s25_block_git_occurrences.py — every git occurrence is judged on its own (REGRA #11 v2).

Two defects of one class, found 26/09/2026:

* ``git stash list`` was DENIED. The rule matched any ``git stash``, but the protocol lists
  ``stash list`` (and ``show``) under LEITURA — "livres, sempre". A guard that blocks a read the
  constitution frees teaches people to reach for the bypass token.
* ``git restore --staged a && git restore b`` was ALLOWED. The restore rule asked "is there a
  restore?" and "is there a --staged?" over the WHOLE command, so one harmless unstage laundered a
  destructive worktree restore in the same line (the file even documented it as a trade-off).

The fix judges each occurrence: a stash passes only as ``list``/``show``; a restore passes only
with ``--staged``/``-S`` and without ``--worktree``/``-W``.

``BLOCK_GIT_UNDER_TEST`` points the suite at a candidate (``.next``) before the atomic swap.
"""
from __future__ import annotations

import os
from pathlib import Path

import pytest
from helpers import CLIENT_HOOKS, HookResult, run_hook_sh

SCRIPT = Path(os.environ.get("BLOCK_GIT_UNDER_TEST", str(CLIENT_HOOKS / "block_git.sh")))


def _decision(command: str) -> bool:
    result: HookResult = run_hook_sh(
        SCRIPT,
        {
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "session_id": "hooks-contract-s25-occurrences",
            "tool_input": {"command": command},
        },
    )
    hso = (result.json or {}).get("hookSpecificOutput", {})
    return hso.get("permissionDecision") in ("deny", "block")


@pytest.mark.parametrize("cmd", ["git stash list", "git stash show -p stash@{0}", "git -C . status && git stash list"])
def test_stash_reads_are_free(cmd: str) -> None:
    assert not _decision(cmd), f"LEITURA per REGRA #11 v2, must pass: {cmd}"


@pytest.mark.parametrize(
    "cmd",
    [
        "git stash",
        "git stash pop",
        "git stash push -m wip",
        "git stash drop stash@{0}",
        "git stash -p",
        "git stash list && git stash pop",
        "git stash list; git stash drop",
        'echo "$(git stash)"',
    ],
)
def test_stash_mutations_are_denied(cmd: str) -> None:
    assert _decision(cmd), f"mutating stash must be denied: {cmd}"


def test_stash_mutation_with_token_passes() -> None:
    assert not _decision("GIT_DESTRUCTIVE_OK=1 git stash pop")


@pytest.mark.parametrize(
    "cmd",
    [
        "git restore --staged foo && git restore bar",
        "git restore -S foo; git restore .",
        "git restore --staged --worktree foo",
        "git restore -S -W foo",
    ],
)
def test_one_unstage_does_not_launder_a_worktree_restore(cmd: str) -> None:
    assert _decision(cmd), f"a worktree restore hides behind an unstage: {cmd}"


@pytest.mark.parametrize("cmd", ["git restore --staged foo", "git restore -S foo bar", "git restore --staged a && git status"])
def test_pure_unstage_passes(cmd: str) -> None:
    assert not _decision(cmd), f"unstage only (ADITIVA), must pass: {cmd}"


def test_a_name_that_merely_starts_with_stash_is_not_a_stash() -> None:
    assert not _decision("git stashboard --help")


def test_masking_works_when_invoked_through_a_symlink(tmp_path: Path) -> None:
    """Claude Code runs ~/.claude/hooks/block_git.sh — a SYMLINK. `dirname ${BASH_SOURCE[0]}` is the
    symlink's directory, where _shell_mask.py does not exist, so the guard silently fell back to the raw
    command: quoted text was never masked in production (26/09/2026). Every other test here invoked the
    repo path and passed."""
    link = tmp_path / "block_git.sh"
    link.symlink_to(SCRIPT.resolve())
    quoted = 'tool add S25b "block_git: git stash pop is now judged per occurrence"'
    result: HookResult = run_hook_sh(
        link,
        {"hook_event_name": "PreToolUse", "tool_name": "Bash", "session_id": "hooks-contract-s25-symlink",
         "tool_input": {"command": quoted}},
    )
    assert ((result.json or {}).get("hookSpecificOutput") or {}).get("permissionDecision") != "deny", result.stdout
    real = run_hook_sh(
        link,
        {"hook_event_name": "PreToolUse", "tool_name": "Bash", "session_id": "hooks-contract-s25-symlink",
         "tool_input": {"command": "true || git stash pop"}},
    )
    assert ((real.json or {}).get("hookSpecificOutput") or {}).get("permissionDecision") == "deny", "control"
