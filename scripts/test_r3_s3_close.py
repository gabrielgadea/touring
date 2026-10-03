import json
import pathlib
import subprocess
import sys

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import r3_s3_close as sc  # noqa: E402

BASE_T = 1_790_964_950_615
BASE = {"t_ms": BASE_T, "arm": {"both": {"offered": 0}, "code": {"offered": 485}, "native": {"offered": 0}}}
DAY_MS = 24 * 3_600_000
ZERO = {k: 0 for k in sc.COUNTER_KEYS}


def arm(code):
    return {"both": {"offered": 0}, "code": {"offered": code}, "native": {"offered": 0}}


def ev(n, offered_after_base=True, kind="a2_offer_closed"):
    off = BASE_T // 1000 + (100 if offered_after_base else -100)
    return [{"kind": kind, "offered_secs": off} for _ in range(n)]


DAEMON_BEFORE_BASE = BASE_T - 1_000


def ev_eval(offers, closed, counters=ZERO, hours=24.0, daemon_start_ms=DAEMON_BEFORE_BASE, **kw):
    return sc.evaluate(BASE, arm(485 + offers), ev(closed), counters, BASE_T + int(hours * 3_600_000),
                       daemon_start_ms=daemon_start_ms, **kw)


def test_offered_total_sums_arms_for_the_envelope_and_the_raw_file():
    assert sc.offered_total(BASE) == 485
    assert sc.offered_total(arm(7)) == 7
    assert sc.offered_total({"code": {"offered": True}}) == 0
    assert sc.offered_total({"both": {"offered": 3}, "code": {"offered": 5}, "native": {"offered": 2}}) == 10


def test_closed_in_window_counts_only_offers_made_after_the_base():
    events = ev(3) + ev(2, offered_after_base=False) + ev(4, kind="a1_block") + [{"kind": "a2_offer_closed"}]
    assert sc.closed_in_window(events, BASE_T) == 3


def test_closed_in_window_includes_an_offer_exactly_at_the_base():
    assert sc.closed_in_window([{"kind": "a2_offer_closed", "offered_secs": BASE_T / 1000}], BASE_T) == 1


def test_pass_at_full_coverage():
    r = ev_eval(16, 16)
    assert (r["status"], r["coverage"], r["may_stop_unit"]) == (sc.PASS, 1.0, True)


def test_pass_at_exactly_the_threshold():
    assert ev_eval(20, 19)["status"] == sc.PASS


def test_a_deficit_below_the_threshold_is_not_demonstrated_never_failed():
    r = ev_eval(20, 18)
    assert r["status"] == sc.NOT_DEMONSTRATED and r["unexplained"] == 2 and r["may_stop_unit"] is True


def test_no_offers_cannot_pass():
    r = ev_eval(0, 0)
    assert r["status"] == sc.NOT_DEMONSTRATED and r["coverage"] is None


@pytest.mark.parametrize("key", sc.COUNTER_KEYS)
def test_any_loss_counter_fails_even_with_full_coverage(key):
    r = ev_eval(10, 10, counters={**ZERO, key: 1})
    assert r["status"] == sc.FAILED and r["may_stop_unit"] is False


def test_missing_counters_are_unmeasured_and_block_the_stop():
    for bad in (None, {"emitter_dropped_since_start": 0}, {**ZERO, "foreign_lines": None}):
        r = ev_eval(10, 10, counters=bad)
        assert r["status"] == sc.UNMEASURED and r["may_stop_unit"] is False


def test_a_short_window_is_interim():
    r = ev_eval(10, 10, hours=23.0)
    assert r["status"] == sc.INTERIM and r["may_stop_unit"] is False


def test_the_preregistered_thresholds_are_pinned():
    assert (sc.COVERAGE_MIN, sc.MIN_WINDOW_H) == (0.95, 23.5)


def test_window_threshold_is_inclusive():
    assert ev_eval(10, 10, hours=23.5)["status"] == sc.PASS
    assert ev_eval(10, 10, hours=23.49)["status"] == sc.INTERIM


def test_more_closed_lines_than_offers_is_inconsistent():
    r = ev_eval(5, 6)
    assert r["status"] == sc.INCONSISTENT and r["may_stop_unit"] is False


def test_a_counter_below_the_base_is_unmeasured():
    r = sc.evaluate(BASE, arm(400), ev(1), ZERO, BASE_T + DAY_MS, daemon_start_ms=DAEMON_BEFORE_BASE)
    assert r["status"] == sc.UNMEASURED and r["offers_in_window"] == -85


def test_a_status_masked_by_a_higher_one_is_still_reported():
    r = ev_eval(5, 6, counters={**ZERO, "foreign_lines": 1})
    assert r["status"] == sc.FAILED and r["also"] == [sc.INCONSISTENT]
    r = ev_eval(5, 6, hours=1.0)
    assert r["status"] == sc.INTERIM and r["also"] == [sc.INCONSISTENT]
    assert ev_eval(10, 10)["also"] == []


def test_the_loss_counter_covers_the_window_only_if_the_daemon_started_before_the_base():
    assert ev_eval(10, 10, daemon_start_ms=BASE_T)["loss_counter_covers_window"] is True
    r = ev_eval(10, 10, daemon_start_ms=BASE_T + 1)
    assert r["loss_counter_covers_window"] is False and "DAEMON_REINICIADO_NA_JANELA" in r["flags"]


def test_an_unknown_daemon_start_blocks_the_stop_but_not_the_verdict():
    r = ev_eval(10, 10, daemon_start_ms=None)
    assert r["status"] == sc.PASS and r["may_stop_unit"] is False and "INICIO_DO_DAEMON_DESCONHECIDO" in r["flags"]


def test_a_daemon_restart_in_the_window_blocks_the_stop_even_on_pass():
    r = ev_eval(10, 10, daemon_start_ms=BASE_T + 5_000)
    assert r["status"] == sc.PASS and r["may_stop_unit"] is False


def test_an_unsettled_counter_is_flagged_and_the_pass_stands_as_a_lower_bound():
    r = ev_eval(10, 10, settled=False)
    assert r["status"] == sc.PASS and "CONTADOR_AINDA_SE_MOVIA" in r["flags"]
    assert "CONTADOR_AINDA_SE_MOVIA" not in ev_eval(10, 10)["flags"]


def test_distinct_sessions_in_the_window_are_reported_for_audit():
    events = [{"kind": "a2_offer_closed", "offered_secs": BASE_T // 1000 + 5, "session_hash": h} for h in ("a", "a", "b")]
    r = sc.evaluate(BASE, arm(488), events, ZERO, BASE_T + DAY_MS, daemon_start_ms=DAEMON_BEFORE_BASE)
    assert r["distinct_sessions"] == 2


def stat_line(start_ticks, comm="touring-daemon"):
    return f"99 ({comm}) S " + " ".join(["1"] * 18) + f" {start_ticks} " + " ".join(["0"] * 6)


def test_proc_start_epoch_reads_field_22_after_the_last_parenthesis():
    assert sc.proc_start_epoch(stat_line(5000), 1_000_000, 100) == 1_000_050.0
    assert sc.proc_start_epoch(stat_line(5000, comm="a b) c"), 1_000_000, 100) == 1_000_050.0


def fake_proc(tmp_path, pid="321", comm="touring-daemon", ticks=5000, btime=1_000_000):
    proc = tmp_path / "proc"
    (proc / pid).mkdir(parents=True)
    (proc / pid / "comm").write_text(comm + "\n")
    (proc / pid / "stat").write_text(stat_line(ticks))
    (proc / "stat").write_text(f"cpu  1 2 3\nbtime {btime}\nprocesses 9\n")
    pidf = tmp_path / "d.pid"
    pidf.write_text(pid + "\n")
    return pidf, proc


def test_read_daemon_start_ms_reads_the_pid_file_and_proc(tmp_path):
    pidf, proc = fake_proc(tmp_path)
    assert sc.read_daemon_start_ms(pidf, proc=proc, hz=100) == 1_000_050_000


def test_read_daemon_start_ms_refuses_a_pid_that_is_not_the_daemon(tmp_path):
    pidf, proc = fake_proc(tmp_path, comm="touring-mcp")
    assert sc.read_daemon_start_ms(pidf, proc=proc, hz=100) is None


def test_read_daemon_start_ms_is_none_when_anything_is_missing(tmp_path):
    pidf, proc = fake_proc(tmp_path)
    assert sc.read_daemon_start_ms(tmp_path / "nao-existe.pid", proc=proc, hz=100) is None
    pidf.write_text("nao-e-numero\n")
    assert sc.read_daemon_start_ms(pidf, proc=proc, hz=100) is None
    pidf, proc = fake_proc(tmp_path / "x")
    (proc / "stat").write_text("sem btime\n")
    assert sc.read_daemon_start_ms(pidf, proc=proc, hz=100) is None


def test_loss_evidence_outranks_a_short_window():
    assert ev_eval(10, 10, counters={**ZERO, "emitter_dropped_since_start": 3}, hours=1.0)["status"] == sc.FAILED


class Clock:
    def __init__(self):
        self.t = 0.0

    def now(self):
        return self.t

    def sleep(self, s):
        self.t += s


def same(v):
    return v


def test_settle_returns_once_the_counter_has_been_quiet_long_enough():
    c = Clock()
    val, ok = sc.settle(lambda: 7, key=same, sleep=c.sleep, clock=c.now, quiet_s=120, max_wait_s=600, poll_s=15)
    assert (val, ok) == (7, True) and c.t == 120


def test_settle_defaults_to_the_offered_total_and_returns_the_last_document():
    c = Clock()
    docs = iter([arm(5)] * 20)
    doc, ok = sc.settle(lambda: next(docs), sleep=c.sleep, clock=c.now, quiet_s=30, max_wait_s=600, poll_s=15)
    assert ok is True and doc == arm(5)
    c = Clock()
    n = iter(range(1000))
    doc, ok = sc.settle(lambda: arm(next(n)), sleep=c.sleep, clock=c.now, quiet_s=120, max_wait_s=60, poll_s=15)
    assert ok is False and sc.offered_total(doc) >= 4


def test_a_repeated_status_is_not_listed_as_also():
    r = sc.evaluate(BASE, arm(400), ev(1), None, BASE_T + DAY_MS, daemon_start_ms=DAEMON_BEFORE_BASE)
    assert r["status"] == sc.UNMEASURED and r["also"] == [sc.INCONSISTENT]


def test_settle_compares_by_the_key_not_by_the_whole_document():
    c = Clock()
    n = iter(range(1000))
    _, ok = sc.settle(lambda: {"code": {"offered": 5, "followed": next(n)}}, sleep=c.sleep, clock=c.now,
                      quiet_s=30, max_wait_s=600, poll_s=15)
    assert ok is True and c.t == 30


def test_settle_restarts_the_quiet_period_when_the_counter_moves():
    c = Clock()
    seq = iter([1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2])
    val, ok = sc.settle(lambda: next(seq), key=same, sleep=c.sleep, clock=c.now, quiet_s=30, max_wait_s=600, poll_s=15)
    assert (val, ok) == (2, True) and c.t == 60  # moveu na leitura de t=30; quieto de t=30 a t=60


def test_settle_gives_up_at_the_ceiling_when_the_counter_never_stops():
    c = Clock()
    n = iter(range(1000))
    val, ok = sc.settle(lambda: next(n), key=same, sleep=c.sleep, clock=c.now, quiet_s=120, max_wait_s=60, poll_s=15)
    assert ok is False and c.t >= 60


def run_stub(stdout="", rc=0):
    return lambda *a, **k: subprocess.CompletedProcess(a, rc, stdout=stdout, stderr="")


def test_kpi_counters_finds_the_block_at_any_depth():
    doc = {"a": [{"x": 1}, {"shadow": {**ZERO, "emitter_dropped_since_start": 2}}]}
    assert sc.kpi_counters(run=run_stub(json.dumps(doc)))["emitter_dropped_since_start"] == 2


@pytest.mark.parametrize("stdout,rc", [("nao e json", 0), (json.dumps({"a": 1}), 0), (json.dumps({"emitter_dropped_since_start": 0}), 0), ("{}", 1), (json.dumps(ZERO), 1)])
def test_kpi_counters_returns_none_on_any_failure(stdout, rc):
    assert sc.kpi_counters(run=run_stub(stdout, rc)) is None


def test_kpi_counters_returns_none_when_the_call_raises():
    def boom(*a, **k):
        raise subprocess.TimeoutExpired("touring", 90)

    assert sc.kpi_counters(run=boom) is None


def test_read_events_skips_and_counts_unreadable_lines(tmp_path):
    p = tmp_path / "e.jsonl"
    p.write_text('{"kind":"a1_block"}\nlixo\n\n[1,2]\n{"kind":"a2_offer_closed"}\n')
    events, bad = sc.read_events(p)
    assert [e["kind"] for e in events] == ["a1_block", "a2_offer_closed"] and bad == 2


def test_stop_unit_reports_the_systemctl_result():
    calls = []

    def run(cmd, **k):
        calls.append(cmd)
        return subprocess.CompletedProcess(cmd, 0, stdout="inactive\n" if "is-active" in cmd else "", stderr="")

    assert sc.stop_unit("u", run=run) == {"stop_rc": 0, "active_after": "inactive"}
    assert calls[0][:3] == ["systemctl", "--user", "stop"] and calls[1][2] == "is-active"


def fixture(tmp_path, offers, closed):
    (tmp_path / "base.json").write_text(json.dumps({"t_ms": 1000, "arm": {"code": {"offered": 10}}}))
    (tmp_path / "arm.json").write_text(json.dumps({"code": {"offered": 10 + offers}}))
    lines = [json.dumps({"kind": "a2_offer_closed", "offered_secs": 5}) for _ in range(closed)]
    (tmp_path / "ev.jsonl").write_text("\n".join(lines) + "\n")
    return ["--base", str(tmp_path / "base.json"), "--arm", str(tmp_path / "arm.json"),
            "--events", str(tmp_path / "ev.jsonl"), "--out", str(tmp_path / "out.json"), "--quiet-s", "0"]


@pytest.fixture
def stubs(monkeypatch):
    stopped = []
    monkeypatch.setattr(sc, "kpi_counters", lambda *a, **k: dict(ZERO))
    monkeypatch.setattr(sc, "read_daemon_start_ms", lambda *a, **k: 0)  # nasceu antes da base (t_ms = 1000)
    monkeypatch.setattr(sc, "stop_unit", lambda unit=sc.UNIT, **k: stopped.append(unit) or {"stop_rc": 0, "active_after": "inactive"})
    monkeypatch.setattr(sc.time, "time", lambda: 1000 / 1000 + 30 * 3600)  # 30 h depois da base
    return stopped


def test_main_stops_the_unit_on_pass_and_writes_the_artifact(tmp_path, stubs):
    rc = sc.main(fixture(tmp_path, 8, 8) + ["--stop-unit"])
    out = json.loads((tmp_path / "out.json").read_text())
    assert rc == 0 and out["status"] == sc.PASS and stubs == [sc.UNIT] and out["stop"]["active_after"] == "inactive"


def test_main_does_not_stop_the_unit_without_the_flag(tmp_path, stubs):
    sc.main(fixture(tmp_path, 8, 8))
    assert stubs == [] and json.loads((tmp_path / "out.json").read_text())["stop"] == "nao_pedido"


def test_main_refuses_to_stop_when_the_verdict_shows_loss(tmp_path, stubs, monkeypatch):
    monkeypatch.setattr(sc, "kpi_counters", lambda *a, **k: {**ZERO, "foreign_lines": 4})
    rc = sc.main(fixture(tmp_path, 8, 8) + ["--stop-unit"])
    out = json.loads((tmp_path / "out.json").read_text())
    assert rc == 1 and out["status"] == sc.FAILED and stubs == [] and out["stop"] == "recusado_pelo_veredito"


def test_main_keeps_the_pass_but_refuses_to_stop_when_the_daemon_restarted_in_the_window(tmp_path, stubs, monkeypatch):
    monkeypatch.setattr(sc, "read_daemon_start_ms", lambda *a, **k: 5_000)  # depois da base (t_ms = 1000)
    rc = sc.main(fixture(tmp_path, 8, 8) + ["--stop-unit"])
    out = json.loads((tmp_path / "out.json").read_text())
    assert rc == 0 and out["status"] == sc.PASS and stubs == [] and out["stop"] == "recusado_pelo_veredito"
    assert out["daemon_start_ms"] == 5_000 and "DAEMON_REINICIADO_NA_JANELA" in out["flags"]


def test_main_exits_2_and_never_stops_when_a_file_is_unreadable(tmp_path, stubs):
    argv = fixture(tmp_path, 8, 8) + ["--stop-unit"]
    (tmp_path / "arm.json").unlink()
    rc = sc.main(argv)
    out = json.loads((tmp_path / "out.json").read_text())
    assert rc == 2 and out["status"] == sc.UNMEASURED and stubs == []
