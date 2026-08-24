"""Tests for Omarchy hook scripts."""
from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[3]
CLIENT_OMARCHY = REPO / "client" / "omarchy"
HOOKS_POST_BOOT = CLIENT_OMARCHY / "hooks" / "post-boot.d"
HOOKS_POST_UPDATE = CLIENT_OMARCHY / "hooks" / "post-update.d"

ALL_HOOKS = (
    list(HOOKS_POST_BOOT.glob("*"))
    + list(HOOKS_POST_UPDATE.glob("*"))
)
HOOK_SCRIPTS = [h for h in ALL_HOOKS if h.is_file() and not h.name.endswith(".sample")]


def all_hook_scripts() -> list[Path]:
    return HOOK_SCRIPTS


class TestBashSyntax:
    @pytest.mark.parametrize("hook", all_hook_scripts(), ids=lambda h: h.name)
    def test_bash_n(self, hook: Path) -> None:
        """bash -n must pass for all hooks."""
        result = subprocess.run(["bash", "-n", str(hook)], capture_output=True, text=True)
        assert result.returncode == 0, f"bash -n failed on {hook.name}:\n{result.stderr}"

    @pytest.mark.parametrize("hook", all_hook_scripts(), ids=lambda h: h.name)
    def test_shellcheck(self, hook: Path) -> None:
        """shellcheck -S warning must pass for all hooks."""
        if not shutil.which("shellcheck"):
            pytest.skip("shellcheck not in PATH")
        result = subprocess.run(
            ["shellcheck", "-S", "warning", str(hook)],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0, f"shellcheck failed on {hook.name}:\n{result.stdout}"


def _minimal_path(tmp_path: Path) -> str:
    """A PATH with only basic POSIX tools — no touring, pkexec, curl, etc."""
    # Keep only /usr/bin, /bin (bare minimum for bash, printf, date, etc.)
    return str(tmp_path / "stubs") + ":/usr/bin:/bin"


def _make_stubs(stub_dir: Path, *names: str) -> None:
    """Create no-op stubs that record calls but exit 0."""
    stub_dir.mkdir(parents=True, exist_ok=True)
    for name in names:
        stub = stub_dir / name
        stub.write_text("#!/usr/bin/env bash\nexit 0\n")
        stub.chmod(0o755)


class TestHooksExitZeroWithMinimalPath:
    """All hooks must exit 0 when called in a minimal environment (no Touring/pkexec/curl)."""

    @pytest.mark.parametrize("hook", all_hook_scripts(), ids=lambda h: h.name)
    def test_hook_exits_zero(self, hook: Path, tmp_path: Path) -> None:
        stub_dir = tmp_path / "stubs"
        # Provide a no-op for commands the hooks might call when available.
        # Only stub tools that don't exist on a standard system.
        # System tools (mkdir, hostname, date, stat, cat, find, wc, python3)
        # must NOT be stubbed — stubs exit 0 without creating files/dirs.
        _make_stubs(stub_dir, "omarchy-notification-send", "touring",
                    "pkexec", "limine-entry-tool", "curl")
        env = {
            **os.environ,
            "PATH": str(stub_dir) + ":/usr/bin:/bin",
            "HOME": str(tmp_path),
        }
        result = subprocess.run(
            ["bash", str(hook)],
            capture_output=True,
            text=True,
            env=env,
        )
        assert result.returncode == 0, (
            f"{hook.name} must exit 0 in minimal PATH\n"
            f"stdout: {result.stdout}\nstderr: {result.stderr}"
        )


class TestCiFireNoToken:
    """45-touring-ci-fire must exit 0 and not call curl when no token file exists."""

    def test_no_token_exits_zero_no_curl(self, tmp_path: Path) -> None:
        stub_dir = tmp_path / "stubs"
        stub_dir.mkdir()

        # Fake curl that records if it was called.
        curl_log = tmp_path / "curl_calls.log"
        fake_curl = stub_dir / "curl"
        fake_curl.write_text(
            f"#!/usr/bin/env bash\necho called >> {curl_log}\nexit 0\n"
        )
        fake_curl.chmod(0o755)

        _make_stubs(stub_dir, "omarchy-notification-send", "hostname", "date",
                    "stat", "cat")

        hook = HOOKS_POST_UPDATE / "45-touring-ci-fire"
        env = {
            **os.environ,
            "PATH": str(stub_dir) + ":/usr/bin:/bin",
            # HOME points to tmp_path where there is NO secrets dir.
            "HOME": str(tmp_path),
        }
        result = subprocess.run(
            ["bash", str(hook)],
            capture_output=True,
            text=True,
            env=env,
        )
        assert result.returncode == 0, f"must exit 0 without token\nstderr: {result.stderr}"
        # curl must NOT have been called.
        assert not curl_log.exists() or curl_log.read_text().strip() == "", (
            "curl must not be called when no token file exists"
        )
