"""HORUST-REAL — integridade do corpus antes de selar (TDD): cada propriedade pré-registrada tem um teste que a viola e é pega.
O módulo sob teste vem de `$HORUST_SRC_DIR` (os mutantes rodam sobre uma cópia).
"""

import copy
import hashlib
import json
import os
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(Path(os.environ.get("HORUST_SRC_DIR", HERE))))
import construtor_horust as ch  # noqa: E402
import integridade_horust as ih  # noqa: E402

CLASSES = ("nome", "import", "atributo", "kwarg")
CODIGO = {"nome": "E0425", "import": "E0432", "atributo": "E0609", "kwarg": "E0560"}
OPERADOR = {"nome": "nome_valor", "import": "import_use", "atributo": "atributo_campo", "kwarg": "kwarg_chave"}


def fabrica(tmp_path, n_por_classe=20, n_neg=320, n_crates=5):
    """Um mini-worktree + um corpus válido por construção."""
    raiz = tmp_path / "wt"
    positivos, negativos, k = [], [], 0
    for classe in CLASSES:
        for i in range(n_por_classe):
            pkg = f"c{i % n_crates}"
            rel = f"crates/{pkg}/src/p{k}.rs"
            (raiz / rel).parent.mkdir(parents=True, exist_ok=True)
            original = f"fn a{k}() {{ calcula_{k}(1) }}\n"
            (raiz / rel).write_text(original)
            positivos.append(ch.Positivo(f"p{k:011d}", rel, pkg, classe, OPERADOR[classe], CODIGO[classe], 1, 5, f"calcula_x{k}", f"calcula_{k}", "near_miss",
                                         f"cannot find value `calcula_x{k}` in this scope", original, original.replace(f"calcula_{k}", f"calcula_x{k}")))
            k += 1
    for i in range(n_neg):
        pkg = f"c{i % n_crates}"
        rel = f"crates/{pkg}/src/n{i}.rs"
        (raiz / rel).parent.mkdir(parents=True, exist_ok=True)
        (raiz / rel).write_text(f"fn n{i}() {{}}\n")
        negativos.append(ch.Negativo(f"n{i:011d}", rel, pkg, f"fn n{i}() {{}}\n"))
    saida = tmp_path / "corpus"
    ch.escrever_corpus(saida, raiz, positivos, negativos, meta={})
    return raiz, saida


def verifica(raiz, saida, dev=frozenset(), controle=frozenset()):
    return ih.verificar(saida, raiz, set(dev), set(controle), minimo_por_classe=20, minimo_negativos=320, teto=0.25)


def test_corpus_valido_nao_tem_violacao(tmp_path):
    raiz, saida = fabrica(tmp_path)
    assert verifica(raiz, saida) == []


def test_pega_arquivo_do_dev_ou_do_controle_ou_fixture(tmp_path):
    raiz, saida = fabrica(tmp_path)
    assert any("dev" in v for v in verifica(raiz, saida, dev={"crates/c0/src/p0.rs"}))
    assert any("controle" in v for v in verifica(raiz, saida, controle={"crates/c1/src/n1.rs"}))
    linhas = [json.loads(x) for x in (saida / "entradas.jsonl").read_text().splitlines()]
    linhas[0]["path"] = str(raiz / "crates/touring-code/src/vgp/x.rs")
    (saida / "entradas.jsonl").write_text("\n".join(json.dumps(x) for x in linhas) + "\n")
    assert any("fixture_vrs" in v for v in verifica(raiz, saida))


def test_pega_arquivo_do_worktree_que_mudou_depois(tmp_path):
    raiz, saida = fabrica(tmp_path)
    (raiz / "crates/c0/src/p0.rs").write_text("alguem mexeu\n")
    assert any("sha_original" in v for v in verifica(raiz, saida))


def test_pega_negativo_diferente_do_arquivo_real(tmp_path):
    raiz, saida = fabrica(tmp_path)
    (saida / "snap" / "n00000000000.rs").write_text("fn outro() {}\n")
    assert any("negativo" in v and "difere" in v for v in verifica(raiz, saida))


def test_pega_snap_adulterado_pelo_sha_da_entrada(tmp_path):
    raiz, saida = fabrica(tmp_path)
    (saida / "snap" / "p00000000000.rs").write_text("fn adulterado() {}\n")
    assert any("snap_sha256" in v for v in verifica(raiz, saida))


def test_pega_mesmo_arquivo_em_positivo_e_negativo_e_em_dois_positivos(tmp_path):
    raiz, saida = fabrica(tmp_path)
    linhas = [json.loads(x) for x in (saida / "entradas.jsonl").read_text().splitlines()]
    pos = [x for x in linhas if x["rotulo"] == "positivo"]
    neg = [x for x in linhas if x["rotulo"] == "negativo"]
    neg[0]["path"] = pos[0]["path"]
    pos[1]["path"] = pos[2]["path"]
    (saida / "entradas.jsonl").write_text("\n".join(json.dumps(x) for x in pos + neg) + "\n")
    v = verifica(raiz, saida)
    assert any("arquivo repetido" in x for x in v) and any("positivo e negativo" in x for x in v)


def test_pega_classe_abaixo_do_minimo_e_negativos_abaixo_do_minimo(tmp_path):
    raiz, saida = fabrica(tmp_path, n_por_classe=19, n_neg=319)
    v = verifica(raiz, saida)
    assert sum("abaixo de 20" in x for x in v) == 4 and any("negativos" in x and "320" in x for x in v)


def test_pega_crate_acima_do_teto_em_classe(tmp_path):
    raiz, saida = fabrica(tmp_path, n_por_classe=20, n_crates=2)  # 10/20 = 50% por crate
    assert any("teto" in x for x in verifica(raiz, saida))


def test_pega_id_duplicado(tmp_path):
    raiz, saida = fabrica(tmp_path)
    linhas = [json.loads(x) for x in (saida / "entradas.jsonl").read_text().splitlines()]
    linhas[1]["id"] = linhas[0]["id"]
    (saida / "entradas.jsonl").write_text("\n".join(json.dumps(x) for x in linhas) + "\n")
    assert any("id duplicado" in x for x in verifica(raiz, saida))


def test_pega_codigo_fora_da_classe_e_mensagem_sem_o_codigo(tmp_path):
    raiz, saida = fabrica(tmp_path)
    linhas = [json.loads(x) for x in (saida / "entradas.jsonl").read_text().splitlines()]
    linhas[0]["codigo_esperado"] = "E0599"  # classe nome com código de atributo
    linhas[1]["mensagem"] = "sem codigo nenhum"
    (saida / "entradas.jsonl").write_text("\n".join(json.dumps(x) for x in linhas) + "\n")
    v = verifica(raiz, saida)
    assert any("fora da classe" in x for x in v) and any("mensagem não traz" in x for x in v)


def test_selo_tem_os_campos_obrigatorios_e_o_sha_do_pre_registro(tmp_path):
    raiz, saida = fabrica(tmp_path)
    pre = tmp_path / "PRE.md"
    pre.write_text("pre-registro\n")
    selo = ih.montar_selo(saida, pre, [pre], {"worktree_head": "abc", "worktree_tree": "def"})
    assert {"sha_tree", "arquivos", "bytes", "entradas_jsonl_sha256", "holdout_meta_sha256", "pre_registro_sha256", "gerador_sha256"} <= set(selo)
    assert selo["pre_registro_sha256"] == hashlib.sha256(b"pre-registro\n").hexdigest()
    assert selo["worktree_head"] == "abc" and selo["arquivos"] == len([p for p in saida.rglob("*") if p.is_file()])


def test_pega_base_trocada(tmp_path):
    raiz, saida = fabrica(tmp_path)
    (saida / "base" / "p00000000000.rs").write_text("fn outra_base() {}\n")
    assert any("base/ não é o original" in x for x in verifica(raiz, saida))


def test_pre_registro_com_adendos_continua_verificavel_pelo_prefixo_original(tmp_path):
    original = b"pre-registro original\n"
    pre = tmp_path / "PRE.md"
    pre.write_bytes(original)
    sha = hashlib.sha256(original).hexdigest()
    assert ih.verificar_prefixo(pre, len(original), sha)
    pre.write_bytes(original + b"\n## Adendo 1\n")
    assert ih.verificar_prefixo(pre, len(original), sha), "acrescentar adendo no fim não quebra o original"
    pre.write_bytes(b"pre-registro ALTERADO\n" + b"\n## Adendo 1\n")
    assert not ih.verificar_prefixo(pre, len(original), sha), "mudar um byte do original quebra"
    pre.write_bytes(original[:-3])
    assert not ih.verificar_prefixo(pre, len(original), sha), "arquivo menor que o original quebra"
