"""Contract of the post-compaction recorder + gap judge (08/10/2026, item 4 of the compaction plan).

PostCompact records one line per compaction (metrics + what the summary kept); SessionStart with
source=compact injects ONLY the deterministic gaps — files changed, DAGs cited, tests run in the
compacted segment that the summary does not mention. Silent when nothing is missing.
"""
from __future__ import annotations

import io
import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import compaction_ledger as cl  # noqa: E402
import loop_resume  # noqa: E402

TASK = "task_1791479126715571926"


def _rec(kind: str, **kw) -> dict:
    if kind == "edit":
        return {"type": "assistant", "message": {"content": [
            {"type": "tool_use", "id": kw["id"], "name": kw.get("tool", "Edit"), "input": {"file_path": kw["path"]}}]}}
    if kind == "bash":
        return {"type": "assistant", "message": {"content": [
            {"type": "tool_use", "id": kw["id"], "name": "Bash", "input": {"command": kw["cmd"]}}]}}
    if kind == "user":
        return {"type": "user", "message": {"content": kw["text"]}}
    if kind == "boundary":
        return {"type": "system", "subtype": "compact_boundary", "uuid": kw["uuid"],
                "compactMetadata": {"trigger": "auto", "preTokens": 650000, "postTokens": 40000, "durationMs": 90000}}
    if kind == "summary":
        return {"type": "user", "isCompactSummary": True, "message": {"content": kw["text"]}}
    raise ValueError(kind)


def _transcript(tmp_path: Path, summary: str | None, fresh_boundary: bool = True) -> Path:
    recs = [
        _rec("user", text="trabalho antigo em old.py"),
        _rec("edit", id="t0", path="/repo/old.py"),
        _rec("boundary", uuid="b-old"),
        _rec("summary", text="resumo antigo cita old.py"),
        _rec("user", text=f"implemente o item 4 do DAG {TASK}"),
        _rec("edit", id="t1", path="/repo/hooks/a.py"),
        _rec("edit", id="t2", path="/repo/docs/b.md", tool="Write"),
        _rec("bash", id="t3", cmd="cd /repo && python3 -m pytest tests/test_a.py -q"),
    ]
    if fresh_boundary:
        recs.append(_rec("boundary", uuid="b-new"))
        if summary is not None:
            recs.append(_rec("summary", text=summary))
    p = tmp_path / "s.jsonl"
    p.write_text("\n".join(json.dumps(r) for r in recs) + "\n")
    return p


@pytest.fixture(autouse=True)
def _ledger_home(tmp_path, monkeypatch):
    monkeypatch.setenv("TOURING_COMPACTION_LEDGER_DIR", str(tmp_path / "ledger"))


def test_facts_come_from_the_last_segment_only(tmp_path):
    facts = cl.segment_facts(_transcript(tmp_path, "x"))
    assert facts["files"] == ["/repo/hooks/a.py", "/repo/docs/b.md"], "old.py belongs to the previous cycle"
    assert facts["dags"] == [TASK]
    assert facts["tests"] == ["cd /repo && python3 -m pytest tests/test_a.py -q"]
    assert facts["boundary"]["uuid"] == "b-new" and facts["boundary"]["preTokens"] == 650000


def test_segment_when_postcompact_fires_before_the_boundary_is_written(tmp_path):
    """Order is not guaranteed: without the fresh boundary, the open tail IS the compacted segment."""
    facts = cl.segment_facts(_transcript(tmp_path, None, fresh_boundary=False))
    assert facts["files"] == ["/repo/hooks/a.py", "/repo/docs/b.md"]
    assert facts["boundary"] is None


def test_judge_names_only_what_the_summary_dropped(tmp_path):
    facts = cl.segment_facts(_transcript(tmp_path, "x"))
    gaps = cl.judge(f"Editei hooks/a.py no DAG {TASK}; rodei pytest tests/test_a.py", facts)
    assert gaps == {"files": ["/repo/docs/b.md"], "dags": [], "tests": []}


def test_record_appends_once_per_boundary_and_writes_the_gaps(tmp_path):
    t = _transcript(tmp_path, "resumo sem nada")
    payload = {"hook_event_name": "PostCompact", "session_id": "s1", "transcript_path": str(t),
               "trigger": "auto", "compact_summary": "resumo sem nada", "cwd": "/repo"}
    cl.record(payload)
    cl.record(payload)
    lines = (tmp_path / "ledger" / "compactions.jsonl").read_text().splitlines()
    assert len(lines) == 1, "idempotent per boundary"
    row = json.loads(lines[0])
    assert row["pre_tokens"] == 650000 and row["summary_chars"] == len("resumo sem nada")
    assert row["coverage"] == {"files": [0, 2], "dags": [0, 1], "tests": [0, 1]}
    assert json.loads((tmp_path / "ledger" / "compact_gaps" / "s1.json").read_text())["files"]


def test_gaps_context_is_silent_when_nothing_is_missing(tmp_path):
    full = f"a.py b.md {TASK} tests/test_a.py"
    t = _transcript(tmp_path, full)
    assert cl.gaps_context({"session_id": "s2", "transcript_path": str(t), "source": "compact"}) is None


def test_gaps_context_names_the_missing_file_from_the_transcript(tmp_path):
    t = _transcript(tmp_path, f"a.py {TASK} tests/test_a.py")
    ctx = cl.gaps_context({"session_id": "s3", "transcript_path": str(t), "source": "compact"})
    assert ctx and "/repo/docs/b.md" in ctx and "/repo/hooks/a.py" not in ctx
    assert "arquivos 1/2" in ctx


def _run_resume(payload: dict, monkeypatch) -> str:
    monkeypatch.setattr(sys, "stdin", io.StringIO(json.dumps(payload)))
    monkeypatch.setattr(loop_resume, "active_marker", lambda *a, **k: (None, None))
    out = io.StringIO()
    monkeypatch.setattr(sys, "stdout", out)
    assert loop_resume.main([]) == 0
    return out.getvalue()


def test_loop_resume_records_on_postcompact_without_a_loop_marker(tmp_path, monkeypatch):
    t = _transcript(tmp_path, "nada")
    _run_resume({"hook_event_name": "PostCompact", "session_id": "s4", "transcript_path": str(t),
                 "trigger": "manual", "compact_summary": "nada"}, monkeypatch)
    assert (tmp_path / "ledger" / "compactions.jsonl").exists()


def test_loop_resume_injects_gaps_on_sessionstart_compact_without_a_loop_marker(tmp_path, monkeypatch):
    t = _transcript(tmp_path, "nada")
    out = _run_resume({"hook_event_name": "SessionStart", "source": "compact", "session_id": "s5",
                       "transcript_path": str(t)}, monkeypatch)
    ctx = json.loads(out)["hookSpecificOutput"]["additionalContext"]
    assert "/repo/docs/b.md" in ctx and TASK in ctx


def test_loop_resume_stays_silent_on_startup_without_marker(tmp_path, monkeypatch):
    t = _transcript(tmp_path, "nada")
    out = _run_resume({"hook_event_name": "SessionStart", "source": "startup", "session_id": "s6",
                       "transcript_path": str(t)}, monkeypatch)
    assert out == ""
