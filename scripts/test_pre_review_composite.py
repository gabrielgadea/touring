#!/usr/bin/env python3
"""Testes do composite scoring de scripts/pre_review_triage.py (S-6, plano r2).

"O modelo pontua, o código decide": os pesos vivem em config e o composite é
computado em código — trocar peso muda o veredito SEM re-chamar o modelo.
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import pre_review_triage as triage_mod  # noqa: E402


def _fake_answers():
    base = {qid: {"noul": 0.05} for qid in triage_mod.TRIAGE_QUESTIONS}
    return base | {
        "security_risk": {"score": 2.0},
        "pattern_alignment": {"score": 1.0},
        "diff_quality": {"score": 0.0},
    }


def test_compose_composite_is_pure_and_weight_driven() -> None:
    """Mesmas respostas, pesos diferentes → composite diferente, sem modelo."""
    scores = {"security_risk": 2.0, "pattern_alignment": 1.0, "diff_quality": 0.0}
    heavy_security = triage_mod.compose_composite(scores, {"security_risk": 0.9, "pattern_alignment": 0.05, "diff_quality": 0.05})
    heavy_quality = triage_mod.compose_composite(scores, {"security_risk": 0.05, "pattern_alignment": 0.05, "diff_quality": 0.9})
    assert heavy_security is not None and heavy_quality is not None
    assert heavy_security > heavy_quality, "o peso move o composite — tuning por número"
    assert triage_mod.compose_composite({}, triage_mod.DEFAULT_WEIGHTS) is None  # sem scores → declarado


def test_triage_carries_scores_composite_and_weights(monkeypatch: pytest.MonkeyPatch) -> None:
    calls = []
    monkeypatch.setattr(triage_mod.jev_client, "system_one", lambda s, q: calls.append(q) or _fake_answers())
    result = triage_mod.triage("diff qualquer")
    assert result["scores"] == {"security_risk": 2.0, "pattern_alignment": 1.0, "diff_quality": 0.0}
    assert result["composite"] is not None and result["weights"] == triage_mod.DEFAULT_WEIGHTS
    questions = calls[0]
    assert len(questions) == 10, "7 Nouls + 3 Scores na MESMA chamada (o fan-out da doc)"
    assert questions["security_risk"]["type"] == "score" and len(questions["security_risk"]["criteria"]) == 3


def test_retuning_never_recalls_the_model(monkeypatch: pytest.MonkeyPatch) -> None:
    """A prova do padrão: mudar os pesos e re-computar NÃO gera nova chamada."""
    calls = []
    monkeypatch.setattr(triage_mod.jev_client, "system_one", lambda s, q: calls.append(1) or _fake_answers())
    first = triage_mod.triage("diff", weights={"security_risk": 0.9, "pattern_alignment": 0.05, "diff_quality": 0.05})
    second = triage_mod.compose_composite(first["scores"], {"security_risk": 0.05, "pattern_alignment": 0.05, "diff_quality": 0.9})
    assert len(calls) == 1, "o re-tuning aconteceu sobre as respostas já medidas — zero re-prompt"
    assert first["composite"] != second
