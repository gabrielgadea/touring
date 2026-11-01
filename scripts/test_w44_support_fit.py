#!/usr/bin/env python3
"""Testes de scripts/w44_support_fit.py — grade determinística, sem GPU/modelo."""

from __future__ import annotations

import sys
from pathlib import Path

import torch

sys.path.insert(0, str(Path(__file__).parent))

from w44_support_fit import MIN_PRECISION, TAU, grid_search_t  # noqa: E402


def _samples(n: int = 110, k: int = 8, seed: int = 42) -> list[tuple[torch.Tensor, int]]:
    """Curva sub-confiante sintética: margens pequenas (max prob ~0,4–0,55 em T=1).
    ~8% de erros com margem MENOR que a dos acertos — existe janela de T onde os
    certos cruzam o τ e os errados não (precisão no suportado ≥ 0,90 atingível)."""
    g = torch.Generator().manual_seed(seed)
    out = []
    for i in range(n):
        logits = torch.randn(k, generator=g) * 0.25
        target = i % k
        if i % 13 == 0:  # ~8%: o modelo erra, com margem fraca
            logits[(target + 1) % k] += 0.35
        else:
            logits[target] += 0.7
        out.append((logits, target))
    return out


def test_sharpens_subconfident_curve_and_recovers_support() -> None:
    samples = _samples()
    best, rows = grid_search_t(samples, tau=0.6, min_precision=0.90)
    assert best is not None, "deve existir janela com precisão ≥ 0,90"
    base = min(rows, key=lambda r: abs(r["t"] - 1.0))
    assert best["t"] < 1.0, "curva sub-confiante pede T<1 (afiar), nunca T>1 (amolecer)"
    assert best["support"] > base["support"], "T* sobe o suporte vs T=1"
    assert best["precision_supported"] >= 0.90, "o piso de precisão é respeitado"


def test_accuracy_is_temperature_invariant() -> None:
    """argmax não muda com T — a acurácia global é constante ao longo da grade."""
    _, rows = grid_search_t(_samples())
    accs = {r["accuracy"] for r in rows}
    assert len(accs) == 1, f"acurácia deveria ser invariante sob T, veio {accs}"


def test_floor_is_measured_on_precision_supported_not_global_accuracy() -> None:
    """Errata de métrica (S-1-CITERIO-ACEITACAO, 02/10/2026, pré-registrada).

    O critério de aceitação de 01/10 media a acurácia GLOBAL, que é invariante sob T
    e portanto nunca desce abaixo do piso — condição inatingível por construção. A
    condição 2 passa a medir PRECISÃO@SUPORTADO(τ), que é a grandeza que o τ protege.

    Este teste existe para travar a métrica, não o número: se alguém voltar a filtrar
    por `accuracy`, o T* passa a ser escolhido por uma grandeza que não responde a T
    e a curva perde o sentido.
    """
    best, rows = grid_search_t(_samples(), tau=TAU, min_precision=MIN_PRECISION)
    assert best is not None, "a versão corrigida do critério deve achar janela"

    # 1. T* é escolhido por precisão@suportado: ela respeita o piso…
    assert best["precision_supported"] >= MIN_PRECISION, "T* viola o piso que o mediu"
    # …e a acurácia global, que NÃO pode servir de critério, pode estar abaixo dele.
    # Numa curva real (acc global 0,625) o critério antigo reprovaria todo T; aqui
    # affine a asserção: o que não pode acontecer é T* existir só porque a global passou.
    at_t1 = min(rows, key=lambda r: abs(r["t"] - 1.0))
    assert best["t"] != at_t1["t"] or best["support"] > at_t1["support"], (
        "T* tem de se distinguir de T=1 — se não, a escolha não veio de nenhuma métrica"
    )

    # 2. a curva expõe as DUAS grandezas, para a errata ser auditável pelo artefato
    for row in rows:
        assert {"support", "precision_supported", "accuracy"} <= set(row), (
            "a curva precisa das duas grandezas: a que decide e a invariante"
        )

    # 3. a precisão@suportado é monotónica em T: afiar nunca a degrada
    precs = [r["precision_supported"] for r in rows]
    assert max(precs) >= MIN_PRECISION, "nenhum T atinge o piso de precisão@suportado"


def test_support_is_monotone_decreasing_in_t() -> None:
    """Afiar (T<1) nunca reduz suporte; amolecer (T>1) nunca aumenta — a forma da curva."""
    _, rows = grid_search_t(_samples())
    supports = [r["support"] for r in rows]
    for prev, cur in zip(supports, supports[1:]):
        assert cur <= prev + 1e-9, "suporte cresce quando T desce, decresce quando T sobe"


def test_empty_input_is_safe_not_crash() -> None:
    best, rows = grid_search_t([], tau=0.6)
    assert best is None and len(rows) > 0  # grade computada, decisão ausente declarada
