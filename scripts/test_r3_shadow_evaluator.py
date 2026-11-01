"""S-11 — the shadow evaluator: durable tail, closed-schema validation, pluggable arms, fail-open."""
import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_event_schema as ev  # noqa: E402
import r3_shadow_evaluator as se  # noqa: E402

H = "9f86d081884c"


def a2(ts=1000, followed=True, head="touring run --lang bash --code 'ls'"):
    return {"v": 1, "kind": "a2_offer_closed", "use_case": "a2_outcome", "ts_ms": ts, "session_hash": H,
            "mode": "code", "offered_secs": 1, "closed_by": "consumed", "cmd_heads": [head],
            "baseline_value": 1.0 if followed else None, "baseline_followed": followed}


def a1(ts=2000, sha="ab12cd34ef567890", head="[TOURING SUGGEST · code-mode-loop · conf=0.95]", hook="pre-bash"):
    return {"v": 1, "kind": "a1_block", "use_case": "a1_relevance", "ts_ms": ts, "session_hash": H,
            "hook": hook, "bytes": 800, "first_line_sha": sha, "first_line_head": head}


def put(logs, *events, raw=None):
    with (logs / "r3_events.jsonl").open("a") as f:
        for e in events:
            f.write(json.dumps(e) + "\n")
        if raw:
            f.write(raw)


def shadow(logs):
    p = logs / "r3_shadow.jsonl"
    return [json.loads(x) for x in p.read_text().splitlines()] if p.exists() else []


class FakeArm:
    name, use_case, model, kind = "B-L", "a2_outcome", "fake", "local"

    def __init__(self, answer=True):
        self.answer, self.seen = answer, []

    def decide(self, event):
        self.seen.append(event)
        return {"followed": self.answer, "p": 0.9}


def test_offset_survives_restart(tmp_path):
    arm = FakeArm()
    put(tmp_path, a2(1000), a2(1001))
    s1 = se.run_once(tmp_path, [arm])
    assert s1["read"] == 2 and len(shadow(tmp_path)) == 2
    put(tmp_path, a2(1002))
    s2 = se.run_once(tmp_path, [FakeArm()])  # a NEW evaluator object, as after a restart
    assert s2["read"] == 1 and len(shadow(tmp_path)) == 3  # only the new event was processed


def test_stopped_evaluator_never_blocks_the_emitter(tmp_path):
    """The emitter only appends to r3_events.jsonl; with no evaluator running the queue just grows."""
    for i in range(50):
        put(tmp_path, a2(1000 + i))
    assert not (tmp_path / "r3_shadow.jsonl").exists()       # nothing consumed, nothing blocked
    s = se.run_once(tmp_path, [FakeArm()])
    assert s["read"] == 50 and len(shadow(tmp_path)) == 50   # the whole backlog is drained later


def test_no_free_text_leaves_the_machine(tmp_path):
    seen = []

    class ApiArm:
        name, use_case, model, kind = "B-J", "a2_outcome", "jev", "api"

        def decide(self, payload):
            seen.append(payload)
            return {"followed": True, "p": 0.8}

    put(tmp_path, a2(1000, head="touring run --lang bash --code 'cat ~/secret-project/notes.md'"))
    se.run_once(tmp_path, [ApiArm()])
    assert len(seen) == 1
    assert ev.validate_jev_payload(seen[0]) == []                     # the API arm got ONLY redacted features
    assert "secret-project" not in json.dumps(seen[0]) and "cmd_heads" not in seen[0]
    assert all(ev.scan_privacy(json.dumps(r)) == [] for r in shadow(tmp_path))


def test_a_local_arm_may_see_the_80_char_heads(tmp_path):
    arm = FakeArm()
    put(tmp_path, a2(1000))
    se.run_once(tmp_path, [arm])
    assert arm.seen[0]["cmd_heads"]  # loopback only: allowed by the contract


def test_invalid_lines_are_dropped_and_counted_never_crash(tmp_path):
    bad = dict(a2(1001), note="short")                       # unknown key: the schema is closed
    put(tmp_path, a2(1000), bad, raw="not json at all\n")
    s = se.run_once(tmp_path, [FakeArm()])
    assert s["invalid"] == 2 and len(shadow(tmp_path)) == 1


def test_a_partial_trailing_line_waits_for_the_next_pass(tmp_path):
    full = json.dumps(a2(1000)) + "\n"
    half = json.dumps(a2(1001))
    (tmp_path / "r3_events.jsonl").write_text(full + half[:30])   # the emitter is mid-write
    s1 = se.run_once(tmp_path, [FakeArm()])
    assert s1["read"] == 1
    with (tmp_path / "r3_events.jsonl").open("a") as f:
        f.write(half[30:] + "\n")
    assert se.run_once(tmp_path, [FakeArm()])["read"] == 1       # completed, then consumed exactly once


def test_truncated_or_rotated_events_file_restarts_from_zero(tmp_path):
    put(tmp_path, a2(1000), a2(1001), a2(1002))
    se.run_once(tmp_path, [FakeArm()])
    (tmp_path / "r3_events.jsonl").write_text(json.dumps(a2(2000)) + "\n")  # smaller than the saved offset
    assert se.run_once(tmp_path, [FakeArm()])["read"] == 1


def test_an_arm_that_raises_is_counted_and_the_others_still_run(tmp_path):
    class Boom(FakeArm):
        name = "B-J"

        def decide(self, event):
            raise RuntimeError("API down")

    good = FakeArm()
    put(tmp_path, a2(1000))
    s = se.run_once(tmp_path, [Boom(), good])
    assert s["arm_errors"] == {"B-J": 1} and len(shadow(tmp_path)) == 1 and shadow(tmp_path)[0]["arm"] == "B-L"


def test_shadow_lines_validate_and_carry_agreement_with_the_baseline(tmp_path):
    put(tmp_path, a2(1000, followed=True), a2(1001, followed=False))
    se.run_once(tmp_path, [FakeArm(answer=True)])
    rows = shadow(tmp_path)
    assert all(ev.validate_event(r) == [] for r in rows)
    assert [r["baseline_agrees"] for r in rows] == [True, False]
    assert all(r["latency_ms"] >= 0 and r["use_case"] == "a2_outcome" for r in rows)


def test_a1_dedup_arm_drops_a_repeat_but_never_an_exempt_block(tmp_path):
    first = a1(2000)
    again = a1(2001)                                            # same sha in the same session ⇒ duplicate
    deny1 = a1(2002, sha="1111111111111111", head="[CODE MODE · rajada] 2ª inspeção `grep`", hook="pre-bash")
    deny2 = a1(2003, sha="1111111111111111", head="[CODE MODE · rajada] 2ª inspeção `grep`", hook="pre-bash")
    put(tmp_path, first, again, deny1, deny2)
    se.run_once(tmp_path, [se.DedupArm()])
    drops = [r["decision"]["would_drop"] for r in shadow(tmp_path)]
    assert drops == [False, True, False, False]                 # repeat dropped; a repeated EXEMPT deny is NEVER dropped


def test_dedup_window_forgets_after_five_blocks(tmp_path):
    put(tmp_path, a1(1, sha="aaaaaaaaaaaaaaaa"))
    for i in range(2, 8):                                       # 6 different blocks push the first out of the window
        put(tmp_path, a1(i, sha=f"{i:016x}"))
    put(tmp_path, a1(9, sha="aaaaaaaaaaaaaaaa"))
    se.run_once(tmp_path, [se.DedupArm()])
    assert shadow(tmp_path)[-1]["decision"]["would_drop"] is False


def test_stats_file_records_what_the_kpi_needs(tmp_path):
    put(tmp_path, a2(1000), raw="garbage\n")
    se.run_once(tmp_path, [FakeArm()])
    st = json.loads((tmp_path / "r3_shadow.stats.json").read_text())
    assert st["read"] == 1 and st["invalid"] == 1 and "updated_ms" in st


def test_main_once_exits_zero_on_an_empty_queue(tmp_path, monkeypatch):
    monkeypatch.setenv("R3_LOGS_DIR", str(tmp_path))
    assert se.main(["--once"]) == 0
