# touring-quality:allow-secrets — the fixtures carry the GIT_DESTRUCTIVE_OK=1 bypass token on purpose
"""test_s25_block_git_no_jq.py — a guard that cannot read its input must say so, and still guard.

Until 17/10/2026 `CMD=$(… | jq …)` left CMD empty whenever jq was missing from the PATH (or choked on the
JSON) and `[[ -z "$CMD" ]] && exit 0` ended the hook in silence: every destructive git went through, with
nothing on stderr to say the guard was blind.  Now: stderr names the cause; python3 reads the JSON when jq
cannot (the guard stays complete); when nothing can read it, the raw JSON text is judged coarsely —
destructive git named in it is DENIED, anything else is allowed WITH the warning.
"""
from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

import pytest
from helpers import CLIENT_HOOKS

SCRIPT = CLIENT_HOOKS / "block_git.sh"
BASH = shutil.which("bash") or "/bin/bash"
#: The tools block_git.sh calls by name.  jq and python3 are added (or not) per scenario.
BASE_TOOLS = ("cat", "grep", "sed", "readlink", "dirname", "awk")


def _restricted_path(tmp_path: Path, *, jq: bool, python3: bool) -> str:
    """A PATH holding only the tools the hook needs — jq and python3 on request — so 'jq missing' is real."""
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir(exist_ok=True)
    wanted = [*BASE_TOOLS, *(["jq"] if jq else []), *(["python3"] if python3 else [])]
    for tool in wanted:
        found = shutil.which(tool)
        assert found, f"{tool} is not installed on this machine"
        link = bin_dir / tool
        if not link.exists():
            link.symlink_to(found)
    return str(bin_dir)


def _run(tmp_path: Path, stdin: bytes, *, jq: bool, python3: bool, script: Path = SCRIPT):
    env = {"PATH": _restricted_path(tmp_path, jq=jq, python3=python3), "HOME": str(tmp_path)}
    return subprocess.run([BASH, str(script)], input=stdin, capture_output=True, env=env, timeout=30, check=False)


def _bash_input(command: str) -> bytes:
    return json.dumps({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": command}}).encode()


def _denied(proc: subprocess.CompletedProcess[bytes]) -> bool:
    try:
        hso = json.loads(proc.stdout.decode().strip() or "{}").get("hookSpecificOutput", {})
    except ValueError:
        return False
    return hso.get("permissionDecision") == "deny"


# ── jq missing, python3 present: the guard stays complete ────────────────────────────────────────────


def test_without_jq_python3_reads_the_command_and_the_warning_names_jq(tmp_path: Path) -> None:
    proc = _run(tmp_path, _bash_input("git reset --hard HEAD"), jq=False, python3=True)
    assert proc.returncode == 0
    assert _denied(proc), proc.stdout
    assert "jq" in proc.stderr.decode(), proc.stderr


def test_without_jq_the_nested_forms_are_still_judged(tmp_path: Path) -> None:
    assert _denied(_run(tmp_path, _bash_input("bash -c 'git reset --hard'"), jq=False, python3=True))
    assert not _denied(_run(tmp_path, _bash_input("echo 'git reset --hard'"), jq=False, python3=True))
    assert not _denied(_run(tmp_path, _bash_input("GIT_DESTRUCTIVE_OK=1 git reset --hard"), jq=False, python3=True))


def test_without_jq_a_harmless_command_passes_with_the_warning(tmp_path: Path) -> None:
    proc = _run(tmp_path, _bash_input("git status"), jq=False, python3=True)
    assert proc.returncode == 0 and not _denied(proc)
    assert "jq" in proc.stderr.decode(), "a blind guard that says nothing is the defect"


# ── neither jq nor python3: the raw JSON is judged coarsely ──────────────────────────────────────────


@pytest.mark.parametrize("command", [
    "git reset --hard HEAD",
    "git stash",
    "git clean -fd",
    "git checkout -- .",
    "git push --force origin main",
    "git branch -D topic",
    "git restore src/lib.rs",
    "git rebase main",
    "bash -c 'git reset --hard'",
], ids=lambda c: c[:30])
def test_without_any_reader_destructive_git_in_the_raw_text_is_denied(tmp_path: Path, command: str) -> None:
    proc = _run(tmp_path, _bash_input(command), jq=False, python3=False)
    assert proc.returncode == 0
    assert _denied(proc), f"the raw fallback let {command!r} through"
    reason = json.loads(proc.stdout)["hookSpecificOutput"]["permissionDecisionReason"]
    assert "jq" in reason and "GIT_DESTRUCTIVE_OK=1" in reason, reason
    assert "jq" in proc.stderr.decode()


@pytest.mark.parametrize("command", ["ls -la", "cargo test", "git status", "git log --oneline -5"])
def test_without_any_reader_a_harmless_command_passes_with_the_warning(tmp_path: Path, command: str) -> None:
    proc = _run(tmp_path, _bash_input(command), jq=False, python3=False)
    assert proc.returncode == 0 and not _denied(proc)
    assert "jq" in proc.stderr.decode(), "allowed in silence is exactly the old defect"


def test_without_any_reader_the_token_in_the_raw_text_allows(tmp_path: Path) -> None:
    proc = _run(tmp_path, _bash_input("GIT_DESTRUCTIVE_OK=1 git reset --hard"), jq=False, python3=False)
    assert proc.returncode == 0 and not _denied(proc)
    assert "jq" in proc.stderr.decode()


# ── jq present but the input is not readable JSON ────────────────────────────────────────────────────


def test_unreadable_json_is_not_a_silent_pass(tmp_path: Path) -> None:
    proc = _run(tmp_path, b"this is not json: git reset --hard", jq=True, python3=False)
    assert _denied(proc), proc.stdout
    assert "jq" in proc.stderr.decode()
    benign = _run(tmp_path, b"this is not json", jq=True, python3=False)
    assert benign.returncode == 0 and not _denied(benign)
    assert benign.stderr, "unreadable input must be reported, not swallowed"


# ── controls: the legitimate quiet paths stay quiet ──────────────────────────────────────────────────


def test_a_valid_payload_without_a_command_is_silent(tmp_path: Path) -> None:
    payload = json.dumps({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {}}).encode()
    proc = _run(tmp_path, payload, jq=True, python3=True)
    assert proc.returncode == 0 and not proc.stdout and not proc.stderr


def test_with_everything_present_nothing_is_said_on_stderr(tmp_path: Path) -> None:
    proc = _run(tmp_path, _bash_input("git status"), jq=True, python3=True)
    assert proc.returncode == 0 and not proc.stdout and not proc.stderr, proc.stderr


# ── the masker missing is loud too ───────────────────────────────────────────────────────────────────


def test_missing_masker_warns_and_falls_back_to_the_raw_command(tmp_path: Path) -> None:
    lone = tmp_path / "hooks"
    lone.mkdir()
    shutil.copy(SCRIPT, lone / "block_git.sh")  # deliberately NOT copying _shell_mask.py
    proc = _run(tmp_path, _bash_input("git reset --hard"), jq=True, python3=True, script=lone / "block_git.sh")
    assert _denied(proc)
    assert "mascarador" in proc.stderr.decode(), proc.stderr
