#!/usr/bin/env python3
"""Guard do minerador REFINE — o bug de instrumento achado pelo ADW skill-refine
(2026-08-28): o corpo do SKILL.md ecoado no transcript contava como correcao do
usuario, inflando correction_rate proporcionalmente ao tamanho da skill."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import mine_transcripts as mt  # noqa: E402


def test_skill_echo_is_not_a_user_correction():
    body = mt._normalize(
        "## Regras\nNunca aplique blind. Isso esta errado quando o gate falha; "
        "na verdade o fluxo deve rotear cada licao ao destino certo antes de "
        "propor o diff, de novo e de novo ate o quality gate passar limpo."
    )
    echoed = (
        "Nunca aplique blind. Isso esta errado quando o gate falha; na verdade "
        "o fluxo deve rotear cada licao ao destino certo antes de propor o diff"
    )
    assert mt.is_skill_echo(echoed, body), "eco literal do corpo deve ser descartado"


def test_genuine_user_correction_still_counts():
    body = mt._normalize("## Regras\nNunca aplique blind.")
    genuine = (
        "nao, isso esta errado — o minerador deveria ignorar a janela onde o "
        "texto e eco da propria skill, corrige isso antes de reportar o rate"
    )
    assert not mt.is_skill_echo(genuine, body), "correcao genuina nunca e eco"
    lowered = genuine.lower()
    assert any(m in lowered for m in mt.CORRECTION_MARKERS)


def test_short_messages_are_never_classified_as_echo():
    assert not mt.is_skill_echo("nao, errado", mt._normalize("nao, errado " * 50))


def test_missing_skill_body_fails_open():
    assert mt.skill_body("skill-que-nao-existe-xyz") == ""
    assert not mt.is_skill_echo("qualquer texto longo " * 10, "")
