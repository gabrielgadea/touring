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


def write_ledger(root: Path, topic: str, n_findings: int, questions=None,
                 tail_keys=None) -> None:
    path = sp.ledger_path(root, topic)
    path.parent.mkdir(parents=True, exist_ok=True)
    findings = {f"f_{i}": {"id": f"f_{i}", "key": f"k{i}", "round": 1}
                for i in range(n_findings)}
    if tail_keys:
        # Name the keys of the LAST findings — those are the ones a cycle reads
        # as new, so this is how a self-referential finding is staged.
        for nome, chave in zip(list(findings)[-len(tail_keys):], tail_keys):
            findings[nome]["key"] = chave
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
        write_ledger(root, topic, count + grow, tail_keys=plan.get("next_keys"))
        plan["next_keys"] = None  # one cycle only, so the next call is clean
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


def test_yielding_cycle_routes_ticket_through_factory(root, fake_explore, fake_ticket,
                                                      monkeypatch, capsys):
    """Sugestão-2 intake: the scout's ticket reaches the factory router and the
    recommended ADW travels with it (state + cycle output + status queue)."""
    import sys as _sys
    import types
    calls = {"routed": [], "recorded": []}
    fake_factory = types.ModuleType("factory")
    fake_factory.route_ticket = lambda desc, r: (calls["routed"].append(desc)
                                                or {"adw": "audit", "router": "deterministic"})
    fake_factory.record_route = lambda r, adw: calls["recorded"].append(adw)
    monkeypatch.setitem(_sys.modules, "factory", fake_factory)

    fake_explore["growth"] = [3]
    assert sp.cmd_cycle(root, "meu topico") == 0
    result = json.loads(capsys.readouterr().out.splitlines()[0])
    assert result["routed_adw"] == "audit"
    assert calls["recorded"] == ["audit"]
    assert "meu topico" in calls["routed"][0]

    entry = sp.load_state(root, "meu topico")["history"][-1]
    assert entry["adw"] == "audit"
    assert entry["start_hint"].startswith("touring factory start ")

    sp.cmd_status(root, "meu topico")
    status = json.loads(capsys.readouterr().out)
    assert status["queue"] == [{"ticket": "task_fake_1", "adw": "audit",
                                "start_hint": entry["start_hint"]}]


def test_factory_failure_never_blocks_scout_cycle(root, fake_explore, fake_ticket,
                                                  monkeypatch, capsys):
    import sys as _sys
    import types
    broken = types.ModuleType("factory")

    def _boom(desc, r):
        raise RuntimeError("router down")

    broken.route_ticket = _boom
    monkeypatch.setitem(_sys.modules, "factory", broken)

    fake_explore["growth"] = [2]
    assert sp.cmd_cycle(root, "t") == 0  # fail-open: routing enriches, never blocks
    result = json.loads(capsys.readouterr().out.splitlines()[0])
    assert result["ticket"] == "task_fake_1" and result["routed_adw"] is None
    assert sp.load_state(root, "t")["history"][-1]["adw"] is None


def test_a_finding_pointing_at_our_own_ticket_is_not_yield(root, fake_explore,
                                                           fake_ticket, capsys):
    """The loop measured on 19/09/2026: the scout retrieving its own ticket.

    Ticket `task_1788296582280254749` carried `decomp:task_1787922622533751850`
    — the ticket filed by the cycle before it — as its sample. A cycle whose
    only 'discovery' is its own handwriting must read as DRY.
    """
    # Cycle 1 yields for real and files a ticket; cycle 2 grows by one too.
    fake_explore["growth"] = [1, 1]
    sp.cmd_cycle(root, "t")
    capsys.readouterr()
    filed = sp.load_state(root, "t")["history"][-1]["ticket"]
    assert filed, "the fixture must file a ticket for this test to mean anything"

    # Cycle 2 finds exactly one 'new' finding: a pointer to that ticket.
    fake_explore["next_keys"] = [f"decomp:{filed}"]
    sp.cmd_cycle(root, "t")
    result = json.loads(capsys.readouterr().out.splitlines()[0])

    assert result["yield"] == 0, f"an echo is not a finding: {result}"
    assert result["raw_yield"] == 1, "the raw count stays visible, never hidden"
    assert result["own_echoes"] == 1
    assert result["ticket"] is None, "a dry cycle files no ticket"


def test_a_real_finding_still_counts_alongside_an_echo(root, fake_explore,
                                                       fake_ticket, capsys):
    fake_explore["growth"] = [1, 2]
    sp.cmd_cycle(root, "t")
    capsys.readouterr()
    filed = sp.load_state(root, "t")["history"][-1]["ticket"]

    fake_explore["next_keys"] = [f"decomp:{filed}", "memory:a-real-lesson"]
    sp.cmd_cycle(root, "t")
    result = json.loads(capsys.readouterr().out.splitlines()[0])

    assert result["yield"] == 1, f"the genuine finding survives the filter: {result}"
    assert result["own_echoes"] == 1


def test_slug_matches_explore_convention():
    assert sp.slugify("run_gateway") == "run-gateway"
    assert sp.ledger_path(Path("/x"), "run_gateway") == Path("/x/.touring-explore/run-gateway.ledger.json")


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
