#!/usr/bin/env python3
"""Testes vendados de scripts/g2_calibration.py (cross-audit 2026-10-01 — F3.1).

recommend_tau é puro; curve_for_choice usa jev_client monkeypatchado — a rede
nunca é tocada (padrão dos cookbooks oficiais de replay vendado).
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import g2_calibration as gc  # noqa: E402


def _bin(lo: float, hi: float, n: int, support: float, acc: float) -> dict:
    return {"range": [lo, hi], "n": n, "support": support, "accuracy": acc, "mean_conf": (lo + min(hi, 1.0)) / 2}


def test_recommend_tau_highest_qualifying_bin_wins() -> None:
    """Duas faixas fecham; a MAIS confiante é a escolhida (varredura de cima para baixo)."""
    bins = [_bin(0.4, 0.6, 20, 0.2, 0.9), _bin(0.6, 0.8, 30, 0.3, 0.95)]
    tau, reason = gc.recommend_tau(bins)
    assert tau == 0.6
    assert "acc=0.95" in reason


def test_recommend_tau_none_when_support_fails() -> None:
    """Acurácia alta com suporte 5% < 15% → null + razão nomeada (nunca τ de mentira)."""
    bins = [_bin(0.8, 1.0, 3, 0.05, 0.99)]
    tau, reason = gc.recommend_tau(bins)
    assert tau is None
    assert "revisão humana" in reason


def test_recommend_tau_none_when_accuracy_fails() -> None:
    bins = [_bin(0.6, 0.8, 50, 0.5, 0.7)]
    tau, _reason = gc.recommend_tau(bins)
    assert tau is None


def test_curve_for_choice_buckets_skips_and_prob_fallback(monkeypatch: pytest.MonkeyPatch) -> None:
    """Bucketing por faixa, confiança derivada de max(probabilities) quando o campo
    falta, e item sem resposta (None) FORA da média — nunca zero disfarçado."""
    items = [{"id": "a"}, {"id": "b"}, {"id": "c"}, {"id": "d"}]
    spec = {
        "qid": "q",
        "question": "pick",
        "state": lambda it: {"id": it["id"]},
        "gold": lambda it: {"a": "x", "b": "x", "c": "y", "d": "y"}[it["id"]],
    }
    answers = {
        "a": {"q": {"choice": "x", "confidence": 0.9}},                     # certo, bin 0.8-1.0
        "b": {"q": {"choice": "y", "confidence": 0.7}},                     # errado, bin 0.6-0.8
        "c": {"q": {"choice": "y", "probabilities": {"y": 0.5, "x": 0.5}}},  # conf via max(probs), bin 0.4-0.6
        # "d": ausente → system_one devolve None → skipped
    }
    monkeypatch.setattr(gc.jev_client, "system_one", lambda state, questions: answers.get(state["id"]))
    bins = gc.curve_for_choice(items, spec)
    by_lo = {b["range"][0]: b for b in bins}
    assert by_lo[0.8]["accuracy"] == 1.0 and by_lo[0.8]["n"] == 1
    assert by_lo[0.6]["accuracy"] == 0.0
    assert by_lo[0.4]["n"] == 1 and by_lo[0.4]["mean_conf"] == 0.5
    assert sum(b["n"] for b in bins) == 3  # o quarto ficou de fora
    # support é round(1/3, 4) por faixa — a soma é 0.9999, não 1.0 (o código arredonda
    # por desenho; o teste não pode exigir precisão que o contrato não promete)
    assert abs(sum(b["support"] for b in bins) - 1.0) < 0.001
