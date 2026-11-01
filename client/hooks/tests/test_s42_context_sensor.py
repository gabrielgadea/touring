"""test_s42_context_sensor.py — check_context.sh says WHY it is silent (absent signal is not zero).

`[[ -f $FLAG && ! -f $DELIVERED ]] || exit 0` was the same silence for four different worlds: the context
is under the checkpoint (the good one); the status line never renders for this session (headless run, subagent
— there is no sensor); the status line stopped rendering (the sensor is stale); the warning was already
delivered.  A session in any of the last two went into the compaction with no warning and no trace of why.

Contract (17/10/2026):
* status_monitor.sh writes a heartbeat `<session>.seen` = "<epoch> <pct>" on every render, armed or not, and
  stamps the armed flag with a second line `ts=<epoch>`;
* check_context.sh appends one line per decision to ~/.claude/touring/logs/context_check_journal.jsonl with
  `silent_reason` in {abaixo, sem_sensor, obsoleto, entregue, monitor_projeto, sem_sessao} (null when it spoke);
* what it INJECTS is unchanged, byte for byte.
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import time
from pathlib import Path

import pytest
from helpers import CLIENT_HOOKS, run_hook_sh

CHECK = CLIENT_HOOKS / "check_context.sh"
STATUS = CLIENT_HOOKS / "status_monitor.sh"
EXPECTED_WARNING = (
    "Contexto em 57% — a autocompactação dispara aos 65%. Grave agora o estado em disco (RETOMADA do bundle, "
    "`touring decompose`, nó no grafo) para que a retomada não dependa do resumo. Se a próxima tarefa for "
    "distinta desta, prefira /clear e reconstrução pelo disco a /compact."
)


def _alerts(home: Path) -> Path:
    d = home / ".claude" / "touring" / "context_alert"
    d.mkdir(parents=True, exist_ok=True)
    return d


def _journal(home: Path) -> list[dict]:
    path = home / ".claude" / "touring" / "logs" / "context_check_journal.jsonl"
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()] if path.exists() else []


def _payload(session: str | None = "sess-a") -> dict:
    body = {"hook_event_name": "PostToolUse", "tool_name": "Bash", "tool_input": {}}
    return {**body, "session_id": session} if session is not None else body


def _heartbeat(home: Path, session: str, *, age: int, pct: int = 40) -> None:
    (_alerts(home) / f"{session}.seen").write_text(f"{int(time.time()) - age} {pct}\n")


def _check(home: Path, session: str | None = "sess-a", **env: str):
    return run_hook_sh(CHECK, _payload(session), home=home, extra_env=env or None)


def _last(home: Path) -> dict:
    lines = _journal(home)
    assert lines, "the hook left no journal line: a silent hook that does not say why is the defect"
    return lines[-1]


# ── the four silences, told apart ────────────────────────────────────────────────────────────────────


def test_fresh_heartbeat_and_no_flag_is_abaixo(tmp_path: Path) -> None:
    _heartbeat(tmp_path, "sess-a", age=3, pct=41)
    result = _check(tmp_path)
    assert result.rc == 0 and result.json is None and not result.stdout_raw.strip()
    line = _last(tmp_path)
    assert line["silent_reason"] == "abaixo" and line["said"] is False
    assert line["pct"] == 41 and 3 <= line["sensor_age_s"] < 60 and line["session"] == "sess-a"


def test_no_heartbeat_at_all_is_sem_sensor_not_abaixo(tmp_path: Path) -> None:
    """The status line never rendered: 'no flag' cannot be read as 'under the checkpoint'."""
    result = _check(tmp_path)
    assert result.rc == 0 and result.json is None
    line = _last(tmp_path)
    assert line["silent_reason"] == "sem_sensor" and line["sensor_age_s"] is None and line["pct"] is None


def test_old_heartbeat_is_obsoleto(tmp_path: Path) -> None:
    _heartbeat(tmp_path, "sess-a", age=4000)
    _check(tmp_path)
    line = _last(tmp_path)
    assert line["silent_reason"] == "obsoleto" and line["sensor_age_s"] >= 4000


def test_staleness_threshold_follows_the_override(tmp_path: Path) -> None:
    _heartbeat(tmp_path, "sess-a", age=30)
    _check(tmp_path, CONTEXT_SENSOR_STALE_SECS="10")
    assert _last(tmp_path)["silent_reason"] == "obsoleto"
    _heartbeat(tmp_path, "sess-b", age=30)
    _check(tmp_path, "sess-b", CONTEXT_SENSOR_STALE_SECS="100")
    assert _last(tmp_path)["silent_reason"] == "abaixo"


def test_unreadable_heartbeat_proves_nothing(tmp_path: Path) -> None:
    (_alerts(tmp_path) / "sess-a.seen").write_text("not-a-number\n")
    _check(tmp_path)
    line = _last(tmp_path)
    assert line["silent_reason"] == "obsoleto" and "ilegível" in line["detail"]


def test_delivered_flag_is_entregue(tmp_path: Path) -> None:
    (_alerts(tmp_path) / "sess-a.pct").write_text("60\nts=1\n")
    (_alerts(tmp_path) / "sess-a.delivered").write_text("")
    result = _check(tmp_path)
    assert result.json is None
    line = _last(tmp_path)
    assert line["silent_reason"] == "entregue" and line["pct"] == 60


def test_payload_without_session_is_sem_sessao(tmp_path: Path) -> None:
    result = _check(tmp_path, None)
    assert result.rc == 0 and result.json is None
    line = _last(tmp_path)
    assert line["silent_reason"] == "sem_sessao" and line["session"] == "_nosession"


def test_registered_project_monitor_is_named(tmp_path: Path) -> None:
    project = tmp_path / "proj"
    (project / ".claude").mkdir(parents=True)
    hooks = {"PostToolUse": [{"hooks": [{"type": "command", "command": "python3 context_monitor.py"}]}]}
    (project / ".claude" / "settings.json").write_text(json.dumps({"hooks": hooks}))
    _heartbeat(tmp_path, "sess-a", age=1)
    _check(tmp_path, CLAUDE_PROJECT_DIR=str(project))
    assert _last(tmp_path)["silent_reason"] == "monitor_projeto"


def test_without_jq_the_session_is_unreadable_and_the_journal_says_so(tmp_path: Path) -> None:
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    for tool in ("cat", "tr", "date", "mkdir", "head", "grep"):
        (bin_dir / tool).symlink_to(shutil.which(tool))
    env = {"PATH": str(bin_dir), "HOME": str(tmp_path)}
    proc = subprocess.run([shutil.which("bash") or "bash", str(CHECK)], input=json.dumps(_payload()).encode(),
                          capture_output=True, env=env, timeout=15, check=False)
    assert proc.returncode == 0 and not proc.stdout
    line = _last(tmp_path)
    assert line["silent_reason"] == "sem_sessao" and "jq" in line["detail"]


# ── speaking: unchanged, and journalled ──────────────────────────────────────────────────────────────


def test_the_injected_warning_is_unchanged_byte_for_byte(tmp_path: Path) -> None:
    (_alerts(tmp_path) / "sess-a.pct").write_text("57")  # the old single-line flag still arms it
    _heartbeat(tmp_path, "sess-a", age=1, pct=57)
    result = _check(tmp_path)
    assert result.additional_context() == EXPECTED_WARNING
    assert set(result.json["hookSpecificOutput"]) == {"hookEventName", "additionalContext"}
    line = _last(tmp_path)
    assert line["said"] is True and line["silent_reason"] is None and line["pct"] == 57


def test_two_line_flag_still_reads_the_percentage_from_line_one(tmp_path: Path) -> None:
    (_alerts(tmp_path) / "sess-a.pct").write_text("57\nts=1760000000\n")
    assert _check(tmp_path).additional_context() == EXPECTED_WARNING


def test_speaking_works_without_any_heartbeat_and_records_the_missing_sensor(tmp_path: Path) -> None:
    (_alerts(tmp_path) / "sess-a.pct").write_text("57\n")
    result = _check(tmp_path)
    assert "57%" in result.additional_context()
    line = _last(tmp_path)
    assert line["said"] is True and line["sensor_age_s"] is None


# ── the journal is not one line per tool call ────────────────────────────────────────────────────────


def test_repeated_identical_silence_is_journalled_once_per_window(tmp_path: Path) -> None:
    _heartbeat(tmp_path, "sess-a", age=1)
    for _ in range(5):
        _check(tmp_path)
    assert [x["silent_reason"] for x in _journal(tmp_path)] == ["abaixo"]


def test_a_change_of_reason_is_journalled_at_once(tmp_path: Path) -> None:
    _heartbeat(tmp_path, "sess-a", age=1)
    _check(tmp_path)
    _heartbeat(tmp_path, "sess-a", age=5000)
    _check(tmp_path)
    assert [x["silent_reason"] for x in _journal(tmp_path)] == ["abaixo", "obsoleto"]


def test_window_zero_journals_every_call(tmp_path: Path) -> None:
    _heartbeat(tmp_path, "sess-a", age=1)
    for _ in range(3):
        _check(tmp_path, CONTEXT_JOURNAL_EVERY="0")
    assert len(_journal(tmp_path)) == 3


# ── status_monitor.sh: the other half of the contract ────────────────────────────────────────────────


def _render(home: Path, session: str, tokens: int, **env: str) -> None:
    payload = {"session_id": session, "model": {"display_name": "T", "id": "t"},
               "context_window": {"context_window_size": 1000000, "current_usage": {"input_tokens": tokens}}}
    proc = subprocess.run(["bash", str(STATUS)], input=json.dumps(payload).encode(), capture_output=True,
                          env={**os.environ, "HOME": str(home), **env}, timeout=15, check=False)
    assert proc.returncode == 0, proc.stderr


def test_every_render_writes_the_heartbeat_even_below_the_checkpoint(tmp_path: Path) -> None:
    _render(tmp_path, "sess-a", 400_000)
    seen = (_alerts(tmp_path) / "sess-a.seen").read_text().split()
    assert len(seen) == 2 and seen[1] == "40" and abs(int(seen[0]) - time.time()) < 60
    assert not (_alerts(tmp_path) / "sess-a.pct").exists(), "below the checkpoint nothing is armed"


def test_armed_flag_is_stamped_and_line_one_stays_the_percentage(tmp_path: Path) -> None:
    _render(tmp_path, "sess-a", 600_000)
    lines = (_alerts(tmp_path) / "sess-a.pct").read_text().splitlines()
    assert lines[0] == "60"
    assert lines[1].startswith("ts=") and abs(int(lines[1][3:]) - time.time()) < 60


@pytest.mark.parametrize(("tokens", "reason"), [(400_000, "abaixo")])
def test_statusline_then_hook_end_to_end(tmp_path: Path, tokens: int, reason: str) -> None:
    """Integration: the real status line feeds the real hook; without the status line the same hook says sem_sensor."""
    _check(tmp_path, "sess-lonely")
    assert _last(tmp_path)["silent_reason"] == "sem_sensor"
    _render(tmp_path, "sess-a", tokens)
    _check(tmp_path)
    assert _last(tmp_path)["silent_reason"] == reason
    _render(tmp_path, "sess-a", 600_000)
    assert _check(tmp_path).additional_context().startswith("Contexto em 60%")
    assert _last(tmp_path)["said"] is True
