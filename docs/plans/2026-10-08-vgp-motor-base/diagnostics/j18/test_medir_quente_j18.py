"""Testes sintéticos do instrumento quente J18; nenhum daemon ou API real é chamado."""

import json
from types import SimpleNamespace

import medir_daemon_quente_j18 as quente


def corpus_unitario(tmp_path):
    (tmp_path / "snap.py").write_text("json.loadz('{}')\n")
    entrada = {
        "id": "caso",
        "path": "<string>",
        "snap": "snap.py",
        "project_root": str(tmp_path),
    }
    return tmp_path, [entrada]


def test_pre_aquecimento_e_reparo_preservam_o_denominador(tmp_path, monkeypatch):
    corpus, entradas = corpus_unitario(tmp_path)

    def chamada_fake(_entrada, _corpus, session, sequencia, fase, _trace):
        return f"{session}:{fase}:{sequencia}", 0, 1.0

    def rota_fake(_trace, _session, tool_id):
        if ":pre:" in tool_id:
            return "standalone" if tool_id.endswith(":1") else "daemon"
        if ":medida:" in tool_id:
            return "standalone"
        return "daemon"

    monkeypatch.setattr(quente, "chamada", chamada_fake)
    monkeypatch.setattr(quente, "rota_de", rota_fake)
    monkeypatch.setattr(quente, "jsonl_da_session", lambda *_: [])
    registros, ignorados, pre, reparos, descartar = quente.medir_entradas(
        entradas, corpus, "sessao", tmp_path / "trace.jsonl"
    )
    assert [item["rota"] for item in pre] == ["standalone", "daemon"]
    assert len(reparos) == 1
    assert reparos[0]["motivo"] == "rota não-daemon: aquecimento=daemon, medida=standalone"
    assert reparos[0]["original"]["rc"] == 0
    assert reparos[0]["original"]["wall_ms"] == 1.0
    assert len(descartar) == 2
    assert len(registros) == 1 and "reparo-medida" in next(iter(registros))
    assert not ignorados


def test_sem_daemon_apos_cinco_tentativas_e_inconclusivo(tmp_path, monkeypatch):
    corpus, entradas = corpus_unitario(tmp_path)
    monkeypatch.setattr(
        quente,
        "chamada",
        lambda _e, _c, session, sequencia, fase, _trace: (f"{session}:{fase}:{sequencia}", 0, 1.0),
    )
    monkeypatch.setattr(quente, "rota_de", lambda *_: "standalone")
    monkeypatch.setattr(quente, "jsonl_da_session", lambda *_: [])
    registros, _, pre, reparos, descartar = quente.medir_entradas(
        entradas, corpus, "sessao", tmp_path / "trace"
    )
    assert len(pre) == 5 and pre[-1]["rota"] == "standalone"
    assert not registros and not reparos and not descartar


def test_resumir_exclui_o_par_reparado_das_rotas(tmp_path, monkeypatch):
    trace = [
        {"tool_use_id": "original-aquecimento", "route": "daemon"},
        {"tool_use_id": "original-medida", "route": "standalone"},
        {"tool_use_id": "reparo-aquecimento", "route": "daemon"},
        {"tool_use_id": "reparo-medida", "route": "daemon"},
    ]
    monkeypatch.setattr(quente, "jsonl_da_session", lambda *_: trace)
    args = SimpleNamespace(corpus=tmp_path, trace=tmp_path / "trace")
    registros = {
        "reparo-medida": {
            "elapsed_ms": 10.0,
            "camadas": [1, 2],
            "wall_ms": 12.0,
        }
    }
    resumo = quente.resumir(
        args,
        "sessao",
        registros,
        [],
        [{"tentativa": 1, "rota": "daemon"}],
        [{"id": "caso"}],
        {"original-aquecimento", "original-medida"},
    )
    assert resumo["valido"]
    assert resumo["denominador"]["rotas"] == {"daemon": 2}
    assert resumo["descartados_do_denominador"] == [
        "original-aquecimento",
        "original-medida",
    ]


def test_rota_ausente_nunca_vira_daemon(tmp_path):
    (tmp_path / "trace").write_text(json.dumps({"session_id": "s", "tool_use_id": "outro"}))
    assert quente.rota_de(tmp_path / "trace", "s", "ausente") == "ausente"
