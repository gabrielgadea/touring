"""Tests for subagent_output_contract.py — every delivery is kept whole: on disk, in memory, findable by the parent."""
from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

HOOKS = Path(__file__).resolve().parents[1]
SCRIPT = HOOKS / "subagent_output_contract.py"
STRIPPED = {"SUBAGENT_OUTPUT_CONTRACT_DISABLED", "SUBAGENT_OUTPUT_CONTRACT_MODE", "SUBAGENT_OUTPUT_MAX_CHARS",
            "SUBAGENT_OUTPUT_MEMORY_DISABLED", "TOURING_HOOK_INJECTION", "CLAUDE_PROJECT_DIR", "TOURING_BIN"}
RICH = "# Gramática\n" + "\n".join(f"regra-{n} ::= prefixo '@' versão '!' caminho  # nível {n} — ação, não só nome" for n in range(120))


def _fake_touring(tmp_path: Path, rc: int = 0) -> Path:
    """A stand-in for the `touring` binary: records its argv as one JSON line, exits `rc`."""
    script = tmp_path / "touring-falso"
    script.write_text(f"#!{sys.executable}\nimport json,sys\nopen({str(tmp_path / 'memoria.jsonl')!r},'a').write(json.dumps(sys.argv[1:])+'\\n')\n"
                      f"sys.exit({rc})\n")
    script.chmod(0o755)
    return script


def _run(payload: dict | bytes, tmp_path: Path, **env: str) -> tuple[subprocess.CompletedProcess, dict]:
    clean = {k: v for k, v in os.environ.items() if k not in STRIPPED}
    clean.update(HOME=str(tmp_path), TOURING_INJECTION_DEDUP_DISABLED="1", TOURING_BIN=str(_fake_touring(tmp_path)), **env)
    data = payload if isinstance(payload, bytes) else json.dumps(payload).encode()
    proc = subprocess.run([sys.executable, str(SCRIPT)], input=data, capture_output=True, env=clean, timeout=20,
                          check=False)
    return proc, (json.loads(proc.stdout) if proc.stdout.strip() else {})


def _stop(tmp_path: Path, message: object, **extra: object) -> dict:
    return {"hook_event_name": "SubagentStop", "session_id": "s1", "agent_id": "a1b2c3d4e5f6", "agent_type": "touring-architect",
            "cwd": str(tmp_path / "proj"), "stop_hook_active": False, "last_assistant_message": message, **extra}


def _file(tmp_path: Path, name: str = "touring-architect-a1b2c3d4e5f6.md", project: str = "proj") -> Path:
    return tmp_path / "claude-artefatos" / project / "subagentes" / name


def _log(tmp_path: Path) -> list[dict]:
    path = tmp_path / ".claude" / "touring" / "subagent_output_contract.jsonl"
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def _memory(tmp_path: Path) -> list[list[str]]:
    path = tmp_path / "memoria.jsonl"
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


# ---- fidelity and persistence: the executor keeps the whole text ------------------------------------------

def test_the_delivery_is_archived_whole_with_its_hash_and_never_blocked(tmp_path: Path) -> None:
    proc, out = _run(_stop(tmp_path, RICH), tmp_path)
    assert proc.returncode == 0 and out == {}, "a long, rich delivery is what was asked for: it passes"
    text = _file(tmp_path).read_text()
    header, body = text.split("\n", 1)
    digest = hashlib.sha256(RICH.encode()).hexdigest()
    assert body == RICH + "\n", "byte for byte: nothing summarized, nothing cut"
    for needle in ("agent_type=touring-architect", "agent_id=a1b2c3d4e5f6", "session_id=s1", f"chars={len(RICH)}", f"sha256={digest}"):
        assert needle in header, needle
    record = _log(tmp_path)[-1]
    assert record["verdict"] == "ok" and record["sha256"] == digest and record["path"] == str(_file(tmp_path))
    assert "last_assistant_message" not in record and RICH[:40] not in json.dumps(record)


# ids are explicit: pytest puts the default id (the whole value) in PYTEST_CURRENT_TEST, and a 120 KB env var
# makes the hook's own spawn fail with E2BIG.
@pytest.mark.parametrize("message", [
    pytest.param("PRONTO", id="uma-linha"), pytest.param("ç" * 30_000, id="30k-acentuados"),
    pytest.param("linha\n" * 20_000, id="20k-linhas"), pytest.param("aspas \" e \\ barra e \u2028 separador", id="especiais"),
])
def test_every_size_and_every_character_survives_the_archive(tmp_path: Path, message: str) -> None:
    _run(_stop(tmp_path, message), tmp_path)
    assert _file(tmp_path).read_text().split("\n", 1)[1] == message + "\n"


def test_a_second_delivery_of_the_same_agent_is_appended_never_overwritten(tmp_path: Path) -> None:
    _run(_stop(tmp_path, "primeira entrega " * 80), tmp_path)
    _run(_stop(tmp_path, "segunda entrega " * 80), tmp_path)
    text = _file(tmp_path).read_text()
    assert text.count("<!-- entrega-subagente") == 2
    assert text.index("primeira entrega") < text.index("segunda entrega")


def test_two_agents_of_the_same_type_never_share_a_file(tmp_path: Path) -> None:
    _run(_stop(tmp_path, "do primeiro", agent_id="aopus-1-24c414fd5b336e4a", agent_type="opus-1"), tmp_path)
    _run(_stop(tmp_path, "do segundo", agent_id="aopus-1-3991fd746084e4ff", agent_type="opus-1"), tmp_path)
    assert "do primeiro" in _file(tmp_path, "opus-1-aopus-1-24c414fd5b336e4a.md").read_text()
    assert "do segundo" in _file(tmp_path, "opus-1-aopus-1-3991fd746084e4ff.md").read_text()


def test_the_project_names_the_folder_even_from_a_subdirectory(tmp_path: Path) -> None:
    project = tmp_path / "meu-projeto"
    _run(_stop(tmp_path, "x", cwd=str(project / "crates" / "x")), tmp_path, CLAUDE_PROJECT_DIR=str(project))
    assert _file(tmp_path, project="meu-projeto").is_file()


def test_a_hostile_agent_type_cannot_leave_the_deliveries_folder(tmp_path: Path) -> None:
    _run(_stop(tmp_path, "x", agent_type="../../fora", agent_id="../id"), tmp_path)
    folder = tmp_path / "claude-artefatos" / "proj" / "subagentes"
    assert [p.parent for p in (tmp_path / "claude-artefatos").rglob("*.md")] == [folder]


def test_an_archive_failure_is_named_and_never_traps_the_subagent(tmp_path: Path) -> None:
    (tmp_path / "claude-artefatos").write_text("um arquivo onde a pasta deveria nascer")
    proc, out = _run(_stop(tmp_path, RICH), tmp_path)
    assert proc.returncode == 0 and out == {} and b"N\xc3\x83O arquivada" in proc.stderr
    assert "archive_error" in _log(tmp_path)[-1]


# ---- memory: a later session can find the delivery --------------------------------------------------------

def test_a_substantial_delivery_is_indexed_in_memory_with_path_hash_and_opening(tmp_path: Path) -> None:
    _run(_stop(tmp_path, RICH), tmp_path)
    (argv,) = _memory(tmp_path)
    assert argv[:3] == ["memory", "store", "entrega-subagente:touring-architect-a1b2c3d4e5f6"]
    value = argv[3]
    assert str(_file(tmp_path)) in value and hashlib.sha256(RICH.encode()).hexdigest()[:16] in value and "Gramática" in value
    assert "#kind:deliverable" in argv and "#artifact:report" in argv
    assert _log(tmp_path)[-1]["memory"] == "stored"


def test_a_one_line_delivery_is_archived_but_not_indexed(tmp_path: Path) -> None:
    _run(_stop(tmp_path, "PRONTO"), tmp_path)
    assert _file(tmp_path).is_file() and _memory(tmp_path) == [] and _log(tmp_path)[-1]["memory"] == "skipped"


def test_a_memory_failure_is_recorded_and_the_archive_stands(tmp_path: Path) -> None:
    clean = {k: v for k, v in os.environ.items() if k not in STRIPPED}
    clean.update(HOME=str(tmp_path), TOURING_BIN=str(_fake_touring(tmp_path, rc=3)))
    subprocess.run([sys.executable, str(SCRIPT)], input=json.dumps(_stop(tmp_path, RICH)).encode(), env=clean, check=True, timeout=20)
    assert _file(tmp_path).is_file() and _log(tmp_path)[-1]["memory"] == "failed:rc3"
    clean["TOURING_BIN"] = str(tmp_path / "nao-existe")
    subprocess.run([sys.executable, str(SCRIPT)], input=json.dumps(_stop(tmp_path, RICH)).encode(), env=clean, check=True, timeout=20)
    assert _log(tmp_path)[-1]["memory"].startswith("failed:")


# ---- richness: what the subagent is told at start ---------------------------------------------------------

def test_the_subagent_is_told_to_deliver_whole_and_where_it_is_kept(tmp_path: Path) -> None:
    payload = {"hook_event_name": "SubagentStart", "session_id": "s1", "agent_id": "a1b2c3d4e5f6", "agent_type": "touring-architect",
               "cwd": str(tmp_path / "proj")}
    _, out = _run(payload, tmp_path)
    hso = out["hookSpecificOutput"]
    assert hso["hookEventName"] == "SubagentStart"
    text = hso["additionalContext"]
    for needle in ("FIDELIDADE", "RIQUEZA", "COMPLETO", str(_file(tmp_path)), "touring memory recall", "touring-architect-a1b2c3d4e5f6-<assunto>.md"):
        assert needle in text, needle
    assert "teto" not in text and "8000" not in text, "the contract is about content, not about a ceiling"


def test_a_read_only_agent_is_told_to_put_everything_in_the_message(tmp_path: Path) -> None:
    payload = {"hook_event_name": "SubagentStart", "agent_id": "a1", "agent_type": "Explore", "cwd": str(tmp_path / "proj")}
    text = _run(payload, tmp_path)[1]["hookSpecificOutput"]["additionalContext"]
    assert "sem ferramenta de escrita" in text and "<assunto>" not in text


# ---- the parent: where the whole text of the agent it launched will be -----------------------------------

@pytest.mark.parametrize("response", [
    {"agentId": "af71b59db0d91199e", "status": "async_launched"},
    {"content": [{"type": "text", "text": "Async agent launched successfully.\nagentId: af71b59db0d91199e (internal ID)"}]},
    "Spawned successfully.\nagent_id: af71b59db0d91199e\nname: opus-1",
])
def test_the_parent_is_told_where_the_whole_delivery_will_be(tmp_path: Path, response: object) -> None:
    payload = {"hook_event_name": "PostToolUse", "tool_name": "Agent", "cwd": str(tmp_path / "proj"),
               "tool_input": {"subagent_type": "general-purpose"}, "tool_response": response}
    hso = _run(payload, tmp_path)[1]["hookSpecificOutput"]
    assert hso["hookEventName"] == "PostToolUse"
    assert f"{tmp_path}/claude-artefatos/proj/subagentes/*-af71b59db0d91199e.md" in hso["additionalContext"]


def test_the_path_the_parent_is_told_matches_the_file_the_stop_writes(tmp_path: Path) -> None:
    """D8: the announcement to the parent and the archive are one naming rule."""
    launched = {"hook_event_name": "PostToolUse", "tool_name": "Agent", "cwd": str(tmp_path / "proj"),
                "tool_response": {"agentId": "aopus-1-24c414fd5b336e4a"}}
    told = _run(launched, tmp_path)[1]["hookSpecificOutput"]["additionalContext"]
    _run(_stop(tmp_path, "x", agent_id="aopus-1-24c414fd5b336e4a", agent_type="opus-1"), tmp_path)
    pattern = next(word for word in told.split() if "*-" in word)
    assert [str(p) for p in Path(pattern).parent.glob(Path(pattern).name)] == [str(_file(tmp_path, "opus-1-aopus-1-24c414fd5b336e4a.md"))]


def test_nothing_is_said_to_the_parent_without_an_agent_id_or_for_another_tool(tmp_path: Path) -> None:
    base = {"hook_event_name": "PostToolUse", "cwd": str(tmp_path)}
    assert _run({**base, "tool_name": "Agent", "tool_response": {"error": "denied"}}, tmp_path)[1] == {}
    assert _run({**base, "tool_name": "Bash", "tool_response": {"agentId": "af71b59db0d91199e"}}, tmp_path)[1] == {}


# ---- scope and switches -----------------------------------------------------------------------------------

def test_internal_agents_with_an_empty_type_are_out_of_scope(tmp_path: Path) -> None:
    proc, out = _run(_stop(tmp_path, RICH, agent_type=""), tmp_path)
    assert proc.returncode == 0 and out == {} and _log(tmp_path) == [] and not (tmp_path / "claude-artefatos").exists()


def test_the_human_kill_switch_silences_every_event(tmp_path: Path) -> None:
    for payload in (_stop(tmp_path, RICH), {"hook_event_name": "SubagentStart", "agent_type": "Explore", "agent_id": "a"}):
        proc, out = _run(payload, tmp_path, SUBAGENT_OUTPUT_CONTRACT_DISABLED="1")
        assert proc.returncode == 0 and out == {}
    assert not (tmp_path / "claude-artefatos").exists()


def test_the_injection_switch_cuts_the_announcements_and_never_the_archive(tmp_path: Path) -> None:
    start = {"hook_event_name": "SubagentStart", "agent_id": "a", "agent_type": "Explore", "cwd": str(tmp_path)}
    assert _run(start, tmp_path, TOURING_HOOK_INJECTION="off")[1] == {}
    _run(_stop(tmp_path, RICH), tmp_path, TOURING_HOOK_INJECTION="off")
    assert _file(tmp_path).is_file(), "archiving is not context injection: the switch must not turn it off"


@pytest.mark.parametrize("raw", [b"\xff not json", b"[]", b'{"hook_event_name":"SubagentStop","agent_type":"x","last_assistant_message":17}',
                                 b'{"hook_event_name":"SubagentStop","agent_type":"x","last_assistant_message":"   "}'])
def test_garbage_and_empty_deliveries_never_block_and_archive_nothing(tmp_path: Path, raw: bytes) -> None:
    proc, out = _run(raw, tmp_path)
    assert proc.returncode == 0 and out == {} and not (tmp_path / "claude-artefatos").exists()


# ---- the ceiling is an opt-in, and even then nothing is lost ----------------------------------------------

def test_the_opt_in_ceiling_sends_a_long_reply_back_once_after_archiving_it(tmp_path: Path) -> None:
    _, out = _run(_stop(tmp_path, RICH), tmp_path, SUBAGENT_OUTPUT_CONTRACT_MODE="block")
    assert out["decision"] == "block" and str(_file(tmp_path)) in out["reason"] and "já gravado" in out["reason"]
    assert _file(tmp_path).read_text().split("\n", 1)[1] == RICH + "\n"
    _, out = _run(_stop(tmp_path, RICH, stop_hook_active=True), tmp_path, SUBAGENT_OUTPUT_CONTRACT_MODE="block")
    assert out == {} and _log(tmp_path)[-1]["verdict"] == "long-after-block"


def test_the_opt_in_ceiling_is_configurable_and_a_bad_value_falls_back(tmp_path: Path) -> None:
    short = "y" * 3000
    assert _run(_stop(tmp_path, short), tmp_path, SUBAGENT_OUTPUT_CONTRACT_MODE="block", SUBAGENT_OUTPUT_MAX_CHARS="2000")[1]["decision"] == "block"
    assert _run(_stop(tmp_path, short), tmp_path, SUBAGENT_OUTPUT_CONTRACT_MODE="block", SUBAGENT_OUTPUT_MAX_CHARS="abc")[1] == {}
