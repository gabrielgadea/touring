#!/usr/bin/env python3
"""The judge's private gate daemon lives exactly as long as the cargo clause.

Cross-audit 14/09/2026 (C9): `_isolated_daemon_env` spawned a daemon per run and
nothing stopped it, so a PID outlived every verdict. `clause_cargo` now stops it
through `touring daemon-ctl stop --socket` on every exit — green, red or raised.
"""
from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import loop_converged as lc  # noqa: E402


@pytest.fixture
def recorded(monkeypatch):
    calls = []
    env = {"TOURING_DAEMON_SOCKET": "/tmp/touring-gate-test.sock"}
    monkeypatch.setattr(lc, "_isolated_daemon_env", lambda scope: dict(env))

    def fake_subprocess_run(cmd, **kwargs):
        calls.append(cmd)

        class Done:
            returncode = 0

        return Done()

    monkeypatch.setattr(lc.subprocess, "run", fake_subprocess_run)
    return calls


def stops(calls):
    return [c for c in calls if c[:3] == ["touring", "daemon-ctl", "stop"]]


def test_a_green_clause_stops_its_daemon(recorded, monkeypatch, tmp_path):
    monkeypatch.setattr(lc, "run", lambda cmd, **kw: (0, "", ""))
    ok, _ = lc.clause_cargo(tmp_path, rust_full=False)
    assert ok
    assert stops(recorded) == [["touring", "daemon-ctl", "stop", "--socket", "/tmp/touring-gate-test.sock"]]


def test_a_red_clause_stops_its_daemon(recorded, monkeypatch, tmp_path):
    monkeypatch.setattr(lc, "run", lambda cmd, **kw: (101, "", "error: boom"))
    ok, _ = lc.clause_cargo(tmp_path, rust_full=True)
    assert not ok
    assert len(stops(recorded)) == 1


def test_a_raising_clause_still_stops_its_daemon(recorded, monkeypatch, tmp_path):
    def explode(cmd, **kw):
        raise RuntimeError("cargo vanished")

    monkeypatch.setattr(lc, "run", explode)
    with pytest.raises(RuntimeError):
        lc.clause_cargo(tmp_path, rust_full=False)
    assert len(stops(recorded)) == 1
