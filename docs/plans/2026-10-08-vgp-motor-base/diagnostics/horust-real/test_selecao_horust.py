"""HORUST-REAL — seleção (TDD): ordem por hash, round-robin entre pacotes, teto de concentração, elegibilidade, disjunção e ids.
O módulo sob teste vem de `$HORUST_SRC_DIR` (os mutantes rodam sobre uma cópia).
"""

import collections
import os
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(Path(os.environ.get("HORUST_SRC_DIR", HERE))))
import selecao_horust as sel  # noqa: E402

SEMENTE = "horust-real-20261010"


def test_ordem_e_deterministica_e_depende_da_semente():
    chaves = [f"arq{i}.rs" for i in range(30)]
    assert sel.ordem(chaves, SEMENTE) == sel.ordem(list(reversed(chaves)), SEMENTE)
    assert sel.ordem(chaves, SEMENTE) != sel.ordem(chaves, SEMENTE + "-r2")
    assert sorted(sel.ordem(chaves, SEMENTE)) == sorted(chaves)


def test_round_robin_espalha_entre_os_pacotes():
    grupos = {"a": [f"a{i}" for i in range(50)], "b": [f"b{i}" for i in range(50)], "c": [f"c{i}" for i in range(50)], "d": ["d0"]}
    esc = sel.round_robin(grupos, 40, SEMENTE)
    cont = collections.Counter(x[0] for x in esc)
    assert len(esc) == 40 and len(set(esc)) == 40
    assert max(cont.values()) <= 14 and cont["d"] == 1, cont


def test_round_robin_esgota_sem_repetir_e_sem_inventar():
    esc = sel.round_robin({"a": ["a0", "a1"], "b": ["b0"]}, 10, SEMENTE)
    assert sorted(esc) == ["a0", "a1", "b0"]


def test_round_robin_e_deterministico():
    g = {"a": [f"a{i}" for i in range(9)], "b": [f"b{i}" for i in range(9)]}
    assert sel.round_robin(g, 7, SEMENTE) == sel.round_robin(dict(reversed(list(g.items()))), 7, SEMENTE)


def test_teto_corta_o_crate_dominante_e_maximiza_n():
    por_crate = {"a": [f"a{i}" for i in range(30)], "b": [f"b{i}" for i in range(6)], "c": [f"c{i}" for i in range(6)], "d": [f"d{i}" for i in range(6)], "e": [f"e{i}" for i in range(6)]}
    esc = sel.selecao_com_teto(por_crate, 0.25, SEMENTE)
    cont = collections.Counter(x[0] for x in esc)
    n = len(esc)
    assert all(v <= int(0.25 * n) for v in cont.values()), (n, cont)
    assert n == 32 and cont["a"] == 8 and all(cont[c] == 6 for c in "bcde"), "o maior N viável: 8 de 32 é exatamente 25%"


def test_teto_sem_crate_dominante_devolve_tudo():
    por_crate = {c: [f"{c}{i}" for i in range(5)] for c in "abcde"}
    assert len(sel.selecao_com_teto(por_crate, 0.25, SEMENTE)) == 25


def test_teto_inviavel_com_um_crate_so_devolve_vazio():
    assert sel.selecao_com_teto({"a": ["a0", "a1", "a2"]}, 0.25, SEMENTE) == []


def test_teto_e_deterministico_na_ordem():
    por = {"a": [f"a{i}" for i in range(20)], "b": [f"b{i}" for i in range(20)], "c": [f"c{i}" for i in range(2)]}
    assert sel.selecao_com_teto(por, 0.5, SEMENTE) == sel.selecao_com_teto(dict(reversed(list(por.items()))), 0.5, SEMENTE)


@pytest.mark.parametrize(
    "linhas,tamanho,ok",
    [(14, 100, False), (15, 100, True), (40, 200_000, True), (40, 200_001, False)],
)
def test_elegibilidade_por_tamanho(linhas, tamanho, ok):
    assert sel.tamanho_elegivel(linhas, tamanho) is ok


def test_excluidos_por_prefixo_e_por_conjunto():
    dev, ctrl = {"crates/a/src/x.rs"}, {"crates/b/src/y.rs"}
    assert sel.motivo_exclusao("crates/a/src/x.rs", dev, ctrl) == "dev"
    assert sel.motivo_exclusao("crates/b/src/y.rs", dev, ctrl) == "controle"
    assert sel.motivo_exclusao("crates/touring-code/src/vgp/lang/rust.rs", dev, ctrl) == "fixture_vrs"
    assert sel.motivo_exclusao("crates/touring-pyright/src/lib.rs", dev, ctrl) == "fixture_vrs"
    assert sel.motivo_exclusao("crates/touring-identity/src/lib.rs", dev, ctrl) == "fixture_vrs"
    assert sel.motivo_exclusao("crates/c/src/z.rs", dev, ctrl) == ""


def test_arquivo_em_mais_de_um_alvo_e_ambiguo():
    por_alvo = {("p", "lib"): {"a.rs", "b.rs"}, ("p", "bins"): {"b.rs", "c.rs"}, ("q", "lib"): {"d.rs"}}
    assert sel.arquivos_ambiguos(por_alvo) == {"b.rs"}
    assert sel.arquivos_ambiguos({("p", "lib"): {"a.rs"}, ("q", "lib"): {"a.rs"}}) == {"a.rs"}


def test_interseccao_e_vazia_ou_nomeia_o_vazamento():
    assert sel.vazamento({"a", "b"}, {"c"}) == set()
    assert sel.vazamento({"a", "b"}, {"b", "c"}) == {"b"}


def test_id_da_injecao_e_deterministico_12_hex_e_muda_com_cada_campo():
    base = ("crates/a/src/x.rs", "nome", 10, "foo", SEMENTE)
    i = sel.id_injecao(*base)
    assert len(i) == 12 and int(i, 16) >= 0 and i == sel.id_injecao(*base)
    for k, v in enumerate(("crates/a/src/y.rs", "import", 11, "bar", "outra")):
        alterado = list(base)
        alterado[k] = v
        assert sel.id_injecao(*alterado) != i


def test_fase_da_classe():
    assert [sel.fase(c) for c in ("nome", "import", "atributo", "kwarg")] == ["R", "R", "T", "T"]
    with pytest.raises(KeyError):
        sel.fase("outra")


def test_cotas_por_operador_somam_o_total_e_respeitam_os_pesos():
    c = sel.cotas("nome", 72)
    assert sum(c.values()) == 72 and c == {"nome_valor": 43, "nome_tipo": 29}
    assert sum(sel.cotas("kwarg", 72).values()) == 72 and sel.cotas("kwarg", 72) == {"kwarg_chave": 48, "kwarg_remove": 24}
    assert sum(sel.cotas("import", 72).values()) == 72
    assert sel.cotas("atributo", 72) == {"atributo_campo": 36, "atributo_metodo": 36}


def test_estilo_alterna_50_50():
    estilos = [sel.estilo_do(i) for i in range(10)]
    assert estilos.count("near_miss") == estilos.count("plausivel") == 5


def test_round_robin_nao_depende_da_ordem_dos_itens_no_grupo():
    itens = [f"a{i}" for i in range(12)]
    assert sel.round_robin({"a": itens}, 5, SEMENTE) == sel.round_robin({"a": list(reversed(itens))}, 5, SEMENTE)


# ------------------------------------------------------------------ antecadeias (pacotes independentes no mesmo cargo)


def relacionados(a, b, deps):
    def alcanca(x, y, visto=None):
        visto = visto or set()
        return any(d == y or (d not in visto and alcanca(d, y, visto | {d})) for d in deps.get(x, ()))

    return alcanca(a, b) or alcanca(b, a)


def test_antecadeias_cobrem_todos_os_nos_sem_par_relacionado():
    deps = {"a": {"b"}, "b": {"c"}, "d": set(), "e": {"c"}, "f": {"d"}}
    grupos = sel.antecadeias(["a", "b", "c", "d", "e", "f"], deps)
    assert sorted(x for g in grupos for x in g) == ["a", "b", "c", "d", "e", "f"]
    for g in grupos:
        assert not any(relacionados(x, y, deps) for x in g for y in g if x < y), g


def test_antecadeias_dependencia_transitiva_por_no_que_nao_esta_na_lista_separa():
    deps = {"a": {"meio"}, "meio": {"c"}}
    grupos = sel.antecadeias(["a", "c", "z"], deps)
    assert not any("a" in g and "c" in g for g in grupos)
    assert any("z" in g for g in grupos)


def test_antecadeias_sem_dependencias_e_um_grupo_so_e_e_deterministico():
    nos = [f"p{i}" for i in range(8)]
    assert sel.antecadeias(nos, {}) == [sorted(nos)]
    deps = {"a": {"b"}, "b": set(), "c": set()}
    assert sel.antecadeias(["c", "b", "a"], deps) == sel.antecadeias(["a", "b", "c"], deps)


def test_antecadeias_cadeia_linear_vira_um_grupo_por_no():
    deps = {"a": {"b"}, "b": {"c"}, "c": set()}
    assert len(sel.antecadeias(["a", "b", "c"], deps)) == 3
