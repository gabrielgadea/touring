"""D1 (analise-92, 06/10/2026): an absent `status`/`doctor` is never shown as 0.00 or 0/0.

Measured: `touring status -j` took 4.6-4.8 s on projects/analise against a 1.5 s per-call timeout, so the
opening panel of every session read "composite 0.00" and recommended `touring e2e` — a recommendation derived
from absence (REGRA ZERO: absence is not zero). The fix carries the REASON of an absence to the panel.
"""
from __future__ import annotations

import importlib.util
import json
import os
import stat
import subprocess
import sys
from pathlib import Path

HOOK = Path(__file__).resolve().parents[1] / "session_startup_intelligence.py"


def _load():
    spec = importlib.util.spec_from_file_location("ssi_under_test", HOOK)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def test_absent_status_and_doctor_read_as_unavailable_with_the_reason():
    m = _load()
    g = {"status": None, "doctor": None, "_why": {"status": "timeout 1.5s", "doctor": "rc=1"}}
    head = "\n".join(m._health_headline(g))
    assert "0.00" not in head and "0/0" not in head, head
    assert "indisponível" in head and "timeout 1.5s" in head and "rc=1" in head, head


def test_absence_derives_no_recommendation():
    m = _load()
    recs = m._health_recs({"status": None, "_why": {"status": "timeout 1.5s"}})
    assert not any("composite" in r for r in recs), recs


def test_present_values_are_still_shown():
    m = _load()
    g = {"status": {"composite_health_score": 0.81},
         "doctor": [{"name": "a", "status": "ok"}, {"name": "b", "status": "ok"}]}
    head = "\n".join(m._health_headline(g))
    assert "0.81" in head and "2/2" in head, head
    assert m._health_recs(g) == [] or not any("composite" in r for r in m._health_recs(g))


def test_a_real_low_composite_still_recommends():
    m = _load()
    recs = m._health_recs({"status": {"composite_health_score": 0.5}})
    assert any("0.50" in r for r in recs), recs


def test_run_touring_detail_names_timeout_and_exit_code(tmp_path):
    m = _load()
    fake = tmp_path / "bin"
    fake.mkdir()
    exe = fake / "touring"
    exe.write_text("#!/bin/sh\ncase \"$1\" in slow) exec sleep 5;; bad) exit 3;; esac\necho '{}'\n", encoding="utf-8")
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)
    old = os.environ["PATH"]
    os.environ["PATH"] = f"{fake}:{old}"
    try:
        assert m.run_touring_detail(["slow"], timeout=0.3) == (None, "timeout 0.3s")
        assert m.run_touring_detail(["bad"], timeout=2) == (None, "rc=3")
        assert m.run_touring_detail(["ok"], timeout=2) == ("{}", None)
    finally:
        os.environ["PATH"] = old


def test_the_panel_of_a_session_whose_touring_is_slow_never_reads_zero(tmp_path):
    """End to end, the RED case analise-92 asked for: a fake `touring` that sleeps past the timeout."""
    fake = tmp_path / "bin"
    fake.mkdir()
    exe = fake / "touring"
    exe.write_text("#!/bin/sh\nexec sleep 6\n", encoding="utf-8")  # exec: no grandchild holding the pipe
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)
    env = {**os.environ, "PATH": f"{fake}:{os.environ['PATH']}", "TOURING_SESSIONSTART_SLIM": "1"}
    p = subprocess.run([sys.executable, str(HOOK)], input=json.dumps({"cwd": str(tmp_path), "session_id": "t"}),
                       capture_output=True, text=True, env=env, timeout=30, check=False)
    assert p.returncode == 0, p.stderr
    ctx = json.loads(p.stdout)["hookSpecificOutput"]["additionalContext"]
    assert "composite **0.00**" not in ctx and "0/0" not in ctx, ctx[:600]
    assert "indisponível" in ctx, ctx[:600]
    assert "< 0.7" not in ctx, ctx[:600]
