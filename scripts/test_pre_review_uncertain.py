#!/usr/bin/env python3
"""Testes do outcome "uncertain" de scripts/pre_review_triage.py (S-10, plano r2).

A banda 0,5±0,1 é roteável de primeira classe: fast path exige todas "no";
qualquer yes OU uncertain roteia para revisão completa (self-consistency cookbook).
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import pre_review_triage as triage_mod  # noqa: E402


def _fake_answers(probs: dict[str, float]):
    return lambda state, questions: {qid: {"noul": p} for qid, p in probs.items()}


def test_outcome_of_bands() -> None:
    assert triage_mod.outcome_of(0.9) == "yes"
    assert triage_mod.outcome_of(0.6) == "yes"
    assert triage_mod.outcome_of(0.52) == "uncertain"
    assert triage_mod.outcome_of(0.4) == "no"
    assert triage_mod.outcome_of(0.1) == "no"
    assert triage_mod.outcome_of(None) == "uncertain"  # ausência é incerteza declarada


def test_uncertain_blocks_fast_path_without_being_yes(monkeypatch: pytest.MonkeyPatch) -> None:
    probs = {qid: 0.05 for qid in triage_mod.TRIAGE_QUESTIONS}
    probs["concurrency_or_shared_state"] = 0.52  # nem sim nem não: INCERTO
    monkeypatch.setattr(triage_mod.jev_client, "system_one", _fake_answers(probs))
    result = triage_mod.triage("diff qualquer")
    assert result["fast_path"] is False
    assert result["flagged"] == []
    assert result["uncertain"] == ["concurrency_or_shared_state"]
    assert result["outcomes"]["concurrency_or_shared_state"] == "uncertain"


def test_all_no_gives_fast_path(monkeypatch: pytest.MonkeyPatch) -> None:
    probs = {qid: 0.05 for qid in triage_mod.TRIAGE_QUESTIONS}
    monkeypatch.setattr(triage_mod.jev_client, "system_one", _fake_answers(probs))
    result = triage_mod.triage("diff trivial")
    assert result["fast_path"] is True
    assert result["uncertain"] == [] and result["flagged"] == []
