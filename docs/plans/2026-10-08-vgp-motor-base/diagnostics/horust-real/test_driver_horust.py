"""HORUST-REAL — o orquestrador (TDD): o registro de uma tentativa basta para refazer a mutação, a amostra de reconfirmação segue a regra
pré-registrada e as rodadas seguintes só retentam o que está abaixo de 20. Sem cargo, sem disco do worktree.
O módulo sob teste vem de `$HORUST_SRC_DIR` (os mutantes rodam sobre uma cópia).
"""

import json
import os
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(Path(os.environ.get("HORUST_SRC_DIR", HERE))))
import construir_horust_real as cr  # noqa: E402
import construtor_horust as ch  # noqa: E402
import operadores_rust as op  # noqa: E402
import oraculo_cargo_json as oc  # noqa: E402

TEXTO = """use crate::tipos::Coisa;
use crate::outros::{Alfa, Beta};
pub fn principal(c: &Config) -> u32 {
    let total = calcula_total(c.valor, 3);
    let r = Coisa::nova(total).com_nome("a");
    r.contagem
}
pub fn monta() -> Config {
    Config {
        nome: String::from("a"),
        limite: 10,
        ativo: true,
    }
}
"""


def tentativas():
    arq = ch.Arquivo("crates/p/src/a.rs", "p", "lib", TEXTO)
    return ch.planejar({arq.rel: arq}, {"p": op.tokens(TEXTO)}, cr.SEMENTE, set(), {"nome": 1})  # um arquivo, uma tentativa


def test_a_semente_da_rodada_1_e_a_pre_registrada_e_as_demais_levam_sufixo():
    assert cr.semente_da_rodada(1) == "horust-real-20261010"
    assert cr.semente_da_rodada(2) == "horust-real-20261010-r2" and cr.semente_da_rodada(3) == "horust-real-20261010-r3"


@pytest.mark.parametrize("operador", sorted(op.CODIGO))
def test_registro_basta_para_refazer_a_mutacao_de_cada_operador(operador):
    sitio = next((s for s in op.sitios(TEXTO) if s.operador == operador), None)
    if sitio is None:
        pytest.skip(f"o texto de teste não tem sítio de {operador}")
    novo = "" if operador == "kwarg_remove" else op.variante(sitio.nome, op.tipo_do_nome(sitio), "near_miss", "k", set())
    nome_msg = sitio.tipo if operador == "kwarg_remove" else novo
    classe, codigo = op.CODIGO[operador]
    inj = oc.Injecao("id1", "crates/p/src/a.rs", classe, operador, codigo, sitio.linha, nome_msg, TEXTO, op.mutar(TEXTO, sitio, novo))
    t = ch.Tentativa(inj, "p", "lib", "near_miss", novo, sitio.nome, sitio.inicio)
    r = cr.registro(t, 1, "", oc.Diag("crates/p/src/a.rs", sitio.linha, 9, codigo, f"msg `{nome_msg}`"))
    refeita = cr.reconstruir(r, TEXTO)
    assert refeita.texto_novo == inj.texto_novo and refeita.nome == inj.nome and (refeita.codigo, refeita.linha) == (inj.codigo, inj.linha)
    assert r["motivo"] == "" and r["msg"].endswith(f"`{nome_msg}`") and r["coluna_rustc"] == 9


def test_registro_de_descarte_nao_carrega_diagnostico():
    sitio = next(s for s in op.sitios(TEXTO) if s.operador == "nome_valor")
    inj = oc.Injecao("i", "crates/p/src/a.rs", "nome", "nome_valor", "E0425", sitio.linha, "x", TEXTO, TEXTO)
    r = cr.registro(ch.Tentativa(inj, "p", "lib", "near_miss", "x", sitio.nome, sitio.inicio), 2, "sem_erro", None)
    assert (r["motivo"], r["msg"], r["rodada"]) == ("sem_erro", "", 2)


@pytest.mark.parametrize("n,esperado", [(5, 3), (30, 3), (31, 4), (100, 10), (101, 11)])
def test_amostra_de_reconfirmacao_e_10_por_cento_com_minimo_3(n, esperado):
    validos = [{"id": f"id{i}"} for i in range(n)]
    assert len(cr.amostra(validos, "nome")) == min(esperado, n)


def test_amostra_e_deterministica_e_pertence_aos_validos():
    validos = [{"id": f"id{i}"} for i in range(50)]
    a = cr.amostra(validos, "nome")
    assert a == cr.amostra(list(reversed(validos)), "nome") or {x["id"] for x in a} == {x["id"] for x in cr.amostra(list(reversed(validos)), "nome")}
    assert {x["id"] for x in a} <= {x["id"] for x in validos}


def test_rodada_1_tenta_todas_as_classes_e_as_seguintes_so_as_abaixo_de_20():
    assert cr.classes_pendentes(1, []) == dict.fromkeys(("nome", "import", "atributo", "kwarg"), 72)
    registros = [{"classe": "nome", "motivo": "", "rodada": 1}] * 25 + [{"classe": "import", "motivo": "", "rodada": 1}] * 19 + [{"classe": "atributo", "motivo": "sem_erro", "rodada": 1}] * 40
    assert cr.classes_pendentes(3, registros) == {"import": 72, "atributo": 72, "kwarg": 72}


def test_pendentes_so_olham_as_rodadas_anteriores_para_o_plano_nao_mudar_na_retomada():
    anteriores = [{"classe": "nome", "motivo": "", "rodada": 1}] * 19
    parcial_da_rodada_2 = [{"classe": "nome", "motivo": "", "rodada": 2}] * 5
    assert "nome" in cr.classes_pendentes(2, anteriores + parcial_da_rodada_2), "o parcial da rodada 2 não pode tirar a classe do plano da 2"
    assert "nome" in cr.classes_pendentes(3, anteriores + [{"classe": "nome", "motivo": "", "rodada": 3}] * 5)


def test_classe_com_exatamente_20_validos_nao_e_retentada():
    pend = cr.classes_pendentes(3, [{"classe": "nome", "motivo": "", "rodada": 1}] * 20 + [{"classe": "kwarg", "motivo": "", "rodada": 1}] * 19)
    assert "nome" not in pend and "kwarg" in pend


def arquivos_do_plano(n=16):
    return {f"crates/p{i % 2}/src/m{i}.rs": ch.Arquivo(f"crates/p{i % 2}/src/m{i}.rs", f"p{i % 2}", "lib", TEXTO) for i in range(n)}


def test_plano_da_rodada_retomado_e_o_mesmo_menos_o_que_ja_foi_feito():
    arqs = arquivos_do_plano()
    tk = {"p0": op.tokens(TEXTO), "p1": op.tokens(TEXTO)}
    inteiro = cr.plano_da_rodada(1, arqs, tk, [])
    assert inteiro
    feitos = [{"id": t.inj.id, "rel": t.inj.arq, "rodada": 1, "classe": t.inj.classe, "motivo": ""} for t in inteiro[:3]]
    retomado = cr.plano_da_rodada(1, arqs, tk, feitos)
    assert [t.inj.id for t in retomado] == [t.inj.id for t in inteiro[3:]]


def test_plano_da_rodada_2_nao_reusa_arquivo_da_rodada_1():
    arqs = arquivos_do_plano(40)
    tk = {"p0": op.tokens(TEXTO), "p1": op.tokens(TEXTO)}
    r1 = cr.plano_da_rodada(1, arqs, tk, [])
    feitos = [{"id": t.inj.id, "rel": t.inj.arq, "rodada": 1, "classe": t.inj.classe, "motivo": "sem_erro"} for t in r1[:10]]
    r2 = cr.plano_da_rodada(2, arqs, tk, feitos)
    assert r2 and not ({t.inj.arq for t in r2} & {r["rel"] for r in feitos})


def test_negativos_nao_incluem_arquivo_tentado_e_espalham_entre_pacotes(monkeypatch):
    monkeypatch.setattr(cr, "NEG_ALVO", 12)
    arqs = {f"crates/p{i % 3}/src/f{i}.rs": ch.Arquivo(f"crates/p{i % 3}/src/f{i}.rs", f"p{i % 3}", "lib", "x\n") for i in range(30)}
    tentados = {rel for i, rel in enumerate(sorted(arqs)) if i % 3 == 0}
    neg = cr.escolher_negativos(arqs, tentados)
    assert len(neg) == 12 and not ({n.rel for n in neg} & tentados)
    assert {n.pkg for n in neg} == {"p0", "p1", "p2"} and max(sum(n.pkg == p for n in neg) for p in ("p0", "p1", "p2")) <= 5


def test_ids_escolhidos_respeitam_o_teto_de_25_por_cento_por_crate():
    regs = [{"classe": "nome", "motivo": "", "pkg": "dominante", "id": f"d{i}"} for i in range(40)]
    regs += [{"classe": "nome", "motivo": "", "pkg": f"c{k}", "id": f"c{k}_{i}"} for k in range(4) for i in range(6)]
    regs += [{"classe": "nome", "motivo": "sem_erro", "pkg": "c0", "id": "descartado"}]
    esc = cr.escolher_ids(regs)
    assert "descartado" not in esc and 24 < len(esc) <= 32
    assert sum(i.startswith("d") for i in esc) <= int(0.25 * len(esc))


def test_lotes_so_roda_dentro_do_slot(monkeypatch):
    monkeypatch.delenv("HW_SLOT_HELD", raising=False)
    with pytest.raises(RuntimeError, match="hw-slot"):
        cr.exigir_slot()
    monkeypatch.setenv("HW_SLOT_HELD", "rust-1@123")
    cr.exigir_slot()


def test_primeiros_n_lotes_pendentes_em_ordem_e_n_zero_ou_none_sao_todos():
    grupos = {("a", "lib", "R"): [1], ("b", "lib", "R"): [2], ("c", "lib", "T"): [3]}
    assert [k for k, _ in cr.primeiros_lotes(grupos, 2)] == [("a", "lib", "R"), ("b", "lib", "R")]
    assert len(list(cr.primeiros_lotes(grupos, None))) == 3 and len(list(cr.primeiros_lotes(grupos, 0))) == 3


def test_ler_registros_o_ultimo_de_cada_id_vence(tmp_path, monkeypatch):
    monkeypatch.setattr(cr, "ESTADO", tmp_path)
    (tmp_path / "resultados.jsonl").write_text(
        json.dumps({"id": "a", "motivo": "codigo_inesperado"}) + "\n" + json.dumps({"id": "b", "motivo": ""}) + "\n" + json.dumps({"id": "a", "motivo": ""}) + "\n")
    r = {x["id"]: x["motivo"] for x in cr.ler_registros()}
    assert r == {"a": "", "b": ""} and len(cr.ler_registros()) == 2


def test_rodada_2_e_de_reparo_dos_operadores_e_a_3_volta_a_ser_por_classe():
    assert cr.reparo_da_rodada(1) is None and cr.reparo_da_rodada(3) is None
    r = cr.reparo_da_rodada(2)
    assert set(r) == {"nome", "import"} and set(r["nome"]) == {"nome_tipo"} and set(r["import"]) == {"import_use"}


def test_registro_guarda_o_codigo_que_o_rustc_deu_e_o_esperado():
    sitio = next(s for s in op.sitios(TEXTO) if s.operador == "nome_tipo")
    novo = op.variante(sitio.nome, "tipo", "near_miss", "k", set())
    inj = oc.Injecao("i", "crates/p/src/a.rs", "nome", "nome_tipo", "E0425", sitio.linha, novo, TEXTO, op.mutar(TEXTO, sitio, novo), "p", ("E0425", "E0412"))
    r = cr.registro(ch.Tentativa(inj, "p", "lib", "near_miss", novo, sitio.nome, sitio.inicio), 1, "", oc.Diag("crates/p/src/a.rs", sitio.linha, 9, "E0412", "x"))
    assert (r["codigo"], r["codigo_rustc"]) == ("E0425", "E0412")
    r2 = cr.registro(ch.Tentativa(inj, "p", "lib", "near_miss", novo, sitio.nome, sitio.inicio), 1, "sem_erro", None)
    assert r2["codigo_rustc"] == ""


def test_candidatos_a_revalidar_sao_so_nome_tipo_descartado_por_codigo():
    regs = [
        {"id": "a", "operador": "nome_tipo", "motivo": "codigo_inesperado"},
        {"id": "b", "operador": "nome_tipo", "motivo": "sem_erro"},
        {"id": "c", "operador": "nome_valor", "motivo": "codigo_inesperado"},
        {"id": "d", "operador": "nome_tipo", "motivo": ""},
    ]
    assert [r["id"] for r in cr.candidatos_a_revalidar(regs)] == ["a"]


def test_plano_da_rodada_2_so_tem_os_operadores_do_reparo_e_so_o_penultimo_do_import():
    arqs = arquivos_do_plano(40)
    tk = {"p0": op.tokens(TEXTO), "p1": op.tokens(TEXTO)}
    p = cr.plano_da_rodada(2, arqs, tk, [])
    assert p and {t.inj.operador for t in p} <= {"nome_tipo", "import_use"}
    assert all(t.nome_orig != "Coisa" for t in p if t.inj.operador == "import_use"), "o nome importado (último segmento) nunca é mutado no reparo"
    assert all(t.inj.aceitos == ("E0425", "E0412") for t in p if t.inj.operador == "nome_tipo")


def test_reconstruir_um_nome_tipo_devolve_os_codigos_aceitos_e_os_outros_operadores_so_o_proprio():
    sitio = next(s for s in op.sitios(TEXTO) if s.operador == "nome_tipo")
    novo = op.variante(sitio.nome, "tipo", "near_miss", "k", set())
    inj = oc.Injecao("i", "crates/p/src/a.rs", "nome", "nome_tipo", "E0425", sitio.linha, novo, TEXTO, op.mutar(TEXTO, sitio, novo), "p", ("E0425", "E0412"))
    r = cr.registro(ch.Tentativa(inj, "p", "lib", "near_miss", novo, sitio.nome, sitio.inicio), 1, "", oc.Diag("crates/p/src/a.rs", sitio.linha, 9, "E0412", "x"))
    assert cr.reconstruir(r, TEXTO).aceitos == ("E0425", "E0412")
    valor = next(s for s in op.sitios(TEXTO) if s.operador == "nome_valor")
    r2 = dict(r, operador="nome_valor", nome_orig=valor.nome, linha=valor.linha, coluna=valor.inicio, nome_novo="zzzz", nome_msg="zzzz")
    assert cr.reconstruir(r2, TEXTO).aceitos == ()


def test_filtro_do_import_existe_so_na_rodada_2_e_deixa_passar_o_que_nao_e_import_use():
    assert cr.filtro_da_rodada(1) is None and cr.filtro_da_rodada(3) is None
    f = cr.filtro_da_rodada(2)
    use = [s for s in op.sitios(TEXTO) if s.operador == "import_use"]
    assert {s.posicao for s in use} <= {"ultimo", "penultimo"} and use
    assert all(f(s) == (s.posicao == "penultimo") for s in use)
    assert all(f(s) for s in op.sitios(TEXTO) if s.operador != "import_use")


def test_regra_do_registro_separa_original_de_adendo():
    original = {"id": "a", "rodada": 1, "motivo": ""}  # forma da rodada 1: sem codigo_rustc
    revalidado = {"id": "b", "rodada": 1, "motivo": "", "codigo_rustc": "E0425"}
    rodada2 = {"id": "c", "rodada": 2, "motivo": ""}
    assert cr.regra_do_registro(original) == "original"
    assert cr.regra_do_registro(revalidado) == "adendo1" and cr.regra_do_registro(rodada2) == "adendo1"
    assert cr.regra_do_registro({"id": "d", "rodada": 2, "motivo": "", "codigo_rustc": "E0432"}) == "adendo1"


def test_meta_lista_os_ids_por_regra_e_o_sha_do_original():
    regs = [{"id": "a", "rodada": 1, "motivo": "", "classe": "nome"}, {"id": "b", "rodada": 2, "motivo": "", "codigo_rustc": "E0425", "classe": "nome"}]
    por_regra = cr.ids_por_regra(regs, {"a", "b"})
    assert por_regra == {"original": ["a"], "adendo1": ["b"]}
    assert cr.ids_por_regra(regs, {"b"}) == {"original": [], "adendo1": ["b"]}
