"""Tests for ab34_arm.py (AB34 arm switch and analysis, experiment 3.4).

Synthetic transcripts and a fake host only (public repo). HOME is redirected to a temp
dir for EVERY test, so nothing here can reach the real ~/.claude (settings, projects,
arm log) and no touring process is ever started, stopped or restarted.
"""

import datetime as dt
import fcntl
import importlib.util
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]


def _load():
    spec = importlib.util.spec_from_file_location("ab34_arm", HERE / "ab34_arm.py")
    mod = importlib.util.module_from_spec(spec)
    sys.modules["ab34_arm"] = mod  # dataclasses resolve annotations through sys.modules
    spec.loader.exec_module(mod)
    return mod


ab = _load()
PROJECT = "-synthetic-project"
DAY0 = "2026-11-02"
SEED = 20261105


@pytest.fixture(autouse=True)
def fake_home(tmp_path, monkeypatch):
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    return home


def make_cal(**over):
    cal = ab.make_calendar(SEED, DAY0)
    cal["transcript_dirs"] = [PROJECT]
    cal.update(over)
    return cal


@pytest.fixture
def env(tmp_path):
    """A calendar, a settings.json and an arm log path, all private to the test."""
    cal_path = tmp_path / "cal.json"
    cal_path.write_text(json.dumps(make_cal()), encoding="utf-8")
    settings = tmp_path / "settings.json"
    settings.write_text(
        json.dumps({"env": {"KEEP": "1"}, "hooks": {"a": [1]}}, indent=2) + "\n"
    )
    return SimpleNamespace(
        cal=str(cal_path),
        settings=str(settings),
        log=str(tmp_path / "ab34" / "arm.jsonl"),
        tmp=tmp_path,
    )


def noon(k, cal=None):
    cal = cal or make_cal()
    return ab.day_start_utc(cal, k) + dt.timedelta(hours=12)


class FakeHost:
    """The daemon, the binaries and the KPI, in memory. ``lose_race`` restarts come back
    with the flag unchanged: a hook autostart from a flag-less session won the lock."""

    def __init__(self, flag_on=False, lose_race=0, daemon_up=True):
        self.up = daemon_up
        self.flag_on = flag_on
        self.pid = 4000
        self.lose_race = lose_race
        self.restarts: list[bool] = []
        self.touring_sha = "t" * 64
        self.daemon_sha = "d" * 64
        self.kpi = {"runs_total": 100, "runs_ok": 90, "wasted_attempts_retry_pairs": 4}

    def daemon_state(self, sock):
        if not self.up:
            return None
        return {
            "pid": self.pid,
            "exe": "/x/touring-daemon",
            "exe_sha": self.daemon_sha,
            "cwd": "/w",
            "flag_raw": ab.ON if self.flag_on else None,
            "flag_on": self.flag_on,
        }

    def restart_daemon(self, sock, want_on, state):
        self.restarts.append(want_on)
        self.pid += 1
        self.up = True
        if self.lose_race > 0:
            self.lose_race -= 1
        else:
            self.flag_on = want_on

    def identity(self, state):
        return {
            "touring_version": "touring 1.0",
            "touring_sha": self.touring_sha,
            "daemon_exe": "/x/touring-daemon",
            "daemon_sha": self.daemon_sha,
            "hook_sha": "h" * 64,
        }

    def kpi_snapshot(self):
        return self.kpi

    def sleep(self, seconds):
        pass


def run(env, *argv, host=None, now=None):
    common = ["--calendar", env.cal, "--settings", env.settings, "--log", env.log]
    return ab.main([argv[0], *common, *argv[1:]], host=host, now=now)


def read_settings(env):
    return json.loads(Path(env.settings).read_text(encoding="utf-8"))


def log_records(env):
    return [
        json.loads(line)
        for line in Path(env.log).read_text(encoding="utf-8").splitlines()
    ]


def backups(env):
    return sorted(Path(env.settings).parent.glob("settings.json.bak-*"))


# ----------------------------------------------------------------------------- calendar


def test_shipped_calendar_is_balanced_two_days_per_cell():
    cal = json.loads((HERE / "ab34-calendario.json").read_text(encoding="utf-8"))
    ab.validate_calendar(cal)
    cells = [cell for _block, cell in ab.flat_days(cal)]
    assert sorted(cells) == sorted(ab.CELLS * 2)
    assert sum(ab.want_flags(c)[0] for c in cells) == 4  # C on 4 days of 8
    assert sum(ab.want_flags(c)[1] for c in cells) == 4  # E on 4 days of 8
    assert all(sorted(block) == sorted(ab.CELLS) for block in cal["blocks"])


def test_shipped_calendar_is_the_one_its_seed_makes():
    cal = json.loads((HERE / "ab34-calendario.json").read_text(encoding="utf-8"))
    assert cal["blocks"] == ab.schedule_from_seed(cal["seed"])
    assert cal["day0"] is None  # the orchestrator sets it, in a commit of its own
    assert cal == ab.make_calendar(cal["seed"])


def test_seed_is_reproducible_and_the_shuffle_is_pinned():
    assert ab.schedule_from_seed(SEED) == ab.schedule_from_seed(SEED)
    # Golden value: a different shuffle algorithm would silently reorder the experiment.
    assert ab.schedule_from_seed(SEED) == [
        ["C-", "CE", "-E", "--"],
        ["-E", "C-", "--", "CE"],
    ]


def test_shuffle_covers_every_permutation():
    seen = {tuple(ab.schedule_from_seed(s)[0]) for s in range(2000)}
    assert len(seen) == 24


@pytest.mark.parametrize("seed", range(20261101, 20261141))
def test_every_valid_seed_gives_a_balanced_design(seed):
    cal = ab.make_calendar(seed)
    try:
        ab.validate_calendar(cal)
    except ab.Refusal:
        return  # rejected seeds (repeated position) are covered separately
    cells = [cell for _b, cell in ab.flat_days(cal)]
    assert all(cells.count(c) == 2 for c in ab.CELLS)


def test_seed_with_a_repeated_position_is_refused():
    # 20261101 draws two identical blocks; the validator refuses it, the seed is not used.
    with pytest.raises(ab.Refusal, match="mesma posição"):
        ab.validate_calendar(ab.make_calendar(20261101))


@pytest.mark.parametrize(
    ("mutate", "message"),
    [
        (lambda c: c.pop("seed"), "sem as chaves"),
        (lambda c: c.update(day0="03/11/2026"), "day0"),
        (lambda c: c.update(tz="Mars/Olympus"), "tz desconhecido"),
        (lambda c: c.update(seed=True), "seed"),
        (lambda c: c.update(seed=SEED + 1), "reproduz a semente"),
        (lambda c: c.update(blocks=[]), "lista não vazia"),
        (lambda c: c["blocks"][0].reverse(), "reproduz a semente"),
        (lambda c: c["blocks"][1].__setitem__(0, "C-"), "4 células"),
        (lambda c: c.update(blocks=c["blocks"] * 3), "no máximo"),
        (lambda c: c.update(daemon_socket="daemon.sock"), "absoluto"),
        (lambda c: c.update(transcript_dirs=[]), "transcript_dirs"),
    ],
)
def test_malformed_calendar_fails_loud(mutate, message):
    cal = make_cal()
    mutate(cal)
    with pytest.raises(ab.Refusal, match=message):
        ab.validate_calendar(cal)


@pytest.mark.parametrize("payload", ["not json{", "[1, 2]"])
def test_malformed_calendar_file_exits_2_with_a_message(env, payload, capsys):
    Path(env.cal).write_text(payload, encoding="utf-8")
    assert run(env, "status", host=FakeHost()) == 2
    assert "recusado" in capsys.readouterr().err


def test_missing_calendar_file_exits_2(env, capsys):
    os.unlink(env.cal)
    assert run(env, "status", host=FakeHost()) == 2
    assert "calendário ilegível" in capsys.readouterr().err


def test_calendar_command_prints_what_the_seed_makes(capsys):
    assert ab.main(["calendar", "--seed", str(SEED)]) == 0
    assert json.loads(capsys.readouterr().out) == ab.make_calendar(SEED)
    assert ab.main(["calendar", "--seed", "20261101"]) == 2  # repeated-position seed


def test_day_boundary_is_local_midnight():
    cal = make_cal()
    before = dt.datetime(2026, 11, 3, 2, 59, 59, tzinfo=dt.UTC)  # 23:59:59 in Sao Paulo
    after = dt.datetime(2026, 11, 3, 3, 0, 0, tzinfo=dt.UTC)  # 00:00:00 in Sao Paulo
    assert ab.level_now(cal, before)[0] == 0
    assert ab.level_now(cal, after)[0] == 1
    assert ab.level_now(cal, ab.day_start_utc(cal, 0) - dt.timedelta(seconds=1)) is None
    assert ab.level_now(cal, ab.day_start_utc(cal, 8)) is None
    assert ab.level_now(make_cal(day0=None), after) is None


# ---------------------------------------------------------------------------- arm / reset


@pytest.mark.parametrize("k", range(8))
def test_arm_writes_the_flags_of_each_day(env, k):
    cal = make_cal()
    cell = ab.flat_days(cal)[k][1]
    want_c, want_e = ab.want_flags(cell)
    host = FakeHost()
    assert run(env, "arm", "--yes", host=host, now=noon(k)) == 0
    settings = read_settings(env)
    assert (settings["env"].get(ab.FLAG_E) == "on") is want_e
    assert settings["env"]["KEEP"] == "1" and settings["hooks"] == {"a": [1]}
    assert host.flag_on is want_c
    (rec,) = log_records(env)
    assert (rec["day"], rec["cell"], rec["ok"]) == (k, cell, True)
    assert rec["want"] == {"C": want_c, "E": want_e}
    assert rec["binary"]["touring_sha"] == "t" * 64


# An oracle that does not call the code under test: the shipped calendar, day by day, as
# (compactor on, eviction on). `want_flags` and the per-day test above would agree on a
# swapped meaning; this table would not.
SHIPPED_FLAGS = [
    (True, False),  # day 0, C-
    (True, True),  # day 1, CE
    (False, True),  # day 2, -E
    (False, False),  # day 3, --
    (False, True),  # day 4, -E
    (True, False),  # day 5, C-
    (False, False),  # day 6, --
    (True, True),  # day 7, CE
]


@pytest.mark.parametrize("k", range(8))
def test_arm_writes_the_literal_flags_of_the_shipped_calendar(env, k):
    host = FakeHost()
    assert run(env, "arm", "--yes", host=host, now=noon(k)) == 0
    want_c, want_e = SHIPPED_FLAGS[k]
    assert host.flag_on is want_c
    assert (read_settings(env)["env"].get(ab.FLAG_E) == "on") is want_e


def test_want_flags_reads_the_cell_letters():
    assert ab.want_flags("--") == (False, False)
    assert ab.want_flags("C-") == (True, False)
    assert ab.want_flags("-E") == (False, True)
    assert ab.want_flags("CE") == (True, True)


def test_arm_flips_both_flags_when_the_cell_changes(env):
    host = FakeHost()
    cal = make_cal()
    first_ce = next(k for k, (_b, c) in enumerate(ab.flat_days(cal)) if c == "CE")
    first_off = next(k for k, (_b, c) in enumerate(ab.flat_days(cal)) if c == "--")
    assert run(env, "arm", "--yes", host=host, now=noon(first_ce)) == 0
    assert host.flag_on and read_settings(env)["env"][ab.FLAG_E] == "on"
    assert run(env, "arm", "--yes", host=host, now=noon(first_off)) == 0
    assert not host.flag_on and ab.FLAG_E not in read_settings(env)["env"]


def test_arm_is_idempotent(env):
    host = FakeHost()
    now = noon(0)  # day 0 is C- (compactor only)
    assert run(env, "arm", "--yes", host=host, now=now) == 0
    settings_bytes = Path(env.settings).read_bytes()
    restarts, bak = len(host.restarts), len(backups(env))
    assert run(env, "arm", "--yes", host=host, now=now + dt.timedelta(hours=1)) == 0
    assert Path(env.settings).read_bytes() == settings_bytes
    assert len(host.restarts) == restarts and len(backups(env)) == bak
    first, second = log_records(env)
    assert first["actions"] and second["actions"] == []  # the 2nd run is a heartbeat
    assert second["ok"] and second["saw"]["daemon_pid"] == first["after"]["daemon_pid"]


def test_arm_dry_run_writes_nothing(env, capsys):
    host = FakeHost()
    before = Path(env.settings).read_bytes()
    assert run(env, "arm", host=host, now=noon(0)) == 0
    assert "dry-run" in capsys.readouterr().out
    assert Path(env.settings).read_bytes() == before
    assert host.restarts == [] and not Path(env.log).exists() and not backups(env)


def test_arm_makes_a_backup_only_when_it_changes_settings(env):
    host = FakeHost()
    # day 0 is C-: settings unchanged (E off), only the daemon moves
    assert run(env, "arm", "--yes", host=host, now=noon(0)) == 0
    assert backups(env) == []
    assert run(env, "arm", "--yes", host=host, now=noon(1)) == 0  # day 1 is CE
    assert len(backups(env)) == 1


def test_arm_refuses_outside_the_experiment(env, capsys):
    host = FakeHost()
    assert run(env, "arm", "--yes", host=host, now=noon(8)) == 4
    assert "não ativo" in capsys.readouterr().err
    assert host.restarts == [] and not Path(env.log).exists()


def test_arm_retries_when_a_hook_autostart_wins_the_race(env):
    host = FakeHost(lose_race=1)
    assert run(env, "arm", "--yes", host=host, now=noon(0)) == 0
    assert host.restarts == [True, True] and host.flag_on
    (rec,) = log_records(env)
    assert rec["ok"] and "x2" in rec["actions"][-1]


def test_arm_reports_failure_when_the_flag_never_sticks(env):
    host = FakeHost(lose_race=99)
    assert run(env, "arm", "--yes", host=host, now=noon(0)) == 3
    assert len(host.restarts) == ab.MAX_RESTARTS
    (rec,) = log_records(env)
    assert rec["ok"] is False and rec["after"]["C_daemon_env"] is None


def test_arm_spawns_a_missing_daemon_with_the_right_flag(env):
    host = FakeHost(daemon_up=False)
    assert run(env, "arm", "--yes", host=host, now=noon(0)) == 0
    assert host.flag_on and host.restarts == [True]
    assert log_records(env)[0]["saw"]["daemon_pid"] is None


def test_arm_leaves_a_running_arm_alone(env, capsys):
    host = FakeHost()
    Path(env.log).parent.mkdir(parents=True)
    with open(Path(env.log).parent / "arm.lock", "w", encoding="utf-8") as held:
        fcntl.flock(held, fcntl.LOCK_EX)
        assert run(env, "arm", "--yes", host=host, now=noon(0)) == 0
    assert "em andamento" in capsys.readouterr().out
    assert host.restarts == [] and not Path(env.log).exists()


def test_arm_refuses_a_settings_file_that_is_not_json(env):
    Path(env.settings).write_text("{oops", encoding="utf-8")
    assert run(env, "arm", "--yes", host=FakeHost(), now=noon(0)) == 2


def test_status_shows_the_cell_and_the_flags(env, capsys):
    assert run(env, "status", host=FakeHost(flag_on=True), now=noon(1)) == 0
    out = capsys.readouterr().out
    assert "dia 1/7" in out and "célula CE" in out and "'on'" in out


def test_reset_refuses_while_a_block_is_running(env, capsys):
    host = FakeHost(flag_on=True)
    settings = json.loads(Path(env.settings).read_text())
    settings["env"][ab.FLAG_E] = "on"
    Path(env.settings).write_text(json.dumps(settings))
    before = Path(env.settings).read_bytes()
    for k in (0, 3, 4, 7):  # first, last of block 0, first, last of block 1
        assert run(env, "reset", "--yes", host=host, now=noon(k)) == 4
    assert "reset recusado" in capsys.readouterr().err
    assert Path(env.settings).read_bytes() == before
    assert host.restarts == [] and host.flag_on and not Path(env.log).exists()


def test_reset_after_the_end_turns_both_flags_off(env):
    host = FakeHost(flag_on=True)
    settings = json.loads(Path(env.settings).read_text())
    settings["env"][ab.FLAG_E] = "on"
    Path(env.settings).write_text(json.dumps(settings))
    assert run(env, "reset", "--yes", host=host, now=noon(8)) == 0
    assert not host.flag_on and ab.FLAG_E not in read_settings(env)["env"]
    assert len(backups(env)) == 1
    (rec,) = log_records(env)
    assert rec["event"] == "reset" and rec["ok"]
    restarts = len(host.restarts)
    assert run(env, "reset", "--yes", host=host, now=noon(9)) == 0  # idempotent
    assert len(host.restarts) == restarts and len(backups(env)) == 1


def test_reset_with_an_extension_keeps_the_arms(env):
    cal = make_cal()
    assert ab.level_now(cal, noon(7)) is not None
    host = FakeHost(flag_on=True)
    assert run(env, "reset", "--yes", host=host, now=noon(7)) == 4


def test_reset_is_allowed_when_day0_is_null(env):
    Path(env.cal).write_text(json.dumps(make_cal(day0=None)), encoding="utf-8")
    host = FakeHost(flag_on=True)
    assert run(env, "reset", "--yes", host=host, now=noon(0)) == 0
    assert not host.flag_on


def test_preflight_uses_the_same_mechanism_outside_the_experiment(env):
    Path(env.cal).write_text(json.dumps(make_cal(day0=None)), encoding="utf-8")
    host = FakeHost()
    assert run(env, "arm", "--yes", "--force-cell", "CE", host=host, now=noon(0)) == 0
    assert host.flag_on and read_settings(env)["env"][ab.FLAG_E] == "on"
    assert log_records(env)[0]["event"] == "preflight"


def test_preflight_is_refused_while_the_experiment_runs(env, capsys):
    host = FakeHost()
    assert run(env, "arm", "--yes", "--force-cell", "CE", host=host, now=noon(0)) == 4
    assert "inativo" in capsys.readouterr().err and host.restarts == []
    Path(env.cal).write_text(json.dumps(make_cal(day0=None)), encoding="utf-8")
    assert run(env, "arm", "--yes", "--force-cell", "ZZ", host=host, now=noon(0)) == 2


def test_restart_env_changes_exactly_one_variable():
    base = {
        "HOME": "/h",
        "KIMI_API_KEY": "secret",
        "TOURING_DAEMON_SOCKET": "/s",
        ab.FLAG_C: "on",
    }
    off = ab.restart_env(base, want_on=False)
    assert off == {k: v for k, v in base.items() if k != ab.FLAG_C}
    on = ab.restart_env({k: v for k, v in base.items() if k != ab.FLAG_C}, want_on=True)
    assert on == {**{k: v for k, v in base.items() if k != ab.FLAG_C}, ab.FLAG_C: "on"}
    assert base[ab.FLAG_C] == "on"  # the input is never mutated


def test_flag_predicate_matches_the_daemon():
    assert all(ab.flag_is_on(v) for v in ("on", "1", "true", "yes", " ON ", "True"))
    assert not any(ab.flag_is_on(v) for v in (None, "", "0", "off", "no", "onn"))


# ---------------------------------------------------------------- real Host (stubbed touring)


def _stub_touring(tmp_path, body):
    path = tmp_path / "touring-stub"
    path.write_text("#!/bin/sh\n" + body, encoding="utf-8")
    path.chmod(0o755)
    host = ab.Host()
    host.touring = str(path)
    return host


@pytest.fixture
def fake_daemon(tmp_path):
    """A child whose /proc comm is ``touring-daemon`` and whose environment has the flag."""
    link = tmp_path / "touring-daemon"
    link.symlink_to("/bin/sleep")
    child = subprocess.Popen(
        [str(link), "60"],
        env={"PATH": "/usr/bin", ab.FLAG_C: "on", "MARK": "x"},
        cwd="/",
    )
    yield child
    child.terminate()
    child.wait(timeout=5)


def test_host_reads_the_daemon_environment_from_proc(tmp_path, fake_daemon):
    status = json.dumps(
        {"socket_alive": True, "daemon_pid": fake_daemon.pid, "daemon_exe": "/x"}
    )
    host = _stub_touring(tmp_path, f"echo '{status}'\n")
    state = host.daemon_state("/tmp/s.sock")
    assert (
        state["pid"] == fake_daemon.pid
        and state["flag_on"]
        and state["flag_raw"] == "on"
    )
    assert state["cwd"] == "/" and len(state["exe_sha"]) == 64


def test_host_refuses_a_process_that_is_not_the_daemon(tmp_path):
    status = json.dumps(
        {"socket_alive": True, "daemon_pid": os.getpid(), "daemon_exe": "/x"}
    )
    host = _stub_touring(tmp_path, f"echo '{status}'\n")
    assert host.daemon_state("/tmp/s.sock") is None  # REGRA #19: comm is verified


def test_host_daemon_state_is_none_when_the_socket_is_dead(tmp_path):
    host = _stub_touring(tmp_path, "echo '{\"socket_alive\": false}'\n")
    assert host.daemon_state("/tmp/s.sock") is None


def test_host_restart_runs_under_the_old_environment_with_one_change(
    tmp_path, fake_daemon
):
    out = tmp_path / "seen.txt"
    host = _stub_touring(tmp_path, f'echo "$@" > {out}; env >> {out}; pwd >> {out}\n')
    state = {"pid": fake_daemon.pid, "cwd": "/"}
    host.restart_daemon("/tmp/s.sock", want_on=False, state=state)
    seen = out.read_text(encoding="utf-8")
    assert "daemon-ctl restart --socket /tmp/s.sock -j" in seen
    assert ab.FLAG_C not in seen and "MARK=x" in seen  # flag gone, the rest kept
    assert seen.strip().splitlines()[-1] == "/"
    host.restart_daemon(
        "/tmp/s.sock", want_on=True, state=None
    )  # no daemon: caller's env
    assert f"{ab.FLAG_C}=on" in out.read_text(encoding="utf-8")


def test_host_restart_failure_is_loud(tmp_path):
    host = _stub_touring(tmp_path, "echo boom >&2; exit 7\n")
    with pytest.raises(RuntimeError, match="exit 7"):
        host.restart_daemon("/tmp/s.sock", want_on=True, state=None)


def test_host_identity_reads_the_version_from_stderr(tmp_path):
    host = _stub_touring(tmp_path, "echo 'touring 30.4.101 (git abc)' >&2\n")
    ident = host.identity({"exe": "/x/d", "exe_sha": "e" * 64})
    assert ident["touring_version"] == "touring 30.4.101 (git abc)"
    assert len(ident["touring_sha"]) == 64 and ident["daemon_sha"] == "e" * 64


def test_host_kpi_snapshot_keeps_the_cumulative_counters(tmp_path):
    kpi = {
        "code_mode_adherence": {
            "runs_total": 9,
            "runs_ok": 8,
            "wasted_attempts_retry_pairs": 2,
            "x": 1,
        }
    }
    host = _stub_touring(tmp_path, f"echo '{json.dumps(kpi)}'\n")
    assert host.kpi_snapshot() == {
        "runs_total": 9,
        "runs_ok": 8,
        "wasted_attempts_retry_pairs": 2,
    }
    assert _stub_touring(tmp_path, "echo nothing\n").kpi_snapshot() is None


# ------------------------------------------------------------------- the log and the days


def rec(
    day,
    *,
    ok=True,
    actions=(),
    tsha="a" * 64,
    dsha="b" * 64,
    stamp=None,
    event="arm",
    kpi=None,
):
    cal = make_cal()
    return {
        "ts": (
            stamp or ab.day_start_utc(cal, day) + dt.timedelta(seconds=30)
        ).isoformat(),
        "event": event,
        "day": day,
        "ok": ok,
        "actions": list(actions),
        "binary": {"touring_sha": tsha, "daemon_sha": dsha},
        "kpi": kpi,
    }


def exclusions(records_by_day, k):
    cal = make_cal()
    binaries = ab.block_binaries(cal, records_by_day)
    return ab.day_exclusions(cal, k, records_by_day.get(k, []), binaries)


def test_a_block_with_two_binaries_drops_all_its_days():
    by_day = {k: [rec(k)] for k in range(8)}
    by_day[2] = [rec(2, dsha="c" * 64)]  # a release landed inside block 0
    for k in range(4):
        assert exclusions(by_day, k) == ["binario_misto"], k
    for k in range(4, 8):
        assert exclusions(by_day, k) == [], k


def test_a_release_between_blocks_is_allowed():
    by_day = {k: [rec(k, tsha="a" * 64 if k < 4 else "9" * 64)] for k in range(8)}
    assert all(exclusions(by_day, k) == [] for k in range(8))


def test_a_binary_change_inside_one_day_is_caught():
    by_day = {k: [rec(k)] for k in range(8)}
    by_day[5] = [rec(5), rec(5, tsha="f" * 64)]
    assert "binario_misto" in exclusions(by_day, 5)
    assert exclusions(by_day, 0) == []


def test_an_unknown_binary_is_not_treated_as_the_same_one():
    by_day = {k: [rec(k)] for k in range(8)}
    by_day[6] = [rec(6, dsha="")]
    for k in range(4, 8):
        assert exclusions(by_day, k) == ["binario_ausente"]


def test_a_day_without_a_record_or_unverified_or_drifting_is_excluded():
    by_day = {k: [rec(k)] for k in range(8)}
    del by_day[1]
    by_day[2] = [rec(2, ok=False)]
    by_day[3] = [
        rec(3, actions=["daemon:restart x1"]),
        rec(3),
        rec(3, actions=["daemon:restart x1"]),
    ]
    assert exclusions(by_day, 1) == ["sem_registro"]
    assert exclusions(by_day, 2) == ["nao_verificado"]
    assert exclusions(by_day, 3) == ["deriva"]
    assert exclusions({0: [rec(0, actions=["x"])]}, 0) == []  # the first record arms


def test_read_log_counts_bad_lines_and_orders_by_time(env):
    path = Path(env.log)
    path.parent.mkdir(parents=True)
    late, early = rec(1), rec(0)
    path.write_text(
        "\n".join(
            [
                json.dumps(late),
                "not json",
                json.dumps({"ts": "x", "event": "arm"}),
                json.dumps({**early, "event": "other"}),
                json.dumps(early),
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    records, bad = ab.read_log(env.log)
    assert [r["day"] for r in records] == [0, 1] and bad == 3
    assert ab.read_log(str(path) + ".missing") == ([], 0)


# --------------------------------------------------------------------------- statistics


def test_t_table_known_values():
    assert ab.t_quantile(1) == 12.706 and ab.t_quantile(4) == 2.776
    assert ab.t_quantile(30) == 2.042 and ab.t_quantile(500) == 1.96


def _rows(
    effect_c=0.8, effect_e=0.9, jitter=(1.01, 0.99, 1.0, 1.02, 0.98, 1.0, 1.01, 0.99)
):
    cal = make_cal()
    rows = []
    for k, (block, cell) in enumerate(ab.flat_days(cal)):
        c, e = ab.want_flags(cell)
        y = (
            10000
            * (effect_c if c else 1)
            * (effect_e if e else 1)
            * (1.1**block)
            * jitter[k]
        )
        rows.append(
            {"day": k, "block": block, "c": int(c), "e": int(e), "y": y, "pp": y / 1000}
        )
    return rows


def test_fit_effects_recovers_the_planted_effects():
    fit = ab.fit_effects(_rows(), "y", log=True)
    assert fit["n"] == 8 and fit["df"] == 4
    assert fit["efeitos"]["C"]["est"] == pytest.approx(-0.2, abs=0.02)
    assert fit["efeitos"]["E"]["est"] == pytest.approx(-0.1, abs=0.02)
    assert (
        fit["efeitos"]["C"]["lo"]
        < fit["efeitos"]["C"]["est"]
        < fit["efeitos"]["C"]["hi"]
    )
    assert fit["interacao_exploratoria"]["est"] == pytest.approx(0.0, abs=0.05)


def test_fit_effects_in_pp_is_additive():
    fit = ab.fit_effects(_rows(1.0, 1.0, jitter=(1.0,) * 8), "pp", log=False)
    assert fit["efeitos"]["C"]["est"] == pytest.approx(0.0, abs=1e-9)


def test_ols_matches_numpy():
    np = pytest.importorskip("numpy")
    rows = _rows()
    x, _names = ab._design(rows, interaction=False)
    y = [row["y"] for row in rows]
    mine = ab.fit_ols(x, y)
    xa, ya = np.array(x), np.array(y)
    beta, *_ = np.linalg.lstsq(xa, ya, rcond=None)
    resid = ya - xa @ beta
    df = len(y) - xa.shape[1]
    se = np.sqrt(np.diag(np.linalg.inv(xa.T @ xa)) * (resid @ resid) / df)
    assert mine["df"] == df
    assert mine["beta"] == pytest.approx(list(beta), rel=1e-9)
    assert mine["se"] == pytest.approx(list(se), rel=1e-9)


def test_fit_effects_is_none_when_c_and_e_are_collinear():
    # Only the `--` and `CE` days remain: C and E move together, no contrast to fit.
    rows = [r for r in _rows() if r["c"] == r["e"]]
    assert len(rows) == 4
    assert ab.fit_effects(rows, "y", log=True) is None


def test_fit_effects_is_none_without_enough_days_for_the_model():
    assert ab.fit_effects(_rows()[:2], "y", log=True) is None  # 2 rows, 3 parameters


def test_fit_effects_skips_missing_outcomes():
    rows = _rows()
    rows[0]["pp"] = None
    assert ab.fit_effects(rows, "pp", log=False)["n"] == 7


GOOD = {"est": 0.0, "lo": -1.0, "hi": 1.0}


def primary(est, lo, hi, df=4):
    return {"est": est, "lo": lo, "hi": hi, "df": df}


def harms(**over):
    return {name: over.get(name, GOOD) for name in ab.HARM_METRICS["C"]}


@pytest.mark.parametrize(
    ("prim", "harm", "verdict"),
    [
        (primary(-0.20, -0.30, -0.10), harms(), "adotar"),
        (primary(-0.10, -0.15, -0.02), harms(), "adotar"),  # exactly -10%
        (primary(-0.0999, -0.15, -0.02), harms(), "inconclusivo"),  # just short of 10%
        (primary(-0.20, -0.30, 0.0), harms(), "inconclusivo"),  # CI touches 0
        (primary(-0.20, -0.30, 0.01), harms(), "inconclusivo"),  # CI crosses 0
        (
            primary(-0.20, -0.30, -0.10),
            harms(erro_pp={"est": 1, "lo": 0, "hi": 2.0}),
            "adotar",
        ),
        (
            primary(-0.20, -0.30, -0.10),
            harms(erro_pp={"est": 1, "lo": 0, "hi": 2.01}),
            "inconclusivo",
        ),
        (
            primary(-0.20, -0.30, -0.10),
            harms(retry_pp={"est": 3, "lo": 2.01, "hi": 4}),
            "nao-adotar",
        ),
        (
            primary(-0.20, -0.30, -0.10),
            harms(retry_pp={"est": 3, "lo": 2.0, "hi": 4}),
            "inconclusivo",
        ),
        (primary(-0.20, -0.30, -0.10), {**harms(), "wasted_pp": None}, "inconclusivo"),
        (primary(-0.03, -0.05, -0.01), harms(), "nao-adotar"),  # CI excludes a 10% cut
        (primary(0.05, 0.01, 0.09), harms(), "nao-adotar"),
        (primary(-0.20, -0.30, -0.10, df=1), harms(), "inconclusivo"),
        (None, harms(), "inconclusivo"),
        (primary(-0.20, -0.9, 0.5), harms(), "inconclusivo"),
    ],
)
def test_decision_rule(prim, harm, verdict):
    assert ab.decide_factor(prim, harm)["veredito"] == verdict


def test_decision_thresholds_are_the_pre_registered_ones():
    assert (ab.MIN_TURNS, ab.MARGIN_REL, ab.HARM_PP, ab.MIN_DF) == (20, -0.10, 2.0, 2)


# ------------------------------------------------------------- transcripts and analyze


def stamp_of(when):
    return when.isoformat().replace("+00:00", "Z")


def user_text(when, text, **extra):
    return {
        "type": "user",
        "timestamp": stamp_of(when),
        "message": {"content": text},
        **extra,
    }


def tool_use(when, tid, name="Read", inp=None):
    block = {
        "type": "tool_use",
        "id": tid,
        "name": name,
        "input": inp or {"file_path": f"/src/{tid}"},
    }
    return {
        "type": "assistant",
        "timestamp": stamp_of(when),
        "message": {"model": "m", "content": [block]},
    }


def tool_result(when, tid, text, is_error=False):
    block = {
        "type": "tool_result",
        "tool_use_id": tid,
        "content": text,
        "is_error": is_error,
    }
    return {
        "type": "user",
        "timestamp": stamp_of(when),
        "message": {"content": [block]},
    }


def hook(when, text):
    payload = json.dumps({"hookSpecificOutput": {"additionalContext": text}})
    att = {"type": "hook_success", "command": "h", "stdout": payload}
    return {"type": "attachment", "timestamp": stamp_of(when), "attachment": att}


def boundary(when, pre, post):
    meta = {"trigger": "auto", "preTokens": pre, "postTokens": post, "durationMs": 1000}
    return {
        "type": "system",
        "subtype": "compact_boundary",
        "timestamp": stamp_of(when),
        "compactMetadata": meta,
    }


def write_transcript(home, name, records):
    folder = home / ".claude" / "projects" / PROJECT
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / f"{name}.jsonl"
    path.write_text("\n".join(json.dumps(r) for r in records) + "\n", encoding="utf-8")
    end = ab.parse_ts(records[-1]["timestamp"]).timestamp()
    os.utime(path, (end, end))
    return path


def turn_records(start, n_turns, target_bytes, tag, *, inj=200, step=60):
    """``n_turns`` human turns, each with one hook injection and one tool result, so a
    turn weighs exactly ``target_bytes`` (``inj`` injected + the rest in the result)."""
    out = []
    for i in range(n_turns):
        t = start + dt.timedelta(seconds=step * i)
        tid = f"{tag}-{i}"
        out += [
            user_text(t, f"prompt {tag} {i}"),
            hook(t + dt.timedelta(seconds=1), "h" * inj),
            tool_use(t + dt.timedelta(seconds=2), tid),
            tool_result(t + dt.timedelta(seconds=3), tid, "x" * (target_bytes - inj)),
        ]
    return out


def test_fold_session_counts_turns_bytes_and_raw_reads(fake_home):
    t0 = ab.day_start_utc(make_cal(), 0) + dt.timedelta(hours=2)
    at = lambda s: t0 + dt.timedelta(seconds=s)
    records = [
        user_text(at(0), "faça a auditoria"),
        {**user_text(at(1), "sidechain line"), "isSidechain": True},
        {**user_text(at(2), "meta blob"), "isMeta": True},
        user_text(at(3), "<local-command-stdout>harness output</local-command-stdout>"),
        hook(at(4), "i" * 100),
        tool_use(
            at(5), "a", "Read", {"file_path": "/home/u/.claude/touring/evicted/abc.txt"}
        ),
        tool_result(at(6), "a", "e" * 60),
        tool_use(
            at(7),
            "b",
            "Bash",
            {"command": "cat /home/u/.claude/touring/compacted/9f.txt"},
        ),
        tool_result(at(8), "b", "c" * 70, is_error=True),
        tool_use(
            at(9), "c", "Edit", {"file_path": "/home/u/.claude/touring/evicted/zz"}
        ),
        tool_result(at(10), "c", "z" * 80),
        boundary(at(11), 600000, 30000),
        user_text(at(12), "continue"),
    ]
    sess = ab.fold_session(str(write_transcript(fake_home, "s", records)))
    assert (
        sess.human_turns == 2
    )  # sidechain, meta, harness output and tool results are not turns
    assert sess.bytes == 100 + 60 + 70 + 80
    assert (sess.calls, sess.erro) == (3, 1)
    assert (sess.raw_e, sess.raw_c) == (1, 1)  # Edit is not a reader
    assert sess.compactions == [
        {"trigger": "auto", "pre": 600000, "post": 30000, "ms": 1000}
    ]
    bare = fake_home / "nots.jsonl"
    bare.write_text(json.dumps({"type": "x"}) + "\n", encoding="utf-8")
    assert ab.fold_session(str(bare)) is None  # no timestamp anywhere: nothing to place


def arm_log(cal, records_by_day=None, *, binary_by_block=None, kpi_step=(1000, 20)):
    """A healthy log: one verified arm record per day plus the closing reset record."""
    out = []
    for k, (block, cell) in enumerate(ab.flat_days(cal)):
        tsha = (binary_by_block or {}).get(block, "a" * 64)
        r = rec(k, actions=["daemon:restart x1"], tsha=tsha, dsha="b" * 64)
        r["cell"] = cell
        r["kpi"] = {
            "runs_total": 1000 + kpi_step[0] * k,
            "runs_ok": 900,
            "wasted_attempts_retry_pairs": 10 + kpi_step[1] * k,
        }
        out.append(r)
    end = ab.day_start_utc(cal, ab.total_days(cal)) + dt.timedelta(seconds=30)
    out.append(
        rec(
            0,
            event="reset",
            stamp=end,
            kpi={
                "runs_total": 1000 + kpi_step[0] * 8,
                "runs_ok": 900,
                "wasted_attempts_retry_pairs": 10 + kpi_step[1] * 8,
            },
        )
    )
    return out


def build_dataset(
    env, home, *, turns_per_session=13, c_effect=0.8, e_effect=0.95, log=None
):
    """8 days x 2 sessions; bytes per turn = 10000 * c_effect^C * e_effect^E * 1.1^block."""
    cal = make_cal()
    compacted, evicted = env.tmp / "compacted", env.tmp / "evicted"
    compacted.mkdir(exist_ok=True)
    evicted.mkdir(exist_ok=True)
    for k, (block, cell) in enumerate(ab.flat_days(cal)):
        c, e = ab.want_flags(cell)
        target = round(
            10000 * (c_effect if c else 1) * (e_effect if e else 1) * (1.1**block)
        )
        for part, hours in enumerate((2, 5)):
            start = ab.day_start_utc(cal, k) + dt.timedelta(hours=hours)
            write_transcript(
                home,
                f"d{k}s{part}",
                turn_records(start, turns_per_session, target, f"d{k}s{part}"),
            )
        for flag, folder in ((c, compacted), (e, evicted)):
            if flag:
                f = folder / f"raw{k}"
                f.write_text("raw")
                stamp = (ab.day_start_utc(cal, k) + dt.timedelta(hours=12)).timestamp()
                os.utime(f, (stamp, stamp))
    Path(env.log).parent.mkdir(parents=True, exist_ok=True)
    records = log if log is not None else arm_log(cal)
    Path(env.log).write_text(
        "\n".join(json.dumps(r) for r in records) + "\n", encoding="utf-8"
    )
    return compacted, evicted


def analyze(env, compacted, evicted, capsys, as_of=None):
    as_of = as_of or (ab.day_start_utc(make_cal(), 8) + dt.timedelta(hours=1))
    args = [
        "--compacted-dir",
        str(compacted),
        "--evicted-dir",
        str(evicted),
        "--as-of",
        as_of.isoformat(),
        "--json",
    ]
    assert run(env, "analyze", *args, host=FakeHost()) == 0
    return json.loads(capsys.readouterr().out)


def test_analyze_recovers_the_planted_effects_end_to_end(env, fake_home, capsys):
    compacted, evicted = build_dataset(env, fake_home)
    rep = analyze(env, compacted, evicted, capsys)
    assert rep["dias_validos"] == 8 and rep["dias_excluidos"] == {}
    day0 = rep["dias"][0]
    assert day0["turnos"] == 26 and day0["bytes_por_turno"] == pytest.approx(8000)
    assert day0["wasted_pp"] == pytest.approx(2.0)
    fatores = rep["fatores"]
    assert (fatores["n"], fatores["df"]) == (8, 4)
    assert fatores["C"]["primario"]["est"] == pytest.approx(-0.2, abs=0.005)
    assert fatores["E"]["primario"]["est"] == pytest.approx(-0.05, abs=0.005)
    assert fatores["C"]["decisao"]["veredito"] == "adotar"
    assert (
        fatores["E"]["decisao"]["veredito"] == "nao-adotar"
    )  # -5% never reaches the 10% bar
    assert rep["ressalvas"] == []


def test_analyze_text_output_names_the_verdict(env, fake_home, capsys):
    compacted, evicted = build_dataset(env, fake_home)
    as_of = (ab.day_start_utc(make_cal(), 8) + dt.timedelta(hours=1)).isoformat()
    args = [
        "--compacted-dir",
        str(compacted),
        "--evicted-dir",
        str(evicted),
        "--as-of",
        as_of,
    ]
    assert run(env, "analyze", *args, host=FakeHost()) == 0
    out = capsys.readouterr().out
    assert "dias válidos 8/8" in out and "fator C" in out and "adotar" in out


def test_analyze_drops_a_block_that_saw_two_binaries(env, fake_home, capsys):
    cal = make_cal()
    log = arm_log(cal)
    log[2]["binary"]["daemon_sha"] = "c" * 64  # a release inside block 0
    compacted, evicted = build_dataset(env, fake_home, log=log)
    rep = analyze(env, compacted, evicted, capsys)
    assert rep["dias_validos"] == 4
    assert {int(k): v for k, v in rep["dias_excluidos"].items()} == {
        k: ["binario_misto"] for k in range(4)
    }
    assert (
        rep["fatores"]["C"]["decisao"]["veredito"] == "inconclusivo"
    )  # df fell below 2


def test_analyze_accepts_a_release_between_blocks(env, fake_home, capsys):
    cal = make_cal()
    compacted, evicted = build_dataset(
        env, fake_home, log=arm_log(cal, binary_by_block={0: "a" * 64, 1: "e" * 64})
    )
    assert analyze(env, compacted, evicted, capsys)["dias_validos"] == 8


def test_analyze_excludes_days_with_fewer_than_20_turns(env, fake_home, capsys):
    compacted, evicted = build_dataset(
        env, fake_home, turns_per_session=9
    )  # 18 turns a day
    rep = analyze(env, compacted, evicted, capsys)
    assert rep["dias_validos"] == 0
    assert (
        all(v == ["poucos_turnos"] for v in rep["dias_excluidos"].values())
        and len(rep["dias_excluidos"]) == 8
    )
    assert rep["fatores"]["C"]["decisao"]["veredito"] == "inconclusivo"


def test_analyze_keeps_a_day_with_exactly_20_turns(env, fake_home, capsys):
    compacted, evicted = build_dataset(env, fake_home, turns_per_session=10)
    assert analyze(env, compacted, evicted, capsys)["dias_validos"] == 8


def test_analyze_excludes_the_day_that_is_still_running(env, fake_home, capsys):
    compacted, evicted = build_dataset(env, fake_home)
    mid = ab.day_start_utc(make_cal(), 7) + dt.timedelta(hours=10)
    rep = analyze(env, compacted, evicted, capsys, as_of=mid)
    assert rep["dias_excluidos"] == {"7": ["em_curso"]}


def test_analyze_flags_c_contamination_on_an_off_day(env, fake_home, capsys):
    compacted, evicted = build_dataset(env, fake_home)
    cal = make_cal()
    off_day = next(k for k, (_b, c) in enumerate(ab.flat_days(cal)) if c == "--")
    stray = compacted / "stray"
    stray.write_text("raw")
    stamp = (ab.day_start_utc(cal, off_day) + dt.timedelta(hours=9)).timestamp()
    os.utime(stray, (stamp, stamp))
    rep = analyze(env, compacted, evicted, capsys)
    assert rep["dias_excluidos"] == {str(off_day): ["contaminado_c"]}


def test_analyze_reports_a_treatment_that_never_fired(env, fake_home, capsys):
    compacted, evicted = build_dataset(env, fake_home)
    for f in evicted.iterdir():
        f.unlink()
    rep = analyze(env, compacted, evicted, capsys)
    assert rep["dias_validos"] == 8  # intention to treat: the days stay
    assert sum("E ligado e nenhum arquivo evicted" in n for n in rep["ressalvas"]) == 4


def test_analyze_discards_sessions_a_flag_change_can_reach(env, fake_home, capsys):
    cal = make_cal()
    compacted, evicted = build_dataset(env, fake_home)
    day3 = ab.day_start_utc(cal, 3)
    write_transcript(
        fake_home, "cross", turn_records(day3 - dt.timedelta(minutes=5), 10, 9000, "x")
    )
    write_transcript(
        fake_home, "early", turn_records(day3 + dt.timedelta(seconds=5), 5, 9000, "e")
    )
    write_transcript(
        fake_home,
        "before",
        turn_records(ab.day_start_utc(cal, 0) - dt.timedelta(minutes=2), 5, 9000, "b"),
    )
    rep = analyze(env, compacted, evicted, capsys)
    assert rep["sessoes"]["descartadas"] == {
        "cruza_a_virada": 1,
        "aberta_antes_do_arm": 1,
        "fora_da_sequencia": 1,
    }
    assert (
        rep["dias_validos"] == 8 and rep["dias"][3]["turnos"] == 26
    )  # none of them leaked in


def test_analyze_counts_malformed_log_lines(env, fake_home, capsys):
    compacted, evicted = build_dataset(env, fake_home)
    with open(env.log, "a", encoding="utf-8") as fh:
        fh.write("garbage\n")
    rep = analyze(env, compacted, evicted, capsys)
    assert rep["log"]["linhas_invalidas"] == 1 and any(
        "inválidas" in n for n in rep["ressalvas"]
    )


def test_analyze_needs_a_day0(env, capsys):
    Path(env.cal).write_text(json.dumps(make_cal(day0=None)), encoding="utf-8")
    assert run(env, "analyze", host=FakeHost()) == 2
    assert "day0" in capsys.readouterr().err


def test_analyze_without_a_log_excludes_every_day(env, fake_home, capsys):
    compacted, evicted = build_dataset(env, fake_home)
    os.unlink(env.log)
    rep = analyze(env, compacted, evicted, capsys)
    assert rep["dias_validos"] == 0
    assert all("sem_registro" in v for v in rep["dias_excluidos"].values())


# ---------------------------------------------------------------- bundle invariants


def test_systemd_units_match_the_script():
    units = {
        name: (HERE / "systemd" / name).read_text(encoding="utf-8")
        for name in (
            "ab34-arm.service",
            "ab34-arm.timer",
            "ab34-reset.service",
            "ab34-reset.timer",
        )
    }
    assert "ab34_arm.py arm --yes" in units["ab34-arm.service"]
    assert "ab34_arm.py reset --yes" in units["ab34-reset.service"]
    for name in ("ab34-arm.service", "ab34-reset.service"):
        assert "KillMode=process" in units[name] and "Type=oneshot" in units[name]
        assert (
            "SuccessExitStatus=4" in units[name]
        )  # "not applicable now" is not a failure
        assert '"+%%F' in units[name] and not re.search(r"(?<!%)%(?![%h])", units[name])
        script = re.search(r"/(docs/plans/[^ ]+/ab34_arm\.py)", units[name]).group(1)
        assert (REPO / script).is_file()
    for name in ("ab34-arm.timer", "ab34-reset.timer"):
        assert "Persistent=true" in units[name] and "OnCalendar=" in units[name]
        assert f"Unit={name.replace('.timer', '.service')}" in units[name]


def test_preregistration_states_the_numbers_the_code_uses():
    doc = (HERE / "AB34-preregistro.md").read_text(encoding="utf-8")
    cal = json.loads((HERE / "ab34-calendario.json").read_text(encoding="utf-8"))
    assert str(cal["seed"]) in doc
    assert f"{ab.MIN_TURNS} turnos humanos" in doc
    assert "10%" in doc and "2 pp" in doc and f"{ab.MIN_DF}" in doc
    for flag in (ab.FLAG_C, ab.FLAG_E):
        assert flag in doc
    for cell in ab.CELLS:
        assert f"`{cell}`" in doc


def test_marketplace_manifest_lists_the_mod():
    manifest = json.loads(
        (HERE / "mod-install" / "marketplace.json").read_text(encoding="utf-8")
    )
    plugin = json.loads(
        (
            REPO / "tools/mods/touring-compact-evict/.claude-plugin/plugin.json"
        ).read_text()
    )
    (entry,) = manifest["plugins"]
    assert entry["name"] == plugin["name"] and entry["source"] == f"./{plugin['name']}"
    assert manifest["name"] == "ab34-local" and "owner" in manifest
