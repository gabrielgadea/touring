"""HORUST-REAL / estrato cfg — seleção, plano, registros, positivos, negativos pareados, corpus e integridade (tudo sem cargo)."""

import json
import os
import sys
from pathlib import Path

import pytest

sys.path.insert(0, os.environ.get("HORUST_SRC_DIR") or str(Path(__file__).resolve().parent))
import construir_cfg as cc  # noqa: E402
import construtor_horust as ch  # noqa: E402
import integridade_cfg as ic  # noqa: E402
import operador_cfg as oc_  # noqa: E402
import oraculo_cargo_json as oc  # noqa: E402
import selecao_horust as sel  # noqa: E402

MODELO = """use std::fmt;

fn NOME(x: u32) -> u32 {
    x + 1
}

pub fn principal() -> u32 {
    NOME(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        assert_eq!(principal(), 4);
    }
}
"""


def texto(nome):
    return MODELO.replace("NOME", nome)


def arquivos(spec):
    """spec: lista (pkg, indice) -> Arquivo com função de nome único por arquivo."""
    out = {}
    for pkg, i in spec:
        rel = f"crates/{pkg}/src/m{i}.rs"
        out[rel] = ch.Arquivo(rel, pkg, "lib", texto(f"ajuda_{pkg.replace('-', '_')}_{i}"))
    return out


# ------------------------------------------------------------------ seleção e linha


def test_fase_do_estrato_cfg_e_de_resolucao():
    assert sel.fase("cfg") == "R" and "cfg" not in sel.PESOS


def test_selecao_ate_respeita_o_maximo_e_o_teto_e_e_deterministica():
    por_crate = {f"c{k}": [f"c{k}-{i}" for i in range(30)] for k in range(6)}
    a = sel.selecao_ate(por_crate, 0.25, "s", 40)
    assert a == sel.selecao_ate(por_crate, 0.25, "s", 40) and len(a) == 40
    por_dono = {c: sum(x.startswith(c + "-") for x in a) for c in por_crate}
    assert max(por_dono.values()) <= int(0.25 * 40)


def test_selecao_ate_devolve_o_viavel_quando_o_maximo_nao_cabe():
    por_crate = {"a": [f"a{i}" for i in range(30)], "b": [f"b{i}" for i in range(10)], "c": [f"c{i}" for i in range(10)], "d": ["d0", "d1"]}
    saida = sel.selecao_ate(por_crate, 0.25, "s", 40)
    assert 0 < len(saida) < 40 and sum(x.startswith("a") for x in saida) <= int(0.25 * len(saida))


def test_selecao_ate_pequena_devolve_tudo_que_cabe_e_vazia_se_nada_cabe():
    assert sorted(sel.selecao_ate({"a": ["x"], "b": ["y"], "c": ["z"], "d": ["w"]}, 0.25, "s", 40)) == ["w", "x", "y", "z"]
    assert sel.selecao_ate({"a": ["x", "y", "z"]}, 0.25, "s", 40) == []


@pytest.mark.parametrize("linha_def,linha_uso", [(3, 8), (5, 2), (10, 40), (40, 3)])
def test_linha_do_uso_inverte_linha_do_erro(linha_def, linha_uso):
    s = oc_.SitioCfg("abc", linha_def, linha_uso)
    assert oc_.linha_do_uso(oc_.linha_do_erro(s), linha_def) == linha_uso


# ------------------------------------------------------------------ plano


def test_plano_uma_tentativa_por_arquivo_com_classe_cfg_e_texto_mutado():
    arqs = arquivos([("p", i) for i in range(6)] + [("q", i) for i in range(6)])
    plano = cc.planejar_cfg(arqs, set(), "sem", 8)
    assert len(plano) == 8 and len({t.inj.arq for t in plano}) == 8 and len({t.inj.id for t in plano}) == 8
    for t in plano:
        assert (t.inj.classe, t.inj.operador, t.inj.codigo, t.inj.aceitos) == ("cfg", "cfg_fn", "E0425", ("E0425",))
        (s,) = oc_.sitios_cfg(arqs[t.inj.arq].texto)
        assert t.inj.texto_novo == oc_.mutar_cfg(arqs[t.inj.arq].texto, s) and t.inj.linha == oc_.linha_do_erro(s) and t.inj.nome == s.nome
        assert (t.estilo, t.nome_orig, t.coluna) == ("cfg", s.nome, s.linha_def)


def test_plano_espalha_entre_pacotes_e_e_deterministico():
    arqs = arquivos([("p", i) for i in range(8)] + [("q", i) for i in range(8)] + [("r", i) for i in range(8)])
    a = cc.planejar_cfg(arqs, set(), "sem", 6)
    assert [t.inj.id for t in a] == [t.inj.id for t in cc.planejar_cfg(arqs, set(), "sem", 6)]
    assert {t.pkg for t in a[:3]} == {"p", "q", "r"} and len({t.pkg for t in a}) == 3
    assert [t.inj.id for t in a] != [t.inj.id for t in cc.planejar_cfg(arqs, set(), "outra", 6)]


def test_plano_exclui_arquivos_do_principal_e_nomes_repetidos_em_outro_arquivo_do_pacote():
    arqs = arquivos([("p", i) for i in range(4)])
    excluidos = {"crates/p/src/m0.rs"}
    # m1 passa a conter o nome que m2 define: nenhum dos dois pode ser sítio
    arqs["crates/p/src/m1.rs"] = ch.Arquivo("crates/p/src/m1.rs", "p", "lib", arqs["crates/p/src/m1.rs"].texto + "\npub fn x() { ajuda_p_2(0); }\n")
    plano = cc.planejar_cfg(arqs, excluidos, "sem", 10)
    por_arq = {t.inj.arq: t.inj.nome for t in plano}
    # m0 excluído; m2 define um nome que m1 também cita; m1 e m3 ficam, cada um com a SUA função
    assert por_arq == {"crates/p/src/m1.rs": "ajuda_p_1", "crates/p/src/m3.rs": "ajuda_p_3"}


def test_plano_nome_igual_em_pacote_diferente_nao_exclui():
    arqs = {
        "crates/p/src/a.rs": ch.Arquivo("crates/p/src/a.rs", "p", "lib", texto("mesmo_nome")),
        "crates/q/src/a.rs": ch.Arquivo("crates/q/src/a.rs", "q", "lib", texto("mesmo_nome")),
    }
    assert len(cc.planejar_cfg(arqs, set(), "sem", 10)) == 2


def test_plano_com_varios_sitios_no_arquivo_escolhe_um_so():
    t2 = texto("um_nome").replace("pub fn principal() -> u32 {\n    um_nome(3)\n}", "fn outro_nome(x: u32) -> u32 {\n    x\n}\n\npub fn principal() -> u32 {\n    outro_nome(um_nome(3))\n}")
    arqs = {"crates/p/src/a.rs": ch.Arquivo("crates/p/src/a.rs", "p", "lib", t2)}
    assert len(oc_.sitios_cfg(t2)) == 2 and len(cc.planejar_cfg(arqs, set(), "sem", 10)) == 1


# ------------------------------------------------------------------ registro e reconstrução


def um_registro(valido=True):
    arqs = arquivos([("p", 0)])
    (t,) = cc.planejar_cfg(arqs, set(), "sem", 1)
    diag = oc.Diag(t.inj.arq, t.inj.linha, 5, "E0425", f"cannot find function `{t.inj.nome}` in this scope") if valido else None
    return arqs, t, cc.registro_cfg(t, 1, "" if valido else "sem_erro", diag)


def test_registro_guarda_linha_def_linha_uso_e_basta_para_refazer_a_mutacao():
    arqs, t, r = um_registro()
    assert (r["classe"], r["operador"], r["codigo"], r["codigo_rustc"], r["motivo"]) == ("cfg", "cfg_fn", "E0425", "E0425", "")
    assert r["linha_def"] == t.coluna and r["linha_uso"] == oc_.linha_do_uso(t.inj.linha, t.coluna) and r["coluna_rustc"] == 5
    refeita = cc.reconstruir_cfg(r, arqs[t.inj.arq].texto)
    assert refeita.texto_novo == t.inj.texto_novo and (refeita.linha, refeita.nome, refeita.aceitos) == (t.inj.linha, t.inj.nome, ("E0425",))
    assert cc.tentativa_do_registro_cfg(r, arqs[t.inj.arq].texto).inj.id == t.inj.id


def test_registro_de_descarte_nao_tem_diagnostico():
    _a, _t, r = um_registro(valido=False)
    assert r["motivo"] == "sem_erro" and r["codigo_rustc"] == "" and r["coluna_rustc"] == 0


# ------------------------------------------------------------------ positivos, negativos, corpus


def registros_validos(n_por_crate=(10, 10, 10, 10, 10)):
    regs = []
    for k, n in enumerate(n_por_crate):
        arqs = arquivos([(f"c{k}", i) for i in range(n)])
        for t in cc.planejar_cfg(arqs, set(), "sem", n):
            regs.append(cc.registro_cfg(t, 1, "", oc.Diag(t.inj.arq, t.inj.linha, 5, "E0425", f"cannot find function `{t.inj.nome}` in this scope")))
    return regs


def test_escolha_de_ids_respeita_maximo_40_e_teto_25_por_cento():
    regs = registros_validos((30, 30, 30, 30, 30))
    ids = cc.escolher_ids_cfg(regs)
    por_crate = {}
    for r in regs:
        if r["id"] in ids:
            por_crate[r["pkg"]] = por_crate.get(r["pkg"], 0) + 1
    assert len(ids) == 40 and max(por_crate.values()) <= 10 and ids == cc.escolher_ids_cfg(regs)


def test_escolha_de_ids_ignora_descartes():
    regs = registros_validos((6, 6, 6, 6))
    for r in regs[:5]:
        r["motivo"] = "sem_erro"
    ids = cc.escolher_ids_cfg(regs)
    assert not ids & {r["id"] for r in regs[:5]}


def test_positivos_e_negativos_pareados():
    regs = registros_validos((4, 4, 4, 4))
    arqs = {}
    for k in range(4):
        arqs.update(arquivos([(f"c{k}", i) for i in range(4)]))
    pos = cc.construir_positivos_cfg(regs, {r["id"] for r in regs}, lambda rel: arqs[rel].texto)
    assert len(pos) == len(regs)
    for p in pos:
        assert (p.classe, p.operador, p.codigo, p.estilo) == ("cfg", "cfg_fn", "E0425", "cfg")
        assert p.texto_novo != p.texto_original and p.texto_novo.count("#[cfg(test)]") == p.texto_original.count("#[cfg(test)]") + 1
    neg = cc.negativos_pareados(pos)
    assert len(neg) == len(pos) and {n.pareado_com for n in neg} == {p.id for p in pos} and len({n.id for n in neg}) == len(neg)
    por_id = {p.id: p for p in pos}
    assert all(n.texto == por_id[n.pareado_com].texto_original and n.rel == por_id[n.pareado_com].rel and n.id != n.pareado_com for n in neg)


def corpus_minimo(tmp_path, n=4, crates=("c0", "c1", "c2", "c3", "c4")):
    """Mundo falso: worktree com os originais; corpus-cfg escrito pelo escritor real."""
    raiz = tmp_path / "wt"
    regs = []
    arqs = {}
    for c in crates:
        a = arquivos([(c, i) for i in range(n)])
        arqs.update(a)
        for t in cc.planejar_cfg(a, set(), "sem", n):
            regs.append(cc.registro_cfg(t, 1, "", oc.Diag(t.inj.arq, t.inj.linha, 5, "E0425", f"cannot find function `{t.inj.nome}` in this scope")))
    for rel, a in arqs.items():
        (raiz / rel).parent.mkdir(parents=True, exist_ok=True)
        (raiz / rel).write_text(a.texto)
    pos = cc.construir_positivos_cfg(regs, {r["id"] for r in regs}, lambda rel: arqs[rel].texto)
    neg = cc.negativos_pareados(pos)
    corpus = tmp_path / "corpus-cfg"
    ch.escrever_corpus(corpus, raiz, pos, neg, {"nome": "HORUST-REAL-CFG"}, estrato="horust-real-cfg")
    return corpus, raiz, pos


def test_corpus_cfg_traz_estrato_classe_e_pareamento(tmp_path):
    corpus, _raiz, pos = corpus_minimo(tmp_path)
    ent = [json.loads(x) for x in (corpus / "entradas.jsonl").read_text().splitlines()]
    assert {e["estrato"] for e in ent} == {"horust-real-cfg"} and {e["classe"] for e in ent if e["rotulo"] == "positivo"} == {"cfg"}
    negs = [e for e in ent if e["rotulo"] == "negativo"]
    assert len(negs) == len(pos) and all(e["pareado_com"] for e in negs) and all("pareado_com" not in e for e in ent if e["rotulo"] == "positivo")


def test_corpus_principal_continua_sem_pareado_com(tmp_path):
    raiz = tmp_path / "wt"
    (raiz / "crates/p/src").mkdir(parents=True)
    (raiz / "crates/p/src/a.rs").write_text("x")
    ch.escrever_corpus(tmp_path / "c", raiz, [], [ch.Negativo("n1", "crates/p/src/a.rs", "p", "x")], {})
    e = json.loads((tmp_path / "c/entradas.jsonl").read_text())
    assert e["estrato"] == "horust-real" and "pareado_com" not in e


# ------------------------------------------------------------------ integridade


def verificar(corpus, raiz, principal=frozenset(), dev=frozenset(), controle=frozenset(), minimo=20, maximo=40, teto=0.25):
    return ic.verificar(corpus, raiz, set(dev), set(controle), set(principal), minimo, maximo, teto)


def test_integro_nao_tem_violacao(tmp_path):
    corpus, raiz, _pos = corpus_minimo(tmp_path)
    assert verificar(corpus, raiz) == []


def trocar(corpus, ident, campo, valor):
    p = corpus / "entradas.jsonl"
    linhas = [json.loads(x) for x in p.read_text().splitlines()]
    for e in linhas:
        if e["id"] == ident:
            e[campo] = valor
    p.write_text("\n".join(json.dumps(e, sort_keys=True) for e in linhas) + "\n")


def primeiro(corpus, rotulo):
    return next(e for e in (json.loads(x) for x in (corpus / "entradas.jsonl").read_text().splitlines()) if e["rotulo"] == rotulo)


def test_snap_sem_a_linha_cfg_e_violacao(tmp_path):
    corpus, raiz, _ = corpus_minimo(tmp_path)
    e = primeiro(corpus, "positivo")
    snap = corpus / e["snap"]
    snap.write_text(snap.read_text().replace("#[cfg(test)]\nfn ", "fn ", 1))
    trocar(corpus, e["id"], "snap_sha256", ic._sha(snap.read_bytes()))
    assert any("cfg" in v and e["id"] in v for v in verificar(corpus, raiz))


def test_snap_com_duas_linhas_a_mais_ou_outra_edicao_e_violacao(tmp_path):
    corpus, raiz, _ = corpus_minimo(tmp_path)
    e = primeiro(corpus, "positivo")
    snap = corpus / e["snap"]
    snap.write_text(snap.read_text().replace("x + 1", "x + 2"))
    trocar(corpus, e["id"], "snap_sha256", ic._sha(snap.read_bytes()))
    assert any(e["id"] in v for v in verificar(corpus, raiz))


def test_cfg_acima_de_outra_coisa_que_nao_a_funcao_nomeada_e_violacao(tmp_path):
    corpus, raiz, _ = corpus_minimo(tmp_path)
    e = primeiro(corpus, "positivo")
    trocar(corpus, e["id"], "nome_esperado", "outro_nome")
    assert any(e["id"] in v for v in verificar(corpus, raiz))


def test_original_do_worktree_alterado_e_violacao(tmp_path):
    corpus, raiz, _ = corpus_minimo(tmp_path)
    e = primeiro(corpus, "positivo")
    f = Path(e["path"])
    f.write_text(f.read_text() + "\n// editado\n")
    assert any("sha_original" in v and e["id"] in v for v in verificar(corpus, raiz))


def test_negativo_que_difere_do_par_ou_sem_par_e_violacao(tmp_path):
    corpus, raiz, _ = corpus_minimo(tmp_path)
    n = primeiro(corpus, "negativo")
    trocar(corpus, n["id"], "pareado_com", "inexistente")
    assert any(n["id"] in v and "par" in v for v in verificar(corpus, raiz))
    n2 = primeiro(corpus, "negativo")
    (corpus / n2["snap"]).write_text("// outro\n")
    assert any(n2["id"] in v for v in verificar(corpus, raiz))


def test_positivo_sem_negativo_pareado_e_violacao(tmp_path):
    corpus, raiz, _ = corpus_minimo(tmp_path)
    n = primeiro(corpus, "negativo")
    p = corpus / "entradas.jsonl"
    p.write_text("\n".join(x for x in p.read_text().splitlines() if json.loads(x)["id"] != n["id"]) + "\n")
    assert any("sem negativo pareado" in v for v in verificar(corpus, raiz))


@pytest.mark.parametrize("conjunto", ["principal", "dev", "controle"])
def test_arquivo_do_principal_do_dev_ou_do_controle_e_violacao(tmp_path, conjunto):
    corpus, raiz, pos = corpus_minimo(tmp_path)
    rel = pos[0].rel
    assert any(rel in v for v in verificar(corpus, raiz, **{conjunto: {rel}}))


def test_fixture_vrs_e_violacao(tmp_path):
    corpus, raiz, _pos = corpus_minimo(tmp_path, crates=("touring-pyright", "c1", "c2", "c3", "c4"))
    assert any("fixture_vrs" in v for v in verificar(corpus, raiz))


def test_acima_do_teto_por_crate_e_do_maximo_e_violacao(tmp_path):
    corpus, raiz, _ = corpus_minimo(tmp_path, n=5)  # 25 positivos, 5 por crate = 20%
    assert verificar(corpus, raiz) == []
    assert any("teto" in v for v in verificar(corpus, raiz, teto=0.10))
    assert any("máximo" in v for v in verificar(corpus, raiz, maximo=20))


def test_codigo_diferente_de_e0425_ou_classe_diferente_de_cfg_e_violacao(tmp_path):
    corpus, raiz, _ = corpus_minimo(tmp_path)
    e = primeiro(corpus, "positivo")
    trocar(corpus, e["id"], "codigo_esperado", "E0599")
    assert any(e["id"] in v and "E0425" in v for v in verificar(corpus, raiz))
    e2 = [x for x in (json.loads(y) for y in (corpus / "entradas.jsonl").read_text().splitlines()) if x["rotulo"] == "positivo"][1]
    trocar(corpus, e2["id"], "classe", "nome")
    assert any(e2["id"] in v and "classe" in v for v in verificar(corpus, raiz))


def test_piloto_quando_n_abaixo_do_minimo_nao_e_violacao_mas_e_declarado(tmp_path):
    corpus, raiz, pos = corpus_minimo(tmp_path, n=2)  # 10 positivos
    assert verificar(corpus, raiz, minimo=20) == [] and ic.piloto(len(pos), 20) is True and ic.piloto(20, 20) is False


def test_construir_positivos_so_devolve_os_ids_pedidos():
    regs = registros_validos((3, 3))
    arqs = {}
    for k in range(2):
        arqs.update(arquivos([(f"c{k}", i) for i in range(3)]))
    pedido = {regs[0]["id"], regs[4]["id"]}
    pos = cc.construir_positivos_cfg(regs, pedido, lambda rel: arqs[rel].texto)
    assert {p.id for p in pos} == pedido


def test_nome_citado_so_em_comentario_ou_string_de_outro_arquivo_nao_exclui():
    arqs = arquivos([("p", 0), ("p", 1)])
    outro = arqs["crates/p/src/m1.rs"]
    arqs[outro.rel] = ch.Arquivo(outro.rel, "p", "lib", outro.texto + '\n// ver ajuda_p_0\nconst S: &str = "ajuda_p_0";\n')
    assert {t.inj.arq for t in cc.planejar_cfg(arqs, set(), "sem", 10)} == set(arqs)


def test_integridade_estrato_errado_e_linha_do_erro_errada_sao_violacao(tmp_path):
    corpus, raiz, _ = corpus_minimo(tmp_path)
    e = primeiro(corpus, "positivo")
    trocar(corpus, e["id"], "estrato", "horust-real")
    assert any("estrato" in v and e["id"] in v for v in verificar(corpus, raiz))
    trocar(corpus, e["id"], "estrato", "horust-real-cfg")
    trocar(corpus, e["id"], "linha", 1)
    assert any("linha do erro" in v and e["id"] in v for v in verificar(corpus, raiz))


def test_limites_do_teto_e_do_maximo_sao_inclusivos(tmp_path):
    corpus, raiz, pos = corpus_minimo(tmp_path, n=5)  # 25 positivos, 5 por crate
    assert len(pos) == 25
    assert verificar(corpus, raiz, teto=0.20, maximo=25) == []  # 5 == int(0.20*25) e 25 == maximo
    assert any("teto" in v for v in verificar(corpus, raiz, teto=0.19))
    assert any("máximo" in v for v in verificar(corpus, raiz, maximo=24))


def _regravar_snap(corpus, e, novo_texto):
    snap = corpus / e["snap"]
    snap.write_text(novo_texto)
    trocar(corpus, e["id"], "snap_sha256", ic._sha(snap.read_bytes()))


def test_linha_inserida_que_nao_e_o_cfg_test_e_violacao(tmp_path):
    corpus, raiz, _ = corpus_minimo(tmp_path)
    e = primeiro(corpus, "positivo")
    texto_snap = (corpus / e["snap"]).read_text()
    _regravar_snap(corpus, e, texto_snap.replace("#[cfg(test)]\nfn ", "#[inline]\nfn ", 1))
    assert any(e["id"] in v and "cfg" in v for v in verificar(corpus, raiz))


def test_cfg_inserido_acima_de_outra_funcao_e_violacao_mesmo_com_a_chamada_na_linha_certa(tmp_path):
    corpus, raiz, _ = corpus_minimo(tmp_path)
    e = primeiro(corpus, "positivo")
    linhas = (corpus / e["snap"]).read_text().split("\n")
    linhas.pop(linhas.index("#[cfg(test)]"))
    alvo = next(i for i, ln in enumerate(linhas) if ln.startswith("pub fn principal"))
    linhas.insert(alvo, "#[cfg(test)]")  # mesma contagem de linhas e a chamada continua na linha esperada
    _regravar_snap(corpus, e, "\n".join(linhas))
    vs = [v for v in verificar(corpus, raiz) if e["id"] in v]
    assert any("está acima de" in v for v in vs) and not any("linha do erro" in v for v in vs)
