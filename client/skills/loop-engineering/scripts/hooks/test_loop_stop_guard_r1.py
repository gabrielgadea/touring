"""The burst block names the R1 skeleton by its REAL path, never by a placeholder (V0e3, approved by Gabriel 08/10/2026).

Injection-density invariant: a derivable value travels as the value. The block used to say
`--file <r1_varredura_agregado.py>` while the skeleton exists on disk; when it does not exist, the message must still
carry no `<…>` placeholder — only the memory query, which is a real command.
"""

import json
import re

import loop_stop_guard as guard


def _burst_transcript(tmp_path):
    """A turn opened by a genuine user message followed by BURST_BASH_FLOOR Bash calls and no `touring run`."""
    path = tmp_path / "t.jsonl"
    calls = [{"type": "tool_use", "id": f"b{i}", "name": "Bash", "input": {"command": f"ls /x{i}"}}
             for i in range(guard.BURST_BASH_FLOOR)]
    rows = [{"type": "user", "message": {"role": "user", "content": "faça a varredura"}},
            {"type": "assistant", "message": {"role": "assistant", "content": calls}}]
    path.write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
    return path


def test_burst_block_names_the_real_r1_path(tmp_path, monkeypatch):
    skeleton = tmp_path / "r1_varredura_agregado.py"
    skeleton.write_text("# R1\n", encoding="utf-8")
    monkeypatch.setattr(guard, "R1_SKELETON", skeleton)
    monkeypatch.delenv("TOURING_WORK_OUTER_DISABLED", raising=False)
    reason = guard.code_mode_burst_block({"transcript_path": str(_burst_transcript(tmp_path))})
    assert reason and str(skeleton) in reason
    assert not re.search(r"<[^>]*r1_varredura[^>]*>", reason)


def test_burst_block_without_skeleton_carries_no_placeholder(tmp_path, monkeypatch):
    monkeypatch.setattr(guard, "R1_SKELETON", tmp_path / "ausente.py")
    monkeypatch.delenv("TOURING_WORK_OUTER_DISABLED", raising=False)
    reason = guard.code_mode_burst_block({"transcript_path": str(_burst_transcript(tmp_path))})
    assert reason and "touring memory query" in reason
    assert not re.search(r"<[^>]+\.py>", reason)
