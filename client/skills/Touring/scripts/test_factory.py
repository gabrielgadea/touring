#!/usr/bin/env python3
"""Tests for factory.py — F4 router mechanics.

Deterministic-first routing, rule priority, refusal of fake gates (missing
verify_cmd), stats/RL bookkeeping. CLI probes are stubbed — routing must never
depend on their availability.
"""

import json
from pathlib import Path

import pytest

import factory


@pytest.fixture(autouse=True)
def _no_probes(monkeypatch):
    monkeypatch.setattr(factory, "probe", lambda cmd, root: "")


@pytest.fixture()
def root(tmp_path: Path) -> Path:
    return tmp_path


@pytest.mark.parametrize("ticket,expected", [
    ("login fails with 500 after the last deploy", "bugfix"),
    ("fix the wrong result in parse_duration", "bugfix"),
    ("typo in the README badge", "chore"),
    ("bump dependency and cleanup imports", "chore"),
    ("implement support for OAuth device flow", "feature"),
    ("add a new endpoint for exports", "feature"),
    ("production is down — payments outage", "hotfix"),
    ("security review of the upload path", "audit"),
    ("research how the daemon shards project state", "explore-plan"),
])
def test_deterministic_routes(ticket, expected):
    hit = factory.deterministic_route(ticket)
    assert hit is not None and hit[0] == expected


def test_rule_priority_incident_beats_bugfix():
    # A ticket with both families routes by priority order: hotfix outranks bugfix.
    adw, _ = factory.deterministic_route("urgent production incident: checkout fails")
    assert adw == "hotfix"


def test_ambiguous_ticket_falls_to_llm_router(monkeypatch, root):
    monkeypatch.setattr(factory, "llm_route", lambda t: ("audit", "llm-router: stub"))
    decision = factory.route_ticket("hmm, something about the thing", root)
    assert decision["router"] == "llm" and decision["adw"] == "audit"


def test_llm_route_unavailable_defaults_to_discovery(monkeypatch):
    monkeypatch.setattr(factory.subprocess, "run",
                        lambda *a, **k: (_ for _ in ()).throw(FileNotFoundError()))
    adw, why = factory.llm_route("???")
    assert adw == "explore-plan" and "default" in why


def test_start_refuses_fake_gate_when_verify_cmd_missing(root, capsys, monkeypatch):
    monkeypatch.setattr(factory, "record_route", lambda *a: None)
    rc = factory.cmd_start(root, "fix the broken parser", {}, background=False)
    out = json.loads(capsys.readouterr().out)
    assert rc == 2 and out["started"] is False
    assert "verify_cmd" in out["error"]


def test_build_vars_fills_ticket_and_reports_missing():
    variables, missing = factory.build_vars("bugfix", "fix X", {"target": "src/"})
    assert variables["symptom"] == "fix X" and variables["target"] == "src/"
    assert missing == ["verify_cmd"]
    variables, missing = factory.build_vars("audit", "src/lib.rs", {})
    assert variables == {"target": "src/lib.rs"} and missing == []


def test_stats_and_reward_bookkeeping(root):
    factory.record_route(root, "bugfix")
    factory.record_route(root, "bugfix")
    factory.record_route(root, "chore")
    factory.reward_outcome(root, "bugfix", "completed")
    stats = factory.load_stats(root)
    assert stats["routed_total"] == 3
    assert stats["by_adw"] == {"bugfix": 2, "chore": 1}
    assert stats["outcomes"] == [{"adw": "bugfix", "status": "completed", "reward": 1.0}]


def test_cila_level_parses_hook_json():
    raw = json.dumps({"metadata": {"cila_level": 3, "cila_name": "Complex"}})
    assert factory.cila_level(raw) == 3
    assert factory.cila_level("garbage cila_level: 2 text") == 2
    assert factory.cila_level("") == 0


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
