"""Tests for a3_arm.py (A3 arm switch + analysis, S-12).

Synthetic transcripts only (public repo). HOME is redirected to a temp dir for EVERY
test, so nothing here can reach the real ~/.claude (settings, projects, journal).
"""
import datetime as dt
import importlib.util
import json
import os
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent


def _load():
    spec = importlib.util.spec_from_file_location("a3_arm", HERE / "a3_arm.py")
    mod = importlib.util.module_from_spec(spec)
    sys.modules["a3_arm"] = mod  # dataclasses resolve annotations through sys.modules
    spec.loader.exec_module(mod)
    return mod


a3 = _load()
UTC = dt.UTC
PROJECT = "-synthetic-project"


@pytest.fixture(autouse=True)
def fake_home(tmp_path, monkeypatch):
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    return home


def make_cal(**over):
    cal = {
        "day0": "2026-10-28",
        "switch_hour_local": 4,
        "tz": "America/Sao_Paulo",
        "primary_blocks": ["ABBA", "BAAB"],
        "extension_blocks": ["ABBA", "BAAB"],
        "extension_active": False,
        "arms": {"A": "full", "B": "no-advisory"},
        "blocos_invalidados": [],
    }
    cal.update(over)
    return cal


def write_calendar(tmp_path, name="cal.json", **over) -> str:
    path = tmp_path / name
    path.write_text(json.dumps(make_cal(**over)), encoding="utf-8")
    return str(path)


def utc(text: str) -> dt.datetime:
    return a3.parse_ts(text)


# ------------------------------------------------------------------- synthetic corpus


def _record(kind: str, stamp: dt.datetime, content, model: str | None = None) -> dict:
    msg: dict = {"content": content}
    if model:
        msg["model"] = model
    return {"type": kind, "timestamp": stamp.isoformat().replace("+00:00", "Z"), "message": msg}


def write_session(home, name, start, calls, bad, *, model="model-x", step=2, kind="erro", late_model=None):
    """Transcript with ``calls`` Read calls, ``bad`` of them failing as ``kind``.

    ``late_model`` replaces ``model`` on the second half of the calls (a mid-session switch).
    """
    folder = home / ".claude" / "projects" / PROJECT
    folder.mkdir(parents=True, exist_ok=True)
    texts = {
        "erro": ("boom", True),
        "negado": ("PreToolUse:Read hook error: [G6 repeat] denied", True),
        "vazio": ("[]", False),
    }
    records = [_record("user", start, "synthetic prompt")]
    stamp = start
    for i in range(calls):
        stamp = start + dt.timedelta(seconds=step * (i + 1))
        use = {"type": "tool_use", "id": f"t{i}", "name": "Read", "input": {"file_path": f"/synthetic/{name}/{i}"}}
        used = late_model if late_model and i >= calls // 2 else model
        records.append(_record("assistant", stamp, [use], used))
        text, is_err = texts[kind] if i < bad else ("x" * 80, False)
        res = {"type": "tool_result", "tool_use_id": f"t{i}", "content": text, "is_error": is_err}
        records.append(_record("user", stamp + dt.timedelta(seconds=1), [res]))
    path = folder / f"{name}.jsonl"
    path.write_text("\n".join(json.dumps(r) for r in records) + "\n", encoding="utf-8")
    epoch = stamp.timestamp() + 1
    os.utime(path, (epoch, epoch))
    return path


def arm_days(cal, block, arm):
    pattern, first, _end = a3.block_spans(cal)[block]
    return [first + i for i, letter in enumerate(pattern) if letter == arm]


def fill_cell(home, cal, block, arm, bads, *, calls=40, model="model-x", kind="erro", tag=""):
    """One session per entry of ``bads`` (its failing-call count), spread over the cell's days."""
    days = arm_days(cal, block, arm)
    for i, bad in enumerate(bads):
        day = days[i % len(days)]
        start = a3.day_start_utc(cal, day) + dt.timedelta(hours=2, minutes=7 * (i // len(days)))
        write_session(home, f"b{block}{arm}{tag}{i:02d}", start, calls, bad, model=model, kind=kind)


def fill_all(home, cal, a_bads, b_bads, blocks=(0, 1)):
    for block in blocks:
        fill_cell(home, cal, block, "A", a_bads)
        fill_cell(home, cal, block, "B", b_bads)


def run_analyze(tmp_path, cal_path, as_of, capsys, *extra):
    rc = a3.main(
        ["analyze", "--calendar", cal_path, f"--dirs={PROJECT}", "--as-of", as_of, "--json", *extra]
    )
    out = capsys.readouterr().out
    assert rc == 0, out
    return json.loads(out)


END_PRIMARY = "2026-11-06T12:00:00Z"  # after day 8 closes (2026-11-05 04:00 local)
END_EXTENDED = "2026-11-14T12:00:00Z"  # after day 16 closes

# Scenarios: failing calls per 40-call session (12 sessions per cell, 2 blocks).
NON_INFERIOR = ([1] * 12, [1] * 8 + [0] * 2 + [2] * 2)  # d = 0
INFERIOR = ([0] * 12, [2] * 6 + [3] * 6)  # d = -6.25
CROSSING = ([1] * 12, [1] * 6 + [2] * 6)  # d = -1.25, CI straddles -1
WIDE = ([1] * 12, [0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3])  # d = -0.52, wide variance


# ------------------------------------------------------------------------- arm_for


def test_arm_for_across_the_0400_boundary():
    cal = make_cal()
    # 04:00 BRT = 07:00 UTC (no DST in America/Sao_Paulo since 2019).
    assert a3.arm_for("2026-10-28T06:59:59Z", cal) is None  # 03:59:59 local, still day -1
    assert a3.arm_for("2026-10-28T07:00:00Z", cal) == (0, "A")
    assert a3.arm_for("2026-10-29T06:59:59.999Z", cal) == (0, "A")  # last instant of day 0
    assert a3.arm_for("2026-10-29T07:00:00Z", cal) == (0, "B")


def test_arm_for_follows_the_pattern_and_the_block_boundary():
    cal = make_cal()
    letters = []
    for day in range(8):
        stamp = a3.day_start_utc(cal, day) + dt.timedelta(hours=5)
        letters.append(a3.arm_for(stamp.isoformat(), cal))
    assert letters == [(0, "A"), (0, "B"), (0, "B"), (0, "A"), (1, "B"), (1, "A"), (1, "A"), (1, "B")]


def test_arm_for_none_when_day0_is_null_before_day0_or_after_the_end():
    assert a3.arm_for("2026-10-30T12:00:00Z", make_cal(day0=None)) is None
    cal = make_cal()
    assert a3.arm_for("2026-10-20T12:00:00Z", cal) is None
    assert a3.arm_for("2026-11-05T06:59:59Z", cal) == (1, "B")  # last second of day 7
    assert a3.arm_for("2026-11-05T07:00:00Z", cal) is None  # day 8 does not exist yet


def test_arm_for_extends_only_when_the_extension_is_active():
    stamp = "2026-11-05T12:00:00Z"
    assert a3.arm_for(stamp, make_cal()) is None
    assert a3.arm_for(stamp, make_cal(extension_active=True)) == (2, "A")
    assert a3.arm_for("2026-11-13T12:00:00Z", make_cal(extension_active=True)) is None


def test_calendar_shipped_in_the_bundle_is_inactive_and_valid():
    cal = a3.load_calendar(a3.CALENDAR_PATH)
    assert cal["day0"] is None
    assert cal["extension_active"] is False
    assert cal["arms"] == {"A": "full", "B": "no-advisory"}
    assert a3.arm_for("2026-11-01T12:00:00Z", cal) is None


@pytest.mark.parametrize(
    "bad",
    [
        {"day0": "28/10/2026"},
        {"switch_hour_local": 24},
        {"tz": "Mars/Olympus"},
        {"arms": {"A": "full"}},
        {"primary_blocks": []},
        {"primary_blocks": ["ABCA"]},
        {"extension_active": "no"},
        {"blocos_invalidados": [{"block": 9, "motivo": "x"}]},
        {"blocos_invalidados": [{"block": 0}]},
    ],
)
def test_invalid_calendar_is_refused(tmp_path, bad, capsys):
    path = write_calendar(tmp_path, **bad)
    assert a3.main(["status", "--calendar", path]) == 2
    assert "recusado" in capsys.readouterr().err


# ------------------------------------------------------------------------- status


def test_status_reports_the_arm_or_inactivity(tmp_path, capsys):
    path = write_calendar(tmp_path)
    now = utc("2026-10-29T12:00:00Z")  # day 1 -> B
    assert a3.main(["status", "--calendar", path], now=now) == 0
    out = capsys.readouterr().out
    assert "braço B" in out
    assert "no-advisory" in out
    assert a3.main(["status", "--calendar", write_calendar(tmp_path, day0=None)], now=now) == 0
    assert "experimento não ativo" in capsys.readouterr().out


# -------------------------------------------------------------------------- apply


def write_settings(tmp_path, env=None, trailing_newline=False):
    data = {"$schema": "x", "env": {"KEEP": "ção"} if env is None else env, "model": "m", "last": [1, 2]}
    path = tmp_path / "settings.json"
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + ("\n" if trailing_newline else ""), encoding="utf-8")
    return path


def backups(path):
    return sorted(path.parent.glob(path.name + ".bak-*"))


def test_apply_dry_run_leaves_the_file_byte_identical(tmp_path, capsys):
    cal = write_calendar(tmp_path)
    settings = write_settings(tmp_path)
    before = settings.read_bytes()
    rc = a3.main(["apply", "--calendar", cal, "--settings", str(settings)], now=utc("2026-10-29T12:00:00Z"))
    assert rc == 0
    out = capsys.readouterr().out
    assert settings.read_bytes() == before
    assert not backups(settings)
    assert "dry-run" in out
    assert '+ "no-advisory"' in out
    assert "- (ausente)" in out


def test_apply_yes_sets_the_key_for_arm_b_with_backup(tmp_path):
    cal = write_calendar(tmp_path)
    settings = write_settings(tmp_path, trailing_newline=True)
    original = settings.read_bytes()
    now = utc("2026-10-29T12:00:00Z")  # B
    assert a3.main(["apply", "--yes", "--calendar", cal, "--settings", str(settings)], now=now) == 0
    data = json.loads(settings.read_text(encoding="utf-8"))
    assert data["env"] == {"KEEP": "ção", a3.ENV_KEY: "no-advisory"}
    assert (data["$schema"], data["model"], data["last"]) == ("x", "m", [1, 2])
    assert settings.read_text(encoding="utf-8").endswith("}\n")
    assert "ção" in settings.read_text(encoding="utf-8")  # ensure_ascii=False
    (backup,) = backups(settings)
    assert backup.read_bytes() == original


def test_apply_yes_removes_the_key_for_arm_a(tmp_path):
    cal = write_calendar(tmp_path)
    settings = write_settings(tmp_path, env={"KEEP": "1", a3.ENV_KEY: "no-advisory"})
    now = utc("2026-10-28T12:00:00Z")  # day 0 -> A
    assert a3.main(["apply", "--yes", "--calendar", cal, "--settings", str(settings)], now=now) == 0
    data = json.loads(settings.read_text(encoding="utf-8"))
    assert data["env"] == {"KEEP": "1"}
    assert len(backups(settings)) == 1
    assert not settings.read_text(encoding="utf-8").endswith("\n")  # newline status preserved


def test_apply_creates_env_when_the_file_has_none(tmp_path):
    cal = write_calendar(tmp_path)
    settings = tmp_path / "settings.json"
    settings.write_text('{"model": "m"}', encoding="utf-8")
    now = utc("2026-10-29T12:00:00Z")
    assert a3.main(["apply", "--yes", "--calendar", cal, "--settings", str(settings)], now=now) == 0
    assert json.loads(settings.read_text(encoding="utf-8")) == {"model": "m", "env": {a3.ENV_KEY: "no-advisory"}}


def test_apply_is_idempotent_and_writes_no_second_backup(tmp_path, capsys):
    cal = write_calendar(tmp_path)
    settings = write_settings(tmp_path)
    argv = ["apply", "--yes", "--calendar", cal, "--settings", str(settings)]
    now = utc("2026-10-29T12:00:00Z")
    assert a3.main(argv, now=now) == 0
    after_first = settings.read_bytes()
    assert a3.main(argv, now=now) == 0
    assert "sem mudança" in capsys.readouterr().out
    assert settings.read_bytes() == after_first
    assert len(backups(settings)) == 1


def test_apply_refuses_when_the_experiment_is_not_active(tmp_path, capsys):
    settings = write_settings(tmp_path)
    before = settings.read_bytes()
    now = utc("2026-10-29T12:00:00Z")
    inactive = (
        write_calendar(tmp_path, "null.json", day0=None),
        write_calendar(tmp_path, "future.json", day0="2026-12-01"),
    )
    for cal in inactive:
        argv = ["apply", "--yes", "--calendar", cal, "--settings", str(settings)]
        assert a3.main(argv, now=now) == 2
        assert "nada a aplicar" in capsys.readouterr().err
    assert settings.read_bytes() == before
    assert not backups(settings)


def test_apply_refuses_malformed_or_missing_settings(tmp_path):
    cal = write_calendar(tmp_path)
    now = utc("2026-10-29T12:00:00Z")
    bad = tmp_path / "bad.json"
    bad.write_text("{not json", encoding="utf-8")
    assert a3.main(["apply", "--yes", "--calendar", cal, "--settings", str(bad)], now=now) == 2
    assert bad.read_text(encoding="utf-8") == "{not json"
    assert a3.main(["apply", "--calendar", cal, "--settings", str(tmp_path / "nope.json")], now=now) == 2
    listy = tmp_path / "env-list.json"
    listy.write_text('{"env": []}', encoding="utf-8")
    assert a3.main(["apply", "--yes", "--calendar", cal, "--settings", str(listy)], now=now) == 2


# ------------------------------------------------------------------- analyze: pooling


def test_cell_pooling_counts_all_outcome_classes_and_skips_short_sessions(tmp_path, fake_home, capsys):
    cal = make_cal()
    path = write_calendar(tmp_path)
    # Block 0 / A: 10 sessions x 40 calls; sessions 0..2 carry 2 erro + 1 negado + 1 vazio each.
    for i in range(10):
        day = arm_days(cal, 0, "A")[i % 2]
        start = a3.day_start_utc(cal, day) + dt.timedelta(hours=2, minutes=i)
        write_session(fake_home, f"a{i}", start, 40, 2 if i < 3 else 0)
    start = a3.day_start_utc(cal, 0) + dt.timedelta(hours=6)
    write_session(fake_home, "neg", start, 40, 1, kind="negado")
    write_session(fake_home, "vaz", start + dt.timedelta(minutes=1), 40, 1, kind="vazio")
    write_session(fake_home, "short", start + dt.timedelta(minutes=2), 39, 39)  # < 40 calls: ignored
    rep = run_analyze(tmp_path, path, END_PRIMARY, capsys)
    cell = rep["blocos"][0]["celulas"]["A"]
    assert cell["sessoes"] == 12
    assert cell["sessoes_total"] == 13
    assert cell["calls"] == 480
    assert (cell["erro"], cell["negado"], cell["vazio"]) == (6, 1, 1)
    assert cell["eficacia_pct"] == pytest.approx(100 - 100 * 8 / 480, abs=1e-4)
    assert cell["negado_pct"] == pytest.approx(100 / 480, abs=1e-4)
    assert rep["corpus"]["sessoes_na_sequencia"] == 13


def test_session_belongs_to_the_arm_of_its_first_record(tmp_path, fake_home, capsys):
    cal = make_cal()
    path = write_calendar(tmp_path)
    # Opens 30 min before the day-1 switch (so on day 0, arm A) and runs past 04:00.
    start = a3.day_start_utc(cal, 1) - dt.timedelta(minutes=30)
    write_session(fake_home, "cross", start, 45, 0, step=120)
    # Opens before day 0 and ends inside it: outside the sequence, never counted.
    write_session(fake_home, "early", a3.day_start_utc(cal, 0) - dt.timedelta(minutes=30), 45, 0, step=120)
    rep = run_analyze(tmp_path, path, END_PRIMARY, capsys)
    assert rep["corpus"]["sessoes_na_sequencia"] == 1
    assert rep["blocos"][0]["celulas"]["A"]["sessoes_total"] == 1
    assert rep["blocos"][0]["celulas"]["B"]["sessoes_total"] == 0


def test_files_older_than_day0_are_not_read(tmp_path, fake_home, capsys):
    cal = make_cal()
    path = write_calendar(tmp_path)
    old = write_session(fake_home, "old", a3.day_start_utc(cal, 0) + dt.timedelta(hours=3), 45, 0)
    epoch = a3.day_start_utc(cal, 0).timestamp() - 3600
    os.utime(old, (epoch, epoch))
    rep = run_analyze(tmp_path, path, END_PRIMARY, capsys)
    assert rep["corpus"]["arquivos"] == 0


# ----------------------------------------------------------- analyze: decision branches


def test_decision_non_inferior(tmp_path, fake_home, capsys):
    cal = make_cal()
    fill_all(fake_home, cal, *NON_INFERIOR)
    rep = run_analyze(tmp_path, write_calendar(tmp_path), END_PRIMARY, capsys)
    est = rep["estimativa"]
    assert est["blocos_validos"] == 2
    assert est["d_medio_pp"] == pytest.approx(0.0, abs=1e-6)
    assert est["ic95_pp"][0] > -1.0
    assert rep["decisao"]["resultado"] == "nao-inferior"
    assert rep["decisao"]["regra"] == "ic95"
    for blk in rep["blocos"]:
        assert blk["estado"] == "valido"
        assert blk["celulas"]["A"]["eficacia_pct"] == pytest.approx(100 - 100 * 12 / 480, abs=1e-4)
        assert blk["celulas"]["B"]["eficacia_pct"] == pytest.approx(100 - 100 * 12 / 480, abs=1e-4)


def test_decision_inferior(tmp_path, fake_home, capsys):
    cal = make_cal()
    fill_all(fake_home, cal, *INFERIOR)
    rep = run_analyze(tmp_path, write_calendar(tmp_path), END_PRIMARY, capsys)
    assert rep["estimativa"]["d_medio_pp"] == pytest.approx(-6.25, abs=1e-6)
    assert rep["estimativa"]["ic95_pp"][1] < -1.0
    assert rep["decisao"]["resultado"] == "inferior"
    assert rep["decisao"]["acao"] == "fica full"


def test_decision_extend_when_the_ci_crosses_the_margin(tmp_path, fake_home, capsys):
    cal = make_cal()
    fill_all(fake_home, cal, *CROSSING)
    rep = run_analyze(tmp_path, write_calendar(tmp_path), END_PRIMARY, capsys)
    lower, upper = rep["estimativa"]["ic95_pp"]
    assert lower < -1.0 < upper
    assert rep["estimativa"]["d_medio_pp"] == pytest.approx(-1.25, abs=1e-4)
    assert rep["decisao"]["resultado"] == "estender"


def test_after_the_extension_the_point_estimate_decides(tmp_path, fake_home, capsys):
    cal = make_cal(extension_active=True)
    fill_all(fake_home, cal, *WIDE, blocks=(0, 1, 2, 3))
    path = write_calendar(tmp_path, extension_active=True)
    rep = run_analyze(tmp_path, path, END_EXTENDED, capsys)
    lower, _upper = rep["estimativa"]["ic95_pp"]
    assert rep["estimativa"]["blocos_validos"] == 4
    assert lower < -1.0  # the CI still crosses the margin ...
    assert rep["estimativa"]["d_medio_pp"] >= -1.0  # ... and the point estimate decides
    assert rep["decisao"]["resultado"] == "nao-inferior"
    assert rep["decisao"]["regra"] == "estimativa-pontual-pos-extensao"
    # Same data, only the two primary blocks in force: the CI rule asks for the extension.
    primary = run_analyze(tmp_path, write_calendar(tmp_path), END_PRIMARY, capsys)
    assert primary["decisao"]["resultado"] == "estender"


def test_after_the_extension_a_point_estimate_below_the_margin_stays_on_full(tmp_path, fake_home, capsys):
    cal = make_cal(extension_active=True)
    fill_all(fake_home, cal, *CROSSING, blocks=(0, 1, 2, 3))
    rep = run_analyze(tmp_path, write_calendar(tmp_path, extension_active=True), END_EXTENDED, capsys)
    assert rep["estimativa"]["d_medio_pp"] < -1.0
    assert rep["decisao"] == {
        "resultado": "inferior",
        "regra": "estimativa-pontual-pos-extensao",
        "acao": "fica full",
    }


def test_blocks_still_running_keep_the_decision_pending(tmp_path, fake_home, capsys):
    cal = make_cal()
    fill_all(fake_home, cal, *NON_INFERIOR)
    mid = "2026-11-03T12:00:00Z"  # inside block 1
    rep = run_analyze(tmp_path, write_calendar(tmp_path), mid, capsys)
    assert [b["estado"] for b in rep["blocos"]] == ["valido", "em_curso"]
    assert rep["decisao"]["resultado"] == "pendente"
    assert "em curso" in rep["decisao"]["motivo"]
    assert rep["estimativa"]["blocos_validos"] == 1


def test_cell_under_ten_sessions_invalidates_the_block(tmp_path, fake_home, capsys):
    cal = make_cal()
    fill_cell(fake_home, cal, 0, "A", NON_INFERIOR[0])
    fill_cell(fake_home, cal, 0, "B", NON_INFERIOR[1])
    fill_cell(fake_home, cal, 1, "A", NON_INFERIOR[0])
    fill_cell(fake_home, cal, 1, "B", [1] * 9)  # one short of the minimum
    rep = run_analyze(tmp_path, write_calendar(tmp_path), END_PRIMARY, capsys)
    assert [b["estado"] for b in rep["blocos"]] == ["valido", "celula_abaixo_do_minimo"]
    assert "B=9" in rep["blocos"][1]["motivo"]
    assert rep["blocos"][1]["d_pp"] is None
    assert rep["estimativa"]["blocos_validos"] == 1
    assert rep["decisao"]["resultado"] == "pendente"
    assert rep["blocos_invalidados"][0]["block"] == 1


def test_calendar_invalidated_block_is_skipped_with_its_reason(tmp_path, fake_home, capsys):
    cal = make_cal()
    fill_all(fake_home, cal, *NON_INFERIOR)
    path = write_calendar(tmp_path, blocos_invalidados=[{"block": 0, "motivo": "update-touring no dia 2"}])
    rep = run_analyze(tmp_path, path, END_PRIMARY, capsys)
    assert rep["blocos"][0]["estado"] == "invalidado"
    assert rep["blocos"][0]["motivo"] == "update-touring no dia 2"
    assert rep["blocos"][0]["d_pp"] is None
    assert rep["estimativa"]["blocos_validos"] == 1
    assert rep["blocos_invalidados"] == [
        {"block": 0, "estado": "invalidado", "motivo": "update-touring no dia 2"}
    ]
    assert rep["decisao"]["resultado"] == "pendente"


# -------------------------------------------------------------------------- bootstrap


def test_bootstrap_is_deterministic_for_a_seed_and_moves_with_it():
    pairs = [
        ([(40, 1)] * 6 + [(40, 2)] * 6, [(40, 1)] * 4 + [(40, 3)] * 8),
        ([(40, 0)] * 5 + [(40, 2)] * 7, [(40, 2)] * 6 + [(40, 1)] * 6),
    ]
    first = a3.bootstrap_ci(pairs, seed=a3.SEED, resamples=500)
    assert a3.bootstrap_ci(pairs, seed=a3.SEED, resamples=500) == first
    assert a3.bootstrap_ci(pairs, seed=1, resamples=500) != first
    assert first[0] < first[1]
    assert a3.bootstrap_ci([], resamples=10) is None


def test_analyze_output_is_reproducible(tmp_path, fake_home, capsys):
    cal = make_cal()
    fill_all(fake_home, cal, *CROSSING)
    path = write_calendar(tmp_path)
    first = run_analyze(tmp_path, path, END_PRIMARY, capsys)
    again = run_analyze(tmp_path, path, END_PRIMARY, capsys)
    assert first == again
    assert first["semente"] == 20261027
    assert first["reamostragens"] == 2000


# ------------------------------------------------------------ analyze: sensitivity etc


def test_sensitivity_drops_sessions_that_cross_a_switch(tmp_path, fake_home, capsys):
    cal = make_cal()
    fill_all(fake_home, cal, *NON_INFERIOR)
    # Three bad sessions that open on day 0 (arm A) and run past the day-1 switch.
    for i in range(3):
        start = a3.day_start_utc(cal, 1) - dt.timedelta(minutes=30, seconds=i)
        write_session(fake_home, f"cross{i}", start, 45, 20, step=120)
    rep = run_analyze(tmp_path, write_calendar(tmp_path), END_PRIMARY, capsys)
    sens = rep["sensibilidade"]
    assert sens["sessoes_excluidas"] == 3
    assert sens["estimativa"]["d_medio_pp"] == pytest.approx(0.0, abs=1e-6)
    assert rep["estimativa"]["d_medio_pp"] > 1.0  # the crossing sessions drag A down
    assert sens["blocos_validos"] == [0, 1]


def test_per_model_split_uses_the_first_assistant_message_model(tmp_path, fake_home, capsys):
    cal = make_cal()
    for block in (0, 1):
        fill_cell(fake_home, cal, block, "A", [1] * 6, model="model-x", tag="x")
        fill_cell(fake_home, cal, block, "A", [1] * 6, model="model-y", tag="y")
        fill_cell(fake_home, cal, block, "B", [1] * 12, model="model-x")
    rep = run_analyze(tmp_path, write_calendar(tmp_path), END_PRIMARY, capsys)
    models = rep["secundarios"]["por_modelo"]
    assert set(models) == {"model-x", "model-y"}
    assert models["model-x"]["A"]["sessoes"] == 12
    assert models["model-y"]["A"]["sessoes"] == 12
    assert models["model-x"]["B"]["sessoes"] == 24
    assert models["model-x"]["d_pp"] == pytest.approx(0.0, abs=1e-6)
    assert models["model-y"]["B"]["sessoes"] == 0  # no B session ran on that model
    assert models["model-y"]["d_pp"] is None


def test_model_is_the_one_of_the_first_assistant_record_not_the_last(tmp_path, fake_home):
    cal = make_cal()
    start = a3.day_start_utc(cal, 0) + dt.timedelta(hours=3)
    path = write_session(fake_home, "switch", start, 44, 0, model="model-x", late_model="model-y")
    sess = a3.scan_session(str(path), cal)
    assert sess.model == "model-x"
    assert sess.calls == 44


def test_secondary_retry_and_recovery_are_reported_per_arm(tmp_path, fake_home, capsys):
    cal = make_cal()
    fill_all(fake_home, cal, *NON_INFERIOR)
    for block in (0, 1):  # denied sessions in the B cells: kind=negado
        fill_cell(fake_home, cal, block, "B", [3] * 2, kind="negado", tag="n")
    rep = run_analyze(tmp_path, write_calendar(tmp_path), END_PRIMARY, capsys)
    recov = rep["secundarios"]["recuperacao_pos_deny"]
    assert recov["A"]["negacoes"] == 0
    assert recov["B"]["negacoes"] > 0
    assert recov["B"]["recuperou_pct"] is not None
    for blk in rep["blocos"]:
        assert blk["celulas"]["B"]["retry_pct"] == pytest.approx(0.0)
        assert blk["celulas"]["B"]["negado_pct"] is not None


def write_journal(home, cal):
    """Journal lines in the schema of injection_journal.rs ``line_for`` (ts in unix ms)."""

    def ms(day, hours):
        return int((a3.day_start_utc(cal, day) + dt.timedelta(hours=hours)).timestamp() * 1000)

    def line(day, hours, cuts, session=None):
        rec = {"ts": ms(day, hours), "level": "no-advisory", "site": "s",
               "cuts": [{"family": f, "bytes": n} for f, n in cuts]}
        if session:
            rec["session"] = session
        return rec

    lines = [
        line(1, 3, [("touring-suggest", 400)]),
        line(2, 5, [("gate-advice", 100), ("touring-suggest", 50)], session="b0B00"),  # a B-cell session
        line(0, 5, [("touring-suggest", 7)]),  # a cut on an A day
        line(20, 5, [("touring-suggest", 999)]),  # far past the sequence end
    ]
    journal = home / ".claude" / "touring" / "injection_suppressed.jsonl"
    journal.parent.mkdir(parents=True, exist_ok=True)
    journal.write_text("\n".join(json.dumps(x) for x in lines) + "\nnot json\n", encoding="utf-8")


def test_dose_comes_from_the_journal_and_marks_its_absence(tmp_path, fake_home, capsys):
    cal = make_cal()
    fill_all(fake_home, cal, *NON_INFERIOR)
    path = write_calendar(tmp_path)
    rep = run_analyze(tmp_path, path, END_PRIMARY, capsys)
    assert rep["secundarios"]["dose"]["diario_ausente"] is True  # absent signal is not zero

    write_journal(fake_home, cal)
    dose = run_analyze(tmp_path, path, END_PRIMARY, capsys)["secundarios"]["dose"]
    assert dose["diario_ausente"] is False
    assert dose["bytes_total_sequencia"] == 400 + 150 + 7  # the line far past the end is out
    cells = {(c["block"], c["braco"]): c for c in dose["por_celula"]}
    assert cells[(0, "B")]["bytes_janela"] == 550
    assert cells[(0, "B")]["por_nivel"] == {"no-advisory": 550}
    assert cells[(0, "B")]["bytes_sessoes"] == 150
    assert cells[(0, "A")]["bytes_janela"] == 7  # a cut on an A day: the arm check to eyeball
    assert dose["por_familia"]["touring-suggest"] == 457


# -------------------------------------------------------------------- CLI behaviour


def test_analyze_refuses_when_not_activated(tmp_path, capsys):
    assert a3.main(["analyze", "--calendar", write_calendar(tmp_path, day0=None)]) == 2
    assert "day0 = null" in capsys.readouterr().err


def test_analyze_refuses_a_non_positive_sessions_cap(tmp_path, capsys):
    assert a3.main(["analyze", "--calendar", write_calendar(tmp_path), "--sessions", "0"]) == 2
    assert "--sessions" in capsys.readouterr().err


def test_sessions_cap_that_cuts_into_the_window_is_flagged(tmp_path, fake_home, capsys):
    cal = make_cal()
    fill_cell(fake_home, cal, 0, "A", [0, 0])
    rep = run_analyze(tmp_path, write_calendar(tmp_path), END_PRIMARY, capsys, "--sessions", "1")
    assert rep["corpus"]["arquivos"] == 1
    assert rep["corpus"]["truncado"] is True
    assert any("--sessions" in note for note in rep["ressalvas"])
    full = run_analyze(tmp_path, write_calendar(tmp_path), END_PRIMARY, capsys, "--sessions", "50")
    assert full["corpus"]["truncado"] is False


def test_analyze_text_summary_has_the_decision_line(tmp_path, fake_home, capsys):
    cal = make_cal()
    fill_all(fake_home, cal, *INFERIOR)
    argv = ["analyze", "--calendar", write_calendar(tmp_path), f"--dirs={PROJECT}", "--as-of", END_PRIMARY]
    rc = a3.main(argv)
    out = capsys.readouterr().out
    assert rc == 0
    assert "decisão: inferior" in out
    assert "bloco 0 ABBA [valido]" in out


def test_a_transcript_the_instrument_cannot_measure_is_listed_not_dropped_silently(
    tmp_path, fake_home, capsys, monkeypatch
):
    cal = make_cal()
    fill_all(fake_home, cal, *NON_INFERIOR)
    # The instrument fails on one transcript (forced: the real trigger — a dict
    # input.command — was fixed in measure_tool_trio.tool_class).
    start = a3.day_start_utc(cal, 0) + dt.timedelta(hours=9)
    use = {"type": "tool_use", "id": "t0", "name": "Bash", "input": {"command": "ls"}}
    path = fake_home / ".claude" / "projects" / PROJECT / "malformed.jsonl"
    lines = [_record("user", start, "p"), _record("assistant", start + dt.timedelta(seconds=1), [use], "model-x")]
    path.write_text("\n".join(json.dumps(x) for x in lines) + "\n", encoding="utf-8")
    os.utime(path, (start.timestamp() + 5, start.timestamp() + 5))
    real = a3.m.iter_records

    def failing(p, *args, **kwargs):
        if str(p).endswith("malformed.jsonl"):
            raise TypeError("forced instrument failure")
        return real(p, *args, **kwargs)

    monkeypatch.setattr(a3.m, "iter_records", failing)
    rep = run_analyze(tmp_path, write_calendar(tmp_path), END_PRIMARY, capsys)
    assert rep["corpus"]["nao_medidos"] == [{"arquivo": "malformed.jsonl", "erro": "TypeError"}]
    assert rep["decisao"]["resultado"] == "nao-inferior"  # the rest of the corpus still decides
    assert any("fora da análise" in note for note in rep["ressalvas"])


def test_instrument_is_imported_not_duplicated():
    assert a3.m.__name__ == "measure_tool_trio"
    assert not hasattr(a3, "classify_bash")
    assert not hasattr(a3, "outcome")
