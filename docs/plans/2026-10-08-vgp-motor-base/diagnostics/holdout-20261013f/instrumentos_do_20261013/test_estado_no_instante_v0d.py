"""Estado no instante COMPLETO (HO13, 10/10/2026) — TDD com fixture SINTETICA (nenhum dado de held-out).

O caso real que motivou: `from scripts.eleitoral import publicar_mapa as pm` — o manifesto sobrepunha so o
`scripts/eleitoral/__init__.py`, e o motor julgou `pm._publicar` contra o `publicar_mapa.py` de HOJE (a funcao hoje se
chama outra coisa). Regra pre-registrada: o manifesto sobrepoe TODO modulo do projeto que a entrada importa — `import a.b`,
`from a import b` (b submodulo ou subpacote), relativos — e uma entrada cujo modulo nao reconstroi no instante e INCOMPLETA.

O modulo sob teste vem de `$ESTADO_MODULO` (o mutante roda sobre uma COPIA); sem a variavel, o original ao lado.
"""

import importlib.util
import os
import sys
from pathlib import Path

import pytest

SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))  # a copia mutada importa `corpus_v0d` daqui
import corpus_v0d  # noqa: E402

CAMINHO = Path(os.environ.get("ESTADO_MODULO", SCRIPTS / "estado_no_instante_v0d.py"))
_spec = importlib.util.spec_from_file_location("estado_sob_teste", CAMINHO)
eni = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(eni)

HOJE = "def _publicar_e_conferir():\n    return 'hoje'\n"
INSTANTE = "def _publicar():\n    return 'instante'\n"
ARQUIVOS = {
    "scripts/__init__.py": "",
    "scripts/eleitoral/__init__.py": "from .publicar_mapa import _publicar_e_conferir\n",
    "scripts/eleitoral/publicar_mapa.py": HOJE,
    "scripts/eleitoral/outro/__init__.py": "",
    "scripts/eleitoral/outro/folha.py": "x = 1\n",
    "scripts/eleitoral/vizinho.py": "from . import publicar_mapa\nfrom .outro import folha\n",
    "util.py": "",
    "solto/semi.py": "import irmao\n",
    "solto/irmao.py": "y = 2\n",
}


@pytest.fixture
def raiz(tmp_path: Path) -> Path:
    r = tmp_path / "proj"
    for rel, texto in ARQUIVOS.items():
        (r / rel).parent.mkdir(parents=True, exist_ok=True)
        (r / rel).write_text(texto)
    return r


def inline(raiz: Path) -> dict:
    return {"project_root": str(raiz), "path": "<stdin>", "ts": "2026-10-01T10:00:00.000Z", "id": "inl"}


def de_arquivo(raiz: Path, rel: str) -> dict:
    return {"project_root": str(raiz), "path": str(raiz / rel), "ts": "2026-10-01T10:00:00.000Z", "id": "arq"}


def rels(raiz: Path, achados) -> set[str]:
    return {str(Path(p).resolve().relative_to(raiz.resolve())) for p in achados}


def modulos(raiz: Path, e: dict, texto: str) -> set[str]:
    return rels(raiz, eni.modulos_do_projeto(e, texto))


INITS = {"scripts/__init__.py", "scripts/eleitoral/__init__.py"}


# --------------------------------------------------------------------------- a resolucao dos imports


def test_from_pacote_import_submodulo_o_caso_real(raiz):
    """`from scripts.eleitoral import publicar_mapa as pm`: o submodulo entra, nao so o __init__."""
    achados = modulos(raiz, inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\nprint(pm._publicar)\n")
    assert achados == INITS | {"scripts/eleitoral/publicar_mapa.py"}


def test_import_pontilhado_traz_os_pacotes_ancestrais(raiz):
    achados = modulos(raiz, inline(raiz), "import scripts.eleitoral.publicar_mapa\n")
    assert achados == INITS | {"scripts/eleitoral/publicar_mapa.py"}


def test_import_pontilhado_com_alias(raiz):
    achados = modulos(raiz, inline(raiz), "import scripts.eleitoral.publicar_mapa as pm\n")
    assert achados == INITS | {"scripts/eleitoral/publicar_mapa.py"}


def test_from_pacote_import_subpacote(raiz):
    achados = modulos(raiz, inline(raiz), "from scripts.eleitoral import outro\n")
    assert achados == INITS | {"scripts/eleitoral/outro/__init__.py"}


def test_from_pontilhado_import_submodulo(raiz):
    achados = modulos(raiz, inline(raiz), "from scripts.eleitoral.outro import folha\n")
    assert achados == INITS | {"scripts/eleitoral/outro/__init__.py", "scripts/eleitoral/outro/folha.py"}


def test_varios_nomes_no_mesmo_from(raiz):
    achados = modulos(raiz, inline(raiz), "from scripts.eleitoral import publicar_mapa, outro, _inexistente\n")
    assert achados == INITS | {"scripts/eleitoral/publicar_mapa.py", "scripts/eleitoral/outro/__init__.py"}


def test_nome_que_nao_e_modulo_nao_inventa_arquivo(raiz):
    """`publicar` e uma funcao, nao um arquivo: so os __init__ entram."""
    achados = modulos(raiz, inline(raiz), "from scripts.eleitoral import publicar\n")
    assert achados == INITS


def test_import_estrela_so_o_pacote(raiz):
    assert modulos(raiz, inline(raiz), "from scripts.eleitoral import *\n") == INITS


def test_relativos(raiz):
    e = de_arquivo(raiz, "scripts/eleitoral/vizinho.py")
    texto = ARQUIVOS["scripts/eleitoral/vizinho.py"]
    assert modulos(raiz, e, texto) == {
        "scripts/eleitoral/__init__.py",  # o pacote corrente roda o próprio __init__
        "scripts/eleitoral/publicar_mapa.py",
        "scripts/eleitoral/outro/__init__.py",
        "scripts/eleitoral/outro/folha.py",
    }


def test_irmao_do_script_segue_resolvendo(raiz):
    """A segunda base (o diretorio do script) — o que a regra antiga ja fazia — continua valendo."""
    e = de_arquivo(raiz, "solto/semi.py")
    assert modulos(raiz, e, ARQUIVOS["solto/semi.py"]) == {"solto/irmao.py"}


def test_externo_nao_entra(raiz):
    assert modulos(raiz, inline(raiz), "import os, json\nfrom collections import abc\n") == set()


def test_modulo_fora_da_raiz_do_projeto_nao_entra(raiz, tmp_path):
    """Um irmão do script que mora FORA da raiz do projeto (ex.: em /tmp) não é módulo do projeto."""
    fora = tmp_path / "fora"
    fora.mkdir()
    (fora / "a.py").write_text("import b\n")
    (fora / "b.py").write_text("")
    e = {"project_root": str(raiz), "path": str(fora / "a.py"), "ts": "2026-10-01T10:00:00.000Z", "id": "f"}
    assert modulos(raiz, e, "import b\n") == set()


def test_o_proprio_arquivo_nao_e_overlay_de_si(raiz):
    e = de_arquivo(raiz, "util.py")
    assert modulos(raiz, e, "import util\n") == set()


def test_a_regra_antiga_so_via_o_init_o_defeito_documentado(raiz):
    """O legado devolve SO o __init__ no caso real — e o defeito que o teste 1 fecha."""
    achados = rels(raiz, eni.modulos_do_projeto_legado(inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\n"))
    assert achados == {"scripts/eleitoral/__init__.py"}


def test_tipos_dos_modulos(raiz):
    tipos = {str(Path(p).resolve().relative_to(raiz.resolve())): t for p, t in eni.modulos_tipados(inline(raiz), "from scripts.eleitoral import publicar_mapa\n").items()}
    assert tipos == {"scripts/__init__.py": "ancestral", "scripts/eleitoral/__init__.py": "direto", "scripts/eleitoral/publicar_mapa.py": "submodulo"}


# --------------------------------------------------------------------------- completude e overlay do INSTANTE


def conteudo_de(faltando: set[str] | None = None):
    faltando = faltando or set()

    def fake(arq: str, ts: str, indice: dict) -> str | None:
        return None if Path(arq).name in faltando else (INSTANTE if Path(arq).name == "publicar_mapa.py" else "")

    return fake


def test_estado_completo_quando_tudo_reconstroi(raiz, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de())
    assert eni.estado_completo(inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\n", {}) == (True, "ok")


def test_incompleta_quando_o_submodulo_nao_reconstroi(raiz, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de({"publicar_mapa.py"}))
    ok, motivo = eni.estado_completo(inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\n", {})
    assert ok is False and motivo == "estado incompleto: submódulo importado não reconstrói no instante"


def test_incompleta_quando_o_init_nao_reconstroi(raiz, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de({"__init__.py"}))
    ok, motivo = eni.estado_completo(inline(raiz), "import scripts.eleitoral.publicar_mapa\n", {})
    assert ok is False and motivo.startswith("estado incompleto: ")


def test_sem_imports_do_projeto_e_completa(raiz, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de({"publicar_mapa.py"}))
    assert eni.estado_completo(inline(raiz), "import os\nprint(1)\n", {}) == (True, "ok")


def test_overlay_carrega_o_instante_nao_o_hoje(raiz, monkeypatch, tmp_path):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de())
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    over, faltas = eni.overlays_e_faltas(inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\n", {}, destino)
    assert faltas == []
    por_caminho = {Path(o["path"]).name: o for o in over}
    assert len(over) == 3
    arq = destino.parent / por_caminho["publicar_mapa.py"]["file"]
    assert arq.read_text() == INSTANTE and arq.read_text() != (raiz / "scripts/eleitoral/publicar_mapa.py").read_text()
    assert por_caminho["publicar_mapa.py"]["sha256"] == eni.sha(INSTANTE)


def test_overlays_e_faltas_nomeia_o_que_faltou(raiz, monkeypatch, tmp_path):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de({"publicar_mapa.py"}))
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    over, faltas = eni.overlays_e_faltas(inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\n", {}, destino)
    assert len(over) == 2 and [Path(f).name for f in faltas] == ["publicar_mapa.py"]


def test_profundidade_2_mede_o_que_o_modulo_sobreposto_importa_e_nao_esta_coberto(raiz, monkeypatch, tmp_path):
    """Medida, não exigida: `publicar_mapa.py` (do INSTANTE) importa `util` e `.outro.folha` — fora do manifesto."""
    instante = "import util\nfrom .outro import folha\nfrom . import publicar_mapa\n"
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", lambda arq, ts, indice: instante if Path(arq).name == "publicar_mapa.py" else "")
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    e = inline(raiz)
    over, _ = eni.overlays_e_faltas(e, "from scripts.eleitoral import publicar_mapa as pm\n", {}, destino)
    lacunas = {str(Path(p).relative_to(raiz.resolve())) for p in eni.importacoes_nao_cobertas(e, over, destino)}
    assert lacunas == {"util.py", "scripts/eleitoral/outro/__init__.py", "scripts/eleitoral/outro/folha.py"}


def test_profundidade_2_nao_conta_o_que_ja_esta_coberto_nem_a_propria_entrada(raiz, monkeypatch, tmp_path):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", lambda arq, ts, indice: "from . import vizinho\n")
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    e = de_arquivo(raiz, "scripts/eleitoral/vizinho.py")
    over, _ = eni.overlays_e_faltas(e, ARQUIVOS["scripts/eleitoral/vizinho.py"], {}, destino)
    # os módulos sobrepostos importam `vizinho` (a própria entrada) e irmãos já cobertos: nada a mais
    assert eni.importacoes_nao_cobertas(e, over, destino) == []


def test_overlays_legado_continua_pulando_em_silencio(raiz, monkeypatch, tmp_path):
    """`overlays()` mantem a assinatura e o comportamento (quem nao exige completude nao muda)."""
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de({"publicar_mapa.py"}))
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    assert len(eni.overlays(inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\n", {}, destino)) == 2


def test_selftest_do_modulo_continua_passando():
    assert eni.selftest() == 0
