"""S-12 — delivery proofs S1 (hot path), S2 (fail-open), S4 (byte-identical) are MEASURED, not promised."""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_prove_delivery as pd  # noqa: E402


def trace(path: Path, rows):
    path.write_text("".join(json.dumps(r) + "\n" for r in rows))


def row(ts, ms, hook="cli-suggest", reason="daemon-json"):
    return {"ts_ms": ts, "hook": hook, "elapsed_ms": ms, "exit_reason": reason}


def test_p95_is_nearest_rank_and_ignores_other_hooks(tmp_path):
    rows = [row(i, i) for i in range(1, 101)] + [row(i, 9999, hook="pre-bash") for i in range(1, 50)]
    trace(tmp_path / "t.jsonl", rows)
    w = pd.hook_window(tmp_path / "t.jsonl", "cli-suggest", 0, 10**12)
    assert w["n"] == 100 and w["p95"] == 95 and w["p50"] == 50


def test_s1_threshold_uses_the_prior_window_and_the_larger_of_10pct_or_20ms():
    # ratio rule dominates for a big baseline: 400 ms ⇒ limit 440
    assert pd.s1_verdict({"p95": 400, "n": 500}, {"p95": 440, "n": 500})["passes"] is True
    assert pd.s1_verdict({"p95": 400, "n": 500}, {"p95": 441, "n": 500})["passes"] is False
    # the +20 ms rule dominates for a small baseline: 3 ms ⇒ limit 23 (10% would be 3.3)
    assert pd.s1_verdict({"p95": 3, "n": 500}, {"p95": 23, "n": 500})["passes"] is True
    assert pd.s1_verdict({"p95": 3, "n": 500}, {"p95": 24, "n": 500})["passes"] is False


def test_s1_refuses_to_conclude_on_too_few_samples():
    out = pd.s1_verdict({"p95": 10, "n": 500}, {"p95": 10, "n": 20}, min_n=200)
    assert out["passes"] is None and "samples" in out["reason"]   # "no signal" is NOT a pass


def test_windows_are_split_at_the_shadow_on_timestamp(tmp_path):
    rows = [row(100, 5), row(200, 5), row(1000, 50), row(1100, 50)]
    trace(tmp_path / "t.jsonl", rows)
    before, after = pd.split_windows(tmp_path / "t.jsonl", "cli-suggest", on_ts=500, lookback_ms=10_000)
    assert before["n"] == 2 and after["n"] == 2 and before["p95"] == 5 and after["p95"] == 50


def test_s4_detects_a_one_byte_difference(tmp_path):
    prog = tmp_path / "hook.py"
    prog.write_text("import os,sys\nsys.stdout.write('ctx:A' + ('!' if os.environ.get('TOURING_R3_SHADOW')=='1' else ''))\n")
    inputs = [{"i": n} for n in range(3)]
    out = pd.s4_byte_identical([sys.executable, str(prog)], inputs)
    assert out["identical"] is False and out["n_diff"] == 3 and out["first_diff_input"] == 0


def test_s4_passes_when_the_flag_changes_nothing(tmp_path):
    prog = tmp_path / "hook.py"
    prog.write_text("import sys,json\nsys.stdout.write(json.dumps({'additionalContext': json.load(sys.stdin)['i']}))\n")
    out = pd.s4_byte_identical([sys.executable, str(prog)], [{"i": "a"}, {"i": "b"}])
    assert out["identical"] is True and out["n"] == 2 and out["n_diff"] == 0


def test_s4_runs_each_input_with_the_flag_on_and_off_in_standalone_mode(tmp_path):
    """The emitter lives in the daemon, so the proof MUST run the hook standalone (TOURING_NO_DAEMON=1)
    where the per-command env is honoured; otherwise ON and OFF would both hit the same daemon."""
    prog = tmp_path / "echoenv.py"
    prog.write_text("import os,sys\nsys.stdout.write(os.environ.get('TOURING_NO_DAEMON','') + os.environ.get('TOURING_R3_SHADOW','x'))\n")
    seen = pd.run_hook([sys.executable, str(prog)], {}, shadow="1")
    assert seen == b"11" and pd.run_hook([sys.executable, str(prog)], {}, shadow="0") == b"10"


def test_s2_exit_reason_distribution_is_compared_not_just_counted(tmp_path):
    ok = [row(i, 3, reason="daemon-json") for i in range(1000)]
    trace(tmp_path / "base.jsonl", ok)
    same = ok + [row(2000 + i, 3, reason="daemon-json") for i in range(100)]
    trace(tmp_path / "same.jsonl", same)
    worse = ok + [row(2000 + i, 3, reason="error") for i in range(100)]
    trace(tmp_path / "worse.jsonl", worse)
    base = pd.exit_reasons(tmp_path / "base.jsonl", "cli-suggest", 0, 1999)
    assert pd.s2_verdict(base, pd.exit_reasons(tmp_path / "same.jsonl", "cli-suggest", 2000, 10**12))["passes"] is True
    assert pd.s2_verdict(base, pd.exit_reasons(tmp_path / "worse.jsonl", "cli-suggest", 2000, 10**12))["passes"] is False
