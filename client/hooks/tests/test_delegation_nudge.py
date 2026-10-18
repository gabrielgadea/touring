"""test_delegation_nudge.py — o hook que aconselha delegar quando a sessão principal lê/busca sozinha.

Cada teste roda o hook REAL (subprocesso, payload JSON no stdin) com HOME, cwd e env sintéticos:
nada aqui lê ou grava ~/.claude. O estado por sessão é o ponto de injeção do relógio — os testes de
janela e de cooldown o semeiam com carimbos no passado em vez de dormir.

O hook só ACONSELHA: nenhum teste aceita `permissionDecision` na saída.
"""

from __future__ import annotations

import fcntl
import importlib.util
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path
from types import ModuleType
from typing import Any

import pytest
from helpers import CLIENT_HOOKS, REPO_ROOT, HookResult, schema_violations

HOOK = CLIENT_HOOKS / "delegation_nudge.py"
CANONICAL_DOC = REPO_ROOT / "docs" / "economia-de-creditos-e-contexto.md"
SESSION_ENV = ("ANTHROPIC_BASE_URL", "DELEGATION_NUDGE_DISABLED", "PYTHONIOENCODING")
Run = tuple[subprocess.CompletedProcess[str], dict[str, Any]]


def _load(path: Path, name: str) -> ModuleType:
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None, path
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _env(tmp_path: Path, env_extra: dict[str, str] | None = None) -> dict[str, str]:
    (tmp_path / "home").mkdir(exist_ok=True)
    env = {k: v for k, v in os.environ.items() if k not in SESSION_ENV}
    env.update(HOME=str(tmp_path / "home"), **(env_extra or {}))
    return env


def _run_payload(tmp_path: Path, payload: dict[str, Any] | str | bytes, env_extra: dict[str, str] | None = None) -> Run:
    if isinstance(payload, dict):
        data = json.dumps(payload).encode("utf-8", errors="surrogatepass")
    else:
        data = payload.encode() if isinstance(payload, str) else payload
    proc = subprocess.run([sys.executable, str(HOOK)], input=data, capture_output=True, env=_env(tmp_path, env_extra),
                          timeout=15, check=False)
    text = subprocess.CompletedProcess(proc.args, proc.returncode, proc.stdout.decode("utf-8", "replace"),
                                       proc.stderr.decode("utf-8", "replace"))
    return text, (json.loads(text.stdout) if text.stdout.strip() else {})


def _tool(tmp_path: Path, tool_name: str, tool_input: dict[str, Any], *, session: str = "sess-a",
          transcript: str | None = None, extra: dict[str, Any] | None = None,
          env_extra: dict[str, str] | None = None) -> Run:
    return _run_payload(tmp_path, {
        "session_id": session, "transcript_path": transcript or str(tmp_path / "t" / f"{session}.jsonl"),
        "cwd": str(tmp_path / "proj"), "hook_event_name": "PreToolUse", "tool_name": tool_name,
        "tool_input": tool_input, **(extra or {}),
    }, env_extra)


def _file(tmp_path: Path, name: str, size: int) -> Path:
    path = tmp_path / "proj" / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"x" * size)
    return path


def _read(tmp_path: Path, name: str, size: int = 100, **kwargs: Any) -> Run:
    path = _file(tmp_path, name, size)
    tool_input: dict[str, Any] = {"file_path": str(path), **kwargs.pop("tool_input", {})}
    return _tool(tmp_path, "Read", tool_input, **kwargs)


def _context(data: dict[str, Any]) -> str:
    return data["hookSpecificOutput"]["additionalContext"]


def _state_dir(tmp_path: Path) -> Path:
    return tmp_path / "home" / ".claude" / "touring" / "delegation_nudge"


def _state(tmp_path: Path, session: str = "sess-a") -> dict[str, Any]:
    return json.loads((_state_dir(tmp_path) / f"{session}.json").read_text())


def _seed(tmp_path: Path, session: str, *, calls: list[list[Any]] | None = None, last_nudge: float = 0.0,
          switches: list[str] | None = None) -> None:
    _state_dir(tmp_path).mkdir(parents=True, exist_ok=True)
    (_state_dir(tmp_path) / f"{session}.json").write_text(json.dumps(
        {"calls": calls or [], "last_nudge": last_nudge, "switches": switches or []}))


def _log(tmp_path: Path) -> list[dict[str, Any]]:
    path = tmp_path / "home" / ".claude" / "touring" / "delegation_nudge.jsonl"
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def _silent(run: Run) -> None:
    proc, data = run
    assert proc.returncode == 0, proc.stderr
    assert data == {}, data


# --- the read burst: >= 6 calls OR >= 150 KB in 10 minutes ----------------------------------------

def test_the_sixth_read_in_the_window_advises_and_the_first_five_are_silent(tmp_path: Path) -> None:
    for i in range(5):
        _silent(_read(tmp_path, f"f{i}.rs"))
    proc, data = run = _read(tmp_path, "f5.rs")
    assert proc.returncode == 0, proc.stderr
    hso = data["hookSpecificOutput"]
    assert set(hso) == {"hookEventName", "additionalContext"}, "advisory only: no permissionDecision"
    assert hso["hookEventName"] == "PreToolUse"
    as_bytes = subprocess.CompletedProcess(proc.args, proc.returncode, proc.stdout.encode(), proc.stderr.encode())
    assert schema_violations("PreToolUse", HookResult(HOOK, {}, as_bytes)) == []
    assert run[0].stderr == ""


def test_the_advice_is_dense_and_specific(tmp_path: Path) -> None:
    for i in range(5):
        _read(tmp_path, f"src/mod{i}.rs", 2048)
    text = _context(_read(tmp_path, "src/ultimo.rs", 2048)[1])
    assert "6 leituras" in text and "12 KB" in text
    for recent in ("src/ultimo.rs", "src/mod4.rs", "src/mod3.rs"):  # the 3 most recent, newest first
        assert recent in text, recent
    assert text.index("src/ultimo.rs") < text.index("src/mod4.rs") < text.index("src/mod3.rs")
    assert "src/mod0.rs" not in text
    assert 'Agent(subagent_type="Explore", model="haiku", prompt="' in text
    assert "touring run --lang python --file" in text
    assert "§1" in text and "economia-de-creditos-e-contexto.md" in text
    assert "SEGREDO" not in text
    assert len(text) < 900, len(text)


def test_the_volume_trigger_fires_below_six_calls(tmp_path: Path) -> None:
    _silent(_read(tmp_path, "grande1.log", 80 * 1024))
    text = _context(_read(tmp_path, "grande2.log", 80 * 1024)[1])
    assert "2 leituras" in text and "160 KB" in text


def test_read_volume_is_bounded_by_limit_and_by_the_default_line_cap(tmp_path: Path) -> None:
    """Read of a 1 MB file with limit=10 costs ~0.8 KB; without limit it is capped at 2000 lines, not the file."""
    _read(tmp_path, "enorme.txt", 1024 * 1024, tool_input={"limit": 10})
    _read(tmp_path, "enorme2.txt", 1024 * 1024)
    kbs = [c[1] for c in _state(tmp_path)["calls"]]
    assert kbs[0] == pytest.approx(10 * 80 / 1024, abs=0.01)
    assert kbs[1] == pytest.approx(2000 * 80 / 1024, abs=0.01)


def test_a_small_file_costs_its_own_size_and_a_missing_one_costs_nothing(tmp_path: Path) -> None:
    _read(tmp_path, "pequeno.txt", 3 * 1024)
    _tool(tmp_path, "Read", {"file_path": str(tmp_path / "proj" / "nao-existe.rs")})
    assert [round(c[1], 2) for c in _state(tmp_path)["calls"]] == [3.0, 0.0]


def test_grep_and_glob_count_as_one_call_each_and_are_named_in_the_advice(tmp_path: Path) -> None:
    for i in range(3):
        _tool(tmp_path, "Grep", {"pattern": f"foo{i}", "path": str(tmp_path / "proj" / "src")})
    for i in range(2):
        _tool(tmp_path, "Glob", {"pattern": f"**/*.py{i}"})
    text = _context(_tool(tmp_path, "Grep", {"pattern": "ultimo_padrao"})[1])
    assert "6 leituras" in text
    assert 'Grep "ultimo_padrao"' in text and "Glob **/*.py1" in text
    assert [c[1] for c in _state(tmp_path)["calls"]] == [0.0] * 6


def test_an_advice_is_followed_by_ten_minutes_of_silence(tmp_path: Path) -> None:
    for i in range(5):
        _read(tmp_path, f"f{i}.rs")
    assert _context(_read(tmp_path, "f5.rs")[1])
    for i in range(6, 12):
        _silent(_read(tmp_path, f"f{i}.rs"))


def test_after_the_cooldown_a_still_hot_window_advises_again(tmp_path: Path) -> None:
    now = time.time()
    _seed(tmp_path, "sess-a", calls=[[now - 30 * i, 1.0, f"a{i}"] for i in range(6)], last_nudge=now - 601)
    assert "leituras" in _context(_read(tmp_path, "novo.rs")[1])
    _seed(tmp_path, "sess-a", calls=[[now - 30 * i, 1.0, f"a{i}"] for i in range(6)], last_nudge=now - 60)
    _silent(_read(tmp_path, "novo2.rs"))


def test_calls_older_than_ten_minutes_leave_the_window(tmp_path: Path) -> None:
    now = time.time()
    _seed(tmp_path, "sess-a", calls=[[now - 601 - i, 50.0, f"velho{i}"] for i in range(8)])
    _silent(_read(tmp_path, "novo.rs"))
    assert len(_state(tmp_path)["calls"]) == 1


def test_sessions_do_not_share_a_window(tmp_path: Path) -> None:
    for i in range(5):
        _read(tmp_path, f"f{i}.rs", session="sess-a")
    _silent(_read(tmp_path, "g0.rs", session="sess-b"))
    assert "leituras" in _context(_read(tmp_path, "f5.rs", session="sess-a")[1])


def test_parallel_hooks_of_one_session_do_not_lose_calls(tmp_path: Path) -> None:
    """Read/Grep/Glob fire in parallel: the state is read-modify-written under a lock."""
    payloads = [json.dumps({"session_id": "sess-p", "transcript_path": "/x/y.jsonl", "cwd": str(tmp_path),
                            "hook_event_name": "PreToolUse", "tool_name": "Grep",
                            "tool_input": {"pattern": f"p{i}"}}).encode() for i in range(12)]
    procs = [subprocess.Popen([sys.executable, str(HOOK)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, env=_env(tmp_path)) for _ in payloads]
    outputs = [p.communicate(data, timeout=30)[0] for p, data in zip(procs, payloads, strict=True)]
    assert len(_state(tmp_path, "sess-p")["calls"]) == 12
    assert sum(1 for out in outputs if out.strip()) == 1, "exactly one advice: the cooldown holds under concurrency"


def test_the_state_is_read_modified_and_written_under_the_lock(tmp_path: Path) -> None:
    """Deterministic proof of the flock (the race test above only makes a missing lock likely to show)."""
    _state_dir(tmp_path).mkdir(parents=True)
    state_file = _state_dir(tmp_path) / "sess-l.json"
    state_file.write_text(json.dumps({"calls": [], "last_nudge": 0.0, "switches": []}))
    payload = json.dumps({"session_id": "sess-l", "transcript_path": "/x/y.jsonl", "cwd": str(tmp_path),
                          "hook_event_name": "PreToolUse", "tool_name": "Grep", "tool_input": {"pattern": "p"}})
    with state_file.open("r+") as holder:
        fcntl.flock(holder, fcntl.LOCK_EX)
        proc = subprocess.Popen([sys.executable, str(HOOK)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, env=_env(tmp_path))
        assert proc.stdin is not None
        proc.stdin.write(payload.encode())
        proc.stdin.close()
        time.sleep(0.6)
        assert proc.poll() is None, "the hook must wait for the lock holder"
        assert json.loads(state_file.read_text())["calls"] == []
        fcntl.flock(holder, fcntl.LOCK_UN)
    assert proc.wait(timeout=15) == 0
    assert len(_state(tmp_path, "sess-l")["calls"]) == 1


def test_a_new_session_prunes_state_files_untouched_for_a_week(tmp_path: Path) -> None:
    _state_dir(tmp_path).mkdir(parents=True)
    old, recent = _state_dir(tmp_path) / "velha.json", _state_dir(tmp_path) / "recente.json"
    for path in (old, recent):
        path.write_text("{}")
    week_ago = time.time() - 8 * 86_400
    os.utime(old, (week_ago, week_ago))
    _read(tmp_path, "f.rs")
    assert not old.exists() and recent.exists()
    assert (_state_dir(tmp_path) / "sess-a.json").exists()


# --- who is advised: only the MAIN session on an Anthropic backend --------------------------------

def _fill(tmp_path: Path, session: str = "sess-a", n: int = 5) -> None:
    now = time.time()
    _seed(tmp_path, session, calls=[[now - i, 1.0, f"a{i}"] for i in range(n)])


def test_a_subagent_is_never_advised_by_agent_id(tmp_path: Path) -> None:
    _fill(tmp_path)
    run = _read(tmp_path, "f.rs", extra={"agent_id": "a5731db2cffd99402", "agent_type": "Explore"})
    _silent(run)
    assert len(_state(tmp_path)["calls"]) == 5, "a subagent's reads are not the main session's"


def test_a_subagent_is_never_advised_by_transcript_path_fallback(tmp_path: Path) -> None:
    _fill(tmp_path)
    _silent(_read(tmp_path, "f.rs", transcript="/h/.claude/projects/p/sess-a/subagents/agent-a1.jsonl"))
    assert len(_state(tmp_path)["calls"]) == 5


@pytest.mark.parametrize("base_url", ["https://api.kimi.com/coding/", "https://api.moonshot.ai/anthropic",
                                      "https://api.minimax.io/anthropic"])
def test_sister_sessions_are_skipped(tmp_path: Path, base_url: str) -> None:
    _fill(tmp_path)
    _silent(_read(tmp_path, "f.rs", env_extra={"ANTHROPIC_BASE_URL": base_url}))
    assert len(_state(tmp_path)["calls"]) == 5


@pytest.mark.parametrize("base_url", ["https://api.anthropic.com", "https://litellm.example.org/v1"])
def test_anthropic_or_unknown_gateway_is_advised(tmp_path: Path, base_url: str) -> None:
    _fill(tmp_path)
    assert "leituras" in _context(_read(tmp_path, "f.rs", env_extra={"ANTHROPIC_BASE_URL": base_url})[1])


def test_the_human_kill_switch(tmp_path: Path) -> None:
    _fill(tmp_path)
    _silent(_read(tmp_path, "f.rs", env_extra={"DELEGATION_NUDGE_DISABLED": "1"}))
    assert len(_state(tmp_path)["calls"]) == 5


@pytest.mark.parametrize("tool_name", ["Bash", "Edit", "Write", "Agent"])
def test_other_tools_leave_no_trace(tmp_path: Path, tool_name: str) -> None:
    _silent(_tool(tmp_path, tool_name, {"command": "ls", "file_path": "/x"}))
    assert not _state_dir(tmp_path).exists()
    assert _log(tmp_path) == []


def test_an_unknown_event_is_ignored(tmp_path: Path) -> None:
    _silent(_run_payload(tmp_path, {"session_id": "s", "hook_event_name": "Stop", "tool_name": "Read"}))
    assert not _state_dir(tmp_path).exists()


@pytest.mark.parametrize("session", ["../../evil", "a/b", "", None])
def test_a_hostile_or_missing_session_id_never_escapes_the_state_dir(tmp_path: Path, session: Any) -> None:
    run = _tool(tmp_path, "Grep", {"pattern": "x"}, session=session)  # type: ignore[arg-type]
    assert run[0].returncode == 0
    assert not (tmp_path / "evil.json").exists() and not (tmp_path / "home" / "evil.json").exists()
    escaped = [p for p in tmp_path.rglob("*.json") if _state_dir(tmp_path) not in p.parents]
    assert escaped == []


# --- the log: one line per advice ------------------------------------------------------------------

def test_every_advice_is_logged_and_silence_is_not(tmp_path: Path) -> None:
    for i in range(5):
        _read(tmp_path, f"f{i}.rs", 1024)
    assert _log(tmp_path) == []
    _read(tmp_path, "f5.rs", 1024)
    _read(tmp_path, "f6.rs", 1024)
    (record,) = _log(tmp_path)
    assert set(record) == {"ts", "session", "evento", "n", "kb", "alvos"}
    assert (record["session"], record["evento"], record["n"]) == ("sess-a", "leitura", 6)
    assert record["kb"] == pytest.approx(6.0, abs=0.1)
    assert record["alvos"][0].endswith("f5.rs") and len(record["alvos"]) == 3
    datetime.fromisoformat(record["ts"])


# --- robustness: exit 0, no output, loud only in the log -------------------------------------------

@pytest.mark.parametrize("payload", ["isto não é JSON", b"\xff\xfe{lixo", "[]", "null", "42", ""])
def test_garbage_input_exits_zero_with_no_output(tmp_path: Path, payload: str | bytes) -> None:
    proc, data = _run_payload(tmp_path, payload)
    assert (proc.returncode, proc.stdout, data) == (0, "", {})


@pytest.mark.parametrize("tool_input", [None, "x", [], {"file_path": 5}, {"file_path": None, "limit": "dez"}])
def test_malformed_tool_input_never_crashes(tmp_path: Path, tool_input: Any) -> None:
    proc, _ = _tool(tmp_path, "Read", tool_input)
    assert proc.returncode == 0 and proc.stdout == ""


@pytest.mark.parametrize("content", ["{corrompido", "[]", '{"calls": "x"}', '{"calls": [[1]], "last_nudge": "a"}', ""])
def test_a_corrupt_state_resets_instead_of_crashing(tmp_path: Path, content: str) -> None:
    _state_dir(tmp_path).mkdir(parents=True)
    (_state_dir(tmp_path) / "sess-a.json").write_text(content)
    _silent(_read(tmp_path, "f.rs"))
    assert len(_state(tmp_path)["calls"]) == 1


def test_an_unwritable_state_dir_is_an_error_line_not_a_crash(tmp_path: Path) -> None:
    (tmp_path / "home" / ".claude").mkdir(parents=True)
    (tmp_path / "home" / ".claude" / "touring").write_text("sou um arquivo, não um diretório")
    proc, data = _read(tmp_path, "f.rs")
    assert (proc.returncode, proc.stdout, data) == (0, "", {})


def test_an_internal_error_is_logged_as_an_error_event_and_stays_silent_in_the_session(tmp_path: Path) -> None:
    touring = tmp_path / "home" / ".claude" / "touring"
    touring.mkdir(parents=True)
    (touring / "delegation_nudge").write_text("sou um arquivo onde devia haver o diretório de estado")
    proc, data = _read(tmp_path, "f.rs")
    assert (proc.returncode, proc.stdout, data) == (0, "", {})
    (record,) = _log(tmp_path)
    assert (record["evento"], record["session"], record["alvos"]) == ("erro", "sess-a", ["FileExistsError"])


def test_a_hostile_output_encoding_does_not_lose_the_advice(tmp_path: Path) -> None:
    now = time.time()
    _seed(tmp_path, "sess-a", calls=[[now - i, 1.0, f"ação{i}"] for i in range(5)])
    proc, data = _read(tmp_path, "ünico.rs", env_extra={"PYTHONIOENCODING": "ascii"})
    assert proc.returncode == 0 and "leituras" in _context(data)


# --- model switch (UserPromptSubmit) ---------------------------------------------------------------

def _assistant(model: str, uid: str, ts: datetime, *, sidechain: bool = False) -> dict[str, Any]:
    return {"parentUuid": "p", "isSidechain": sidechain, "type": "assistant", "uuid": uid,
            "timestamp": ts.strftime("%Y-%m-%dT%H:%M:%S.000Z"),
            "message": {"model": model, "id": f"msg_{uid}", "role": "assistant", "content": []}}


def _user(uid: str, ts: datetime) -> dict[str, Any]:
    return {"parentUuid": "p", "isSidechain": False, "type": "user", "uuid": uid,
            "timestamp": ts.strftime("%Y-%m-%dT%H:%M:%S.000Z"),
            "message": {"role": "user", "content": 'ele disse "model":"claude-haiku-5-5" no texto'}}


def _transcript(tmp_path: Path, lines: list[dict[str, Any]], name: str = "t.jsonl") -> Path:
    path = tmp_path / "t" / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("".join(json.dumps(line, separators=(",", ":")) + "\n" for line in lines))
    return path


def _prompt(tmp_path: Path, transcript: Path, *, session: str = "sess-a", extra: dict[str, Any] | None = None,
            env_extra: dict[str, str] | None = None) -> Run:
    return _run_payload(tmp_path, {"session_id": session, "transcript_path": str(transcript),
                                   "cwd": str(tmp_path), "hook_event_name": "UserPromptSubmit",
                                   "prompt": "continue", **(extra or {})}, env_extra)


NOW = datetime.now(timezone.utc)


def _switch_lines(first: str = "claude-opus-5-5", second: str = "claude-sonnet-5-5",
                  age: timedelta = timedelta(minutes=2)) -> list[dict[str, Any]]:
    return [_assistant(first, "u1", NOW - age - timedelta(minutes=3)), _user("u2", NOW - age - timedelta(minutes=2)),
            _assistant(second, "u3", NOW - age)]


def test_a_model_switch_in_the_tail_advises_once(tmp_path: Path) -> None:
    transcript = _transcript(tmp_path, _switch_lines())
    proc, data = _prompt(tmp_path, transcript)
    assert proc.returncode == 0, proc.stderr
    hso = data["hookSpecificOutput"]
    assert set(hso) == {"hookEventName", "additionalContext"} and hso["hookEventName"] == "UserPromptSubmit"
    text = hso["additionalContext"]
    for needle in ("opus", "sonnet", "cache", "effort", "subagente", "regra 17"):
        assert needle in text, needle
    assert len(text) < 700, len(text)
    _silent(_prompt(tmp_path, transcript))  # the same switch is never announced twice


def test_a_new_switch_is_a_new_advice(tmp_path: Path) -> None:
    lines = _switch_lines()
    _prompt(tmp_path, _transcript(tmp_path, lines))
    lines.append(_assistant("claude-haiku-5-5", "u4", NOW - timedelta(minutes=1)))
    text = _context(_prompt(tmp_path, _transcript(tmp_path, lines))[1])
    assert "sonnet" in text and "haiku" in text


def test_one_family_means_no_switch(tmp_path: Path) -> None:
    lines = [_assistant("claude-opus-5-5", f"u{i}", NOW - timedelta(minutes=10 - i)) for i in range(5)]
    _silent(_prompt(tmp_path, _transcript(tmp_path, lines)))
    assert not _state_dir(tmp_path).exists() or _state(tmp_path).get("switches", []) == []


def test_two_versions_of_one_family_are_not_a_switch(tmp_path: Path) -> None:
    lines = [_assistant("claude-opus-5-5", "u1", NOW - timedelta(minutes=5)),
             _assistant("claude-opus-5-6", "u2", NOW - timedelta(minutes=1))]
    _silent(_prompt(tmp_path, _transcript(tmp_path, lines)))


def test_sidechain_and_synthetic_turns_and_quoted_models_are_not_the_session_model(tmp_path: Path) -> None:
    lines = [_assistant("claude-opus-5-5", "u1", NOW - timedelta(minutes=5)),
             _assistant("claude-haiku-5-5", "u2", NOW - timedelta(minutes=4), sidechain=True),
             _assistant("<synthetic>", "u3", NOW - timedelta(minutes=3)),
             _user("u4", NOW - timedelta(minutes=2)),
             _assistant("claude-opus-5-5", "u5", NOW - timedelta(minutes=1))]
    _silent(_prompt(tmp_path, _transcript(tmp_path, lines)))


def test_a_switch_older_than_thirty_minutes_is_not_news(tmp_path: Path) -> None:
    _silent(_prompt(tmp_path, _transcript(tmp_path, _switch_lines(age=timedelta(minutes=45)))))


def test_only_the_last_two_hundred_lines_are_read(tmp_path: Path) -> None:
    filler = [_user(f"f{i}", NOW - timedelta(minutes=1)) for i in range(250)]
    # opus at the very start, then 250 user lines, then sonnet turns only: the opus turn is outside the tail
    lines = [_assistant("claude-opus-5-5", "u1", NOW - timedelta(minutes=9)), *filler,
             _assistant("claude-sonnet-5-5", "u2", NOW - timedelta(minutes=1))]
    _silent(_prompt(tmp_path, _transcript(tmp_path, lines)))


def test_a_huge_transcript_is_read_by_its_tail_only(tmp_path: Path) -> None:
    path = _transcript(tmp_path, [_assistant("claude-opus-5-5", "u0", NOW - timedelta(minutes=9))])
    with path.open("a") as fh:
        fh.write(("x" * 4000 + "\n") * 600)  # ~2.4 MB of filler after the opus turn
        for line in _switch_lines(first="claude-haiku-5-5", second="claude-sonnet-5-5")[1:]:
            fh.write(json.dumps(line, separators=(",", ":")) + "\n")
    _silent(_prompt(tmp_path, path))  # haiku was never seen: only sonnet in the tail


@pytest.mark.parametrize("path", ["ausente.jsonl", ""])
def test_a_missing_transcript_is_silent(tmp_path: Path, path: str) -> None:
    _silent(_prompt(tmp_path, tmp_path / path if path else Path("")))


def test_the_switch_advice_skips_subagents_sisters_and_the_kill_switch(tmp_path: Path) -> None:
    transcript = _transcript(tmp_path, _switch_lines())
    _silent(_prompt(tmp_path, transcript, extra={"agent_id": "a1"}))
    _silent(_prompt(tmp_path, transcript, env_extra={"ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/"}))
    _silent(_prompt(tmp_path, transcript, env_extra={"DELEGATION_NUDGE_DISABLED": "1"}))
    assert "troca" in _context(_prompt(tmp_path, transcript)[1]).lower()


def test_the_switch_advice_is_logged(tmp_path: Path) -> None:
    _prompt(tmp_path, _transcript(tmp_path, _switch_lines()))
    (record,) = _log(tmp_path)
    assert (record["evento"], record["session"], record["alvos"]) == ("troca-de-modelo", "sess-a",
                                                                       ["opus→sonnet"])


def test_a_read_burst_and_a_switch_do_not_share_cooldowns(tmp_path: Path) -> None:
    _fill(tmp_path)
    assert "leituras" in _context(_read(tmp_path, "f.rs")[1])
    assert "troca" in _context(_prompt(tmp_path, _transcript(tmp_path, _switch_lines()))[1]).lower()
    assert [r["evento"] for r in _log(tmp_path)] == ["leitura", "troca-de-modelo"]


# --- one source for the family classifier and the doc -----------------------------------------------

def test_the_family_classifier_is_the_gates_and_the_providers() -> None:
    nudge, gate = _load(HOOK, "delegation_nudge_t"), _load(CLIENT_HOOKS / "agent_model_gate.py", "gate_t")
    for model in ("claude-opus-5-5", "Sonnet", " HAIKU ", "claude-fable-5-1", "inherit", "<synthetic>", None, 5):
        assert nudge.model_family(model) == gate.model_family(model)


def test_the_thresholds_are_the_ones_the_doc_declares() -> None:
    nudge = _load(HOOK, "delegation_nudge_d")
    doc = CANONICAL_DOC.read_text(encoding="utf-8")
    section = doc[doc.index("**Indução de delegação"):]
    section = section[:section.index("\n\n**", 5)] if "\n\n**" in section[5:] else section
    assert f"≥ {nudge.MAX_CALLS} chamadas" in section and f"{nudge.MAX_KB} KB" in section
    assert f"{nudge.WINDOW_S // 60} min" in section


# --- registration: the template the installer applies ------------------------------------------------

def _nudge_registrations() -> list[tuple[str, str | None, int | None]]:
    """(event, matcher, timeout) of every template entry that runs the nudge."""
    hooks = json.loads((REPO_ROOT / "client" / "claude" / "hooks.settings.json").read_text(encoding="utf-8"))
    return [(event, group.get("matcher"), hook.get("timeout")) for event in ("PreToolUse", "UserPromptSubmit")
            for group in hooks[event] for hook in group["hooks"]
            if hook["command"] == "python3 $HOME/.claude/hooks/delegation_nudge.py"]


def test_the_template_registers_the_nudge_for_reads_and_for_the_prompt() -> None:
    assert sorted(_nudge_registrations(), key=str) == [("PreToolUse", "Read|Grep|Glob", 5), ("UserPromptSubmit", None, 5)]
    nudge = _load(HOOK, "delegation_nudge_reg")
    assert {"Read", "Grep", "Glob"} == set(nudge.READ_TOOLS)

