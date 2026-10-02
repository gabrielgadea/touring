"""Contract tests for the r3 shadow event schema (S-1) — privacy is a test, not a promise (S6)."""
import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_event_schema as ev  # noqa: E402
import r3_use_cases as uc  # noqa: E402

A2 = {
    "v": 1, "kind": "a2_offer_closed", "use_case": "a2_outcome", "ts_ms": 1790956285165,
    "session_hash": "9f86d081884c", "mode": "code", "offered_secs": 1790956280,
    "closed_by": "consumed", "cmd_heads": ["touring run --lang bash --code 'ls'"],
    "baseline_value": 1.0, "baseline_followed": True,
}
A1 = {
    "v": 1, "kind": "a1_block", "use_case": "a1_relevance", "ts_ms": 1790956285200,
    "session_hash": "9f86d081884c", "hook": "pre-bash", "bytes": 912,
    "first_line_sha": "ab12cd34ef567890", "first_line_head": "[TOURING SUGGEST · code-mode-loop · conf=0.95]",
}
SD = {
    "v": 1, "kind": "shadow_decision", "use_case": "a2_outcome", "ts_ms": 1790956285900,
    "arm": "B-L", "event_ref": {"kind": "a2_offer_closed", "ts_ms": 1790956285165, "session_hash": "9f86d081884c"},
    "decision": {"followed": True, "p": 0.93}, "baseline_agrees": True, "latency_ms": 31, "model": "laya-multilingual",
}


def test_each_event_type_roundtrips():
    for e in (A2, A1, SD):
        assert ev.validate_event(e) == [], e["kind"]
        assert ev.validate_event(json.loads(json.dumps(e))) == []


def test_free_text_over_80_chars_is_rejected():
    long = "x" * 81
    bad = dict(A2, cmd_heads=[long])
    assert any("80" in v for v in ev.validate_event(bad))
    bad = dict(A1, first_line_head=long)
    assert any("80" in v for v in ev.validate_event(bad))
    # the guard also catches free text hidden under an UNKNOWN key (a new field cannot smuggle a prompt)
    bad = dict(A1, prompt="y" * 200)
    assert ev.validate_event(bad) != []


def test_unknown_kind_and_missing_fields_are_rejected():
    assert ev.validate_event(dict(A2, kind="other")) != []
    broken = dict(A2)
    del broken["closed_by"]
    assert any("closed_by" in v for v in ev.validate_event(broken))
    assert ev.validate_event(dict(A2, closed_by="vanished")) != []
    assert ev.validate_event(dict(A2, cmd_heads=["a", "b", "c", "d"])) != []  # at most K=3


def test_the_schema_is_closed_even_for_a_SHORT_unknown_key():
    """Isolated from the 80-char cap: a short prompt under a new key must still be refused."""
    for e in (A1, A2, SD):
        violations = ev.validate_event(dict(e, note="short"))
        assert any("unknown key" in v for v in violations), e["kind"]


def test_session_id_must_be_hashed_never_the_raw_uuid():
    raw = "24006143-f691-47fe-b057-e2111fa79e97"
    for e in (A1, A2):  # every kind that carries a session, not just one
        assert any("session_hash" in v for v in ev.validate_event(dict(e, session_hash=raw))), e["kind"]
    bad_ref = dict(SD, event_ref=dict(SD["event_ref"], session_hash=raw))
    assert ev.validate_event(bad_ref) != []
    assert ev.hash_session(raw) != raw and len(ev.hash_session(raw)) == 12
    assert ev.hash_session(raw) == ev.hash_session(raw)  # deterministic, so events of a session join


def test_jev_payload_has_only_redacted_features():
    payload = ev.redact_for_jev(A2)
    assert ev.validate_jev_payload(payload) == []
    assert set(payload) <= ev.JEV_ALLOWED_KEYS
    # the command text is gone; only its first token survives
    assert "ls" not in json.dumps(payload) and payload["first_token"] == "touring"
    smuggled = dict(payload, note="rm -rf /home/user/secret-project")
    assert ev.validate_jev_payload(smuggled) != []
    # isolated from the whitespace rule: a whitespace-free value under a key that is not allowed
    assert any("not allowed" in v for v in ev.validate_jev_payload(dict(payload, note="secret")))
    assert ev.validate_jev_payload(dict(payload, first_token="touring run --lang bash")) != []  # whitespace = free text


def test_privacy_scan_flags_a_line_with_long_free_text():
    assert ev.scan_privacy(json.dumps(A2)) == []
    assert ev.scan_privacy(json.dumps(dict(A2, cmd_heads=["z" * 120]))) != []


def test_registry_has_both_use_cases_with_the_frozen_floors():
    assert set(uc.REGISTRY) == {"a2_outcome", "a1_relevance"}
    f2 = uc.REGISTRY["a2_outcome"]["floors"]
    assert (f2["delta_agreement"], f2["stratum_regression"], f2["precision"], f2["coverage"]) == (0.05, -0.03, 0.90, 0.25)
    assert f2["kappa_gold"] == 0.60 and f2["b0_sufficient_agreement"] == 0.95
    f1 = uc.REGISTRY["a1_relevance"]["floors"]
    assert (f1["bytes_saved_rule"], f1["followed_recall_rule"], f1["harm_lower_ci"], f1["model_increment"]) == (0.25, 1.00, 0.90, 0.10)
    assert f1["followed_recall_model"] == 0.95
    assert uc.REGISTRY["a2_outcome"]["event_kind"] == "a2_offer_closed"
    assert uc.REGISTRY["a1_relevance"]["event_kind"] == "a1_block"
