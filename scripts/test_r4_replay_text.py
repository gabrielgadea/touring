"""Errata 12 (r4 minimo) — B-L fed with the TRUNCATED TEXT: every rule is proven by a test that fails without it."""
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r4_replay_text as r4  # noqa: E402

LEAK = "SENTINEL-SESSION-9f3a"
ITEM = {"item_id": "a2-00001", "session": LEAK, "split": "audit-test", "b0_decisao": "bypass", "b0_valor": 0.123456, "n_cmds": 2, "ts": 1790000000,
        "rota_proposta": "touring run --lang python --code 'print(1)'", "cmds": ["ls -la", "touring run --lang bash --code 'cat x'"]}


def test_squash_collapses_every_run_of_whitespace_before_truncating():
    assert r4.truncate("a  b\n\n c\t d", 200) == "a b c d"
    assert len(r4.truncate("x " * 500, 200)) == 200 and r4.truncate("y" * 300, 200) == "y" * 200
    assert r4.truncate("  padded  ", 200) == "padded"


def test_the_state_is_the_route_and_up_to_three_commands_each_cut_at_200():
    long = "c" * 500
    item = {**ITEM, "rota_proposta": "r" * 500, "cmds": [long, "b", "c", "FOURTH-COMMAND"]}
    s = r4.build_state(item)
    assert "FOURTH-COMMAND" not in s and "r" * 200 in s and "r" * 201 not in s and "c" * 200 in s and "c" * 201 not in s
    assert s.count("\n1. ") == 1 and "\n2. b" in s and "\n3. c" in s and "4." not in s


def test_a_missing_command_list_is_said_not_invented():
    s = r4.build_state({**ITEM, "cmds": []})
    assert "(none)" in s and "1." not in s


def test_the_state_never_carries_the_decision_the_session_or_any_other_field():
    s = r4.build_state(ITEM)
    for secret in (LEAK, "bypass", "0.123456", "audit-test", "a2-00001", "1790000000"):
        assert secret not in s


def test_the_question_is_the_one_the_r3_arm_asked_so_only_the_input_differs():
    import r3_replay_a2 as rp
    assert r4.QUESTION == "Did the agent follow the recommended route after the nudge?" and r4.QID == rp.QID == "a2_followed"
    assert r4.SPEC == {rp.QID: {"type": "noul", "instructions": r4.QUESTION}}


class Fake:
    def __init__(self, *answers):
        self.answers, self.calls = list(answers), []

    def decide(self, state, questions=None, return_details=False, **kw):
        self.calls.append((state, questions, return_details))
        v = self.answers.pop(0)
        if isinstance(v, Exception):
            raise v
        return {"a2_followed": {"type": "noul", "noul": v}}


def test_predict_sends_the_built_text_and_the_registered_question_and_reads_the_probability():
    fake = Fake(0.42)
    out = r4.predict([ITEM], fake)
    assert fake.calls == [(r4.build_state(ITEM), r4.SPEC, True)]
    assert out["a2-00001"]["p_followed"] == pytest.approx(0.42) and out["a2-00001"]["latency_ms"] >= 0


def test_a_failed_inference_is_an_error_never_a_probability_of_zero():
    out = r4.predict([ITEM, {**ITEM, "item_id": "a2-00002"}], Fake(RuntimeError("oom"), 0.6))
    assert out["a2-00001"] == {"erro": "RuntimeError: oom"} and out["a2-00002"]["p_followed"] == pytest.approx(0.6)


def test_the_items_are_not_mutated_and_no_text_reaches_the_output():
    before = {**ITEM, "cmds": list(ITEM["cmds"])}
    out = r4.predict([ITEM], Fake(0.5))
    assert ITEM == before
    assert "touring" not in repr(out) and "ls -la" not in repr(out)             # predictions carry numbers, never the text


def test_with_arms_adds_each_arm_and_keeps_every_other_field():
    rows = [{"item_id": "a2-00001", "session_hash": "h1", "b0": "followed", "split": "audit-test", "B-L": {"p_followed": 0.3}, "B-J": {"p_followed": 0.4}},
            {"item_id": "a2-00002", "session_hash": "h2", "b0": "not_followed", "split": "audit-val", "B-L": {"p_followed": 0.1}, "B-J": {"p_followed": 0.2}}]
    out = r4.with_arms(rows, {"B-LT-EN": {"a2-00001": {"p_followed": 0.7, "latency_ms": 90.0}}})
    assert out[0]["B-LT-EN"] == {"item": "h1", "p_followed": 0.7, "latency_ms": 90.0}
    assert out[1]["B-LT-EN"] == {"item": "h2", "erro": "sem predicao"}
    assert out[0]["B-L"] == {"p_followed": 0.3} and out[1]["b0"] == "not_followed" and "B-LT-EN" not in rows[0]


def world(primary_ok, secondary_ok, n_val=90, n_test=150):
    """Deterministic: 2/3 followed; an arm is either perfectly separated (p .9/.1) or pure noise (p .5); B0 is wrong on every 5th item."""
    rows, gold = [], []
    for i in range(n_val + n_test):
        g = i % 3 != 0
        iid = f"a2-{i:05d}"
        sep = 0.9 if g else 0.1
        rows.append({"item_id": iid, "split": "audit-val" if i < n_val else "audit-test", "b0": "followed" if (g if i % 5 else not g) else "not_followed",
                     "B-LT-EN": {"p_followed": sep if primary_ok else 0.5, "latency_ms": 100.0},
                     "B-LT-ML": {"p_followed": sep if secondary_ok else 0.5, "latency_ms": 100.0}})
        gold.append({"item_id": iid, "followed": g, "provenance": "agree"})
    return rows, gold


SUMMARY = {"kappa": 0.77, "gold_insufficient": False, "n_indeterminate": 8}


def test_a_good_primary_arm_passes_and_the_verdict_comes_from_the_gate():
    out = r4.evaluate(*world(True, False), SUMMARY)
    assert out["verdict_primary"] == "PASSA" and out["primary"] == "B-LT-EN"


def test_only_the_primary_arm_decides_a_secondary_that_passes_cannot_make_the_primary_pass():
    out = r4.evaluate(*world(False, True), SUMMARY)
    assert out["verdict_primary"] != "PASSA" and out["verdict_secondary"] == "PASSA"      # the pre-registered single primary hypothesis


def test_an_insufficient_gold_is_passed_through_without_scoring_any_arm():
    rows, gold = world(True, True)
    out = r4.evaluate(rows, gold, {"kappa": 0.4, "gold_insufficient": True, "n_indeterminate": 3})
    assert out["verdict_primary"] == "GOLD_INSUFICIENTE" and out["results"]["arms"] == {}


def test_a_response_that_does_not_answer_the_question_is_an_error_not_a_zero():
    class Empty:
        def decide(self, state, questions=None, return_details=False, **kw):
            return {"some_other_question": {"noul": 0.9}}

    out = r4.predict([ITEM], Empty())
    assert out == {"a2-00001": {"erro": "no answer to the question"}}


def test_the_laya_decision_object_form_is_read_too():
    class Obj:
        answers = {"a2_followed": {"type": "noul", "noul": 0.31}}

    class Agent:
        def decide(self, state, questions=None, return_details=False, **kw):
            return Obj()

    assert r4.predict([ITEM], Agent())["a2-00001"]["p_followed"] == pytest.approx(0.31)


def test_the_checkpoints_are_the_english_root_primary_and_the_multilingual_subfolder():
    assert r4.SUBFOLDER == {"B-LT-EN": None, "B-LT-ML": "multilingual"} and r4.PRIMARY == "B-LT-EN" and r4.ARMS == ("B-LT-EN", "B-LT-ML")
