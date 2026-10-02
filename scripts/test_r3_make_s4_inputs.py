"""S-12 — the S4 inputs are REAL Bash commands from real transcripts, deterministic and bounded."""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_make_s4_inputs as mk  # noqa: E402


def transcript(path: Path, session: str, commands, ts="2026-09-10T10:00:00Z"):
    lines = [json.dumps({"timestamp": ts, "message": {"content": [{"type": "tool_use", "name": "Bash", "input": {"command": c}}]}})
             for c in commands]
    lines.append(json.dumps({"timestamp": ts, "message": {"content": [{"type": "tool_use", "name": "Read", "input": {"file_path": "/x"}}]}}))
    (path / f"{session}.jsonl").write_text("\n".join(lines) + "\n")


def test_payload_has_the_shape_of_a_pretooluse_bash_hook_input(tmp_path):
    transcript(tmp_path, "abcd1234-0000", ["ls -la"])
    [p] = mk.build_inputs(tmp_path, n=5, seed=1)
    assert p["hook_event_name"] == "PreToolUse" and p["tool_name"] == "Bash" and p["tool_input"] == {"command": "ls -la"}
    assert p["session_id"] and "cwd" in p and "transcript_path" in p


def test_only_bash_commands_dedup_and_bounded_length(tmp_path):
    transcript(tmp_path, "s1", ["echo a", "echo a", "echo b", "x" * 5000])
    cmds = [p["tool_input"]["command"] for p in mk.build_inputs(tmp_path, n=10, seed=1)]
    assert sorted(cmds) == ["echo a", "echo b"]          # Read ignored, duplicate dropped, the 5000-char command refused


def test_selection_is_deterministic_under_the_seed_and_capped_at_n(tmp_path):
    for s in range(4):
        transcript(tmp_path, f"sess{s}", [f"cmd-{s}-{i}" for i in range(30)])
    a = mk.build_inputs(tmp_path, n=50, seed=20261002)
    assert a == mk.build_inputs(tmp_path, n=50, seed=20261002) and len(a) == 50
    assert a != mk.build_inputs(tmp_path, n=50, seed=7)
    assert len({p["session_id"] for p in a}) > 1
    # round-robin, not session-by-session: 4 draws from 4 sessions of 30 commands each must be ONE from EACH
    assert len({p["session_id"] for p in mk.build_inputs(tmp_path, n=4, seed=1)}) == 4


def test_session_id_is_a_hash_not_the_raw_transcript_name(tmp_path):
    transcript(tmp_path, "00fc1e5d-secret-name", ["ls"])
    [p] = mk.build_inputs(tmp_path, n=1, seed=1)
    assert "secret-name" not in p["session_id"] and "secret-name" not in p["transcript_path"]
