"""test_s25_block_git_data_masking.py — S48 achado-ao-vivo: shell-data masking.

Problem: block_git.sh was matching git command text that appeared inside
heredoc bodies (e.g. a Python script that cat-heredocs a Rust test containing
"git clean -fd") — those strings are data, not commands the shell executes.

Fix: MASKED_CMD strips heredoc bodies and quoted literals before pattern matching.
Reference implementation: touring_ceg::gateway::gate::mask_shell_data
  (crates/touring-ceg/src/gateway/gate.rs, commit S24, tests s24_shell_data_tests)

Test cases (per team-lead spec):
  test_heredoc_body_is_not_a_git_command  — was DENIED before fix, now ALLOWED
  test_quoted_git_text_is_not_a_command   — echo "git reset --hard" → ALLOWED
  test_real_git_clean_still_denied        — control: bare git clean -fd → DENIED
  test_git_before_heredoc_still_denied    — control: real cmd before <<EOF → DENIED
"""
from __future__ import annotations

from helpers import CLIENT_HOOKS, HookResult, run_hook_sh

SCRIPT = CLIENT_HOOKS / "block_git.sh"


def _bash_payload(command: str) -> dict:
    return {
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "session_id": "hooks-contract-s25-mask",
        "tool_input": {"command": command},
    }


def _is_denied(result: HookResult) -> bool:
    j = result.json or {}
    hso = j.get("hookSpecificOutput", {})
    return hso.get("permissionDecision") in ("deny", "block")


# ── Required test 1: heredoc body must NOT trigger the guard ─────────────────

def test_heredoc_body_is_not_a_git_command() -> None:
    """Heredoc body containing 'git clean -fd' is data, not a command.

    Before the mask fix this was DENIED (false positive). After fix: ALLOWED.
    Simulates: Python script that cat-heredocs a Rust test with git command text.
    """
    cmd = (
        "cat > /tmp/test_git_ops.rs <<'EOF'\n"
        "// This test verifies git clean -fd removes untracked files\n"
        "fn test_git_clean() { assert!(run_git_clean_fd()); }\n"
        "EOF"
    )
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0
    assert not _is_denied(result), (
        "Heredoc body with 'git clean -fd' text was incorrectly DENIED.\n"
        "This is data inside a cat-heredoc, not a git command.\n"
        f"stdout={result.stdout_raw!r}"
    )


def test_heredoc_without_quotes_body_is_not_a_command() -> None:
    """Unquoted heredoc (<<EOF) body is also data — must not be matched."""
    cmd = (
        "cat > patch.py <<EOF\n"
        "# Run: git reset --hard HEAD~1 to undo\n"
        "print('done')\n"
        "EOF"
    )
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0
    assert not _is_denied(result), (
        "Unquoted heredoc body with 'git reset --hard' text was incorrectly DENIED.\n"
        f"stdout={result.stdout_raw!r}"
    )


# ── Required test 2: quoted git text must NOT trigger the guard ──────────────

def test_quoted_git_text_is_not_a_command() -> None:
    """echo \"git reset --hard\" — the git command is inside a double-quoted string.

    The shell executes echo, not git. Must be ALLOWED.
    """
    cmd = 'echo "git reset --hard"'
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0
    assert not _is_denied(result), (
        "echo \"git reset --hard\" was incorrectly DENIED.\n"
        "The git command is quoted data, not a real invocation.\n"
        f"stdout={result.stdout_raw!r}"
    )


def test_single_quoted_git_text_is_not_a_command() -> None:
    """echo 'git clean -fd' — single-quoted, nothing inside executes. Must be ALLOWED."""
    cmd = "echo 'git clean -fd is what you need'"
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0
    assert not _is_denied(result), (
        "Single-quoted 'git clean -fd' was incorrectly DENIED.\n"
        f"stdout={result.stdout_raw!r}"
    )


# ── Required control 1: real git clean -fd must still be DENIED ──────────────

def test_real_git_clean_still_denied() -> None:
    """Control: a bare 'git clean -fd' command (no quotes, no heredoc) must still be DENIED."""
    result = run_hook_sh(SCRIPT, _bash_payload("git clean -fd"))
    assert result.rc == 0
    assert _is_denied(result), (
        "REGRESSION: bare 'git clean -fd' was no longer denied after mask fix.\n"
        f"stdout={result.stdout_raw!r}"
    )


# ── Required control 2: git command BEFORE heredoc is still DENIED ───────────

def test_git_before_heredoc_still_denied() -> None:
    """Control: git clean -fd on first line (outside heredoc) must still be DENIED.

    The heredoc starts on a later line; line 1 is a real command.
    """
    cmd = (
        "git clean -fd\n"
        "cat > f <<EOF\n"
        "some content\n"
        "EOF"
    )
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0
    assert _is_denied(result), (
        "REGRESSION: 'git clean -fd' before a heredoc was no longer denied.\n"
        f"stdout={result.stdout_raw!r}"
    )


# ── Edge cases ────────────────────────────────────────────────────────────────

def test_command_substitution_in_dquote_is_not_masked() -> None:
    """$(git stash) inside a double-quoted string executes — must still be DENIED."""
    cmd = 'echo "saving: $(git stash)"'
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0
    assert _is_denied(result), (
        "$(git stash) inside double quotes executes and must be DENIED.\n"
        f"stdout={result.stdout_raw!r}"
    )


def test_heredoc_with_token_in_body_does_not_bypass() -> None:
    """GIT_DESTRUCTIVE_OK=1 inside a heredoc body must NOT bypass the guard.

    The token only counts when it appears as a real shell variable assignment
    in the command being executed, not as text data in a heredoc.
    """
    cmd = (
        "git clean -fd\n"
        "cat > /tmp/notes.txt <<'EOF'\n"
        "To override, set: GIT_DESTRUCTIVE_OK=1 git clean -fd\n"
        "EOF"
    )
    result = run_hook_sh(SCRIPT, _bash_payload(cmd))
    assert result.rc == 0
    assert _is_denied(result), (
        "Token inside heredoc body bypassed the guard — masking must cover the token too.\n"
        f"stdout={result.stdout_raw!r}"
    )
