import datetime as dt
import json
import pathlib
import sys

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import r3_latency_by_day as lat  # noqa: E402
import r3_prove_delivery as pd  # noqa: E402

DAY = 86_400_000
D0 = 1_790_900_000_000 - (1_790_900_000_000 % DAY)  # 00:00 UTC de um dia qualquer
ON = D0 + 2 * DAY


def rows(hook, day_offset, ms_values, base_ms=3_600_000):
    return [{"hook": hook, "ts_ms": D0 + day_offset * DAY + base_ms + i, "elapsed_ms": v} for i, v in enumerate(ms_values)]


@pytest.mark.parametrize("n", [1, 2, 19, 20, 21, 99, 100, 101, 3862])
def test_p95_is_exactly_the_percentile_of_the_criterion_it_explains(n):
    values = [float((i * 7919) % 1000) for i in range(n)]
    assert lat.p95(values) == pd._pctl(sorted(values), 95)


def test_p95_of_nothing_is_none_not_zero():
    assert lat.p95([]) is None


def test_contract_with_daily_p95s_of_the_real_instrument(tmp_path):
    values = [float((i * 31) % 500) for i in range(60)]
    data = rows("cli-suggest", 0, values) + rows("cli-suggest", 1, [v + 100 for v in values])
    trace = tmp_path / "t.jsonl"
    trace.write_text("\n".join(json.dumps(r) for r in data) + "\n")
    theirs = pd.daily_p95s(trace, "cli-suggest", D0, D0 + 2 * DAY, min_n=30)
    cells = lat.breakdown(data, ON + 10 * DAY)
    assert sorted(c["p95"] for c in cells) == theirs
    day0 = [r["elapsed_ms"] for r in data if r["ts_ms"] < D0 + DAY]
    assert cells[0]["p50"] == pd._pctl(sorted(day0), 50) and len(day0) % 2 == 0  # n par: o corte da mediana importa


def test_phase_flips_at_exactly_the_on_instant():
    data = [{"hook": "cli-suggest", "ts_ms": ON - 1 - i, "elapsed_ms": 1.0} for i in range(30)]
    data += [{"hook": "cli-suggest", "ts_ms": ON + i, "elapsed_ms": 2.0} for i in range(30)]
    cells = {c["phase"]: c for c in lat.breakdown(data, ON)}
    assert cells["off"]["p95"] == 1.0 and cells["ON"]["p95"] == 2.0 and cells["ON"]["n"] == 30


def test_small_cells_and_foreign_hooks_are_dropped():
    data = rows("cli-suggest", 0, [1.0] * 29) + rows("pre-bash", 0, [1.0] * 100)
    assert lat.breakdown(data, ON) == []
    assert lat.breakdown(rows("cli-suggest", 0, [1.0] * 30), ON, min_n=31) == []


def test_rows_without_a_numeric_latency_or_timestamp_are_ignored():
    good = rows("cli-suggest", 0, [5.0] * 30)
    bad = [{"hook": "cli-suggest", "ts_ms": D0 + 1, "elapsed_ms": None}, {"hook": "cli-suggest", "ts_ms": D0 + 1, "elapsed_ms": True},
           {"hook": "cli-suggest", "ts_ms": "x", "elapsed_ms": 9.0}, {"hook": "cli-suggest", "elapsed_ms": 9.0}]
    cells = lat.breakdown(good + bad, ON)
    assert len(cells) == 1 and cells[0]["n"] == 30 and cells[0]["p95"] == 5.0


def test_the_median_stays_flat_while_only_the_tail_moves():
    calm = [3.0] * 90 + [600.0] * 10  # cauda de 10 %: o p95 por posto so a enxerga com mais de 5 % de amostras altas
    loaded = [3.0] * 90 + [900.0] * 10
    data = rows("cli-suggest", 0, calm) + rows("cli-suggest", 1, loaded)
    by_day = [c for c in lat.breakdown(data, ON)]
    assert [c["p50"] for c in by_day] == [3.0, 3.0] and by_day[0]["p95"] < by_day[1]["p95"]


def test_brt_cuts_the_day_three_hours_later_than_utc():
    late_utc = D0 + 2 * 3_600_000  # 02:00 UTC = 23:00 BRT do dia anterior
    data = [{"hook": "cli-suggest", "ts_ms": late_utc + i, "elapsed_ms": 1.0} for i in range(30)]
    utc_day = lat.breakdown(data, ON, tz=lat.TZS["utc"])[0]["day"]
    brt_day = lat.breakdown(data, ON, tz=lat.TZS["brt"])[0]["day"]
    assert utc_day != brt_day


def test_main_exits_2_without_a_cell_with_enough_samples_and_0_otherwise(tmp_path, capsys):
    trace = tmp_path / "t.jsonl"
    trace.write_text("lixo\n" + json.dumps(rows("cli-suggest", 0, [1.0] * 5)[0]) + "\n")
    assert lat.main(["--trace", str(trace), "--on-ts", str(ON)]) == 2
    trace.write_text("\n".join(json.dumps(r) for r in rows("cli-suggest", 0, [1.0] * 40)) + "\n")
    assert lat.main(["--trace", str(trace), "--on-ts", str(ON), "--tz", "brt"]) == 0
    assert json.loads(capsys.readouterr().out.splitlines()[-1])[0]["n"] == 40
    # o default segue o corte do criterio (UTC): 01:00 UTC e o mesmo dia em UTC e outro em BRT (22:00 do dia anterior)
    first_day_utc = dt.datetime.fromtimestamp((D0 + 3_600_000) / 1000, dt.timezone.utc).strftime("%d/%m")
    assert lat.main(["--trace", str(trace), "--on-ts", str(ON)]) == 0
    assert json.loads(capsys.readouterr().out.splitlines()[-1])[0]["day"] == first_day_utc
