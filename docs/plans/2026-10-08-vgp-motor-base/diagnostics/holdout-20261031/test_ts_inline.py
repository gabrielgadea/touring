"""Regression tests: dated duplicates must beat api-request-blob without ts."""

import sys
from pathlib import Path

import pytest

SP = Path(__file__).resolve().parent
sys.path.insert(0, str(SP.parent / "instrumentos-2026-10-09"))
import construir_disjunto as cd  # noqa: E402

CORTE = "2026-10-09T18:14:31.894Z"
ANTES = "2026-10-01T10:00:00.000Z"
DEPOIS = "2026-10-27T14:22:17.466Z"
VAZIO = {"ids": set(), "tool_use_ids": set(), "shas": set(), "comandos": set()}


def registro(uid: str, ts: str, comando: str) -> dict:
    return {
        "tool_use_id": uid,
        "ts": ts,
        "session": "s1",
        "command": comando,
        "programa": "x = 1\n",
        "extrator": "c",
    }


def monta_fake(registros: list[dict]):
    return registros, [], {}


def test_duplicata_datada_vence_blob_e_respeita_o_corte(monkeypatch):
    registros = [
        registro("mesmo-id", "", "python3 -c 'x = 1'"),
        registro("mesmo-id", DEPOIS, "python3 -c 'x = 1'"),
        registro("mesmo-id", ANTES, "python3 -c 'x = 1'"),
    ]
    monkeypatch.setattr(cd.v5, "universo", lambda: registros)
    monkeypatch.setattr(cd.v5, "monta", monta_fake)
    pool, info = cd.pool_inline(CORTE, VAZIO)
    assert [r["ts"] for r in pool["c"]] == [ANTES]
    assert info["excluidos_por_chave"]["tool_use_id duplicado (pai/subagente)"] == 2


def test_inline_sem_qualquer_instante_vira_exclusao_contada(monkeypatch):
    monkeypatch.setattr(cd.v5, "universo", lambda: [registro("sem-ts", "", "python3 -c 'x = 1'")])
    monkeypatch.setattr(cd.v5, "monta", monta_fake)
    pool, info = cd.pool_inline(CORTE, VAZIO)
    assert pool["c"] == []
    assert info["excluidos_por_chave"]["ts_ausente"] == 1


def test_barreira_do_cache_nao_deixa_ts_vazio_atravessar_o_corte(monkeypatch, tmp_path):
    monkeypatch.setattr(cd, "STATE", {})
    monkeypatch.setattr(cd, "CACHE", tmp_path / "cache.json")
    monkeypatch.setattr(cd.c0, "varrer_transcripts", lambda: {})
    monkeypatch.setattr(
        cd.c0,
        "universo_negativos",
        lambda _indice: (
            [f"/p/a.py@{DEPOIS}", "/p/a.py@", f"/p/a.py@{ANTES}"],
            {"vistos": 1, "candidatos": 3},
        ),
    )
    cd.ajustar_arquivo(CORTE, VAZIO, {}, 18, False)
    filtrados, contagens = cd.c0.universo_negativos({})
    assert filtrados == [f"/p/a.py@{ANTES}"]
    assert contagens["ts_ausente"] == 1
