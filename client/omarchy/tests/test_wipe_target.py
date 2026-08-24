"""Contract tests for wipe_target.sh.

Verifica:
1. --dry-run com serial inexistente → rc != 0 e mensagem contém 'serial'
2. Sem --yes, wipefs nunca é chamado (monkeypatch via PATH com fake wipefs)
3. --help exit 0
"""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path


WIPE_SCRIPT = Path(__file__).resolve().parents[1] / "bin" / "wipe_target.sh"


def test_wipe_script_exists_and_executable() -> None:
    assert WIPE_SCRIPT.is_file(), f"wipe_target.sh not found at {WIPE_SCRIPT}"
    assert os.access(WIPE_SCRIPT, os.X_OK), "wipe_target.sh not executable"


def test_help_exits_zero() -> None:
    result = subprocess.run(
        ["bash", str(WIPE_SCRIPT), "--help"],
        capture_output=True,
        text=True,
        timeout=10,
    )
    assert result.returncode == 0, (
        f"--help returned {result.returncode}:\n{result.stderr[:200]}"
    )


def test_dry_run_nonexistent_serial_fails() -> None:
    """--dry-run com serial inexistente → rc != 0 e mensagem contém 'serial'."""
    result = subprocess.run(
        [
            "bash",
            str(WIPE_SCRIPT),
            "--dry-run",
            "--serial",
            "NONEXISTENT_SERIAL_XYZ_9999",
        ],
        capture_output=True,
        text=True,
        timeout=10,
    )
    assert result.returncode != 0, (
        "Expected nonzero exit for non-existent serial in --dry-run mode"
    )
    combined = (result.stdout + result.stderr).lower()
    assert "serial" in combined, (
        f"Expected 'serial' in output, got:\nstdout={result.stdout[:200]}\nstderr={result.stderr[:200]}"
    )


def test_no_yes_never_calls_wipefs() -> None:
    """Sem --yes, wipefs nunca é executado, mesmo que esteja no PATH."""
    tmpdir = Path(tempfile.mkdtemp(prefix="wipe_test_"))
    marker = tmpdir / "wipefs_was_called"

    # fake wipefs que cria um arquivo marcador se invocado
    fake_wipefs = tmpdir / "wipefs"
    fake_wipefs.write_text(
        f"#!/bin/sh\ntouch '{marker}'\nexit 0\n",
        encoding="utf-8",
    )
    fake_wipefs.chmod(0o755)

    # Também criar blkdiscard falso para evitar erros de ferramenta ausente
    fake_blkdiscard = tmpdir / "blkdiscard"
    fake_blkdiscard.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    fake_blkdiscard.chmod(0o755)

    env = {
        **os.environ,
        "PATH": str(tmpdir) + ":" + os.environ.get("PATH", ""),
    }

    try:
        subprocess.run(
            ["bash", str(WIPE_SCRIPT), "--serial", "KP102L1HDJDW"],
            env=env,
            capture_output=True,
            text=True,
            timeout=10,
            input="",
        )
        assert not marker.exists(), (
            "wipefs was called without --yes flag! "
            "wipe_target.sh must never call wipefs without --yes."
        )
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


def test_dry_run_never_calls_wipefs() -> None:
    """--dry-run também nunca deve chamar wipefs (mesmo que as verificações passem)."""
    tmpdir = Path(tempfile.mkdtemp(prefix="wipe_dryrun_"))
    marker = tmpdir / "wipefs_was_called"

    fake_wipefs = tmpdir / "wipefs"
    fake_wipefs.write_text(
        f"#!/bin/sh\ntouch '{marker}'\nexit 0\n",
        encoding="utf-8",
    )
    fake_wipefs.chmod(0o755)

    env = {
        **os.environ,
        "PATH": str(tmpdir) + ":" + os.environ.get("PATH", ""),
    }

    try:
        subprocess.run(
            ["bash", str(WIPE_SCRIPT), "--dry-run", "--serial", "KP102L1HDJDW"],
            env=env,
            capture_output=True,
            text=True,
            timeout=10,
        )
        assert not marker.exists(), (
            "wipefs was called during --dry-run! "
            "--dry-run must never execute destructive commands."
        )
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)
