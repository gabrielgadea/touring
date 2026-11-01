#!/usr/bin/env python3
"""Testes vendados de scripts/pre_review_triage.py (S-5.3)."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import pre_review_triage as prt  # noqa: E402


class _FakeJev:
    """Controla as probabilidades devolvidas pelo adapter."""

    def __init__(self, probs: dict[str, float] | None) -> None:
        self.probs = probs
        self.calls: list[dict] = []

    def __call__(self, state, questions, **_kw) -> dict | None:
        self.calls.append({"state": state, "questions": questions})
        if self.probs is None:
            return None
        return {qid: {"type": "noul", "noul": p} for qid, p in self.probs.items()}


def test_seven_questions_in_one_call(monkeypatch: pytest.MonkeyPatch) -> None:
    """7 Nouls + 3 Scores (S-6) vão numa ÚNICA chamada (parallel questions — cookbook)."""
    fake = _FakeJev({qid: 0.1 for qid in prt.TRIAGE_QUESTIONS} | {qid: {"score": 1.0} for qid in prt.TRIAGE_SCORES})
    monkeypatch.setattr(prt.jev_client, "system_one", fake)
    prt.triage("diff --git a/x.py b/x.py\n+linha")
    assert len(fake.calls) == 1
    assert len(fake.calls[0]["questions"]) == 10


def test_all_false_fast_path(monkeypatch: pytest.MonkeyPatch) -> None:
    fake = _FakeJev({qid: 0.05 for qid in prt.TRIAGE_QUESTIONS})
    monkeypatch.setattr(prt.jev_client, "system_one", fake)
    result = prt.triage("+return 42")
    assert result["fast_path"] is True
    assert result["flagged"] == []


def test_flagged_names_reasons(monkeypatch: pytest.MonkeyPatch) -> None:
    probs = {qid: 0.05 for qid in prt.TRIAGE_QUESTIONS}
    probs["changes_public_api"] = 0.93
    fake = _FakeJev(probs)
    monkeypatch.setattr(prt.jev_client, "system_one", fake)
    result = prt.triage("+pub fn api()")
    assert result["fast_path"] is False
    assert result["flagged"] == ["changes_public_api"]
    assert result["reasons"] == ["muda API pública"]


def test_advisory_never_blocks(monkeypatch: pytest.MonkeyPatch) -> None:
    """Jev ausente → skipped com exit 0; o funil nunca bloqueia (contrato advisory)."""
    fake = _FakeJev(None)
    monkeypatch.setattr(prt.jev_client, "system_one", fake)
    result = prt.triage("+anything")
    assert result["status"] == "skipped"
    assert result["fast_path"] is None
    # main() retorna 0 mesmo skipped — prova pelo caminho público
    monkeypatch.setattr(sys, "argv", ["pre_review_triage.py", "--diff-file", "/dev/null"])
    assert prt.main() == 0


def test_probabilities_recorded(monkeypatch: pytest.MonkeyPatch) -> None:
    """As probabilidades brutas seguem visíveis — evidência no modelo, decisão no código."""
    fake = _FakeJev({qid: 0.3 for qid in prt.TRIAGE_QUESTIONS})
    monkeypatch.setattr(prt.jev_client, "system_one", fake)
    result = prt.triage("+x = 1")
    assert set(result["probabilities"]) == set(prt.TRIAGE_QUESTIONS)
    assert result["policy"]["advisory"] is True
