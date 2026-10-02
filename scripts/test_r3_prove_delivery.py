"""S-12 — delivery proofs S1 (hot path), S2 (fail-open), S4 (byte-identical) are MEASURED, not promised."""
import json
import os
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


def test_s4_state_is_isolated_fresh_per_run_and_never_the_real_one(tmp_path):
    """Standalone runs must not pollute the live hook trace (it feeds S1b) nor the real journals/counters, and each run
    starts from a FRESH scratch project (the suggester's 300 s dedup cache would otherwise differ ON vs OFF by itself)."""
    prog = tmp_path / "env.py"
    prog.write_text("import os,sys,json\nsys.stdout.write(json.dumps({k: os.environ.get(k) for k in "
                    "('TOURING_HOOK_TRACE_FILE','TOURING_PROJECT_ROOT','TOURING_R3_EVENTS_FILE','TOURING_DAEMON_SOCKET')}))\n")
    real = {"TOURING_HOOK_TRACE_FILE": "/real/trace.jsonl", "TOURING_DAEMON_SOCKET": "/real/d.sock", "TOURING_PROJECT_ROOT": "/real/proj"}
    saved = {k: os.environ.get(k) for k in real}
    os.environ.update(real)
    try:
        seen = [json.loads(pd.run_hook([sys.executable, str(prog)], {}, "1", state_dir=tmp_path / f"s{n}")) for n in range(2)]
    finally:
        for k, v in saved.items():
            os.environ.pop(k, None) if v is None else os.environ.__setitem__(k, v)
    for n, e in enumerate(seen):
        assert e["TOURING_HOOK_TRACE_FILE"] is None and e["TOURING_DAEMON_SOCKET"] is None      # real trace/daemon unreachable
        assert e["TOURING_PROJECT_ROOT"] == str(tmp_path / f"s{n}") and e["TOURING_R3_EVENTS_FILE"].startswith(str(tmp_path / f"s{n}"))


def test_s4_every_run_gets_a_fresh_scratch_project_and_the_payload_cwd_points_at_it(tmp_path):
    """State accumulating across runs would make the 2nd run differ from the 1st (the suggester's dedup cache is exactly
    this), and a payload `cwd` still naming the REAL project would let the hook resolve the real one."""
    marker = tmp_path / "cwd_was_not_redirected"
    prog = tmp_path / "stateful.py"
    prog.write_text(
        "import os, sys, json\nfrom pathlib import Path\n"
        "root = Path(os.environ['TOURING_PROJECT_ROOT']); payload = json.load(sys.stdin)\n"
        f"if payload['cwd'] != str(root): Path({str(marker)!r}).write_text('bad')\n"
        "seen = root / 'seen'; n = len(seen.read_text().splitlines()) if seen.exists() else 0\n"
        "seen.open('a').write('x\\n'); sys.stdout.write(str(n))\n")
    out = pd.s4_byte_identical([sys.executable, str(prog)], [{"cwd": "/real/project", "i": 1}, {"cwd": "/real/project", "i": 2}])
    assert out["passes"] is True and out["control_identical"] is True      # a shared dir would make run 2 print 1, not 0
    assert not marker.exists()                                              # the payload cwd named the scratch project


def _event(**over):
    base = {"v": 1, "kind": "a1_block", "use_case": "a1_relevance", "ts_ms": 1790000000000, "session_hash": "0123456789ab",
            "hook": "pre-bash", "bytes": 120, "first_line_sha": "0123456789abcdef", "first_line_head": "[TOURING] a short head"}
    base.update(over)
    return base


def test_s6_passes_only_when_every_line_is_valid_and_has_no_long_free_text(tmp_path):
    f = tmp_path / "e.jsonl"
    f.write_text("".join(json.dumps(_event(ts_ms=1790000000000 + i)) + "\n" for i in range(3)))
    out = pd.s6_privacy(f)
    assert out["passes"] is True and out["n"] == 3 and out["n_invalid"] == 0 and out["n_privacy"] == 0


def test_s6_fails_on_free_text_over_80_chars_an_unknown_key_and_a_garbage_line(tmp_path):
    f = tmp_path / "bad.jsonl"
    f.write_text("\n".join([json.dumps(_event()), json.dumps(_event(first_line_head="x" * 81)),
                            json.dumps({**_event(), "prompt": "a free text key"}), "not json at all"]) + "\n")
    out = pd.s6_privacy(f)
    assert out["passes"] is False and out["n"] == 4
    # each net must count ON ITS OWN (the three bad lines are caught by BOTH, so a sum would hide a net that is off)
    assert out["n_invalid"] >= 3 and out["n_privacy"] >= 3   # long text, forbidden key, garbage line
    assert "x" * 20 not in json.dumps(out)       # the report names the violation, never echoes the offending text


def test_s6_a_line_only_the_closed_schema_catches_still_fails(tmp_path):
    """A missing required field carries no free text, so the privacy scan is silent: only the schema net can fail it."""
    ev = _event()
    del ev["first_line_sha"]
    f = tmp_path / "schema_only.jsonl"
    f.write_text(json.dumps(ev) + "\n")
    out = pd.s6_privacy(f)
    assert out["passes"] is False and out["n_invalid"] == 1 and out["n_privacy"] == 0


def test_s6_no_events_is_no_signal_never_a_pass(tmp_path):
    f = tmp_path / "empty.jsonl"
    f.write_text("")
    assert pd.s6_privacy(f)["passes"] is None
    assert pd.s6_privacy(tmp_path / "missing.jsonl")["passes"] is None


def test_s4_a_proof_where_every_output_is_empty_is_void_not_a_pass(tmp_path):
    """Measured on the real hooks: in a clean scratch project `cli-suggest` and `pre-bash` answer 0 bytes for ordinary
    commands, so 'ON == OFF' would be 0 bytes == 0 bytes for all 200 inputs — a green that proves nothing."""
    silent = tmp_path / "silent.py"
    silent.write_text("import sys\nsys.stdout.write('')\n")
    out = pd.s4_byte_identical([sys.executable, str(silent)], [{"i": 1}, {"i": 2}])
    assert out["passes"] is None and out["n_nonempty"] == 0 and "empty" in out["reason"]
    assert out["identical"] is True       # the raw comparison is reported, the VERDICT is what refuses to conclude


def test_s4_counts_the_non_empty_outputs_so_the_strength_of_the_proof_is_visible(tmp_path):
    prog = tmp_path / "mixed.py"
    prog.write_text("import sys, json\np = json.load(sys.stdin)\nsys.stdout.write('ctx' if p['i'] % 2 else '')\n")
    out = pd.s4_byte_identical([sys.executable, str(prog)], [{"i": n} for n in range(6)])
    assert out["passes"] is True and out["n"] == 6 and out["n_nonempty"] == 3


def test_s4_cli_exit_codes_0_pass_1_diff_2_void(tmp_path, capsys):
    """The CLI maps the verdict to an exit code: a VOID proof (nondeterministic control) is 2, never a pass."""
    inputs = tmp_path / "in.jsonl"
    inputs.write_text(json.dumps({"i": 1}) + "\n")
    progs = {"same": "import sys\nsys.stdout.write('x')\n",
             "flag_changes_output": "import os,sys\nsys.stdout.write('x' + os.environ['TOURING_R3_SHADOW'])\n",
             "nondeterministic": "import sys,time\nsys.stdout.write(str(time.time_ns()))\n"}
    expected = {"same": 0, "flag_changes_output": 1, "nondeterministic": 2}
    for name, src in progs.items():
        (tmp_path / f"{name}.py").write_text(src)
        rc = pd.main(["s4", "--argv-json", json.dumps([sys.executable, str(tmp_path / f"{name}.py")]), "--inputs", str(inputs)])
        capsys.readouterr()
        assert rc == expected[name], name


def test_s4_has_a_control_and_an_invalid_control_voids_the_verdict(tmp_path):
    """OFF x OFF must be identical for the harness to mean anything; a nondeterministic hook makes the proof VOID
    (passes=None), never a pass and never a blame on the flag."""
    prog = tmp_path / "rand.py"
    prog.write_text("import os,sys,time\nsys.stdout.write(str(time.time_ns()))\n")
    out = pd.s4_byte_identical([sys.executable, str(prog)], [{"i": 1}, {"i": 2}])
    assert out["control_identical"] is False and out["passes"] is None and "control" in out["reason"]
    stable = tmp_path / "stable.py"
    stable.write_text("import sys\nsys.stdout.write('same')\n")
    ok = pd.s4_byte_identical([sys.executable, str(stable)], [{"i": 1}])
    assert ok["control_identical"] is True and ok["passes"] is True


def test_s4_runs_each_input_with_the_flag_on_and_off_in_standalone_mode(tmp_path):
    """The emitter lives in the daemon, so the proof MUST run the hook standalone (TOURING_NO_DAEMON=1)
    where the per-command env is honoured; otherwise ON and OFF would both hit the same daemon."""
    prog = tmp_path / "echoenv.py"
    prog.write_text("import os,sys\nsys.stdout.write(os.environ.get('TOURING_NO_DAEMON','') + os.environ.get('TOURING_R3_SHADOW','x'))\n")
    seen = pd.run_hook([sys.executable, str(prog)], {}, shadow="1")
    assert seen == b"11" and pd.run_hook([sys.executable, str(prog)], {}, shadow="0") == b"10"


DAY = 86_400_000


def test_daily_p95s_groups_by_day_and_drops_thin_days(tmp_path):
    """Errata 5 (E5.2): the natural envelope is the set of PER-DAY p95s; a day with too few rows says nothing."""
    rows = [row(d * DAY + i, 10 * (d + 1) + i % 5) for d in range(3) for i in range(300)] + [row(7 * DAY + i, 9999) for i in range(40)]
    trace(tmp_path / "t.jsonl", rows)
    p95s = pd.daily_p95s(tmp_path / "t.jsonl", "cli-suggest", 0, 10 * DAY, min_n=200)
    assert len(p95s) == 3 and p95s == sorted(p95s) and p95s[0] < p95s[1] < p95s[2]   # day 7 (n=40) is dropped


def test_s1b_passes_inside_the_natural_envelope_and_fails_above_it():
    prior = [122.0, 596.0, 300.0]   # the real shape: one day can be 5x another with no shadow at all
    assert pd.s1b_verdict(prior, {"p95": 596.0, "n": 500})["passes"] is True
    assert pd.s1b_verdict(prior, {"p95": 596.5, "n": 500})["passes"] is False
    assert pd.s1b_verdict(prior, {"p95": 130.0, "n": 500})["passes"] is True


def test_s1b_refuses_to_conclude_without_samples_or_prior_days():
    assert pd.s1b_verdict([100.0], {"p95": 50.0, "n": 20})["passes"] is None        # too few ON samples
    assert pd.s1b_verdict([], {"p95": 50.0, "n": 500})["passes"] is None            # no baseline day at all


def test_the_default_hooks_are_both_places_the_a2_observer_runs():
    assert set(pd.HOOKS) == {"cli-suggest", "post-tool-rl"}


def test_cli_checks_every_hook_the_worst_verdict_decides_and_s1_is_informational(tmp_path, capsys):
    on = 20 * DAY
    base = [row(d * DAY + i, 50, hook=h) for h in pd.HOOKS for d in range(5) for i in range(300)]
    ok_after = [row(on + i, 50, hook="cli-suggest") for i in range(300)]
    slow_after = [row(on + i, 5000, hook="post-tool-rl") for i in range(300)]
    trace(tmp_path / "t.jsonl", base + ok_after + slow_after)
    args = ["--trace", str(tmp_path / "t.jsonl"), "--on-ts", str(on), "--lookback-days", "20"]
    assert pd.main(["s1b", *args]) == 1                    # post-tool-rl blew past the envelope: the worst decides
    verdicts = json.loads(capsys.readouterr().out)
    assert verdicts["cli-suggest"]["passes"] is True and verdicts["post-tool-rl"]["passes"] is False
    assert pd.main(["s1", *args]) == 0                     # the original S1 only REPORTS now (Errata 5)
    assert all(v["informational"] for v in json.loads(capsys.readouterr().out).values())


def test_no_signal_is_exit_2_never_a_pass(tmp_path, capsys):
    """'No signal is not a pass': ON-window rows missing for a hook ⇒ exit 2, for S1b and for S2 alike."""
    on = 20 * DAY
    trace(tmp_path / "t.jsonl", [row(d * DAY + i, 50, hook=h) for h in pd.HOOKS for d in range(5) for i in range(300)])  # nothing ON
    args = ["--trace", str(tmp_path / "t.jsonl"), "--on-ts", str(on), "--lookback-days", "20"]
    for cmd in ("s1b", "s2"):
        assert pd.main([cmd, *args]) == 2
        out = json.loads(capsys.readouterr().out)           # ONE json per call, one entry per hook
        assert set(out) == set(pd.HOOKS) and all(v["passes"] is None for v in out.values()), cmd


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
