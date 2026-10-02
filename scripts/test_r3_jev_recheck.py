"""Errata 11 — the Jev stability recheck: a hard call cap, failure != p=0, and the reading rules are code, each proven by a test."""
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_jev_recheck as rc  # noqa: E402

P1 = {"use_case": "a2_outcome", "kind": "a2_offer_closed", "mode": "code", "closed_by": "consumed", "n_cmds": 3, "first_token": "touring",
      "baseline_value_bucket": "full"}
P2 = {**P1, "first_token": "git"}


def test_equal_payloads_share_a_stable_short_hash_and_different_ones_do_not():
    assert rc.payload_hash(P1) == rc.payload_hash(dict(reversed(list(P1.items()))))   # key order is irrelevant
    assert rc.payload_hash(P1) != rc.payload_hash(P2) and len(rc.payload_hash(P1)) == 8


def test_the_distinct_payloads_collapse_equal_items():
    out = rc.distinct_payloads([P1, P2, P1, dict(P1)])
    assert sorted(out.values(), key=lambda p: p["first_token"]) == [P2, P1] and len(out) == 2


def test_compare_reports_the_largest_and_mean_absolute_difference_over_shared_keys():
    out = rc.compare({"a": 0.5, "b": 0.2, "c": 0.9}, {"a": 0.5, "b": 0.3, "z": 0.1})
    assert out["n"] == 2 and out["n_equal"] == 1
    assert out["max_abs_diff"] == pytest.approx(0.1) and out["mean_abs_diff"] == pytest.approx(0.05)


def test_the_second_pass_is_needed_only_when_the_first_one_differs_from_the_cache():
    assert rc.needs_second_pass({"max_abs_diff": 0.0}) is False
    assert rc.needs_second_pass({"max_abs_diff": 0.004}) is True


def test_stability_is_read_with_the_preregistered_tolerance():
    assert rc.stability({"max_abs_diff": 0.05}) == "estavel" and rc.stability({"max_abs_diff": 0.0501}) == "instavel"
    assert rc.TOLERANCE == 0.05


def test_the_call_cap_is_hard_the_call_after_the_cap_never_happens():
    calls = []

    def call(payload):
        calls.append(payload)
        return 0.5

    payloads = {f"h{i}": {"n": i} for i in range(4)}
    with pytest.raises(RuntimeError, match="cap"):
        rc.run(payloads, call, max_calls=3)
    assert len(calls) == 3


def test_the_cap_counts_across_passes():
    n = {"calls": 0}

    def call(payload):
        n["calls"] += 1
        return 0.5

    payloads = {"h0": {}, "h1": {}}
    first = rc.run(payloads, call, max_calls=3)
    assert first["n_calls"] == 2
    with pytest.raises(RuntimeError, match="cap"):
        rc.run(payloads, call, max_calls=3, already_spent=first["n_calls"])
    assert n["calls"] == 3


def test_a_failed_call_is_an_error_never_a_probability_of_zero():
    out = rc.run({"h0": {}, "h1": {}}, lambda p: None if not p.get("ok") else 0.7, max_calls=5)
    assert out["live"] == {} and sorted(out["errors"]) == ["h0", "h1"]
    ok = rc.run({"h0": {"ok": 1}, "h1": {}}, lambda p: 0.7 if p.get("ok") else None, max_calls=5)
    assert ok["live"] == {"h0": 0.7} and list(ok["errors"]) == ["h1"] and ok["n_calls"] == 2        # an error still spent the call


def test_an_exception_in_a_call_is_an_error_the_run_goes_on_and_the_call_is_counted():
    def call(payload):
        if payload.get("boom"):
            raise TimeoutError("jev timed out")
        return 0.4

    out = rc.run({"h0": {"boom": 1}, "h1": {}}, call, max_calls=5)
    assert out["live"] == {"h1": 0.4} and out["errors"] == {"h0": "TimeoutError: jev timed out"} and out["n_calls"] == 2


def test_substitute_replaces_only_the_jev_probability_of_the_items_that_were_re_asked():
    preds = [{"item_id": "x", "B-J": {"item": "s", "p_followed": 0.1, "latency_ms": 0.0}, "B-L": {"p_followed": 0.3}},
             {"item_id": "y", "B-J": {"item": "t", "p_followed": 0.2}, "B-L": {"p_followed": 0.4}}]
    out = rc.substitute(preds, {"x": "ha", "y": "hb"}, {"ha": 0.9})
    assert out[0]["B-J"] == {"item": "s", "p_followed": 0.9, "latency_ms": 0.0} and out[0]["B-L"] == {"p_followed": 0.3}
    assert out[1] == preds[1] and preds[0]["B-J"]["p_followed"] == 0.1                                # the input is not mutated


def test_a_response_without_the_answer_is_not_valid():
    assert rc.has_answer({"a2_followed": {"type": "noul", "noul": 0.4}}, "a2_followed") is True
    assert rc.has_answer({"answers": {"a2_followed": {"noul": 0.4}}}, "a2_followed") is True
    assert rc.has_answer({}, "a2_followed") is False and rc.has_answer(None, "a2_followed") is False
    assert rc.has_answer({"other": {"noul": 0.4}}, "a2_followed") is False
