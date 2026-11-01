#!/usr/bin/env python3
"""Testes vendados de scripts/s54_replay.py (cross-audit 2026-10-01 — F3.1).

Corpus sintético em tmp_path; recall vivo e Jev monkeypatchados — nem daemon
nem rede são tocados.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import s54_replay as s54  # noqa: E402


def _corpus(tmp_path: Path) -> None:
    pairs = [
        {"id": "r:1", "query": "q1", "candidate_key": "k1", "text": "t1"},
        {"id": "r:2", "query": "q1", "candidate_key": "k2", "text": "t2"},
        {"id": "r:3", "query": "q2", "candidate_key": "k3", "text": "t3"},
    ]
    labels = [{"id": "r:1", "label": "yes"}, {"id": "r:2", "label": "no"}, {"id": "r:3", "label": "yes"}]
    split = {"sets": {"pairs": {"assignment": {"r:1": "test", "r:2": "test", "r:3": "train"}}}}
    (tmp_path / "pairs.jsonl").write_text("".join(json.dumps(p) + "\n" for p in pairs))
    (tmp_path / "labels_recall_relevance.jsonl").write_text("".join(json.dumps(l) + "\n" for l in labels))
    (tmp_path / "split.json").write_text(json.dumps(split))


def test_load_test_groups_only_test_split(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Só o split de teste entra; grupos por query; ordem determinística por id."""
    _corpus(tmp_path)
    monkeypatch.setattr(s54, "CORPUS", tmp_path)
    groups = s54.load_test_groups()
    assert set(groups) == {"q1"}  # r:3 é train — fora
    assert [c["id"] for c in groups["q1"]] == ["r:1", "r:2"]
    assert groups["q1"][0]["label"] == "yes"


def test_jev_rerank_maps_candidate_key_and_fails_open(monkeypatch: pytest.MonkeyPatch) -> None:
    """P(sim) indexado por candidate_key; adapter None → mapa vazio, nunca exceção."""
    cands = [{"candidate_key": "k1", "text": "a"}, {"candidate_key": "k2", "text": "b"}]
    monkeypatch.setattr(
        s54.jev_client, "system_one",
        lambda state, questions: {"c0": {"noul": 0.9}, "c1": {"noul": 0.2}},
    )
    assert s54.jev_rerank("q", cands) == {"k1": 0.9, "k2": 0.2}
    monkeypatch.setattr(s54.jev_client, "system_one", lambda *a, **kw: None)
    assert s54.jev_rerank("q", cands) == {}


def test_metrics_top1_mrr_and_missing(monkeypatch: pytest.MonkeyPatch) -> None:
    """Aritmética fechada à mão: top1, MRR e o contador de ausência do recall vivo."""
    groups = {
        "q1": [
            {"id": "r:1", "candidate_key": "k1", "text": "t", "label": "yes"},
            {"id": "r:2", "candidate_key": "k2", "text": "t", "label": "no"},
        ],
        "q2": [{"id": "r:3", "candidate_key": "k3", "text": "t", "label": "yes"}],
    }
    # recall vivo: k2 à frente de k1 em q1 (top1 errado, rr=1/2); q2 sem k3 (missing)
    monkeypatch.setattr(s54, "live_recall_ranking", lambda q: {"q1": ["k2", "k1"], "q2": ["kz"]}[q])
    monkeypatch.setattr(s54, "jev_rerank", lambda q, c: {"k1": 0.9, "k2": 0.1} if q == "q1" else {"k3": 0.8})
    m = s54.metrics(groups)
    assert m["n_queries"] == 2 and m["n_yes_pairs"] == 2
    assert m["candidates_missing_from_live_recall"] == 1
    assert m["current"] == {"top1": 0, "mrr": 0.25}  # q1 rr=1/2, q2 rr=0 → 0.5/2
    assert m["jev"] == {"top1": 2, "mrr": 1.0}  # yes em 1º nas duas queries


def test_live_recall_ranking_reads_cases_and_sorts(monkeypatch: pytest.MonkeyPatch) -> None:
    """A ordem vem do payload do recall vivo (cases → score desc); falha do CLI propaga."""
    payload = json.dumps({"cases": {
        "g1": [{"key": "k1", "score": 0.3}, {"key": "k2", "score": 0.9}],
        "g2": "nao-e-lista",  # classe sem grupos é ignorada, não quebra
    }})
    monkeypatch.setattr(s54.subprocess, "run", lambda *a, **kw: SimpleNamespace(stdout=payload))
    assert s54.live_recall_ranking("q") == ["k2", "k1"]


def test_score_gate_injects_only_above_tau(monkeypatch: pytest.MonkeyPatch) -> None:
    """S-7: o gate injeta só score ≥ τ; o share e o top1 saem na métrica."""
    groups = {
        "q1": [
            {"id": "r:1", "candidate_key": "k1", "text": "t", "label": "yes"},
            {"id": "r:2", "candidate_key": "k2", "text": "t", "label": "no"},
            {"id": "r:3", "candidate_key": "k3", "text": "t", "label": "no"},
        ],
    }
    monkeypatch.setattr(s54, "live_recall_ranking", lambda q: ["k1", "k2", "k3"])
    monkeypatch.setattr(s54, "jev_rerank", lambda q, c: {"k1": 0.9, "k2": 0.1, "k3": 0.2})
    monkeypatch.setattr(s54, "jev_score_gate", lambda q, c: {"k1": 2.0, "k2": 1.0, "k3": 0.0})
    m = s54.metrics(groups, gate_tau=1.0)
    sg = m["score_gate"]
    assert sg["injected_share"] == round(2 / 3, 4), "k1(2,0) e k2(1,0) passam; k3(0,0) fica fora"
    assert sg["top1"] == 1, "o primeiro injetado é o relevante"
    assert sg["mrr"] == 1.0


def test_main_end_to_end_blinded(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """main() inteiro vendado: corpus fixture + recall/jev fakes → results.json no OUT."""
    _corpus(tmp_path)
    monkeypatch.setattr(s54, "CORPUS", tmp_path)
    out = tmp_path / "out"
    monkeypatch.setattr(s54, "OUT", out)
    monkeypatch.setattr(s54, "live_recall_ranking", lambda q: ["k1", "k2"])
    monkeypatch.setattr(s54, "jev_rerank", lambda q, c: {"k1": 0.9, "k2": 0.1})
    monkeypatch.setattr(s54, "jev_score_gate", lambda q, c: {"k1": 2.0, "k2": 0.0})
    assert s54.main() == 0
    results = json.loads((out / "results.json").read_text())
    assert results["n_queries"] == 1 and results["n_yes_pairs"] == 1
    assert results["jev"]["top1"] == 1
