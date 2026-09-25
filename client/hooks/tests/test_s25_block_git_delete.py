"""test_s25_block_git_delete.py — S25: git push --delete / -d / :<ref> must be guarded.

Three required cases (team-lead spec):
  1. Without token → DENY  (RED before patch, GREEN after)
  2. With GIT_DESTRUCTIVE_OK=1 token → ALLOW
  3. Control: git push origin main → ALLOW (non-destructive, unaffected)

All forms of remote-delete are covered:
  - git push origin --delete feature/x
  - git push origin -d feature/x
  - git push origin :feature/x  (empty left-side refspec)
"""
from __future__ import annotations

import json
import subprocess
from pathlib import Path

import pytest
from helpers import CLIENT_HOOKS, run_hook_sh, HookResult

SCRIPT = CLIENT_HOOKS / "block_git.sh"


def _bash_payload(command: str) -> dict:
    return {
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "session_id": "hooks-contract-s25",
        "tool_input": {"command": command},
    }


def _is_denied(result: HookResult) -> bool:
    j = result.json or {}
    hso = j.get("hookSpecificOutput", {})
    return hso.get("permissionDecision") in ("deny", "block")


# ── Without token: all three delete forms must be DENIED ─────────────────────

@pytest.mark.parametrize("cmd", [
    "git push origin --delete feature/old-branch",
    "git push origin -d feature/old-branch",
    "git push origin :feature/old-branch",
])
def test_push_delete_without_token_is_denied(cmd: str) -> None:
    """S25: remote-delete commands without GIT_DESTRUCTIVE_OK=1 must be denied."""
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0, f"Script exited {result.rc}: {result.stderr_raw}"
    assert _is_denied(result), (
        f"Expected DENY for {cmd!r} but got ALLOW.\n"
        f"stdout={result.stdout_raw!r}"
    )


# ── With token: same commands must be ALLOWED ─────────────────────────────────

@pytest.mark.parametrize("cmd", [
    "GIT_DESTRUCTIVE_OK=1 git push origin --delete feature/old-branch",
    "GIT_DESTRUCTIVE_OK=1 git push origin -d feature/old-branch",
    "GIT_DESTRUCTIVE_OK=1 git push origin :feature/old-branch",
])
def test_push_delete_with_token_passes(cmd: str) -> None:
    """S25: GIT_DESTRUCTIVE_OK=1 allows the delete through (human decision)."""
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0
    assert not _is_denied(result), (
        f"Token present — expected ALLOW for {cmd!r} but got DENY.\n"
        f"stdout={result.stdout_raw!r}"
    )


# ── Control: ordinary push is FREE (not affected by S25) ─────────────────────

@pytest.mark.parametrize("cmd", [
    "git push origin main",
    "git push origin feature/new-branch",
    "git push",
])
def test_ordinary_push_is_free(cmd: str) -> None:
    """Control: plain git push (no force, no delete) must remain unblocked."""
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0
    assert not _is_denied(result), (
        f"Ordinary push {cmd!r} was blocked — S25 pattern too broad.\n"
        f"stdout={result.stdout_raw!r}"
    )


# ── Regression: existing patterns still work ─────────────────────────────────

def test_push_force_still_denied() -> None:
    """Regression: git push --force must still be denied (pre-existing guard)."""
    result = run_hook_sh(SCRIPT, _bash_payload("git push origin main --force"))
    assert result.rc == 0
    assert _is_denied(result), "git push --force regression: should still be denied"


def test_push_dry_run_not_confused_with_delete() -> None:
    """git push -n (--dry-run) must NOT be caught by the -d delete pattern."""
    result = run_hook_sh(SCRIPT, _bash_payload("git push -n origin main"))
    assert result.rc == 0
    assert not _is_denied(result), (
        "git push -n (dry-run) was incorrectly blocked by -d delete pattern.\n"
        f"stdout={result.stdout_raw!r}"
    )
