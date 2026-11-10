"""Tests for scripts/block_git_divergence.py — the shell-guard-vs-Rust-policy comparison.

A stub guard stands in for block_git.sh (the real one is frozen and exercised by
client/hooks/tests): it denies a command containing `git reset`, and `git commit -n` only
when the working directory holds an executable pre-commit hook — the same shape of
dependence the real guard has on the repository.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "block_git_divergence.py"

STUB_GUARD = r"""#!/usr/bin/env bash
payload=$(cat)
cmd=$(printf '%s' "$payload" | python3 -c 'import json,sys; print(json.load(sys.stdin)["tool_input"]["command"])')
deny() { printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny"}}\n'; exit 0; }
case "$cmd" in
  *"git reset"*) deny ;;
  *"git commit -n"*) [[ -x .git/hooks/pre-commit ]] && deny ;;
esac
exit 0
"""


def _run(tmp_path: Path, commands: list[tuple[str, str]], justified: list[list[str]] | None = None) -> dict:
    guard = tmp_path / "guard.sh"
    guard.write_text(STUB_GUARD)
    baseline = tmp_path / "baseline.jsonl"
    baseline.write_text(
        "".join(json.dumps({"cmd": cmd, "pipeline": {"v": rust}}) + "\n" for cmd, rust in commands)
    )
    differences = tmp_path / "justified.jsonl"
    differences.write_text("".join(json.dumps(row) + "\n" for row in justified or []))
    proc = subprocess.run(
        [
            sys.executable,
            str(SCRIPT),
            "--baseline",
            str(baseline),
            "--justified",
            str(differences),
            "--guard",
            str(guard),
            "--json",
        ],
        capture_output=True,
        text=True,
        check=True,
        timeout=120,
    )
    return json.loads(proc.stdout)


def test_classifies_each_owner_of_a_command(tmp_path: Path) -> None:
    report = _run(
        tmp_path,
        [
            ("ls -la", "allow"),  # nobody objects
            ("rm -rf build", "block"),  # Rust only: the guard does not own deletes
            ("git reset --hard", "allow"),  # shell only: git left the Rust policy
            ("echo done && git reset --hard", "allow"),  # still the guard's
        ],
    )
    by_cmd = {row["cmd"]: row["class"] for row in report["rows"]}
    assert by_cmd["ls -la"] == "agree"
    assert by_cmd["rm -rf build"] == "rust-only (guard allows)"
    assert by_cmd["git reset --hard"] == "shell-only (git destructive; Rust allows)"
    assert by_cmd["echo done && git reset --hard"] == "shell-only (git destructive; Rust allows)"
    assert report["counts"] == {
        "agree": 1,
        "rust-only (guard allows)": 1,
        "shell-only (git destructive; Rust allows)": 2,
    }


def test_the_hooks_dependence_is_measured_in_both_repositories(tmp_path: Path) -> None:
    report = _run(tmp_path, [("git commit -n -m x", "allow"), ("git reset --hard", "allow")])
    rows = {row["cmd"]: row for row in report["rows"]}
    bypass = rows["git commit -n -m x"]
    assert (bypass["guard_hooked_repo"], bypass["guard_plain_repo"]) == ("deny", "allow")
    assert bypass["hooks_dependent"] is True
    always = rows["git reset --hard"]
    assert (always["guard_hooked_repo"], always["guard_plain_repo"]) == ("deny", "deny")
    assert always["hooks_dependent"] is False


def test_a_guard_that_fails_is_an_error_not_an_allow(tmp_path: Path) -> None:
    guard = tmp_path / "broken.sh"
    guard.write_text("#!/usr/bin/env bash\nexit 3\n")
    baseline = tmp_path / "baseline.jsonl"
    baseline.write_text(json.dumps({"cmd": "ls", "pipeline": {"v": "allow"}}) + "\n")
    proc = subprocess.run(
        [sys.executable, str(SCRIPT), "--baseline", str(baseline), "--guard", str(guard), "--json"],
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    )
    assert proc.returncode != 0, "a crashing guard must not read as 'allow'"


def test_the_rust_verdict_is_todays_not_the_frozen_baseline(tmp_path: Path) -> None:
    """The baseline froze the policy BEFORE C2; the replay test lets today's Rust differ from it only on the
    rows of justified_differences.jsonl, whose `after` is today's verdict. Reading the baseline alone reported
    `git restore --staged a && git restore b` as "Rust allows" while the Rust policy blocked it (10/11/2026)."""
    cmd = "git restore --staged a && git reset --hard"
    report = _run(
        tmp_path,
        [(cmd, "allow"), ("git reset --hard", "allow")],
        justified=[
            [cmd, "pipeline", "allow", "block: REGRA #11 v2: git reset is destructive", "C2"],
            ["git reset --hard", "structural", "allow", "block: another field", "C2"],  # never decides
        ],
    )
    rows = {row["cmd"]: row for row in report["rows"]}
    assert rows[cmd]["rust"] == "block"
    assert rows[cmd]["class"] == "agree"
    assert rows["git reset --hard"]["rust"] == "allow"  # only a `pipeline` row overrides the baseline


def test_the_default_fixtures_read_the_restore_chain_as_blocked() -> None:
    """End to end on the committed fixtures: the row that motivated the fix."""
    proc = subprocess.run(
        [sys.executable, str(SCRIPT), "--json"], capture_output=True, text=True, check=True, timeout=600
    )
    rows = {row["cmd"]: row for row in json.loads(proc.stdout)["rows"]}
    assert rows["git restore --staged a && git restore b"]["rust"] == "block"
