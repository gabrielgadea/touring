"""Federation contract (P2): project wins by name, no copy, a broken index is inaferivel, cwd walk-up."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

import federacao
import recuperar
import simhash

PROGRAMA = "\n".join(f"linha_{i} = calcular(valor_{i}, base) + ajuste_{i}" for i in range(12))
OUTRO_PROGRAMA = "\n".join(f"for item in lista_{i}:\n    print(item.nome, item.peso, item.custo)" for i in range(8))


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


# ---- T3 (04/11/2026): the tantivy subprocess is skipped when the federated sources already decided ----------


@pytest.fixture
def espia_subprocesso(monkeypatch: pytest.MonkeyPatch):
    """Replace ``subprocess.run`` as ``recuperar`` sees it: records every argv, answers with canned tantivy rows.

    Spying at the process boundary (not at ``fonte_tantivy``) proves no ``touring tantivy search`` is SPAWNED.
    """
    chamadas: list[list[str]] = []
    resposta = {"rows": []}

    def falso(argv, **_kw):
        chamadas.append(list(argv))
        return subprocess.CompletedProcess(argv, 0, stdout=json.dumps(resposta["rows"]), stderr="")

    monkeypatch.setattr(recuperar.subprocess, "run", falso)
    monkeypatch.setattr(recuperar, "fonte_manifest", lambda *_a, **_k: [])
    # No ambient daemon: since T3 a live socket (the session exports TOURING_DAEMON_SOCKET) answers first and
    # the subprocess this spy watches is never spawned.
    _sem_daemon(monkeypatch)
    return chamadas, resposta


def _sem_daemon(monkeypatch: pytest.MonkeyPatch) -> None:
    """Hide every route to a running daemon, so the test decides the tantivy answer."""
    for var in ("TOURING_DAEMON_SOCKET", "TOURING_DAEMON_SOCK", "CLAUDE_PROJECT_DIR"):
        monkeypatch.delenv(var, raising=False)
    monkeypatch.setattr(recuperar, "_socket_do_daemon", lambda *_a, **_k: None)


def test_indice_sem_equivalente_ainda_consulta_o_tantivy_e_ele_pode_reprovar(tmp_path: Path, espia_subprocesso) -> None:
    # No loss of recall: the index has nothing equivalent, so the verdict is still open and tantivy must run.
    chamadas, resposta = espia_subprocesso
    _indice(tmp_path, [_entrada("scripts/outro.py", conteudo=OUTRO_PROGRAMA)])
    achado = tmp_path / "fora_do_indice" / "recalcula.py"
    achado.parent.mkdir()
    achado.write_text(PROGRAMA)
    resposta["rows"] = [{"file_path": str(achado), "symbol_name": "recalcula"}]
    v = recuperar.consultar("/tmp/x/novo.py", PROGRAMA.replace("ajuste_3", "correcao_3"), str(tmp_path))
    assert len(chamadas) == 1
    assert chamadas[0][:3] == ["touring", "tantivy", "search"]
    assert v.estado == "reprovado"
    assert v.candidatos[0][0] == f"tantivy:{achado}"


def test_sem_indice_nenhum_o_tantivy_continua_sendo_a_fonte(tmp_path: Path, espia_subprocesso) -> None:
    chamadas, _ = espia_subprocesso
    v = recuperar.consultar("/tmp/x/novo.py", PROGRAMA, str(tmp_path))  # no .claude/portfolio/indice.json above
    assert len(chamadas) == 1
    assert v.estado == "aprovado"


def test_tantivy_que_falha_com_indice_indeciso_continua_inaferivel(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(recuperar, "fonte_manifest", lambda *_a, **_k: [])

    def fora_do_ar(*_a, **_k):
        raise subprocess.TimeoutExpired(cmd="touring", timeout=2.5)

    # `_coletar` starts the lookup with `iniciar_tantivy` and collects it later (A7), so the failure is
    # injected there: patching `fonte_tantivy` intercepted nothing and the live daemon answered instead.
    _sem_daemon(monkeypatch)
    monkeypatch.setattr(recuperar, "iniciar_tantivy", lambda *_a, **_k: fora_do_ar)
    _indice(tmp_path, [_entrada("scripts/outro.py", conteudo=OUTRO_PROGRAMA)])
    v = recuperar.consultar("/tmp/x/novo.py", PROGRAMA, str(tmp_path))
    assert v.estado == "inaferivel"
    assert any("tantivy" in f for f in v.fontes_falhas)


def test_cache_marshal_devolve_o_mesmo_que_o_json_sem_reler_o_json(tmp_path: Path, monkeypatch) -> None:
    p = _indice(tmp_path, [_entrada("scripts/a.py"), _entrada("scripts/b.py", proposito="outro")])
    via_json = federacao.fonte_projeto(p)
    assert federacao._cache_de(p).is_file()  # controle: a 1a chamada gravou o cache

    def proibido(_indice: Path) -> list:
        raise AssertionError("o JSON foi relido apesar do cache valido")

    monkeypatch.setattr(federacao, "_fonte_projeto_json", proibido)
    assert federacao.fonte_projeto(p) == via_json


def test_cache_invalida_quando_o_indice_muda(tmp_path: Path) -> None:
    p = _indice(tmp_path, [_entrada("scripts/a.py")])
    assert [r.rel for r in federacao.fonte_projeto(p)] == ["scripts/a.py"]
    entradas = [_entrada("scripts/a.py"), _entrada("scripts/novo.py")]
    p.write_text(json.dumps({"versao": federacao.VERSAO, "entradas": entradas}))
    assert [r.rel for r in federacao.fonte_projeto(p)] == ["scripts/a.py", "scripts/novo.py"]


@pytest.mark.parametrize("lixo", [b"", b"nao e marshal", b"\x00\x01\x02"])
def test_cache_corrompido_cai_no_json(tmp_path: Path, lixo: bytes) -> None:
    p = _indice(tmp_path, [_entrada("scripts/a.py")])
    federacao._cache_de(p).write_bytes(lixo)
    assert [r.rel for r in federacao.fonte_projeto(p)] == ["scripts/a.py"]


def test_indice_invalido_levanta_mesmo_com_cache_antigo(tmp_path: Path) -> None:
    p = _indice(tmp_path, [_entrada("scripts/a.py")])
    federacao.fonte_projeto(p)
    p.write_text("{quebrado")
    with pytest.raises(federacao.IndiceInvalido):
        federacao.fonte_projeto(p)
