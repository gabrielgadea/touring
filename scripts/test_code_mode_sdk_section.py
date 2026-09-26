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

import pytest

REPO = Path(__file__).resolve().parent.parent
HOOK = REPO / "scripts" / "hooks" / "code_mode_sdk_section.py"
INSTALADA = REPO / "client" / "hooks" / "code_mode_sdk_section.py"
SUGGESTER = REPO / "crates" / "touring-cli" / "src" / "cli_suggester.rs"

spec = importlib.util.spec_from_file_location("code_mode_sdk_section", HOOK)
hook = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hook)

#: A forma do stub que `touring run --sdk-stub` emite (Protocol com métodos tipados).
STUB = (
    "class _Touring(Protocol):\n"
    "    def ast_blast(self, file_path: str) -> Any:\n"
    '        """Full dependency tree (blast radius) for a file."""\n'
    "        ...\n\n"
    "    def memory_recall(self, query: str) -> Any:\n"
    '        """Semantic memory recall."""\n'
    "        ...\n\n"
    "    def query(self, hook: str, payload: dict) -> Any:\n"
    '        """Escape hatch: any hook in the read-only allowlist."""\n'
    "        ...\n"
)


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
    body = hook.section(STUB, "code", "TOURING_CODE_MODE (sessão)")
    assert "apresentação `code`" in body
    assert "TOURING_CODE_MODE (sessão)" in body
    assert "NEGADA" in body  # o efeito do modo code é dito com todas as letras


def test_section_native_says_induction_off():
    body = hook.section(STUB, "native", "default")
    assert "indução desligada" in body


def test_section_has_single_transport_line():
    """Regressão 25/08: a substituição que adicionou a apresentação deixou a
    linha Transporte duplicada — um dump de 2× o custo por sessão."""
    body = hook.section(STUB, "both", "default")
    assert body.count("Transporte:") == 1


def test_section_teaches_the_wall_clock_budget():
    """A cópia instalada ensinava o `--timeout-ms` (28/08) e a testada não; a unificada ensina."""
    assert "--timeout-ms" in hook.section(STUB, "both", "default")


# ── uma cópia só, e a seção inteira sem corte ─────────────────────────────────


def test_the_tested_copy_is_the_installed_source():
    """26/09/2026: ~/.claude/hooks carregava a cópia de client/hooks, que ainda dizia
    «`grep`/`cat`/`find` é NEGADA» quando o executor já deixava a inspeção isolada passar —
    e este guard D8 validava scripts/hooks, a OUTRA cópia. Duas cópias derivam; uma só não."""
    assert HOOK.resolve() == INSTALADA.resolve()


def test_section_indexes_every_sdk_method_from_the_stub_itself():
    """O índice é lido do stub, não transcrito: um método novo no SDK aparece sozinho."""
    body = hook.section(STUB, "code", "default")
    for nome in ("ast_blast", "memory_recall", "query"):
        assert f"`{nome}`" in body, nome
    assert "--sdk-stub" in body  # o contrato tipado inteiro, sob demanda


def test_the_full_stub_is_not_inlined():
    """O stub medido em 26/09 tinha ~3,3 KB e o teto de 4 KB o cortava no meio de uma
    docstring: o modelo recebia um contrato tipado quebrado, e a seção consumia 40% do
    orçamento de 10 KB do SessionStart. A seção traz o índice; o contrato vem sob demanda."""
    body = hook.section(STUB, "code", "default")
    assert "class _Touring" not in body
    assert "Full dependency tree" not in body


def test_real_stub_section_fits_the_ceiling_without_any_cut():
    stub = hook.sdk_stub()
    if stub is None:
        pytest.skip("binário touring indisponível — sem stub real para medir")
    body = hook.section(stub, "code", "default")
    assert len(body.encode()) <= hook.MAX_BYTES
    assert hook.caber(body) == body  # controle: o que cabe atravessa intacto


def test_a_cut_is_declared_at_a_line_boundary():
    """A6: limite de saída explícito, nunca truncamento silencioso no meio de um token."""
    longo = "".join(f"linha {i} " + "x" * 40 + "\n" for i in range(200))
    cortado = hook.caber(longo, teto=600)
    assert len(cortado.encode()) <= 600
    linhas = cortado.rstrip("\n").splitlines()
    assert "cortada" in linhas[-1] and "--sdk-stub" in linhas[-1]
    assert linhas[-2].endswith("x" * 40)  # a última linha mantida está inteira


# ── guard D8 — o texto declara exatamente o que o executor aplica ─────────────


def _rust_collapsed_classes() -> list[str]:
    src = SUGGESTER.read_text(encoding="utf-8")
    m = re.search(
        r'CODE_MODE_COLLAPSED_CLASSES:\s*&\[&str\]\s*=\s*&\[([^\]]+)\]', src
    )
    assert m, "CODE_MODE_COLLAPSED_CLASSES não encontrado no cli_suggester.rs"
    # `[\w-]+`, não `\w+`: a classe `sed-n` tem hífen, e o `\w+` a lia como
    # `sed` — na prática ela era INVISÍVEL a este guard (furo achado no S3,
    # 27/08). Um verificador que casa menos que o extrator mede outra coisa.
    return re.findall(r'"([\w-]+)"', m.group(1))


def _rust_deny_at() -> int:
    src = SUGGESTER.read_text(encoding="utf-8")
    m = re.search(r'INSPECT_BURST_DENY_AT:\s*u32\s*=\s*(\d+)', src)
    assert m, "INSPECT_BURST_DENY_AT não encontrado no cli_suggester.rs"
    return int(m.group(1))


def _rust_window_secs() -> int:
    src = SUGGESTER.read_text(encoding="utf-8")
    m = re.search(r'INSPECT_BURST_WINDOW_SECS:\s*u64\s*=\s*(\d+)', src)
    assert m, "INSPECT_BURST_WINDOW_SECS não encontrado no cli_suggester.rs"
    return int(m.group(1))


def test_d8_declared_classes_match_executor_classes():
    """Se o Rust recalibrar as classes, a seção de sessão ficaria mentindo —
    prometendo colapso de classes que o gate não nega (ou o inverso)."""
    classes = _rust_collapsed_classes()
    assert classes, "executor sem classes colapsadas"
    body = hook.section(STUB, "code", "default")
    efeito = next(l for l in body.splitlines() if l.startswith("Efeito:"))
    for cls in classes:
        assert f"`{cls}`" in efeito, f"classe `{cls}` negada pelo executor mas não declarada"
    promised = set(re.findall(r"`([\w-]+)`", efeito))
    over_promised = promised - set(classes)
    assert not over_promised, f"seção promete colapsar {over_promised} que o executor não nega"


def test_d8_declared_trigger_matches_executor_predicate():
    """S3 — o furo que o teste de classes sozinho NÃO pegava: ele exigia a
    classe PRESENTE na linha, e uma classe listada como "não colapsa" também
    está presente. Presença não é declaração. Aqui o que se compara é o
    GATILHO: se o executor nega a partir da N-ésima chamada em W segundos, a
    seção tem de dizer N e W — os dois números, do executor, no texto."""
    deny_at, window = _rust_deny_at(), _rust_window_secs()
    body = hook.section(STUB, "code", "default")
    efeito = next(l for l in body.splitlines() if l.startswith("Efeito:"))
    ordinal = {2: "2ª", 3: "3ª", 4: "4ª"}.get(deny_at, f"{deny_at}ª")
    assert ordinal in efeito, (
        f"executor nega na {ordinal} chamada, mas a seção não declara esse gatilho: {efeito}"
    )
    assert f"{window}s" in efeito, (
        f"executor usa janela de {window}s, mas a seção não a declara: {efeito}"
    )
    assert "ISOLADA" in efeito.upper(), (
        "o executor deixa a inspeção isolada passar; a seção precisa dizê-lo, "
        "senão o modelo evita uma chamada que teria passado"
    )


# ── main() — fail-open e kill switch ──────────────────────────────────────────


def test_kill_switch_silences(monkeypatch, capsys):
    monkeypatch.setenv("TOURING_SDK_SECTION_DISABLED", "1")
    assert hook.main() == 0
    assert capsys.readouterr().out == ""


def test_main_emits_valid_hook_json(monkeypatch, capsys):
    monkeypatch.delenv("TOURING_SDK_SECTION_DISABLED", raising=False)
    monkeypatch.setattr(hook, "sdk_stub", lambda: STUB)
    monkeypatch.setattr(hook, "presentation", lambda root: ("both", "default"))
    assert hook.main() == 0
    out = json.loads(capsys.readouterr().out)
    assert out["hookSpecificOutput"]["hookEventName"] == "SessionStart"
    contexto = out["hookSpecificOutput"]["additionalContext"]
    assert "`memory_recall`" in contexto and "--sdk-stub" in contexto


def test_main_silent_without_stub(monkeypatch, capsys):
    """Sem transporte não há contrato a anunciar — silêncio, nunca rota morta."""
    monkeypatch.delenv("TOURING_SDK_SECTION_DISABLED", raising=False)
    monkeypatch.setattr(hook, "sdk_stub", lambda: None)
    assert hook.main() == 0
    assert capsys.readouterr().out == ""
