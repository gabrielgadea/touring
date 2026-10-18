"""test_s25_block_git_restore.py — RED→GREEN tests for git restore (REGRA #21 fix).

Context:
    The old pattern used a lookahead `(?!--staged)` which grep -E does NOT support.
    This meant `git restore <path>` was NEVER actually denied (pre-existing bug).
    Fixed in block_git.sh: two-condition check (match 'git restore' AND NOT '--staged').

Tests:
    RED group: `git restore src/foo.rs` MUST be denied (was silently allowed before fix).
    GREEN group: `git restore --staged src/foo.rs` MUST be allowed.
"""
from __future__ import annotations

import pytest
from helpers import CLIENT_HOOKS, HookResult, run_hook_sh

SCRIPT = CLIENT_HOOKS / "block_git.sh"


def _bash_payload(command: str) -> dict:
    return {
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "session_id": "hooks-contract-s25-restore",
        "tool_input": {"command": command},
    }


def _is_denied(result: HookResult) -> bool:
    j = result.json or {}
    hso = j.get("hookSpecificOutput", {})
    return hso.get("permissionDecision") in ("deny", "block")


# ── git restore (working tree): DEVE ser negado ───────────────────────────────
# These were FALSE NEGATIVEs (never fired) with the old lookahead pattern.

@pytest.mark.parametrize("cmd", [
    "git restore src/foo.rs",
    "git restore .",
    "git restore src/a.rs src/b.rs",
    "git restore --source HEAD~1 src/foo.rs",
])
def test_restore_without_staged_is_denied(cmd: str) -> None:
    """REGRA #21 fix: git restore (working tree) must be denied — was a silent false negative."""
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0, f"Script exited {result.rc}: {result.stderr_raw}"
    assert _is_denied(result), (
        f"git restore without --staged was NOT denied (lookahead bug still present): "
        f"cmd={cmd!r}, json={result.json}"
    )


# ── git restore --staged: DEVE ser permitido ─────────────────────────────────

@pytest.mark.parametrize("cmd", [
    "git restore --staged src/foo.rs",
    "git restore --staged .",
    "git restore --staged src/a.rs src/b.rs",
])
def test_restore_staged_is_allowed(cmd: str) -> None:
    """git restore --staged only unstages — must not be denied."""
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0, f"Script exited {result.rc}: {result.stderr_raw}"
    assert not _is_denied(result), (
        f"git restore --staged was incorrectly denied: cmd={cmd!r}, json={result.json}"
    )


# ── Token GIT_DESTRUCTIVE_OK=1 bypassa o guard ───────────────────────────────

def test_restore_with_token_is_allowed() -> None:
    """GIT_DESTRUCTIVE_OK=1 git restore src/foo.rs → ALLOW (conscious bypass)."""
    cmd = "GIT_DESTRUCTIVE_OK=1 git restore src/foo.rs"
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0, f"Script exited {result.rc}: {result.stderr_raw}"
    assert not _is_denied(result), (
        f"Token should bypass guard: json={result.json}"
    )
