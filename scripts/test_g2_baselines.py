#!/usr/bin/env python3
"""Testes vendados de scripts/g2_baselines.py (S-2.1).

Corpus sintético em tmp_path; Jev e professor monkeypatchados; rede nunca é tocada.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import g2_baselines as g2  # noqa: E402


@pytest.fixture()
def corpus(tmp_path: Path) -> Path:
    """Corpus mínimo: 4 prompts (3 analysis + 1 plan) e 2 pares, todos em test."""
    prompts = [
        {"id": f"p:{i:011x}", "task": "prompt_intent", "project": "t", "text": f"texto {i}"}
        for i in range(4)
    ]
    labels = [
        {"id": "p:00000000000", "label": "analysis", "confidence": 0.9, "weak_label": None},
        {"id": "p:00000000001", "label": "analysis", "confidence": 0.9, "weak_label": None},
        {"id": "p:00000000002", "label": "plan", "confidence": 0.9, "weak_label": None},
        {"id": "p:00000000003", "label": "analysis", "confidence": 0.9, "weak_label": None},
    ]
    assignment = {p["id"]: "test" for p in prompts}
    split = {
        "split_seed": 1,
        "sets": {
            "prompts": {"assignment": assignment, "counts": {"test": 4}},
            "memories": {"assignment": {}, "counts": {"test": 0}},
            "pairs": {"assignment": {}, "counts": {"test": 0}},
        },
    }
    (tmp_path / "prompts.jsonl").write_text("".join(json.dumps(p) + "\n" for p in prompts))
    (tmp_path / "labels_prompt_intent.jsonl").write_text("".join(json.dumps(l) + "\n" for l in labels))
    for empty in ("memories.jsonl", "pairs.jsonl", "labels_memory_kind.jsonl", "labels_recall_relevance.jsonl"):
        (tmp_path / empty).write_text("")
    (tmp_path / "split.json").write_text(json.dumps(split))
    return tmp_path


def test_test_items_only_test_split(corpus: Path) -> None:
    data = g2.load_corpus(corpus)
    items = g2.test_items(data, "prompts", "prompts", "labels_intent")
    assert [i["id"] for i in items] == sorted(i["id"] for i in items)  # ordem determinística
    assert len(items) == 4


def test_ece_computed_per_question() -> None:
    """ECE perfeito (0) quando prob prevê o outcome; None quando não há probs."""
    perfect = [(1.0, True), (0.0, False)] * 5
    assert g2.ece(perfect) == 0.0
    assert g2.ece([]) is None
    worst = [(1.0, False), (0.0, True)] * 5
    assert g2.ece(worst) == 1.0


def test_table_covers_backends(corpus: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """A tabela tem uma entrada por backend pedido, mesmo com células skipped."""
    monkeypatch.setattr(
        g2.jev_client,
        "system_one",
        lambda state, questions, **kw: {
            "intent": {"type": "choice", "choice": "analysis", "probabilities": {"analysis": 0.9, "plan": 0.1}, "confidence": 0.9}
        },
    )
    monkeypatch.setattr(g2, "backend_current_intent", lambda text: "analysis")
    data = g2.load_corpus(corpus)
    items = g2.test_items(data, "prompts", "prompts", "labels_intent")
    spec = g2.CHOICE_TASKS["prompt_intent"]
    table = g2.run_choice_task(spec, items, ["current", "jev", "laya"])
    assert set(table) == {"current", "jev", "laya"}
    assert table["jev"]["accuracy"] == 0.75  # 3 analysis certos, 1 plan errado
    assert table["jev"]["ece"] is not None
    assert table["current"]["ece"] is None  # keyword não tem probabilidade
    assert table["laya"]["skipped"] is True  # pacote ausente → skipped, nunca inventado


def test_jev_skipped_without_key(corpus: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Sem chave, o adapter devolve None → célula fora da média, skip documentado."""
    monkeypatch.setattr(g2.jev_client, "system_one", lambda *a, **kw: None)
    monkeypatch.setattr(g2, "backend_current_intent", lambda text: "analysis")
    data = g2.load_corpus(corpus)
    items = g2.test_items(data, "prompts", "prompts", "labels_intent")
    table = g2.run_choice_task(g2.CHOICE_TASKS["prompt_intent"], items, ["current", "jev"])
    assert table["jev"]["skipped"] is True
    assert table["jev"]["accuracy"] is None
    assert table["current"]["accuracy"] == 0.75


def test_results_written(corpus: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(g2, "backend_current_intent", lambda text: "analysis")
    monkeypatch.setattr(g2.jev_client, "system_one", lambda *a, **kw: None)
    monkeypatch.setattr(sys, "argv", ["g2_baselines.py", "--corpus", str(corpus), "--tasks", "prompt_intent", "--backends", "current,jev", "--out", str(tmp_path / "out")])
    assert g2.main() == 0
    results = json.loads((tmp_path / "out" / "results.json").read_text())
    assert results["tasks"]["prompt_intent"]["n_test"] == 4
    assert "split_hash" in results


# ---------------------------------------------------------------- cross-audit 2026-10-01 (F3.1)


def test_load_jsonl_skips_blank_lines(tmp_path: Path) -> None:
    p = tmp_path / "x.jsonl"
    p.write_text('{"a": 1}\n\n{"a": 2}\n')
    assert g2.load_jsonl(p) == [{"a": 1}, {"a": 2}]


def test_backend_current_intent_parses_mode_header(monkeypatch: pytest.MonkeyPatch) -> None:
    """O parser lê o header MODE do shim de produção — o instrumento correto (S-2.1)."""
    ctx = "prefixo\n[PROMPT ENHANCEMENT -- DEBUG MODE]\nsufixo"
    payload = json.dumps({"hookSpecificOutput": {"additionalContext": ctx}})
    monkeypatch.setattr(g2.subprocess, "run", lambda *a, **kw: SimpleNamespace(stdout=payload))
    assert g2.backend_current_intent("stack trace ao subir") == "debug"


def test_backend_current_intent_without_header_is_loud(monkeypatch: pytest.MonkeyPatch) -> None:
    """Sem o header MODE o instrumento falha ALTO (ValueError), nunca majestade silenciosa."""
    payload = json.dumps({"hookSpecificOutput": {"additionalContext": "sem header"}})
    monkeypatch.setattr(g2.subprocess, "run", lambda *a, **kw: SimpleNamespace(stdout=payload))
    with pytest.raises(ValueError, match="MODE header ausente"):
        g2.backend_current_intent("x")


def test_backend_professor_extracts_json_and_fails_open(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(g2.subprocess, "run", lambda *a, **kw: SimpleNamespace(stdout='lixo {"answer": "plan"} fim'))
    assert g2.backend_professor("s", "q", ["plan"]) == {"answer": "plan"}
    monkeypatch.setattr(g2.subprocess, "run", lambda *a, **kw: SimpleNamespace(stdout="sem json"))
    assert g2.backend_professor("s", "q", None) is None

    def boom(*a, **kw):
        raise g2.subprocess.TimeoutExpired(cmd="claude", timeout=1)

    monkeypatch.setattr(g2.subprocess, "run", boom)
    assert g2.backend_professor("s", "q", None) is None


def test_backend_laya_skips_documented(monkeypatch: pytest.MonkeyPatch) -> None:
    """Sem o pacote laya, a célula é skipped DECLARADO — nunca inventada."""
    monkeypatch.setitem(sys.modules, "laya", None)  # import forçado a ImportError
    assert g2.backend_laya({}, {}) == {"skipped": "laya_not_installed"}


def test_predict_choice_all_backends(monkeypatch: pytest.MonkeyPatch) -> None:
    q = {"intent": {"type": "choice", "instructions": "i", "criteria": {"a": None}}}
    # current com weak_label (memory_kind): a tag registrada vence sem chamar nada
    assert g2.predict_choice("current", {}, q, "intent", ["a"], "p:00000000000", 10, "txt", "q", weak_label="lesson") == ("lesson", None)
    monkeypatch.setattr(g2, "backend_current_intent", lambda t: "code")
    assert g2.predict_choice("current", {}, q, "intent", ["a"], "p:00000000000", 10, "txt", "q") == ("code", None)
    monkeypatch.setattr(g2.jev_client, "system_one", lambda s, qq: {"intent": {"choice": "a", "probabilities": {"a": 0.7}}})
    assert g2.predict_choice("jev", {}, q, "intent", ["a"], "p:00000000000", 10, "txt", "q") == ("a", 0.7)
    monkeypatch.setattr(g2.jev_client, "system_one", lambda *a, **kw: None)
    assert g2.predict_choice("jev", {}, q, "intent", ["a"], "p:00000000000", 10, "txt", "q") == (None, None)
    # professor fora da subamostra: célula fora da média, sem chamada
    assert g2.predict_choice("professor", {}, q, "intent", ["a"], "p:00000000001", 100, "txt", "q") == (None, None)
    # laya skipped → fora da média
    monkeypatch.setitem(sys.modules, "laya", None)
    assert g2.predict_choice("laya", {}, q, "intent", ["a"], "p:00000000000", 10, "txt", "q") == (None, None)


def test_predict_noul_contracts(monkeypatch: pytest.MonkeyPatch) -> None:
    # current: baseline por item não existe no G1 — unmeasured declarado, nunca inventado
    assert g2.predict_noul("current", "q", "c", "p:00000000000", 10) == (None, None)
    monkeypatch.setattr(g2.jev_client, "system_one", lambda s, qq: {"relevant": {"noul": 0.8}})
    assert g2.predict_noul("jev", "q", "c", "p:00000000000", 10) == (True, 0.8)
    monkeypatch.setattr(g2.jev_client, "system_one", lambda *a, **kw: None)
    assert g2.predict_noul("jev", "q", "c", "p:00000000000", 10) == (None, None)
    # professor fora da subamostra não chama
    assert g2.predict_noul("professor", "q", "c", "p:00000000001", 100) == (None, None)


def test_professor_subsample_deterministic_and_real_subset() -> None:
    picks = [g2._in_professor_subsample(f"p:{i:011x}", 100) for i in range(100)]
    assert picks == [g2._in_professor_subsample(f"p:{i:011x}", 100) for i in range(100)]
    assert 0 < sum(picks) < 100  # subamostra real: nem todos, nem nenhum


def test_summarize_empty_is_declared_skip_and_mixed_math() -> None:
    out = g2.summarize([])
    assert out == {"n": 0, "skipped": True, "accuracy": None, "ece": None, "p50_ms": None, "p99_ms": None, "input_tokens": None}
    cells = [
        {"correct": True, "prob": 0.9, "latency_ms": 10, "input_tokens": 5},
        {"correct": False, "prob": None, "latency_ms": 30, "input_tokens": 7},
    ]
    out = g2.summarize(cells)
    assert out["accuracy"] == 0.5 and out["p50_ms"] == 20.0 and out["input_tokens"] == 12
    assert out["skipped"] is False
    assert abs(out["ece"] - 0.1) < 1e-9  # um item (0.9, certo): |acc-conf| = 0.1
