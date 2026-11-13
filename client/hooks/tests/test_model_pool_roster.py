"""Tests for model_pool.py (the roster, one source) and model_pool_roster.py (the SessionStart hook)."""
from __future__ import annotations

import importlib.util
import json
import os
import re
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
gate = _load("agent_model_gate")
hook = _load("model_pool_roster")


def _run(payload: dict, tmp_path: Path, **env: str) -> subprocess.CompletedProcess:
    clean = {k: v for k, v in os.environ.items()
             if k not in {"ANTHROPIC_BASE_URL", "MODEL_POOL_ROSTER_DISABLED", "MODEL_POOL_PREWARM",
                          "TOURING_HOOK_INJECTION", "CLAUDE_PROJECT_DIR", "ANTHROPIC_MODEL"}}
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
    """A fake ~/.claude/sessions and /proc: each row is a live `claude` process with its env.

    `proc_start` is the registry's `procStart` (omitted when absent); `starttime` is field 22 of the fake
    /proc/<pid>/stat (no stat file when absent).
    """
    sessions, proc = tmp_path / "sessions", tmp_path / "proc"
    sessions.mkdir()
    for row in rows:
        meta = {"pid": row["pid"], "name": row["name"], "cwd": row["cwd"], "status": row.get("status", "idle")}
        if "proc_start" in row:
            meta["procStart"] = row["proc_start"]
        (sessions / f"{row['pid']}.json").write_text(json.dumps(meta))
        base = proc / str(row["pid"])
        base.mkdir(parents=True)
        (base / "comm").write_text("claude\n")
        (base / "environ").write_bytes("\0".join(f"{k}={v}" for k, v in row.get("env", {}).items()).encode())
        if "starttime" in row:  # state + 18 filler fields, then starttime = field 22 (index 19 after the comm's `)`)
            tail = " ".join(["S", *["0"] * 18, str(row["starttime"]), "0 0"])
            (base / "stat").write_text(f"{row['pid']} (claude) {tail}\n")
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


# ---- P1: the announcement never promises a seat the gate refuses ------------------------------------------

def _ordered_seats(text: str) -> list[str]:
    """The seats the startup order names, in order; [] when the text orders none. The stated count must match."""
    match = re.search(r"invoque os (\d+) assentos — (.+?) — cada um", text)
    if not match:
        return []
    seats = match.group(2).split(" · ")
    assert int(match.group(1)) == len(seats), "the order says one count and names another"
    return seats


def _call_for(text: str, seat: str) -> dict:
    """The `Agent` arguments the roster line of `seat` teaches (`opus-1`, or one of `sonnet-1..3`)."""
    family, number = seat.rsplit("-", 1)
    for line in text.splitlines():
        head = re.match(r"\s*(\w+)-(\d+)(?:\.\.(\d+))?\s", line)
        if head and head.group(1) == family and int(head.group(2)) <= int(number) <= int(head.group(3) or head.group(2)):
            args = re.search(r'Agent\(model="(\w+)", effort="(\w+)"\)', line)
            assert args, f"the line of {seat} orders it but teaches no Agent(...) call: {line!r}"
            return {"subagent_type": "general-purpose", "model": args.group(1), "effort": args.group(2)}
    raise AssertionError(f"no roster line for {seat}")


SESSIONS = [("claude-fable-5-2", "fable"), ("claude-opus-5-5", "opus"), ("claude-sonnet-5-5", "sonnet"), (None, None)]


@pytest.mark.parametrize(("model", "family"), SESSIONS)
def test_no_ordered_call_is_refused_by_the_gate(tmp_path: Path, model: str | None, family: str | None) -> None:
    payload = {"source": "startup", "cwd": str(tmp_path), **({"model": model} if model else {})}
    text = hook.build_context(payload, {}, tmp_path, tmp_path / "proc")
    seats = _ordered_seats(text)
    assert seats, "startup must order seats"
    for seat in seats:
        verdict = gate.decide(_call_for(text, seat), {}, [], tmp_path, session_family=family)
        assert verdict.allowed, f"{seat} is ordered but the gate says {verdict.why}"


@pytest.mark.parametrize(("model", "opens"), [("claude-fable-5-2", True), ("claude-opus-5-5", False),
                                              ("claude-sonnet-5-5", False), (None, False)])
def test_the_opus_seat_is_ordered_only_under_a_fable_session_and_is_said_closed_otherwise(
        tmp_path: Path, model: str | None, opens: bool) -> None:
    payload = {"source": "startup", "cwd": str(tmp_path), **({"model": model} if model else {})}
    text = hook.build_context(payload, {}, tmp_path, tmp_path / "proc")
    seats = _ordered_seats(text)
    assert ("opus-1" in seats) is opens and len(seats) == (9 if opens else 8)
    opus_line = next(line for line in text.splitlines() if line.lstrip().startswith("opus-1"))
    assert ("FECHADO" in opus_line) is not opens


def test_the_session_model_resolves_like_the_gate_payload_then_transcript_then_env_then_settings(tmp_path: Path) -> None:
    def seats(payload: dict, env: dict) -> int:
        return len(_ordered_seats(hook.build_context({"source": "startup", "cwd": str(tmp_path), **payload}, env,
                                                     tmp_path, tmp_path / "proc")))

    assert seats({}, {"ANTHROPIC_MODEL": "claude-fable-5-2"}) == 9          # env
    assert seats({}, {"ANTHROPIC_MODEL": "claude-opus-5-5"}) == 8
    (tmp_path / ".claude").mkdir()
    (tmp_path / ".claude" / "settings.json").write_text(json.dumps({"model": "claude-fable-5-2"}))
    assert seats({}, {}) == 9                                                # settings.json
    assert seats({"model": "claude-opus-5-5"}, {"ANTHROPIC_MODEL": "claude-fable-5-2"}) == 8  # payload wins


def test_importing_the_gate_has_no_side_effect(tmp_path: Path) -> None:
    proc = subprocess.run([sys.executable, "-c", f"import sys; sys.path.insert(0, {str(HOOKS)!r}); import agent_model_gate"],
                          input=b"", capture_output=True, env={**os.environ, "HOME": str(tmp_path)}, timeout=20, check=False)
    assert proc.returncode == 0 and proc.stdout == b"" and proc.stderr == b""
    assert not (tmp_path / ".claude").exists()


# ---- P2: a recycled PID is not the old session ---------------------------------------------------------------

KIMI = {"ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/", "ANTHROPIC_MODEL": "kimi-novo"}


def test_a_recycled_pid_whose_starttime_differs_from_the_registry_is_a_dead_entry(tmp_path: Path) -> None:
    cwd = str(tmp_path / "proj")
    sessions, proc = _sessions(tmp_path, [{"pid": 42, "name": "irmã-antiga", "cwd": cwd, "env": KIMI,
                                           "proc_start": "111", "starttime": 222}])
    rows, stale = hook.live_sisters(cwd, sessions, proc)
    assert rows == [] and stale == 1
    assert "irmã-antiga" not in hook.sisters_line(cwd, sessions, proc)


def test_the_same_incarnation_is_still_listed(tmp_path: Path) -> None:
    cwd = str(tmp_path / "proj")
    sessions, proc = _sessions(tmp_path, [{"pid": 42, "name": "irmã", "cwd": cwd, "env": KIMI,
                                           "proc_start": "5015532", "starttime": 5015532}])
    rows, stale = hook.live_sisters(cwd, sessions, proc)
    assert [r["name"] for r in rows] == ["irmã"] and stale == 0


def test_a_registry_without_procstart_is_taken_as_before(tmp_path: Path) -> None:
    cwd = str(tmp_path / "proj")
    sessions, proc = _sessions(tmp_path, [{"pid": 42, "name": "irmã", "cwd": cwd, "env": KIMI, "starttime": 222}])
    rows, stale = hook.live_sisters(cwd, sessions, proc)
    assert [r["name"] for r in rows] == ["irmã"] and stale == 0


def test_a_procstart_with_no_readable_stat_is_not_invented_into_a_death(tmp_path: Path) -> None:
    cwd = str(tmp_path / "proj")
    sessions, proc = _sessions(tmp_path, [{"pid": 42, "name": "irmã", "cwd": cwd, "env": KIMI, "proc_start": "111"}])
    rows, stale = hook.live_sisters(cwd, sessions, proc)
    assert [r["name"] for r in rows] == ["irmã"] and stale == 0


def test_the_real_live_registry_procstart_is_the_proc_starttime() -> None:
    """The field's meaning, checked on whatever real sessions exist (skipped where there are none)."""
    pairs = []
    for path in (Path.home() / ".claude" / "sessions").glob("*.json"):
        try:
            meta = json.loads(path.read_text(encoding="utf-8"))
            stat = Path(f"/proc/{int(meta['pid'])}/stat").read_text()
        except (OSError, ValueError, KeyError):
            continue
        if "procStart" in meta:
            pairs.append((str(meta["procStart"]), stat.rsplit(")", 1)[1].split()[19]))
    if not pairs:
        pytest.skip("no live interactive session with procStart on this machine")
    assert all(registry == live for registry, live in pairs)


# ---- P3: a symlinked cwd does not hide a live sister ---------------------------------------------------------

def test_a_symlinked_cwd_matches_the_real_one_in_both_directions(tmp_path: Path) -> None:
    real = tmp_path / "real"
    real.mkdir()
    link = tmp_path / "link"
    link.symlink_to(real)
    sessions, proc = _sessions(tmp_path, [{"pid": 7, "name": "pelo-real", "cwd": str(real), "env": KIMI},
                                          {"pid": 8, "name": "pelo-link", "cwd": str(link), "env": KIMI}])
    for hook_cwd in (str(real), str(link)):
        rows, _ = hook.live_sisters(hook_cwd, sessions, proc)
        assert sorted(r["name"] for r in rows) == ["pelo-link", "pelo-real"], hook_cwd
