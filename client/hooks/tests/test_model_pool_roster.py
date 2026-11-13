"""Tests for model_pool.py (the roster, one source) and model_pool_roster.py (the SessionStart hook)."""
from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

HOOKS = Path(__file__).resolve().parents[1]
SCRIPT = HOOKS / "model_pool_roster.py"


def _load(name: str):
    sys.path.insert(0, str(HOOKS))
    spec = importlib.util.spec_from_file_location(name, HOOKS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


pool = _load("model_pool")
hook = _load("model_pool_roster")


def _run(payload: dict, tmp_path: Path, **env: str) -> subprocess.CompletedProcess:
    clean = {k: v for k, v in os.environ.items()
             if k not in {"ANTHROPIC_BASE_URL", "MODEL_POOL_ROSTER_DISABLED", "MODEL_POOL_PREWARM",
                          "TOURING_HOOK_INJECTION", "CLAUDE_PROJECT_DIR"}}
    clean.update(HOME=str(tmp_path), TOURING_INJECTION_DEDUP_DISABLED="1", **env)
    return subprocess.run([sys.executable, str(SCRIPT)], input=json.dumps(payload).encode(), capture_output=True,
                          env=clean, timeout=20, check=False)


def _context(proc: subprocess.CompletedProcess) -> str:
    assert proc.returncode == 0, proc.stderr
    out = json.loads(proc.stdout)
    assert out["hookSpecificOutput"]["hookEventName"] == "SessionStart"
    return out["hookSpecificOutput"]["additionalContext"]


# ---- the roster itself ----------------------------------------------------------------------------------

def test_the_roster_is_the_ordered_one_fable_one_opus_three_sonnet_five_haiku() -> None:
    assert [(s.family, s.role, s.count, s.effort) for s in pool.POOL] == [
        ("fable", "orquestrador", 1, "medium"),
        ("opus", "subagente", 1, "medium"),
        ("sonnet", "subagente", 3, "high"),
        ("haiku", "subagente", 5, "xhigh"),
    ]


def test_slot_names_are_nine_and_cover_only_the_subagents() -> None:
    assert pool.slot_names() == ["opus-1", "sonnet-1", "sonnet-2", "sonnet-3",
                                 "haiku-1", "haiku-2", "haiku-3", "haiku-4", "haiku-5"]


@pytest.mark.parametrize(("model", "effort"), [
    ("opus", "medium"), ("claude-opus-5-5", "medium"), ("Sonnet", "high"), ("haiku", "xhigh"),
    ("claude-haiku-5-5", "xhigh"), ("fable", "medium"), ("inherit", None), ("", None), (None, None),
])
def test_default_effort_follows_the_family_and_is_none_when_there_is_no_family(model: object, effort: str | None) -> None:
    assert pool.default_effort(model) == effort


def test_the_pool_uses_aliases_only_so_the_newest_version_is_always_the_one_resolved() -> None:
    assert all(not slot.family.startswith("claude-") and "-" not in slot.family for slot in pool.POOL)


# ---- the hook: who gets the roster ----------------------------------------------------------------------

def test_an_anthropic_session_gets_the_roster_with_every_model_count_and_effort(tmp_path: Path) -> None:
    text = _context(_run({"hook_event_name": "SessionStart", "source": "resume", "cwd": str(tmp_path)}, tmp_path))
    for needle in ("fable", "opus-1", "sonnet-1..3", "haiku-1..5", "medium", "high", "xhigh",
                   'model="opus"', 'model="sonnet"', 'model="haiku"'):
        assert needle in text, needle


def test_a_sister_session_gets_nothing_because_the_matrix_is_for_anthropic_sessions_only(tmp_path: Path) -> None:
    for url in ("https://api.kimi.com/coding/", "https://api.minimax.io/anthropic"):
        proc = _run({"source": "startup", "cwd": str(tmp_path)}, tmp_path, ANTHROPIC_BASE_URL=url)
        assert proc.returncode == 0 and proc.stdout == b"", url


def test_a_subagent_gets_nothing(tmp_path: Path) -> None:
    proc = _run({"source": "startup", "cwd": str(tmp_path), "agent_id": "a1"}, tmp_path)
    assert proc.returncode == 0 and proc.stdout == b""


def test_the_human_kill_switch_silences_the_hook(tmp_path: Path) -> None:
    proc = _run({"source": "startup", "cwd": str(tmp_path)}, tmp_path, MODEL_POOL_ROSTER_DISABLED="1")
    assert proc.returncode == 0 and proc.stdout == b""


def test_the_injection_switch_off_silences_the_hook_and_full_lets_it_speak(tmp_path: Path) -> None:
    payload = {"source": "resume", "cwd": str(tmp_path)}
    off = _run(payload, tmp_path, TOURING_HOOK_INJECTION="off")
    assert off.returncode == 0 and off.stdout == b""
    assert "opus-1" in _context(_run(payload, tmp_path, TOURING_HOOK_INJECTION="full"))


def test_garbage_on_stdin_never_blocks_the_session(tmp_path: Path) -> None:
    clean = {k: v for k, v in os.environ.items() if k != "ANTHROPIC_BASE_URL"}
    proc = subprocess.run([sys.executable, str(SCRIPT)], input=b"\xff not json", capture_output=True,
                          env={**clean, "HOME": str(tmp_path)}, timeout=20, check=False)
    assert proc.returncode == 0


# ---- the hook: when the nine are invoked ----------------------------------------------------------------

def test_startup_orders_the_nine_invocations_in_the_first_turn(tmp_path: Path) -> None:
    text = _context(_run({"source": "startup", "cwd": str(tmp_path)}, tmp_path))
    assert "MUST" in text and "run_in_background" not in text
    for name in pool.slot_names():
        assert name in text, name


@pytest.mark.parametrize("source", ["resume", "clear", "compact", "fork"])
def test_other_sources_announce_the_roster_without_ordering_a_new_spawn(tmp_path: Path, source: str) -> None:
    text = _context(_run({"source": source, "cwd": str(tmp_path)}, tmp_path))
    assert "opus-1" in text and "MUST" not in text


def test_prewarm_off_keeps_the_roster_and_drops_the_order(tmp_path: Path) -> None:
    text = _context(_run({"source": "startup", "cwd": str(tmp_path)}, tmp_path, MODEL_POOL_PREWARM="0"))
    assert "opus-1" in text and "MUST" not in text and "sob demanda" in text


# ---- the hook: sister sessions --------------------------------------------------------------------------

def _sessions(tmp_path: Path, rows: list[dict]) -> tuple[Path, Path]:
    """A fake ~/.claude/sessions and /proc: each row is a live `claude` process with its env."""
    sessions, proc = tmp_path / "sessions", tmp_path / "proc"
    sessions.mkdir()
    for row in rows:
        (sessions / f"{row['pid']}.json").write_text(json.dumps(
            {"pid": row["pid"], "name": row["name"], "cwd": row["cwd"], "status": row.get("status", "idle")}))
        base = proc / str(row["pid"])
        base.mkdir(parents=True)
        (base / "comm").write_text("claude\n")
        (base / "environ").write_bytes("\0".join(f"{k}={v}" for k, v in row.get("env", {}).items()).encode())
    return sessions, proc


def test_live_kimi_and_minimax_sessions_of_this_project_are_named_with_model_and_status(tmp_path: Path) -> None:
    cwd = str(tmp_path / "proj")
    sessions, proc = _sessions(tmp_path, [
        {"pid": 11, "name": "touring-k", "cwd": cwd, "status": "idle",
         "env": {"ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/", "ANTHROPIC_MODEL": "kimi-k3.1-pro"}},
        {"pid": 12, "name": "touring-m", "cwd": cwd, "status": "busy",
         "env": {"ANTHROPIC_BASE_URL": "https://api.minimax.io/anthropic", "ANTHROPIC_MODEL": "MiniMax-M3.1-Flash"}},
        {"pid": 13, "name": "touring-a", "cwd": cwd, "env": {}},
        {"pid": 14, "name": "outro-k", "cwd": str(tmp_path / "outro"),
         "env": {"ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/"}},
    ])
    line = hook.sisters_line(cwd, sessions, proc)
    assert "touring-k" in line and "kimi-k3.1-pro" in line and "idle" in line
    assert "touring-m" in line and "MiniMax-M3.1-Flash" in line and "busy" in line
    assert "touring-a" not in line and "outro-k" not in line


def test_no_live_sister_is_said_as_such_with_the_dead_registry_entries_counted(tmp_path: Path) -> None:
    sessions, proc = _sessions(tmp_path, [])
    (sessions / "99.json").write_text(json.dumps({"pid": 99, "name": "morta", "cwd": str(tmp_path)}))
    line = hook.sisters_line(str(tmp_path), sessions, proc)
    assert "nenhuma" in line and "1 registro" in line


def test_a_missing_session_registry_is_declared_and_never_reported_as_zero_sisters(tmp_path: Path) -> None:
    line = hook.sisters_line(str(tmp_path), tmp_path / "ausente", tmp_path / "proc")
    assert "indisponível" in line and "nenhuma" not in line


def test_the_hook_output_carries_the_sisters_line(tmp_path: Path) -> None:
    text = _context(_run({"source": "resume", "cwd": str(tmp_path)}, tmp_path))
    assert "Sessões irmãs" in text
