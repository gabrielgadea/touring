#!/usr/bin/env python3
# touring-quality:allow-secrets
"""Testes vendados de scripts/jev_client.py — replay sem API (padrão fita dos cookbooks).

Plano S-0.2: nenhum teste chama a API real; o client é injetado (fake).
As "chaves" deste arquivo são FIXTURES de teste, construídas em runtime para
não carregar padrão de credencial de vendor no texto-fonte (CWE-798).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import jev_client  # noqa: E402

FIXTURE_KEY = "-".join(["fixture", "not", "real", "0001"])  # valor de teste, não credencial
LEAK_SENTINEL = "SENTINEL-" + "Z" * 24  # construído em runtime: prova de não-vazamento


class FakeClient:
    """Client de mentira: conta chamadas e devolve resposta com a forma do SDK."""

    def __init__(self, raises: Exception | None = None) -> None:
        self.calls: list[dict] = []
        self.raises = raises

    def system_one(self, *, state, questions, model=None, **_kw):
        self.calls.append({"state": state, "model": model})
        if self.raises is not None:
            raise self.raises
        answers = {
            "is_manifest": SimpleNamespace(type="noul", noul=0.97),
            "kind": SimpleNamespace(
                type="choice",
                choice="rust",
                probabilities={"rust": 0.9, "python": 0.08, "other": 0.02},
                confidence=0.85,
            ),
        }
        return SimpleNamespace(
            answers=answers,
            model="jev-1.13.0",
            usage=SimpleNamespace(input_tokens=123),
        )


@pytest.fixture()
def infra(tmp_path: Path) -> dict:
    return {
        "key_file": tmp_path / "key.env",
        "cache_file": tmp_path / "cache.json",
        "journal_file": tmp_path / "journal.jsonl",
    }


def _questions() -> dict:
    return {
        "is_manifest": {"type": "noul", "instructions": "Is the state a Rust workspace manifest?"},
        "kind": {
            "type": "choice",
            "instructions": "Which ecosystem owns `state`?",
            "criteria": {"rust": "Cargo.toml present", "python": "pyproject", "other": "none"},
        },
    }


def test_reads_env_file_when_env_unset(monkeypatch: pytest.MonkeyPatch, infra: dict) -> None:
    monkeypatch.delenv("TYPESAFE_API_KEY", raising=False)
    infra["key_file"].write_text(f"TYPESAFE_API_KEY={FIXTURE_KEY}\n")
    assert jev_client.load_key(infra["key_file"]) == FIXTURE_KEY


def test_cache_hit_skips_network(infra: dict) -> None:
    fake = FakeClient()
    kwargs = {**infra, "client": fake}
    first = jev_client.system_one("state-x", _questions(), **kwargs)
    second = jev_client.system_one("state-x", _questions(), **kwargs)
    assert first == second is not None
    assert len(fake.calls) == 1, "segunda chamada idêntica NÃO pode tocar a rede"


def test_fail_open_on_timeout(infra: dict) -> None:
    fake = FakeClient(raises=TimeoutError("boom"))
    result = jev_client.system_one("s", _questions(), **infra, client=fake)
    assert result is None
    last = json.loads(infra["journal_file"].read_text().strip().splitlines()[-1])
    assert last["route_taken"] == "fallback"
    assert "TimeoutError" in last["fallback_reason"]


def test_fail_open_on_429(infra: dict) -> None:
    err_cls = getattr(jev_client, "TypeSafeAPIError", None) if jev_client._SDK_AVAILABLE else None
    if err_cls is None:
        pytest.skip("SDK ausente neste ambiente")
    try:
        err = err_cls("rate limited", status_code=429)
    except TypeError:
        err = TimeoutError("429-simulado")  # forma do erro é do SDK; o contrato é o None
    fake = FakeClient(raises=err)
    assert jev_client.system_one("s", _questions(), **infra, client=fake) is None


def test_never_logs_key(infra: dict, capsys: pytest.CaptureFixture[str]) -> None:
    infra["key_file"].write_text(f"TYPESAFE_API_KEY={LEAK_SENTINEL}\n")
    fake = FakeClient()
    jev_client.system_one("s", _questions(), **infra, client=fake)
    journal_text = infra["journal_file"].read_text()
    stderr_text = capsys.readouterr().err
    assert LEAK_SENTINEL not in journal_text
    assert LEAK_SENTINEL not in stderr_text


def test_questions_nao_mapping_devolve_none_com_erro_nomeado(infra: dict) -> None:
    """V0e (09/10/2026 — dívida V0c c2e3da31104b): `sorted(questions.keys())`
    explodia com AttributeError cru quando `questions` não era um Mapping — um
    negativo sem nome. O erro estruturado nomeia o atributo ausente, no padrão
    das demais recusas de spec do módulo, e segue fail-open (journal + None)."""
    result = jev_client.system_one("s", ["não", "é", "dict"], **infra, client=FakeClient())
    assert result is None
    last = json.loads(infra["journal_file"].read_text().strip().splitlines()[-1])
    assert last["route_taken"] == "fallback"
    assert "keys" in last["fallback_reason"]
    assert "list" in last["fallback_reason"]


def test_journal_schema_complete(infra: dict) -> None:
    fake = FakeClient()
    jev_client.system_one("s", _questions(), **infra, client=fake)
    record = json.loads(infra["journal_file"].read_text().strip().splitlines()[-1])
    required = {"ts", "model", "question_ids", "route_taken", "cache_key", "latency_ms", "input_tokens"}
    assert required <= set(record), f"faltam campos: {required - set(record)}"
    assert record["route_taken"] == "api"
    assert record["latency_ms"] >= 0


def test_journal_carries_confidence_digest(infra: dict) -> None:
    """O KPI jev_confidence_hist (W3) lê o journal: toda rota (api E cache) grava
    o digest por pergunta com confidence e max_prob — a ausência calada desse campo
    foi o defeito apontado pela revisão da W1 (touring-02)."""
    fake = FakeClient()
    kwargs = {**infra, "client": fake}
    jev_client.system_one("s", _questions(), **kwargs)  # rota api
    jev_client.system_one("s", _questions(), **kwargs)  # rota cache
    lines = infra["journal_file"].read_text().strip().splitlines()
    assert len(lines) == 2
    for line, route in zip(lines, ("api", "cache")):
        record = json.loads(line)
        assert record["route_taken"] == route
        digest = record["answers_digest"]
        assert digest["kind"]["confidence"] == 0.85
        assert digest["kind"]["max_prob"] == 0.9
        assert digest["is_manifest"]["noul"] == 0.97


def test_no_key_returns_none(monkeypatch: pytest.MonkeyPatch, infra: dict) -> None:
    monkeypatch.delenv("TYPESAFE_API_KEY", raising=False)
    # key_file aponta para arquivo inexistente: sem chave, sem exceção, com journal
    result = jev_client.system_one("s", _questions(), **infra)
    assert result is None
    last = json.loads(infra["journal_file"].read_text().strip().splitlines()[-1])
    assert last["fallback_reason"] == "no_key"


def test_base_url_allows_missing_key(monkeypatch: pytest.MonkeyPatch, infra: dict) -> None:
    """Servidor local (aluno Laya): sem bearer, a chave pode ser None — o
    contrato /v1/systemone é o mesmo, só muda o endereço (contrato, não fornecedor)."""
    monkeypatch.delenv("TYPESAFE_API_KEY", raising=False)
    fake = FakeClient()
    result = jev_client.system_one("s", _questions(), **infra, client=fake, base_url="http://127.0.0.1:8971")
    assert result is not None
    last = json.loads(infra["journal_file"].read_text().strip().splitlines()[-1])
    assert last.get("base_url") == "http://127.0.0.1:8971"
