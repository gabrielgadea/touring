"""Testes do diagnóstico retroativo, sem ler qualquer corpus."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import diagnosticar_dependencias as d


def test_classe_veredito_distingue_deny_nao_deny_e_unknown():
    assert d.classe_veredito({"verdict": "deny"}) == "deny"
    assert d.classe_veredito({"verdict": "allow"}) == "nao_deny"
    assert d.classe_veredito({"verdict": "warn"}) == "nao_deny"
    assert d.classe_veredito({"camada2": "unknown: prazo"}) == "unknown"
    assert d.classe_veredito({"_erro": "falha"}) == "unknown"


def test_resumir_separa_mudancas_direcionadas_e_indeterminados():
    registros = [
        {"id": "a", "balde": "arquivo", "dependencias": 2, "dependencias_alteradas": 1, "inconclusivo_reconstrucao": False, "overlays": [{}], "veredito_atual": "deny", "veredito_historico": "nao_deny"},
        {"id": "b", "balde": "inline", "dependencias": 3, "dependencias_alteradas": 2, "inconclusivo_reconstrucao": False, "overlays": [{}], "veredito_atual": "nao_deny", "veredito_historico": "deny"},
        {"id": "c", "balde": "arquivo", "dependencias": 1, "dependencias_alteradas": 1, "inconclusivo_reconstrucao": False, "overlays": [{}], "veredito_atual": "deny", "veredito_historico": "unknown"},
        {"id": "d", "balde": "inline", "dependencias": 1, "dependencias_alteradas": 0, "inconclusivo_reconstrucao": True},
    ]
    r = d.resumir(registros)
    assert r["entradas"] == 4
    assert r["dependencias_locais"] == 7
    assert r["dependencias_alteradas"] == 4
    assert r["entradas_com_dependencia_alterada"] == 3
    assert r["inconclusivo_reconstrucao"] == 1
    assert r["comparacoes_motor"] == 3
    assert r["unknown_historico"] == 1
    assert r["mudanca_veredito_efetivo"] == 2
    assert r["deny_para_nao_deny"] == 1
    assert r["nao_deny_para_deny"] == 1
    assert r["sem_mudanca_conhecida"] == 0


def test_resumir_sem_dependencias_alteradas():
    r = d.resumir([{"id": "a", "balde": "arquivo", "dependencias": 2, "dependencias_alteradas": 0, "inconclusivo_reconstrucao": False}])
    assert r["entradas_com_dependencia_alterada"] == 0
    assert r["comparacoes_motor"] == 0
    assert r["mudanca_veredito_efetivo"] == 0


def test_resumir_conta_unknown_atual_sem_o_contar_como_mudanca():
    registros = [{"id": "a", "balde": "inline", "dependencias": 1, "dependencias_alteradas": 1, "inconclusivo_reconstrucao": False, "overlays": [{}], "veredito_atual": "unknown", "veredito_historico": "deny"}]
    r = d.resumir(registros)
    assert r["unknown_atual"] == 1
    assert r["mudanca_veredito_efetivo"] == 0
    assert r["sem_mudanca_conhecida"] == 0


def test_resumir_conta_par_estavel():
    registros = [{"id": "a", "balde": "arquivo", "dependencias": 1, "dependencias_alteradas": 1, "inconclusivo_reconstrucao": False, "overlays": [{}], "veredito_atual": "deny", "veredito_historico": "deny"}]
    r = d.resumir(registros)
    assert r["sem_mudanca_conhecida"] == 1
    assert r["mudanca_veredito_efetivo"] == 0


def test_absolutizar_estado_preserva_antigos_e_soma_novos_overlays(monkeypatch):
    monkeypatch.setattr(d, "CORPUS", Path("/corpus"))
    monkeypatch.setattr(d, "TRABALHO", Path("/trabalho"))
    e = {"id": "a"}
    original = {"disk_file": "antes/a.py", "overlays": [{"path": "/p/a.py", "file": "modulos/a.py"}]}
    estado = d.absolutizar_estado(e, [{"path": "/p/b.py", "file": "modulos/b.py"}], original)
    assert estado["disk_file"] == "/corpus/antes/a.py"
    assert [o["file"] for o in estado["overlays"]] == ["/corpus/modulos/a.py", "/trabalho/modulos/b.py"]


def test_entradas_le_arquivo_e_inline(monkeypatch, tmp_path):
    monkeypatch.setattr(d, "CORPUS", tmp_path)
    (tmp_path / "entradas.jsonl").write_text('{"id":"a","rotulo":"negativo"}\n')
    (tmp_path / "entradas_inline_neg.jsonl").write_text('{"id":"b","rotulo":"negativo"}\n')
    assert [e["id"] for e in d.entradas()] == ["a", "b"]


def test_selftest_do_instrumento():
    assert d.selftest() == 0
