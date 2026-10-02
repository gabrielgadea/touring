#!/usr/bin/env python3
"""Testes vendados de scripts/laya_student_server.py (cross-audit 2026-10-01 — F3.1).

O Agent real NUNCA é carregado (init 18,7s) — get_agent é monkeypatchado.
parse_systemone_body é o contrato de validação do wire (erro 422 que nomeia o
campo, em vez do 500 opaco — A5: o erro ensina a correção).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import laya_student_server as srv  # noqa: E402


def test_build_answer_choice_from_dict() -> None:
    ans = {"choice": "code", "probabilities": {"code": 0.9}, "confidence": 0.9}
    out = srv.build_answer(ans)
    assert out == {"type": "choice", "choice": "code", "probabilities": {"code": 0.9}, "confidence": 0.9}


def test_build_answer_object_attrs_and_values_fallback() -> None:
    """Objeto (não dict) com choice dentro de values e answer_confidence alternativo."""
    ans = SimpleNamespace(values={"choice": "plan"}, probabilities={"plan": 0.8}, answer_confidence=0.8)
    out = srv.build_answer(ans)
    assert out["type"] == "choice" and out["choice"] == "plan" and out["confidence"] == 0.8


def test_build_answer_noul_and_score() -> None:
    assert srv.build_answer({"noul": 0.7}) == {"type": "noul", "noul": 0.7}
    out = srv.build_answer({"score": 3, "probabilities": {"1": 0.1}, "legend": {}, "confidence": 0.5})
    assert out["type"] == "score" and out["score"] == 3


def test_parse_systemone_body_valid() -> None:
    raw = json.dumps({"state": {"x": 1}, "questions": {"q": {"type": "noul", "instructions": "..."}}}).encode()
    body, err = srv.parse_systemone_body(raw)
    assert err is None and body is not None and body["questions"]["q"]["type"] == "noul"


def test_parse_systemone_body_names_the_field_on_type_errors() -> None:
    """O 422 nomeia o campo errado — A5: a mensagem ENSINA a correção."""
    body, err = srv.parse_systemone_body(b"{not json")
    assert body is None and err is not None and "JSON" in err
    body, err = srv.parse_systemone_body(json.dumps({"questions": [{"type": "noul"}]}).encode())
    assert body is None and err is not None and "'questions'" in err
    body, err = srv.parse_systemone_body(json.dumps({"questions": {"q": "oops"}}).encode())
    assert body is None and err is not None and "q" in err
    body, err = srv.parse_systemone_body(b"{}")  # questions ausente → default, não erro
    assert err is None and body == {}


def test_answer_questions_with_fake_agent(monkeypatch: pytest.MonkeyPatch) -> None:
    class FakeAgent:
        def predict(self, states, questions):
            return {"answers": {"q": {"choice": "code", "probabilities": {"code": 0.9}, "confidence": 0.9}}}

    monkeypatch.setattr(srv, "get_agent", lambda: FakeAgent())
    out = srv.answer_questions({"prompt": "x"}, {"q": {"type": "choice", "instructions": "i", "criteria": {"code": None}}})
    assert out["q"]["choice"] == "code"


def test_answer_questions_unknown_type_never_reaches_model(monkeypatch: pytest.MonkeyPatch) -> None:
    seen: dict = {}

    class FakeAgent:
        def predict(self, states, questions):
            seen.update(questions)
            return {"answers": {}}

    monkeypatch.setattr(srv, "get_agent", lambda: FakeAgent())
    out = srv.answer_questions("s", {"bad": {"type": "wat"}})
    assert out == {} and seen == {}
