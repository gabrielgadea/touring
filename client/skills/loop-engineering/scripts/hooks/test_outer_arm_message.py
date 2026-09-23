#!/usr/bin/env python3
"""A mensagem do arm só pode afirmar o que o executor de fato fez.

Medido em 22/09/2026: com o flow `strategy-outer` o marcador nasce SEM bundle, o
código corretamente não dispara `spawn_outer_artifacts` — e a mensagem injetada
afirmava, mesmo assim, que "o diagnóstico determinístico e o ledger CCE já foram
DISPARADOS em background (bundle: <plan bundle dir>)". Texto e executor divergindo
é o antipadrão D8 do próprio repositório: quem lê acredita e não roda o que falta.

Dois casos, um por ramo:
  · sem bundle  → a mensagem NÃO promete artefato e NOMEIA o comando que falta;
  · com bundle  → a mensagem promete, e o executor foi mesmo chamado.
"""
from __future__ import annotations

import importlib.util
import io
import json
import sys
from pathlib import Path

import pytest

HOOK = Path(__file__).with_name("loop_outer_arm.py")


def load_module():
    spec = importlib.util.spec_from_file_location("loop_outer_arm_under_test", HOOK)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run_arm(monkeypatch, capsys, *, bundle, flow="strategy-outer"):
    """Roda main() com o marcador controlado; devolve (contexto, spawns)."""
    mod = load_module()
    spawns = []
    monkeypatch.setattr(mod, "detect_flow", lambda prompt: flow)
    monkeypatch.setattr(mod, "arm", lambda *a, **k: {"flow": flow, "bundle": bundle})
    monkeypatch.setattr(mod, "active_marker", lambda *a, **k: (None, {"bundle": bundle}))
    monkeypatch.setattr(mod, "spawn_outer_artifacts",
                        lambda cwd, b, topic: spawns.append((cwd, b, topic)))
    monkeypatch.setattr(sys, "stdin", io.StringIO(json.dumps(
        {"prompt": "/loop-engineering analise profunda do módulo X",
         "cwd": "/tmp/projeto", "session_id": "s1"})))
    assert mod.main() == 0
    return capsys.readouterr().out, spawns


def test_without_a_bundle_the_message_promises_nothing_and_names_the_command(monkeypatch, capsys):
    out, spawns = run_arm(monkeypatch, capsys, bundle=None)
    assert spawns == [], "sem bundle não há o que disparar"
    assert "<plan bundle dir>" not in out, "um placeholder não é um caminho"
    assert "DISPARADOS" not in out.upper(), (
        "a mensagem não pode afirmar disparo que não houve"
    )
    assert "strategy-loop" in out or "loop_diagnose" in out, (
        "quando nada foi disparado, a mensagem tem de nomear o que falta rodar"
    )


def test_with_a_bundle_the_message_matches_what_the_executor_did(monkeypatch, capsys):
    out, spawns = run_arm(monkeypatch, capsys, bundle="/tmp/projeto/docs/plans/2026-09-22-x")
    assert len(spawns) == 1, "com bundle, o executor roda"
    assert "/tmp/projeto/docs/plans/2026-09-22-x" in out
    assert "DISPARADOS" in out.upper()


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
