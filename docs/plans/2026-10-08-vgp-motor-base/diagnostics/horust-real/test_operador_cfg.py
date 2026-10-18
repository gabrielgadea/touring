"""HORUST-REAL / estrato cfg — o operador que move uma definição para `#[cfg(test)]` e deixa o uso vivo no alvo não-teste."""

import os
import sys
from pathlib import Path

import pytest

sys.path.insert(0, os.environ.get("HORUST_SRC_DIR") or str(Path(__file__).resolve().parent))
import operador_cfg as oc_  # noqa: E402

BASE = """use std::fmt;

fn auxiliar(x: u32) -> u32 {
    x + 1
}

pub fn principal() -> u32 {
    auxiliar(3)
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


def so(texto, outros=frozenset()):
    return oc_.sitios_cfg(texto, outros)


def test_um_sitio_simples_com_linha_da_definicao_e_do_uso():
    (s,) = so(BASE)
    assert (s.nome, s.linha_def, s.linha_uso) == ("auxiliar", 3, 8)


def test_mutar_insere_o_cfg_logo_acima_da_definicao_e_o_erro_cai_uma_linha_abaixo():
    (s,) = so(BASE)
    novo = oc_.mutar_cfg(BASE, s)
    linhas = novo.split("\n")
    assert linhas[2] == "#[cfg(test)]" and linhas[3].startswith("fn auxiliar")
    assert len(linhas) == len(BASE.split("\n")) + 1
    assert oc_.linha_do_erro(s) == 9 and linhas[8].strip() == "auxiliar(3)"


def test_uso_antes_da_definicao_nao_desloca_a_linha_do_erro():
    texto = "pub fn topo() -> u32 {\n    ajuda(1)\n}\n\nfn ajuda(x: u32) -> u32 {\n    x\n}\n"
    (s,) = so(texto)
    assert (s.linha_def, s.linha_uso) == (5, 2) and oc_.linha_do_erro(s) == 2


def test_dois_usos_ou_nenhum_nao_sao_sitio():
    dois = BASE.replace("auxiliar(3)", "auxiliar(3) + auxiliar(4)")
    sem_uso = BASE.replace("    auxiliar(3)\n", "    3\n")
    assert so(dois) == [] and so(sem_uso) == []


def test_uso_so_dentro_da_zona_de_teste_nao_e_sitio_e_e_o_negativo_natural():
    texto = BASE.replace("    auxiliar(3)\n", "    3\n").replace("assert_eq!(principal(), 4);", "assert_eq!(auxiliar(3), 4);")
    assert so(texto) == []


@pytest.mark.parametrize("uso", ["x.auxiliar(3)", "Self::auxiliar(3)", "outro::auxiliar(3)", "auxiliar", "&auxiliar"])
def test_so_chamada_livre_conta_como_uso(uso):
    assert so(BASE.replace("auxiliar(3)", uso)) == []


def test_recursao_nao_e_sitio_porque_o_uso_sai_junto_com_a_definicao():
    texto = "fn conta(n: u32) -> u32 {\n    if n == 0 { 0 } else { conta(n - 1) }\n}\n"
    assert so(texto) == []


def test_definicao_indentada_ou_dentro_de_impl_nao_e_sitio():
    texto = "struct S;\nimpl S {\n    fn auxiliar(&self) -> u32 { 1 }\n}\npub fn f() -> u32 {\n    auxiliar(3)\n}\n"
    assert so(texto) == []


def test_nome_em_comentario_ou_string_nao_conta_como_ocorrencia():
    texto = BASE.replace("    x + 1\n", '    // auxiliar de novo\n    let _s = "auxiliar(9)";\n    x + 1\n')
    (s,) = so(texto)
    assert s.nome == "auxiliar"


def test_nome_presente_em_outro_arquivo_do_pacote_e_excluido():
    assert so(BASE, frozenset({"auxiliar"})) == []
    assert len(so(BASE, frozenset({"outra_coisa"}))) == 1


@pytest.mark.parametrize("atributo", ["#[test]", '#[cfg(feature = "x")]', "#[no_mangle]", '#[cfg_attr(test, allow(dead_code))]'])
def test_atributo_que_muda_o_item_exclui_o_sitio(atributo):
    assert so(BASE.replace("fn auxiliar", f"{atributo}\nfn auxiliar")) == []


def test_atributo_inofensivo_fica_e_o_cfg_entra_entre_ele_e_a_funcao():
    texto = BASE.replace("fn auxiliar", "#[inline]\nfn auxiliar")
    (s,) = so(texto)
    novo = oc_.mutar_cfg(texto, s).split("\n")
    assert novo[2] == "#[inline]" and novo[3] == "#[cfg(test)]" and novo[4].startswith("fn auxiliar")
    assert oc_.linha_do_erro(s) == s.linha_uso + 1


@pytest.mark.parametrize("assinatura", ["pub fn", "pub(crate) fn", "const fn", "pub const fn", "async fn", "unsafe fn", "pub(super) fn"])
def test_modificadores_de_funcao_sao_aceitos(assinatura):
    assert len(so(BASE.replace("fn auxiliar", f"{assinatura} auxiliar", 1))) == 1


def test_definicao_depois_do_primeiro_cfg_test_ou_dentro_de_macro_nao_e_sitio():
    depois = BASE + "\nfn solta(x: u32) -> u32 {\n    x\n}\npub fn g() -> u32 {\n    solta(1)\n}\n"
    macro = "macro_rules! m {\n    () => { ajuda(1) };\n}\nfn ajuda(x: u32) -> u32 {\n    x\n}\n"
    assert [s.nome for s in so(depois)] == ["auxiliar"] and so(macro) == []


def test_nome_curto_ou_palavra_reservada_nao_e_sitio():
    assert so(BASE.replace("auxiliar", "ab")) == []


def test_mutar_recusa_quando_a_linha_nao_e_a_definicao():
    (s,) = so(BASE)
    with pytest.raises(ValueError):
        oc_.mutar_cfg(BASE.replace("fn auxiliar", "fn outra"), s)


def test_dois_sitios_no_mesmo_arquivo_sao_listados_em_ordem_de_linha():
    texto = BASE + ""
    texto = texto.replace("pub fn principal() -> u32 {\n    auxiliar(3)\n}", "fn extra(x: u32) -> u32 {\n    x\n}\n\npub fn principal() -> u32 {\n    extra(auxiliar(3))\n}")
    nomes = [s.nome for s in so(texto)]
    assert nomes == ["auxiliar", "extra"]


def test_comentario_de_documentacao_entre_o_atributo_e_a_funcao_nao_esconde_o_atributo():
    assert so(BASE.replace("fn auxiliar", "#[test]\n/// documenta\nfn auxiliar")) == []
    assert len(so(BASE.replace("fn auxiliar", "#[inline]\n/// documenta\nfn auxiliar"))) == 1


def test_recursao_na_mesma_linha_da_definicao_com_uso_fora_tambem_e_excluida_por_conservadorismo():
    texto = "fn recursiva(n: u32) -> u32 { if n == 0 { 0 } else { recursiva(n - 1) } }\n\npub fn g() -> u32 {\n    recursiva(3)\n}\n"
    assert so(texto) == []


def test_definicao_depois_do_cfg_test_com_uso_antes_dele_nao_e_sitio():
    texto = "pub fn g() -> u32 {\n    solta(1)\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n\nfn solta(x: u32) -> u32 {\n    x\n}\n"
    assert so(texto) == []
