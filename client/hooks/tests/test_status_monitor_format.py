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
    return _render_model(home, tokens, cost, "T", "t")


def _render_model(home: Path, tokens: int, cost: float, display_name: str, model_id: str) -> tuple[str, Path]:
    fake_bin = home / "fake-bin"
    fake_bin.mkdir(exist_ok=True)
    marker = home / "bc-was-called"
    bc = fake_bin / "bc"
    bc.write_text(f"#!/bin/sh\ntouch '{marker}'\nexit 1\n")  # a broken bc: the same as no bc for `|| fallback`
    bc.chmod(0o755)
    payload = {"session_id": "fmt-sess", "model": {"display_name": display_name, "id": model_id},
               "cost": {"total_cost_usd": cost},
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


# ── rótulo amigável (26/10/2026) ─────────────────────────────────────────────────
# O `display_name` da statusline vem da configuração LOCAL, não da resposta da API:
# para um id que o Claude Code não reconhece, sai o id cru. O mapeamento para o nome
# comercial está no status_monitor.sh (as env vars de rótulo do Claude Code não chegam
# à statusline — elas rotulam a linha do /model).
def test_the_raw_minimax_id_is_rendered_as_its_commercial_name(tmp_path: Path) -> None:
    out, _ = _render_model(tmp_path, 250_000, 0.0, "MiniMax-M3.1-Flash", "MiniMax-M3.1-Flash")
    assert "MiniMax M3.1 Flash" in out
    assert "MiniMax-M3.1-Flash" not in out


def test_the_retired_preview_id_is_rendered_as_the_same_commercial_name(tmp_path: Path) -> None:
    out, _ = _render_model(tmp_path, 250_000, 0.0, "MiniMax-M3.1-Flash-Preview", "MiniMax-M3.1-Flash-Preview")
    assert "MiniMax M3.1 Flash" in out
    assert "Preview" not in out


def test_a_minimax_id_keeps_its_one_million_window_under_the_renaming(tmp_path: Path) -> None:
    out, _ = _render_model(tmp_path, 250_000, 0.0, "MiniMax-M3.1-Flash", "MiniMax-M3.1-Flash")
    assert "250K/1.0M (25.0%)" in out


def test_an_unrelated_model_keeps_its_own_name(tmp_path: Path) -> None:
    out, _ = _render_model(tmp_path, 250_000, 0.0, "Opus 5.5", "claude-opus-5-5")
    assert "Opus 5.5" in out
