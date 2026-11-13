"""Tests for subagent_output_contract.py — the long deliverable goes to a file, the final message stays short."""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

HOOKS = Path(__file__).resolve().parents[1]
SCRIPT = HOOKS / "subagent_output_contract.py"
STRIPPED = {"SUBAGENT_OUTPUT_CONTRACT_DISABLED", "SUBAGENT_OUTPUT_CONTRACT_MODE", "SUBAGENT_OUTPUT_MAX_CHARS",
            "TOURING_HOOK_INJECTION", "CLAUDE_PROJECT_DIR"}


def _run(payload: dict | bytes, tmp_path: Path, **env: str) -> tuple[subprocess.CompletedProcess, dict]:
    clean = {k: v for k, v in os.environ.items() if k not in STRIPPED}
    clean.update(HOME=str(tmp_path), TOURING_INJECTION_DEDUP_DISABLED="1", **env)
    data = payload if isinstance(payload, bytes) else json.dumps(payload).encode()
    proc = subprocess.run([sys.executable, str(SCRIPT)], input=data, capture_output=True, env=clean, timeout=20,
                          check=False)
    return proc, (json.loads(proc.stdout) if proc.stdout.strip() else {})


def _stop(tmp_path: Path, message: str, **extra: object) -> dict:
    return {"hook_event_name": "SubagentStop", "session_id": "s1", "agent_id": "a1b2c3d4e5", "agent_type": "touring-architect",
            "cwd": str(tmp_path / "proj"), "stop_hook_active": False, "last_assistant_message": message, **extra}


def _log(tmp_path: Path) -> list[dict]:
    path = tmp_path / ".claude" / "touring" / "subagent_output_contract.jsonl"
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


# ---- SubagentStop: the executor --------------------------------------------------------------------------

def test_a_long_final_message_is_blocked_once_and_taught_the_file_and_the_short_reply(tmp_path: Path) -> None:
    proc, out = _run(_stop(tmp_path, "x" * 9000), tmp_path)
    assert proc.returncode == 0 and out["decision"] == "block"
    reason = out["reason"]
    target = tmp_path / "claude-artefatos" / "proj" / "subagentes"
    assert str(target) in reason and "touring-architect-a1b2c3d4" in reason
    for needle in ("9000", "8000", "INTEGRAL", "report_path", "resumo"):
        assert needle in reason, needle
    assert target.is_dir(), "the hook creates the directory it tells the subagent to write into"
    assert _log(tmp_path)[-1]["verdict"] == "block"


def test_a_short_final_message_passes_in_silence_and_is_measured(tmp_path: Path) -> None:
    proc, out = _run(_stop(tmp_path, "y" * 8000), tmp_path)
    assert proc.returncode == 0 and out == {}
    record = _log(tmp_path)[-1]
    assert record["verdict"] == "ok" and record["chars"] == 8000 and "last_assistant_message" not in record


def test_the_second_stop_is_never_blocked_so_the_subagent_cannot_loop(tmp_path: Path) -> None:
    proc, out = _run(_stop(tmp_path, "x" * 50_000, stop_hook_active=True), tmp_path)
    assert proc.returncode == 0 and out == {}
    assert _log(tmp_path)[-1]["verdict"] == "long-after-block"


def test_an_agent_that_cannot_write_is_told_to_shorten_instead(tmp_path: Path) -> None:
    _, out = _run(_stop(tmp_path, "x" * 9000, agent_type="Explore"), tmp_path)
    assert "sem ferramenta de escrita" in out["reason"]


def test_internal_agents_with_an_empty_type_are_out_of_scope(tmp_path: Path) -> None:
    proc, out = _run(_stop(tmp_path, "x" * 9000, agent_type=""), tmp_path)
    assert proc.returncode == 0 and out == {} and _log(tmp_path) == []


def test_shadow_mode_measures_and_never_blocks(tmp_path: Path) -> None:
    _, out = _run(_stop(tmp_path, "x" * 9000), tmp_path, SUBAGENT_OUTPUT_CONTRACT_MODE="shadow")
    assert out == {} and _log(tmp_path)[-1]["verdict"] == "shadow-would-block"


def test_the_human_kill_switch_silences_both_events(tmp_path: Path) -> None:
    for payload in (_stop(tmp_path, "x" * 9000), {"hook_event_name": "SubagentStart", "agent_type": "Explore", "agent_id": "a"}):
        proc, out = _run(payload, tmp_path, SUBAGENT_OUTPUT_CONTRACT_DISABLED="1")
        assert proc.returncode == 0 and out == {}


def test_the_limit_is_configurable_and_a_bad_value_falls_back_to_the_default(tmp_path: Path) -> None:
    _, out = _run(_stop(tmp_path, "x" * 3000), tmp_path, SUBAGENT_OUTPUT_MAX_CHARS="2000")
    assert out["decision"] == "block" and "2000" in out["reason"]
    _, out = _run(_stop(tmp_path, "x" * 3000), tmp_path, SUBAGENT_OUTPUT_MAX_CHARS="abc")
    assert out == {}


@pytest.mark.parametrize("raw", [b"\xff not json", b"[]", b'{"hook_event_name":"SubagentStop","agent_type":"x","last_assistant_message":17}'])
def test_garbage_never_blocks_a_subagent(tmp_path: Path, raw: bytes) -> None:
    proc, out = _run(raw, tmp_path)
    assert proc.returncode == 0 and out == {}


def test_the_project_directory_names_the_artifact_folder_even_from_a_subdirectory(tmp_path: Path) -> None:
    project = tmp_path / "meu-projeto"
    _, out = _run(_stop(tmp_path, "x" * 9000, cwd=str(project / "crates" / "x")), tmp_path, CLAUDE_PROJECT_DIR=str(project))
    assert str(tmp_path / "claude-artefatos" / "meu-projeto" / "subagentes") in out["reason"]


# ---- SubagentStart: the announcement (same numbers, same folder) -----------------------------------------

def test_every_subagent_is_told_the_contract_at_start_with_the_same_limit_and_folder(tmp_path: Path) -> None:
    payload = {"hook_event_name": "SubagentStart", "session_id": "s1", "agent_id": "a1b2c3d4e5", "agent_type": "general-purpose",
               "cwd": str(tmp_path / "proj")}
    proc, out = _run(payload, tmp_path)
    assert proc.returncode == 0
    hso = out["hookSpecificOutput"]
    assert hso["hookEventName"] == "SubagentStart"
    text = hso["additionalContext"]
    for needle in ("8000", str(tmp_path / "claude-artefatos" / "proj" / "subagentes"), "general-purpose-a1b2c3d4",
                   "report_path", "INTEGRAL"):
        assert needle in text, needle


def test_the_announcement_obeys_the_injection_switch_but_the_block_does_not(tmp_path: Path) -> None:
    start = {"hook_event_name": "SubagentStart", "agent_id": "a", "agent_type": "Explore", "cwd": str(tmp_path)}
    _, out = _run(start, tmp_path, TOURING_HOOK_INJECTION="off")
    assert out == {}
    _, out = _run(_stop(tmp_path, "x" * 9000), tmp_path, TOURING_HOOK_INJECTION="off")
    assert out["decision"] == "block", "enforcement is not context injection: the switch must not turn it off"
