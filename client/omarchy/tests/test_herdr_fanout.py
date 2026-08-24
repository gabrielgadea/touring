"""Tests for herdr_branch.sh and the herdr-fanout ADW spec."""
from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[3]  # projects/touring
CLIENT_OMARCHY = REPO / "client" / "omarchy"
HERDR_BRANCH = CLIENT_OMARCHY / "bin" / "herdr_branch.sh"
MOCK_HERDR = CLIENT_OMARCHY / "tests" / "bin" / "herdr"
ADW_SPEC = ".touring/adw/herdr-fanout-demo.toml"


def run_branch(
    branch: str,
    *,
    journal_dir: Path,
    mock_state: str = "done",
    extra_env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    """Run herdr_branch.sh with the mock herdr in PATH."""
    mock_log = str(journal_dir / "herdr_mock.log")
    env = {
        **os.environ,
        "PATH": str(MOCK_HERDR.parent) + ":" + os.environ.get("PATH", ""),
        "HERDR_MOCK_LOG": mock_log,
        "HERDR_MOCK_STATE": mock_state,
        **(extra_env or {}),
    }
    return subprocess.run(
        [
            "bash",
            str(HERDR_BRANCH),
            branch,
            "/tmp",        # cwd (irrelevant for mock)
            "test prompt",
            str(journal_dir),
        ],
        capture_output=True,
        text=True,
        env=env,
    )


class TestDoneBranchWritesJournalAndExits0:
    def test_done_branch_writes_journal_and_exit_0(self, tmp_path: Path) -> None:
        """When herdr reports `done`, herdr_branch.sh must exit 0 and write a journal."""
        result = run_branch("audit", journal_dir=tmp_path, mock_state="done")
        assert result.returncode == 0, f"stderr: {result.stderr}"
        journal = tmp_path / "audit.txt"
        assert journal.exists(), "journal file must be written"
        content = journal.read_text()
        assert "state=done" in content
        assert "mock pane output" in content


class TestBlockedBranchFailsNode:
    def test_blocked_branch_fails_node(self, tmp_path: Path) -> None:
        """When herdr reports `blocked`, herdr_branch.sh must exit non-zero AND write a journal."""
        result = run_branch("docs", journal_dir=tmp_path, mock_state="blocked")
        assert result.returncode != 0, "blocked branch must exit non-zero"
        journal = tmp_path / "docs.txt"
        assert journal.exists(), "journal file must be written even on failure"
        content = journal.read_text()
        assert "state=blocked" in content


class TestCallsSequence:
    def test_calls_sequence(self, tmp_path: Path) -> None:
        """workspace create → agent start → agent prompt → agent wait must all be called."""
        run_branch("audit", journal_dir=tmp_path, mock_state="done")
        mock_log = tmp_path / "herdr_mock.log"
        assert mock_log.exists(), "mock log must be written"
        calls = mock_log.read_text()
        assert "workspace create" in calls, "workspace create must be called"
        assert "agent start" in calls, "agent start must be called"
        assert "agent prompt" in calls, "agent prompt must be called"
        assert "agent wait" in calls, "agent wait must be called"
        # Prompt must include branch name suffix.
        assert "ramo: audit" in calls, "prompt must include (ramo: <branch>)"


class TestLintDemoSpec:
    def test_lint_demo_spec(self) -> None:
        """touring adw lint herdr-fanout-demo must exit 0."""
        if not shutil.which("touring"):
            pytest.skip("touring not in PATH")
        result = subprocess.run(
            ["touring", "adw", "lint", "herdr-fanout-demo"],
            capture_output=True,
            text=True,
            cwd=str(REPO),
        )
        assert result.returncode == 0, f"lint errors: {result.stdout}\n{result.stderr}"
