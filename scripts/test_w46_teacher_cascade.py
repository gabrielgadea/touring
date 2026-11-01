#!/usr/bin/env python3
"""Testes vendados de scripts/w46_teacher_cascade.py (S-5, plan r2).

Jev e professor monkeypatchados — nem rede nem CLI são tocados.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import w46_teacher_cascade as w46  # noqa: E402


def _transcripts(tmp_path: Path) -> Path:
    d = tmp_path / "proj"
    d.mkdir(exist_ok=True)
    rows = [
        {"type": "user", "message": {"content": "como funciona o blast radius do wiring no touring?"}},
        {"type": "user", "message": {"content": "<task-notification>{\"status\":\"completed\"}</task-notification>"}},
        {"type": "user", "message": {"content": "corrija o SIGSEGV no parser do scanner tree-sitter"}},
        {"type": "assistant", "message": {"content": "resposta"}},
        {"type": "user", "message": {"content": "como funciona o blast radius do wiring no touring?"}},  # dup
        {"type": "user", "message": {"content": "curto"}},
    ]
    (d / "sessao.jsonl").write_text("".join(json.dumps(r) + "\n" for r in rows))
    return tmp_path


def test_extract_filters_dedups_and_deterministic(tmp_path: Path) -> None:
    prompts = w46.extract_prompts(_transcripts(tmp_path))
    assert len(prompts) == 2, f"só 2 prompts válidos (machine/dup/curto fora): {list(prompts.values())}"
    again = w46.extract_prompts(_transcripts(tmp_path))
    assert prompts == again, "mesma entrada → mesma saída, sempre"


def test_lgpd_exclusion_by_construction(tmp_path: Path) -> None:
    """LGPD: transcripts do projeto ANTT nem são lidos — o Jev nunca os vê.
    (O nome do teste NÃO pode conter o marcador: o tmp_path deriva dele e a
    guarda lê o caminho inteiro — a primeira versão excluiu até o diretório bom.)"""
    good = tmp_path / "-home-gabrielgadea-projects-touring"
    bad = tmp_path / "-home-gabrielgadea-projects-analise"
    for d in (good, bad):
        d.mkdir()
        (d / "s.jsonl").write_text(json.dumps({"type": "user", "message": {"content": f"prompt vindo de {d.name} com texto suficiente"}}) + "\n")
    prompts = w46.extract_prompts(tmp_path)
    assert len(prompts) == 1 and "touring" in next(iter(prompts.values())), "analise ficou de fora por construção"


def test_sample_is_seeded_and_bounded(tmp_path: Path) -> None:
    prompts = w46.extract_prompts(_transcripts(tmp_path))
    a = w46.sample_prompts(prompts, 5)
    b = w46.sample_prompts(prompts, 5)
    assert a == b, "a seed declarada torna a amostra replayável"
    assert len(a) == 2  # n > universo → universo


def test_cascade_strong_jev_never_calls_professor(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(w46.jev_client, "system_one", lambda state, questions: {
        qid: {"choice": "analysis", "probabilities": {"analysis": 0.9}, "confidence": 0.9} for qid in questions
    })
    called = []
    entries = w46.cascade_label([("aa" * 8, "explique o wiring agora mesmo")], professor=lambda *a: called.append(a))
    assert entries[0]["provenance"] == "jev" and entries[0]["label"] == "analysis"
    assert called == [], "conf ≥ τ → o professor nunca é chamado (o custo vai só na incerteza)"


def test_cascade_escalates_weak_to_professor(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(w46.jev_client, "system_one", lambda state, questions: {
        qid: {"choice": "code", "probabilities": {"code": 0.45}, "confidence": 0.45} for qid in questions
    })
    monkeypatch.setattr(w46, "backend_professor", lambda *a, **kw: {"answer": "debug"})
    entries = w46.cascade_label([("bb" * 8, "corrija o erro no boot agora")])
    assert entries[0]["label"] == "debug" and entries[0]["provenance"] == "professor"
    assert entries[0]["confidence"] == 0.8


def test_cascade_missing_answer_is_excluded_not_invented(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(w46.jev_client, "system_one", lambda *a, **kw: None)
    entries = w46.cascade_label([("cc" * 8, "texto qualquer de teste")], professor=None)
    assert entries == [], "sem resposta → fora do corpus, nunca um rótulo de mentira"


def test_write_corpus_format_and_provenance(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    entries = [{
        "id": "r2:abc", "task": "prompt_intent", "project": "production", "text": "texto de exemplo aqui",
        "label": "plan", "confidence": 0.9, "weak_label": None, "provenance": "jev", "extension_seed": 20261001,
    }]
    result = w46.write_corpus(entries, tmp_path)
    labels = [json.loads(l) for l in (tmp_path / "labels_prompt_intent_r2.jsonl").read_text().splitlines()]
    assert labels[0]["extension"] is True and labels[0]["provenance"] == "jev"
    assert result["written"] == 1 and result["provenance"] == {"jev": 1}
    # o split congelado nunca é tocado
    assert not (tmp_path / "split.json").exists()
