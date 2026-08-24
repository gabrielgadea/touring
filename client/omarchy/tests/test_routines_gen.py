"""tests/test_routines_gen.py — Tests for routines_gen.py."""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any
from unittest.mock import patch

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent / "bin"))
import routines_gen  # noqa: E402

_SCRIPT = Path(__file__).parent.parent / "bin" / "routines_gen.py"


# ---------------------------------------------------------------------------
# test_oncalendar_from_at — 6 exact mappings
# ---------------------------------------------------------------------------
@pytest.mark.parametrize(
    ("at", "expected"),
    [
        ("07:00", "*-*-* 07:00:00"),
        ("Sun 06:00", "Sun *-*-* 06:00:00"),
        ("Mon 09:00", "Mon *-*-* 09:00:00"),
        ("*:00/6", "*-*-* 00/6:00:00"),
        ("daily", "*-*-* 00:00:00"),
        ("hourly", "*-*-* *:00:00"),
    ],
)
def test_oncalendar_from_at(at: str, expected: str) -> None:
    assert routines_gen.oncalendar_from_at(at) == expected


def test_oncalendar_invalid_raises() -> None:
    with pytest.raises(ValueError, match="Cannot convert"):
        routines_gen.oncalendar_from_at("every-tuesday")


# ---------------------------------------------------------------------------
# test_persistent_true
# ---------------------------------------------------------------------------
def test_persistent_true() -> None:
    content = routines_gen.timer_content("test-id", "07:00")
    assert "Persistent=true" in content


# ---------------------------------------------------------------------------
# test_cloud_routine_generates_no_unit
# ---------------------------------------------------------------------------
def test_cloud_routine_generates_no_unit(tmp_path: Path) -> None:
    toml = tmp_path / "routines.toml"
    toml.write_text(
        '[defaults]\nmodel="sonnet"\n'
        '[[routine]]\nid="docs-drift"\nat="Mon 09:00"\nrunner="cloud"\n'
        'repo="gabrielgadea/touring"\nprompt="check docs drift"\n'
    )
    prefix = tmp_path / "units"
    prefix.mkdir()
    rc = routines_gen.main(
        ["apply", "--toml", str(toml), "--prefix", str(prefix), "--dry"]
    )
    assert rc == 0
    assert not (prefix / "routine-docs-drift.timer").exists()
    assert not (prefix / "routine-docs-drift.service").exists()


# ---------------------------------------------------------------------------
# test_board_json_schema — exact keys
# ---------------------------------------------------------------------------
def test_board_json_schema(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    toml = tmp_path / "routines.toml"
    toml.write_text(
        '[defaults]\nmodel="sonnet"\n'
        '[[routine]]\nid="inbox-digest"\nat="08:00"\nrunner="local"\nskill="inbox-digest"\n'
    )

    def _fake_entries(
        routines: list[dict[str, Any]], defaults: dict[str, Any]
    ) -> list[dict[str, Any]]:
        return [
            {
                "id": "inbox-digest",
                "at": "08:00",
                "runner": "local",
                "next": None,
                "last_status": None,
                "last_run": None,
            }
        ]

    with patch.object(routines_gen, "board_entries", _fake_entries):
        rc = routines_gen.main(["board", "--json", "--toml", str(toml)])

    assert rc == 0
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert len(data) == 1
    assert set(data[0].keys()) == {"id", "at", "runner", "next", "last_status", "last_run"}


# ---------------------------------------------------------------------------
# test_bypass_rejected — exit 2
# ---------------------------------------------------------------------------
def test_bypass_rejected(tmp_path: Path) -> None:
    toml = tmp_path / "routines.toml"
    toml.write_text(
        "[defaults]\n"
        '[[routine]]\nid="bad"\nat="07:00"\nrunner="local"\ncmd="echo hi"\n'
        'mode="bypassPermissions"\n'
    )
    prefix = tmp_path / "units"
    prefix.mkdir()
    rc = routines_gen.main(
        ["apply", "--toml", str(toml), "--prefix", str(prefix), "--dry"]
    )
    assert rc == 2


# ---------------------------------------------------------------------------
# test_apply_dry_writes_units_to_prefix
# Dry writes files but does NOT call systemctl.
# We use a fake systemctl in PATH that writes a sentinel if invoked.
# ---------------------------------------------------------------------------
def test_apply_dry_writes_units_to_prefix(tmp_path: Path) -> None:
    toml = tmp_path / "routines.toml"
    toml.write_text(
        '[defaults]\nmodel="sonnet"\ncwd="~/Work"\n'
        '[[routine]]\nid="inbox-digest"\nat="08:00"\nrunner="local"\nskill="inbox-digest"\n'
    )
    prefix = tmp_path / "units"
    sentinel = tmp_path / "systemctl_called"
    fake_bin = tmp_path / "fake_bin"
    fake_bin.mkdir()
    fake_systemctl = fake_bin / "systemctl"
    fake_systemctl.write_text(f"#!/bin/sh\ntouch '{sentinel}'\n")
    fake_systemctl.chmod(0o755)

    env = {**os.environ, "PATH": f"{fake_bin}:{os.environ.get('PATH', '')}"}
    result = subprocess.run(
        [sys.executable, str(_SCRIPT), "apply",
         "--toml", str(toml), "--prefix", str(prefix), "--dry"],
        env=env,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
    # --dry writes unit files
    assert (prefix / "routine-inbox-digest.service").exists()
    assert (prefix / "routine-inbox-digest.timer").exists()
    # systemctl was NOT invoked
    assert not sentinel.exists()


# ---------------------------------------------------------------------------
# test_orphan_unit_removed
# ---------------------------------------------------------------------------
def test_orphan_unit_removed(tmp_path: Path) -> None:
    toml = tmp_path / "routines.toml"
    toml.write_text(
        '[defaults]\nmodel="sonnet"\ncwd="~/Work"\n'
        '[[routine]]\nid="new-routine"\nat="07:00"\nrunner="local"\ncmd="echo hello"\n'
    )
    prefix = tmp_path / "units"
    prefix.mkdir()
    (prefix / "routine-old-routine.service").write_text(
        "[Unit]\nDescription=old\n\n[Service]\nType=oneshot\nExecStart=/bin/true\n"
    )
    (prefix / "routine-old-routine.timer").write_text(
        "[Unit]\nDescription=old\n\n[Timer]\nOnCalendar=daily\n\n[Install]\nWantedBy=timers.target\n"
    )

    rc = routines_gen.main(
        ["apply", "--toml", str(toml), "--prefix", str(prefix), "--no-reload"]
    )
    assert rc == 0
    assert not (prefix / "routine-old-routine.service").exists()
    assert not (prefix / "routine-old-routine.timer").exists()
    assert (prefix / "routine-new-routine.service").exists()
    assert (prefix / "routine-new-routine.timer").exists()


# ---------------------------------------------------------------------------
# test_units_pass_systemd_analyze
# ---------------------------------------------------------------------------
def test_units_pass_systemd_analyze(tmp_path: Path) -> None:
    if not shutil.which("systemd-analyze"):
        pytest.skip("systemd-analyze not available")

    toml = tmp_path / "routines.toml"
    toml.write_text(
        '[defaults]\nmodel="sonnet"\ncwd="~/Work"\n'
        '[[routine]]\nid="inbox-digest"\nat="08:00"\nrunner="local"\nskill="inbox-digest"\n'
        '[[routine]]\nid="validate-all"\nat="07:00"\nrunner="local"\ncmd="echo validate"\n'
    )
    prefix = tmp_path / "units"
    rc = routines_gen.main(
        ["apply", "--toml", str(toml), "--prefix", str(prefix), "--dry"]
    )
    assert rc == 0

    service_files = sorted(prefix.glob("routine-*.service"))
    assert len(service_files) > 0, "No service files generated"

    result = subprocess.run(
        ["systemd-analyze", "--user", "verify"] + [str(f) for f in service_files],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, (
        f"systemd-analyze verify failed:\nSTDOUT: {result.stdout}\nSTDERR: {result.stderr}"
    )


def test_last_status_is_never_until_the_unit_actually_runs(monkeypatch) -> None:
    """`Result=success` on a unit that never ran must not be reported as success.

    systemd initialises `Result` to `success`, so a freshly generated board came
    up entirely green before a single routine had fired — a status that cannot
    tell "ran and passed" from "never fired" attests nothing. The discriminator
    is `ExecMainExitTimestamp`, which stays empty until the first run.
    Measured 2026-08-23 against `routine-validate-all.service`.
    """
    import importlib.util
    spec = importlib.util.spec_from_file_location(
        "routines_gen", Path(__file__).parent.parent / "bin" / "routines_gen.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)

    monkeypatch.setattr(mod, "_systemctl", lambda *a: (
        0, "Result=success\nExecMainStatus=\nExecMainExitTimestamp=\n"))
    status, last_run = mod._service_status("validate-all")
    assert status == "never", f"never-run unit reported as {status!r}"
    assert last_run is None

    monkeypatch.setattr(mod, "_systemctl", lambda *a: (
        0, "Result=success\nExecMainStatus=0\n"
           "ExecMainExitTimestamp=Sun 2026-08-23 18:30:00 -03\n"))
    status, last_run = mod._service_status("validate-all")
    assert status == "success", f"a real success reported as {status!r}"
    assert last_run


def test_cmd_routine_uses_the_systemd_home_specifier_not_shell_expansion() -> None:
    """`~` in a cmd routine must become `%h`, never `$HOME`.

    systemd expands `$VAR` in ExecStart before the shell runs and took the entire
    path as the variable name:
        Invalid environment variable name evaluates to an empty string:
        HOME/projects/touring/client/omarchy/bin/validate_all.sh
    The unit died with status=2/INVALIDARGUMENT, so `routine-validate-all` could
    never fire. Measured 2026-08-23 on the first real `systemctl --user start`.
    """
    import importlib.util
    spec = importlib.util.spec_from_file_location(
        "routines_gen", Path(__file__).parent.parent / "bin" / "routines_gen.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)

    unit = mod.service_content("validate-all", {
        "cmd": "~/projects/touring/client/omarchy/bin/validate_all.sh",
        "cwd": "%h/Work",
    })
    exec_line = next(ln for ln in unit.splitlines() if ln.startswith("ExecStart="))
    assert "$HOME" not in exec_line, f"shell-style expansion resurfaced: {exec_line}"
    assert "%h/projects/touring" in exec_line, f"home specifier missing: {exec_line}"
    assert "~" not in exec_line, f"unexpanded tilde left in the unit: {exec_line}"


def test_a_literal_percent_in_a_cmd_is_escaped() -> None:
    """systemd reads `%` as a specifier; a command carrying one must escape it."""
    import importlib.util
    spec = importlib.util.spec_from_file_location(
        "routines_gen", Path(__file__).parent.parent / "bin" / "routines_gen.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)

    unit = mod.service_content("df", {"cmd": "df --output=pcent | tail -1  # 50%", "cwd": "%h/Work"})
    exec_line = next(ln for ln in unit.splitlines() if ln.startswith("ExecStart="))
    assert "50%%" in exec_line, f"literal percent not escaped: {exec_line}"
