"""test_rate_limits_snapshot.py — a cota da franquia, do payload da status line para o disco.

Roda o gravador REAL e o status_monitor.sh REAL (subprocesso, payload no stdin) com HOME
sintético: nada aqui lê ~/.claude. Todo "não gravou" tem controle positivo no mesmo teste —
a mesma montagem GRAVA quando o payload traz a cota — porque arquivo ausente também é o
que se vê com o gravador quebrado.
"""

from __future__ import annotations

import importlib
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

import pytest
from helpers import CLIENT_HOOKS

WRITER = CLIENT_HOOKS / "rate_limits_snapshot.py"
STATUSLINE = CLIENT_HOOKS / "status_monitor.sh"
ABSENT = object()
NOW = int(time.time())
RL = {"five_hour": {"used_percentage": 23.5, "resets_at": NOW + 3600},
      "seven_day": {"used_percentage": 41, "resets_at": NOW + 3 * 86400}}


def _payload(rate_limits: Any = RL) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "session_id": "hooks-contract-rate-limits", "version": "2.1.294",
        "model": {"id": "claude-opus-5-5", "display_name": "Opus 5.5"},
        "context_window": {"context_window_size": 1000000, "current_usage": {"input_tokens": 1000}},
    }
    if rate_limits is not ABSENT:
        payload["rate_limits"] = rate_limits
    return payload


def _env(tmp_path: Path, extra: dict[str, str] | None = None) -> dict[str, str]:
    (tmp_path / "home").mkdir(exist_ok=True)
    env = {k: v for k, v in os.environ.items() if k != "ANTHROPIC_BASE_URL"}
    env.update(HOME=str(tmp_path / "home"), **(extra or {}))
    return env


def _write(tmp_path: Path, payload: Any, env_extra: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(WRITER)], input=json.dumps(payload), capture_output=True, text=True,
                          env=_env(tmp_path, env_extra), timeout=15, check=False)


def _read_mode(tmp_path: Path, *flags: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(WRITER), *flags], capture_output=True, text=True,
                          env=_env(tmp_path), timeout=15, check=False)


def _path(tmp_path: Path) -> Path:
    return tmp_path / "home" / ".claude" / "touring" / "rate_limits.json"


def _snapshot(tmp_path: Path) -> dict[str, Any]:
    return json.loads(_path(tmp_path).read_text(encoding="utf-8"))


# --- writing -------------------------------------------------------------------------------------

def test_writes_the_windows_under_the_anthropic_provider(tmp_path: Path) -> None:
    proc = _write(tmp_path, _payload())
    assert proc.returncode == 0, proc.stderr
    snap = _snapshot(tmp_path)
    record = snap["providers"]["anthropic"]
    assert snap["schema"] == 1
    assert record["windows"] == RL
    assert (record["session_id"], record["model_id"], record["cc_version"]) == (
        "hooks-contract-rate-limits", "claude-opus-5-5", "2.1.294")
    assert abs(record["captured_epoch"] - time.time()) < 60


@pytest.mark.parametrize("bad", [ABSENT, None, {}, {"five_hour": "texto"}, {"five_hour": {"resets_at": 1}}, "texto"],
                         ids=["ausente", "null", "vazio", "janela-nao-objeto", "sem-percentual", "nao-objeto"])
def test_a_payload_without_a_reading_never_creates_nor_erases(tmp_path: Path, bad: Any) -> None:
    """Absence never becomes zero: it writes nothing, and never wipes the last real reading."""
    assert _write(tmp_path, _payload(bad)).returncode == 0
    assert not _path(tmp_path).exists()
    assert _write(tmp_path, _payload()).returncode == 0  # positive control: the same setup DOES write
    before = _path(tmp_path).read_bytes()
    assert _write(tmp_path, _payload(bad)).returncode == 0
    assert _path(tmp_path).read_bytes() == before


def test_a_sister_backend_is_kept_apart_from_the_anthropic_reading(tmp_path: Path) -> None:
    _write(tmp_path, _payload())
    other = {"five_hour": {"used_percentage": 90, "resets_at": NOW + 60}}
    _write(tmp_path, _payload(other), env_extra={"ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/"})
    providers = _snapshot(tmp_path)["providers"]
    assert providers["anthropic"]["windows"] == RL
    assert providers["kimi"]["windows"] == other


def test_an_unchanged_reading_within_a_minute_is_not_rewritten_but_a_change_is(tmp_path: Path) -> None:
    _write(tmp_path, _payload())
    first = _snapshot(tmp_path)["providers"]["anthropic"]["captured_epoch"]
    time.sleep(1.1)
    _write(tmp_path, _payload())
    assert _snapshot(tmp_path)["providers"]["anthropic"]["captured_epoch"] == first
    changed = {**RL, "five_hour": {**RL["five_hour"], "used_percentage": 24}}
    _write(tmp_path, _payload(changed))
    record = _snapshot(tmp_path)["providers"]["anthropic"]
    assert record["windows"]["five_hour"]["used_percentage"] == 24
    assert record["captured_epoch"] > first


def test_a_corrupt_snapshot_is_replaced_by_a_valid_one(tmp_path: Path) -> None:
    _path(tmp_path).parent.mkdir(parents=True)
    _path(tmp_path).write_text("{lixo", encoding="utf-8")
    assert _write(tmp_path, _payload()).returncode == 0
    assert _snapshot(tmp_path)["providers"]["anthropic"]["windows"] == RL


def test_no_temporary_file_is_left_behind(tmp_path: Path) -> None:
    for pct in (1, 2, 3):
        _write(tmp_path, _payload({"five_hour": {"used_percentage": pct, "resets_at": NOW + 60}}))
    leftovers = [p.name for p in _path(tmp_path).parent.iterdir() if p.name.endswith(".tmp")]
    assert leftovers == []


def test_garbage_on_stdin_never_breaks_the_status_line(tmp_path: Path) -> None:
    proc = subprocess.run([sys.executable, str(WRITER)], input=b"\xff{lixo", capture_output=True,
                          env=_env(tmp_path), timeout=15, check=False)
    assert proc.returncode == 0
    assert not _path(tmp_path).exists()


# --- reading -------------------------------------------------------------------------------------

def test_show_prints_each_window_in_ptbr_with_age_and_reset(tmp_path: Path) -> None:
    _write(tmp_path, _payload())
    proc = _read_mode(tmp_path, "--show")
    assert proc.returncode == 0, proc.stderr
    for needle in ("Franquia Anthropic", "lida há", "5 h 23,5%", "7 d 41%", "zera"):
        assert needle in proc.stdout, (needle, proc.stdout)


def test_show_without_a_reading_exits_3_and_says_why(tmp_path: Path) -> None:
    proc = _read_mode(tmp_path, "--show")
    assert proc.returncode == 3
    assert "sem leitura" in proc.stdout


def test_show_flags_a_window_whose_reset_already_passed(tmp_path: Path) -> None:
    _write(tmp_path, _payload({"five_hour": {"used_percentage": 97, "resets_at": NOW - 60}}))
    assert "já zerou" in _read_mode(tmp_path, "--show").stdout


def test_json_mode_returns_the_snapshot_or_exits_3(tmp_path: Path) -> None:
    assert _read_mode(tmp_path, "--json").returncode == 3
    _write(tmp_path, _payload())
    proc = _read_mode(tmp_path, "--json")
    assert proc.returncode == 0
    assert json.loads(proc.stdout)["providers"]["anthropic"]["windows"] == RL


# --- the status line feeds it --------------------------------------------------------------------

def _statusline(tmp_path: Path, payload: dict[str, Any]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["bash", str(STATUSLINE)], input=json.dumps(payload), capture_output=True, text=True,
                          env=_env(tmp_path), timeout=15, check=False)


def _anthropic_windows_when_ready(tmp_path: Path, seconds: float = 5.0) -> dict[str, Any] | None:
    """The writer runs in background: poll until a complete snapshot holds the Anthropic reading."""
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            return _snapshot(tmp_path)["providers"]["anthropic"]["windows"]
        except (OSError, ValueError, KeyError):
            time.sleep(0.05)
    return None


@pytest.mark.skipif(shutil.which("jq") is None, reason="status_monitor.sh renders with jq")
def test_the_status_line_records_the_reading_and_still_renders(tmp_path: Path) -> None:
    quiet = _statusline(tmp_path, _payload(ABSENT))
    assert quiet.returncode == 0 and "Opus 5.5" in quiet.stdout
    time.sleep(0.5)
    assert not _path(tmp_path).exists()  # no reading in the payload → the writer is not even spawned
    loud = _statusline(tmp_path, _payload())
    assert loud.returncode == 0 and "Opus 5.5" in loud.stdout
    assert _anthropic_windows_when_ready(tmp_path) == RL, "the status line never fed the writer"


# --- D8: one predicate decides the provider ------------------------------------------------------

def test_the_provider_comes_from_one_source() -> None:
    sys.path.insert(0, str(CLIENT_HOOKS))
    try:
        source = importlib.import_module("backend_provider")
        writer = importlib.import_module("rate_limits_snapshot")
        gate = importlib.import_module("agent_model_gate")
    finally:
        sys.path.remove(str(CLIENT_HOOKS))
    assert writer.provider_of is source.provider_of
    assert gate.provider_of is source.provider_of
