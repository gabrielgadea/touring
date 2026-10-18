"""HO13 — a fiacao do construtor do held-out: o filtro de estado completo, o congelamento por sha e o resumo do meta.
Fixtures sinteticas; nada de transcript real nem de held-out."""

import hashlib
import json
import sys
from pathlib import Path

import pytest

SP = Path(__file__).resolve().parent
sys.path.insert(0, str(SP))
import construir_disjunto as cd  # noqa: E402

MOTIVO = "estado incompleto: submódulo importado não reconstrói no instante"
VAZIO = {"c": 0, "heredoc": 0, "touring_run": 0, "tmp_escrito_e_executado": 0}


def reg(i: int, programa: str) -> dict:
    return {"ts": f"2026-10-01T10:00:0{i}.000Z", "tool_use_id": f"t{i}", "session": f"s{i}", "programa": programa, "extrator": "c"}


def pool_com(bons: int, ruins: int) -> dict:
    registros = [reg(i, f"x = {i}\n") for i in range(bons)] + [reg(bons + i, f"x = {bons + i}  # RUIM\n") for i in range(ruins)]
    return {"c": registros, "heredoc": [], "touring_run": [], "tmp_escrito_e_executado": []}


def filtro_ruim(r: dict) -> tuple[bool, str]:
    return ("RUIM" not in r["programa"], MOTIVO)


def test_sem_filtro_o_comportamento_e_o_de_antes():
    escolhidos, stats = cd.escolher_inline(pool_com(3, 3), 1, VAZIO | {"c": 6}, 18)
    assert len(escolhidos) == 6 and stats["c"]["rejeitados"] == {}


def test_filtro_deixa_de_fora_o_incompleto_e_conta_o_motivo():
    escolhidos, stats = cd.escolher_inline(pool_com(3, 3), 1, VAZIO | {"c": 3}, 18, filtro_ruim)
    assert len(escolhidos) == 3 and all("RUIM" not in r["programa"] for r in escolhidos)
    assert stats["c"]["rejeitados"].get(MOTIVO, 0) + stats["c"]["aceitos"] <= 6
    assert stats["c"]["aceitos"] == 3


def test_filtro_conta_todo_recusado_quando_a_quota_nao_fecha():
    escolhidos, stats = cd.escolher_inline(pool_com(2, 4), 1, VAZIO | {"c": 5}, 18, filtro_ruim)
    assert len(escolhidos) == 2 and stats["c"]["rejeitados"] == {MOTIVO: 4} and stats["c"]["faltou"] == 3


def test_o_filtro_so_age_depois_dos_testes_baratos():
    chamadas = []

    def espia(r: dict) -> tuple[bool, str]:
        chamadas.append(r["tool_use_id"])
        return True, "ok"

    pool = {"c": [reg(0, "a = 1\n"), reg(1, "a = 1\n")], "heredoc": [], "touring_run": [], "tmp_escrito_e_executado": []}
    escolhidos, stats = cd.escolher_inline(pool, 1, VAZIO | {"c": 2}, 18, espia)
    assert len(escolhidos) == 1 and stats["c"]["rejeitados"] == {"programa repetido na amostra": 1} and len(chamadas) == 1


def test_congelados_confere_o_sha(tmp_path):
    arq = tmp_path / "regra.py"
    arq.write_text("x = 1\n")
    reg_json = tmp_path / "congelados.json"
    reg_json.write_text(json.dumps({"arquivos": {str(arq): hashlib.sha256(arq.read_bytes()).hexdigest()}}))
    assert cd.conferir_congelados(reg_json)["arquivos"][str(arq)] == hashlib.sha256(b"x = 1\n").hexdigest()
    assert cd.conferir_congelados(None) == {}


def test_congelados_aborta_se_o_arquivo_mudou(tmp_path):
    arq = tmp_path / "regra.py"
    arq.write_text("x = 1\n")
    reg_json = tmp_path / "congelados.json"
    reg_json.write_text(json.dumps({"arquivos": {str(arq): hashlib.sha256(arq.read_bytes()).hexdigest()}}))
    arq.write_text("x = 2\n")
    with pytest.raises(SystemExit, match="CONGELADO MUDOU"):
        cd.conferir_congelados(reg_json)


def test_resumo_do_estado_completo():
    estado = {"entradas_incompletas": 0, "modulos_por_tipo": {"submodulo": 2}, "entradas_com_modulo_que_a_regra_antiga_nao_sobrepunha": 2}
    sel = {"c": {"rejeitados": {MOTIVO: 3, "teto por sessao": 9}}, "heredoc": {"rejeitados": {MOTIVO: 1}}}
    r = cd.resumo_estado_completo(True, {MOTIVO: 5, "nao compila": 7}, sel, estado)
    assert r["recusados_por_estado_incompleto"] == {"arquivo": {MOTIVO: 5}, "inline": {MOTIVO: 4}}
    assert r["exigido"] is True and r["entradas_incompletas_no_corpus"] == 0
    assert r["fecho"] is None and set(r["verificador_independente_do_fecho"]) == {"entradas_com_lacuna", "modulos_nao_cobertos"}


# --------------------------------------------------------------------------- HO13f: o fecho transitivo no construtor

MOTIVO_FECHO = "estado incompleto: módulo do fecho transitivo (profundidade ≥ 2) não reconstrói no instante"
MOTIVO_RASO = "estado incompleto: módulo importado não reconstrói no instante"  # os módulos falsos abaixo são do tipo `direto`


def modulo(nome: str, profundidade: int, conteudo: str | None = "") -> dict:
    return {"path": Path("/p") / nome, "tipo": "direto", "profundidade": profundidade, "conteudo": conteudo}


@pytest.fixture
def avaliacao_limpa(monkeypatch):
    monkeypatch.setattr(cd, "AVALIACAO", {})
    return cd.AVALIACAO


def test_avaliar_candidato_o_fecho_decide_e_a_regra_antiga_so_e_contada(monkeypatch, avaliacao_limpa):
    fechos = {"funda": [modulo("a.py", 1), modulo("b.py", 2, None)], "rasa": [modulo("a.py", 1, None)], "ok": [modulo("a.py", 1)]}
    monkeypatch.setattr(cd.eni, "fecho_transitivo", lambda entrada, texto, indice: fechos[texto])
    assert cd.avaliar_candidato("arquivo", {}, "funda", {}) == (False, MOTIVO_FECHO)
    assert cd.avaliar_candidato("arquivo", {}, "rasa", {}) == (False, MOTIVO_RASO)
    assert cd.avaliar_candidato("arquivo", {}, "ok", {}) == (True, "ok")
    a = avaliacao_limpa["arquivo"]
    assert dict(a["antes"]) == {"ok": 2, MOTIVO_RASO: 1}
    assert dict(a["depois"]) == {MOTIVO_FECHO: 1, MOTIVO_RASO: 1, "ok": 1}
    assert a["tamanhos"] == [2, 1, 1]


def test_resumo_do_vies_separa_o_que_so_o_fecho_recusa(monkeypatch, avaliacao_limpa):
    fechos = {"funda": [modulo("a.py", 1), modulo("b.py", 2, None)], "ok": [modulo("a.py", 1), modulo("b.py", 2), modulo("c.py", 3)]}
    monkeypatch.setattr(cd.eni, "fecho_transitivo", lambda entrada, texto, indice: fechos[texto])
    for texto in ("funda", "funda", "ok", "ok"):
        cd.avaliar_candidato("c", {}, texto, {})
    v = cd.resumo_do_vies()["c"]
    assert (v["avaliados"], v["recusados_regra_profundidade_1"], v["recusados_fecho"], v["recusados_so_pelo_fecho"]) == (4, 0, 2, 2)
    assert (v["taxa_regra_profundidade_1"], v["taxa_fecho"]) == (0.0, 0.5)
    assert v["tamanho_do_fecho_dos_avaliados"]["max"] == 3 and v["motivos_fecho"] == {MOTIVO_FECHO: 2, "ok": 2}
    assert cd.resumo_do_vies().keys() == {"c"}


def test_memoizar_git_cacheia_a_leitura_e_nao_o_resto(monkeypatch):
    chamadas = []

    def falso(root, *args):
        chamadas.append(args)
        return f"saida{len(chamadas)}"

    monkeypatch.setattr(cd.c0, "_git", falso)  # o monkeypatch restaura o original no fim, mesmo que memoizar o troque
    contagem = cd.memoizar_git()
    r1 = cd.c0._git("/r", "show", "abc:x.py")
    r2 = cd.c0._git("/r", "show", "abc:x.py")
    r3 = cd.c0._git("/r", "rev-list", "-1", "--before=T", "HEAD")
    cd.c0._git("/r", "status")
    cd.c0._git("/r", "status")
    assert r1 == r2 == "saida1" and r3 == "saida2" and len(chamadas) == 4  # show 1x, rev-list 1x, status 2x (nao cacheia)
    assert contagem == {"hits": 1, "miss": 2}
    assert cd.c0._git("/outra", "show", "abc:x.py") == "saida5"  # a chave inclui a raiz
