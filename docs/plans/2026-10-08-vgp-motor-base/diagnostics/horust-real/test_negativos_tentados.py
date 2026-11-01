"""HORUST-REAL — §5: o negativo é disjunto de TODO arquivo tentado (rodadas 1, 2 e revalidação), não só dos que viraram positivo."""

import os
import sys
from pathlib import Path

sys.path.insert(0, os.environ.get("HORUST_SRC_DIR") or str(Path(__file__).resolve().parent))
import construtor_horust as ch  # noqa: E402
import verificar_negativos_tentados as vn  # noqa: E402


def corpus(tmp_path):
    raiz = tmp_path / "wt"
    neg = [ch.Negativo(f"n{i}", f"crates/c{i}/src/n{i}.rs", f"c{i}", f"fn n{i}() {{}}\n") for i in range(3)]
    pos = [ch.Positivo("p0", "crates/c9/src/p0.rs", "c9", "nome", "nome_valor", "E0425", 1, 1, "x", "y", "near_miss", "m", "fn y() {}\n", "fn x() {}\n")]
    for a in [*neg, *pos]:
        (raiz / a.rel).parent.mkdir(parents=True, exist_ok=True)
        (raiz / a.rel).write_text("x")
    ch.escrever_corpus(tmp_path / "c", raiz, pos, neg, {})
    return raiz, tmp_path / "c"


def test_negativos_disjuntos_dos_tentados_nao_tem_violacao(tmp_path):
    raiz, c = corpus(tmp_path)
    assert vn.negativos_tentados(c, raiz, {"crates/c9/src/p0.rs", "crates/zz/src/outro.rs"}) == []


def test_negativo_tentado_em_alguma_rodada_e_violacao_e_positivo_tentado_nao_e(tmp_path):
    raiz, c = corpus(tmp_path)
    v = vn.negativos_tentados(c, raiz, {"crates/c1/src/n1.rs", "crates/c9/src/p0.rs"})
    assert len(v) == 1 and "n1" in v[0] and "tentado" in v[0]


def test_resumo_conta_negativos_e_tentados(tmp_path):
    raiz, c = corpus(tmp_path)
    r = vn.resumo(c, raiz, {"crates/c1/src/n1.rs", "crates/c9/src/p0.rs"})
    assert (r["negativos"], r["arquivos_tentados"], r["violacoes"]) == (3, 2, 1)
