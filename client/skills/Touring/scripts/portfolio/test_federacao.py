"""Federation contract (P2): project wins by name, no copy, a broken index is inaferivel, cwd walk-up."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

import federacao
import recuperar
import simhash

PROGRAMA = "\n".join(f"linha_{i} = calcular(valor_{i}, base) + ajuste_{i}" for i in range(12))


def _indice(raiz: Path, entradas: list[dict], versao: int = federacao.VERSAO) -> Path:
    p = raiz / federacao.INDICE_REL
    p.parent.mkdir(parents=True)
    p.write_text(json.dumps({"versao": versao, "entradas": entradas}))
    return p


def _entrada(caminho: str, conteudo: str = PROGRAMA, proposito: str = "calcula o ajuste") -> dict:
    return {
        "caminho": caminho,
        "proposito": proposito,
        "familia": "processo",
        "simhash": simhash.hex64(simhash.simhash64(conteudo, distintos=True)),
        "linhas": recuperar.linhas_substantivas(conteudo),
        "termos": sorted(recuperar.termos_de(caminho, proposito)),
    }


def test_projeto_vence_por_nome() -> None:
    projeto = [recuperar.Candidato(id="projeto:a/probe.py", nome="probe.py", origem="projeto")]
    globais = [
        recuperar.Candidato(id="global:probe.py", nome="probe.py", origem="global"),
        recuperar.Candidato(id="global:dump.py", nome="dump.py", origem="global"),
    ]
    assert [c.id for c in federacao.federar(projeto, globais)] == ["projeto:a/probe.py", "global:dump.py"]


def test_homonimos_na_mesma_fonte_ficam() -> None:
    projeto = [
        recuperar.Candidato(id="projeto:a/probe.py", nome="probe.py"),
        recuperar.Candidato(id="projeto:b/probe.py", nome="probe.py"),
    ]
    assert len(federacao.federar(projeto)) == 2


def test_cwd_aninhado_acha_o_indice_do_projeto(tmp_path: Path) -> None:
    indice = _indice(tmp_path, [_entrada("scripts/x.py")])
    fundo = tmp_path / "docs" / "plans" / "bundle"
    fundo.mkdir(parents=True)
    assert federacao.achar_indice(str(fundo)) == indice
    assert federacao.achar_indice(None) is None


def test_indice_ausente_nao_e_fonte(tmp_path: Path) -> None:
    assert federacao.achar_indice(str(tmp_path)) is None


@pytest.mark.parametrize("conteudo", ["{nao json", json.dumps({"versao": 99, "entradas": []}), json.dumps([])])
def test_indice_invalido_levanta_em_vez_de_virar_vazio(tmp_path: Path, conteudo: str) -> None:
    p = tmp_path / federacao.INDICE_REL
    p.parent.mkdir(parents=True)
    p.write_text(conteudo)
    with pytest.raises(federacao.IndiceInvalido):
        federacao.fonte_projeto(p)


def test_candidato_pre_computado_decide_sem_ler_o_arquivo(tmp_path: Path) -> None:
    # The project file is NOT on disk: the precomputed simhash alone must carry the body signal.
    indice = _indice(tmp_path, [_entrada("scripts/ajuste.py")])
    (cand,) = federacao.fonte_projeto(indice)
    assert cand.conteudo == ""
    assert cand.caminho == str(tmp_path / "scripts/ajuste.py")
    v = recuperar.decidir("novo.py", PROGRAMA.replace("ajuste_3", "correcao_3"), [cand])
    assert v.estado == "reprovado"
    assert v.candidatos[0][0] == "projeto:scripts/ajuste.py"


def test_consultar_usa_o_indice_do_projeto_e_nomeia_indice_quebrado(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setattr(recuperar, "fonte_tantivy", lambda *_a, **_k: [])
    monkeypatch.setattr(recuperar, "fonte_manifest", lambda *_a, **_k: [])
    _indice(tmp_path, [_entrada("scripts/ajuste.py")])
    v = recuperar.consultar("/tmp/x/novo.py", PROGRAMA, str(tmp_path))
    assert v.estado == "reprovado"
    (tmp_path / federacao.INDICE_REL).write_text("{quebrado")
    v = recuperar.consultar("/tmp/x/outro.py", "x = 1\n", str(tmp_path))
    assert v.estado == "inaferivel"
    assert any(f.startswith("projeto") for f in v.fontes_falhas)
