#!/usr/bin/env python3
"""Testes do hook SessionStart `code_mode_sdk_section.py` + guard D8 cruzado.

O guard D8 (rules/touring-4-pillars.md): o texto declarado ao modelo e o
predicado do executor devem derivar da MESMA fonte — o prompt nunca promete o
que o executor não aplica. Aqui as duas fontes são arquivos distintos (o hook
Python declara as classes que o gate Rust nega), então o guard as reconcilia
lendo `CODE_MODE_COLLAPSED_CLASSES` do cli_suggester.rs e exigindo que o
efeito `code` da seção mencione exatamente essas classes.
"""

from __future__ import annotations

import importlib.util
import json
import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
HOOK = REPO / "scripts" / "hooks" / "code_mode_sdk_section.py"
SUGGESTER = REPO / "crates" / "touring-cli" / "src" / "cli_suggester.rs"

spec = importlib.util.spec_from_file_location("code_mode_sdk_section", HOOK)
hook = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hook)


# ── presentation() — resolução por escopo ─────────────────────────────────────


def test_env_session_wins_over_project(tmp_path, monkeypatch):
    (tmp_path / ".touring").mkdir()
    (tmp_path / ".touring" / "touring.toml").write_text('[code_mode]\nmode = "code"\n')
    monkeypatch.setenv("TOURING_CODE_MODE", "native")
    monkeypatch.delenv("TOURING_CODE_ONLY", raising=False)
    modo, origem = hook.presentation(str(tmp_path))
    assert modo == "native"
    assert "sessão" in origem


def test_alias_code_only_maps_to_code(tmp_path, monkeypatch):
    monkeypatch.delenv("TOURING_CODE_MODE", raising=False)
    monkeypatch.setenv("TOURING_CODE_ONLY", "1")
    modo, origem = hook.presentation(str(tmp_path))
    assert modo == "code"
    assert "alias" in origem


def test_project_toml_is_read(tmp_path, monkeypatch):
    (tmp_path / ".touring").mkdir()
    (tmp_path / ".touring" / "touring.toml").write_text('[code_mode]\nmode = "code"\n')
    monkeypatch.delenv("TOURING_CODE_MODE", raising=False)
    monkeypatch.delenv("TOURING_CODE_ONLY", raising=False)
    modo, origem = hook.presentation(str(tmp_path))
    assert modo == "code"
    assert "projeto" in origem


def test_malformed_toml_fails_closed_to_default(tmp_path, monkeypatch):
    """Qualquer forma fora da esperada cai no default — nunca num colapso não pedido."""
    (tmp_path / ".touring").mkdir()
    (tmp_path / ".touring" / "touring.toml").write_text('[code_mode]\nmode = "explode"\n')
    monkeypatch.delenv("TOURING_CODE_MODE", raising=False)
    monkeypatch.delenv("TOURING_CODE_ONLY", raising=False)
    modo, origem = hook.presentation(str(tmp_path))
    assert modo == "both"
    assert origem == "default"


def test_default_when_nothing_declares(tmp_path, monkeypatch):
    monkeypatch.delenv("TOURING_CODE_MODE", raising=False)
    monkeypatch.delenv("TOURING_CODE_ONLY", raising=False)
    modo, origem = hook.presentation(str(tmp_path))
    assert (modo, origem) == ("both", "default")


# ── section() — a seção declara a apresentação ────────────────────────────────


def test_section_declares_mode_and_origin():
    body = hook.section("STUB", "code", "TOURING_CODE_MODE (sessão)")
    assert "apresentação `code`" in body
    assert "TOURING_CODE_MODE (sessão)" in body
    assert "NEGADA" in body  # o efeito do modo code é dito com todas as letras


def test_section_native_says_induction_off():
    body = hook.section("STUB", "native", "default")
    assert "indução desligada" in body


def test_section_has_single_transport_line():
    """Regressão 25/08: a substituição que adicionou a apresentação deixou a
    linha Transporte duplicada — um dump de 2× o custo por sessão."""
    body = hook.section("STUB", "both", "default")
    assert body.count("Transporte:") == 1


# ── guard D8 — o texto declara exatamente o que o executor aplica ─────────────


def _rust_collapsed_classes() -> list[str]:
    src = SUGGESTER.read_text(encoding="utf-8")
    m = re.search(
        r'CODE_MODE_COLLAPSED_CLASSES:\s*&\[&str\]\s*=\s*&\[([^\]]+)\]', src
    )
    assert m, "CODE_MODE_COLLAPSED_CLASSES não encontrado no cli_suggester.rs"
    return re.findall(r'"(\w+)"', m.group(1))


def test_d8_declared_classes_match_executor_classes():
    """Se o Rust recalibrar as classes, a seção de sessão ficaria mentindo —
    prometendo colapso de classes que o gate não nega (ou o inverso)."""
    classes = _rust_collapsed_classes()
    assert classes, "executor sem classes colapsadas"
    body = hook.section("STUB", "code", "default")
    efeito = next(l for l in body.splitlines() if l.startswith("Efeito:"))
    for cls in classes:
        assert f"`{cls}`" in efeito, f"classe `{cls}` negada pelo executor mas não declarada"
    promised = set(re.findall(r"`(\w+)`", efeito))
    non_collapsed = {"ls", "wc", "sed"}  # declaradas como NÃO-colapsadas
    over_promised = promised - set(classes) - non_collapsed
    assert not over_promised, f"seção promete colapsar {over_promised} que o executor não nega"


# ── main() — fail-open e kill switch ──────────────────────────────────────────


def test_kill_switch_silences(monkeypatch, capsys):
    monkeypatch.setenv("TOURING_SDK_SECTION_DISABLED", "1")
    assert hook.main() == 0
    assert capsys.readouterr().out == ""


def test_main_emits_valid_hook_json(monkeypatch, capsys):
    monkeypatch.delenv("TOURING_SDK_SECTION_DISABLED", raising=False)
    monkeypatch.setattr(hook, "sdk_stub", lambda: "STUB")
    monkeypatch.setattr(hook, "presentation", lambda root: ("both", "default"))
    assert hook.main() == 0
    out = json.loads(capsys.readouterr().out)
    assert out["hookSpecificOutput"]["hookEventName"] == "SessionStart"
    assert "STUB" in out["hookSpecificOutput"]["additionalContext"]


def test_main_silent_without_stub(monkeypatch, capsys):
    """Sem transporte não há contrato a anunciar — silêncio, nunca rota morta."""
    monkeypatch.delenv("TOURING_SDK_SECTION_DISABLED", raising=False)
    monkeypatch.setattr(hook, "sdk_stub", lambda: None)
    assert hook.main() == 0
    assert capsys.readouterr().out == ""
