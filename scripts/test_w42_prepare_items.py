#!/usr/bin/env python3
"""Teste do invariante de taxonomia (S-9, plan r2): ≤20 rótulos flat ou falha ALTO."""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))

from w42_prepare_items import MAX_FLAT_LABELS, check_flat_label_limit  # noqa: E402


def test_current_classes_are_within_limit() -> None:
    check_flat_label_limit(8)  # as classes de produção — nunca levanta


def test_boundary_twenty_passes_twentyone_fails_loud() -> None:
    check_flat_label_limit(MAX_FLAT_LABELS)  # exatamente no teto: passa
    with pytest.raises(ValueError, match="beam search paralelo"):
        check_flat_label_limit(MAX_FLAT_LABELS + 1)  # a rota hierárquica é nomeada
