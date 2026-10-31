"""Unit tests for hot-run repair consolidation and its fail-closed checks."""

import importlib.util
import json
from pathlib import Path

import pytest

ARQ = Path(__file__).resolve().with_name("consolidar_reparo_quente.py")
spec = importlib.util.spec_from_file_location("consolidar_reparo_quente", ARQ)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


def test_ler_jsonl_filtra_a_session(tmp_path):
    arq = tmp_path / "trace.jsonl"
    arq.write_text('{"session_id":"a","route":"daemon"}\n{"session_id":"b","route":"standalone"}\n')
    assert mod.ler_jsonl(arq, "a") == [{"session_id": "a", "route": "daemon"}]


def test_exigir_fecha_na_ausencia_do_sinal():
    with pytest.raises(SystemExit, match="falta"):
        mod.exigir(False, "falta")


def dados_resumo():
    original = {
        "session": "s",
        "metodo": "m",
        "denominador": {"rotas": {"daemon": 2127, "standalone": 1}},
        "p95_motor_ms": 1.0,
    }
    por_id = {
        str(i): {"i": i, "id": f"id{i}", "elapsed_ms": float(i % 10 + 1), "camadas": [1, 2], "wall_ms": 20.0}
        for i in range(1064)
    }
    trace = [{"route": "daemon"} for _ in range(2128)]
    return original, por_id, trace


def test_construir_resumo_exige_todas_as_rotas_daemon():
    original, por_id, trace = dados_resumo()
    resumo = mod.construir_resumo(original, {"session": "r"}, por_id, trace, 268, {"route": "standalone"})
    assert resumo["valido"] and resumo["veredito"] == "OK"
    assert resumo["p95_motor_ms"] == 10.0
    trace[17] = {"route": "standalone"}
    assert not mod.construir_resumo(original, {"session": "r"}, por_id, trace, 268, {"route": "standalone"})["valido"]


def test_carregar_substitui_a_entrada_e_preserva_o_indice(monkeypatch, tmp_path):
    monkeypatch.setattr(mod, "AQUI", tmp_path)
    (tmp_path / "hot_daemon.json").write_text(json.dumps({"session": "s", "p95_motor_ms": 1.0}))
    (tmp_path / "hot_reparo_134.json").write_text(
        json.dumps({"session": "r", "valido": True, "denominador": {"rotas": {"daemon": 2}}})
    )
    (tmp_path / "hot_daemon.porid.json").write_text(
        json.dumps({"por_id": {"velho": {"i": 134, "id": "x", "elapsed_ms": 99.0}}})
    )
    (tmp_path / "hot_reparo_134.porid.json").write_text(
        json.dumps({"por_id": {"novo": {"i": 0, "id": "x", "elapsed_ms": 48.0}}})
    )
    trace = [{"session_id": "s", "route": "daemon"} for _ in range(269)]
    trace[268]["route"] = "standalone"
    (tmp_path / "hot_daemon.trace.jsonl").write_text("".join(json.dumps(r) + "\n" for r in trace))
    (tmp_path / "hot_reparo_134.trace.jsonl").write_text(
        '{"session_id":"r","route":"daemon"}\n{"session_id":"r","route":"daemon"}\n'
    )
    original, reparo, por_id, lido, reparo_trace, posicao, standalone = mod.carregar()
    assert original["session"] == "s" and reparo["session"] == "r"
    assert por_id["velho"]["i"] == 134 and por_id["velho"]["elapsed_ms"] == 48.0
    assert len(lido) == 269 and len(reparo_trace) == 2
    assert posicao == 268 and standalone["route"] == "standalone"
