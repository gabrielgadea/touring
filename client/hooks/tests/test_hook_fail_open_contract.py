"""Fail-open, but never mute: the contract the ruff sweep of 17/10/2026 checked hook by hook.

A hook never takes the session down (exit 0, valid output), and a failure it swallows still says so on
stderr — the house convention (`rate_limits_snapshot`: "todo erro sai em stderr com exit 0"). The silent
`except Exception: pass` is the one that hid a real defect on 01/10: the recall journal never got written
and nothing said why (comment in `_journal_recall_injection`).
"""
from __future__ import annotations

import importlib.util
import json
import os
import stat
import subprocess
import sys
from pathlib import Path

import pytest

HOOKS = Path(__file__).resolve().parents[1]
SSI = HOOKS / "session_startup_intelligence.py"


def _load_ssi():
    spec = importlib.util.spec_from_file_location("ssi_fail_open_under_test", SSI)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _fast_failing_touring(tmp_path: Path) -> dict[str, str]:
    """PATH with a `touring` that answers at once with rc=3: every source absent, no 4.5 s wait."""
    fake = tmp_path / "bin"
    fake.mkdir()
    exe = fake / "touring"
    exe.write_text("#!/bin/sh\nexit 3\n", encoding="utf-8")
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)
    return {**os.environ, "PATH": f"{fake}:{os.environ['PATH']}", "TOURING_SESSIONSTART_SLIM": "1",
            "TOURING_RECALL_JOURNAL": str(tmp_path / "recall_journal.jsonl")}


@pytest.mark.parametrize("payload", ["[]", '"texto"', "42", "nao-e-json", ""])
def test_a_payload_that_is_not_an_object_still_renders_the_panel(tmp_path: Path, payload: str) -> None:
    """`data.get` on a list/str/int used to raise in main(); the last-line handler then replaced the WHOLE
    panel with the degraded note. A payload we cannot read is an absent payload, not a crash."""
    p = subprocess.run([sys.executable, str(SSI)], input=payload, capture_output=True, text=True,
                       env=_fast_failing_touring(tmp_path), timeout=30, check=False)
    assert p.returncode == 0, p.stderr
    ctx = json.loads(p.stdout)["hookSpecificOutput"]["additionalContext"]
    assert "degraded fail-open" not in ctx, ctx[:300]
    assert "TOURING" in ctx, ctx[:300]


def test_a_recall_journal_that_cannot_be_written_says_so_on_stderr(tmp_path, monkeypatch, capsys) -> None:
    m = _load_ssi()
    blocker = tmp_path / "um-arquivo"
    blocker.write_text("", encoding="utf-8")
    monkeypatch.setenv("TOURING_RECALL_JOURNAL", str(blocker / "recall_journal.jsonl"))  # parent is a file
    results = {"memory_x": {"cases": {"g": [{"key": "k", "value": "conteúdo"}]}}}
    m._journal_recall_injection(results, "contexto", str(tmp_path))  # must not raise
    err = capsys.readouterr().err
    assert "recall_journal" in err and "Error" in err, err


def test_parse_json_safe_still_answers_none_on_every_bad_input() -> None:
    """Narrowing `except Exception` must keep every failure json.loads can raise."""
    m = _load_ssi()
    assert m.parse_json_safe("{ruim") is None
    assert m.parse_json_safe("[" * 200_000) is None  # RecursionError, not ValueError
    assert m.parse_json_safe('{"a": 1}') == {"a": 1}


@pytest.mark.parametrize("ts", ["x", float("-inf"), -(10**400)])  # TypeError · ValueError (int(nan)) · OverflowError
def test_fmt_age_degrades_to_question_mark(ts) -> None:
    assert _load_ssi()._fmt_age(ts) == "?"


PROBE = 'def main():\n    raise RuntimeError("sonda-de-ultima-linha")\n\n\n'


@pytest.mark.parametrize("hook", ["code_mode_sdk_section.py", "skill_picker_hook.py", "session_startup_intelligence.py"])
def test_the_last_line_handler_fails_open_and_names_the_error(tmp_path: Path, hook: str) -> None:
    """Replace main() by one that raises: exit 0, and the error is named (stderr, or the degraded note)."""
    src = (HOOKS / hook).read_text(encoding="utf-8")
    marker = 'if __name__ == "__main__":'
    assert src.count(marker) == 1, hook
    probe = tmp_path / hook
    probe.write_text(src.replace(marker, PROBE + marker), encoding="utf-8")
    env = {**os.environ, "PYTHONPATH": str(HOOKS)}
    p = subprocess.run([sys.executable, str(probe)], input="{}", capture_output=True, text=True, env=env,
                       timeout=30, check=False)
    assert p.returncode == 0, p.stderr
    if hook == "session_startup_intelligence.py":
        assert "degraded fail-open" in json.loads(p.stdout)["hookSpecificOutput"]["additionalContext"]
    else:
        assert "sonda-de-ultima-linha" in p.stderr, (p.stdout, p.stderr)
