#!/usr/bin/env python3
"""Tests for scout_perpetuo.py — F1.7 acceptance mechanics.

Covers: yield from ledger delta, ticket on yield, exponential backoff (never to
zero, never past monthly), NEW_FINDINGS loop protocol, act-vs-wait stances.
"""

import json
from pathlib import Path

import pytest

import scout_perpetuo as sp


@pytest.fixture()
def root(tmp_path: Path) -> Path:
    return tmp_path


def write_ledger(root: Path, topic: str, n_findings: int, questions=None) -> None:
    path = sp.ledger_path(root, topic)
    path.parent.mkdir(parents=True, exist_ok=True)
    findings = {f"f_{i}": {"id": f"f_{i}", "key": f"k{i}", "round": 1}
                for i in range(n_findings)}
    path.write_text(json.dumps({
        "version": 1, "topic": topic, "findings": findings,
        "questions": questions or [], "rounds": [],
        "verdict": {"converged": False, "statement": "exploration incomplete"},
    }), encoding="utf-8")


@pytest.fixture()
def fake_explore(monkeypatch):
    """Explore stub: each call grows the ledger by a scripted amount."""
    plan = {"growth": [], "calls": 0}

    def fake(topic, root):
        idx = plan["calls"]
        plan["calls"] += 1
        grow = plan["growth"][idx] if idx < len(plan["growth"]) else 0
        count, _, _ = sp.ledger_snapshot(root, topic)
        write_ledger(root, topic, count + grow)
        return 0

    monkeypatch.setattr(sp, "run_explore", fake)
    return plan


@pytest.fixture()
def fake_ticket(monkeypatch):
    filed = []
    monkeypatch.setattr(sp, "file_ticket",
                        lambda topic, n, keys: (filed.append((topic, n)) or f"task_fake_{len(filed)}"))
    return filed


def test_yielding_cycle_files_ticket_and_halves_interval(root, fake_explore, fake_ticket, capsys):
    fake_explore["growth"] = [5]
    assert sp.cmd_cycle(root, "meu topico") == 0
    out = capsys.readouterr().out
    result = json.loads(out.splitlines()[0])
    assert result["yield"] == 5 and result["ticket"] == "task_fake_1"
    assert "NEW_FINDINGS=5" in out  # ADW loop protocol
    state = sp.load_state(root, "meu topico")
    assert state["interval_hours"] == sp.DEFAULT_INTERVAL_H / 2
    assert fake_ticket == [("meu topico", 5)]


def test_dry_cycle_backs_off_never_to_zero_or_past_monthly(root, fake_explore, fake_ticket, capsys):
    fake_explore["growth"] = [0] * 8
    for _ in range(8):
        sp.cmd_cycle(root, "t")
    state = sp.load_state(root, "t")
    assert state["interval_hours"] == sp.MAX_INTERVAL_H  # capped at ~monthly
    assert all(h["yield"] == 0 and not h["ticket"] for h in state["history"])
    assert fake_ticket == []
    # floor check: yields keep halving but never reach zero interval
    fake_explore["growth"] = [3] * 12
    fake_explore["calls"] = 0
    for _ in range(12):
        sp.cmd_cycle(root, "t")
    assert sp.load_state(root, "t")["interval_hours"] == sp.MIN_INTERVAL_H


def test_status_recommends_wait_while_yielding(root, fake_explore, fake_ticket, capsys):
    fake_explore["growth"] = [4, 2]
    sp.cmd_cycle(root, "t")
    sp.cmd_cycle(root, "t")
    capsys.readouterr()
    sp.cmd_status(root, "t")
    status = json.loads(capsys.readouterr().out)
    assert status["yield_curve"] == [4, 2]
    assert status["recommendation"]["stance"] == "wait"
    assert len(status["tickets"]) == 2


def test_status_recommends_act_on_dry_equilibrium(root, fake_explore, fake_ticket, capsys):
    fake_explore["growth"] = [3, 0, 0]
    for _ in range(3):
        sp.cmd_cycle(root, "t")
    capsys.readouterr()
    sp.cmd_status(root, "t")
    status = json.loads(capsys.readouterr().out)
    assert status["recommendation"]["stance"] == "act"


def test_status_act_with_caveats_when_questions_open(root, fake_explore, fake_ticket, capsys):
    fake_explore["growth"] = [0, 0]
    sp.cmd_cycle(root, "t")
    sp.cmd_cycle(root, "t")
    count, _, _ = sp.ledger_snapshot(root, "t")
    write_ledger(root, "t", count, questions=[{"q": "what about X?", "resolved": False}])
    capsys.readouterr()
    sp.cmd_status(root, "t")
    status = json.loads(capsys.readouterr().out)
    assert status["recommendation"]["stance"] == "act-with-caveats"
    assert status["open_questions"]


def test_slug_matches_explore_convention():
    assert sp.slugify("run_gateway") == "run-gateway"
    assert sp.ledger_path(Path("/x"), "run_gateway") == Path("/x/.touring-explore/run-gateway.ledger.json")


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
