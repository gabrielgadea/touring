"""HORUST-REAL — operadores de injeção (TDD). Texto Rust SINTÉTICO: cada operador acha o sítio certo, ignora o que é comentário,
string, atributo, macro e código de teste, e produz um nome novo com a distância de edição pré-registrada.
O módulo sob teste vem de `$HORUST_SRC_DIR` (os mutantes rodam sobre uma cópia).
"""

import os
import re
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(Path(os.environ.get("HORUST_SRC_DIR", HERE))))
import operadores_rust as op  # noqa: E402

BASE = """use crate::types::Thing;
use super::helper;
// comentario_falso(1) tem forma de chamada
fn exemplo(x: &Config) -> Result<Thing, Error> {
    let s = "texto com chamada falsa(1)";
    let y = calcula_total(x.valor, 3);
    let z = Builder::novo(y).com_nome("a");
    if let Some(v) = x.itens.first() { return Ok(v.clone()); }
    helper::ajuda(y)
}
"""


def achados(texto, operador):
    return [s for s in op.sitios(texto) if s.operador == operador]


def nomes(texto, operador):
    return sorted(s.nome for s in achados(texto, operador))


# ------------------------------------------------------------------ máscara


def test_mascara_preserva_tamanho_e_quebras():
    t = 'let a = "x(1)"; // y(2)\nlet b = 3;\n'
    m = op.mascarar(t)
    assert len(m) == len(t) and m.count("\n") == t.count("\n")
    assert "x(1)" not in m and "y(2)" not in m and "let a" in m and "let b" in m


@pytest.mark.parametrize(
    "t,some",
    [
        ('let s = r#"aspa " dentro foo(1)"#; ok(2)', "foo(1)"),
        ("/* a /* aninhado foo(1) */ ainda comentario foo(2) */ ok(3)", "foo("),
        ("let c = '\"'; let d = \"foo(1)\"; ok(4)", "foo(1)"),
        ('let s = "abc \\" foo(1) \\" def"; ok(5)', "foo(1)"),
    ],
)
def test_mascara_literais_e_comentarios(t, some):
    m = op.mascarar(t)
    assert some not in m
    assert re.search(r"ok\(\d\)", m), "o código depois do literal tem de sobreviver"


def test_mascara_lifetime_nao_abre_char():
    m = op.mascarar("fn f<'a>(x: &'a str) -> &'a str { chama(x) }")
    assert "chama(x)" in m


# ------------------------------------------------------------------ nome


def test_nome_valor_acha_chamada_livre_e_ignora_o_resto():
    assert nomes(BASE, "nome_valor") == ["calcula_total"]
    s = achados(BASE, "nome_valor")[0]
    assert (s.classe, s.codigo) == ("nome", "E0425") and BASE.splitlines()[s.linha - 1][s.inicio : s.fim] == "calcula_total"


def test_nome_tipo_acha_tipo_de_projeto_e_ignora_prelude():
    n = nomes(BASE, "nome_tipo")
    assert "Config" in n and "Thing" in n
    assert "Result" not in n and "Some" not in n
    # o rustc deste worktree diz «error[E0425]: cannot find type»; o E0412 clássico também vale
    assert all(s.codigo == "E0425" and s.aceitos == ("E0425", "E0412") for s in achados(BASE, "nome_tipo"))
    assert all(s.aceitos == (s.codigo,) for s in achados(BASE, "nome_valor"))


# ------------------------------------------------------------------ import


def test_import_use_simples_penultimo_e_ultimo_segmento():
    t = "use crate::alfa::beta::Gama;\n"
    s = achados(t, "import_use")
    assert sorted(x.nome for x in s) == ["Gama", "beta"]
    assert all(x.codigo == "E0432" and x.classe == "import" for x in s)


@pytest.mark.parametrize(
    "linha",
    ["pub use crate::a::B;", "use std::collections::HashMap;", "use crate::a::B as C;", "use crate::a::{B, C};", "use crate::a::*;"],
)
def test_import_use_exclui_pub_std_alias_e_glob(linha):
    assert achados(linha + "\n", "import_use") == []


def test_import_use_com_chaves_muta_um_nome_da_lista():
    s = achados("use crate::a::b::{Alfa, Beta, self};\n", "import_use_chaves")
    assert sorted(x.nome for x in s) == ["Alfa", "Beta"] and all(x.codigo == "E0432" for x in s)


def test_import_caminho_tipo_e_modulo():
    n = nomes(BASE, "import_caminho")
    assert n == ["Builder", "helper"]
    assert all(s.codigo == "E0433" for s in achados(BASE, "import_caminho"))


@pytest.mark.parametrize("linha", ["let a = String::from(\"x\");", "let b = std::mem::take(&mut x);", "let c = Self::novo();", "let d = Vec::new();"])
def test_import_caminho_ignora_std_e_prelude(linha):
    assert achados(linha + "\n", "import_caminho") == []


# ------------------------------------------------------------------ atributo


def test_atributo_campo_e_metodo():
    campos = nomes(BASE, "atributo_campo")
    metodos = nomes(BASE, "atributo_metodo")
    assert "valor" in campos and "itens" in campos
    assert "first" in metodos and "com_nome" in metodos and "clone" in metodos
    assert "first" not in campos and "valor" not in metodos
    assert {s.codigo for s in achados(BASE, "atributo_campo")} == {"E0609"}
    assert {s.codigo for s in achados(BASE, "atributo_metodo")} == {"E0599"}


def test_atributo_nao_pega_await_nem_indice_de_tupla_nem_update():
    t = "let a = fut.await; let b = t.0; let c = Foo { x: 1, ..Default::default() };\n"
    assert nomes(t, "atributo_campo") == []


# ------------------------------------------------------------------ kwarg

LITERAL = """fn f() -> Config {
    Config {
        nome: String::from("a"),
        limite: 10,
        ativo: true,
    }
}
"""


def test_kwarg_chave_e_remocao_no_literal_de_varias_linhas():
    assert nomes(LITERAL, "kwarg_chave") == ["ativo", "limite", "nome"]
    r = achados(LITERAL, "kwarg_remove")
    assert sorted(s.nome for s in r) == ["ativo", "limite", "nome"]
    assert {(s.codigo, s.tipo) for s in r} == {("E0063", "Config")}
    assert {s.codigo for s in achados(LITERAL, "kwarg_chave")} == {"E0560"}


def test_kwarg_ignora_padrao_struct_e_impl():
    t = """struct Config {
    pub nome: String,
    pub limite: u32,
}
impl Config {
    fn g(&self) -> u32 {
        match self {
            Config {
                nome: _,
                limite: l,
            } => *l,
        }
    }
    fn h(&self) {
        let Config {
            nome: n,
            limite: l,
        } = self.clone();
    }
}
"""
    assert achados(t, "kwarg_chave") == [] and achados(t, "kwarg_remove") == []


def test_kwarg_remocao_exige_mais_de_um_campo_e_nao_ter_update():
    um = "fn f() {\n    Foo {\n        a: 1,\n    }\n}\n"
    upd = "fn f() {\n    Foo {\n        a: 1,\n        b: 2,\n        ..Default::default()\n    }\n}\n"
    assert achados(um, "kwarg_remove") == []
    assert achados(upd, "kwarg_remove") == []
    assert sorted(s.nome for s in achados(upd, "kwarg_chave")) == ["a", "b"]


def test_kwarg_campo_com_valor_de_varias_linhas_nao_e_sitio():
    t = "fn f() {\n    Foo {\n        a: vec![\n            1,\n        ],\n        b: 2,\n        c: 3,\n    }\n}\n"
    assert nomes(t, "kwarg_remove") == ["b", "c"]


# ------------------------------------------------------------------ zonas excluídas


def test_nada_depois_do_cfg_test_nem_em_macro_nem_em_atributo():
    t = """#[derive(Debug)]
#[serde(default = "padrao_falso")]
fn a() { vivo(1) }
macro_rules! m {
    () => { dentro_macro(2) };
}
#[cfg(test)]
mod tests {
    fn t() { dentro_teste(3) }
}
"""
    assert nomes(t, "nome_valor") == ["vivo"]


# ------------------------------------------------------------------ variantes de nome


def test_distancia_de_edicao():
    assert op.distancia("kitten", "sitting") == 3
    assert op.distancia("abc", "abc") == 0 and op.distancia("abc", "abd") == 1 and op.distancia("ab", "ba") == 2


@pytest.mark.parametrize("nome,tipo", [("calcula_total", "valor"), ("Config", "tipo"), ("itens", "campo"), ("helper", "modulo")])
def test_near_miss_tem_distancia_1_ou_2_e_e_identificador_valido(nome, tipo):
    vistos = set()
    for i in range(60):
        v = op.variante(nome, tipo, "near_miss", f"chave-{i}", existentes=set())
        assert v and v != nome and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", v)
        assert 1 <= op.distancia(nome, v) <= 2
        assert v[0].isupper() == nome[0].isupper(), "tipo continua tipo, valor continua valor"
        vistos.add(op.distancia(nome, v))
    assert vistos == {1, 2}, "as duas distâncias pré-registradas aparecem"


@pytest.mark.parametrize("nome,tipo", [("calcula_total", "valor"), ("Config", "tipo"), ("itens", "campo"), ("helper", "modulo")])
def test_plausivel_acrescenta_algo_ao_nome_original(nome, tipo):
    for i in range(30):
        v = op.variante(nome, tipo, "plausivel", f"c{i}", existentes=set())
        assert v and v != nome and nome.lower() in v.lower() and op.distancia(nome, v) > 2


def test_variante_e_deterministica_e_respeita_existentes_e_palavras_reservadas():
    a = op.variante("itens", "campo", "near_miss", "k", existentes=set())
    assert a == op.variante("itens", "campo", "near_miss", "k", existentes=set())
    assert op.variante("itens", "campo", "near_miss", "k", existentes={a}) not in (None, a)
    tudo = {op.variante("ab", "campo", "near_miss", f"x{i}", existentes=set()) for i in range(200)}
    assert "as" not in tudo and "fn" not in tudo
    assert op.variante("ab", "campo", "plausivel", "k", existentes={f"ab_{s}" for s in ("id", "count", "all")} | {"try_ab"}) is None


def test_variante_uma_so_opcao_cheia_devolve_none():
    cheio = {f"get_x{i}" for i in range(5)}
    assert op.variante("x1", "valor", "plausivel", "k", existentes=cheio | {"try_x1", "x1_all", "x1_with", "get_x1"}) is None


# ------------------------------------------------------------------ mutar


def test_mutar_troca_so_o_span_e_mantem_o_resto():
    s = achados(BASE, "nome_valor")[0]
    novo = op.mutar(BASE, s, "calcula_totel")
    antigas, novas = BASE.splitlines(), novo.splitlines()
    assert len(antigas) == len(novas)
    difs = [i for i, (a, b) in enumerate(zip(antigas, novas)) if a != b]
    assert difs == [s.linha - 1] and "calcula_totel(x.valor, 3)" in novas[s.linha - 1]


def test_mutar_remocao_tira_a_linha_do_campo():
    s = [x for x in achados(LITERAL, "kwarg_remove") if x.nome == "limite"][0]
    novo = op.mutar(LITERAL, s, "")
    assert "limite" not in novo and len(novo.splitlines()) == len(LITERAL.splitlines()) - 1
    assert "nome:" in novo and "ativo:" in novo


def test_mutar_recusa_sitio_que_nao_bate_com_o_texto():
    s = achados(BASE, "nome_valor")[0]
    with pytest.raises(ValueError):
        op.mutar(BASE.replace("calcula_total", "outro_nome_x"), s, "calcula_totel")


def test_tokens_do_crate():
    assert op.tokens("fn a_b(x: Foo) { let y_1 = x; }") >= {"a_b", "Foo", "y_1", "fn", "x"}


def test_atributo_metodo_em_linha_de_cadeia_que_comeca_com_ponto():
    t = "fn f() {\n    let r = lista\n        .iter()\n        .map(|x| x + 1)\n        .collect::<Vec<_>>();\n}\n"
    assert nomes(t, "atributo_metodo") == ["collect", "iter", "map"]


def test_near_miss_distancia_2_aparece_em_cerca_de_30_por_cento():
    n = 600
    dois = sum(op.distancia("calcula_total", op.variante("calcula_total", "valor", "near_miss", f"k{i}", set())) == 2 for i in range(n))
    assert 0.20 <= dois / n <= 0.40, dois / n


def test_nome_curto_e_palavra_reservada_nao_sao_sitio():
    t = "fn f() {\n    let a = f(1);\n    let b = id(2);\n    while (a < 3) { loop_fn(); }\n    match (b) { _ => {} }\n    if (a) { return(b); }\n}\n"
    assert nomes(t, "nome_valor") == ["loop_fn"]


def test_import_use_na_raiz_nao_muta_o_segmento_crate():
    assert [x.nome for x in achados("use crate::Gama;\n", "import_use")] == ["Gama"]


def test_kwarg_literal_seguido_de_igualdade_e_literal_mas_com_guarda_e_padrao():
    literal = "fn f(c: Config) -> bool {\n    let eq = Config {\n        a: 1,\n        b: 2,\n    } == c;\n    eq\n}\n"
    assert nomes(literal, "kwarg_chave") == ["a", "b"]
    guarda = "fn g(c: Config) -> u8 {\n    match c {\n        Config {\n            a: _,\n            b: 5,\n        } if flag => 1,\n        _ => 0,\n    }\n}\n"
    assert achados(guarda, "kwarg_chave") == []


def test_kwarg_ignora_definicao_de_struct_com_campos_privados_e_literal_de_self():
    t = "struct Privada {\n    nome: String,\n    limite: u32,\n}\nimpl Privada {\n    fn novo() -> Self {\n        Self {\n            nome: String::new(),\n            limite: 3,\n        }\n    }\n}\n"
    assert achados(t, "kwarg_chave") == [] and achados(t, "kwarg_remove") == []


def test_import_use_marca_a_posicao_do_segmento():
    s = {x.nome: x.posicao for x in achados("use crate::alfa::beta::Gama;\n", "import_use")}
    assert s == {"Gama": "ultimo", "beta": "penultimo"}
    assert all(x.posicao == "" for x in achados(BASE, "nome_valor"))
