#!/usr/bin/env python3
"""Guards for `scripts/update-touring` — the tool that performs every deploy.

Until 2026-08-18 this script existed ONLY at ~/.local/bin/update-touring: outside
version control, unreviewable, and unable to survive a new machine. It now lives
here and is reached on PATH through a symlink, the same shape already used for
the touring/touring-hook/touring-daemon binaries.

Two kinds of check, split by what each environment can actually see:

  · **Content invariants** run anywhere, CI included. They encode the two defects
    that versioning exposed: a default workspace pointing at the FROZEN pre-move
    root, and process checks broad enough to match other projects' daemons.
  · **The symlink check** needs the operator's machine, so it skips elsewhere
    rather than failing — a gate that cannot look must not block.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "scripts" / "update-touring"
INSTALLED = Path.home() / ".local" / "bin" / "update-touring"

#: Lines that are comments (or blank) carry no behaviour — the invariants below
#: are about what the script DOES, and several comments quote the old code on
#: purpose so the next reader knows what was wrong.
def executable_lines() -> list[tuple[int, str]]:
    out = []
    for n, raw in enumerate(SCRIPT.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if line and not line.startswith("#"):
            out.append((n, raw))
    return out


def test_the_script_is_versioned_here_and_executable():
    assert SCRIPT.is_file(), f"{SCRIPT} missing — the deploy tool must be in the repo"
    assert os.access(SCRIPT, os.X_OK), "the deploy tool must be executable"


def test_it_parses():
    proc = subprocess.run(["bash", "-n", str(SCRIPT)], capture_output=True, text=True)
    assert proc.returncode == 0, proc.stderr


@pytest.mark.skipif(shutil.which("shellcheck") is None, reason="shellcheck not installed")
def test_shellcheck_is_clean():
    proc = subprocess.run(
        ["shellcheck", "-S", "warning", str(SCRIPT)], capture_output=True, text=True
    )
    assert proc.returncode == 0, proc.stdout


def test_the_workspace_is_never_defaulted_to_the_frozen_root():
    """`~/.claude/rust` is the FROZEN pre-move tree (see CLAUDE.md).

    The old default was `${TOURING_WORKSPACE_ROOT:-${HOME}/.claude/rust}`, so from
    any shell that did not export that variable — an ordinary terminal — this
    script rebuilt and installed the frozen tree and said nothing. The workspace
    is now derived from the script's own location, which is exactly what living
    inside the workspace buys."""
    for n, line in executable_lines():
        assert ".claude/rust" not in line, (
            f"line {n} resolves the workspace to the frozen root: {line.strip()}"
        )
    body = SCRIPT.read_text(encoding="utf-8")
    assert "BASH_SOURCE" in body, "the workspace must be derived from the script's location"


def test_no_process_check_matches_another_project_s_daemon():
    """Per-project daemons are normal now; "any process named touring-daemon" is not
    a question with a useful answer.

    The broad pattern made three separate call sites wrong at once: the kill
    fallback would SIGTERM/SIGKILL other projects' daemons (the cascading kill
    REGRA #19 exists to prevent), `start_daemon` would skip starting the global
    daemon because a sibling was up, and `verify` would fail the run over an
    unrelated process's binary."""
    # `pgrep` was the observed offender, but `pkill`/`killall` and a
    # `ps | grep` pipeline are the SAME name-match mistake with a trigger
    # attached — a guard that names one spelling invites the next (T5,
    # cross-audit 2026-08-23).
    offenders = [
        (n, line.strip())
        for n, line in executable_lines()
        if re.search(r"\b(pgrep|pkill|killall)\b", line)
        or re.search(r"\bps\b.*\|\s*grep", line)
    ]
    assert offenders == [], (
        "process checks must resolve the owner of the global socket "
        f"(global_daemon_pid), not match a name: {offenders}"
    )


def test_no_uid_is_hardcoded_in_runtime_paths():
    """`/tmp/touring-daemon-1000.sock` only resolves on uid 1000 — on any other
    machine `global_daemon_pid` finds nothing and every dependent path (kill,
    cleanup, verify) silently acts on nothing. The uid must come from `id -u`
    (T1, cross-audit 2026-08-23)."""
    offenders = [
        (n, line.strip())
        for n, line in executable_lines()
        if re.search(r"touring-daemon-1000\.", line)
    ]
    assert offenders == [], f"uid hardcoded in runtime path: {offenders}"
    body = SCRIPT.read_text(encoding="utf-8")
    assert 'touring-daemon-$(id -u)' in body, (
        "socket/lock paths must derive the uid via $(id -u)"
    )


def test_the_owner_resolver_verifies_through_proc():
    """REGRA #19: a pid is only ours once /proc confirms what it is. A recycled
    pid from a stale file must never be mistaken for the daemon."""
    body = SCRIPT.read_text(encoding="utf-8")
    assert "global_daemon_pid()" in body, "the single owner-resolver must exist"
    resolver = body.split("global_daemon_pid()", 1)[1].split("\n}", 1)[0]
    assert "/proc/" in resolver, "the resolver must confirm the pid through /proc"
    assert "comm" in resolver, "the resolver must confirm the process identity"
    assert "DAEMON_SOCKET" in resolver, "the resolver must be scoped to the global socket"


def test_kill_is_never_applied_to_a_list_of_pids():
    """One pid, named, or none. `kill -9 ${pids}` over a pgrep result is precisely
    the cascading kill that took out other sessions' processes before."""
    for n, line in executable_lines():
        if re.search(r"\bkill\s+-", line):
            assert "${pid}" in line or '"${pid}"' in line, (
                f"line {n} kills something other than the resolved global pid: {line.strip()}"
            )


# ── the half only the operator's machine can answer ──────────────────────────


def test_the_installed_tool_is_a_symlink_to_this_file():
    """Drift is prevented structurally: editing the installed path edits the repo
    file, because they are the same inode. Replacing the symlink with a copy
    would silently restore the old situation, so it is asserted.

    Strict mode (`UPDATE_TOURING_REQUIRE_SYMLINK=1`, exported by the
    propagate-release gate 1/6): a missing installed tool FAILS instead of
    skipping — the release path is about to invoke `update-touring` from the
    PATH, so "absent here" means "step 2/6 will run something unreviewed";
    a silent skip is exactly the hole the gate exists to close (T4,
    cross-audit 2026-08-23)."""
    if not INSTALLED.exists():
        if os.environ.get("UPDATE_TOURING_REQUIRE_SYMLINK") == "1":
            pytest.fail(
                f"{INSTALLED} absent under UPDATE_TOURING_REQUIRE_SYMLINK=1 — the "
                f"release gate needs the real symlink. Restore with: "
                f"ln -sfn {SCRIPT} {INSTALLED}"
            )
        pytest.skip(f"{INSTALLED} absent — needs the operator's machine")
    assert INSTALLED.is_symlink(), (
        f"{INSTALLED} is a regular file again — the deploy tool has drifted out of "
        f"the repo. Restore with: ln -sfn {SCRIPT} {INSTALLED}"
    )
    assert INSTALLED.resolve() == SCRIPT.resolve(), (
        f"{INSTALLED} points at {INSTALLED.resolve()}, not {SCRIPT}"
    )


def test_the_installed_tool_still_runs():
    if not INSTALLED.exists():
        pytest.skip(f"{INSTALLED} absent — needs the operator's machine")
    proc = subprocess.run(
        ["bash", "-n", str(INSTALLED)], capture_output=True, text=True
    )
    assert proc.returncode == 0, proc.stderr


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v", "--tb=short", "-p", "no:cacheprovider"]))
