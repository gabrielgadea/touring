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
import re
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


# --- 11/11/2026 (retro): argparse real, idade/sessão no --show e a queda de franquia sem virar a janela --------

T0 = 1_800_000_000  # fixed clock for the module-level tests: ages and resets are exact, nothing sleeps
RESET_A, RESET_B = T0 + 3600, T0 + 3600 + 5 * 3600


def _writer() -> Any:
    sys.path.insert(0, str(CLIENT_HOOKS))
    try:
        return importlib.import_module("rate_limits_snapshot")
    finally:
        sys.path.remove(str(CLIENT_HOOKS))


def _read(home: Path, windows: dict[str, Any], now: float, session: str = "sess-A", provider: str = "anthropic",
          model: str = "claude-opus-5-5") -> None:
    payload = {"session_id": session, "model": {"id": model}, "version": "2.1.300", "rate_limits": windows}
    _writer().record_reading(home, provider, payload, now)


def _five(pct: float, reset: int | None = RESET_A) -> dict[str, Any]:
    window: dict[str, Any] = {"used_percentage": pct}
    if reset is not None:
        window["resets_at"] = reset
    return {"five_hour": window}


def _report(home: Path, now: float, provider: str = "anthropic") -> str:
    text = _writer().show_report(home, provider, now)
    assert text is not None
    return text


DROP = "a franquia caiu de {a} para {b} sem virar a janela — confirme por uma segunda leitura"


def test_help_is_harmless_it_neither_reads_stdin_nor_writes(tmp_path: Path) -> None:
    proc = subprocess.run([sys.executable, str(WRITER), "--help"], input=json.dumps(_payload()), capture_output=True,
                          text=True, env=_env(tmp_path), timeout=15, check=False)
    assert proc.returncode == 0, proc.stderr
    assert "--show" in proc.stdout and "--json" in proc.stdout
    assert not _path(tmp_path).exists(), "--help ran the writer body"
    assert _write(tmp_path, _payload()).returncode == 0 and _path(tmp_path).exists()  # positive control: no flag DOES write


def test_an_unknown_flag_fails_loud_and_writes_nothing(tmp_path: Path) -> None:
    proc = subprocess.run([sys.executable, str(WRITER), "--bogus"], input=json.dumps(_payload()), capture_output=True,
                          text=True, env=_env(tmp_path), timeout=15, check=False)
    assert proc.returncode == 2
    assert "--bogus" in proc.stderr
    assert not _path(tmp_path).exists()


def test_the_status_line_still_calls_the_writer_with_no_arguments() -> None:
    """The stdin-writing contract is `no argv`: pin it to the REAL caller, so a changed call is a conscious decision."""
    calls = [line for line in STATUSLINE.read_text(encoding="utf-8").splitlines() if "rate_limits_snapshot.py" in line
             and "python3" in line]
    assert len(calls) == 1, calls
    assert re.search(r'rate_limits_snapshot\.py"?\s*(>|2>|&|$)', calls[0]), calls[0]


def test_show_prints_the_age_the_capturing_session_and_the_model(tmp_path: Path) -> None:
    home = tmp_path / "home"
    _read(home, _five(40), T0, session="sessao-capturadora-123", model="claude-fable-5-2")
    text = _report(home, T0 + 7200)  # read two hours ago, window of 5 h still open? reset is T0+3600 -> past
    assert "lida há 2 h" in text.splitlines()[0]
    assert "sessão sessao-capturadora-123" in text
    assert "modelo claude-fable-5-2" in text
    assert "há 2 h" in text.splitlines()[1]


def test_show_does_not_invent_a_session_when_the_payload_had_none(tmp_path: Path) -> None:
    home = tmp_path / "home"
    _writer().record_reading(home, "anthropic", {"rate_limits": _five(40)}, T0)
    text = _report(home, T0 + 60)
    assert "None" not in text and "sessão" not in text


def test_a_drop_inside_the_same_window_is_flagged_for_a_second_reading(tmp_path: Path) -> None:
    home = tmp_path / "home"
    _read(home, _five(41), T0, session="sess-A")
    _read(home, _five(38), T0 + 120, session="sess-B")
    text = _report(home, T0 + 180)
    assert DROP.format(a="41%", b="38%") in text
    assert "5 h" in text and "sess-A" in text  # names the window and the session that recorded the higher value


def test_no_warning_on_a_first_reading_an_equal_one_or_a_rise(tmp_path: Path) -> None:
    home = tmp_path / "home"
    _read(home, _five(38), T0)
    assert "caiu" not in _report(home, T0 + 10)
    _read(home, _five(38), T0 + 200)  # equal, rewritten after > 60 s
    assert "caiu" not in _report(home, T0 + 210)
    _read(home, _five(41), T0 + 300)
    assert "caiu" not in _report(home, T0 + 310)
    _read(home, _five(38), T0 + 400)  # positive control: the drop IS flagged in this very setup
    assert DROP.format(a="41%", b="38%") in _report(home, T0 + 410)


def test_a_drop_after_the_window_turned_is_a_note_not_a_warning(tmp_path: Path) -> None:
    home = tmp_path / "home"
    _read(home, _five(97, RESET_A), T0)
    _read(home, _five(2, RESET_B), RESET_A + 60)
    text = _report(home, RESET_A + 120)
    assert "sem virar a janela" not in text
    assert "a janela virou" in text and "97%" in text and "2%" in text


def test_a_drop_without_a_reset_time_claims_nothing(tmp_path: Path) -> None:
    home = tmp_path / "home"
    _read(home, _five(50, None), T0)
    _read(home, _five(40, None), T0 + 120)
    assert "caiu" not in _report(home, T0 + 130)  # same window cannot be proven: it stays silent, never guesses


def test_the_last_distinct_value_survives_rewrites_of_an_unchanged_reading(tmp_path: Path) -> None:
    home = tmp_path / "home"
    _read(home, _five(30), T0)
    _read(home, _five(80), T0 + 100)
    _read(home, _five(78), T0 + 200)
    _read(home, _five(78), T0 + 400)  # same value again, > 60 s later: rewritten, history must survive
    assert DROP.format(a="80%", b="78%") in _report(home, T0 + 410)
    _read(home, _five(79), T0 + 500)  # now rising: the previous distinct value is 78, no warning
    assert "caiu" not in _report(home, T0 + 510)


def test_previous_is_additive_schema_stays_1_and_windows_remain_the_payload(tmp_path: Path) -> None:
    home = tmp_path / "home"
    _read(home, _five(41), T0)
    _read(home, _five(38), T0 + 120)
    snap = json.loads((home / ".claude" / "touring" / "rate_limits.json").read_text(encoding="utf-8"))
    record = snap["providers"]["anthropic"]
    assert snap["schema"] == 1
    assert record["windows"] == _five(38)  # an old reader sees exactly what it always saw
    assert record["previous"]["windows"]["five_hour"]["used_percentage"] == 41
    assert record["previous"]["windows"]["five_hour"]["session_id"] == "sess-A"


def test_quota_line_is_byte_identical_with_or_without_the_previous_field(tmp_path: Path) -> None:
    """agent_model_gate cites quota_line in a deny: the new field must not leak into it."""
    home = tmp_path / "home"
    _read(home, _five(41), T0)
    plain = _writer().quota_line(home, now=T0 + 60)
    _read(home, _five(38), T0 + 120)
    after = _writer().quota_line(home, now=T0 + 180)
    assert plain is not None and after is not None
    assert after == plain.replace("41%", "38%")  # only the percentage moved; same age, same layout
    assert "caiu" not in after and "sess" not in after


def test_a_snapshot_written_before_the_previous_field_existed_still_reads(tmp_path: Path) -> None:
    path = tmp_path / "home" / ".claude" / "touring" / "rate_limits.json"
    path.parent.mkdir(parents=True)
    old = {"schema": 1, "providers": {"anthropic": {"captured_at": "2026-11-11T10:00:00-03:00", "captured_epoch": T0,
                                                    "session_id": "velha", "model_id": "m", "cc_version": "1",
                                                    "windows": _five(55)}}}
    path.write_text(json.dumps(old), encoding="utf-8")
    text = _report(tmp_path / "home", T0 + 30)
    assert "5 h 55%" in text and "caiu" not in text and "sessão velha" in text


def test_each_provider_keeps_its_own_previous(tmp_path: Path) -> None:
    home = tmp_path / "home"
    _read(home, _five(90), T0, provider="anthropic")
    _read(home, _five(10), T0 + 100, provider="kimi", session="sess-K")
    _read(home, _five(5), T0 + 200, provider="kimi", session="sess-K")
    assert "caiu" not in _report(home, T0 + 210, "anthropic")
    assert DROP.format(a="10%", b="5%") in _report(home, T0 + 210, "kimi")


def test_show_end_to_end_prints_the_warning_through_the_cli(tmp_path: Path) -> None:
    far = NOW + 3 * 3600
    assert _write(tmp_path, _payload({"five_hour": {"used_percentage": 41, "resets_at": far}})).returncode == 0
    first = _read_mode(tmp_path, "--show")
    assert first.returncode == 0 and "caiu" not in first.stdout
    assert _write(tmp_path, _payload({"five_hour": {"used_percentage": 38, "resets_at": far}})).returncode == 0
    second = _read_mode(tmp_path, "--show")
    assert second.returncode == 0, second.stderr
    assert DROP.format(a="41%", b="38%") in second.stdout
    assert "hooks-contract-rate-limits" in second.stdout
