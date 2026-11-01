#!/usr/bin/env python3
"""Tests for scout_perpetuo_reminder.py — the SessionStart affordance hook.

The hook is exercised exactly as production runs it: a subprocess fed the
SessionStart JSON payload on stdin. Contract: exit 0 always (fail-open) and a
dense additionalContext that carries the armed command for THIS project.
"""

import json
import subprocess
import sys
import time
from pathlib import Path

import pytest

HOOK = Path(__file__).resolve().parent / "scout_perpetuo_reminder.py"


def run_hook(stdin_text: str) -> tuple[int, str]:
    proc = subprocess.run([sys.executable, str(HOOK)], input=stdin_text,
                          capture_output=True, text=True, timeout=30)
    return proc.returncode, proc.stdout


def context_of(stdout: str) -> str:
    payload = json.loads(stdout)
    assert payload["hookSpecificOutput"]["hookEventName"] == "SessionStart"
    return payload["hookSpecificOutput"]["additionalContext"]


def write_state(root: Path, topic: str, *, last_cycle_age_h: float,
                interval_h: float, entries: list[dict]) -> None:
    state_dir = root / ".touring" / "scout-perpetuo"
    state_dir.mkdir(parents=True)
    state_dir.joinpath("slug.json").write_text(json.dumps({
        "topic": topic, "interval_hours": interval_h,
        "last_cycle_ts": time.time() - last_cycle_age_h * 3600,
        "history": entries,
    }), encoding="utf-8")


def test_overdue_topic_emits_must_and_armed_queue(tmp_path: Path):
    write_state(tmp_path, "adw software factory", last_cycle_age_h=100, interval_h=6,
                entries=[{"ts": 1.0, "yield": 3, "findings_total": 3,
                          "ticket": "task_123", "adw": "audit",
                          "start_hint": 'touring factory start "scout-ticket: x"',
                          "explore_exit": 0}])
    code, out = run_hook(json.dumps({"cwd": str(tmp_path)}))
    assert code == 0
    ctx = context_of(out)
    assert "VENCIDO" in ctx and "adw software factory" in ctx
    assert "MUST (antes de trabalho novo)" in ctx
    assert f"--root {tmp_path}" in ctx
    assert 'touring factory start "scout-ticket: x"' in ctx  # fila engatilhada


def test_fresh_topic_is_should_not_must(tmp_path: Path):
    write_state(tmp_path, "t", last_cycle_age_h=1, interval_h=24,
                entries=[{"ts": 1.0, "yield": 0, "findings_total": 0,
                          "ticket": "", "explore_exit": 0}])
    code, out = run_hook(json.dumps({"cwd": str(tmp_path)}))
    assert code == 0
    ctx = context_of(out)
    assert "em dia" in ctx and "SHOULD (ao abrir frente nova)" in ctx
    assert "VENCIDO" not in ctx


def test_touring_project_without_scout_seeds_arming(tmp_path: Path):
    (tmp_path / ".touring").mkdir()
    code, out = run_hook(json.dumps({"cwd": str(tmp_path)}))
    assert code == 0
    ctx = context_of(out)
    assert "Nenhum scout armado" in ctx
    assert "touring adw run scout-perpetuo" in ctx


def test_walk_up_finds_root_from_subdir(tmp_path: Path):
    write_state(tmp_path, "t", last_cycle_age_h=100, interval_h=6,
                entries=[{"ts": 1.0, "yield": 1, "findings_total": 1,
                          "ticket": "task_9", "explore_exit": 0}])
    sub = tmp_path / "a" / "b"
    sub.mkdir(parents=True)
    code, out = run_hook(json.dumps({"cwd": str(sub)}))
    assert code == 0
    assert "VENCIDO" in context_of(out)


def test_no_touring_anywhere_still_explains(tmp_path: Path):
    code, out = run_hook(json.dumps({"cwd": str(tmp_path / "x")}))
    assert code == 0
    assert "gerador de demanda" in context_of(out)


def test_garbage_stdin_fails_open(tmp_path: Path):
    code, _ = run_hook("not-json{{{")
    assert code == 0


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
