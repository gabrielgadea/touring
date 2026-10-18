"""test_status_monitor_quota.py — a cota da franquia na barra de status, e o aviso único aos 85%.

O payload da status line traz `rate_limits.five_hour` e `rate_limits.seven_day` (`used_percentage` 0–100,
`resets_at` em epoch) para assinantes Pro/Max, depois da 1ª resposta da sessão, cada janela podendo faltar
sozinha. `status_monitor.sh` já repassava o payload ao `rate_limits_snapshot.py`, mas não o mostrava: a cota
que paga cada subagente era invisível justamente a quem decide lançar o fanout.

Contrato (18/10/2026):
* segmento compacto `5h 60% ↻1h04 · 7d 50%`, cor por faixa (verde < 70, amarelo < 85, vermelho >= 85), cada
  janela pela própria porcentagem; o reset (`↻`) só da janela de 5 h;
* **ausência nunca vira zero**: payload sem `rate_limits` (outro backend, versão antiga), janela sem número
  ou com lixo não mostra NADA daquela janela;
* aos 85% da janela de 5 h o monitor arma `<sessão>.quota` (linha 1 = porcentagem, linha 2 = `resets=<epoch>`)
  no MESMO diretório e pelo MESMO ciclo do marcador de contexto (`.pct` / `.delivered`), e o `check_context.sh`
  (PostToolUse) o entrega UMA vez por janela — sem um terceiro canal. Sai quando a janela cai abaixo do limiar
  (renderização com leitura); ausência de leitura NÃO desarma.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import time
from pathlib import Path
from typing import Any

import pytest
from helpers import CLIENT_HOOKS, run_hook_sh

HOOKS_DIR = Path(os.environ.get("QUOTA_HOOKS_DIR") or CLIENT_HOOKS)  # the mutation runner points it at a mutated copy
STATUS = HOOKS_DIR / "status_monitor.sh"
CHECK = HOOKS_DIR / "check_context.sh"
ANSI = re.compile(r"\x1b\[[0-9;]*m")
GREEN, YELLOW, RED = "32", "33", "31"
SESSION = "quota-sess"
CONTEXT_WARNING_HEAD = "Contexto em 57% — a autocompactação dispara aos 65%."


def _window(pct: Any, resets_in_min: float | None = None) -> dict[str, Any]:
    """A window `resets_in_min` minutes from now, less 20 s so the countdown (rounded UP) reads exactly that."""
    window: dict[str, Any] = {"used_percentage": pct}
    if resets_in_min is not None:
        window["resets_at"] = int(time.time() + resets_in_min * 60 - 20)
    return window


def _payload(rate_limits: Any = ..., *, session: str | None = SESSION, tokens: int = 250_000) -> dict[str, Any]:
    body: dict[str, Any] = {"model": {"display_name": "T", "id": "t"}, "cost": {"total_cost_usd": 0.0},
                            "context_window": {"context_window_size": 1_000_000, "current_usage": {"input_tokens": tokens}}}
    if session is not None:
        body["session_id"] = session
    if rate_limits is not ...:
        body["rate_limits"] = rate_limits
    return body


def _render(home: Path, payload: dict[str, Any], **env: str) -> str:
    proc = subprocess.run(["bash", str(STATUS)], input=json.dumps(payload).encode(), capture_output=True,
                          env={**os.environ, "HOME": str(home), **env}, timeout=15, check=False)
    assert proc.returncode == 0, proc.stderr
    return proc.stdout.decode("utf-8", errors="replace")


def _plain(out: str) -> str:
    return ANSI.sub("", out)


def _alerts(home: Path) -> Path:
    d = home / ".claude" / "touring" / "context_alert"
    d.mkdir(parents=True, exist_ok=True)
    return d


def _color_of(out: str, label: str) -> str:
    match = re.search(rf"\x1b\[(\d+)m{re.escape(label)}", out)
    assert match, f"{label!r} is not preceded by a colour code in {out!r}"
    return match.group(1)


# ── o segmento ───────────────────────────────────────────────────────────────────────────────────────


def test_both_windows_render_compactly_with_the_five_hour_reset(tmp_path: Path) -> None:
    out = _render(tmp_path, _payload({"five_hour": _window(60, 64), "seven_day": _window(50, 3000)}))
    assert "5h 60% ↻1h04 · 7d 50%" in _plain(out)


@pytest.mark.parametrize(("pct", "color"), [(0, GREEN), (69, GREEN), (70, YELLOW), (84, YELLOW), (85, RED), (100, RED)])
def test_each_window_is_coloured_by_its_own_band(tmp_path: Path, pct: int, color: str) -> None:
    out = _render(tmp_path, _payload({"five_hour": _window(pct, 90), "seven_day": _window(10, 3000)}), CLAUDE_AUTOCOMPACT_PCT_OVERRIDE="65")
    assert _color_of(out, f"5h {pct}%") == color
    assert _color_of(out, "7d 10%") == GREEN
    other = _render(tmp_path, _payload({"five_hour": _window(10, 90), "seven_day": _window(pct, 3000)}))
    assert _color_of(other, f"7d {pct}%") == color


def test_the_band_limits_follow_the_environment(tmp_path: Path) -> None:
    out = _render(tmp_path, _payload({"five_hour": _window(87, 90)}), QUOTA_ALERT_PCT="90", QUOTA_WARN_PCT="50")
    assert _color_of(out, "5h 87%") == YELLOW


def test_a_payload_without_rate_limits_shows_nothing_and_never_zero(tmp_path: Path) -> None:
    plain = _plain(_render(tmp_path, _payload()))
    assert not re.search(r"\b[57][hd]\b", plain) and "↻" not in plain, plain
    assert "250K/1.0M (25.0%)" in plain  # the rest of the line is untouched


@pytest.mark.parametrize("rate_limits", [{}, None, "x", [], [1], 7, {"five_hour": None}, {"five_hour": "high"},
                                         {"five_hour": {}}, {"five_hour": {"resets_at": 1}}, {"seven_day": []}])
def test_a_rate_limits_field_without_a_reading_shows_nothing(tmp_path: Path, rate_limits: Any) -> None:
    plain = _plain(_render(tmp_path, _payload(rate_limits)))
    assert not re.search(r"\b[57][hd]\b", plain) and "↻" not in plain, plain


@pytest.mark.parametrize("bad", ["abc", None, True, [], {}, -1, -0.5, "50"])
def test_a_window_with_junk_instead_of_a_number_is_dropped_alone(tmp_path: Path, bad: Any) -> None:
    plain = _plain(_render(tmp_path, _payload({"five_hour": {"used_percentage": bad, "resets_at": 99999999999},
                                               "seven_day": _window(50)})))
    assert "7d 50%" in plain and "5h" not in plain and "↻" not in plain, plain


def test_one_window_alone_renders_alone(tmp_path: Path) -> None:
    only_week = _plain(_render(tmp_path, _payload({"seven_day": _window(50)})))
    assert "7d 50%" in only_week and "5h" not in only_week and "·" not in only_week
    only_hours = _plain(_render(tmp_path, _payload({"five_hour": _window(60, 64)})))
    assert "5h 60% ↻1h04" in only_hours and "7d" not in only_hours and "·" not in only_hours


@pytest.mark.parametrize(("pct", "shown"), [(59.9, "59%"), (59.0, "59%"), (0.4, "0%"), (100.0, "100%"), (250, "100%")])
def test_the_percentage_is_floored_and_capped_at_one_hundred(tmp_path: Path, pct: float, shown: str) -> None:
    assert f"5h {shown}" in _plain(_render(tmp_path, _payload({"five_hour": _window(pct)})))


@pytest.mark.parametrize(("minutes", "text"), [(64, "↻1h04"), (120, "↻2h00"), (60, "↻1h00"), (59, "↻59m"),
                                               (42, "↻42m"), (1, "↻1m"), (299, "↻4h59")])
def test_the_countdown_rounds_up_to_the_minute(tmp_path: Path, minutes: int, text: str) -> None:
    assert f"5h 60% {text}" in _plain(_render(tmp_path, _payload({"five_hour": _window(60, minutes)})))


def test_a_reset_too_far_for_a_five_hour_window_hides_the_countdown(tmp_path: Path) -> None:
    """A window that closes in 10 days is a data error; printing '↻240h00' would also break the width budget."""
    far = {"used_percentage": 60, "resets_at": int(time.time()) + 10 * 86400}
    plain = _plain(_render(tmp_path, _payload({"five_hour": far})))
    assert "5h 60%" in plain and "↻" not in plain
    edge = {"used_percentage": 60, "resets_at": int(time.time()) + 36000 - 5}
    assert "↻10h00" in _plain(_render(tmp_path, _payload({"five_hour": edge})))


def test_half_a_minute_left_still_reads_one_minute(tmp_path: Path) -> None:
    payload = _payload({"five_hour": {"used_percentage": 60, "resets_at": int(time.time()) + 30}})
    assert "5h 60% ↻1m" in _plain(_render(tmp_path, payload))


@pytest.mark.parametrize("resets_at", [None, "soon", 0, -5, 1, "1760000000", True])
def test_a_missing_stale_or_junk_reset_hides_only_the_countdown(tmp_path: Path, resets_at: Any) -> None:
    window = {"used_percentage": 60, **({} if resets_at is None else {"resets_at": resets_at})}
    plain = _plain(_render(tmp_path, _payload({"five_hour": window})))
    assert "5h 60%" in plain and "↻" not in plain, plain


def test_the_segment_stays_inside_the_budget_even_at_one_hundred_percent(tmp_path: Path) -> None:
    plain = _plain(_render(tmp_path, _payload({"five_hour": _window(100, 299), "seven_day": _window(100, 9999)})))
    segment = "5h 100% ↻4h59 · 7d 100%"
    assert segment in plain
    assert len(segment) <= 23  # the example in the request is 21 wide with two-digit values


@pytest.mark.parametrize(("tokens", "tier"), [(250_000, "green"), (600_000, "SALVE"), (700_000, "COMPACT")])
def test_the_segment_rides_every_context_tier(tmp_path: Path, tokens: int, tier: str) -> None:
    """The status line has three output lines (green, SALVE, COMPACT); the quota must not vanish from the loud ones."""
    plain = _plain(_render(tmp_path, _payload({"five_hour": _window(60, 64), "seven_day": _window(50)}, tokens=tokens)))
    assert "5h 60% ↻1h04 · 7d 50%" in plain, plain
    if tier != "green":
        assert tier in plain


def test_the_quota_does_not_displace_cost_or_model(tmp_path: Path) -> None:
    payload = _payload({"five_hour": _window(60, 64)})
    payload["cost"] = {"total_cost_usd": 1.5}
    plain = _plain(_render(tmp_path, payload))
    assert plain.index("T") < plain.index("$1.50") < plain.index("5h 60%")


# ── o marcador de alerta (mesmo diretório e ciclo do marcador de contexto) ──────────────────────────


def _flag(home: Path, session: str = SESSION) -> Path:
    return _alerts(home) / f"{session}.quota"


def test_crossing_the_threshold_arms_a_per_session_marker_with_percentage_and_window_key(tmp_path: Path) -> None:
    payload = _payload({"five_hour": {"used_percentage": 87, "resets_at": 1_900_000_000}})
    _render(tmp_path, payload)
    assert _flag(tmp_path).read_text().splitlines() == ["87", "resets=1900000000"]


def test_below_the_threshold_no_marker_exists(tmp_path: Path) -> None:
    _render(tmp_path, _payload({"five_hour": _window(84, 90)}))
    assert not _flag(tmp_path).exists()


def test_only_the_five_hour_window_arms_the_alert(tmp_path: Path) -> None:
    _render(tmp_path, _payload({"five_hour": _window(10, 90), "seven_day": _window(99, 3000)}))
    assert not _flag(tmp_path).exists()


def test_a_reading_without_a_reset_still_arms_with_a_placeholder_key(tmp_path: Path) -> None:
    _render(tmp_path, _payload({"five_hour": _window(90)}))
    assert _flag(tmp_path).read_text().splitlines() == ["90", "resets=-"]


def test_the_marker_is_rewritten_with_the_latest_reading(tmp_path: Path) -> None:
    _render(tmp_path, _payload({"five_hour": {"used_percentage": 86, "resets_at": 1_900_000_000}}))
    _render(tmp_path, _payload({"five_hour": {"used_percentage": 93, "resets_at": 1_900_000_000}}))
    assert _flag(tmp_path).read_text().splitlines()[0] == "93"


def test_dropping_under_the_threshold_clears_the_marker_and_its_delivery_stamp(tmp_path: Path) -> None:
    _render(tmp_path, _payload({"five_hour": {"used_percentage": 90, "resets_at": 1_900_000_000}}))
    delivered = _alerts(tmp_path) / f"{SESSION}.quota.delivered"
    delivered.write_text("1900000000\n")
    _render(tmp_path, _payload({"five_hour": _window(40, 200)}))
    assert not _flag(tmp_path).exists() and not delivered.exists()


@pytest.mark.parametrize("without", [_payload(), _payload({}), _payload({"seven_day": _window(10)}),
                                     _payload({"five_hour": {"used_percentage": "junk"}})])
def test_an_absent_reading_never_disarms(tmp_path: Path, without: dict[str, Any]) -> None:
    """Absent signal is not zero: a render that carries no 5 h reading must not read as 'the window reset'."""
    _render(tmp_path, _payload({"five_hour": {"used_percentage": 90, "resets_at": 1_900_000_000}}))
    delivered = _alerts(tmp_path) / f"{SESSION}.quota.delivered"
    delivered.write_text("1900000000\n")
    _render(tmp_path, without)
    assert _flag(tmp_path).exists() and delivered.exists()


def test_the_marker_is_per_session(tmp_path: Path) -> None:
    _render(tmp_path, _payload({"five_hour": _window(90, 90)}, session="s-one"))
    assert _flag(tmp_path, "s-one").exists() and not _flag(tmp_path, "s-two").exists()


def test_the_threshold_follows_the_environment(tmp_path: Path) -> None:
    _render(tmp_path, _payload({"five_hour": _window(87, 90)}), QUOTA_ALERT_PCT="90")
    assert not _flag(tmp_path).exists()
    _render(tmp_path, _payload({"five_hour": _window(87, 90)}), QUOTA_ALERT_PCT="80")
    assert _flag(tmp_path).exists()


def test_a_payload_without_a_session_id_arms_nothing_and_still_renders(tmp_path: Path) -> None:
    out = _render(tmp_path, _payload({"five_hour": _window(90, 90)}, session=None))
    assert "5h 90%" in _plain(out) and not list(_alerts(tmp_path).glob("*.quota"))


def test_the_context_marker_cycle_is_untouched(tmp_path: Path) -> None:
    _render(tmp_path, _payload({"five_hour": _window(40, 90)}, tokens=600_000))
    assert (_alerts(tmp_path) / f"{SESSION}.pct").read_text().splitlines()[0] == "60"


# ── a entrega: check_context.sh, uma vez por janela, no mesmo canal ─────────────────────────────────


def _check(home: Path, session: str | None = SESSION, **env: str):
    payload: dict[str, Any] = {"hook_event_name": "PostToolUse", "tool_name": "Bash", "tool_input": {}}
    if session is not None:
        payload["session_id"] = session
    return run_hook_sh(CHECK, payload, home=home, extra_env=env or None)


def _arm_quota(home: Path, pct: int = 87, resets: int | str = 1_900_000_000, session: str = SESSION) -> Path:
    path = _flag(home, session)
    path.write_text(f"{pct}\nresets={resets}\n")
    return path


def _journal(home: Path) -> list[dict[str, Any]]:
    path = home / ".claude" / "touring" / "logs" / "context_check_journal.jsonl"
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()] if path.exists() else []


def test_an_armed_quota_is_delivered_once_through_the_post_tool_use_channel(tmp_path: Path) -> None:
    resets = int(time.time()) + 64 * 60
    _arm_quota(tmp_path, 87, resets)
    first = _check(tmp_path)
    assert first.rc == 0 and first.json is not None
    hso = first.json["hookSpecificOutput"]
    assert hso["hookEventName"] == "PostToolUse" and set(first.json) == {"hookSpecificOutput"}
    text = hso["additionalContext"]
    clock = time.strftime("%H:%M", time.localtime(resets))
    assert "87%" in text and "5 h" in text and clock in text
    assert "rate_limits_snapshot.py --show" in text  # the actionable pointer
    assert first.schema_ok("PostToolUse")
    second = _check(tmp_path)
    assert second.rc == 0 and second.json is None and not second.stdout_raw.strip()
    assert (_alerts(tmp_path) / f"{SESSION}.quota.delivered").read_text().strip() == str(resets)


def test_a_new_window_is_announced_again(tmp_path: Path) -> None:
    _arm_quota(tmp_path, 87, 1_900_000_000)
    assert _check(tmp_path).json is not None
    assert _check(tmp_path).json is None
    _arm_quota(tmp_path, 88, 1_900_018_000)  # the next 5 h window crossed the threshold again
    again = _check(tmp_path).json
    assert again is not None and "88%" in again["hookSpecificOutput"]["additionalContext"]


def test_a_marker_whose_window_already_reset_is_not_announced(tmp_path: Path) -> None:
    """The status line has not rendered since the reset: the reading is stale, and 'zera 14:32' would be in the past."""
    _arm_quota(tmp_path, 90, int(time.time()) - 60)
    result = _check(tmp_path)
    assert result.rc == 0 and result.json is None
    assert not (_alerts(tmp_path) / f"{SESSION}.quota.delivered").exists()


def test_no_reset_key_delivers_once_until_the_marker_is_cleared(tmp_path: Path) -> None:
    _arm_quota(tmp_path, 90, "-")
    assert _check(tmp_path).json is not None
    assert _check(tmp_path).json is None


def test_the_quota_alert_is_journalled_as_such(tmp_path: Path) -> None:
    _arm_quota(tmp_path, 87)
    _check(tmp_path)
    spoke = [line for line in _journal(tmp_path) if line["said"] is True]
    assert len(spoke) == 1 and spoke[0]["detail"] == "quota_5h" and spoke[0]["pct"] == 87


def test_context_and_quota_due_together_make_one_message_and_the_context_part_is_unchanged(tmp_path: Path) -> None:
    (_alerts(tmp_path) / f"{SESSION}.pct").write_text("57\nts=1\n")
    _arm_quota(tmp_path, 91)
    both = _check(tmp_path)
    assert both.json is not None
    text = both.json["hookSpecificOutput"]["additionalContext"]
    assert text.startswith(CONTEXT_WARNING_HEAD) and "91%" in text and text.count("Contexto em") == 1
    assert _check(tmp_path).json is None  # both are now delivered


def test_the_context_warning_alone_is_byte_for_byte_what_it_was(tmp_path: Path) -> None:
    (_alerts(tmp_path) / f"{SESSION}.pct").write_text("57\nts=1\n")
    alone = _check(tmp_path)
    assert alone.json is not None and "Franquia" not in alone.json["hookSpecificOutput"]["additionalContext"]
    assert "Cota" not in alone.json["hookSpecificOutput"]["additionalContext"]


def test_quota_delivery_does_not_wait_for_the_context_sensor(tmp_path: Path) -> None:
    """No heartbeat at all (`sem_sensor` for the context) says nothing about the franchise."""
    _arm_quota(tmp_path)
    assert _check(tmp_path).json is not None


def test_a_registered_project_monitor_does_not_swallow_the_quota_alert(tmp_path: Path) -> None:
    project = tmp_path / "proj"
    (project / ".claude").mkdir(parents=True)
    (project / ".claude" / "settings.json").write_text('{"hooks": {"x": "context_monitor.py"}}')
    _arm_quota(tmp_path)
    assert _check(tmp_path, CLAUDE_PROJECT_DIR=str(project)).json is not None


def test_a_marker_without_a_readable_percentage_is_silence_not_a_crash(tmp_path: Path) -> None:
    _flag(tmp_path).write_text("not-a-number\nresets=1900000000\n")  # a FUTURE window: only the percentage is wrong
    result = _check(tmp_path)
    assert result.rc == 0 and result.json is None


def test_no_session_id_means_no_quota_delivery(tmp_path: Path) -> None:
    _arm_quota(tmp_path)
    result = _check(tmp_path, None)
    assert result.rc == 0 and result.json is None


def test_another_sessions_marker_is_not_delivered_here(tmp_path: Path) -> None:
    _arm_quota(tmp_path, session="someone-else")
    assert _check(tmp_path).json is None
