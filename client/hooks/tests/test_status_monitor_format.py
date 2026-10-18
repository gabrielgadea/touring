"""test_status_monitor_format.py — the status line formats numbers without bc.

status_monitor.sh formatted millions and compared the cost with `bc`, which this machine does not have. The
fallback of `$(… | bc || echo "$((n / 1000))K")M` printed a 1M window as "1000KM", and the cost comparison read
empty, so a positive cost was never shown. Contract (17/10/2026): one decimal for millions (bc's `scale=1`
truncation: 1.0M, 1.2M), "$1.50" for a positive cost, nothing for zero, and bc is never invoked.
"""
from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

from helpers import CLIENT_HOOKS

STATUS = CLIENT_HOOKS / "status_monitor.sh"


def _render(home: Path, tokens: int, cost: float = 0.0) -> tuple[str, Path]:
    fake_bin = home / "fake-bin"
    fake_bin.mkdir(exist_ok=True)
    marker = home / "bc-was-called"
    bc = fake_bin / "bc"
    bc.write_text(f"#!/bin/sh\ntouch '{marker}'\nexit 1\n")  # a broken bc: the same as no bc for `|| fallback`
    bc.chmod(0o755)
    payload = {"session_id": "fmt-sess", "model": {"display_name": "T", "id": "t"}, "cost": {"total_cost_usd": cost},
               "context_window": {"context_window_size": 1_000_000, "current_usage": {"input_tokens": tokens}}}
    env = {**os.environ, "HOME": str(home), "PATH": f"{fake_bin}{os.pathsep}{os.environ.get('PATH', '')}"}
    proc = subprocess.run(["bash", str(STATUS)], input=json.dumps(payload).encode(), capture_output=True, env=env,
                          timeout=15, check=False)
    assert proc.returncode == 0, proc.stderr
    return proc.stdout.decode("utf-8", errors="replace"), marker


def test_a_one_million_window_renders_as_one_decimal_millions(tmp_path: Path) -> None:
    out, _ = _render(tmp_path, 250_000)
    assert "250K/1.0M (25.0%)" in out
    assert "KM" not in out


def test_tokens_above_one_million_render_in_millions_truncated_like_bc(tmp_path: Path) -> None:
    out, _ = _render(tmp_path, 1_299_999)
    assert "1.2M/1.0M" in out


def test_a_positive_cost_is_shown_with_two_decimals(tmp_path: Path) -> None:
    out, _ = _render(tmp_path, 250_000, cost=1.5)
    assert "$1.50" in out


def test_a_zero_cost_shows_no_cost_segment(tmp_path: Path) -> None:
    out, _ = _render(tmp_path, 250_000, cost=0.0)
    assert "$" not in out


def test_the_status_line_never_calls_bc(tmp_path: Path) -> None:
    _, marker = _render(tmp_path, 1_299_999, cost=2.0)
    assert not marker.exists(), "status_monitor.sh still depends on bc"
