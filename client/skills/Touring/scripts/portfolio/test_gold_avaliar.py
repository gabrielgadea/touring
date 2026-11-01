"""Gold set and evaluator contracts (Phase 0): deterministic split, no time leak, unscored is not zero."""

from __future__ import annotations

import gzip
import hashlib
import json
import sys
from pathlib import Path

import pytest

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

import avaliar
import gold

PROGRAMA = "\n".join(f"linha_{i} = calcular(valor_{i}, base) + ajuste_{i}" for i in range(12))
OUTRO = "\n".join(f"for item in lista_{i}:\n    print(item.nome, item.peso)" for i in range(6))


def _item(iid: str, sessao: str, ts: str, content: str, nome: str = "x.py") -> gold.Item:
    return gold.Item(iid, sessao, ts, nome, "Write", content, frozenset(gold.simhash.tokens(content)))


def _corpus(tmp_path: Path, linhas: list[dict]) -> tuple[Path, str]:
    dados = ("\n".join(json.dumps(x) for x in linhas) + "\n").encode()
    p = tmp_path / "c.jsonl.gz"
    p.write_bytes(gzip.compress(dados, mtime=0))
    return p, hashlib.sha256(dados).hexdigest()


def test_divisao_e_deterministica_e_disjunta() -> None:
    ids = [f"id{i}" for i in range(400)]
    primeira = {i: gold.particao(i) for i in ids}
    assert primeira == {i: gold.particao(i) for i in ids}
    lados = set(primeira.values())
    assert lados == {"treino", "held_out"}
    assert 150 < sum(v == "held_out" for v in primeira.values()) < 250  # ~50/50, not degenerate


def test_gold_recusa_sobrescrever(tmp_path: Path) -> None:
    saida = tmp_path / "g.json"
    gold.gravar({"x": 1}, saida)
    with pytest.raises(FileExistsError):
        gold.gravar({"x": 2}, saida)
    assert json.loads(saida.read_text()) == {"x": 1}


def test_manifesto_confere_o_hash(tmp_path: Path) -> None:
    linha = {"id": "a", "sessao": "s", "timestamp": "1", "nome": "a.py", "origem": "Write", "content": "x = 1"}
    p, sha = _corpus(tmp_path, [linha])
    assert len(gold.carregar_corpus(p, sha)) == 1
    with pytest.raises(gold.CorpusAdulterado):
        gold.carregar_corpus(p, "0" * 64)


def test_biblioteca_so_tem_o_que_existia_antes_e_em_outra_sessao() -> None:
    q = _item("q", "s1", "2026-10-02", PROGRAMA)
    itens = [
        q,
        _item("antes", "s2", "2026-10-01", PROGRAMA),
        _item("depois", "s2", "2026-10-03", PROGRAMA),
        _item("mesma", "s1", "2026-10-01", PROGRAMA),
    ]
    assert [i.id for i in gold.biblioteca_de(q, itens)] == ["antes"]


def test_rotulos_pelas_faixas_declaradas() -> None:
    itens = [
        _item("base", "s1", "1", PROGRAMA),
        _item("copia", "s2", "2", PROGRAMA),
        _item("diferente", "s3", "3", OUTRO),
    ]
    r = gold.rotular(itens)
    assert r["base"].classe == "sem_biblioteca"
    assert r["copia"].classe == "tem_equivalente"
    assert r["copia"].equivalentes == ("base",)
    assert r["diferente"].classe == "sem_equivalente"


def test_mutacoes_sao_deterministicas_e_preservam_o_programa() -> None:
    a, b = gold.mutacoes(PROGRAMA), gold.mutacoes(PROGRAMA)
    assert a == b
    assert len(a) == 3
    assert all(m != PROGRAMA for m in a[1:])


def _gold_doc(itens: list[gold.Item]) -> dict:
    return {
        "rotulos": {
            i: {"classe": r.classe, "equivalentes": list(r.equivalentes), "metodo": r.metodo, "particao": "held_out"}
            for i, r in gold.rotular(itens).items()
        }
    }


def test_avaliador_conta_falha_como_unscored() -> None:
    itens = [_item("base", "s1", "1", PROGRAMA), _item("copia", "s2", "2", PROGRAMA)]

    def quebra(_q: gold.Item, _b: list[gold.Item]) -> avaliar.Veredito:
        raise RuntimeError("fonte fora do ar")

    rel = avaliar.avaliar(quebra, itens, _gold_doc(itens))
    assert rel["geral"]["unscored"] == 1
    assert rel["geral"]["precisao"] is None  # nothing scored is not a precision of zero


def test_inaferivel_e_unscored_nunca_aprovado() -> None:
    itens = [_item("base", "s1", "1", PROGRAMA), _item("copia", "s2", "2", PROGRAMA)]
    rel = avaliar.avaliar(lambda _q, _b: avaliar.Veredito("inaferivel"), itens, _gold_doc(itens))
    assert rel["geral"]["unscored"] == 1
    assert rel["geral"].get("fn", 0) == 0


def test_simhash_acerta_copia_e_nao_alarma_programa_distinto() -> None:
    itens = [
        _item("base", "s1", "1", PROGRAMA),
        _item("copia", "s2", "2", PROGRAMA),
        _item("diferente", "s3", "3", OUTRO),
    ]
    rel = avaliar.avaliar(avaliar.recuperador_simhash(), itens, _gold_doc(itens))
    assert rel["geral"]["tp"] == 1
    assert rel["geral"].get("fp_alarme", 0) == 0
    assert rel["geral"]["precisao"] == 1.0


def test_veredito_rejeita_estado_fora_do_contrato() -> None:
    with pytest.raises(ValueError, match="fora de"):
        avaliar.Veredito("talvez")
