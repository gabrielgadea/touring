"""Tests for the offline measurement scripts (synthetic transcripts only — no real content).

The F4a defect of 01/11/2026 (tool_use from non user/assistant records, duplicated ids) was
found by a Rust port, not by a test; these tests pin each script's counting rule."""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import f4a_lessons as f4a
import f4b_lesson_cost as f4b
import f4c_loopguard as f4c
import f4d_prune as f4d


def _use(i, name="Bash", inp=None, rtype="assistant"):
    return {"type": rtype, "message": {"content": [
        {"type": "tool_use", "id": i, "name": name, "input": inp or {}}]}}


def _res(i, err=False, text="ok"):
    return {"type": "user", "message": {"content": [
        {"type": "tool_result", "tool_use_id": i, "is_error": err, "content": text}]}}


def _prompt(text="do it"):
    return {"type": "user", "message": {"content": text}}


def _write(tmp_path, name, records):
    p = tmp_path / name
    p.write_text("\n".join(json.dumps(r) for r in records) + "\n", encoding="utf-8")
    return p


def test_f4a_pairs_a_failure_with_its_resolution(tmp_path):
    p = _write(tmp_path, "s.jsonl", [_use("a"), _res("a", True, "boom /x/y 42"), _use("b"), _res("b")])
    assert f4a.pairs_of(p) == [("Bash", "boom /x/y 42")]


def test_f4a_drops_a_chain_longer_than_the_window(tmp_path):
    recs = []
    for i in range(f4a.WINDOW + 1):
        recs += [_use(f"e{i}"), _res(f"e{i}", True, "fail")]
    recs += [_use("ok"), _res("ok")]
    assert f4a.pairs_of(_write(tmp_path, "s.jsonl", recs)) == []


def test_f4a_counts_only_user_assistant_records_and_each_id_once(tmp_path):
    recs = [_use("a"), _use("a"), _use("x", rtype="attachment"),
            _res("a", True, "err"), _use("b"), _res("b")]
    order, _ = f4a.read_tool_events(_write(tmp_path, "s.jsonl", recs))
    assert order == {"Bash": ["a", "b"]}


def test_f4a_normalization_collapses_paths_numbers_and_hex():
    f4a.NORMALIZE = True
    try:
        assert f4a.norm("no file /a/b.py at 12 0xdeadbeef") == "no file <path> at <n> <hex>"
    finally:
        f4a.NORMALIZE = False


def test_f4b_counts_past_failure_lines_and_keys(tmp_path):
    rec = {"type": "attachment", "x": "pre\nPast failure [outcome:bash:k1:failure]: v\nPast failure [k2]: w"}
    lines, nbytes, keys = f4b.scan(_write(tmp_path, "s.jsonl", [rec]))
    assert lines == 2 and nbytes > 0 and keys == {"outcome:bash:k1:failure", "k2"}


def test_f4c_flags_a_spiral_and_spares_a_clean_long_turn(tmp_path):
    spiral = [_prompt()]
    for i in range(24):
        spiral.append(_use(f"e{i}", "Edit", {"file_path": "/a.py"}))
        spiral.append(_res(f"e{i}", err=i % 2 == 0, text="same error"))
    clean = [_prompt()] + [r for i in range(24) for r in (_use(f"c{i}", "Read"), _res(f"c{i}"))]
    flagged = [t for t in f4c.turns_of(_write(tmp_path, "a.jsonl", spiral)) if t.flag_at]
    spared = [t for t in f4c.turns_of(_write(tmp_path, "b.jsonl", clean)) if t.flag_at]
    assert len(flagged) == 1 and flagged[0].flag_at >= f4c.TH["min_turns"]
    assert spared == []


def _session(ts, prompts, ann, used):
    kinds = ("skill", "agent", "tool")
    return (ts, prompts, {k: set(ann.get(k, ())) for k in kinds},
            {k: set(used.get(k, ())) for k in kinds}, {k: 0 for k in kinds})


def test_f4d_prunes_after_s_sessions_and_counts_a_false_prune():
    sessions = [_session(f"2026-10-0{i}", 1, {"skill": ["s"]}, {}) for i in range(1, f4d.S + 1)]
    sessions.append(_session("2026-10-09", 1, {"skill": ["s"]}, {"skill": ["s"]}))
    res = f4d.simulate(sessions, "skill")
    assert res["items_ever_pruned"] == 1 and res["false_prune_events"] == 1
    assert res["pruned_at_end"] == 0


def test_f4d_never_prunes_an_item_in_use():
    sessions = [_session(f"2026-10-{i:02d}", 20, {"agent": ["a"]}, {"agent": ["a"]}) for i in range(1, 9)]
    assert f4d.simulate(sessions, "agent")["items_ever_pruned"] == 0
